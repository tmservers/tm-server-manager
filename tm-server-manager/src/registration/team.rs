use spacetimedb::{
    AnonymousViewContext, CtxDbRead, Query, ReducerContext, SpacetimeType, Table, Timestamp,
    ViewContext, reducer, table, view,
};

use crate::{
    authorization::Authorization,
    competition::node::NodeHandle,
    pipeline::{LbEntry, LbEntryKind, LbParticipant},
    registration::{RegistrationSettingsTeam, tab_registration},
};

#[derive(Debug, SpacetimeType, Clone, Copy, PartialEq, Eq)]
pub enum TeamRole {
    /// Manages the team, there is exactly one.
    Captain,
    Player,
    /// Plays like a player, e.g. when someone cannot make it.
    Substitute,
}

/// A team of a team registration.
/// Teams can only change while their registration is ongoing, afterwards the nodes after it rely on them.
#[table(accessor = tab_team, vis_private)]
#[derive(Debug, Clone)]
pub struct RegisteredTeam {
    #[auto_inc]
    #[primary_key]
    pub id: u32,

    #[index(hash)]
    registration_id: u32,

    name: String,
    registered_at: Timestamp,
}

#[table(accessor = tab_team_member, vis_private,
    index(accessor = team_user, hash(columns = [team_id, user_id])))]
#[derive(Debug, Clone, Copy)]
pub struct TeamMember {
    #[index(hash)]
    pub team_id: u32,
    #[index(hash)]
    pub user_id: u32,
    pub role: TeamRole,
}

/// An invite of the captain, the user becomes a member once they accept it.
#[table(accessor = tab_team_invite, vis_private,
    index(accessor = team_user, hash(columns = [team_id, user_id])))]
#[derive(Debug, Clone, Copy)]
pub struct TeamInvite {
    #[index(hash)]
    team_id: u32,
    #[index(hash)]
    user_id: u32,
    invited_at: Timestamp,
}

/// The settings of the team registration, as long as its teams can still change.
fn ongoing_team_registration(
    ctx: &ReducerContext,
    registration_id: u32,
) -> Result<RegistrationSettingsTeam, String> {
    let Some(registration) = ctx.db.tab_registration().id().find(registration_id) else {
        return Err("Registration not found.".into());
    };
    registration.team_registration_allowed()
}

fn team_find(ctx: &impl CtxDbRead, team_id: u32) -> Result<RegisteredTeam, String> {
    ctx.db_read_only()
        .tab_team()
        .id()
        .find(team_id)
        .ok_or_else(|| "Team not found.".into())
}

fn member_find(ctx: &impl CtxDbRead, team_id: u32, user_id: u32) -> Option<TeamMember> {
    ctx.db_read_only()
        .tab_team_member()
        .team_user()
        .filter((team_id, user_id))
        .next()
}

/// The membership of the user in a team of the registration, a user can only be in one of them.
fn registration_member_find(
    ctx: &impl CtxDbRead,
    registration_id: u32,
    user_id: u32,
) -> Option<TeamMember> {
    ctx.db_read_only()
        .tab_team_member()
        .user_id()
        .filter(user_id)
        .find(|m| team_find(ctx, m.team_id).is_ok_and(|t| t.registration_id == registration_id))
}

/// The team as long as `user_id` is its captain and the team can still change.
fn captained_team(
    ctx: &ReducerContext,
    team_id: u32,
    user_id: u32,
) -> Result<RegisteredTeam, String> {
    let team = team_find(ctx, team_id)?;
    ongoing_team_registration(ctx, team.registration_id)?;
    if !member_find(ctx, team_id, user_id).is_some_and(|m| m.role == TeamRole::Captain) {
        return Err("Only the captain can manage the team.".into());
    }
    Ok(team)
}

