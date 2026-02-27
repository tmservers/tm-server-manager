mod way_point;
use dxr::{Error as DxrError, TryFromParams, TryFromValue, Value};
use json::Error;
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

mod pause;
pub use pause::Pause;

use crate::base::PlayerInfo;

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
    PlayerInfoChanged(PlayerInfo),

    StartTurnStart(StartTurn),
    StartTurnEnd(StartTurn),
    EndTurnStart(EndTurnStart),
    EndTurnEnd(EndTurnEnd),

    PlayLoopStart(PlayLoopStart),
    PlayLoopEnd(PlayLoopEnd),

    StartRoundStart(StartRound),
    StartRoundEnd(StartRound),
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

    WarmupStart,
    WarmupEnd,
    WarmupStartRound(WarmupRound),
    WarmupEndRound(WarmupRound),

    Pause(Pause),

    Custom(Custom),
}

impl Event {
    pub fn from_modescript(
        name: &str,
        mut body: String,
    ) -> Result<Option<Self>, (String, Error, String)> {
        //Safety: Unsafe is only from simd-json but wrapped the entrire function to avoi every branch.
        unsafe {
            let event = match name {
                "Trackmania.Event.WayPoint" => Event::WayPoint(
                    json::from_str(&mut body).map_err(|e| (name.to_string(), e, body))?,
                ),
                "Trackmania.Event.Respawn" => Event::Respawn(
                    json::from_str(&mut body).map_err(|e| (name.to_string(), e, body))?,
                ),
                "Trackmania.Event.GiveUp" => Event::GiveUp(
                    json::from_str(&mut body).map_err(|e| (name.to_string(), e, body))?,
                ),
                "Trackmania.Scores" => Event::Scores(
                    json::from_str(&mut body).map_err(|e| (name.to_string(), e, body))?,
                ),
                "Trackmania.Event.StartLine" => Event::StartLine(
                    json::from_str(&mut body).map_err(|e| (name.to_string(), e, body))?,
                ),

                "Maniaplanet.LoadingMap_Start" => Event::LoadingMapStart(
                    json::from_str(&mut body).map_err(|e| (name.to_string(), e, body))?,
                ),
                "Maniaplanet.LoadingMap_End" => Event::LoadingMapEnd(
                    json::from_str(&mut body).map_err(|e| (name.to_string(), e, body))?,
                ),
                "Maniaplanet.UnloadingMap_Start" => Event::UnloadingMapStart(
                    json::from_str(&mut body).map_err(|e| (name.to_string(), e, body))?,
                ),
                "Maniaplanet.UnloadingMap_End" => Event::UnloadingMapEnd(
                    json::from_str(&mut body).map_err(|e| (name.to_string(), e, body))?,
                ),

                "Maniaplanet.StartMap_Start" => Event::StartMapStart(
                    json::from_str(&mut body).map_err(|e| (name.to_string(), e, body))?,
                ),
                "Maniaplanet.StartMap_End" => Event::StartMapEnd(
                    json::from_str(&mut body).map_err(|e| (name.to_string(), e, body))?,
                ),
                "Maniaplanet.EndMap_Start" => Event::EndMapStart(
                    json::from_str(&mut body).map_err(|e| (name.to_string(), e, body))?,
                ),
                "Maniaplanet.EndMap_End" => Event::EndMapEnd(
                    json::from_str(&mut body).map_err(|e| (name.to_string(), e, body))?,
                ),

                "Maniaplanet.StartTurn_Start" => Event::StartTurnStart(
                    json::from_str(&mut body).map_err(|e| (name.to_string(), e, body))?,
                ),
                "Maniaplanet.StartTurn_End" => Event::StartTurnEnd(
                    json::from_str(&mut body).map_err(|e| (name.to_string(), e, body))?,
                ),
                "Maniaplanet.EndTurn_Start" => Event::EndTurnStart(
                    json::from_str(&mut body).map_err(|e| (name.to_string(), e, body))?,
                ),
                "Maniaplanet.EndTurn_End" => Event::EndTurnEnd(
                    json::from_str(&mut body).map_err(|e| (name.to_string(), e, body))?,
                ),

                "Maniaplanet.StartPlayLoop" => Event::PlayLoopStart(
                    json::from_str(&mut body).map_err(|e| (name.to_string(), e, body))?,
                ),
                "Maniaplanet.EndPlayLoop" => Event::PlayLoopEnd(
                    json::from_str(&mut body).map_err(|e| (name.to_string(), e, body))?,
                ),

                "Maniaplanet.StartRound_Start" => Event::StartRoundStart(
                    json::from_str(&mut body).map_err(|e| (name.to_string(), e, body))?,
                ),
                "Maniaplanet.StartRound_End" => Event::StartRoundEnd(
                    json::from_str(&mut body).map_err(|e| (name.to_string(), e, body))?,
                ),
                "Maniaplanet.EndRound_Start" => Event::EndRoundStart(
                    json::from_str(&mut body).map_err(|e| (name.to_string(), e, body))?,
                ),
                "Maniaplanet.EndRound_End" => Event::EndRoundEnd(
                    json::from_str(&mut body).map_err(|e| (name.to_string(), e, body))?,
                ),

                "Maniaplanet.Podium_Start" => Event::PodiumStart(
                    json::from_str(&mut body).map_err(|e| (name.to_string(), e, body))?,
                ),
                "Maniaplanet.Podium_End" => Event::PodiumEnd(
                    json::from_str(&mut body).map_err(|e| (name.to_string(), e, body))?,
                ),

                "Maniaplanet.StartMatch_Start" => Event::StartMatchStart(
                    json::from_str(&mut body).map_err(|e| (name.to_string(), e, body))?,
                ),
                "Maniaplanet.StartMatch_End" => Event::StartMatchEnd(
                    json::from_str(&mut body).map_err(|e| (name.to_string(), e, body))?,
                ),
                "Maniaplanet.EndMatch_Start" => Event::EndMatchStart(
                    json::from_str(&mut body).map_err(|e| (name.to_string(), e, body))?,
                ),
                "Maniaplanet.EndMatch_End" => Event::EndMatchEnd(
                    json::from_str(&mut body).map_err(|e| (name.to_string(), e, body))?,
                ),

                "Maniaplanet.StartServer_Start" => Event::StartServerStart(
                    json::from_str(&mut body).map_err(|e| (name.to_string(), e, body))?,
                ),
                "Maniaplanet.StartServer_End" => Event::StartServerEnd(
                    json::from_str(&mut body).map_err(|e| (name.to_string(), e, body))?,
                ),
                "Maniaplanet.EndServer_Start" => Event::EndServerStart(
                    json::from_str(&mut body).map_err(|e| (name.to_string(), e, body))?,
                ),
                "Maniaplanet.EndServer_End" => Event::EndServerEnd(
                    json::from_str(&mut body).map_err(|e| (name.to_string(), e, body))?,
                ),

                "Trackmania.WarmUp.Start" => Event::WarmupStart,
                "Trackmania.WarmUp.End" => Event::WarmupEnd,
                "Trackmania.WarmUp.StartRound" => Event::WarmupStartRound(
                    json::from_str(&mut body).map_err(|e| (name.to_string(), e, body))?,
                ),
                "Trackmania.WarmUp.EndRound" => Event::WarmupEndRound(
                    json::from_str(&mut body).map_err(|e| (name.to_string(), e, body))?,
                ),

                "Maniaplanet.Pause.Status" => Event::Pause(
                    json::from_str(&mut body).map_err(|e| (name.to_string(), e, body))?,
                ),

                _ => Event::Custom(Custom::new(name.to_string(), body)),
            };
            Ok(Some(event))
        }
    }

    pub fn from_legacy(name: &str, body: Vec<Value>) -> Result<Option<Self>, DxrError> {
        let event = match name {
            "ManiaPlanet.PlayerConnect" => {
                Some(Event::PlayerConenct(PlayerConnect::try_from_params(&body)?))
            }
            "ManiaPlanet.PlayerDisconnect" => Some(Event::PlayerDisconnect(
                PlayerDisconnect::try_from_params(&body)?,
            )),
            "ManiaPlanet.PlayerInfoChanged" => Some(Event::PlayerInfoChanged(
                PlayerInfo::try_from_value(&body[0])?,
            )),
            "ManiaPlanet.PlayerChat" => {
                Some(Event::PlayerChat(PlayerChat::try_from_params(&body)?))
            }
            _ => None,
        };

        Ok(event)
    }
}
