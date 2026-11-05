use spacetimedb::{
    AnonymousViewContext, CtxDbRead, ProcedureContext, Query, ReducerContext, SpacetimeType, Table,
    procedure, reducer, table, view,
};

use crate::{
    authorization::Authorization,
    auto_inc_manual::AutoIncWrite,
    competition::{
        CompetitionPermissionsV1, CompetitionRead,
        connection::internal_graph_resolution_node_finished,
        node::{NodeHandle, NodeRead, NodeWrite, Vec2, resolve_input_data},
    },
    leaderboard::{
        context::LbContext,
        error::LbError,
        filter::{LbFilterSettings, LbFilterSettingsV2},
        merge::{LbMergeSettings, LbMergeSettingsV2, LbMergeTeams},
        modes::{match_standings, match_team_standings},
        remap::LbRemapSettings,
        script::{LbScriptSettings, script_permission},
        select::LbSelectSettings,
        shape::{LbShape, check_steps},
    },
};

mod combinators;
pub(crate) mod context;
pub(crate) mod error;
mod filter;
mod merge;
pub(crate) mod modes;
mod remap;
pub(crate) mod script;
pub(crate) mod select;
pub(crate) mod shape;
#[cfg(test)]
pub(crate) mod test_util;

#[table(accessor= tab_leaderboard, vis_private)]
struct LeaderboardV1 {
    name: String,
    settings: Vec<LbSettings>,

    #[auto_inc]
    #[primary_key]
    id: u32,

    #[index(hash)]
    parent_id: u32,

    template: bool,

    status: LeaderboardStatus,
}

/// Superseded by [`LeaderboardV3`], only kept until `migration_leaderboard_to_v3` moved the rows.
#[table(accessor= tab_leaderboard_v2,vis_private)]
pub struct LeaderboardV2 {
    name: String,
    settings: Vec<LbSettingsV2>,

    #[primary_key]
    pub id: u32,

    #[index(hash)]
    parent_id: u32,

    template: bool,

    status: LeaderboardStatus,
}

#[table(accessor= tab_leaderboard_v3,vis_private)]
pub struct LeaderboardV3 {
    name: String,
    settings: Vec<LbSettingsV3>,

    #[primary_key]
    pub id: u32,

    #[index(hash)]
    parent_id: u32,

    template: bool,

    status: LeaderboardStatus,
}

impl LeaderboardV3 {
    pub(crate) fn is_template(&self) -> bool {
        self.template
    }

    pub(crate) fn get_comp_id(&self) -> u32 {
        self.parent_id
    }

    pub(crate) fn is_configured(&self) -> bool {
        self.status == LeaderboardStatus::Configured
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }
}

impl From<LeaderboardV2> for LeaderboardV3 {
    fn from(leaderboard: LeaderboardV2) -> Self {
        LeaderboardV3 {
            name: leaderboard.name,
            settings: leaderboard
                .settings
                .into_iter()
                .flat_map(LbSettingsV2::upgrade)
                .collect(),
            id: leaderboard.id,
            parent_id: leaderboard.parent_id,
            template: leaderboard.template,
            status: leaderboard.status,
        }
    }
}

/// Moves the leaderboards over to the v3 table, it is safe to call this multiple times.
#[reducer]
fn migration_leaderboard_to_v3(ctx: &ReducerContext) -> Result<(), String> {
    let rows: Vec<LeaderboardV2> = ctx.db.tab_leaderboard_v2().iter().collect();
    for row in rows {
        let id = row.id;
        if ctx.db.tab_leaderboard_v3().id().find(id).is_none() {
            ctx.db.tab_leaderboard_v3().try_insert(row.into())?;
        }
        ctx.db.tab_leaderboard_v2().id().delete(id);
    }

    Ok(())
}

#[derive(Debug, SpacetimeType, Clone, Copy, PartialEq, Eq)]
enum LeaderboardStatus {
    Configuring,
    Configured,
    //Ongoing,
    //Ended,
}

#[derive(Debug, SpacetimeType, Clone, Copy)]
enum LbSettings {
    Merge(LbMergeSettings),
    Filter(LbFilterSettings),
}

