use tm_server_types::{
    base::PlayerInfo,
    event::{Scores, WayPoint},
};

use crate::{ClientError, TrackmaniaServer};

pub use tm_server_types::*;

pub trait ModeScriptCallbacks {
    fn on_way_point(&self, execute: impl Fn(&WayPoint) + Send + Sync + 'static);
    fn on_scores(&self, execute: impl Fn(&Scores) + Send + Sync + 'static);
}

impl ModeScriptCallbacks for TrackmaniaServer {
    fn on_way_point(&self, execute: impl Fn(&WayPoint) + Send + Sync + 'static) {
        self.on("Trackmania.Event.WayPoint", execute);
    }

    fn on_scores(&self, execute: impl Fn(&Scores) + Send + Sync + 'static) {
        self.on("Trackmania.Scores", execute);
    }
}
