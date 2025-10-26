use tm_server_types::base::PlayerInfo;

use crate::{ClientError, TrackmaniaServer};

#[allow(async_fn_in_trait)]
pub trait ModeScriptMethodsXmlRpc {
    async fn enable_callbacks(&self, enable: bool) -> Result<bool, ClientError>;
    async fn get_callbacks_list(&self, enable: bool) -> Result<bool, ClientError>;
    async fn get_callbacks_list_enabled(&self, enable: bool) -> Result<bool, ClientError>;
    async fn get_callbacks_list_disabled(&self, enable: bool) -> Result<bool, ClientError>;
    async fn block_callbacks(&self, enable: bool) -> Result<bool, ClientError>;
    async fn unblock_callbacks(&self, enable: bool) -> Result<bool, ClientError>;
    async fn get_callback_help(&self, enable: bool) -> Result<bool, ClientError>;
    async fn get_methods_list(&self, enable: bool) -> Result<bool, ClientError>;
    async fn get_method_help(&self, enable: bool) -> Result<bool, ClientError>;
    async fn get_doscumentation(&self, enable: bool) -> Result<bool, ClientError>;
    async fn set_api_version(&self, enable: bool) -> Result<bool, ClientError>;
    async fn get_api_version(&self, enable: bool) -> Result<bool, ClientError>;
    async fn get_all_api_versions(&self, enable: bool) -> Result<bool, ClientError>;
    async fn get_player_list(&self) -> Result<Vec<PlayerInfo>, ClientError>;
    async fn ui_modules_get_properties(&self) -> Result<String, ClientError>;
    async fn set_player_points(
        &self,
        login: String,
        match_points: u32,
    ) -> Result<bool, ClientError>;
}

impl ModeScriptMethodsXmlRpc for TrackmaniaServer {
    ///Enable or disable mode script callbacks.
    async fn enable_callbacks(&self, enable: bool) -> Result<bool, ClientError> {
        self.call(
            "TriggerModeScriptEventArray",
            (
                "XmlRpc.EnableCallbacks",
                if enable { ["true"] } else { ["false"] },
            ),
        )
        .await
    }

    async fn get_callbacks_list(&self, enable: bool) -> Result<bool, ClientError> {
        todo!()
    }

    async fn get_callbacks_list_enabled(&self, enable: bool) -> Result<bool, ClientError> {
        todo!()
    }

    async fn get_callbacks_list_disabled(&self, enable: bool) -> Result<bool, ClientError> {
        todo!()
    }

    async fn block_callbacks(&self, enable: bool) -> Result<bool, ClientError> {
        todo!()
    }

    async fn unblock_callbacks(&self, enable: bool) -> Result<bool, ClientError> {
        todo!()
    }

    async fn get_callback_help(&self, enable: bool) -> Result<bool, ClientError> {
        todo!()
    }

    async fn get_methods_list(&self, enable: bool) -> Result<bool, ClientError> {
        todo!()
    }

    async fn get_method_help(&self, enable: bool) -> Result<bool, ClientError> {
        todo!()
    }

    async fn get_doscumentation(&self, enable: bool) -> Result<bool, ClientError> {
        todo!()
    }

    async fn set_api_version(&self, enable: bool) -> Result<bool, ClientError> {
        todo!()
    }

    async fn get_api_version(&self, enable: bool) -> Result<bool, ClientError> {
        todo!()
    }

    async fn get_all_api_versions(&self, enable: bool) -> Result<bool, ClientError> {
        todo!()
    }

    async fn get_player_list(&self) -> Result<Vec<PlayerInfo>, ClientError> {
        self.call("GetPlayerList", (1000, 0)).await
    }

    async fn ui_modules_get_properties(&self) -> Result<String, ClientError> {
        self.call(
            "TriggerModeScriptEventArray",
            ("Common.UIModules.GetProperties", ["isthisaid?"]),
        )
        .await
    }

    async fn set_player_points(
        &self,
        login: String,
        match_points: u32,
    ) -> Result<bool, ClientError> {
        self.call(
            "TriggerModeScriptEventArray",
            (
                "Trackmania.SetPlayerPoints",
                [
                    &login,                    //&lt; Login of the player to update
                    "", //&lt; The round points, use an empty string to not update.
                    "", //&lt; The map points, use an empty string to not update.
                    &match_points.to_string(), //&lt; The match points, use an empty string to not update.
                ],
            ),
        )
        .await
    }
}

#[allow(async_fn_in_trait)]
pub trait XmlRpcMethods {
    async fn kick(&self, player: String, message: Option<String>) -> Result<bool, ClientError>;

    async fn add_guest(&self, player: &str) -> Result<bool, ClientError>;

    async fn auto_save_replays(&self, enable: bool) -> Result<bool, ClientError>;

    async fn is_auto_save_replays_enabled(&self) -> Result<bool, ClientError>;

    async fn save_current_replay(&self, path: &str) -> Result<bool, ClientError>;

    async fn write_file(&self, path: &str, content: Vec<u8>) -> Result<bool, ClientError>;

    async fn load_match_settings(&self, path: &str) -> Result<i32, ClientError>;

    async fn chat_send_server_massage(&self, message: &str) -> Result<bool, ClientError>;

    async fn restart_map(&self) -> Result<bool, ClientError>;

    async fn next_map(&self) -> Result<bool, ClientError>;

    async fn connect_fake_player(&self) -> Result<String, ClientError>;

    async fn send_display_manialink_page(
        &self,
        content: &str,
        timeout: i32,
        hide_on_click: bool,
    ) -> Result<bool, ClientError>;
}

impl XmlRpcMethods for TrackmaniaServer {
    async fn kick(&self, player: String, message: Option<String>) -> Result<bool, ClientError> {
        todo!()
    }

    async fn add_guest(&self, login: &str) -> Result<bool, ClientError> {
        self.call("AddGuest", login).await
    }

    async fn auto_save_replays(&self, enable: bool) -> Result<bool, ClientError> {
        self.call("AutoSaveReplays", enable).await
    }

    async fn is_auto_save_replays_enabled(&self) -> Result<bool, ClientError> {
        self.call("IsAutoSaveReplaysEnabled", ()).await
    }

    async fn save_current_replay(&self, path: &str) -> Result<bool, ClientError> {
        self.call("SaveCurrentReplay", path).await
    }

    async fn write_file(&self, path: &str, content: Vec<u8>) -> Result<bool, ClientError> {
        self.call("WriteFile", (path, content)).await
    }

    async fn load_match_settings(&self, path: &str) -> Result<i32, ClientError> {
        self.call("LoadMatchSettings", path).await
    }

    async fn chat_send_server_massage(&self, message: &str) -> Result<bool, ClientError> {
        self.call("ChatSendServerMessage", message).await
    }

    async fn restart_map(&self) -> Result<bool, ClientError> {
        self.call("RestartMap", ()).await
    }

    async fn next_map(&self) -> Result<bool, ClientError> {
        self.call("NextMap", ()).await
    }

    async fn connect_fake_player(&self) -> Result<String, ClientError> {
        self.call("ConnectFakePlayer", ()).await
    }

    async fn send_display_manialink_page(
        &self,
        content: &str,
        timeout: i32,
        hide_on_click: bool,
    ) -> Result<bool, ClientError> {
        self.call(
            "SendDisplayManialinkPage",
            (content, timeout, hide_on_click),
        )
        .await
    }
}
