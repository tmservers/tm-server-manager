use spacetimedb::{ReducerContext, SpacetimeType, reducer};

use crate::auth::Authorization;

#[ spacetimedb::table(name=tm_monitoring)]
pub struct TmMonitoring {
    #[auto_inc]
    #[primary_key]
    pub id: u32,

    tournament: u32,
    competition: u32,

    monitor: MonitoringSettings,

    name: Option<String>,

    active: bool,
}

#[derive(Debug, SpacetimeType)]
pub enum MonitoringSettings {
    Club(MonitoringSettingsClub),
    Map(MonitoringSettingsMap),
}

#[derive(Debug, SpacetimeType)]
pub struct MonitoringSettingsClub {
    club_id: String,
    // filter:
}

#[derive(Debug, SpacetimeType)]
pub struct MonitoringSettingsMap {
    map_uid: String,
    // filter:
}

#[reducer]
pub fn create_monitor(
    ctx: &ReducerContext,
    competition: u32,
    settings: MonitoringSettings,
) -> Result<(), String> {
    ctx.auth_user()?;
    Ok(())
}
