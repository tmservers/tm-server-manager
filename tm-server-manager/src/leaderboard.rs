use std::collections::HashMap;

use spacetimedb::{
    Local, ProcedureContext, ReducerContext, SpacetimeType, Table, procedure, reducer, table,
};
use tm_server_types::config::TmMode;

use crate::{
    authorization::Authorization,
    auto_inc_manual::AutoIncWrite,
    competition::{
        CompetitionPermissionsV1,
        connection::tab_connection__view,
        node::{NodeHandle, NodeLeaderboard, NodeRead, NodeWrite, Vec2},
        tab_competition,
    },
    leaderboard::{filter::LbFilterSettings, merge::LbMergeSettings, remap::LbRemapSettings},
    tm_match::leaderboard::{MatchLeadearboardRead, MatchRoundPlayer},
};

mod filter;
mod merge;
mod remap;

#[table(accessor= tab_leaderboard)]
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

#[table(accessor= tab_leaderboard_v2)]
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

impl LeaderboardV2 {
    pub(crate) fn instantiate(
        mut self,
        parent_id: u32,
        stay_template: bool,
        ctx: &ReducerContext,
    ) -> Self {
        self.template = stay_template;
        self.parent_id = parent_id;
        self.id = ctx.auto_inc::<tab_leaderboard_v2__TableHandle>();
        self
    }

    pub(crate) fn is_template(&self) -> bool {
        self.template
    }

    pub(crate) fn get_comp_id(&self) -> u32 {
        self.parent_id
    }
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

#[derive(Debug, SpacetimeType, Clone, Copy)]
enum LbSettingsV2 {
    Merge(LbMergeSettings),
    Filter(LbFilterSettings),
    Remap(LbRemapSettings),
    Finalize,
}

#[derive(Debug, SpacetimeType, Clone, Copy)]
enum LbParams {
    Score,
    Time,
    Position,
}

#[derive(Debug, SpacetimeType, Clone, Copy)]
pub struct LbEntry {
    // Required
    pub user_id: u32,
    // Required
    pub position: u16,
    // Default: 0
    pub round: u16,
    // Default: 0
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
    pub mode: TmMode,

    //TODO maybe
    // mode_fallback: enum {ScoreAsc,ScoreDsc,TimeAsc,...}

    // TODO could be used for fallback
    step_idx: u8,
}

impl LbEntry {
    pub(crate) fn new(
        user_id: u32,
        mode: TmMode,
        position: u16, /* , origin_idx: u16 */
        origin: NodeHandle,
    ) -> Self {
        let (node_variant, node_id) = origin.split();
        LbEntry {
            user_id,
            position,
            mode,
            map_id: 0,
            round: 0,
            score: i32::MIN,
            time: i32::MIN,
            node_id,
            node_variant,
            step_idx: 0, //TODO
        }
    }

    pub(crate) fn set_score(mut self, score: i32) -> Self {
        self.score = score;
        self
    }

    pub(crate) fn set_origin(&mut self, origin: NodeHandle) {
        let (origin_idx, origin_id) = origin.split();
        self.node_id = origin_id;
        self.node_variant = origin_idx;
    }

    pub(crate) fn set_time(mut self, time: i32) -> Self {
        self.time = time;
        self
    }

    pub(crate) fn set_round(mut self, round: u16) -> Self {
        self.round = round;
        self
    }
    pub(crate) fn set_map(mut self, map: u32) -> Self {
        self.map_id = map;
        self
    }

    pub(crate) fn get_user(&self) -> u32 {
        self.user_id
    }

