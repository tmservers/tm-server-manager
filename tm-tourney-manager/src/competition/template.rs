use std::collections::HashMap;

use spacetimedb::{ReducerContext, Table, reducer};

use crate::{
    authorization::Authorization,
    competition::{
        CompetitionV1,
        connection::{NodeKindHandle, tab_competition_connection},
        tab_competition,
    },
    project::permissions::ProjectPermissionsV1,
    registration::tab_registration,
    scheduling::tab_schedule,
    tm_match::tab_tm_match,
};

#[reducer]
pub fn competition_template_create(
    ctx: &ReducerContext,
    name: String,
    parent_id: u32,
) -> Result<(), String> {
    let account_id = ctx.get_user_account()?;

    // If parent is valid it is guaranteed that it has a valid project associated with it.
    let Some(parent_competition) = ctx.db.tab_competition().id().find(parent_id) else {
        return Err("Invalid parent_id".into());
    };

    //TODO make separate permission?
    ctx.auth_builder(parent_competition.project_id, account_id)?
        .permission(ProjectPermissionsV1::COMPETITION_CREATE)
        .authorize()?;

    //SAFETY: The competition gets commnited afterwards.
    let new_competition =
        unsafe { CompetitionV1::new_template(name, parent_id, parent_competition.get_project()) };
    ctx.db.tab_competition().try_insert(new_competition)?;

    Ok(())
}

pub(super) fn competition_template_instantiate(
    ctx: &ReducerContext,
    target: u32,
    template_id: u32,
) -> Result<(), String> {
    let account_id = ctx.get_user_account()?;

    // If parent is valid it is guaranteed that it has a valid project associated with it.
    let Some(competition_template) = ctx.db.tab_competition().id().find(template_id) else {
        return Err("Invalid parent_id".into());
    };

    if !competition_template.is_template() {
        return Err("Cannot instantiate a template from a non template competition.".into());
    }

    if ctx
        .db
        .tab_competition()
        .id()
        .find(competition_template.parent_id)
        .unwrap()
        .is_template()
    {
        return Err("Cannot instantiate a non root competition as a template. This restriction might get lifted in the future".into());
    }

    //TODO evaluate if other permission would be better.
    ctx.auth_builder(competition_template.project_id, account_id)?
        .permission(ProjectPermissionsV1::COMPETITION_CREATE)
        .authorize()?;

    // Collect all node types which are inside the template.
    let connections = ctx
        .db
        .tab_competition_connection()
        .parent_id()
        .filter(competition_template.id);

    let matches = ctx
        .db
        .tab_tm_match()
        .parent_id()
        .filter(competition_template.id);
    let competitions = ctx
        .db
        .tab_competition()
        .parent_id()
        .filter(competition_template.id);
    let registrations = ctx
        .db
        .tab_registration()
        .parent_id()
        .filter(competition_template.id);
    let schedules = ctx
        .db
        .tab_schedule()
        .parent_id()
        .filter(competition_template.id);
    //TODO rest of node types

    // Instanatiate the top level node.
    let new_comp = competition_template.instantiate(target);
    let new_comp = ctx.db.tab_competition().try_insert(new_comp)?;

    let mut match_map = HashMap::new();
    for old_match in matches {
        let old_id = old_match.id;
        let new_match = old_match.instantiate(new_comp.id);
        let new_match = ctx.db.tab_tm_match().try_insert(new_match)?;
        match_map.insert(old_id, new_match);
    }

    let mut competition_map = HashMap::new();
    for old_competition in competitions {
        let old_id = old_competition.id;
        let new_competition = old_competition.instantiate(new_comp.id);
        let new_competition = ctx.db.tab_competition().try_insert(new_competition)?;
        competition_map.insert(old_id, new_competition);
    }

    let mut registration_map = HashMap::new();
    for old_registration in registrations {
        let old_id = old_registration.id;
        let new_registration = old_registration.instantiate(new_comp.id);
        let new_registration = ctx.db.tab_registration().try_insert(new_registration)?;
        registration_map.insert(old_id, new_registration);
    }

    let mut schedule_map = HashMap::new();
    for old_schedule in schedules {
        let old_id = old_schedule.scheduled_id as u32;
        let new_schedule = old_schedule.instantiate(new_comp.id);
        let new_schedule = ctx.db.tab_schedule().try_insert(new_schedule)?;
        schedule_map.insert(old_id, new_schedule);
    }

    // Rewire all connections with the corresponding maps.
    for old_connection in connections {
        let old_origin = old_connection.connection_origin();
        let new_origin = match old_origin {
            NodeKindHandle::MatchV1(m) => match_map.get(&m).unwrap().id,
            NodeKindHandle::CompetitionV1(i) => competition_map.get(&i).unwrap().id,
            NodeKindHandle::MonitoringV1(_) => todo!(),
            NodeKindHandle::ServerV1(_) => todo!(),
            NodeKindHandle::SchedulingV1(i) => schedule_map.get(&i).unwrap().scheduled_id as u32,
            NodeKindHandle::PortalV1(_) => todo!(),
            NodeKindHandle::RegistrationV1(i) => registration_map.get(&i).unwrap().id,
        };

        let old_target = old_connection.connection_target();
        let new_target = match old_target {
            NodeKindHandle::MatchV1(m) => match_map.get(&m).unwrap().id,
            NodeKindHandle::CompetitionV1(i) => competition_map.get(&i).unwrap().id,
            NodeKindHandle::MonitoringV1(_) => todo!(),
            NodeKindHandle::ServerV1(_) => todo!(),
            NodeKindHandle::SchedulingV1(i) => schedule_map.get(&i).unwrap().scheduled_id as u32,
            NodeKindHandle::PortalV1(_) => todo!(),
            NodeKindHandle::RegistrationV1(i) => registration_map.get(&i).unwrap().id,
        };

        let mut new_connection = old_connection.instantiate(new_comp.id);
        new_connection.update_origin(new_origin);
        new_connection.update_target(new_target);
        ctx.db
            .tab_competition_connection()
            .try_insert(new_connection)?;
    }

    Ok(())
}
