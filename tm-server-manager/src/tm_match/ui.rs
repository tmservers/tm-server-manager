//! What players do in the manialinks of a match and the values the manialinks show.
//!
//! The bridge parses what a manialink sends with its manifest. Events for the mode stay
//! with the driver, the ones for the manager come here and are kept for whoever
//! subscribes, e.g. a website counting votes. Values for the manialinks are written by
//! the manager, e.g. the ready check, or by a client with the permission to. Keys and
//! players are stored by number, the bridge gets them by name.
//!
//! The ready check: every player of a match in preparation tells they are ready, the
//! match starts once all of them are.

use spacetimedb::{
    CtxDbRead, CtxDbWrite, ReducerContext, SpacetimeType, Table, Uuid, ViewContext, reducer, table,
    view,
};
use tm_server_types::{
    config::{BuiltInManialink, Manialink, ModeSettingsV2, ServerConfigV2, UiRoute, UiValue},
    method::DriverNetUiData,
};

use crate::{
    authorization::Authorization,
    competition::{CompetitionPermissionsV1, manialink::ManialinkRead, node::NodeHandle},
    raw_server::{config::RawServerContigRead, occupation::TabRawServerOccupationRead},
    tm_match::{MatchRead, MatchStatus, MatchV1, MatchWrite, entrant::MatchEntrantRead},
    user::UserRead,
};

/// The names of the keys of the values, every name once.
#[table(accessor = tab_ui_key, public)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiKey {
    #[primary_key]
    #[auto_inc]
    pub id: u32,
    #[unique]
    pub name: String,
}

/// Who a value is for.
#[derive(Debug, Clone, PartialEq, Eq, SpacetimeType)]
pub enum UiDataScope {
    All,
    /// The players of a team of the match.
    Team(u32),
    /// One user, it wins over the value of the team.
    User(u32),
}

/// A value the manialinks of a match read by its key.
#[table(accessor = tab_match_ui_data, public)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchUiData {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(hash)]
    pub match_id: u32,
    pub scope: UiDataScope,
    pub key: u32,
    pub value: String,
}

/// What a player did in a manialink, for the manager. The event is the one of the
/// manifest at this index, its arguments fit it.
#[table(accessor = tab_match_ui_event, public, event)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchUiEvent {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub match_id: u32,
    pub user_id: u32,
    pub manialink: Manialink,
    pub event: u32,
    pub args: Vec<UiValue>,
}

/// The most bytes a value has.
pub const MAX_UI_VALUE: usize = 1024;

/// The server of a match tells what a player did in a manialink of its config whose
/// manifest sends the event to the manager.
#[reducer]
pub(crate) fn match_ui_event(
    ctx: &ReducerContext,
    account_id: String,
    layer: String,
    event: u32,
    args: Vec<UiValue>,
) -> Result<(), String> {
    let server_id = ctx.server_id()?;
    let Some(NodeHandle::MatchV1(match_id)) = ctx.raw_server_occupation(server_id) else {
        return Err("The server plays no match.".into());
    };
    let Some(tm_match) = ctx.match_find(match_id) else {
        return Err("The match of the server does not exist.".into());
    };
    if !shows_manialinks(tm_match.status()) {
        return Err(format!(
            "The match is {:?}, its manialinks send nothing.",
            tm_match.status()
        ));
    }
    let config = ctx.raw_server_config(tm_match.get_active_config_id())?;
    let Some(manialink) = layer_manialink(&config, &layer) else {
        return Err(format!("The config of the match shows no layer {layer}."));
    };
    let Some(manifest) = ctx.manialink_manifest(&manialink) else {
        return Err(format!("There is no manialink {}.", manialink.name()));
    };
    let Some(spec) = manifest.events.get(event as usize) else {
        return Err(format!("{layer} sends no event {event}."));
    };
    if spec.route != UiRoute::Manager {
        return Err(format!("{} of {layer} goes to the mode.", spec.name));
    }
    if !spec.fits(&args) {
        return Err(format!(
            "The arguments do not fit {} of {layer}.",
            spec.name
        ));
    }
    let user_id = Uuid::parse_str(&account_id)
        .ok()
        .and_then(|account| ctx.user_id_of_account(account))
        .ok_or_else(|| format!("There is no user {account_id}."))?;

    // The ready check sends nothing but `ready`.
    if manialink == Manialink::BuiltIn(BuiltInManialink::ReadyCheck) {
        let ready = matches!(args.first(), Some(UiValue::Bool(true)));
        ready_check(ctx, &tm_match, user_id, ready)?;
    }
    ctx.db.tab_match_ui_event().insert(MatchUiEvent {
        id: 0,
        match_id,
        user_id,
        manialink,
        event,
        args,
    });
    Ok(())
}