fn invites_delete(ctx: &ReducerContext, registration_id: u32, user_id: u32) {
    let invites: Vec<TeamInvite> = ctx
        .db
        .tab_team_invite()
        .user_id()
        .filter(user_id)
        .filter(|i| team_find(ctx, i.team_id).is_ok_and(|t| t.registration_id == registration_id))
        .collect();
    for invite in invites {
        ctx.db
            .tab_team_invite()
            .team_user()
            .delete((invite.team_id, invite.user_id));
    }
}

fn team_delete_inner(ctx: &ReducerContext, team_id: u32) {
    ctx.db.tab_team_member().team_id().delete(team_id);
    ctx.db.tab_team_invite().team_id().delete(team_id);
    ctx.db.tab_team().id().delete(team_id);
}

/// Creates a team with `user_id` as its captain.
fn create_team(
    ctx: &ReducerContext,
    user_id: u32,
    registration_id: u32,
    name: String,
) -> Result<RegisteredTeam, String> {
    let settings = ongoing_team_registration(ctx, registration_id)?;

    if registration_member_find(ctx, registration_id, user_id).is_some() {
        return Err("You are already in a team of this registration.".into());
    }
    let teams = ctx
        .db
        .tab_team()
        .registration_id()
        .filter(registration_id)
        .count();
    if teams >= settings.team_limit as usize {
        return Err("Registration maximum teams exceeded.".into());
    }

    let team = ctx.db.tab_team().try_insert(RegisteredTeam {
        id: 0,
        registration_id,
        name,
        registered_at: ctx.timestamp,
    })?;
    ctx.db.tab_team_member().insert(TeamMember {
        team_id: team.id,
        user_id,
        role: TeamRole::Captain,
    });
    invites_delete(ctx, registration_id, user_id);

    Ok(team)
}

fn invite(ctx: &ReducerContext, captain_id: u32, team_id: u32, user_id: u32) -> Result<(), String> {
    let team = captained_team(ctx, team_id, captain_id)?;

    if registration_member_find(ctx, team.registration_id, user_id).is_some() {
        return Err("The user already is in a team of this registration.".into());
    }
    if ctx
        .db
        .tab_team_invite()
        .team_user()
        .filter((team_id, user_id))
        .next()
        .is_some()
    {
        return Err("The user already got invited.".into());
    }

    ctx.db.tab_team_invite().insert(TeamInvite {
        team_id,
        user_id,
        invited_at: ctx.timestamp,
    });
    Ok(())
}

fn accept_invite(ctx: &ReducerContext, user_id: u32, team_id: u32) -> Result<(), String> {
    let team = team_find(ctx, team_id)?;
    let settings = ongoing_team_registration(ctx, team.registration_id)?;

    if ctx
        .db
        .tab_team_invite()
        .team_user()
        .filter((team_id, user_id))
        .next()
        .is_none()
    {
        return Err("You are not invited to this team.".into());
    }
    if registration_member_find(ctx, team.registration_id, user_id).is_some() {
        return Err("You are already in a team of this registration.".into());
    }
    let members = ctx.db.tab_team_member().team_id().filter(team_id).count();
    if members >= settings.team_size_max as usize {
        return Err("The team is already full.".into());
    }

    ctx.db.tab_team_member().insert(TeamMember {
        team_id,
        user_id,
        role: TeamRole::Player,
    });
    invites_delete(ctx, team.registration_id, user_id);
    Ok(())
}

fn leave(ctx: &ReducerContext, user_id: u32, team_id: u32) -> Result<(), String> {
    let team = team_find(ctx, team_id)?;
    ongoing_team_registration(ctx, team.registration_id)?;

    let Some(member) = member_find(ctx, team_id, user_id) else {
        return Err("You are not in this team.".into());
    };
    let members = ctx.db.tab_team_member().team_id().filter(team_id).count();
    if members == 1 {
        team_delete_inner(ctx, team_id);
        return Ok(());
    }
    if member.role == TeamRole::Captain {
        return Err("Make someone else captain before leaving the team.".into());
    }

    ctx.db
        .tab_team_member()
        .team_user()
        .delete((team_id, user_id));
    Ok(())
}

