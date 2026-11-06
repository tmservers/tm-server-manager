use spacetimedb::{
    AnonymousViewContext, ProcedureContext, Query, ReducerContext, SpacetimeType, Table, procedure,
    reducer, table, view,
};

use crate::{
    authorization::Authorization,
    auto_inc_manual::AutoIncWrite,
    competition::{
        CompetitionPermissionsV1, CompetitionRead,
        connection::internal_graph_resolution_node_finished,
        node::{NodeHandle, NodeRead, NodeWrite, Vec2},
    },
    pipeline::{
        LbEntry, LbSettingsV3,
        context::LbContext,
        error::LbError,
        evaluate_steps,
        shape::{LbShape, check_steps},
        steps::{filter::LbFilterSettings, merge::LbMergeSettings, remap::LbRemapSettings},
    },
};

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

    // While it got configured it output nothing, so the steps after it were checked against nothing.
    ctx.node_check_dependents(node)?;

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
    fn leaderboard_evaluation(&self, leaderboard_id: u32) -> Result<Vec<LbEntry>, LbError>;
    /// What the leaderboard would output, its steps are checked against its inputs on the way.
    fn leaderboard_output_shape(&self, leaderboard_id: u32) -> Result<LbShape, LbError>;
    fn leaderboards_with_competition(
        &self,
        competition_id: u32,
    ) -> impl Iterator<Item = LeaderboardV3>;
    //fn leaderboard_finalize(&self, lb: Vec<LbEntry>) -> Vec<LbEntry>;
}
impl<Db: spacetimedb::CtxDbRead> LeadearboardRead for Db {
    fn leaderboard_evaluation(&self, leaderboard_id: u32) -> Result<Vec<LbEntry>, LbError> {
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

        if lb.status == LeaderboardStatus::Configuring {
            log::warn!("Tried to eval leadarboard but it is still getting configured.");
            return Ok(Vec::new());
        }

        let node = NodeHandle::LeaderboardV1(leaderboard_id);
        let input = self.node_resolve_input_data(node)?;

        evaluate_steps(&LbContext::new(self, node), &lb.settings, input)
    }

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
