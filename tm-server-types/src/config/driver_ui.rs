//! What players see of a driver match: the manialinks the server shows, where and to
//! whom, and which UI modules of the game stay visible.
//!
//! The config of a match carries it, the bridge sends it to the driver modescript when
//! the config loads and whenever the modescript restarts. Enums are only ever appended,
//! the database stores them.

use crate::method::{DriverAudience, DriverLayer, DriverLayerKind, DriverMarker};

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct DriverUi {
    pub layers: Vec<UiLayer>,
    /// UI modules of the game to show or hide, e.g. its scores table.
    pub modules: Vec<UiModuleVisibility>,
}

impl Default for DriverUi {
    /// The standings of the match and the scores table of the driver.
    fn default() -> Self {
        Self {
            layers: vec![
                UiLayer::new(Manialink::BuiltIn(BuiltInManialink::Standings)),
                UiLayer::new(Manialink::BuiltIn(BuiltInManialink::ScoresTable)),
            ],
            modules: Vec::new(),
        }
    }
}

/// A manialink, where the game shows it and to whom.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct UiLayer {
    pub manialink: Manialink,
    pub placement: Placement,
    pub audience: Audience,
}

impl UiLayer {
    /// The manialink where it belongs, for everyone.
    pub fn new(manialink: Manialink) -> Self {
        Self {
            manialink,
            placement: Placement::Default,
            audience: Audience::Everyone,
        }
    }

    /// The layer the driver modescript creates for it, with the xml of the manialink.
    pub fn layer(&self, xml: impl Into<String>) -> DriverLayer {
        let (kind, attach_id) = self.placement.of(&self.manialink);
        let markers = match &self.placement {
            Placement::Markers(markers) => markers.iter().map(MarkerBinding::marker).collect(),
            _ => Vec::new(),
        };
        DriverLayer {
            name: self.manialink.name(),
            manialink: xml.into(),
            kind,
            attach_id: attach_id.into(),
            audience: self.audience.of(),
            markers,
        }
    }
}

/// A manialink the bridge knows.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub enum Manialink {
    /// One of the default manialinks of `tm-mode-driver-manialinks`.
    BuiltIn(BuiltInManialink),
    /// A revision of a manialink of the competition, which never changes.
    Custom(u32),
}

impl Manialink {
    /// The name of the layer, unique within a config.
    pub fn name(&self) -> String {
        match self {
            Manialink::BuiltIn(manialink) => manialink.name().into(),
            Manialink::Custom(revision) => format!("custom-{revision}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub enum BuiltInManialink {
    /// The top of the match with the points of the round.
    Standings,
    /// The round instead of the match, live while it runs.
    LiveStandings,
    /// Where the local player stands.
    MyStanding,
    /// Replaces the scores table of the game.
    ScoresTable,
    /// The standings of the teams of a match of teams.
    TeamStandings,
    /// The score of the teams at the top of the screen, e.g. for TMWT.
    TeamScoreHeader,
}

impl BuiltInManialink {
    pub const ALL: [BuiltInManialink; 6] = [
        BuiltInManialink::Standings,
        BuiltInManialink::LiveStandings,
        BuiltInManialink::MyStanding,
        BuiltInManialink::ScoresTable,
        BuiltInManialink::TeamStandings,
        BuiltInManialink::TeamScoreHeader,
    ];

    /// The name of the manialink in `tm-mode-driver-manialinks`.
    pub const fn name(self) -> &'static str {
        match self {
            BuiltInManialink::Standings => "standings",
            BuiltInManialink::LiveStandings => "live-standings",
            BuiltInManialink::MyStanding => "my-standing",
            BuiltInManialink::ScoresTable => "scores-table",
            BuiltInManialink::TeamStandings => "team-standings",
            BuiltInManialink::TeamScoreHeader => "team-score-header",
        }
    }

    /// Where it goes without a placement of its own.
    const fn default_kind(self) -> DriverLayerKind {
        match self {
            BuiltInManialink::ScoresTable => DriverLayerKind::ScoresTable,
            _ => DriverLayerKind::Normal,
        }
    }
}

/// Where the game shows a manialink.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub enum Placement {
    /// Where the manialink belongs: the scores table for the scores table, over the race
    /// for the others.
    Default,
    /// Over the race.
    Normal,
    /// While the scores table key is held and whenever the modescript forces it.
    ScoresTable,
    /// On a screen in the stadium.
    Screen(ScreenTarget),
    /// Frames of the manialink which follow players, landmarks or places of the map.
    Markers(Vec<MarkerBinding>),
    /// While the alternative menu is open.
    AltMenu,
    /// During the intro and the podium.
    CutScene,
    /// While a map loads.
    LoadingScreen,
}

impl Placement {
    /// The kind of layer and the screen it attaches to.
    fn of(&self, manialink: &Manialink) -> (DriverLayerKind, &'static str) {
        match self {
            Placement::Default => match manialink {
                Manialink::BuiltIn(manialink) => (manialink.default_kind(), ""),
                Manialink::Custom(_) => (DriverLayerKind::Normal, ""),
            },
            Placement::Normal => (DriverLayerKind::Normal, ""),
            Placement::ScoresTable => (DriverLayerKind::ScoresTable, ""),
            Placement::Screen(target) => (DriverLayerKind::ScreenIn3d, target.attach_id()),
            Placement::Markers(_) => (DriverLayerKind::Markers, ""),
            Placement::AltMenu => (DriverLayerKind::AltMenu, ""),
            Placement::CutScene => (DriverLayerKind::CutScene, ""),
            Placement::LoadingScreen => (DriverLayerKind::LoadingScreen, ""),
        }
    }
}

/// The screens of the stadium a layer can attach to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub enum ScreenTarget {
    /// The big 16:9 screens.
    Screen16x9,
    Screen16x9Small,
    /// The signs of the start, the finish and the checkpoints.
    Sign64x10Start,
    Sign64x10Finish,
    Sign64x10Checkpoint,
    Sign2x3,
}