/// A value for the manialinks of a match, an empty one removes the key.
#[reducer]
pub(crate) fn match_ui_data_set(
    ctx: &ReducerContext,
    match_id: u32,
    scope: UiDataScope,
    key: String,
    value: String,
) -> Result<(), String> {
    let Some(tm_match) = ctx.match_find(match_id) else {
        return Err("Match not found!".into());
    };
    ctx.auth_builder(tm_match.get_comp_id())
        .permission(CompetitionPermissionsV1::MATCH_UI_WRITE)
        .authorize()?;
    if key.is_empty() || key.len() > tm_server_types::config::MAX_UI_NAME {
        return Err(format!(
            "A key has between 1 and {} bytes.",
            tm_server_types::config::MAX_UI_NAME
        ));
    }
    if value.len() > MAX_UI_VALUE {
        return Err(format!("A value has at most {MAX_UI_VALUE} bytes."));
    }
    ctx.match_ui_data_set(match_id, scope, &key, &value);
    Ok(())
}

/// The values for the manialinks of the match of the server, by name, for its bridge.
#[view(accessor = my_match_ui_data, public)]
fn my_match_ui_data(ctx: &ViewContext) -> Vec<DriverNetUiData> {
    let Ok(server_id) = ctx.server_id() else {
        return Vec::new();
    };
    let Some(NodeHandle::MatchV1(match_id)) = ctx.raw_server_occupation(server_id) else {
        return Vec::new();
    };
    ctx.match_ui_data(match_id)
        .into_iter()
        .filter_map(|data| {
            let key = ctx.db.tab_ui_key().id().find(data.key)?.name;
            let (scope, team, account) = match data.scope {
                UiDataScope::All => ("all", 0, String::new()),
                UiDataScope::Team(team) => ("team", team, String::new()),
                UiDataScope::User(user_id) => {
                    ("account", 0, ctx.user_account_from_id(user_id).to_string())
                }
            };
            Some(DriverNetUiData {
                scope: scope.into(),
                team,
                account,
                keys: vec![key],
                values: vec![data.value],
            })
        })
        .collect()
}

/// Manialinks of the driver only show while a server plays the match.
fn shows_manialinks(status: MatchStatus) -> bool {
    matches!(
        status,
        MatchStatus::Preparation
            | MatchStatus::Live
            | MatchStatus::LiveComitted
            | MatchStatus::Recovery
            | MatchStatus::RecoveryPreparation
    )
}

/// The manialink of the layer of this name in a config of the driver.
fn layer_manialink(config: &ServerConfigV2, layer: &str) -> Option<Manialink> {
    let ModeSettingsV2::Driver(driver) = config.get_mode() else {
        return None;
    };
    driver
        .ui
        .layers
        .iter()
        .find(|l| l.manialink.name() == layer)
        .map(|l| l.manialink.clone())
}

/// A player of a match in preparation is ready or not, it starts once all are.
fn ready_check(
    ctx: &ReducerContext,
    tm_match: &MatchV1,
    user_id: u32,
    ready: bool,
) -> Result<(), String> {
    let match_id = tm_match.id;
    if !matches!(tm_match.status(), MatchStatus::Preparation) {
        return Err("The match does not wait for its players anymore.".into());
    }
    let players = ctx.match_players(match_id);
    if !players.contains(&user_id) {
        return Err("Only players of the match tell they are ready.".into());
    }
    let value = if ready { "1" } else { "" };
    ctx.match_ui_data_set(
        match_id,
        UiDataScope::User(user_id),
        BuiltInManialink::READY,
        value,
    );
    if ctx.match_ready_count(match_id, &players) == players.len() {
        ctx.match_try_start(match_id)?;
    }
    Ok(())
}

