use spacetimedb::{ProcedureContext, ReducerContext, Table, procedure, reducer, table};

use crate::{
    authorization::Authorization,
    competition::{
        node::{NodeHandle, NodeWrite, Vec2},
        roles::{CompetitionMember, tab_competition_member},
        template::competition_template_instantiate,
    },
};

pub(super) mod connection;
pub(super) mod node;
mod permissions;
pub mod roles;
pub mod server_pool;
mod template;
pub(crate) use permissions::CompetitionPermissionsV1;

/// Acts as the container for a node graph.
/// Can also be declared as a template.
/// Templates can also be public.
/// On instantiation all non-template nodes are instantiated.
#[derive(Debug, Clone)]
#[table(accessor= tab_competition,vis_private)]
pub struct CompetitionV1 {
    name: String,

    #[auto_inc]
    #[primary_key]
    pub id: u32,

    #[index(hash)]
    parent_id: u32,

    template: bool,

    #[default(false)]
    template_public: bool,
}

impl CompetitionV1 {
    pub(crate) fn get_comp_id(&self) -> u32 {
        self.parent_id
    }

    pub(crate) fn get_name(&self) -> &String {
        &self.name
    }

    pub fn is_template(&self) -> bool {
        self.template
    }

    fn instantiate(mut self, new_parent: u32, stay_template: bool) -> Self {
        self.template = stay_template;
        self.id = 0;
        self.parent_id = new_parent;
        self
    }

    /// # Safety
    /// The new competition has to be commited to spacetime db through the `competition_create` reducer.
    /// Otherwise the id is invalid.
    fn new(name: String, parent_id: u32) -> Self {
        Self {
            id: 0,
            parent_id,
            name,
            //status: CompetitionStatus::Configuring,
            template: false,
            template_public: false,
        }
    }

    /// # Safety
    /// The new competition has to be commited to spacetime db through the `competition_create` reducer.
    /// Otherwise the id is invalid.
    pub unsafe fn new_template(name: String, parent_id: u32) -> Self {
        Self {
            id: 0,
            parent_id,
            name,
            //status: CompetitionStatus::Configuring,
            template: true,
            template_public: false,
        }
    }
}

/* #[derive(Debug, SpacetimeType, Clone, Copy, PartialEq, Eq)]
pub enum CompetitionStatus {
    Configuring,
    Configured,
    /// Once the competition is ongoing the configuration is immutable.
    /// That means it will play through the configured stages and advancing logic.
    Ongoing,
    /// The whole competition is now immutable.
    Completed,
    //Locked,
} */

/// Adds a new Competition to the specified project.
#[reducer]
fn competition_create(
    ctx: &ReducerContext,
    name: String,
    parent_id: u32,
    position: Vec2,
    with_template: u32,
) -> Result<(), String> {
    // If parent is valid it is guaranteed that it has a valid project associated with it.
    let Some(parent_competition) = ctx.db.tab_competition().id().find(parent_id) else {
        return Err("Invalid parent_id".into());
    };

    ctx.auth_builder(parent_competition.id)
        .permission(CompetitionPermissionsV1::COMPETITION_CREATE)
        .authorize()?;

    if with_template != 0 {
        competition_template_instantiate(ctx, parent_id, with_template, name)?;
    } else {
        //SAFETY: The competition gets commnited afterwards.
        let new_competition = CompetitionV1::new(name, parent_id);
        let comp = ctx.db.tab_competition().try_insert(new_competition)?;
        ctx.node_create(NodeHandle::CompetitionV1(comp.id), position)?;
    }

    Ok(())
}

/* #[reducer]
fn competition_configured(ctx: &ReducerContext, id: u32) -> Result<(), String> {
    let Some(mut competition) = ctx.db.tab_competition().id().find(id) else {
        return Err("Competition was mot found!".into());
    };

    //TODO
    ctx.auth_builder(competition.parent_id)
        //.permission(CompetitionPermissionsV1::COMPETITION_)
        .authorize()?;

    if competition.status != CompetitionStatus::Configuring {
        return Err("Competition is not in configuring state".into());
    }
    competition.status = CompetitionStatus::Configured;

    ctx.db.tab_competition().id().update(competition);

    Ok(())
} */

/* #[reducer]
fn competition_ongoing(ctx: &ReducerContext, id: u32) -> Result<(), String> {
    //TODO
    ctx.auth_builder(id)
        //.permission(CompetitionPermissionsV1::COMPETITION_)
        .authorize()?;

    authorized_competition_ongoing(ctx, id)
} */

/* pub(crate) fn authorized_competition_ongoing(ctx: &ReducerContext, id: u32) -> Result<(), String> {
    let Some(mut competition) = ctx.db.tab_competition().id().find(id) else {
        return Err("Competition was mot found!".into());
    };

    if competition.status != CompetitionStatus::Configured {
        return Err("Competition is not in configured state".into());
    }
    competition.status = CompetitionStatus::Ongoing;

    ctx.db.tab_competition().id().update(competition);

    Ok(())
} */

