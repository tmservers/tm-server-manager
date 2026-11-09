//! A world for tests which only changes through the reducers and procedures a deployment uses.
//! An owner with a project, servers logging in like the bridge does and matches played through the events of the servers.

use base64::{Engine, prelude::BASE64_URL_SAFE_NO_PAD};
use spacetimedb::{
    ConnectionId, ReducerContext, TimeDuration, Timestamp, Uuid,
    http::{Body, Response},
    test_utils::{TestAuth, TestContext},
};
use tm_server_types::{
    base::{Player, RoundTime},
    config::{ModeSettingsV2, Rounds, ServerConfigV2},
    event::{
        EndRoundEnd, Event, Pause, PlayerConnect, Scores, StartLine, StartMatch, StartRound,
        WayPoint,
    },
};

use crate::{
    competition::node::Vec2,
    pipeline::{LbEntry, LbSettingsV3, context::LbContext, evaluate_steps},
    tm_match::{MatchRead, leaderboard::MatchLeadearboardRead},
};

/// The trackmania account of the owner of the project.
const OWNER: &str = "3467014a-c1cc-4aae-99fe-6beb5eca232a";

/// The root competition of the project of the owner.
pub(crate) const COMPETITION: u32 = 1;

/// The account of a player, the same for the same number.
pub(crate) fn player(number: u32) -> String {
    format!("00000000-0000-4000-8000-{number:012}")
}

pub(crate) struct World {
    pub(crate) test: TestContext,
}

impl World {
    /// The owner connected and created a project.
    pub(crate) fn new() -> Self {
        let world = World {
            test: TestContext::new().expect("the test datastore should initialize"),
        };
        world.as_owner(crate::client_connected);
        world.as_owner(|ctx| {
            crate::project::create_project(
                ctx,
                "Cup".into(),
                String::new(),
                crate::project::ProjectKind::Tournament,
                Timestamp::UNIX_EPOCH,
                Timestamp::UNIX_EPOCH,
            )
        });
        world
    }

    fn owner() -> TestAuth {
        TestAuth::from_jwt_payload(
            format!(
                r#"{{"iss":"https://auth.spacetimedb.com/oidc","sub":"owner","login_method":"trackmania","provider_id":"{OWNER}","preferred_username":"owner"}}"#
            ),
            ConnectionId::from_u128(1),
        )
        .expect("the claims of the owner are valid")
    }

    /// Calls a reducer as the owner, it has to succeed.
    pub(crate) fn as_owner<T>(
        &self,
        reducer: impl FnOnce(&ReducerContext) -> Result<T, String>,
    ) -> T {
        self.test
            .with_reducer_tx(World::owner(), reducer)
            .unwrap_or_else(|error| panic!("the owner could not call the reducer: {error}"))
    }

    /// A server of the owner which logged in, got verified and is lent to the competition.
    pub(crate) fn server(&self, number: u32) -> Server {
        let server = Server {
            login: format!("server-{number}"),
            connection: ConnectionId::from_u128(1000 + number as u128),
            id: 0,
        };
        let id = server.login(self, false).expect("the server should log in");
        let server = Server { id, ..server };
        self.as_owner(|ctx| crate::raw_server::raw_server_verify(ctx, id));
        self.as_owner(|ctx| crate::competition::server_pool::raw_server_add(ctx, id, COMPETITION));
        server
    }

    /// A configured match in the competition, which plays rounds.
    pub(crate) fn configured_match(&self, name: &str) -> u32 {
        self.as_owner(|ctx| {
            crate::tm_match::match_create(ctx, name.into(), COMPETITION, Vec2::ZERO, 0, false)
        });
        let match_id = self
            .test
            .anonymous_view_context()
            .matches_in_competition(COMPETITION)
            .find(|m| m.get_name() == name)
            .expect("the match was created")
            .id;
        self.as_owner(|ctx| crate::tm_match::match_override_config(ctx, match_id, rounds_config()));
        self.as_owner(|ctx| crate::tm_match::match_configured(ctx, match_id));
        match_id
    }

    /// A match which is live on the server it took from the pool, with the players connected to it.
    pub(crate) fn live_match(&self, server: &Server, players: &[&str]) -> u32 {
        let match_id = self.configured_match("Final");
        self.as_owner(|ctx| crate::tm_match::match_set_preparation(ctx, match_id));
        self.as_owner(|ctx| crate::tm_match::match_try_start(ctx, match_id));
        server.post(self, players.iter().map(|player| connect(player)));
        match_id
    }

