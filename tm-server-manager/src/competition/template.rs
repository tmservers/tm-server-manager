use std::collections::HashMap;

use spacetimedb::{ReducerContext, Table, reducer};

use crate::{
    authorization::Authorization,
    competition::{
        CompetitionPermissionsV1, CompetitionV1,
        connection::{data::tab_connection_data, tab_connection},
        node::{CompetitionNodePosition, NodeHandle, NodePositionRead, NodeWrite, Vec2},
        tab_competition,
    },
    input::{InputRead, InputWrite},
    leaderboard::tab_leaderboard_v2,
    output::{OutputRead, OutputWrite},
    raw_server::config::{RawServerContigRead, tab_raw_server_config_v2},
    registration::tab_registration,
    schedule::tab_schedule,
    tm_match::tab_match,
    tm_server::tab_server,
};

#[reducer]
pub fn competition_template_create(
    ctx: &ReducerContext,
    name: String,
    parent_id: u32,
    with_template: u32,
) -> Result<(), String> {
    //TODO make separate permission?
    ctx.auth_builder(parent_id)
        .permission(CompetitionPermissionsV1::COMPETITION_CREATE)
        .authorize()?;

    if with_template != 0 {
        competition_template_instantiate(ctx, parent_id, with_template, name)?;
    } else {
        //SAFETY: The competition gets commnited afterwards.
        let new_competition = unsafe { CompetitionV1::new_template(name, parent_id) };

        ctx.db.tab_competition().try_insert(new_competition)?;
    }

    Ok(())
}