/// Superseded by [`LbSettingsV3`].
#[derive(Debug, SpacetimeType, Clone, Copy)]
enum LbSettingsV2 {
    Merge(LbMergeSettings),
    Filter(LbFilterSettings),
    Remap(LbRemapSettings),
    Finalize,
}

impl LbSettingsV2 {
    fn upgrade(self) -> Vec<LbSettingsV3> {
        match self {
            LbSettingsV2::Merge(settings) => vec![LbSettingsV3::Merge(settings.into())],
            LbSettingsV2::Filter(settings) => settings.upgrade(),
            LbSettingsV2::Remap(settings) => vec![LbSettingsV3::Remap(settings)],
            LbSettingsV2::Finalize => vec![LbSettingsV3::Match],
        }
    }
}

/// A step of a leaderboard pipeline, used by leaderboard nodes and data connections.
///
/// Steps get evaluated again whenever their leaderboard is read, also for competitions which are long over.
/// So what a variant does must never change, a change is a new variant (like `SelectV2`) next to the old one.
#[derive(Debug, SpacetimeType, Clone)]
pub(crate) enum LbSettingsV3 {
    /// Turns the rounds of every match into the standings of that match.
    Match,
    Merge(LbMergeSettingsV2),
    Filter(LbFilterSettingsV2),
    Remap(LbRemapSettings),
    Select(LbSelectSettings),
    /// Turns the rounds of every match into the standings of its teams.
    MatchTeams,
    /// Merges all entries of every team into one standing of the team, entries without a team are left out.
    MergeTeams(LbMergeSettingsV2),
    Script(LbScriptSettings),
}

/// Takes the entries of a leaderboard and returns the changed ones.
pub(crate) trait LbStep {
    fn apply(&self, cx: &LbContext, entries: Vec<LbEntry>) -> Result<Vec<LbEntry>, LbError>;
    /// Whether the step works on the entries it would get, returns what it would produce.
    /// Only steps which certainly fail or produce something meaningless are rejected.
    fn check(&self, origin: NodeHandle, input: LbShape) -> Result<LbShape, LbError>;
}

impl LbStep for LbSettingsV3 {
    fn apply(&self, cx: &LbContext, entries: Vec<LbEntry>) -> Result<Vec<LbEntry>, LbError> {
        match self {
            LbSettingsV3::Match => match_standings(cx, entries),
            LbSettingsV3::Merge(settings) => settings.apply(cx, entries),
            LbSettingsV3::Filter(settings) => settings.apply(cx, entries),
            LbSettingsV3::Remap(settings) => settings.apply(cx, entries),
            LbSettingsV3::Select(settings) => settings.apply(cx, entries),
            LbSettingsV3::MatchTeams => match_team_standings(cx, entries),
            LbSettingsV3::MergeTeams(settings) => LbMergeTeams(settings).apply(cx, entries),
            LbSettingsV3::Script(settings) => settings.apply(cx, entries),
        }
    }

    fn check(&self, origin: NodeHandle, input: LbShape) -> Result<LbShape, LbError> {
        match self {
            // Standings stay as they are, so this is never wrong.
            LbSettingsV3::Match => Ok(LbShape {
                rounds: false,
                standings: !input.is_empty(),
                ..input
            }),
            LbSettingsV3::Merge(settings) => settings.check(origin, input),
            LbSettingsV3::Filter(settings) => settings.check(origin, input),
            LbSettingsV3::Remap(settings) => settings.check(origin, input),
            LbSettingsV3::Select(settings) => settings.check(origin, input),
            // Whether the mode can rank teams is only known when the match gets evaluated.
            LbSettingsV3::MatchTeams => Ok(LbShape {
                rounds: false,
                standings: !input.is_empty(),
                players: false,
                teams: !input.is_empty(),
                ..input
            }),
            LbSettingsV3::MergeTeams(settings) => LbMergeTeams(settings).check(origin, input),
            LbSettingsV3::Script(settings) => settings.check(origin, input),
        }
    }
}

