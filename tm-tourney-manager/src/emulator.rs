use spacetimedb::{AnonymousViewContext, SpacetimeType, ViewContext, view};
use tm_server_types::event::Event;

use crate::tm_match::event::tab_tm_match_event__view;

//TODO probably factor out in own crate for proper support in client applications.
pub struct LeaderboardEmulator {
    current_round: u16,
    warm_up: u16,

    round: Vec<RoundsRound>,
}

impl LeaderboardEmulator {
    pub fn new() -> Self {
        LeaderboardEmulator {
            current_round: 0,
            warm_up: 0,

            round: Vec::new(),
        }
    }

    pub fn insert(&mut self, event: Event) {
        match event {
            Event::WayPoint(way_point) => todo!(),
            Event::Respawn(respawn) => todo!(),
            Event::StartLine(start_line) => todo!(),
            Event::Scores(scores) => todo!(),
            Event::GiveUp(give_up) => todo!(),
            Event::LoadingMapStart(loading_map_start) => todo!(),
            Event::LoadingMapEnd(loading_map_end) => todo!(),
            Event::StartMapStart(start_map) => todo!(),
            Event::StartMapEnd(start_map) => todo!(),
            Event::EndMapStart(end_map_start) => todo!(),
            Event::EndMapEnd(end_map_end) => todo!(),
            Event::UnloadingMapStart(unloading_map_start) => todo!(),
            Event::UnloadingMapEnd(unloading_map_end) => todo!(),
            Event::PlayerConenct(player_connect) => todo!(),
            Event::PlayerDisconnect(player_disconnect) => todo!(),
            Event::PlayerChat(player_chat) => todo!(),
            Event::StartTurnStart(start_turn) => todo!(),
            Event::StartTurnEnd(start_turn) => todo!(),
            Event::PlayLoopStart(play_loop_start) => todo!(),
            Event::PlayLoopEnd(play_loop_end) => todo!(),
            Event::EndRoundStart(end_round_start) => todo!(),
            Event::EndRoundEnd(end_round_end) => todo!(),
            Event::PodiumStart(podium) => todo!(),
            Event::PodiumEnd(podium) => todo!(),
            Event::Custom(custom) => todo!(),
            _ => todo!(),
        }
    }
}

#[derive(Debug)]
#[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
pub struct RoundsRound {
    players: Vec<RoundsRoundPlayer>,
}

#[derive(Debug)]
#[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
pub struct RoundsRoundPlayer {
    id: String,
    ghost: u32,
}
