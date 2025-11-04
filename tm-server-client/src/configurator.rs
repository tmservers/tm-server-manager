use tm_server_types::config::ServerConfig;

use crate::TrackmaniaServer;
use crate::types::XmlRpcMethods;

#[allow(async_fn_in_trait)]
pub trait ServerConfiguration {
    async fn configure(&self, config: ServerConfig);
}

impl ServerConfiguration for TrackmaniaServer {
    async fn configure(&self, config: ServerConfig) {
        let mode_settings = config.get_mode().get_settings();

        let content = r#"<?xml version="1.0" encoding="utf-8" ?>
<playlist>
	<gameinfos>
		<game_mode>0</game_mode>
		<script_name>Trackmania/TM_Rounds_Online</script_name>
	</gameinfos>

  	<script_settings>
    	<setting name="S_UseTieBreak" value="" type="boolean"/>
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
    	<setting name="S_DisplayTimeDiff" value="" type="boolean"/>
		"#
        .to_string()
            + &mode_settings
            + r#"
	</script_settings>

	<startindex>0</startindex>
	<map><file>Campaigns/Training/Training - 09.Map.Gbx</file></map>
    <map><file>Campaigns/Training/Training - 10.Map.Gbx</file></map>
</playlist>"#;
        _ = self
            .write_file("MatchSettings/format.txt", content.to_string())
            .await;
        let loaded = self.load_match_settings("MatchSettings/format.txt").await;

        if loaded.is_ok_and(|l| l == 2) {
            _ = self
                .chat_send_server_massage("Tournament mode successfully loaded!")
                .await;

            _ = self.chat_send_server_massage("Starting... GLHF").await;

            _ = self.restart_map().await;
        }
    }
}
