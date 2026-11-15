//! The modes a driver match plays.
//!
//! A built-in mode never changes its rules once released. A change of rules is a new
//! version, which is a mode of its own: `RoundsV2` next to `RoundsV1`. Matches keep the
//! version they were configured with, only new configs have to use the latest one.

/// The mode of a driver match. Modes written by competitions come as another variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub enum DriverMode {
    BuiltIn(BuiltInMode),
}

macro_rules! built_in_modes {
    ($($variant:ident => $family:literal v $version:literal,)*) => {
        /// A version of a mode built into the driver.
        ///
        /// Variants are only ever appended, the database stores them.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        #[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
        #[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
        pub enum BuiltInMode {
            $($variant,)*
        }

        impl BuiltInMode {
            /// Every built-in mode, superseded ones included.
            pub const ALL: &[BuiltInMode] = &[$(BuiltInMode::$variant,)*];

            /// The mode the versions belong to, e.g. `rounds`.
            pub const fn family(self) -> &'static str {
                match self {
                    $(BuiltInMode::$variant => $family,)*
                }
            }

            pub const fn version(self) -> u16 {
                match self {
                    $(BuiltInMode::$variant => $version,)*
                }
            }
        }
    };
}

built_in_modes! {
    RoundsV1 => "rounds" v 1,
    TimeAttackV1 => "time-attack" v 1,
    ReverseCupV1 => "reverse-cup" v 1,
    KnockoutV1 => "knockout" v 1,
    EnduranceV1 => "endurance" v 1,
    Tmwt2v2V1 => "tmwt-2v2" v 1,
}

impl BuiltInMode {
    /// The name which never changes, e.g. `rounds-v1`. Bridges report the modes they
    /// support by it, so a name of a newer bridge is no error for an older manager.
    pub fn name(self) -> String {
        format!("{}-v{}", self.family(), self.version())
    }

    /// The mode with exactly this name, e.g. `rounds-v1`.
    pub fn named(name: &str) -> Option<BuiltInMode> {
        Self::ALL.iter().copied().find(|mode| mode.name() == name)
    }

    /// The mode with this name, or the latest version of the mode with this family.
    pub fn from_name(name: &str) -> Option<BuiltInMode> {
        Self::named(name).or_else(|| Self::latest(name))
    }

    /// The latest version of a mode.
    pub fn latest(family: &str) -> Option<BuiltInMode> {
        let version = newest(family, Self::ALL.iter().map(|m| (m.family(), m.version())))?;
        Self::ALL
            .iter()
            .copied()
            .find(|m| m.family() == family && m.version() == version)
    }

    /// A newer version replaced this one, new configs have to use it.
    pub fn superseded_by(self) -> Option<BuiltInMode> {
        Self::latest(self.family()).filter(|latest| latest.version() > self.version())
    }
}

/// The highest version of the family among the versions.
fn newest<'a>(family: &str, versions: impl IntoIterator<Item = (&'a str, u16)>) -> Option<u16> {
    versions
        .into_iter()
        .filter(|(of, _)| *of == family)
        .map(|(_, version)| version)
        .max()
}

impl DriverMode {
    /// Whether a new config may select this mode.
    pub fn check_instantiable(self) -> Result<(), String> {
        match self {
            DriverMode::BuiltIn(mode) => refuse_superseded(mode, mode.superseded_by()),
        }
    }
}

fn refuse_superseded(mode: BuiltInMode, by: Option<BuiltInMode>) -> Result<(), String> {
    match by {
        Some(latest) => Err(format!(
            "{} is superseded by {}, new configs have to use it. Matches configured with {} keep playing it.",
            mode.name(),
            latest.name(),
            mode.name()
        )),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_unique_and_parse_back() {
        let mut names: Vec<String> = BuiltInMode::ALL.iter().map(|m| m.name()).collect();
        for (mode, name) in BuiltInMode::ALL.iter().zip(&names) {
            assert_eq!(BuiltInMode::from_name(name), Some(*mode));
        }
        names.sort();
        names.dedup();
        assert_eq!(names.len(), BuiltInMode::ALL.len());
        assert_eq!(BuiltInMode::RoundsV1.name(), "rounds-v1");
    }

    #[test]
    fn a_family_names_its_latest_version() {
        assert_eq!(
            BuiltInMode::from_name("knockout"),
            Some(BuiltInMode::KnockoutV1)
        );
        assert_eq!(BuiltInMode::named("knockout"), None);
        assert_eq!(
            BuiltInMode::named("knockout-v1"),
            Some(BuiltInMode::KnockoutV1)
        );
        assert_eq!(BuiltInMode::from_name("rounds-v2"), None);
        assert_eq!(BuiltInMode::from_name("Rounds"), None);
    }

    /// Only the latest version of a family can be configured, the others still play.
    #[test]
    fn the_newest_version_of_a_family_is_the_latest() {
        let versions = [("rounds", 1), ("knockout", 3), ("rounds", 2), ("rounds", 1)];
        assert_eq!(newest("rounds", versions), Some(2));
        assert_eq!(newest("knockout", versions), Some(3));
        assert_eq!(newest("endurance", versions), None);
    }

    #[test]
    fn superseded_modes_are_refused_with_their_successor() {
        let refused = refuse_superseded(BuiltInMode::RoundsV1, Some(BuiltInMode::KnockoutV1));
        assert_eq!(
            refused.unwrap_err(),
            "rounds-v1 is superseded by knockout-v1, new configs have to use it. Matches configured with rounds-v1 keep playing it."
        );
        assert_eq!(refuse_superseded(BuiltInMode::RoundsV1, None), Ok(()));
    }

    #[test]
    fn released_modes_are_their_latest_version() {
        for mode in BuiltInMode::ALL {
            assert_eq!(BuiltInMode::latest(mode.family()), Some(*mode));
            assert_eq!(mode.superseded_by(), None, "{} is the latest", mode.name());
            assert!(DriverMode::BuiltIn(*mode).check_instantiable().is_ok());
        }
    }
}
