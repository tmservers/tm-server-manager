use spacetimedb::{
    AnonymousViewContext, Query, ReducerContext, SpacetimeType, Table, reducer, table, view,
};

use crate::{
    authorization::Authorization, competition::template::competition_template_instantiate,
};

pub(super) mod connection;
pub(super) mod node;
mod permissions;
pub mod roles;
pub mod server_pool;
mod template;
pub(crate) use permissions::CompetitionPermissionsV1;

#[derive(Debug, Clone)]
#[table(accessor= tab_competition)]
pub struct CompetitionV1 {
    name: String,

    #[auto_inc]
    #[primary_key]
    pub id: u32,

    #[index(hash)]
    parent_id: u32,

    // Necessary to hide and mark as immutable
    status: CompetitionStatus,
    template: bool,
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
    pub unsafe fn new(name: String, parent_id: u32) -> Self {
        Self {
            id: 0,
            parent_id,
            name,
            status: CompetitionStatus::Configuring,
            template: false,
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
            status: CompetitionStatus::Configuring,
            template: true,
        }
    }
}

#[derive(Debug, SpacetimeType, Clone, Copy, PartialEq, Eq)]
pub enum CompetitionStatus {
    Configuring,
    Configured,
    /// Once the competition is ongoing the configuration is immutable.
    /// That means it will play through the configured stages and advancing logic.
    Ongoing,
    /// The whole competition is now immutable.
    Completed,
    Locked,
}

/// Adds a new Competition to the specified project.
#[reducer]
pub fn competition_create(
    ctx: &ReducerContext,
    name: String,
    parent_id: u32,
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
        let new_competition = unsafe { CompetitionV1::new(name, parent_id) };
        ctx.db.tab_competition().try_insert(new_competition)?;
    }

    Ok(())
}

#[reducer]
pub fn competition_configured(ctx: &ReducerContext, id: u32) -> Result<(), String> {
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
}

#[reducer]
fn competition_ongoing(ctx: &ReducerContext, id: u32) -> Result<(), String> {
    //TODO
    ctx.auth_builder(id)
        //.permission(CompetitionPermissionsV1::COMPETITION_)
        .authorize()?;

    authorized_competition_ongoing(ctx, id)
}

pub(crate) fn authorized_competition_ongoing(ctx: &ReducerContext, id: u32) -> Result<(), String> {
    let Some(mut competition) = ctx.db.tab_competition().id().find(id) else {
        return Err("Competition was mot found!".into());
    };

    if competition.status != CompetitionStatus::Configured {
        return Err("Competition is not in configured state".into());
    }
    competition.status = CompetitionStatus::Ongoing;

    ctx.db.tab_competition().id().update(competition);

    Ok(())
}

#[reducer]
pub fn competition_edit_name(
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
}

#[view(accessor=competition,public)]
pub fn competition(ctx: &AnonymousViewContext) -> impl Query<CompetitionV1> {
    ctx.from
        .tab_competition()
        //TODO this equality doesnt work atm because of enum
        //.r#where(|t| t.status.ne(projectStatus::Planning))
        .build()
}
