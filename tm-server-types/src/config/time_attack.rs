#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct TimeAttack {
    pub time_limit: i32,
}

impl TimeAttack {
    pub fn into_xml(&self) -> String {
        format!(
            r#"
        <setting name="S_TimeLimit" value="{}" type="integer"/>
            "#,
            Into::<i32>::into(self.time_limit),
        )
    }
}

impl Default for TimeAttack {
    fn default() -> Self {
        Self { time_limit: 300 }
    }
}
