/* use spacetimedb::{ReducerContext, SpacetimeType, Table, reducer};

use crate::{
    authorization::Authorization,
    worker::jobs::{TmWorkerJobs, tm_worker_jobs},
}; */

/* #[ spacetimedb::table(accessor=tm_monitoring)]
pub struct TmMonitoring {
    #[auto_inc]
    #[primary_key]
    pub id: u32,

    project: u32,
    competition: u32,

    settings: MonitoringSettings,

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
} */

/* #[reducer]
pub fn create_monitor(
    ctx: &ReducerContext,
    competition: u32,
    settings: MonitoringSettings,
) -> Result<(), String> {
    ctx.get_user()?;
    //TODO proper auth.
    let monitor = ctx.db.tm_monitoring().insert(TmMonitoring {
        id: 0,
        //TODO
        project: 0,
        competition,
        settings,
        name: None,
        active: true,
    });

    match monitor.settings {
        MonitoringSettings::Club(monitoring_settings_club) => todo!(),
        MonitoringSettings::Map(monitoring_settings_map) => {
            ctx.db
                .tm_worker_jobs()
                .insert(TmWorkerJobs::new(monitoring_settings_map.map_uid));
        }
    }

    Ok(())
} */