fn role_update(
    ctx: &ReducerContext,
    captain_id: u32,
    team_id: u32,
    user_id: u32,
    role: TeamRole,
) -> Result<(), String> {
    captained_team(ctx, team_id, captain_id)?;
    let Some(member) = member_find(ctx, team_id, user_id) else {
        return Err("The user is not in this team.".into());
    };
    if user_id == captain_id {
        return Err("Make someone else captain to change your own role.".into());
    }

    let set_role = |member: TeamMember, role: TeamRole| {
        ctx.db
            .tab_team_member()
            .team_user()
            .delete((member.team_id, member.user_id));
        ctx.db
            .tab_team_member()
            .insert(TeamMember { role, ..member });
    };
    // There always is exactly one captain, so making someone captain hands it over.
    if role == TeamRole::Captain {
        let captain = member_find(ctx, team_id, captain_id).unwrap();
        set_role(captain, TeamRole::Player);
    }
    set_role(member, role);
    Ok(())
}

#[reducer]
fn team_create(ctx: &ReducerContext, registration_id: u32, name: String) -> Result<(), String> {
    create_team(ctx, ctx.user_id()?, registration_id, name)?;
    Ok(())
}

#[reducer]
fn team_delete(ctx: &ReducerContext, team_id: u32) -> Result<(), String> {
    captained_team(ctx, team_id, ctx.user_id()?)?;
    team_delete_inner(ctx, team_id);
    Ok(())
}

#[reducer]
fn team_invite(ctx: &ReducerContext, team_id: u32, user_id: u32) -> Result<(), String> {
    invite(ctx, ctx.user_id()?, team_id, user_id)
}

#[reducer]
fn team_invite_revoke(ctx: &ReducerContext, team_id: u32, user_id: u32) -> Result<(), String> {
    captained_team(ctx, team_id, ctx.user_id()?)?;
    ctx.db
        .tab_team_invite()
        .team_user()
        .delete((team_id, user_id));
    Ok(())
}

#[reducer]
fn team_invite_accept(ctx: &ReducerContext, team_id: u32) -> Result<(), String> {
    accept_invite(ctx, ctx.user_id()?, team_id)
}

#[reducer]
fn team_invite_decline(ctx: &ReducerContext, team_id: u32) -> Result<(), String> {
    let user_id = ctx.user_id()?;
    ctx.db
        .tab_team_invite()
        .team_user()
        .delete((team_id, user_id));
    Ok(())
}

#[reducer]
fn team_leave(ctx: &ReducerContext, team_id: u32) -> Result<(), String> {
    leave(ctx, ctx.user_id()?, team_id)
}

#[reducer]
fn team_member_remove(ctx: &ReducerContext, team_id: u32, user_id: u32) -> Result<(), String> {
    let captain_id = ctx.user_id()?;
    captained_team(ctx, team_id, captain_id)?;
    if user_id == captain_id {
        return Err("Use leaving the team instead.".into());
    }
    if member_find(ctx, team_id, user_id).is_none() {
        return Err("The user is not in this team.".into());
    }
    ctx.db
        .tab_team_member()
        .team_user()
        .delete((team_id, user_id));
    Ok(())
}

#[reducer]
fn team_member_role_update(
    ctx: &ReducerContext,
    team_id: u32,
    user_id: u32,
    role: TeamRole,
) -> Result<(), String> {
    role_update(ctx, ctx.user_id()?, team_id, user_id, role)
}

/// The teams of a registration.
#[view(accessor = registration_teams, public)]
fn registration_teams(
    ctx: &AnonymousViewContext,
    registration_id: u32,
) -> impl Query<RegisteredTeam> {
    ctx.from
        .tab_team()
        .filter(|r| r.registration_id.eq(registration_id))
}