impl ScreenTarget {
    /// The `AttachId` of the layer.
    pub const fn attach_id(self) -> &'static str {
        match self {
            ScreenTarget::Screen16x9 => "155",
            ScreenTarget::Screen16x9Small => "155_StadiumSmall",
            ScreenTarget::Sign64x10Start => "64x10_Start",
            ScreenTarget::Sign64x10Finish => "64x10_Finish",
            ScreenTarget::Sign64x10Checkpoint => "64x10_Checkpoint",
            ScreenTarget::Sign2x3 => "2x3_Stadium",
        }
    }
}

/// A frame of a markers layer, shown at everything the anchor names.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct MarkerBinding {
    /// The id of the frame in the manialink.
    pub frame_id: String,
    pub anchor: MarkerAnchor,
    /// The text the marker shows next to the frame, the host fills it in.
    pub label: Option<MarkerLabel>,
    /// A player does not see the marker of its own car.
    pub hide_on_self: bool,
    /// Markers further away than this many meters disappear, none for no limit.
    pub max_distance: Option<u16>,
    pub visibility: MarkerVisibility,
}

impl MarkerBinding {
    fn marker(&self) -> DriverMarker {
        DriverMarker {
            frame_id: self.frame_id.clone(),
            anchor: match &self.anchor {
                MarkerAnchor::Players(_) => "players".into(),
                MarkerAnchor::Landmarks(kind) => kind.tag().into(),
                MarkerAnchor::Position(_) => "position".into(),
            },
            players: match &self.anchor {
                MarkerAnchor::Players(audience) => audience.of(),
                _ => DriverAudience::default(),
            },
            position: match self.anchor {
                MarkerAnchor::Position(MapPosition { x, y, z }) => vec![x, y, z],
                _ => Vec::new(),
            },
            label: self.label.map(MarkerLabel::code).unwrap_or_default().into(),
            hide_on_self: self.hide_on_self,
            max_distance: self.max_distance.map_or(0, u32::from),
            visibility: self.visibility.code().into(),
        }
    }
}

/// What a marker follows.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub enum MarkerAnchor {
    /// One marker above the car of every one of these players.
    Players(Audience),
    /// One marker at every landmark of this kind of the map.
    Landmarks(LandmarkKind),
    /// One marker at this place of the map.
    Position(MapPosition),
}

/// A place of a map in meters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct MapPosition {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub enum LandmarkKind {
    Start,
    Checkpoint,
    Finish,
    /// The line which starts and ends the laps of a map with laps.
    Multilap,
}

impl LandmarkKind {
    /// The tag of the landmarks of the map.
    const fn tag(self) -> &'static str {
        match self {
            LandmarkKind::Start => "Spawn",
            LandmarkKind::Checkpoint => "Checkpoint",
            LandmarkKind::Finish => "Goal",
            LandmarkKind::Multilap => "StartFinish",
        }
    }
}

/// What the label of a marker on a player shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub enum MarkerLabel {
    Name,
    ClubTag,
    /// The position in the standings of the match.
    Position,
    /// The points in the standings of the match.
    Score,
    TeamShortName,
}

impl MarkerLabel {
    const fn code(self) -> &'static str {
        match self {
            MarkerLabel::Name => "name",
            MarkerLabel::ClubTag => "club_tag",
            MarkerLabel::Position => "position",
            MarkerLabel::Score => "score",
            MarkerLabel::TeamShortName => "team_short_name",
        }
    }
}

/// When the game shows a marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub enum MarkerVisibility {
    Always,
    /// When it is in the field of view.
    WhenInFrustum,
    /// When nothing hides it.
    WhenVisible,
    WhenInMiddleOfScreen,
}

impl MarkerVisibility {
    const fn code(self) -> &'static str {
        match self {
            MarkerVisibility::Always => "always",
            MarkerVisibility::WhenInFrustum => "when_in_frustum",
            MarkerVisibility::WhenVisible => "when_visible",
            MarkerVisibility::WhenInMiddleOfScreen => "when_in_middle_of_screen",
        }
    }
}