/// Runs the steps one after another, entries created by a step belong to the origin of `cx`.
pub(crate) fn evaluate_steps(
    cx: &LbContext,
    steps: &[LbSettingsV3],
    entries: Vec<LbEntry>,
) -> Result<Vec<LbEntry>, LbError> {
    steps
        .iter()
        .enumerate()
        .try_fold(entries, |entries, (index, step)| {
            step.apply(cx, entries)
                .map_err(|error| error.at_step(cx.origin(), index))
        })
}

#[derive(Debug, SpacetimeType, Clone, Copy)]
enum LbParams {
    Score,
    Time,
    Position,
}

impl LbParams {
    /// Sorting by the key puts the best entries first.
    fn key(self, entry: &LbEntry) -> i64 {
        match self {
            LbParams::Score => -(entry.score as i64),
            LbParams::Time => LbParams::time_key(entry.time) as i64,
            LbParams::Position => entry.position as i64,
        }
    }

    /// A time of zero or less was not driven so it is the worst.
    fn time_key(time: i32) -> i32 {
        if time <= 0 { i32::MAX } else { time }
    }
}

/// What the entries are grouped by so a step is applied to every group on its own.
#[derive(Debug, SpacetimeType, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LbKey {
    /// The node the entries come from, e.g. every match.
    Origin,
    Map,
    Round,
    Team,
}

/// The values of the keys in a scope, keys which are not part of it stay empty.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
struct LbScopeKey {
    origin: Option<NodeHandle>,
    map_id: Option<u32>,
    round: Option<u16>,
    team_id: Option<u32>,
}

impl LbScopeKey {
    fn of(scope: &[LbKey], entry: &LbEntry) -> Self {
        let mut key = LbScopeKey::default();
        for scope_key in scope {
            match scope_key {
                LbKey::Origin => key.origin = Some(entry.get_node()),
                LbKey::Map => key.map_id = Some(entry.map_id),
                LbKey::Round => key.round = Some(entry.round),
                LbKey::Team => key.team_id = Some(entry.team_id),
            }
        }
        key
    }
}

/// The map entries were played on together, 0 if they were played on different ones.
fn shared_map(a: u32, b: u32) -> u32 {
    if a == b { a } else { 0 }
}

/// The team entries were played for together, 0 if they were played for different ones.
fn shared_team(a: u32, b: u32) -> u32 {
    if a == b { a } else { 0 }
}

/// Whom an entry ranks, a leaderboard has at most one standing per participant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum LbParticipant {
    Player(u32),
    Team(u32),
}

/// Whether an entry is the result of a single round or the standing in a whole leaderboard.
#[derive(Debug, SpacetimeType, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LbEntryKind {
    /// Comes from a match, its mode decides how the rounds become standings.
    Round,
    Standing,
}

#[derive(Debug, SpacetimeType, Clone, Copy)]
pub struct LbEntry {
    /// The user the entry belongs to, 0 if there is none.
    /// Having this as 0 means team_id cannot be 0.
    pub user_id: u32,
    /// The team the entry belongs to, 0 if there is none.
    /// Without a user the entry is the one of the team.
    /// Having this as 0 user_id cannot be 0.
    pub team_id: u32,
    // Required
    pub position: u16,
    // Default: 0
    pub round: u16,
    // Default: 0, which is an unknown map or several ones.
    pub map_id: u32,
    // Default: i32::MIN
    pub score: i32,
    // Default: i32::MIN
    pub time: i32,
    // Required
    node_id: u32,
    // Required
    node_variant: u8,
    // Required
    pub kind: LbEntryKind,
}

impl LbEntry {
    pub(crate) fn new(
        user_id: u32,
        kind: LbEntryKind,
        position: u16, /* , origin_idx: u16 */
        origin: NodeHandle,
    ) -> Self {
        let (node_variant, node_id) = origin.split();
        LbEntry {
            user_id,
            position,
            kind,
            round: 0,
            map_id: 0,
            score: i32::MIN,
            time: i32::MIN,
            node_id,
            node_variant,
            team_id: 0, //TODO
        }
    }

    pub(crate) fn set_score(mut self, score: i32) -> Self {
        self.score = score;
        self
    }