/// The members of a team.
#[view(accessor = team_members, public)]
fn team_members(ctx: &AnonymousViewContext, team_id: u32) -> impl Query<TeamMember> {
    ctx.from.tab_team_member().filter(|r| r.team_id.eq(team_id))
}

/// The invites of the viewer.
#[view(accessor = my_team_invites, public)]
fn my_team_invites(ctx: &ViewContext) -> Vec<TeamInvite> {
    let Ok(user_id) = ctx.user_id() else {
        return Vec::new();
    };
    ctx.db.tab_team_invite().user_id().filter(user_id).collect()
}

/// The open invites of a team, only for its members.
#[view(accessor = team_invites, public)]
fn team_invites(ctx: &ViewContext, team_id: u32) -> Vec<TeamInvite> {
    let Ok(user_id) = ctx.user_id() else {
        return Vec::new();
    };
    if member_find(ctx, team_id, user_id).is_none() {
        return Vec::new();
    }
    ctx.db.tab_team_invite().team_id().filter(team_id).collect()
}

pub(crate) trait TeamRead {
    /// The users playing for the participant, everyone in the team for a team.
    fn participant_users(&self, participant: LbParticipant) -> Vec<u32>;
    /// The teams of the registration with enough members as standings, the earliest first.
    fn registration_team_lb(
        &self,
        registration_id: u32,
        settings: &RegistrationSettingsTeam,
    ) -> Vec<LbEntry>;
}
impl<Db: CtxDbRead> TeamRead for Db {
    fn participant_users(&self, participant: LbParticipant) -> Vec<u32> {
        match participant {
            LbParticipant::Player(user_id) => vec![user_id],
            LbParticipant::Team(team_id) => self
                .db_read_only()
                .tab_team_member()
                .team_id()
                .filter(team_id)
                .map(|m| m.user_id)
                .collect(),
        }
    }

