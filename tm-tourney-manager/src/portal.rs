use spacetimedb::{ReducerContext, Table, ViewContext, reducer, table, view};

use crate::{
    authorization::Authorization,
    competition::{connection::NodeKindHandle, tab_competition},
    project::permissions::ProjectPermissionsV1,
};

/// A portal can only reach one level deeper?
/// That would allow us to define private things for a template i theory
/// and only expose the things we actually want (with one level deeper maybe?)
/// It would be more work to setup i guess but im realllly not sure atm.
#[table(accessor=tab_portal)]
pub struct Portal {
    name: String,

    #[auto_inc]
    #[primary_key]
    pub id: u32,

    parent_id: u32,
    project_id: u32,

    target_id: u32,
    target_variant: u8,
}

impl Portal {
    pub(crate) fn get_comp_id(&self) -> u32 {
        self.parent_id
    }

    pub(crate) fn get_project(&self) -> u32 {
        self.project_id
    }
}

#[reducer]
fn portal_create(
    ctx: &ReducerContext,
    name: String,
    competition_id: u32,
    target: NodeKindHandle,
) -> Result<(), String> {
    let account_id = ctx.get_user_account()?;
    let Some(competition) = ctx.db.tab_competition().id().find(competition_id) else {
        return Err("Could not find supplied competition_id".into());
    };
    ctx.auth_builder(competition.get_project(), account_id)?
        //.permission(ProjectPermissionsV1::PORTAL_CREATE)
        .authorize()?;

    let target_competition_id = target.get_competition(ctx)?;

    // Portals are only allowed to see one level in.
    if competition.id != target_competition_id {
        return Err("Tried to point the portal to an invalid node!".into());
    }

    // TODO more validation maybe missing?

    let (target_variant, target_id) = target.split();

    ctx.db.tab_portal().try_insert(Portal {
        name,
        id: 0,
        parent_id: competition_id,
        project_id: competition.get_project(),
        target_id,
        target_variant,
    })?;

    Ok(())
}

#[view(accessor=portal_target_suggestions,public)]
pub fn portal_target_suggestions(ctx: &ViewContext /* portal_id: u32 */) -> Vec<()> {
    let portal_id = 1u32;

    let Ok(account_id) = ctx.get_user_account() else {
        return Vec::new();
    };

    //TODO
    Vec::new()
}
