#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct RoundsBotOnline {
    /* pub finish_timeout: FinishTimeout,
    pub maps_per_match: MapsPerMatch,
    pub points_limit: PointsLimit,
    pub use_custom_points_repartition: bool,
    pub points_repartition: Vec<u32>,
    pub rounds_per_map: RoundsPerMap,
    pub use_tie_breaker: bool, */
}

impl RoundsBotOnline {
    pub fn into_xml(&self) -> String {
        /*  format!(
            r#"
        <setting name="S_PointsLimit" value="{}" type="integer"/>
        <setting name="S_RoundsPerMap" value="{}" type="integer"/>
        <setting name="S_MapsPerMatch" value="{}" type="integer"/>
        <setting name="S_PointsRepartition" value="{}" type="text"/>
        <setting name="S_UseCustomPointsRepartition" value="{}" type="boolean"/>
        <setting name="S_FinishTimeout" value="{}" type="integer"/>
        <setting name="S_UseTieBreak" value="{}" type="boolean"/>
            "#,
            Into::<i32>::into(self.points_limit),
            Into::<i32>::into(self.rounds_per_map),
            Into::<i32>::into(self.maps_per_match),
            points_repartition_format(&self.points_repartition),
            self.use_custom_points_repartition,
            Into::<i32>::into(self.finish_timeout),
            self.use_tie_breaker
        ) */
        String::new()
    }
}