pub(crate) trait MatchUiRead {
    /// The values of the match.
    fn match_ui_data(&self, match_id: u32) -> Vec<MatchUiData>;
}

impl<Db: CtxDbRead> MatchUiRead for Db {
    fn match_ui_data(&self, match_id: u32) -> Vec<MatchUiData> {
        let mut data: Vec<MatchUiData> = self
            .db_read_only()
            .tab_match_ui_data()
            .match_id()
            .filter(match_id)
            .collect();
        data.sort_by_key(|d| d.id);
        data
    }
}

pub(crate) trait MatchUiWrite: MatchUiRead {
    /// Sets a value, an empty one removes the key.
    fn match_ui_data_set(&self, match_id: u32, scope: UiDataScope, key: &str, value: &str);
    /// How many of the players are ready, and tells everyone.
    fn match_ready_count(&self, match_id: u32, players: &[u32]) -> usize;
    /// A match whose config shows the ready check waits for its players to be ready.
    fn match_ready_check_open(&self, match_id: u32);
    /// The ready check is over: the match started.
    fn match_ready_check_close(&self, match_id: u32);
}

impl<Db: CtxDbWrite> MatchUiWrite for Db {
    fn match_ui_data_set(&self, match_id: u32, scope: UiDataScope, key: &str, value: &str) {
        let key = match self.db().tab_ui_key().name().find(key.to_string()) {
            Some(key) => key.id,
            None => {
                self.db()
                    .tab_ui_key()
                    .insert(UiKey {
                        id: 0,
                        name: key.into(),
                    })
                    .id
            }
        };
        let existing = self
            .match_ui_data(match_id)
            .into_iter()
            .find(|d| d.key == key && d.scope == scope);
        match (existing, value.is_empty()) {
            (Some(data), true) => {
                self.db().tab_match_ui_data().id().delete(data.id);
            }
            (Some(mut data), false) => {
                if data.value != value {
                    data.value = value.into();
                    self.db().tab_match_ui_data().id().update(data);
                }
            }
            (None, true) => {}
            (None, false) => {
                self.db().tab_match_ui_data().insert(MatchUiData {
                    id: 0,
                    match_id,
                    scope,
                    key,
                    value: value.into(),
                });
            }
        }
    }

    fn match_ready_count(&self, match_id: u32, players: &[u32]) -> usize {
        let ready_key = self
            .db()
            .tab_ui_key()
            .name()
            .find(BuiltInManialink::READY.to_string())
            .map(|key| key.id);
        let ready = self
            .match_ui_data(match_id)
            .into_iter()
            .filter(|d| Some(d.key) == ready_key)
            .filter(|d| matches!(d.scope, UiDataScope::User(user) if players.contains(&user)))
            .count();
        self.match_ui_data_set(
            match_id,
            UiDataScope::All,
            BuiltInManialink::READY_COUNT,
            &format!("{ready}/{}", players.len()),
        );
        ready
    }

    fn match_ready_check_open(&self, match_id: u32) {
        let Some(tm_match) = self.match_find(match_id) else {
            return;
        };
        let shows = self
            .raw_server_config(tm_match.get_active_config_id())
            .is_ok_and(|config| {
                layer_manialink(&config, BuiltInManialink::ReadyCheck.name()).is_some()
            });
        if shows {
            let players = self.match_players(match_id);
            self.match_ready_count(match_id, &players);
        }
    }

