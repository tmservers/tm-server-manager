use std::collections::HashSet;

use spacetimedb::{CtxDbRead, ReducerContext, Table, ViewContext, reducer, table, view};

use crate::{
    authorization::Authorization,
    competition::{
        CompetitionPermissionsV1,
        node::{NodeHandle, NodeRead},
    },
    pipeline::{
        LbEntry, LbParticipant, context::LbContext, error::LbError, modes::match_standings,
    },
    tm_match::{MatchRead, MatchStatus},
};

/// A participant of a match with its seed.
/// The entrants get frozen when the match gets prepared, so they stay the same while it is played.
#[table(accessor = tab_match_entrant, vis_private)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MatchEntrant {
    #[index(hash)]
    match_id: u32,
    /// 0 for the entrant of a team.
    pub user_id: u32,
    pub team_id: u32,
    /// Starting at 1.
    pub seed: u16,
}

impl MatchEntrant {
    fn new(match_id: u32, participant: LbParticipant, seed: u16) -> Self {
        let (user_id, team_id) = match participant {
            LbParticipant::Player(user_id) => (user_id, 0),
            LbParticipant::Team(team_id) => (0, team_id),
        };
        MatchEntrant {
            match_id,
            user_id,
            team_id,
            seed,
        }
    }
}

/// The participants of the inputs, which come in the order of their slots.
/// Every input keeps its own order and someone in several inputs keeps the best seed.
fn entrant_order(inputs: Vec<Vec<LbEntry>>) -> Vec<LbParticipant> {
    let mut seen = HashSet::new();
    inputs
        .into_iter()
        .flatten()
        .map(|e| e.participant())
        .filter(|participant| seen.insert(*participant))
        .collect()
}

/// The entrants in a new order, `order` lists the current seeds starting with the one which becomes the first.
fn reseeded(entrants: &[MatchEntrant], order: &[u16]) -> Result<Vec<MatchEntrant>, String> {
    if order.len() != entrants.len() {
        return Err(format!(
            "Expected all {} seeds in the new order but got {}.",
            entrants.len(),
            order.len()
        ));
    }

    let mut given = HashSet::new();
    order
        .iter()
        .zip(1u16..)
        .map(|(&current, seed)| {
            let Some(found) = entrants.iter().find(|e| e.seed == current) else {
                return Err(format!("Seed {current} does not exist."));
            };
            if !given.insert(current) {
                return Err(format!("Seed {current} is given more than once."));
            }
            Ok(MatchEntrant { seed, ..*found })
        })
        .collect()
}

pub(crate) trait MatchEntrantRead {
    /// The entrants the match would get from its inputs right now.
    fn match_entrants_preview(&self, match_id: u32) -> Result<Vec<MatchEntrant>, LbError>;
    /// The frozen entrants of the match, the best seed first.
    fn match_entrants(&self, match_id: u32) -> Vec<MatchEntrant>;
}
impl<Db: CtxDbRead> MatchEntrantRead for Db {
    fn match_entrants_preview(&self, match_id: u32) -> Result<Vec<MatchEntrant>, LbError> {
        let node = NodeHandle::MatchV1(match_id);
        let cx = LbContext::new(self, node);
        let inputs = self
            .node_resolve_inputs(node)?
            .into_iter()
            .map(|input| match_standings(&cx, input))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(entrant_order(inputs)
            .into_iter()
            .zip(1u16..)
            .map(|(participant, seed)| MatchEntrant::new(match_id, participant, seed))
            .collect())
    }

    fn match_entrants(&self, match_id: u32) -> Vec<MatchEntrant> {
        let mut entrants: Vec<MatchEntrant> = self
            .db_read_only()
            .tab_match_entrant()
            .match_id()
            .filter(match_id)
            .collect();
        entrants.sort_by_key(|e| e.seed);
        entrants
    }
}

pub(crate) trait MatchEntrantWrite: MatchEntrantRead {
    /// Freezes the entrants the match gets from its inputs, entrants which already got frozen are kept.
    fn match_entrants_freeze(&self, match_id: u32) -> Result<(), String>;
    fn match_entrants_delete(&self, match_id: u32);
    /// Reorders the frozen entrants while the match gets prepared, see [`match_reseed`].
    fn match_entrants_reseed(&self, match_id: u32, order: &[u16]) -> Result<(), String>;
}
impl<Db: spacetimedb::CtxDbWrite> MatchEntrantWrite for Db {
    fn match_entrants_freeze(&self, match_id: u32) -> Result<(), String> {
        if !self.match_entrants(match_id).is_empty() {
            return Ok(());
        }
        for entrant in self.match_entrants_preview(match_id)? {
            self.db().tab_match_entrant().try_insert(entrant)?;
        }
        Ok(())
    }

    fn match_entrants_delete(&self, match_id: u32) {
        self.db().tab_match_entrant().match_id().delete(match_id);
    }