    /// The rounds of the match as (round, account, points, position), sorted.
    pub(crate) fn rounds(&self, match_id: u32) -> Vec<(u16, String, i32, u16)> {
        let cx = self.test.anonymous_view_context();
        let mut rounds: Vec<_> = cx
            .match_rounds(match_id)
            .iter()
            .map(|e| (e.round, self.account(e.user_id), e.score, e.position))
            .collect();
        rounds.sort();
        rounds
    }

    /// The standings of the match its mode makes from the rounds.
    pub(crate) fn standings(&self, match_id: u32) -> Vec<LbEntry> {
        let cx = self.test.anonymous_view_context();
        let node = crate::competition::node::NodeHandle::MatchV1(match_id);
        evaluate_steps(
            &LbContext::new(&cx, node),
            &[LbSettingsV3::Match],
            cx.match_rounds(match_id),
        )
        .expect("the standings of the match can be evaluated")
    }

    /// The account of a user.
    pub(crate) fn account(&self, user_id: u32) -> String {
        use crate::user::UserRead;
        self.test
            .anonymous_view_context()
            .user_account_from_id(user_id)
            .to_string()
    }
}

/// A server of the owner, which the bridge logged in.
pub(crate) struct Server {
    login: String,
    connection: ConnectionId,
    pub(crate) id: u32,
}

