use async_fn_traits::AsyncFn1;
use tm_server_types::{
    base::PlayerInfo,
    event::{EndRoundStart, PlayerChat, PlayerConnect, PlayerDisconnect, Scores, WayPoint},
};

use crate::TrackmaniaServer;

pub trait TypedCallbacks {
    fn on_way_point(
        &self,
        execute: impl for<'a> AsyncFn1<&'a WayPoint, OutputFuture: Send, Output = ()>
        + Send
        + Sync
        + 'static,
    );
    fn on_scores(
        &self,
        execute: impl for<'a> AsyncFn1<&'a Scores, OutputFuture: Send, Output = ()>
        + Send
        + Sync
        + 'static,
    );
    fn on_player_connect(
        &self,
        execute: impl for<'a> AsyncFn1<&'a PlayerConnect, OutputFuture: Send, Output = ()>
        + Send
        + Sync
        + 'static,
    );
    fn on_player_disconnect(
        &self,
        execute: impl for<'a> AsyncFn1<&'a PlayerDisconnect, OutputFuture: Send, Output = ()>
        + Send
        + Sync
        + 'static,
    );
    fn on_player_info_changed(
        &self,
        execute: impl for<'a> AsyncFn1<&'a PlayerInfo, OutputFuture: Send, Output = ()>
        + Send
        + Sync
        + 'static,
    );
    fn on_player_chat(
        &self,
        execute: impl for<'a> AsyncFn1<&'a PlayerChat, OutputFuture: Send, Output = ()>
        + Send
        + Sync
        + 'static,
    );
    fn on_end_round_start(
        &self,
        execute: impl for<'a> AsyncFn1<&'a EndRoundStart, OutputFuture: Send, Output = ()>
        + Send
        + Sync
        + 'static,
    );
}

impl TypedCallbacks for TrackmaniaServer {
    fn on_way_point(
        &self,
        execute: impl for<'a> AsyncFn1<&'a WayPoint, OutputFuture: Send, Output = ()>
        + Send
        + Sync
        + 'static,
    ) {
        self.on("Trackmania.Event.WayPoint", execute);
    }

    fn on_scores(
        &self,
        execute: impl for<'a> AsyncFn1<&'a Scores, OutputFuture: Send, Output = ()>
        + Send
        + Sync
        + 'static,
    ) {
        self.on("Trackmania.Scores", execute);
    }

    fn on_player_connect(
        &self,
        execute: impl for<'a> AsyncFn1<&'a PlayerConnect, OutputFuture: Send, Output = ()>
        + Send
        + Sync
        + 'static,
    ) {
        self.on("ManiaPlanet.PlayerConnect", execute)
    }

    fn on_player_disconnect(
        &self,
        execute: impl for<'a> AsyncFn1<&'a PlayerDisconnect, OutputFuture: Send, Output = ()>
        + Send
        + Sync
        + 'static,
    ) {
        self.on("ManiaPlanet.PlayerDisconnect", execute);
    }

    fn on_player_info_changed(
        &self,
        execute: impl for<'a> AsyncFn1<&'a PlayerInfo, OutputFuture: Send, Output = ()>
        + Send
        + Sync
        + 'static,
    ) {
        self.on("ManiaPlanet.PlayerInfoChanged", execute);
    }

    fn on_player_chat(
        &self,
        execute: impl for<'a> AsyncFn1<&'a PlayerChat, OutputFuture: Send, Output = ()>
        + Send
        + Sync
        + 'static,
    ) {
        self.on("ManiaPlanet.PlayerChat", execute);
    }
    
    fn on_end_round_start(
        &self,
        execute: impl for<'a> AsyncFn1<&'a EndRoundStart, OutputFuture: Send, Output = ()>
        + Send
        + Sync
        + 'static,
    ) {
        self.on("event", execute);
    }
}
