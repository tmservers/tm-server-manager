mod way_point;
use dxr::{TryFromParams, Value};
pub use way_point::WayPoint;

mod start_line;
pub use start_line::StartLine;

mod respawn;
pub use respawn::Respawn;

mod warm_up;
pub use warm_up::*;

mod give_up;
pub use give_up::GiveUp;

mod scores;
pub use scores::Scores;

mod custom;
pub use custom::Custom;

mod map;
pub use map::*;

mod round;
pub use round::*;

mod turn;
pub use turn::*;

mod podium;
pub use podium::*;

mod play_loop;
pub use play_loop::*;

mod player;
pub use player::*;

mod r#match;
pub use r#match::*;

mod server;
pub use server::*;

/// Can hold every Event trasmitted trough the ModeScript or vanilla events.
#[derive(Debug, Clone)]
#[non_exhaustive]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub enum Event {
    WayPoint(WayPoint),
    Respawn(Respawn),
    StartLine(StartLine),
    Scores(Scores),
    GiveUp(GiveUp),

    LoadingMapStart(LoadingMapStart),
    LoadingMapEnd(LoadingMapEnd),
    StartMapStart(StartMap),
    StartMapEnd(StartMap),
    EndMapStart(EndMapStart),
    EndMapEnd(EndMapEnd),
    UnloadingMapStart(UnloadingMapStart),
    UnloadingMapEnd(UnloadingMapEnd),

    PlayerConenct(PlayerConnect),
    PlayerDisconnect(PlayerDisconnect),
    PlayerChat(PlayerChat),

    StartTurnStart(StartTurn),
    StartTurnEnd(StartTurn),

    PlayLoopStart(PlayLoopStart),
    PlayLoopEnd(PlayLoopEnd),

    EndRoundStart(EndRoundStart),
    EndRoundEnd(EndRoundEnd),

    PodiumStart(Podium),
    PodiumEnd(Podium),

    StartMatchStart(StartMatch),
    StartMatchEnd(StartMatch),
    EndMatchStart(EndMatch),
    EndMatchEnd(EndMatch),

    StartServerStart(StartServer),
    StartServerEnd(StartServer),
    EndServerStart(EndServer),
    EndServerEnd(EndServer),

    Custom(Custom),
}

impl Event {
    pub fn from_modescript(name: &str, body: String) -> Option<Self> {
        let event = match name {
            "Trackmania.Event.WayPoint" => Event::WayPoint(json::from_str(&body).unwrap()),
            "Trackmania.Event.Respawn" => Event::Respawn(json::from_str(&body).unwrap()),
            "Trackmania.Scores" => Event::Scores(json::from_str(&body).unwrap()),
            "Trackmania.Event.StartLine" => Event::StartLine(json::from_str(&body).unwrap()),

            "Maniaplanet.LoadingMap_Start" => {
                Event::LoadingMapStart(json::from_str(&body).unwrap())
            }
            "Maniaplanet.LoadingMap_End" => Event::LoadingMapEnd(json::from_str(&body).unwrap()),
            "Maniaplanet.StartMap_Start" => Event::StartMapStart(json::from_str(&body).unwrap()),
            "Maniaplanet.StartMap_End" => Event::StartMapEnd(json::from_str(&body).unwrap()),
            "Maniaplanet.EndMap_Start" => Event::EndMapStart(json::from_str(&body).unwrap()),
            "Maniaplanet.EndMap_End" => Event::EndMapEnd(json::from_str(&body).unwrap()),
            "Maniaplanet.UnloadingMap_Start" => {
                Event::UnloadingMapStart(json::from_str(&body).unwrap())
            }
            "Maniaplanet.UnloadingMap_End" => {
                Event::UnloadingMapEnd(json::from_str(&body).unwrap())
            }

            "Maniaplanet.StartTurn_Start" => Event::StartTurnStart(json::from_str(&body).unwrap()),
            "Maniaplanet.StartTurn_End" => Event::StartTurnEnd(json::from_str(&body).unwrap()),

            "Maniaplanet.StartPlayLoop" => Event::PlayLoopStart(json::from_str(&body).unwrap()),
            "Maniaplanet.EndPlayLoop" => Event::PlayLoopEnd(json::from_str(&body).unwrap()),

            "Maniaplanet.EndRound_Start" => Event::EndRoundStart(json::from_str(&body).unwrap()),
            "Maniaplanet.EndRound_End" => Event::EndRoundEnd(json::from_str(&body).unwrap()),

            "Maniaplanet.Podium_Start" => Event::PodiumStart(json::from_str(&body).unwrap()),
            "Maniaplanet.Podium_End" => Event::PodiumEnd(json::from_str(&body).unwrap()),

            "Maniaplanet.StartMatch_Start" => {
                Event::StartMatchStart(json::from_str(&body).unwrap())
            }
            "Maniaplanet.StartMatch_End" => Event::StartMatchEnd(json::from_str(&body).unwrap()),
            "Maniaplanet.EndMatch_Start" => Event::EndMatchStart(json::from_str(&body).unwrap()),
            "Maniaplanet.EndMatch_End" => Event::EndMatchEnd(json::from_str(&body).unwrap()),

            "Maniaplanet.StartServer_Start" => {
                Event::StartServerStart(json::from_str(&body).unwrap())
            }
            "Maniaplanet.StartServer_End" => Event::StartServerEnd(json::from_str(&body).unwrap()),
            "Maniaplanet.EndServer_Start" => Event::EndServerStart(json::from_str(&body).unwrap()),
            "Maniaplanet.EndServer_End" => Event::EndServerEnd(json::from_str(&body).unwrap()),

            _ => Event::Custom(Custom::new(name.to_string(), body)),
        };
        Some(event)
    }

    pub fn from_legacy(name: &str, body: Vec<Value>) -> Option<Self> {
        match name {
            "ManiaPlanet.PlayerConnect" => Some(Event::PlayerConenct(
                PlayerConnect::try_from_params(&body).unwrap(),
            )),
            "ManiaPlanet.PlayerDisconnect" => Some(Event::PlayerDisconnect(
                PlayerDisconnect::try_from_params(&body).unwrap(),
            )),
            "ManiaPlanet.PlayerChat" => Some(Event::PlayerChat(
                PlayerChat::try_from_params(&body).unwrap(),
            )),
            _ => None,
        }
    }
}
