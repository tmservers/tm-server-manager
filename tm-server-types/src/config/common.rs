use std::collections::BTreeMap;

use crate::config::{
    LapsNumber,
    helper::{RespawnBehaviour, WarmupDuration, WarmupTimeout},
};

/// The configuration available in every game mode.
/// Only usable parameters included (not shootmania stuff): [Docs](https://wiki.trackmania.io/en/dedicated-server/Usage/OfficialGameModesSettings#s_decoimageurl_checkpoint)
/// Omitted:
/// - Infinite Laps: Reproducible with Force Laps Number
/// - Script Environment: No dev support
/// - Season Ids: Nobody knows what it does
///
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct Common {
    /// Chat time at the end of a map or match
    chat_time: u32,
    respawn_behaviour: RespawnBehaviour,

    /// Minimal time before the server go to the next map in milliseconds.
    delay_before_next_map: u32,

    /// Synchronize players at the launch of the map, to ensure that no one starts late.
    /// Can delay the start by a few seconds.
    synchronize_players_at_map_start: bool,
    /// Synchronize players at the launch of the round, to ensure that no one starts late.
    /// Can delay the start by a few seconds.
    synchronize_players_at_round_start: bool,
    /// No clear official informations about this setting.
    /// It would seem that this tells the server to trust or not trust the network data sent by the client.
    trust_client_simulation: bool,

    /// The car position of other players is extrapolated less precisely, disabling it has a big impact on performance.
    /// This replaces the "S_UseDelayedVisuals" option by removing the delay with ghosts for the modes that need it (There may be a delay in TimeAttack).
    use_crude_extrapolation: bool,

    warmup_duration: WarmupDuration,
    warmup_timeout: WarmupTimeout,
    warmup_number: u32,

    /// Url of the image displayed on the checkpoints ground.
    /// Override the image set in the Club.
    deco_image_url_checkpoint: String,
    /// Url of the image displayed on the block border.
    /// Override the image set in the Club.
    deco_image_url_decal_sponsor_4x1: String,
    /// Url of the image displayed below the podium and big screen.
    /// Override the image set in the Club.
    deco_image_url_screen_16x1: String,
    /// Url of the image displayed on the two big screens.
    /// Override the image set in the Club.
    deco_image_url_screen_16x9: String,
    /// Url of the image displayed on the bleachers.
    /// Override the image set in the Club.
    deco_image_url_screen_8x1: String,
    /// Url of the API route to get the deco image url.
    /// You can replace ":ServerLogin" with a login from a server in another club to use its images.
    deco_image_url_who_am_i_url: String,

    force_laps_number: LapsNumber,
}

impl Common {
    pub fn default_rounds() -> Self {
        Self {
            chat_time: 10,
            respawn_behaviour: RespawnBehaviour::Default,
            delay_before_next_map: 2000,
            synchronize_players_at_map_start: true,
            synchronize_players_at_round_start: true,
            trust_client_simulation: true,
            use_crude_extrapolation: true,
            warmup_duration: WarmupDuration::BasedOnMedal,
            warmup_timeout: WarmupTimeout::BasedOnMedal,
            warmup_number: 0,
            deco_image_url_checkpoint: "".into(),
            deco_image_url_decal_sponsor_4x1: "".into(),
            deco_image_url_screen_16x1: "".into(),
            deco_image_url_screen_16x9: "".into(),
            deco_image_url_screen_8x1: "".into(),
            deco_image_url_who_am_i_url: "".into(),
            force_laps_number: LapsNumber::Validation,
        }
    }