    fn match_ready_check_close(&self, match_id: u32) {
        let keys: Vec<u32> = [BuiltInManialink::READY, BuiltInManialink::READY_COUNT]
            .into_iter()
            .filter_map(|name| self.db().tab_ui_key().name().find(name.to_string()))
            .map(|key| key.id)
            .collect();
        for data in self.match_ui_data(match_id) {
            if keys.contains(&data.key) {
                self.db().tab_match_ui_data().id().delete(data.id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use tm_server_types::config::{
        BuiltInMode, Driver, DriverMode, DriverUi, UiArg, UiEventSpec, UiLayer, UiManifest,
    };

    use super::*;
    use crate::{test_world::*, tm_match::entrant::match_entrant_insert, user::UserWrite};

    /// A config of the driver which shows these manialinks.
    fn showing(manialinks: &[Manialink]) -> ServerConfigV2 {
        let mut ui = DriverUi::default();
        for manialink in manialinks {
            ui.layers.push(UiLayer::new(manialink.clone()));
        }
        ServerConfigV2::default().with_mode(ModeSettingsV2::Driver(Driver {
            mode: DriverMode::BuiltIn(BuiltInMode::RoundsV1),
            ui,
            ..Default::default()
        }))
    }

    const READY_CHECK: Manialink = Manialink::BuiltIn(BuiltInManialink::ReadyCheck);

    /// A match in preparation on the server with these players, which shows the config.
    fn prepared(world: &World, server: &Server, config: ServerConfigV2, players: &[u32]) -> u32 {
        server.report_modes(world, &["rounds-v1"]);
        let match_id = world.configured_match_with("Final", config);
        world.as_owner(|ctx| {
            for (seed, number) in players.iter().enumerate() {
                let account = Uuid::parse_str(&player(*number)).unwrap();
                let user_id = ctx.user_id_or_insert(account)?;
                match_entrant_insert(ctx, match_id, user_id, seed as u16 + 1);
            }
            Ok(())
        });
        world.as_owner(|ctx| crate::tm_match::match_set_preparation(ctx, match_id));
        match_id
    }

    fn event(
        world: &World,
        server: &Server,
        number: u32,
        layer: &str,
        event: u32,
        args: Vec<UiValue>,
    ) -> Result<(), String> {
        server.call(world, |ctx| {
            match_ui_event(ctx, player(number), layer.into(), event, args)
        })
    }

    fn ready(world: &World, server: &Server, number: u32, ready: bool) -> Result<(), String> {
        event(
            world,
            server,
            number,
            "ready-check",
            0,
            vec![UiValue::Bool(ready)],
        )
    }

    /// What the bridge of the server gets, as (scope, account, key, value).
    fn shown(world: &World, server: &Server) -> Vec<(String, String, String, String)> {
        let mut shown: Vec<_> = my_match_ui_data(&server.view(world))
            .into_iter()
            .map(|d| (d.scope, d.account, d.keys.concat(), d.values.concat()))
            .collect();
        shown.sort();
        shown
    }

    fn value(
        scope: &str,
        account: &str,
        key: &str,
        value: &str,
    ) -> (String, String, String, String) {
        (scope.into(), account.into(), key.into(), value.into())
    }

    #[test]
    fn the_match_starts_once_every_player_is_ready() {
        let world = World::new();
        let server = world.server(1);
        let match_id = prepared(&world, &server, showing(&[READY_CHECK]), &[1, 2]);
        assert_eq!(
            shown(&world, &server),
            [value("all", "", "ready-count", "0/2")]
        );

        ready(&world, &server, 1, true).unwrap();
        assert_eq!(
            shown(&world, &server),
            [
                value("account", &player(1), "ready", "1"),
                value("all", "", "ready-count", "1/2"),
            ]
        );
        // Saying so again counts once, taking it back counts no more.
        ready(&world, &server, 1, true).unwrap();
        assert_eq!(
            shown(&world, &server)[1],
            value("all", "", "ready-count", "1/2")
        );
        ready(&world, &server, 1, false).unwrap();
        assert_eq!(
            shown(&world, &server),
            [value("all", "", "ready-count", "0/2")]
        );

        // Values of others are no readiness: another key of a player, the key for
        // someone who does not play.
        let outsider =
            world.as_owner(|ctx| ctx.user_id_or_insert(Uuid::parse_str(&player(3)).unwrap()));
        let first = world
            .test
            .anonymous_view_context()
            .user_id_of_account(Uuid::parse_str(&player(1)).unwrap())
            .unwrap();
        world.as_owner(|ctx| {
            match_ui_data_set(
                ctx,
                match_id,
                UiDataScope::User(first),
                "seat".into(),
                "1".into(),
            )?;
            match_ui_data_set(
                ctx,
                match_id,
                UiDataScope::User(outsider),
                "ready".into(),
                "1".into(),
            )?;
            match_ui_data_set(
                ctx,
                match_id,
                UiDataScope::All,
                "sponsor".into(),
                "acme".into(),
            )
        });

        ready(&world, &server, 2, true).unwrap();
        assert_eq!(world.match_status(match_id), MatchStatus::Preparation);
        assert!(shown(&world, &server).contains(&value("all", "", "ready-count", "1/2")));
        ready(&world, &server, 1, true).unwrap();
        assert_eq!(world.match_status(match_id), MatchStatus::Live);
        // The check is over, also for a config which shows it during the match. The
        // rest stays.
        assert_eq!(
            shown(&world, &server),
            [
                value("account", &player(1), "seat", "1"),
                value("all", "", "sponsor", "acme"),
            ]
        );
        assert_eq!(
            ready(&world, &server, 1, false).unwrap_err(),
            "The match does not wait for its players anymore."
        );
    }

    #[test]
    fn only_players_tell_they_are_ready() {
        let world = World::new();
        let server = world.server(1);
        prepared(&world, &server, showing(&[READY_CHECK]), &[1, 2]);
        world.as_owner(|ctx| ctx.user_id_or_insert(Uuid::parse_str(&player(3)).unwrap()));

        assert_eq!(
            ready(&world, &server, 3, true).unwrap_err(),
            "Only players of the match tell they are ready."
        );
        assert_eq!(
            ready(&world, &server, 4, true).unwrap_err(),
            format!("There is no user {}.", player(4))
        );
        assert_eq!(
            shown(&world, &server),
            [value("all", "", "ready-count", "0/2")]
        );
    }

    #[test]
    fn a_match_without_the_check_does_not_show_it() {
        let world = World::new();
        let server = world.server(1);
        prepared(&world, &server, showing(&[]), &[1]);
        assert_eq!(shown(&world, &server), []);
        assert_eq!(
            ready(&world, &server, 1, true).unwrap_err(),
            "The config of the match shows no layer ready-check."
        );
    }

    #[test]
    fn events_have_to_fit_the_manifest_of_their_manialink() {
        let world = World::new();
        let server = world.server(1);
        let xml = r#"<manialink version="3"><script><!-- SendCustomEvent("vote", ["1"]); SendCustomEvent("skip", []); --></script></manialink>"#;
        let manifest = UiManifest {
            events: vec![
                UiEventSpec {
                    name: "vote".into(),
                    route: UiRoute::Manager,
                    args: vec![UiArg::Int(tm_server_types::config::UiIntRange {
                        min: 1,
                        max: 5,
                    })],
                },
                UiEventSpec {
                    name: "skip".into(),
                    route: UiRoute::Mode,
                    args: Vec::new(),
                },
            ],
            data_keys: Vec::new(),
        };
        world.as_owner(|ctx| {
            crate::competition::manialink::manialink_create(
                ctx,
                COMPETITION,
                "Vote".into(),
                xml.into(),
                manifest,
            )
        });
        // The first revision of the world.
        let revision = 1;
        assert!(
            world
                .test
                .anonymous_view_context()
                .manialink_manifest(&Manialink::Custom(revision))
                .is_some()
        );
        let layer = Manialink::Custom(revision).name();
        prepared(
            &world,
            &server,
            showing(&[Manialink::Custom(revision), READY_CHECK]),
            &[1],
        );

        assert_eq!(
            event(&world, &server, 1, &layer, 0, vec![UiValue::Int(3)]),
            Ok(())
        );
        let refusals = [
            (0, vec![UiValue::Int(6)], "The arguments do not fit vote of"),
            (
                0,
                vec![UiValue::Bool(true)],
                "The arguments do not fit vote of",
            ),
            (0, vec![], "The arguments do not fit vote of"),
            (1, vec![], "skip of"),
            (2, vec![], "sends no event 2"),
        ];
        for (index, args, refused) in refusals {
            let error = event(&world, &server, 1, &layer, index, args).unwrap_err();
            assert!(error.contains(refused), "{error}");
        }
        // The ready check sends no number.
        let error = event(&world, &server, 1, "ready-check", 0, vec![UiValue::Int(1)]).unwrap_err();
        assert_eq!(error, "The arguments do not fit ready of ready-check.");
        // A server without a match has nothing to tell.
        let idle = world.server(2);
        assert_eq!(
            event(&world, &idle, 1, &layer, 0, vec![UiValue::Int(3)]).unwrap_err(),
            "The server plays no match."
        );
    }

    #[test]
    fn values_are_written_with_their_permission_and_read_by_name() {
        let world = World::new();
        let server = world.server(1);
        let match_id = prepared(&world, &server, showing(&[]), &[1]);
        let user_id = world
            .test
            .anonymous_view_context()
            .user_id_of_account(Uuid::parse_str(&player(1)).unwrap())
            .unwrap();
        let set = |scope: UiDataScope, key: &str, value: &str| {
            world.try_as_owner(|ctx| {
                match_ui_data_set(ctx, match_id, scope, key.into(), value.into())
            })
        };

        set(UiDataScope::All, "sponsor", "acme").unwrap();
        set(UiDataScope::Team(7), "side", "left").unwrap();
        set(UiDataScope::User(user_id), "seat", "3").unwrap();
        set(UiDataScope::All, "sponsor", "other").unwrap();
        let shown = my_match_ui_data(&server.view(&world));
        let shown: Vec<(&str, u32, &str, &str, &str)> = shown
            .iter()
            .map(|d| {
                (
                    d.scope.as_str(),
                    d.team,
                    d.account.as_str(),
                    d.keys[0].as_str(),
                    d.values[0].as_str(),
                )
            })
            .collect();
        let account = player(1);
        assert_eq!(
            shown,
            [
                ("all", 0, "", "sponsor", "other"),
                ("team", 7, "", "side", "left"),
                ("account", 0, account.as_str(), "seat", "3"),
            ]
        );
        // Keys are stored once, by number.
        let keys: Vec<String> =
            world.as_owner(|ctx| Ok(ctx.db.tab_ui_key().iter().map(|key| key.name).collect()));
        assert_eq!(keys.iter().filter(|k| *k == "sponsor").count(), 1);

        set(UiDataScope::All, "sponsor", "").unwrap();
        assert_eq!(my_match_ui_data(&server.view(&world)).len(), 2);

        // A member needs the permission.
        let member = player(9);
        world.try_as_user(&member, |_| Ok(())).unwrap();
        world.as_owner(|ctx| {
            crate::competition::roles::member_add(
                ctx,
                COMPETITION,
                Uuid::parse_str(&member).unwrap(),
            )
        });
        let refused = world.try_as_user(&member, |ctx| {
            match_ui_data_set(ctx, match_id, UiDataScope::All, "k".into(), "v".into())
        });
        assert_eq!(
            refused.unwrap_err(),
            "Not sufficient permissions to perform this action."
        );

        assert_eq!(
            set(UiDataScope::All, "", "x").unwrap_err(),
            "A key has between 1 and 64 bytes."
        );
        assert_eq!(
            set(UiDataScope::All, "k", &"x".repeat(MAX_UI_VALUE + 1)).unwrap_err(),
            "A value has at most 1024 bytes."
        );
        // The server of the match has no permission to write them.
        let refused = server.call(&world, |ctx| {
            match_ui_data_set(ctx, match_id, UiDataScope::All, "k".into(), "v".into())
        });
        assert!(refused.is_err());
        assert_eq!(my_match_ui_data(&server.view(&world)).len(), 2);
    }
}