    fn match_entrants_reseed(&self, match_id: u32, order: &[u16]) -> Result<(), String> {
        let Some(tm_match) = self.match_find(match_id) else {
            return Err("Match not found.".into());
        };
        if tm_match.status() != MatchStatus::Preparation {
            return Err("Seeds can only be changed while the match gets prepared.".into());
        }

        let entrants = reseeded(&self.match_entrants(match_id), order)?;
        self.match_entrants_delete(match_id);
        for entrant in entrants {
            self.db().tab_match_entrant().try_insert(entrant)?;
        }
        Ok(())
    }
}

/// Reorders the frozen entrants of a match while it gets prepared.
/// `order` lists the current seeds starting with the one which becomes the first.
#[reducer]
fn match_reseed(ctx: &ReducerContext, match_id: u32, order: Vec<u16>) -> Result<(), String> {
    let Some(tm_match) = ctx.match_find(match_id) else {
        return Err("Match not found.".into());
    };
    ctx.auth_builder(tm_match.get_comp_id())
        .permission(CompetitionPermissionsV1::MATCH_RESEED)
        .authorize()?;

    ctx.match_entrants_reseed(match_id, &order)
}

fn is_member(ctx: &ViewContext, match_id: u32) -> bool {
    ctx.node_get_parent(NodeHandle::MatchV1(match_id))
        .is_ok_and(|parent| ctx.auth_builder(parent).authorize().is_ok())
}

/// The frozen entrants of the match.
#[view(accessor = match_entrants, public)]
fn match_entrants(ctx: &ViewContext, match_id: u32) -> Vec<MatchEntrant> {
    if !is_member(ctx, match_id) {
        return Vec::new();
    }
    ctx.match_entrants(match_id)
}

/// The entrants the match would get from its inputs right now.
#[view(accessor = match_entrants_preview, public)]
fn match_entrants_preview(ctx: &ViewContext, match_id: u32) -> Vec<MatchEntrant> {
    if !is_member(ctx, match_id) {
        return Vec::new();
    }
    ctx.match_entrants_preview(match_id)
        .unwrap_or_else(|error| {
            log::error!("The entrants of match {match_id} could not be resolved: {error}");
            Vec::new()
        })
}

#[cfg(test)]
pub(crate) mod test_util {
    use spacetimedb::{
        Table, Uuid,
        test_utils::{TestAuth, TestContext},
    };

    use crate::{
        competition::{connection::data_connection_for_test, node::NodeHandle},
        tm_match::{
            MatchStatus, MatchV1,
            leaderboard::{MatchLeaderboardWrite, MatchRoundPlayer},
            tab_match,
        },
        user::{UserV1, UserWrite},
    };

    /// Inserts a match which gets prepared, its inputs can be connected afterwards.
    pub(crate) fn prepared_match(test: &TestContext, match_id: u32) {
        test.with_reducer_tx(TestAuth::internal(), |ctx| {
            ctx.db.tab_match().insert(MatchV1 {
                name: String::new(),
                id: match_id,
                parent_id: 1,
                pre_config: 0,
                config: 0,
                status: MatchStatus::Preparation,
                auto_provision_server: false,
                open: false,
                template: false,
            });
            Ok::<_, String>(())
        })
        .unwrap();
    }

    /// The user plays a round of the match and scores the points.
    pub(crate) fn play(test: &TestContext, match_id: u32, user_id: u32, points: i32) {
        test.with_reducer_tx(TestAuth::internal(), |ctx| {
            ctx.match_round_player_insert(MatchRoundPlayer::for_test(
                match_id, user_id, 1, points,
            ))?;
            Ok::<_, String>(())
        })
        .unwrap();
    }

    pub(crate) fn connect(test: &TestContext, origin: NodeHandle, target: NodeHandle, slot: u16) {
        test.with_reducer_tx(TestAuth::internal(), |ctx| {
            data_connection_for_test(ctx, origin, target, slot)
        })
        .unwrap();
    }