impl Server {
    fn auth(&self) -> TestAuth {
        TestAuth::from_jwt_payload(
            format!(r#"{{"iss":"localhost","sub":"{}"}}"#, self.login),
            self.connection,
        )
        .expect("the claims of a server are valid")
    }

    /// Logs in through the procedure with Nadeo answering with a token for the account of the server.
    /// A seamless login tells that the server kept the match running while the bridge was gone.
    fn login(&self, world: &World, seamless: bool) -> Result<u32, String> {
        let account = Uuid::from_u128(0x5e17e7 + self.connection.to_u128());
        let token = format!(
            "header.{}.signature",
            BASE64_URL_SAFE_NO_PAD.encode(format!(r#"{{"sub":"{account}"}}"#))
        );
        let mut procedure = world
            .test
            .procedure_context_builder(self.auth())
            .http(move |_, _| {
                Ok(Response::<()>::builder()
                    .status(200)
                    .body(Body::from_bytes(format!(r#"{{"accessToken":"{token}"}}"#)))
                    .expect("the response is valid"))
            })
            .build();
        crate::raw_server::login_as_server(
            &mut procedure,
            self.login.clone(),
            "password".into(),
            Uuid::parse_str(OWNER).expect("the account of the owner is valid"),
            seamless,
        )
    }

    /// The bridge of the server lost its connection to the database.
    pub(crate) fn disconnect(&self, world: &World) {
        world.test.clock.advance(TimeDuration::from_micros(1));
        world
            .test
            .with_reducer_tx(self.auth(), |ctx| {
                crate::client_disconnected(ctx);
                Ok::<_, String>(())
            })
            .expect("disconnecting never fails");
    }

    /// The bridge came back, the server kept running in the meantime.
    pub(crate) fn reconnect(&self, world: &World) {
        world.test.clock.advance(TimeDuration::from_micros(1));
        self.login(world, true)
            .expect("the server should log in again");
    }

    /// Posts the events one after another, every one in a transaction of its own like the bridge does.
    pub(crate) fn post(&self, world: &World, events: impl IntoIterator<Item = Event>) {
        for event in events {
            let name = format!("{event:?}");
            world.test.clock.advance(TimeDuration::from_micros(1));
            world
                .test
                .with_reducer_tx(self.auth(), |ctx| {
                    crate::raw_server::event::post_event(ctx, event)
                })
                .unwrap_or_else(|error| panic!("the event {name} was rejected: {error}"));
        }
    }
}

/// A config which plays the rounds mode.
pub(crate) fn rounds_config() -> ServerConfigV2 {
    use json::prelude::*;
    let mut config =
        json::serde::to_owned_value(ServerConfigV2::default()).expect("a config serializes");
    config
        .as_object_mut()
        .expect("a config is an object")
        .insert(
            "mode".into(),
            json::serde::to_owned_value(ModeSettingsV2::Rounds(Rounds::default()))
                .expect("the mode serializes"),
        );
    json::serde::from_owned_value(config).expect("the changed config deserializes")
}

pub(crate) fn connect(account: &str) -> Event {
    Event::PlayerConnect(PlayerConnect {
        account_id: account.into(),
        is_spectator: false,
    })
}

pub(crate) fn start_match() -> Event {
    Event::StartMatchStart(StartMatch { count: 0, time: 0 })
}

pub(crate) fn start_round(count: u32) -> Event {
    Event::StartRoundStart(StartRound { count, time: 0 })
}

pub(crate) fn start_line(account: &str) -> Event {
    Event::StartLine(StartLine {
        account_id: account.into(),
        time: 0,
    })
}

pub(crate) fn end_round() -> Event {
    Event::EndRoundEnd(EndRoundEnd {
        count: 0,
        valid: 0,
        time: 0,
        is_valid: true,
    })
}

pub(crate) fn pause(active: bool) -> Event {
    Event::Pause(Pause {
        available: true,
        active,
    })
}

pub(crate) fn finish(account: &str, racetime: u32) -> Event {
    Event::WayPoint(WayPoint {
        account_id: account.into(),
        time: racetime,
        racetime,
        laptime: racetime,
        speed: 100.,
        checkpoint_in_race: 1,
        checkpoint_in_lap: 1,
        is_end_race: true,
        is_end_lap: true,
        is_infinite_laps: false,
        is_independent_laps: false,
        current_race_checkpoints: vec![racetime],
        current_lap_checkpoints: vec![racetime],
        block_id: String::new(),
    })
}

/// The scores the rounds mode sends before a round ends, with the points every player got.
pub(crate) fn round_scores(results: &[(&str, u32, i32)]) -> Event {
    let player = |&(account, time, points): &(&str, u32, i32)| Player {
        best_race_checkpoints: vec![time],
        previous_race_checkpoints: vec![time],
        best_lap_checkpoints: vec![time],
        account_id: account.into(),
        name: String::new(),
        team: -1,
        rank: 0,
        round_points: points,
        map_points: 0,
        match_points: 0,
        best_racetime: RoundTime::Time(time),
        best_laptime: RoundTime::Time(time),
        previous_racetime: RoundTime::Time(time),
    };
    Event::Scores(Scores {
        response_id: String::new(),
        section: "PreEndRound".into(),
        use_teams: false,
        winner_team: -1,
        winner_player: String::new(),
        teams: Vec::new(),
        players: results.iter().map(player).collect(),
    })
}

/// A whole round like the server sends it: everyone starts, finishes in their time and gets their points.
pub(crate) fn round(count: u32, results: &[(&str, u32, i32)]) -> Vec<Event> {
    let mut events = vec![start_round(count)];
    events.extend(results.iter().map(|(account, _, _)| start_line(account)));
    events.extend(
        results
            .iter()
            .map(|(account, time, _)| finish(account, *time)),
    );
    events.push(round_scores(results));
    events.push(end_round());
    events
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        competition::node::NodeHandle, raw_server::occupation::TabRawServerOccupationRead,
    };

    #[test]
    fn a_match_on_a_server_from_the_pool_scores_the_rounds_its_server_reports() {
        let world = World::new();
        let server = world.server(1);
        let (a, b) = (player(1), player(2));
        let match_id = world.live_match(&server, &[&a, &b]);

        server.post(&world, [start_match()]);
        server.post(&world, round(1, &[(&a, 30_000, 10), (&b, 31_000, 6)]));
        server.post(&world, round(2, &[(&b, 29_000, 10), (&a, 32_000, 4)]));

        let cx = world.test.anonymous_view_context();
        assert_eq!(
            cx.occupation_with_occupier(NodeHandle::MatchV1(match_id)),
            Some(server.id)
        );
        assert_eq!(
            world.rounds(match_id),
            vec![
                (1, a.clone(), 10, 1),
                (1, b.clone(), 6, 2),
                (2, a.clone(), 4, 2),
                (2, b.clone(), 10, 1),
            ]
        );
        let standings: Vec<(String, u16, i32)> = world
            .standings(match_id)
            .iter()
            .map(|e| (world.account(e.user_id), e.position, e.score))
            .collect();
        assert_eq!(standings, vec![(b, 1, 16), (a, 2, 14)]);
    }
}