    fn registration_team_lb(
        &self,
        registration_id: u32,
        settings: &RegistrationSettingsTeam,
    ) -> Vec<LbEntry> {
        let mut teams: Vec<RegisteredTeam> = self
            .db_read_only()
            .tab_team()
            .registration_id()
            .filter(registration_id)
            .filter(|t| {
                let members = self
                    .db_read_only()
                    .tab_team_member()
                    .team_id()
                    .filter(t.id)
                    .count();
                // Teams with too few members do not take part.
                members >= settings.team_size_min as usize
            })
            .collect();
        // Teams registered at the same time still need an order which never changes.
        teams.sort_by_key(|t| (t.registered_at, t.id));

        teams
            .into_iter()
            .zip(1u16..)
            .map(|(team, position)| {
                LbEntry::for_participant(
                    LbParticipant::Team(team.id),
                    LbEntryKind::Standing,
                    position,
                    NodeHandle::RegistrationV1(registration_id),
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use spacetimedb::test_utils::{TestAuth, TestContext};

    use super::*;
    use crate::{
        pipeline::test_util::test_context,
        registration::{Registration, RegistrationSettings, RegistrationStatus},
        tm_match::entrant::{MatchEntrantRead, MatchEntrantWrite, test_util::connect},
    };

    const REGISTRATION: u32 = 1;

    /// A team registration for teams of exactly two.
    fn registration(test: &TestContext, status: RegistrationStatus) {
        test.with_reducer_tx(TestAuth::internal(), |ctx| {
            ctx.db.tab_registration().insert(Registration {
                name: String::new(),
                id: REGISTRATION,
                parent_id: 1,
                settings: RegistrationSettings::Team(RegistrationSettingsTeam {
                    team_limit: 4,
                    team_size_min: 2,
                    team_size_max: 2,
                }),
                status,
                template: false,
            });
            Ok::<_, String>(())
        })
        .unwrap();
    }

    fn run<T>(
        test: &TestContext,
        body: impl FnOnce(&ReducerContext) -> Result<T, String>,
    ) -> Result<T, String> {
        test.with_reducer_tx(TestAuth::internal(), body)
    }

    /// A team of `captain` which `member` joined.
    fn team_of_two(test: &TestContext, captain: u32, member: u32) -> u32 {
        run(test, |ctx| {
            let team = create_team(ctx, captain, REGISTRATION, String::new())?;
            invite(ctx, captain, team.id, member)?;
            accept_invite(ctx, member, team.id)?;
            Ok(team.id)
        })
        .unwrap()
    }

    #[test]
    fn a_user_is_in_one_team_of_a_registration_at_most() {
        let test = test_context(&[]);
        registration(&test, RegistrationStatus::Ongoing);
        let team = team_of_two(&test, 1, 2);

        assert!(
            run(&test, |ctx| create_team(
                ctx,
                2,
                REGISTRATION,
                String::new()
            ))
            .is_err()
        );
        let other = run(&test, |ctx| {
            create_team(ctx, 3, REGISTRATION, String::new())
        })
        .unwrap();
        assert!(run(&test, |ctx| invite(ctx, 3, other.id, 1)).is_err());
        assert!(member_find(&test.anonymous_view_context(), team, 2).is_some());
    }

    #[test]
    fn a_full_team_takes_no_more_members() {
        let test = test_context(&[]);
        registration(&test, RegistrationStatus::Ongoing);
        let team = team_of_two(&test, 1, 2);

        run(&test, |ctx| invite(ctx, 1, team, 3)).unwrap();

        assert!(run(&test, |ctx| accept_invite(ctx, 3, team)).is_err());
    }

    #[test]
    fn handing_over_the_captaincy_keeps_one_captain() {
        let test = test_context(&[]);
        registration(&test, RegistrationStatus::Ongoing);
        let team = team_of_two(&test, 1, 2);

        run(&test, |ctx| role_update(ctx, 1, team, 2, TeamRole::Captain)).unwrap();

        let cx = test.anonymous_view_context();
        assert_eq!(member_find(&cx, team, 1).unwrap().role, TeamRole::Player);
        assert_eq!(member_find(&cx, team, 2).unwrap().role, TeamRole::Captain);
    }

    #[test]
    fn teams_cannot_change_once_the_registration_ended() {
        let test = test_context(&[]);
        registration(&test, RegistrationStatus::Ended);

        assert!(
            run(&test, |ctx| create_team(
                ctx,
                1,
                REGISTRATION,
                String::new()
            ))
            .is_err()
        );
    }

    #[test]
    fn only_teams_with_enough_members_take_part() {
        let test = test_context(&[]);
        registration(&test, RegistrationStatus::Ongoing);
        run(&test, |ctx| {
            create_team(ctx, 1, REGISTRATION, String::new())
        })
        .unwrap();
        let full = team_of_two(&test, 2, 3);

        let settings = RegistrationSettingsTeam {
            team_limit: 4,
            team_size_min: 2,
            team_size_max: 2,
        };
        let standings = test
            .anonymous_view_context()
            .registration_team_lb(REGISTRATION, &settings);

        let participants: Vec<LbParticipant> = standings.iter().map(|e| e.participant()).collect();
        assert_eq!(participants, vec![LbParticipant::Team(full)]);
    }

    #[test]
    fn players_of_a_match_play_for_the_team_it_got_frozen_with() {
        let test = test_context(&[]);
        registration(&test, RegistrationStatus::Ongoing);
        let team = team_of_two(&test, 1, 2);
        connect(
            &test,
            NodeHandle::RegistrationV1(REGISTRATION),
            NodeHandle::MatchV1(3),
            1,
        );
        run(&test, |ctx| ctx.match_entrants_freeze(3)).unwrap();

        let cx = test.anonymous_view_context();
        assert_eq!(
            cx.match_user_teams(3),
            HashMap::from([(1, team), (2, team)])
        );
        // Both of them get on the server and get moved there.
        let mut players = cx.match_players(3);
        players.sort();
        assert_eq!(players, vec![1, 2]);
    }
}
