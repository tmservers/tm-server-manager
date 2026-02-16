use tm_server_controller::{callbacks::TypedCallbacks, method::XmlRpcMethods};
use tm_server_types::event::PlayerChat;

use crate::TRACKMANIA;

pub async fn setup_chat() {
    let server = TRACKMANIA.wait();
    server.on_player_chat(async |message: &PlayerChat| {
        if let Err(error) = server
            .chat_forward_to_account(&message.text, &message.account_id, None)
            .await
        {
            tracing::warn!("{error}")
        }
        tracing::info!("Should be routed");
    })
}
