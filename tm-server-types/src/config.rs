mod rounds;
use std::collections::BTreeMap;

pub use rounds::Rounds;

mod reverse_cup;
pub use reverse_cup::ReverseCup;

mod reverse_cup_v2;
pub use reverse_cup_v2::ReverseCupV2;

mod time_attack;
pub use time_attack::TimeAttack;

mod knockout;
pub use knockout::Knockout;

mod driver;
pub use driver::Driver;

mod rounds_bot_online;
pub use rounds_bot_online::RoundsBotOnline;

mod common;
pub use common::*;

mod options;
pub use options::ServerOptions;

mod helper;
pub use helper::*;

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct ServerConfig {
    // Dedicated Config TODO
    options: ServerOptions,

    // Playlist settings.
    common: Common,
    mode: ModeSettings,
    maps: MapPoolConfig,
}

impl ServerConfig {
    pub fn into_xml(&self) -> String {
        r#"<?xml version="1.0" encoding="utf-8" ?>
<playlist>
	<gameinfos>
		<game_mode>0</game_mode>
		"#
        .to_string()
            + &self.mode.mode_header()
            + r#"
    </gameinfos>

  	<script_settings>"#
            + &self.common.into_xml()
            + &self.mode.into_xml()
            + r#"
	</script_settings>
	"# + &self.maps.into_xml()
            + "
</playlist>"
    }

    pub fn get_common(&self) -> &Common {
        &self.common
    }

    pub fn get_mode(&self) -> &ModeSettings {
        &self.mode
    }

    pub fn get_maps(&self) -> &MapPoolConfig {
        &self.maps
    }

    pub fn script_name(&self) -> &str {
        self.mode.script_name()
    }

    pub fn iter_maps(&self) -> impl Iterator<Item = &String> {
        self.maps.map_uids.iter()
    }