    pub(crate) fn set_time(mut self, time: i32) -> Self {
        self.time = time;
        self
    }

    /// An entry of the participant without any results.
    pub(crate) fn for_participant(
        participant: LbParticipant,
        kind: LbEntryKind,
        position: u16,
        origin: NodeHandle,
    ) -> Self {
        match participant {
            LbParticipant::Player(user_id) => LbEntry::new(user_id, kind, position, origin),
            LbParticipant::Team(team_id) => {
                let mut entry = LbEntry::new(0, kind, position, origin);
                entry.team_id = team_id;
                entry
            }
        }
    }

    pub(crate) fn set_round(mut self, round: u16) -> Self {
        self.round = round;
        self
    }

    pub(crate) fn set_map(mut self, map_id: u32) -> Self {
        self.map_id = map_id;
        self
    }

    pub(crate) fn participant(&self) -> LbParticipant {
        if self.user_id != 0 {
            // The team is left out so a player who played for several teams still has one standing.
            LbParticipant::Player(self.user_id)
        } else {
            LbParticipant::Team(self.team_id)
        }
    }

    pub(crate) fn get_node(&self) -> NodeHandle {
        NodeHandle::combine(self.node_variant, self.node_id)
    }
}

#[reducer]
fn leaderboard_create(
    ctx: &ReducerContext,
    name: String,
    parent_id: u32,
    position: Vec2,
    with_template: u32,
    as_template: bool,
) -> Result<(), String> {
    let parent_competition = ctx.competition_find(parent_id)?;
    ctx.auth_builder(parent_id)
        .permission(CompetitionPermissionsV1::LEADERBOARD_CREATE)
        .authorize()?;

    if parent_competition.is_template() {
        return Err(
            "Cannot add a normal server to a template. Try do add a template server to id.".into(),
        );
    }

    //TODO validation.

    if with_template != 0 {
        todo!()
    } else {
        LeaderboardWrite::leaderboard_create(ctx, name, parent_id, position, None, as_template)?;
    }

    Ok(())
}

#[reducer]
fn leaderboard_configured(ctx: &ReducerContext, id: u32) -> Result<(), String> {
    let Some(mut leaderboard) = ctx.db.tab_leaderboard_v3().id().find(id) else {
        return Err("Leaderboard was mot found!".into());
    };

    ctx.auth_builder(leaderboard.parent_id)
        .permission(CompetitionPermissionsV1::LEADERBOARD_CONFIGURED)
        .authorize()?;

    if leaderboard.status != LeaderboardStatus::Configuring {
        return Err("Leaderboard is not in configuring state".into());
    }

    // The inputs could have changed since the steps were saved.
    let node = NodeHandle::LeaderboardV1(id);
    check_steps(node, &leaderboard.settings, ctx.node_input_shape(node)?)?;

    leaderboard.status = LeaderboardStatus::Configured;

    ctx.db.tab_leaderboard_v3().id().update(leaderboard);

    // Everything before it could have finished while it got configured.
    if ctx.node_pending_origins(node).is_empty() {
        internal_graph_resolution_node_finished(ctx, node)?;
    }

    Ok(())
}

/// Allows to change the leaderboard again as long as nothing relies on it yet.
#[reducer]
fn leaderboard_unconfigure(ctx: &ReducerContext, id: u32) -> Result<(), String> {
    let Some(mut leaderboard) = ctx.db.tab_leaderboard_v3().id().find(id) else {
        return Err("Leaderboard was not found!".into());
    };

    ctx.auth_builder(leaderboard.parent_id)
        .permission(CompetitionPermissionsV1::LEADERBOARD_CONFIGURED)
        .authorize()?;

    if leaderboard.status != LeaderboardStatus::Configured {
        return Err("Leaderboard is not in configured state".into());
    }

    let dependents = ctx.node_dependents(NodeHandle::LeaderboardV1(id));
    if !dependents.is_empty() {
        return Err(format!(
            "The leaderboard cannot be changed anymore because {dependents:?} already rely on it."
        ));
    }

    leaderboard.status = LeaderboardStatus::Configuring;
    ctx.db.tab_leaderboard_v3().id().update(leaderboard);

    Ok(())
}

