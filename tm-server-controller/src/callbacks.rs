use async_fn_traits::AsyncFn1;
use tm_server_types::{
    base::PlayerInfo,
    event::{
        EndRoundEnd, EndRoundStart, PlayerChat, PlayerConnect, PlayerDisconnect, Scores, StartMap,
        StartMatch, StartRound, StartServer, WayPoint,
    },
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

    fn on_start_round_start(
        &self,
        execute: impl for<'a> AsyncFn1<&'a StartRound, OutputFuture: Send, Output = ()>
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

    fn on_end_round_end(
        &self,
        execute: impl for<'a> AsyncFn1<&'a EndRoundEnd, OutputFuture: Send, Output = ()>
        + Send
        + Sync
        + 'static,
    );

    fn on_start_server_start(
        &self,
        execute: impl for<'a> AsyncFn1<&'a StartServer, OutputFuture: Send, Output = ()>
        + Send
        + Sync
        + 'static,
    );

    fn on_start_server_end(
        &self,
        execute: impl for<'a> AsyncFn1<&'a StartServer, OutputFuture: Send, Output = ()>
        + Send
        + Sync
        + 'static,
    );

    fn on_start_match_start(
        &self,
        execute: impl for<'a> AsyncFn1<&'a StartMatch, OutputFuture: Send, Output = ()>
        + Send
        + Sync
        + 'static,
    );

    fn on_start_match_end(
        &self,
        execute: impl for<'a> AsyncFn1<&'a StartMatch, OutputFuture: Send, Output = ()>
        + Send
        + Sync
        + 'static,
    );

    fn on_start_map_start(
        &self,
        execute: impl for<'a> AsyncFn1<&'a StartMap, OutputFuture: Send, Output = ()>
        + Send
        + Sync
        + 'static,
    );

    fn on_start_map_end(
        &self,
        execute: impl for<'a> AsyncFn1<&'a StartMap, OutputFuture: Send, Output = ()>
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

    fn on_start_round_start(
        &self,
        execute: impl for<'a> AsyncFn1<&'a StartRound, OutputFuture: Send, Output = ()>
        + Send
        + Sync
        + 'static,
    ) {
        self.on("ManiaPlanet.StartRound_Start", execute);
    }

    fn on_end_round_start(
        &self,
        execute: impl for<'a> AsyncFn1<&'a EndRoundStart, OutputFuture: Send, Output = ()>
        + Send
        + Sync
        + 'static,
    ) {
        self.on("ManiaPlanet.EndRound_Start", execute);
    }

    fn on_end_round_end(
        &self,
        execute: impl for<'a> AsyncFn1<&'a EndRoundEnd, OutputFuture: Send, Output = ()>
        + Send
        + Sync
        + 'static,
    ) {
        self.on("ManiaPlanet.EndRound_End", execute);
    }

    fn on_start_server_start(
        &self,
        execute: impl for<'a> AsyncFn1<&'a StartServer, OutputFuture: Send, Output = ()>
        + Send
        + Sync
        + 'static,
    ) {
        self.on("Maniaplanet.StartServer_Start", execute);
    }

    fn on_start_server_end(
        &self,
        execute: impl for<'a> AsyncFn1<&'a StartServer, OutputFuture: Send, Output = ()>
        + Send
        + Sync
        + 'static,
    ) {
        self.on("Maniaplanet.StartServer_End", execute);
    }

    fn on_start_match_start(
        &self,
        execute: impl for<'a> AsyncFn1<&'a StartMatch, OutputFuture: Send, Output = ()>
        + Send
        + Sync
        + 'static,
    ) {
        self.on("Maniaplanet.StartMatch_Start", execute);
    }

    fn on_start_match_end(
        &self,
        execute: impl for<'a> AsyncFn1<&'a StartMatch, OutputFuture: Send, Output = ()>
        + Send
        + Sync
        + 'static,
    ) {
        self.on("Maniaplanet.StartMatch_End", execute);
    }

    fn on_start_map_start(
        &self,
        execute: impl for<'a> AsyncFn1<&'a StartMap, OutputFuture: Send, Output = ()>
        + Send
        + Sync
        + 'static,
    ) {
        self.on("Maniaplanet.StartMap_Start", execute);
    }

    fn on_start_map_end(
        &self,
        execute: impl for<'a> AsyncFn1<&'a StartMap, OutputFuture: Send, Output = ()>
        + Send
        + Sync
        + 'static,
    ) {
        self.on("Maniaplanet.StartMap_End", execute);
    }
}
