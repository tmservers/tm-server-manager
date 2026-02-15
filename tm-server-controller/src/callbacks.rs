use tm_server_types::event::{PlayerConnect, PlayerDisconnect, Scores, WayPoint};

use crate::TrackmaniaServer;

pub trait TypedCallbacks {
    fn on_way_point(&self, execute: impl Fn(&WayPoint) + Send + Sync + 'static);
    fn on_scores(&self, execute: impl Fn(&Scores) + Send + Sync + 'static);
    fn on_player_connect(&self, execute: impl Fn(&PlayerConnect) + Send + Sync + 'static);
    fn on_player_disconnect(&self, execute: impl Fn(&PlayerDisconnect) + Send + Sync + 'static);
}

impl TypedCallbacks for TrackmaniaServer {
    fn on_way_point(&self, execute: impl Fn(&WayPoint) + Send + Sync + 'static) {
        self.on("Trackmania.Event.WayPoint", execute);
    }

    fn on_scores(&self, execute: impl Fn(&Scores) + Send + Sync + 'static) {
        self.on("Trackmania.Scores", execute);
    }

    fn on_player_connect(&self, execute: impl Fn(&PlayerConnect) + Send + Sync + 'static) {
        self.on("ManiaPlanet.PlayerConnect", execute)
    }

    fn on_player_disconnect(&self, execute: impl Fn(&PlayerDisconnect) + Send + Sync + 'static) {
        self.on("ManiaPlanet.PlayerDisconnect", execute);
    }
}