#[reducer]
fn leaderboard_settings_update(
    ctx: &ReducerContext,
    id: u32,
    settings: Vec<LbSettingsV3>,
) -> Result<(), String> {
    let Some(mut leaderboard) = ctx.db.tab_leaderboard_v3().id().find(id) else {
        return Err("Leaderboard not found.".into());
    };

    ctx.auth_builder(leaderboard.parent_id)
        .permission(CompetitionPermissionsV1::LEADERBOARD_CONFIGURE)
        .permission(script_permission(&leaderboard.settings, &settings))
        .authorize()?;

    if leaderboard.status != LeaderboardStatus::Configuring {
        return Err("The leaderboard can only be changed while it gets configured.".into());
    }

    let node = NodeHandle::LeaderboardV1(id);
    check_steps(node, &settings, ctx.node_input_shape(node)?)?;

    leaderboard.settings = settings;

    ctx.db.tab_leaderboard_v3().id().update(leaderboard);

    Ok(())
}

pub(crate) trait LeadearboardRead {
    fn leaderboard_find(&self, id: u32) -> Result<LeaderboardV3, String>;
    /// What the leaderboard would output, its steps are checked against its inputs on the way.
    fn leaderboard_output_shape(&self, leaderboard_id: u32) -> Result<LbShape, LbError>;
    fn leaderboards_with_competition(
        &self,
        competition_id: u32,
    ) -> impl Iterator<Item = LeaderboardV3>;
    //fn leaderboard_finalize(&self, lb: Vec<LbEntry>) -> Vec<LbEntry>;
}
/// The entries the leaderboard outputs, evaluated during the call of `cx`.
pub(crate) fn leaderboard_evaluate(
    cx: &LbContext,
    leaderboard_id: u32,
) -> Result<Vec<LbEntry>, LbError> {
    let Some(lb) = cx
        .db_read_only()
        .tab_leaderboard_v3()
        .id()
        .find(leaderboard_id)
    else {
        return Err(LbError::new(format!(
            "Leaderboard {leaderboard_id} does not exist."
        )));
    };

    if lb.status == LeaderboardStatus::Configuring {
        log::warn!("Tried to eval leadarboard but it is still getting configured.");
        return Ok(Vec::new());
    }

    let node = NodeHandle::LeaderboardV1(leaderboard_id);
    let cx = cx.at(node);
    let input = resolve_input_data(&cx, node)?;

    evaluate_steps(&cx, &lb.settings, input)
}

impl<Db: spacetimedb::CtxDbRead> LeadearboardRead for Db {
    fn leaderboard_output_shape(&self, leaderboard_id: u32) -> Result<LbShape, LbError> {
        let Some(lb) = self
            .db_read_only()
            .tab_leaderboard_v3()
            .id()
            .find(leaderboard_id)
        else {
            return Err(LbError::new(format!(
                "Leaderboard {leaderboard_id} does not exist."
            )));
        };

        // Like its evaluation it outputs nothing while it gets configured.
        if lb.status == LeaderboardStatus::Configuring {
            return Ok(LbShape::default());
        }

        let node = NodeHandle::LeaderboardV1(leaderboard_id);
        check_steps(node, &lb.settings, self.node_input_shape(node)?)
    }

    fn leaderboard_find(&self, id: u32) -> Result<LeaderboardV3, String> {
        let Some(leaderboard) = self.db_read_only().tab_leaderboard_v3().id().find(id) else {
            return Err("Leaderboard not found!".into());
        };
        Ok(leaderboard)
    }

