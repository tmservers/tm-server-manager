use async_fn_traits::{AsyncFn0, AsyncFn1};
use tm_server_types::{
    base::PlayerInfo,
    event::{
        DriverEvent, DriverHello, DriverMapEnd, DriverMapStart, DriverMatchEnd, DriverMatchStart,
        DriverPlayerJoin, DriverPlayerLeave, DriverRoundEnd, DriverRoundStart, EndRoundEnd,
        EndRoundStart, PlayerChat, PlayerConnect, PlayerDisconnect, Scores, StartMap, StartMatch,
        StartRound, StartServer, WayPoint,
    },
};

use crate::{TrackmaniaServer, Unit};

pub trait TypedCallbacks {
    fn on_way_point(
        &self,
        execute: impl for<'a> AsyncFn1<&'a WayPoint, OutputFuture: Send, Output = ()>
        + Send
        + Sync
        + 'static,
    );

    fn on_begin_match(
        &self,
        execute: impl for<'a> AsyncFn1<&'a Unit, OutputFuture: Send, Output = ()>
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
    // Callbacks of the generic driver modescript.
    // Each hook runs in its own task so there is no ordering between different hooks.
    // Use `on_event` when the order of all driver callbacks matters.
    fn on_driver_hello(
        &self,
        execute: impl for<'a> AsyncFn1<&'a DriverHello, OutputFuture: Send, Output = ()>
        + Send
        + Sync
        + 'static,
    );

    fn on_driver_match_start(
        &self,
        execute: impl for<'a> AsyncFn1<&'a DriverMatchStart, OutputFuture: Send, Output = ()>
        + Send
        + Sync
        + 'static,
    );

    fn on_driver_map_start(
        &self,
        execute: impl for<'a> AsyncFn1<&'a DriverMapStart, OutputFuture: Send, Output = ()>
        + Send
        + Sync
        + 'static,
    );

    fn on_driver_round_start(
        &self,
        execute: impl for<'a> AsyncFn1<&'a DriverRoundStart, OutputFuture: Send, Output = ()>
        + Send
        + Sync
        + 'static,
    );

    fn on_driver_round_end(
        &self,
        execute: impl for<'a> AsyncFn1<&'a DriverRoundEnd, OutputFuture: Send, Output = ()>
        + Send
        + Sync
        + 'static,
    );

    fn on_driver_map_end(
        &self,
        execute: impl for<'a> AsyncFn1<&'a DriverMapEnd, OutputFuture: Send, Output = ()>
        + Send
        + Sync
        + 'static,
    );

    fn on_driver_match_end(
        &self,
        execute: impl for<'a> AsyncFn1<&'a DriverMatchEnd, OutputFuture: Send, Output = ()>
        + Send
        + Sync
        + 'static,
    );

    fn on_driver_player_join(
        &self,
        execute: impl for<'a> AsyncFn1<&'a DriverPlayerJoin, OutputFuture: Send, Output = ()>
        + Send
        + Sync
        + 'static,
    );

    fn on_driver_player_leave(
        &self,
        execute: impl for<'a> AsyncFn1<&'a DriverPlayerLeave, OutputFuture: Send, Output = ()>
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

    fn on_begin_match(
        &self,
        execute: impl for<'a> AsyncFn1<&'a Unit, OutputFuture: Send, Output = ()>
        + Send
        + Sync
        + 'static,
    ) {
        self.on("ManiaPlanet.BeginMatch", execute)
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
        self.on("Maniaplanet.StartRound_Start", execute);
    }

    fn on_end_round_start(
        &self,
        execute: impl for<'a> AsyncFn1<&'a EndRoundStart, OutputFuture: Send, Output = ()>
        + Send
        + Sync
        + 'static,
    ) {
        self.on("Maniaplanet.EndRound_Start", execute);
    }

    fn on_end_round_end(
        &self,
        execute: impl for<'a> AsyncFn1<&'a EndRoundEnd, OutputFuture: Send, Output = ()>
        + Send
        + Sync
        + 'static,
    ) {
        self.on("Maniaplanet.EndRound_End", execute);
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

    fn on_driver_hello(
        &self,
        execute: impl for<'a> AsyncFn1<&'a DriverHello, OutputFuture: Send, Output = ()>
        + Send
        + Sync
        + 'static,
    ) {
        self.on(DriverEvent::HELLO, execute);
    }

    fn on_driver_match_start(
        &self,
        execute: impl for<'a> AsyncFn1<&'a DriverMatchStart, OutputFuture: Send, Output = ()>
        + Send
        + Sync
        + 'static,
    ) {
        self.on(DriverEvent::MATCH_START, execute);
    }

    fn on_driver_map_start(
        &self,
        execute: impl for<'a> AsyncFn1<&'a DriverMapStart, OutputFuture: Send, Output = ()>
        + Send
        + Sync
        + 'static,
    ) {
        self.on(DriverEvent::MAP_START, execute);
    }

    fn on_driver_round_start(
        &self,
        execute: impl for<'a> AsyncFn1<&'a DriverRoundStart, OutputFuture: Send, Output = ()>
        + Send
        + Sync
        + 'static,
    ) {
        self.on(DriverEvent::ROUND_START, execute);
    }

    fn on_driver_round_end(
        &self,
        execute: impl for<'a> AsyncFn1<&'a DriverRoundEnd, OutputFuture: Send, Output = ()>
        + Send
        + Sync
        + 'static,
    ) {
        self.on(DriverEvent::ROUND_END, execute);
    }

    fn on_driver_map_end(
        &self,
        execute: impl for<'a> AsyncFn1<&'a DriverMapEnd, OutputFuture: Send, Output = ()>
        + Send
        + Sync
        + 'static,
    ) {
        self.on(DriverEvent::MAP_END, execute);
    }

    fn on_driver_match_end(
        &self,
        execute: impl for<'a> AsyncFn1<&'a DriverMatchEnd, OutputFuture: Send, Output = ()>
        + Send
        + Sync
        + 'static,
    ) {
        self.on(DriverEvent::MATCH_END, execute);
    }

    fn on_driver_player_join(
        &self,
        execute: impl for<'a> AsyncFn1<&'a DriverPlayerJoin, OutputFuture: Send, Output = ()>
        + Send
        + Sync
        + 'static,
    ) {
        self.on(DriverEvent::PLAYER_JOIN, execute);
    }

    fn on_driver_player_leave(
        &self,
        execute: impl for<'a> AsyncFn1<&'a DriverPlayerLeave, OutputFuture: Send, Output = ()>
        + Send
        + Sync
        + 'static,
    ) {
        self.on(DriverEvent::PLAYER_LEAVE, execute);
    }
}
