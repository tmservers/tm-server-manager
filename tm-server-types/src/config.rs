mod rounds;

pub use rounds::Rounds;

mod common;
pub use common::*;

mod options;
pub use options::ServerOptions;

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
		<script_name>Trackmania/TM_Rounds_Online</script_name>
	</gameinfos>

  	<script_settings>"#.to_string()
    	/* <setting name="S_UseTieBreak" value="" type="boolean"/>
    	<setting name="S_WarmUpNb" value="0" type="integer"/>
    	<setting name="S_WarmUpDuration" value="60" type="integer"/>
    	<setting name="S_ChatTime" value="10" type="integer"/>
    	<setting name="S_UseClublinks" value="" type="boolean"/>
    	<setting name="S_UseClublinksSponsors" value="" type="boolean"/>
    	<setting name="S_NeutralEmblemUrl" value="" type="text"/>
    	<setting name="S_ScriptEnvironment" value="production" type="text"/>
    	<setting name="S_IsChannelServer" value="" type="boolean"/>
    	<setting name="S_RespawnBehaviour" value="-1" type="integer"/>
    	<setting name="S_HideOpponents" value="" type="boolean"/>
    	<setting name="S_UseLegacyXmlRpcCallbacks" value="1" type="boolean"/>
    	<setting name="S_UseAlternateRules" value="" type="boolean"/>
    	<setting name="S_ForceLapsNb" value="-1" type="integer"/>
    	<setting name="S_DisplayTimeDiff" value="" type="boolean"/> */
		/* "#
        .to_string() */
        +&self.common.into_xml()
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
}

impl ModeConfig {
    pub fn into_xml(&self) -> String {
        match self {
            ModeConfig::Rounds(rounds) => rounds.into_xml(),
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