/// Who sees a layer.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub enum Audience {
    Everyone,
    /// Who races, not who watches.
    Players,
    Spectators,
    /// The players of these teams of the match.
    Teams(Vec<u32>),
    /// These accounts.
    Accounts(Vec<String>),
}

impl Audience {
    fn of(&self) -> DriverAudience {
        let (kind, teams, accounts) = match self {
            Audience::Everyone => ("everyone", Vec::new(), Vec::new()),
            Audience::Players => ("players", Vec::new(), Vec::new()),
            Audience::Spectators => ("spectators", Vec::new(), Vec::new()),
            Audience::Teams(teams) => ("teams", teams.clone(), Vec::new()),
            Audience::Accounts(accounts) => ("accounts", Vec::new(), accounts.clone()),
        };
        DriverAudience {
            kind: kind.into(),
            teams,
            accounts,
        }
    }
}

/// A UI module of the game to show or hide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct UiModuleVisibility {
    pub module: UiModule,
    pub visible: bool,
}

/// The UI modules of the game a config can show or hide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub enum UiModule {
    ScoresTable,
    Chrono,
    Checkpoint,
    LapsCounter,
    TimeGap,
    RespawnHelper,
    SpectatorName,
    BigMessage,
}

impl UiModule {
    /// The id of the module in the game.
    pub const fn id(self) -> &'static str {
        match self {
            UiModule::ScoresTable => "Race_ScoresTable",
            UiModule::Chrono => "Race_Chrono",
            UiModule::Checkpoint => "Race_Checkpoint",
            UiModule::LapsCounter => "Race_LapsCounter",
            UiModule::TimeGap => "Race_TimeGap",
            UiModule::RespawnHelper => "Race_RespawnHelper",
            UiModule::SpectatorName => "Race_SpectatorBase_Name",
            UiModule::BigMessage => "Race_BigMessage",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layer(placement: Placement, manialink: BuiltInManialink) -> DriverLayer {
        UiLayer {
            manialink: Manialink::BuiltIn(manialink),
            placement,
            audience: Audience::Everyone,
        }
        .layer("<manialink/>")
    }

    #[test]
    fn built_in_manialinks_go_where_they_belong() {
        let scores = layer(Placement::Default, BuiltInManialink::ScoresTable);
        assert_eq!(scores.kind, DriverLayerKind::ScoresTable);
        assert_eq!(scores.name, "scores-table");
        let standings = layer(Placement::Default, BuiltInManialink::Standings);
        assert_eq!(standings.kind, DriverLayerKind::Normal);
        // Placed elsewhere on purpose.
        let shown = layer(Placement::ScoresTable, BuiltInManialink::TeamStandings);
        assert_eq!(shown.kind, DriverLayerKind::ScoresTable);
    }

    #[test]
    fn screens_are_layers_in_3d_attached_to_their_screen() {
        let layer = layer(
            Placement::Screen(ScreenTarget::Sign64x10Finish),
            BuiltInManialink::Standings,
        );
        assert_eq!(layer.kind, DriverLayerKind::ScreenIn3d);
        assert_eq!(layer.attach_id, "64x10_Finish");
    }

    #[test]
    fn markers_carry_their_anchor_and_label() {
        let binding = |anchor| MarkerBinding {
            frame_id: "Frame_Marker".into(),
            anchor,
            label: Some(MarkerLabel::Position),
            hide_on_self: true,
            max_distance: Some(150),
            visibility: MarkerVisibility::WhenVisible,
        };
        let layer = layer(
            Placement::Markers(vec![
                binding(MarkerAnchor::Players(Audience::Teams(vec![7]))),
                binding(MarkerAnchor::Landmarks(LandmarkKind::Finish)),
                binding(MarkerAnchor::Position(MapPosition { x: 1, y: 2, z: 3 })),
            ]),
            BuiltInManialink::Standings,
        );
        assert_eq!(layer.kind, DriverLayerKind::Markers);
        let [players, finish, place] = &layer.markers[..] else {
            panic!("three markers")
        };
        assert_eq!(players.anchor, "players");
        assert_eq!(players.players.kind, "teams");
        assert_eq!(players.players.teams, [7]);
        assert_eq!(players.label, "position");
        assert_eq!(players.max_distance, 150);
        assert_eq!(players.visibility, "when_visible");
        assert_eq!(finish.anchor, "Goal");
        assert_eq!(place.position, [1, 2, 3]);
    }

    #[test]
    fn audiences_name_who_sees_a_layer() {
        let mut ui = UiLayer::new(Manialink::BuiltIn(BuiltInManialink::MyStanding));
        assert_eq!(ui.layer("").audience.kind, "everyone");
        ui.audience = Audience::Accounts(vec!["a".into()]);
        let audience = ui.layer("").audience;
        assert_eq!(
            (audience.kind.as_str(), audience.accounts),
            ("accounts", vec!["a".into()])
        );
    }

    #[test]
    fn every_built_in_manialink_has_its_own_name() {
        let mut names: Vec<&str> = BuiltInManialink::ALL.iter().map(|m| m.name()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), BuiltInManialink::ALL.len());
    }
}