    pub fn get_mode_settings_struct(&self) -> dxr::Value {
        let mut cfg = self.common.get_xml_map();
        cfg.append(&mut self.mode.get_xml_map());
        dxr::Value::Struct(cfg)
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            common: Common::default_rounds(),
            mode: ModeSettings::Rounds(Rounds::default()),
            maps: Default::default(),
            options: Default::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct ServerConfigV2 {
    // Dedicated Config TODO
    options: ServerOptions,

    // Playlist settings.
    common: Common,
    mode: ModeSettingsV2,
    maps: MapPoolConfig,
}

impl ServerConfigV2 {
    pub fn into_xml(&self) -> String {
        r#"<?xml version="1.0" encoding="utf-8" ?>
<playlist>
	<gameinfos>
		<game_mode>0</game_mode>
		"#
        .to_string()
            + &self.mode.mode_header()
            + r#"
    </gameinfos>

  	<script_settings>"#
            + &self.common.into_xml()
            + &self.mode.into_xml()
            + r#"
	</script_settings>
	"# + &self.maps.into_xml()
            + "
</playlist>"
    }

    pub fn get_common(&self) -> &Common {
        &self.common
    }

    pub fn get_mode(&self) -> &ModeSettingsV2 {
        &self.mode
    }

    pub fn get_maps(&self) -> &MapPoolConfig {
        &self.maps
    }

    pub fn set_maps(&mut self, maps: Vec<String>) {
        self.maps.map_uids = maps;
    }

    pub fn script_name(&self) -> &str {
        self.mode.script_name()
    }

    pub fn iter_maps(&self) -> impl Iterator<Item = &String> {
        self.maps.map_uids.iter()
    }

    pub fn get_mode_settings_struct(&self) -> dxr::Value {
        let mut cfg = self.common.get_xml_map();
        cfg.append(&mut self.mode.get_xml_map());
        dxr::Value::Struct(cfg)
    }
}

impl Default for ServerConfigV2 {
    fn default() -> Self {
        Self {
            common: Common::default_rounds(),
            mode: ModeSettingsV2::TimeAttack(TimeAttack::default()),
            maps: Default::default(),
            options: Default::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub enum ModeSettings {
    Rounds(Rounds),
    ReverseCup(ReverseCup),
    TimeAttack(TimeAttack),
    Knockout(Knockout),
    //ReverseCupV2(ReverseCupV2),
}

impl ModeSettings {
    pub fn into_xml(&self) -> String {
        match self {
            ModeSettings::Rounds(rounds) => rounds.into_xml(),
            ModeSettings::ReverseCup(reverse_cup) => reverse_cup.into_xml(),
            ModeSettings::TimeAttack(time_attack) => time_attack.into_xml(),
            ModeSettings::Knockout(knockout) => knockout.into_xml(),
            //ModeSettings::ReverseCupV2(reverse_cup_v2) => reverse_cup_v2.into_xml(),
        }
    }

    pub fn get_xml_map(&self) -> BTreeMap<String, dxr::Value> {
        match self {
            ModeSettings::Rounds(rounds) => rounds.get_xml_map(),
            ModeSettings::ReverseCup(reverse_cup) => reverse_cup.get_xml_map(),
            ModeSettings::TimeAttack(time_attack) => time_attack.get_xml_map(),
            ModeSettings::Knockout(knockout) => knockout.get_xml_map(),
            //ModeSettings::ReverseCupV2(reverse_cup_v2) => reverse_cup_v2.get_xml_map(),
        }
    }

    pub fn mode_header(&self) -> String {
        match self {
            ModeSettings::Rounds(_) => {
                "<script_name>Trackmania/TM_Rounds_Online</script_name>".into()
            }
            ModeSettings::ReverseCup(_) => {
                "<script_name>Modes/Trackmania/ReverseCup</script_name>".into()
            }
            ModeSettings::TimeAttack(_) => {
                "<script_name>Trackmania/TM_TimeAttack_Online</script_name>".into()
            }
            ModeSettings::Knockout(_) => {
                "<script_name>Trackmania/TM_Knockout_Online</script_name>".into()
            } /* ModeSettings::ReverseCupV2(_) => {
                  "<script_name>Modes/Trackmania/ReverseCup</script_name>".into()
              } */
        }
    }

    pub fn script_name(&self) -> &str {
        match self {
            ModeSettings::Rounds(_) => "Trackmania/TM_Rounds_Online",
            ModeSettings::ReverseCup(_) => "Modes/Trackmania/ReverseCup",
            ModeSettings::TimeAttack(_) => "Trackmania/TM_TimeAttack_Online",
            ModeSettings::Knockout(_) => "Trackmania/TM_Kockout_Online",
            //ModeSettings::ReverseCupV2(_) => "Modes/Trackmania/ReverseCup",
        }
    }

    pub fn get_mode(&self) -> TmMode {
        match self {
            ModeSettings::Rounds(_) => TmMode::Rounds,
            ModeSettings::ReverseCup(_) => TmMode::ReverseCup,
            ModeSettings::TimeAttack(_) => TmMode::TimeAttack,
            ModeSettings::Knockout(_) => TmMode::Knockout,
            //ModeSettings::ReverseCupV2(_) => TmMode::ReverseCup,
        }
    }

    /// Returns the mode script of a custom mode.
    pub fn get_external_script(&self) -> Option<&str> {
        match self {
            ModeSettings::Rounds(_) => None,
            ModeSettings::ReverseCup(_) => {
                Some(include_str!("../external_modes/ReverseCup.Script.txt"))
            }
            ModeSettings::TimeAttack(_) => None,
            ModeSettings::Knockout(_) => None,
            /*  ModeSettings::ReverseCupV2(_) => {
                Some(include_str!("../external_modes/ReverseCup.Script.txt"))
            } */
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub enum ModeSettingsV2 {
    Rounds(Rounds),
    ReverseCup(ReverseCupV2),
    TimeAttack(TimeAttack),
    Knockout(Knockout),
    /// Generic modescript controlled by a wasm driver, which the bridge is going to run.
    Driver(Driver),
}

impl ModeSettingsV2 {
    pub fn into_xml(&self) -> String {
        match self {
            ModeSettingsV2::Rounds(rounds) => rounds.into_xml(),
            ModeSettingsV2::ReverseCup(reverse_cup) => reverse_cup.into_xml(),
            ModeSettingsV2::TimeAttack(time_attack) => time_attack.into_xml(),
            ModeSettingsV2::Knockout(knockout) => knockout.into_xml(),
            ModeSettingsV2::Driver(driver) => driver.into_xml(),
        }
    }

    pub fn get_xml_map(&self) -> BTreeMap<String, dxr::Value> {
        match self {
            ModeSettingsV2::Rounds(rounds) => rounds.get_xml_map(),
            ModeSettingsV2::ReverseCup(reverse_cup) => reverse_cup.get_xml_map(),
            ModeSettingsV2::TimeAttack(time_attack) => time_attack.get_xml_map(),
            ModeSettingsV2::Knockout(knockout) => knockout.get_xml_map(),
            ModeSettingsV2::Driver(driver) => driver.get_xml_map(),
        }
    }

    pub fn mode_header(&self) -> String {
        match self {
            ModeSettingsV2::Rounds(_) => {
                "<script_name>Trackmania/TM_Rounds_Online</script_name>".into()
            }
            ModeSettingsV2::ReverseCup(_) => {
                "<script_name>Modes/Trackmania/ReverseCup</script_name>".into()
            }
            ModeSettingsV2::TimeAttack(_) => {
                "<script_name>Trackmania/TM_TimeAttack_Online</script_name>".into()
            }
            ModeSettingsV2::Knockout(_) => {
                "<script_name>Trackmania/TM_Knockout_Online</script_name>".into()
            }
            ModeSettingsV2::Driver(_) => {
                format!("<script_name>{}</script_name>", Driver::SCRIPT_NAME)
            }
        }
    }

    pub fn script_name(&self) -> &str {
        match self {
            Self::Rounds(_) => "Trackmania/TM_Rounds_Online",
            Self::ReverseCup(_) => "Modes/Trackmania/ReverseCup",
            Self::TimeAttack(_) => "Trackmania/TM_TimeAttack_Online",
            Self::Knockout(_) => "Trackmania/TM_Kockout_Online",
            Self::Driver(_) => Driver::SCRIPT_NAME,
        }
    }

    pub fn get_mode(&self) -> TmMode {
        match self {
            Self::Rounds(_) => TmMode::Rounds,
            Self::ReverseCup(_) => TmMode::ReverseCup,
            Self::TimeAttack(_) => TmMode::TimeAttack,
            Self::Knockout(_) => TmMode::Knockout,
            // The manager does not know driver matches yet. Until its database integration
            // it records them like Rounds, which leaves the points at zero.
            Self::Driver(_) => TmMode::Rounds,
            //ModeSettings::ReverseCupV2(_) => TmMode::ReverseCup,
        }
    }

    /// Returns the mode script of a custom mode.
    pub fn get_external_script(&self) -> Option<&str> {
        match self {
            Self::Rounds(_) => None,
            Self::ReverseCup(_) => Some(include_str!("../external_modes/ReverseCup.Script.txt")),
            Self::TimeAttack(_) => None,
            Self::Knockout(_) => None,
            Self::Driver(_) => Some(Driver::SCRIPT),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct MapPoolConfig {
    start: u32,
    map_uids: Vec<String>,
}

impl MapPoolConfig {
    pub fn into_xml(&self) -> String {
        let start = format!(
            r#"
        <startindex>{}</startindex>
        "#,
            self.start
        );
        let mut maps = start;
        for map in &self.map_uids {
            maps += &format!("<map><file>{}.Map.Gbx</file></map>", map);
        }
        maps
    }

    pub fn maps(&self) -> Vec<String> {
        self.map_uids.clone()
    }
}

impl Default for MapPoolConfig {
    /// Playlist with Training01
    fn default() -> Self {
        Self {
            start: 0,
            map_uids: vec!["olsKnq_qAghcVAnEkoeUnVHFZei".into()],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub enum TmMode {
    Rounds,
    ReverseCup,
    Knockout,
    TimeAttack,
    // This is used upstream in the server-manager module so its not super clean separation of types.
    Unknown,
}

fn tm_bool(value: bool) -> &'static str {
    if value { "True" } else { "False" }
}

impl From<ServerConfig> for ServerConfigV2 {
    fn from(value: ServerConfig) -> Self {
        ServerConfigV2 {
            options: value.options,
            common: value.common,
            mode: match value.mode {
                ModeSettings::Rounds(rounds) => ModeSettingsV2::Rounds(rounds),
                ModeSettings::ReverseCup(reverse_cup) => {
                    ModeSettingsV2::ReverseCup(reverse_cup.into())
                }
                ModeSettings::TimeAttack(time_attack) => ModeSettingsV2::TimeAttack(time_attack),
                ModeSettings::Knockout(knockout) => ModeSettingsV2::Knockout(knockout),
            },
            maps: value.maps,
        }
    }
}

impl From<ReverseCup> for ReverseCupV2 {
    fn from(value: ReverseCup) -> Self {
        ReverseCupV2 {
            finish_timeout: value.finish_timeout,
            maps_per_match: value.maps_per_match,
            points_repartition: value.points_repartition,
            complex_points_repartition: String::new(),
            rounds_per_map: value.rounds_per_map,
            number_of_winners: value.number_of_winners,
            starting_points: value.starting_points,
            disable_last_chance: value.disable_last_chance,
            allow_fast_forward_rounds: value.allow_fast_forward_rounds,
            fast_forward_points_repartition: value.fast_forward_points_repartition,
            dnf_points_loss: value.dnf_points_loss,
            last_chance_dnf_mode: value.last_chance_dnf_mode,
            number_of_players: value.number_of_players,
        }
    }
}
