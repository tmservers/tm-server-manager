mod rounds;
pub use rounds::Rounds;

mod reverse_cup;
pub use reverse_cup::ReverseCup;

mod time_attack;
pub use time_attack::TimeAttack;

mod rounds_bot_online;
pub use rounds_bot_online::RoundsBotOnline;

mod common;
pub use common::*;

mod options;
pub use options::ServerOptions;

mod helper;
pub use helper::*;

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct ServerConfig {
    // Dedicated Config TODO
    options: ServerOptions,

    // Playlist settings.
    common: Common,
    mode: ModeConfig,
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
            + r#"</gameinfos>

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

    pub fn get_mode(&self) -> &ModeConfig {
        &self.mode
    }

    pub fn get_maps(&self) -> &MapPoolConfig {
        &self.maps
    }

    pub fn iter_maps(&self) -> impl Iterator<Item = &String> {
        self.maps.map_uids.iter()
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            common: Common::default_rounds(),
            mode: ModeConfig::Rounds(Rounds::default()),
            maps: Default::default(),
            options: Default::default(),
        }
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub enum ModeConfig {
    Rounds(Rounds),
    ReverseCup(ReverseCup),
    TimeAttack(TimeAttack),
    //RoundsBotOnline(RoundsBotOnline),
}

impl ModeConfig {
    pub fn into_xml(&self) -> String {
        match self {
            ModeConfig::Rounds(rounds) => rounds.into_xml(),
            ModeConfig::ReverseCup(reverse_cup) => reverse_cup.into_xml(),
            ModeConfig::TimeAttack(time_attack) => time_attack.into_xml(),
            //ModeConfig::RoundsBotOnline(rounds_bot) => rounds_bot.into_xml(),
        }
    }

    pub fn mode_header(&self) -> String {
        match self {
            ModeConfig::Rounds(_) => {
                "<script_name>Trackmania/TM_Rounds_Online</script_name>".into()
            }
            ModeConfig::ReverseCup(_) => {
                "<script_name>Modes/Trackmania/ReverseCup</script_name>".into()
            }
            ModeConfig::TimeAttack(_) => {
                "<script_name>Trackmania/TM_TimeAttack_Online</script_name>".into()
            } /* ModeConfig::RoundsBotOnline(_) => {
                  "<script_name>Trackmania/TM_RoundsBot_Online</script_name>".into()
              } */
        }
    }
}

#[derive(Debug, Clone)]
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