    fn leaderboards_with_competition(
        &self,
        competition_id: u32,
    ) -> impl Iterator<Item = LeaderboardV3> {
        self.db_read_only()
            .tab_leaderboard_v3()
            .parent_id()
            .filter(competition_id)
    }
}
pub(crate) trait LeaderboardWrite: LeadearboardRead {
    fn leaderboard_create(
        &self,
        name: String,
        parent_id: u32,
        position: Vec2,
        template: Option<LeaderboardV3>,
        as_template: bool,
    ) -> Result<LeaderboardV3, String>;
    fn leaderboard_name_edit(&self, leaderboard_i32: u32, name: String) -> Result<(), String>;
}
impl<Db: spacetimedb::CtxDbWrite> LeaderboardWrite for Db {
    fn leaderboard_name_edit(&self, leaderboard_id: u32, name: String) -> Result<(), String> {
        let Some(mut tm_match) = self.db().tab_leaderboard_v3().id().find(leaderboard_id) else {
            return Err("Match not found.".into());
        };
        tm_match.name = name;
        self.db().tab_leaderboard_v3().id().update(tm_match);

        Ok(())
    }

    fn leaderboard_create(
        &self,
        name: String,
        parent_id: u32,
        position: Vec2,
        template: Option<LeaderboardV3>,
        as_template: bool,
    ) -> Result<LeaderboardV3, String> {
        let leaderboard = if let Some(mut template) = template {
            template.template = as_template; //TODO: Audit codepath for legal again.
            template.parent_id = parent_id;
            template.id = self.auto_inc::<tab_leaderboard_v3__TableHandle>();
            template
        } else {
            LeaderboardV3 {
                name,
                id: self.auto_inc::<tab_leaderboard_v3__TableHandle>(),
                parent_id,
                template: false,
                status: LeaderboardStatus::Configuring,
                settings: Vec::new(),
            }
        };

        let leaderboard = self.db().tab_leaderboard_v3().try_insert(leaderboard)?;
        self.node_create(NodeHandle::LeaderboardV1(leaderboard.id), position)?;

        Ok(leaderboard)
    }
}

#[view(accessor = competition_leaderboards,public)]
fn competition_leaderboards(
    ctx: &AnonymousViewContext,
    competition_id: u32,
) -> impl Query<LeaderboardV3> {
    ctx.from
        .tab_leaderboard_v3()
        .filter(|r| r.parent_id.eq(competition_id))
}

#[cfg(test)]
mod tests {
    use tm_server_types::config::TmMode;

    use super::*;
    use crate::leaderboard::{
        select::{LbSelectPlaces, LbSelectSettings},
        test_util::{MATCH, for_team, round, round_in, test_context},
    };

    #[test]
    fn errors_name_the_node_and_step_which_failed() {
        let test = test_context(&[(1, TmMode::ReverseCup)]);
        let leaderboard = NodeHandle::LeaderboardV1(3);

        // The reverse cup has no config to take its starting points from.
        let error = evaluate_steps(
            &LbContext::new(&test.anonymous_view_context(), leaderboard),
            &[
                LbSettingsV3::Select(LbSelectSettings {
                    scope: Vec::new(),
                    places: LbSelectPlaces::FirstN(5),
                }),
                LbSettingsV3::Match,
            ],
            vec![round(1, 1, -500, 0)],
        )
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "LeaderboardV1(3) step 2: Could not find the reverse cup config of MatchV1(1)."
        );
    }

    #[test]
    fn team_standings_from_the_best_rounds_of_their_players_in_different_matches() {
        let test = test_context(&[]);
        let leaderboard = NodeHandle::LeaderboardV1(3);
        let other = NodeHandle::MatchV1(2);
        let rounds = |node, user, team, times: [i32; 3]| {
            (1..)
                .zip(times)
                .map(move |(number, time)| for_team(round_in(node, user, number, 0, time), team))
        };

        // Every team has one player in each match, a time of 0 was not driven.
        let entries = rounds(MATCH, 1, 1, [100, 300, 200])
            .chain(rounds(MATCH, 3, 2, [150, 150, 500]))
            .chain(rounds(other, 2, 1, [400, 100, 0]))
            .chain(rounds(other, 4, 2, [200, 250, 900]))
            .collect();

        let standings = evaluate_steps(
            &LbContext::new(&test.anonymous_view_context(), leaderboard),
            &[
                LbSettingsV3::Filter(LbFilterSettingsV2::best(
                    vec![LbKey::Origin],
                    LbParams::Time,
                    2,
                )),
                LbSettingsV3::MergeTeams(LbMergeSettingsV2::summate(LbParams::Time)),
            ],
            entries,
        )
        .unwrap();

        // Team 1 has 100 + 200 and 100 + 400, team 2 has 150 + 150 and 200 + 250.
        let rows: Vec<(u32, u32, u16, i32)> = standings
            .iter()
            .map(|e| (e.user_id, e.team_id, e.position, e.time))
            .collect();
        assert_eq!(rows, vec![(0, 2, 1, 750), (0, 1, 2, 800)]);
        assert!(standings.iter().all(|e| e.get_node() == leaderboard));
    }
}