pub(super) fn competition_template_instantiate(
    ctx: &ReducerContext,
    target_id: u32,
    template_id: u32,
    name: String,
) -> Result<(), String> {
    // If parent is valid it is guaranteed that it has a valid project associated with it.
    let Some(competition_template) = ctx.db.tab_competition().id().find(template_id) else {
        return Err("Invalid parent_id".into());
    };

    if !competition_template.is_template() {
        return Err("Cannot instantiate a template from a non template competition.".into());
    }

    // If parent is valid it is guaranteed that it has a valid project associated with it.
    let Some(target_competition) = ctx.db.tab_competition().id().find(target_id) else {
        return Err("Invalid parent_id".into());
    };

    let stay_template = target_competition.is_template();
    /* if target_competition.is_template() {
        return Err("Cannot do that");
    } */

    //TODO this would make sense in another variation
    /* if ctx
        .db
        .tab_competition()
        .id()
        .find(competition_template.parent_id)
        .unwrap()
        .is_template()
    {
        return Err("Cannot instantiate a non root competition as a template. This restriction might get lifted in the future".into());
    } */

    //TODO evaluate if other permission would be better.
    ctx.auth_builder(competition_template.id)
        .permission(CompetitionPermissionsV1::COMPETITION_CREATE)
        .authorize()?;

    // Collect all node types which are inside the template.
    let connections = ctx
        .db
        .tab_connection()
        .parent_id()
        .filter(competition_template.id);

    let matches = ctx
        .db
        .tab_match()
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
    let servers = ctx
        .db
        .tab_server()
        .parent_id()
        .filter(competition_template.id);
    let leaderboards = ctx
        .db
        .tab_leaderboard_v2()
        .parent_id()
        .filter(competition_template.id);

    let inputs = ctx.inputs_in_parent(competition_template.id);

    // This is always maximnum 1 but keeping the pattern consistent
    let outputs = ctx.outputs_in_parent(competition_template.id);

    let positions: Vec<CompetitionNodePosition> =
        ctx.node_positions(competition_template.id).collect();

    // Instanatiate the top level node.
    let mut new_comp = competition_template.instantiate(target_id, stay_template);
    new_comp.name = name;
    let new_comp = ctx.db.tab_competition().try_insert(new_comp)?;

    let configs = ctx.raw_server_config_shared_competition(template_id);

    let mut config_map = HashMap::new();
    for old_config in configs {
        let old_id = old_config.id;
        let new_config = old_config.instantiate(new_comp.id, ctx);
        let new_config = ctx.db.tab_raw_server_config_v2().try_insert(new_config)?;
        config_map.insert(old_id, new_config);
    }

    let mut match_map = HashMap::new();
    for old_match in matches {
        let old_id = old_match.id;
        let mut new_match = old_match.instantiate(new_comp.id, stay_template);

        new_match.update_shared_configs(&config_map);

        let new_match = ctx.db.tab_match().try_insert(new_match)?;
        ctx.node_create(
            NodeHandle::MatchV1(new_match.id),
            if let Some(val) = positions
                .iter()
                .find(|n| n.node == NodeHandle::MatchV1(old_id))
            {
                val.position
            } else {
                Vec2::ZERO
            },
        )?;
        match_map.insert(old_id, new_match);
    }

    let mut competition_map = HashMap::new();
    for old_competition in competitions {
        let old_id = old_competition.id;
        let old_name = old_competition.name.clone();
        let new_competition = old_competition.instantiate(new_comp.id, stay_template);
        let new_competition = ctx.db.tab_competition().try_insert(new_competition)?;
        competition_template_instantiate(ctx, new_competition.id, old_id, old_name)?;
        ctx.node_create(
            NodeHandle::CompetitionV1(new_competition.id),
            if let Some(val) = positions
                .iter()
                .find(|n| n.node == NodeHandle::CompetitionV1(old_id))
            {
                val.position
            } else {
                Vec2::ZERO
            },
        )?;
        competition_map.insert(old_id, new_competition);
    }

    let mut registration_map = HashMap::new();
    for old_registration in registrations {
        let old_id = old_registration.id;
        let new_registration = old_registration.instantiate(new_comp.id, stay_template);
        let new_registration = ctx.db.tab_registration().try_insert(new_registration)?;
        ctx.node_create(
            NodeHandle::RegistrationV1(new_registration.id),
            if let Some(val) = positions
                .iter()
                .find(|n| n.node == NodeHandle::RegistrationV1(old_id))
            {
                val.position
            } else {
                Vec2::ZERO
            },
        )?;
        registration_map.insert(old_id, new_registration);
    }

    let mut schedule_map = HashMap::new();
    for old_schedule in schedules {
        let old_id = old_schedule.id;
        let new_schedule = old_schedule.instantiate(new_comp.id, stay_template);
        let new_schedule = ctx.db.tab_schedule().try_insert(new_schedule)?;
        ctx.node_create(
            NodeHandle::ScheduleV1(new_schedule.id),
            if let Some(val) = positions
                .iter()
                .find(|n| n.node == NodeHandle::ScheduleV1(old_id))
            {
                val.position
            } else {
                Vec2::ZERO
            },
        )?;
        schedule_map.insert(old_id, new_schedule);
    }

    let mut server_map = HashMap::new();
    for old_server in servers {
        let old_id = old_server.id;
        let new_server = old_server.instantiate(new_comp.id, stay_template);
        let new_server = ctx.db.tab_server().try_insert(new_server)?;
        ctx.node_create(
            NodeHandle::ServerV1(new_server.id),
            if let Some(val) = positions
                .iter()
                .find(|n| n.node == NodeHandle::ServerV1(old_id))
            {
                val.position
            } else {
                Vec2::ZERO
            },
        )?;
        server_map.insert(old_id, new_server);
    }

    let mut leadearboard_map = HashMap::new();
    for old_leaderboard in leaderboards {
        let old_id = old_leaderboard.id;
        let new_leadearboard = old_leaderboard.instantiate(new_comp.id, stay_template, ctx);
        let new_leaderboard = ctx.db.tab_leaderboard_v2().try_insert(new_leadearboard)?;
        ctx.node_create(
            NodeHandle::LeaderboardV1(new_leaderboard.id),
            if let Some(val) = positions
                .iter()
                .find(|n| n.node == NodeHandle::LeaderboardV1(old_id))
            {
                val.position
            } else {
                Vec2::ZERO
            },
        )?;
        leadearboard_map.insert(old_id, new_leaderboard);
    }

    let mut input_map = HashMap::new();
    for old_input in inputs {
        let old_id = old_input.id;
        let new_input = old_input.instantiate(new_comp.id, stay_template);
        let new_input = ctx.input_insert(new_input)?;
        ctx.node_create(
            NodeHandle::InputV1(new_input.id),
            if let Some(val) = positions
                .iter()
                .find(|n| n.node == NodeHandle::InputV1(old_id))
            {
                val.position
            } else {
                Vec2::ZERO
            },
        )?;
        input_map.insert(old_id, new_input);
    }

    let mut output_map = HashMap::new();
    for old_output in outputs {
        let old_id = old_output.id;
        let new_output = old_output.instantiate(new_comp.id, stay_template);
        ctx.node_create(
            NodeHandle::OutputV1(new_output.id),
            if let Some(val) = positions
                .iter()
                .find(|n| n.node == NodeHandle::OutputV1(old_id))
            {
                val.position
            } else {
                Vec2::ZERO
            },
        )?;
        let new_output = ctx.output_insert(new_output)?;
        output_map.insert(old_id, new_output);
    }

    // Rewire all connections with the corresponding maps.
    for old_connection in connections {
        let old_origin = old_connection.origin();
        let new_origin = match old_origin {
            NodeHandle::CompetitionV1(i) => {
                if let Some(comp) = competition_map.get(&i) {
                    comp.id
                } else {
                    // The Connections origin is the currently new competition.
                    new_comp.id
                }
            }
            NodeHandle::MatchV1(m) => match_map.get(&m).unwrap().id,
            NodeHandle::ServerV1(n) => server_map.get(&n).unwrap().id,
            NodeHandle::ScheduleV1(i) => schedule_map.get(&i).unwrap().id,
            NodeHandle::RegistrationV1(i) => registration_map.get(&i).unwrap().id,
            NodeHandle::InputV1(n) => input_map.get(&n).unwrap().id,
            NodeHandle::OutputV1(n) => output_map.get(&n).unwrap().id,
            NodeHandle::LeaderboardV1(n) => leadearboard_map.get(&n).unwrap().id,
        };

        let old_target = old_connection.target();
        let new_target = match old_target {
            NodeHandle::MatchV1(m) => match_map.get(&m).unwrap().id,
            NodeHandle::CompetitionV1(i) => competition_map.get(&i).unwrap().id,
            NodeHandle::ServerV1(n) => server_map.get(&n).unwrap().id,
            NodeHandle::ScheduleV1(i) => schedule_map.get(&i).unwrap().id,
            NodeHandle::RegistrationV1(i) => registration_map.get(&i).unwrap().id,
            NodeHandle::InputV1(n) => input_map.get(&n).unwrap().id,
            NodeHandle::OutputV1(n) => output_map.get(&n).unwrap().id,
            NodeHandle::LeaderboardV1(n) => leadearboard_map.get(&n).unwrap().id,
        };

        let mut new_connection = old_connection.instantiate(new_comp.id);
        new_connection.update_origin(new_origin);
        new_connection.update_target(new_target);
        let new_connection = ctx.db.tab_connection().try_insert(new_connection)?;

        if old_connection.is_data() {
            let old_connection_data = ctx
                .db
                .tab_connection_data()
                .connection_id()
                .find(old_connection.id)
                .unwrap();
            let new_connection_data =
                old_connection_data.instantiate(new_connection.id, new_comp.id);
            ctx.db
                .tab_connection_data()
                .try_insert(new_connection_data)?;
        }
        //TODO also instantiate action connections
    }

    Ok(())
}