    pub(crate) fn get_mode(&self) -> TmMode {
        self.mode
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
) -> Result<(), String> {
    let Some(parent_competition) = ctx.db.tab_competition().id().find(parent_id) else {
        return Err("Invalid competition".into());
    };

    ctx.auth_builder(parent_id)
        //.permission(CompetitionPermissionsV1::LEADERB)
        .authorize()?;

    if parent_competition.is_template() {
        return Err(
            "Cannot add a normal server to a template. Try do add a template server to id.".into(),
        );
    }

    //TODO validation.

    // Try to load template if provided
    if with_template != 0 {
        ctx.leaderboard_template_instantiate(with_template)?;
    } else {
        let output = LeaderboardV2 {
            name,
            id: ctx.auto_inc::<tab_leaderboard_v2__TableHandle>(),
            parent_id,
            template: false,
            status: LeaderboardStatus::Configuring,
            settings: Vec::new(),
        };

        let output = ctx.db.tab_leaderboard_v2().try_insert(output)?;

        ctx.node_create(NodeHandle::LeaderboardV1(output.id), position)?;
    }

    Ok(())
}

#[reducer]
fn leaderboard_configured(ctx: &ReducerContext, id: u32) -> Result<(), String> {
    let Some(mut leaderboard) = ctx.db.tab_leaderboard_v2().id().find(id) else {
        return Err("Leaderboard was mot found!".into());
    };

    ctx.auth_builder(leaderboard.parent_id)
        .permission(CompetitionPermissionsV1::MATCH_CONFIGURE)
        .authorize()?;

    if leaderboard.status != LeaderboardStatus::Configuring {
        return Err("Leaderboard is not in configuring state".into());
    }
    leaderboard.status = LeaderboardStatus::Configured;

    ctx.db.tab_leaderboard_v2().id().update(leaderboard);

    Ok(())
}

#[reducer]
fn leaderboard_settings_update(
    ctx: &ReducerContext,
    id: u32,
    settings: Vec<LbSettingsV2>,
) -> Result<(), String> {
    let Some(mut leaderboard) = ctx.db.tab_leaderboard_v2().id().find(id) else {
        return Err("Leaderboard not found.".into());
    };

    ctx.auth_builder(leaderboard.parent_id)
        //.permission(CompetitionPermissionsV1::REGISTRATION_CREATE)
        .authorize()?;

    //TODO maybe add state to reevaluation.
    //leaderboard.can_change_settings()?;

    leaderboard.settings = settings;

    ctx.db.tab_leaderboard_v2().id().update(leaderboard);

    Ok(())
}

pub(crate) trait LeadearboardRead {
    fn leaderboard_evaluation(&self, leaderboard_id: u32) -> Vec<LbEntry>;
    //fn leaderboard_finalize(&self, lb: Vec<LbEntry>) -> Vec<LbEntry>;
}
impl<Db: spacetimedb::CtxDbRead> LeadearboardRead for Db {
    fn leaderboard_evaluation(&self, leaderboard_id: u32) -> Vec<LbEntry> {
        let Some(lb) = self
            .db_read_only()
            .tab_leaderboard_v2()
            .id()
            .find(leaderboard_id)
        else {
            log::warn!("Leaderboard was evaluated which does not exist.");
            return Vec::new();
        };

        if lb.status == LeaderboardStatus::Configuring {
            log::warn!("Tried to eval leadarboard but it is still getting configured.");
            return Vec::new();
        }

        let settings = lb.settings;

        let mut leaderboards =
            self.node_resolve_input_data(NodeHandle::LeaderboardV1(leaderboard_id));

        for (index, setting) in settings.into_iter().enumerate() {
            leaderboards = match setting {
                LbSettingsV2::Merge(lb_merge_settings) => {
                    lb_merge_settings.evaluate(leaderboard_id, leaderboards)
                }
                LbSettingsV2::Filter(lb_filter_settings) => {
                    lb_filter_settings.evaluate(leaderboard_id, leaderboards, self)
                }
                LbSettingsV2::Remap(lb_remap_settings) => {
                    lb_remap_settings.evaluate(leaderboard_id, leaderboards)
                }
                LbSettingsV2::Finalize => leaderboards.finalize(self),
            }
        }

        leaderboards
    }
}
pub(crate) trait LeaderboardWrite: LeadearboardRead {
    fn leaderboard_template_instantiate(&self, with_template: u32) -> Result<(), String>;
    fn leaderboard_insert(&self, output: LeaderboardV2) -> Result<LeaderboardV2, String>;
    fn leaderboard_name_edit(&self, leaderboard_i32: u32, name: String) -> Result<(), String>;
}
impl<Db: spacetimedb::CtxDbWrite> LeaderboardWrite for Db {
    fn leaderboard_template_instantiate(&self, with_template: u32) -> Result<(), String> {
        todo!()
    }

    fn leaderboard_insert(&self, output: LeaderboardV2) -> Result<LeaderboardV2, String> {
        todo!()
    }

    fn leaderboard_name_edit(&self, leaderboard_id: u32, name: String) -> Result<(), String> {
        let Some(mut tm_match) = self.db().tab_leaderboard_v2().id().find(leaderboard_id) else {
            return Err("Match not found.".into());
        };
        tm_match.name = name;
        self.db().tab_leaderboard_v2().id().update(tm_match);

        Ok(())
    }
}

#[procedure(name = "unstable_competition_leaderboards")]
fn unstable_competition_leaderboards(
    ctx: &mut ProcedureContext,
    competition_id: u32,
) -> Vec<LeaderboardV2> {
    ctx.with_tx(|ctx| {
        ctx.db
            .tab_leaderboard_v2()
            .parent_id()
            .filter(competition_id)
    })
    .collect()
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

mod migrate {
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
}