// We should be able to iterate over every input and accumulate score or position.
//After the accumulation there shuold also be math operations possible.

// How would a distribution onto two servers work?
// would require a 50/50 rotating live distribution of players

/* trait ModifiableLeaderboard {
    fn evaluate(self, input: Vec<MatchRoundPlayer>) -> Vec<MatchRoundPlayer>;
} */

// Leadarboard should be implicitly constrained to a specific player.
// this is important because a player is its own "entity" and it makes no sense
// to have one in a leadearboard multiple times.

// Matches have two "channels" rounds leadarboard and match leadarboard.
// The match leadarboard is only virtually constructed.
// Does this mean the leaderboard node should do the same??? -> i guess ja
// This would also be very good then mhm.

// -> we nonoetheless need to think of new types which we can remap between.
// -> this implies that nothing is materialized so no inndices are possible on SpacetimeType.

// The connetion filtering is applied to the match_leadarboard.
// This means in order to apply the filter input we would have to evaluate the match leadarboard
// and then get the players which are currently in front and remap it to the rounds again.
// do we want to allow that or not? -> rn it would be alriiiight??
//

// all of the above also means that upon multiple input connections you HAVE to merge them together in the first setting.

/* struct LbWrapper {
    inner: LbEntryV2,
} */

/* enum LbEntryV2 {
    Matches(Vec<Vec<{}>>),
    Pure(Vec<{user,position, score,time}) //-> would be possible stale aswell....

} */

// we have the 0 to mark stale things :thinking:

/* pub(crate) trait LeadearboardOperations {

}

impl LeadearboardOperations for Vec<LbEntry> {
    fn purify(&mut self) {
        self.
    }
} */

// LbPropagator = HashMap<NodeHandle, HashMap<UserId,Vec<LbEntry>>>
// Maybe this denormalization is an option? because we always know the node...
// so the index could be internal?
// -> would require a internal representation which gets passed around like the above
// -> then at the end we could somehow downcast it?
// -> maybe thats also trash idk.

/* mod migrate {
    use spacetimedb::{ReducerContext, Table, reducer};

    use crate::{
        auto_inc_manual::AutoIncWrite,
        leaderboard::{
            LbSettings, LbSettingsV2, LeaderboardV2, tab_leaderboard, tab_leaderboard_v2,
            tab_leaderboard_v2__TableHandle,
        },
    };

    #[reducer]
    fn migration_leaderboard_to_v2(ctx: &ReducerContext) -> Result<(), String> {
        if ctx.db.tab_leaderboard_v2().count() != 0 {
            return Err("The table is not empty anymore.".into());
        }
        let rows = ctx.db.tab_leaderboard().iter();
        let mut max_id = 0;
        for row in rows {
            if row.id > max_id {
                max_id = row.id
            }

            let mut settings = Vec::with_capacity(row.settings.len());
            for setting in row.settings {
                settings.push(setting.into());
            }

            ctx.db.tab_leaderboard_v2().try_insert(LeaderboardV2 {
                name: row.name,
                settings,
                id: row.id,
                parent_id: row.parent_id,
                template: row.template,
                status: row.status,
            })?;
        }

        ctx.auto_inc_migration::<tab_leaderboard_v2__TableHandle>(max_id);

        Ok(())
    }

    impl From<LbSettings> for LbSettingsV2 {
        fn from(value: LbSettings) -> Self {
            match value {
                LbSettings::Merge(lb_merge_settings) => LbSettingsV2::Merge(lb_merge_settings),
                LbSettings::Filter(lb_filter_settings) => LbSettingsV2::Filter(lb_filter_settings),
            }
        }
    }
} */