    /// Inserts a user with the account, returns the id of the user.
    pub(crate) fn user(test: &TestContext, account: u32) -> u32 {
        let account_id =
            Uuid::parse_str(&format!("00000000-0000-0000-0000-{account:012}")).unwrap();
        test.with_reducer_tx(TestAuth::internal(), |ctx| {
            ctx.user_insert(UserV1::new(account_id))
        })
        .unwrap()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use spacetimedb::test_utils::TestAuth;
    use tm_server_types::config::TmMode;

    use super::{
        test_util::{connect, play, prepared_match, user},
        *,
    };
    use crate::{
        pipeline::test_util::{standing, test_context},
        tm_match::tab_match,
        user::UserRead,
    };

    fn players(participants: &[LbParticipant]) -> Vec<u32> {
        participants
            .iter()
            .map(|participant| match participant {
                LbParticipant::Player(user_id) => *user_id,
                LbParticipant::Team(_) => panic!("only players enter here"),
            })
            .collect()
    }

    fn users(entrants: &[MatchEntrant]) -> Vec<u32> {
        entrants.iter().map(|e| e.user_id).collect()
    }

    #[test]
    fn entrants_keep_the_order_of_their_inputs_and_their_best_seed() {
        let inputs = vec![
            vec![standing(1, 1), standing(2, 2)],
            vec![standing(3, 1), standing(1, 2)],
        ];

        assert_eq!(players(&entrant_order(inputs)), vec![1, 2, 3]);
    }

    #[test]
    fn reseeding_needs_every_seed_exactly_once() {
        let entrants: Vec<MatchEntrant> = (1u16..=3)
            .map(|seed| MatchEntrant::new(1, LbParticipant::Player(seed as u32 * 10), seed))
            .collect();

        let reseeded_users: Vec<(u32, u16)> = reseeded(&entrants, &[2, 1, 3])
            .unwrap()
            .iter()
            .map(|e| (e.user_id, e.seed))
            .collect();
        assert_eq!(reseeded_users, vec![(20, 1), (10, 2), (30, 3)]);

        assert!(reseeded(&entrants, &[1, 1, 3]).is_err());
        assert!(reseeded(&entrants, &[1, 2]).is_err());
        assert!(reseeded(&entrants, &[1, 2, 4]).is_err());
    }

    #[test]
    fn frozen_entrants_follow_the_slots_and_ignore_later_results() {
        let test = test_context(&[(1, TmMode::Rounds), (2, TmMode::Rounds)]);
        let target = NodeHandle::MatchV1(3);
        play(&test, 1, 10, 5);
        play(&test, 1, 11, 9);
        play(&test, 2, 20, 1);
        connect(&test, NodeHandle::MatchV1(1), target, 2);
        connect(&test, NodeHandle::MatchV1(2), target, 1);
        test.with_reducer_tx(TestAuth::internal(), |ctx| ctx.match_entrants_freeze(3))
            .unwrap();

        play(&test, 1, 12, 20);

        let cx = test.anonymous_view_context();
        assert_eq!(users(&cx.match_entrants(3)), vec![20, 11, 10]);
        assert_eq!(
            users(&cx.match_entrants_preview(3).unwrap()),
            vec![20, 12, 11, 10]
        );
    }

    #[test]
    fn the_players_permitted_on_the_server_are_the_frozen_entrants() {
        let test = test_context(&[(1, TmMode::Rounds)]);
        let (first, second, late) = (user(&test, 1), user(&test, 2), user(&test, 3));
        prepared_match(&test, 3);
        play(&test, 1, first, 5);
        play(&test, 1, second, 9);
        connect(&test, NodeHandle::MatchV1(1), NodeHandle::MatchV1(3), 0);
        test.with_reducer_tx(TestAuth::internal(), |ctx| ctx.match_entrants_freeze(3))
            .unwrap();

        // Results after the freeze do not let anyone else in.
        play(&test, 1, late, 20);

        let cx = test.anonymous_view_context();
        let permitted: HashSet<_> = cx
            .node_permitted_players_input(NodeHandle::MatchV1(3))
            .into_iter()
            .map(|p| p.account_id)
            .collect();
        let entrants: HashSet<_> = [first, second]
            .into_iter()
            .map(|user_id| cx.user_account_from_id(user_id))
            .collect();
        assert_eq!(permitted, entrants);
    }

    #[test]
    fn seeds_can_only_change_while_the_match_gets_prepared() {
        let test = test_context(&[(1, TmMode::Rounds)]);
        prepared_match(&test, 3);
        play(&test, 1, 10, 5);
        play(&test, 1, 11, 9);
        connect(&test, NodeHandle::MatchV1(1), NodeHandle::MatchV1(3), 0);
        let reseed = |order: &[u16]| {
            test.with_reducer_tx(TestAuth::internal(), |ctx| {
                ctx.match_entrants_reseed(3, order)
            })
        };
        test.with_reducer_tx(TestAuth::internal(), |ctx| ctx.match_entrants_freeze(3))
            .unwrap();

        reseed(&[2, 1]).unwrap();
        assert_eq!(
            users(&test.anonymous_view_context().match_entrants(3)),
            vec![10, 11]
        );

        test.with_reducer_tx(TestAuth::internal(), |ctx| {
            let mut tm_match = ctx.db.tab_match().id().find(3).unwrap();
            tm_match.status = MatchStatus::Live;
            ctx.db.tab_match().id().update(tm_match);
            Ok::<_, String>(())
        })
        .unwrap();

        assert!(reseed(&[2, 1]).is_err());
        assert_eq!(
            users(&test.anonymous_view_context().match_entrants(3)),
            vec![10, 11]
        );
    }
}