    pub fn into_xml(&self) -> String {
        format!(
            r#"
    	<setting name="S_ChatTime" value="{}" type="integer"/>
    	<setting name="S_RespawnBehaviour" value="{}" type="integer"/>
        <setting name="S_DelayBeforeNextMap" value="{}" type="integer"/>
        <setting name="S_SynchronizePlayersAtMapStart" value="{}" type="boolean"/>
        <setting name="S_SynchronizePlayersAtRoundStart" value="{}" type="boolean"/>
        <setting name="S_TrustClientSimu" value="{}" type="boolean"/>
        <setting name="S_UseCrudeExtrapolation" value="{}" type="boolean"/>
    	<setting name="S_WarmUpDuration" value="{}" type="integer"/>
        <setting name="S_WarmUpTimeout" value="{}" type="integer"/>
    	<setting name="S_WarmUpNb" value="{}" type="integer"/>
        <setting name="S_DecoImageUrl_Checkpoint" value="{}" type="text"/>
        <setting name="S_DecoImageUrl_DecalSponsor4x1" value="{}" type="text"/>
        <setting name="S_DecoImageUrl_Screen16x1" value="{}" type="text"/>
        <setting name="S_DecoImageUrl_Screen16x9" value="{}" type="text"/>
        <setting name="S_DecoImageUrl_Screen8x1" value="{}" type="text"/>
        <setting name="S_DecoImageUrl_WhoAmIUrl" value="{}" type="text"/>
    	<setting name="S_ForceLapsNb" value="{}" type="integer"/>
        "#,
            self.chat_time,
            Into::<i32>::into(self.respawn_behaviour),
            self.delay_before_next_map,
            self.synchronize_players_at_map_start,
            self.synchronize_players_at_round_start,
            self.trust_client_simulation,
            self.use_crude_extrapolation,
            Into::<i32>::into(self.warmup_duration),
            Into::<i32>::into(self.warmup_timeout),
            self.warmup_number,
            self.deco_image_url_checkpoint,
            self.deco_image_url_decal_sponsor_4x1,
            self.deco_image_url_screen_16x1,
            self.deco_image_url_screen_16x9,
            self.deco_image_url_screen_8x1,
            self.deco_image_url_who_am_i_url,
            Into::<i32>::into(self.force_laps_number),
        )
    }

    pub(super) fn get_xml_map(&self) -> BTreeMap<String, dxr::Value> {
        let mut map = BTreeMap::new();

        map.insert(
            "S_ChatTime".to_string(),
            dxr::Value::Integer(self.chat_time as i32),
        );
        map.insert(
            "S_RespawnBehaviour".to_string(),
            dxr::Value::Integer(Into::<i32>::into(self.respawn_behaviour)),
        );
        map.insert(
            "S_DelayBeforeNextMap".to_string(),
            dxr::Value::Integer(self.delay_before_next_map as i32),
        );
        map.insert(
            "S_SynchronizePlayersAtMapStart".to_string(),
            dxr::Value::Boolean(self.synchronize_players_at_map_start),
        );
        map.insert(
            "S_SynchronizePlayersAtRoundStart".to_string(),
            dxr::Value::Boolean(self.synchronize_players_at_round_start),
        );
        map.insert(
            "S_TrustClientSimu".to_string(),
            dxr::Value::Boolean(self.trust_client_simulation),
        );
        map.insert(
            "S_UseCrudeExtrapolation".to_string(),
            dxr::Value::Boolean(self.use_crude_extrapolation),
        );
        map.insert(
            "S_WarmUpDuration".to_string(),
            dxr::Value::Integer(Into::<i32>::into(self.warmup_duration)),
        );
        map.insert(
            "S_WarmUpTimeout".to_string(),
            dxr::Value::Integer(Into::<i32>::into(self.warmup_timeout)),
        );
        map.insert(
            "S_WarmUpNb".to_string(),
            dxr::Value::Integer(self.warmup_number as i32),
        );
        map.insert(
            "S_DecoImageUrl_Checkpoint".to_string(),
            dxr::Value::String(self.deco_image_url_checkpoint.clone()),
        );
        map.insert(
            "S_DecoImageUrl_DecalSponsor4x1".to_string(),
            dxr::Value::String(self.deco_image_url_decal_sponsor_4x1.clone()),
        );
        map.insert(
            "S_DecoImageUrl_Screen16x1".to_string(),
            dxr::Value::String(self.deco_image_url_screen_16x1.clone()),
        );
        map.insert(
            "S_DecoImageUrl_Screen16x9".to_string(),
            dxr::Value::String(self.deco_image_url_screen_16x9.clone()),
        );
        map.insert(
            "S_DecoImageUrl_Screen8x1".to_string(),
            dxr::Value::String(self.deco_image_url_screen_8x1.clone()),
        );
        map.insert(
            "S_DecoImageUrl_WhoAmIUrl".to_string(),
            dxr::Value::String(self.deco_image_url_who_am_i_url.clone()),
        );
        map.insert(
            "S_ForceLapsNb".to_string(),
            dxr::Value::Integer(Into::<i32>::into(self.force_laps_number)),
        );

        map
    }
}