/* #[reducer]
fn competition_edit_name(
    ctx: &ReducerContext,
    competition_id: u32,
    name: String,
) -> Result<(), String> {
    let Some(mut competition) = ctx.db.tab_competition().id().find(competition_id) else {
        return Err("Invalid competition".into());
    };

    ctx.auth_builder(competition.id)
        .permission(CompetitionPermissionsV1::COMPETITION_EDIT_NAME)
        .authorize()?;

    competition.name = name;

    ctx.db.tab_competition().id().update(competition);

    Ok(())
} */

/* #[view(accessor=competition,public)]
fn competition(ctx: &AnonymousViewContext) -> impl Query<CompetitionV1> {
    ctx.from
        .tab_competition()
        //TODO this equality doesnt work atm because of enum
        //.r#where(|t| t.status.ne(projectStatus::Planning))
        .build()
} */

pub(crate) trait CompetitionRead {
    fn competition_find(&self, competition_id: u32) -> Result<CompetitionV1, String>;
    fn competition_ancestors(&self, competition_id: u32) -> Vec<u32>;
    fn competition_descendants(&self, competition_id: u32) -> Vec<CompetitionV1>;
    fn competition_tree_complete(&self, competition_id: u32) -> Vec<u32>;
}
impl<Db: spacetimedb::CtxDbRead> CompetitionRead for Db {
    fn competition_ancestors(&self, competition_id: u32) -> Vec<u32> {
        let Some(competition) = self
            .db_read_only()
            .tab_competition()
            .id()
            .find(competition_id)
        else {
            return Vec::new();
        };
        let mut ancestors = vec![competition_id];

        let mut parent_id = competition.parent_id;
        while parent_id != 0 {
            if let Some(new_parent) = self.db_read_only().tab_competition().id().find(parent_id) {
                parent_id = new_parent.parent_id;
                ancestors.push(new_parent.id);
                continue;
            }
            parent_id = 0;
        }

        ancestors
    }

    fn competition_descendants(&self, competition_id: u32) -> Vec<CompetitionV1> {
        let mut descendants = Vec::new();
        let mut to_visit = vec![competition_id];

        while let Some(current_id) = to_visit.pop() {
            if let Some(competition) = self.db_read_only().tab_competition().id().find(current_id) {
                descendants.push(competition.clone());
                to_visit.extend(
                    self.db_read_only()
                        .tab_competition()
                        .parent_id()
                        .filter(current_id)
                        .map(|t| t.id),
                );
            }
        }

        descendants
    }

    /// Walks back up to the root of the competition.
    /// Then gathers all children competitions of the root.
    fn competition_tree_complete(&self, competition_id: u32) -> Vec<u32> {
        let root = *self.competition_ancestors(competition_id).last().unwrap();
        self.competition_descendants(root)
            .into_iter()
            .map(|comp| comp.id)
            .collect()
    }

    fn competition_find(&self, competition_id: u32) -> Result<CompetitionV1, String> {
        let Some(comp) = self
            .db_read_only()
            .tab_competition()
            .id()
            .find(competition_id)
        else {
            return Err("Invalid Competition.".into());
        };
        Ok(comp)
    }
}

pub(crate) trait CompetitionWrite: CompetitionRead {
    fn competition_root_create(&self, user_id: u32, name: String) -> Result<u32, String>;
    fn competition_name_edit(&self, competition_id: u32, name: String) -> Result<(), String>;
}
impl<Db: spacetimedb::CtxDbWrite> CompetitionWrite for Db {
    fn competition_root_create(&self, user_id: u32, name: String) -> Result<u32, String> {
        let comp = self
            .db()
            .tab_competition()
            .try_insert(CompetitionV1::new(name, 0))?;
        self.db()
            .tab_competition_member()
            .try_insert(CompetitionMember::new_owner(user_id, comp.id))?;
        Ok(comp.id)
    }

    fn competition_name_edit(&self, competition_id: u32, name: String) -> Result<(), String> {
        let Some(mut tm_match) = self.db().tab_competition().id().find(competition_id) else {
            return Err("Match not found.".into());
        };
        tm_match.name = name;
        self.db().tab_competition().id().update(tm_match);

        Ok(())
    }
}

#[procedure(name = "unstable_competition")]
fn unstable_competition(ctx: &mut ProcedureContext, competition_id: u32) -> Option<CompetitionV1> {
    ctx.with_tx(|ctx| ctx.db.tab_competition().id().find(competition_id))
}

#[procedure(name = "unstable_competition_competitions")]
fn unstable_competition_competitions(
    ctx: &mut ProcedureContext,
    competition_id: u32,
) -> Vec<CompetitionV1> {
    ctx.with_tx(|ctx| ctx.db.tab_competition().parent_id().filter(competition_id))
        .collect()
}
