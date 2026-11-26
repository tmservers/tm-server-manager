//! Manialinks a competition writes for its driver matches.
//!
//! A manialink has revisions which never change: a config shows a revision, so a match
//! shows what it was configured with, however the manialink changes later. A manialink
//! runs on the machines of the players, so writing them is a permission of its own.
//! Every revision is checked before it is saved, see `tm_mode_driver_manialinks::check`.

use spacetimedb::{
    CtxDbRead, ReducerContext, SpacetimeType, Table, Timestamp, ViewContext, reducer, table, view,
};
use tm_server_types::config::{Manialink, ServerConfigV2, UiManifest};

use crate::{
    authorization::Authorization,
    competition::{CompetitionPermissionsV1, CompetitionRead, node::NodeHandle},
    raw_server::{config::RawServerContigRead, occupation::TabRawServerOccupationRead},
    tm_match::MatchRead,
    tm_server::ServerRead,
};

#[table(accessor = tab_manialink, vis_private)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompetitionManialink {
    #[primary_key]
    #[auto_inc]
    pub id: u32,
    #[index(hash)]
    pub competition_id: u32,
    pub name: String,
}

#[table(accessor = tab_manialink_revision, vis_private)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManialinkRevision {
    #[primary_key]
    #[auto_inc]
    pub id: u32,
    #[index(hash)]
    pub manialink_id: u32,
    pub xml: String,
    /// What it sends and reads, checked against the xml.
    pub manifest: UiManifest,
    pub created_at: Timestamp,
}

/// A revision of a manialink as the bridge of a server gets it.
#[derive(Debug, Clone, PartialEq, Eq, SpacetimeType)]
pub struct ServerManialink {
    pub revision: u32,
    pub xml: String,
    pub manifest: UiManifest,
}

#[reducer]
pub(crate) fn manialink_create(
    ctx: &ReducerContext,
    competition_id: u32,
    name: String,
    xml: String,
    manifest: UiManifest,
) -> Result<(), String> {
    ctx.competition_find(competition_id)?;
    ctx.auth_builder(competition_id)
        .permission(CompetitionPermissionsV1::MANIALINK_EDIT)
        .authorize()?;
    tm_mode_driver_manialinks::check_with_manifest(&xml, &manifest)?;

    let manialink = ctx.db.tab_manialink().insert(CompetitionManialink {
        id: 0,
        competition_id,
        name,
    });
    ctx.db.tab_manialink_revision().insert(ManialinkRevision {
        id: 0,
        manialink_id: manialink.id,
        xml,
        manifest,
        created_at: ctx.timestamp,
    });
    Ok(())
}

/// A new revision of the manialink. Configs which show an earlier one keep showing it.
#[reducer]
pub(crate) fn manialink_revise(
    ctx: &ReducerContext,
    manialink_id: u32,
    xml: String,
    manifest: UiManifest,
) -> Result<(), String> {
    let manialink = editable(ctx, manialink_id)?;
    tm_mode_driver_manialinks::check_with_manifest(&xml, &manifest)?;
    ctx.db.tab_manialink_revision().insert(ManialinkRevision {
        id: 0,
        manialink_id: manialink.id,
        xml,
        manifest,
        created_at: ctx.timestamp,
    });
    Ok(())
}

#[reducer]
pub(crate) fn manialink_rename(
    ctx: &ReducerContext,
    manialink_id: u32,
    name: String,
) -> Result<(), String> {
    let mut manialink = editable(ctx, manialink_id)?;
    manialink.name = name;
    ctx.db.tab_manialink().id().update(manialink);
    Ok(())
}

fn editable(ctx: &ReducerContext, manialink_id: u32) -> Result<CompetitionManialink, String> {
    let Some(manialink) = ctx.db.tab_manialink().id().find(manialink_id) else {
        return Err("The manialink does not exist.".into());
    };
    ctx.auth_builder(manialink.competition_id)
        .permission(CompetitionPermissionsV1::MANIALINK_EDIT)
        .authorize()?;
    Ok(manialink)
}

/// The manialinks of a competition, the ones of the competitions above are not included.
#[view(accessor = competition_manialinks, public)]
fn competition_manialinks(ctx: &ViewContext, competition_id: u32) -> Vec<CompetitionManialink> {
    if ctx.auth_builder(competition_id).authorize().is_err() {
        return Vec::new();
    }
    ctx.db
        .tab_manialink()
        .competition_id()
        .filter(competition_id)
        .collect()
}

/// Every revision of a manialink, the first one first.
#[view(accessor = manialink_revisions, public)]
fn manialink_revisions(ctx: &ViewContext, manialink_id: u32) -> Vec<ManialinkRevision> {
    let Some(manialink) = ctx.db.tab_manialink().id().find(manialink_id) else {
        return Vec::new();
    };
    if ctx
        .auth_builder(manialink.competition_id)
        .authorize()
        .is_err()
    {
        return Vec::new();
    }
    let mut revisions: Vec<ManialinkRevision> = ctx
        .db
        .tab_manialink_revision()
        .manialink_id()
        .filter(manialink_id)
        .collect();
    revisions.sort_by_key(|r| r.id);
    revisions
}

/// The manialinks of competitions the config the server plays shows, for its bridge.
#[view(accessor = my_server_manialinks, public)]
fn my_server_manialinks(ctx: &ViewContext) -> Vec<ServerManialink> {
    let Ok(server_id) = ctx.server_id() else {
        return Vec::new();
    };
    let config = match ctx.raw_server_occupation(server_id) {
        Some(NodeHandle::MatchV1(match_id)) => ctx
            .match_find(match_id)
            .map(|m| m.config_ids().to_vec())
            .unwrap_or_default(),
        Some(NodeHandle::ServerV1(id)) => ctx
            .server_find(id)
            .map(|s| vec![s.get_config_id()])
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    let mut revisions: Vec<u32> = config
        .into_iter()
        .filter_map(|id| ctx.raw_server_config(id).ok())
        .flat_map(|config| config.custom_manialinks())
        .collect();
    revisions.sort_unstable();
    revisions.dedup();
    revisions
        .into_iter()
        .filter_map(|revision| ctx.db.tab_manialink_revision().id().find(revision))
        .map(|revision| ServerManialink {
            revision: revision.id,
            xml: revision.xml,
            manifest: revision.manifest,
        })
        .collect()
}

pub(crate) trait ManialinkRead {
    /// What a manialink sends and reads: the manifest of a default, the one of a revision.
    fn manialink_manifest(&self, manialink: &Manialink) -> Option<UiManifest>;
    /// A config of the competition may only show manialinks of it and of the
    /// competitions above it.
    fn manialinks_check(&self, competition_id: u32, config: &ServerConfigV2) -> Result<(), String>;
}

impl<Db: CtxDbRead> ManialinkRead for Db {
    fn manialink_manifest(&self, manialink: &Manialink) -> Option<UiManifest> {
        match manialink {
            Manialink::BuiltIn(manialink) => Some(manialink.manifest()),
            Manialink::Custom(revision) => self
                .db_read_only()
                .tab_manialink_revision()
                .id()
                .find(*revision)
                .map(|revision| revision.manifest),
        }
    }

    fn manialinks_check(&self, competition_id: u32, config: &ServerConfigV2) -> Result<(), String> {
        let allowed = self.competition_ancestors(competition_id);
        for revision in config.custom_manialinks() {
            let manialink = self
                .db_read_only()
                .tab_manialink_revision()
                .id()
                .find(revision)
                .and_then(|r| {
                    self.db_read_only()
                        .tab_manialink()
                        .id()
                        .find(r.manialink_id)
                });
            match manialink {
                Some(manialink) if allowed.contains(&manialink.competition_id) => {}
                Some(_) => {
                    return Err(format!(
                        "Revision {revision} is a manialink of another competition."
                    ));
                }
                None => return Err(format!("There is no revision {revision} of a manialink.")),
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use tm_server_types::config::{
        BuiltInMode, Driver, DriverMode, DriverUi, Manialink, ModeSettingsV2, UiLayer,
    };

    use super::*;
    use crate::{competition::node::Vec2, test_world::*};

    const XML: &str = r#"<manialink version="3"><label text="Hello"/></manialink>"#;

    /// The revisions of the manialink of this name in the competition, the first one first.
    fn revisions(world: &World, competition_id: u32, name: &str) -> Vec<ManialinkRevision> {
        let cx = world.test.view_context(World::owner_auth());
        let manialink = competition_manialinks(&cx, competition_id)
            .into_iter()
            .find(|m| m.name == name)
            .expect("the manialink exists");
        manialink_revisions(&cx, manialink.id)
    }

    fn create(world: &World, competition_id: u32, name: &str, xml: &str) -> u32 {
        world.as_owner(|ctx| {
            manialink_create(
                ctx,
                competition_id,
                name.into(),
                xml.into(),
                UiManifest::default(),
            )
        });
        revisions(world, competition_id, name)[0].id
    }

    /// A config which plays rounds with the driver and shows this revision.
    fn showing(revision: u32) -> ServerConfigV2 {
        let mut ui = DriverUi::default();
        ui.layers.push(UiLayer::new(Manialink::Custom(revision)));
        ServerConfigV2::default().with_mode(ModeSettingsV2::Driver(Driver {
            mode: DriverMode::BuiltIn(BuiltInMode::RoundsV1),
            ui,
            ..Default::default()
        }))
    }

    #[test]
    fn a_new_revision_keeps_the_ones_before() {
        let world = World::new();
        let first = create(&world, COMPETITION, "Sponsors", XML);
        let manialink = revisions(&world, COMPETITION, "Sponsors")[0].manialink_id;

        let changed = XML.replace("Hello", "Hi");
        world.as_owner(|ctx| {
            manialink_revise(ctx, manialink, changed.clone(), UiManifest::default())
        });

        let revisions = revisions(&world, COMPETITION, "Sponsors");
        let saved: Vec<(u32, &str)> = revisions.iter().map(|r| (r.id, r.xml.as_str())).collect();
        assert_eq!(
            saved,
            vec![(first, XML), (revisions[1].id, changed.as_str())]
        );
        assert!(revisions[1].id > first);
    }

    #[test]
    fn manialinks_are_checked_before_they_are_saved() {
        let world = World::new();
        let refused = world.try_as_owner(|ctx| {
            manialink_create(
                ctx,
                COMPETITION,
                "Broken".into(),
                "<manialink><frame>".into(),
                UiManifest::default(),
            )
        });
        assert_eq!(refused.unwrap_err(), "<frame> is not closed.");

        let first = create(&world, COMPETITION, "Fine", XML);
        let manialink = revisions(&world, COMPETITION, "Fine")[0].manialink_id;
        let wrong_type = "<manialink><script><!--\ndeclare netread Integer Net_Driver_StandingsScores for Teams[0];\n--></script></manialink>";
        let refused = world.try_as_owner(|ctx| {
            manialink_revise(ctx, manialink, wrong_type.into(), UiManifest::default())
        });
        assert!(refused.is_err_and(|e| e.contains("is Integer[], not Integer")));
        assert_eq!(revisions(&world, COMPETITION, "Fine").len(), 1);
        assert_eq!(revisions(&world, COMPETITION, "Fine")[0].id, first);
    }

    #[test]
    fn a_revision_keeps_the_manifest_of_what_it_sends() {
        use tm_server_types::config::{UiArg, UiEventSpec, UiRoute};
        let world = World::new();
        let voting = r#"<manialink version="3"><script><!-- SendCustomEvent("vote", ["1"]); --></script></manialink>"#;
        let refused = world.try_as_owner(|ctx| {
            manialink_create(
                ctx,
                COMPETITION,
                "Vote".into(),
                voting.into(),
                UiManifest::default(),
            )
        });
        assert_eq!(
            refused.unwrap_err(),
            "The manialink sends vote, which its manifest does not declare."
        );

        let manifest = UiManifest {
            events: vec![UiEventSpec {
                name: "vote".into(),
                route: UiRoute::Manager,
                args: vec![UiArg::Bool],
            }],
            data_keys: vec!["votes".into()],
        };
        world.as_owner(|ctx| {
            manialink_create(
                ctx,
                COMPETITION,
                "Vote".into(),
                voting.into(),
                manifest.clone(),
            )
        });
        let revision = &revisions(&world, COMPETITION, "Vote")[0];
        assert_eq!(revision.manifest, manifest);
        let read = world.test.anonymous_view_context();
        assert_eq!(
            read.manialink_manifest(&Manialink::Custom(revision.id)),
            Some(manifest)
        );
        assert_eq!(
            read.manialink_manifest(&Manialink::Custom(revision.id + 1)),
            None
        );
        let ready = tm_server_types::config::BuiltInManialink::ReadyCheck;
        assert_eq!(
            read.manialink_manifest(&Manialink::BuiltIn(ready)),
            Some(ready.manifest())
        );
    }

    #[test]
    fn configs_only_show_manialinks_of_their_competition_and_the_ones_above() {
        let world = World::new();
        world.as_owner(|ctx| {
            crate::competition::competition_create(
                ctx,
                "Group A".into(),
                COMPETITION,
                Vec2::ZERO,
                0,
            )
        });
        let group = world
            .test
            .anonymous_view_context()
            .competition_descendants(COMPETITION)
            .into_iter()
            .find(|c| c.id != COMPETITION)
            .expect("the group exists")
            .id;
        let of_root = create(&world, COMPETITION, "Sponsors", XML);
        let of_group = create(&world, group, "Bracket", XML);

        // The root shows its own, the group the ones of the root too.
        let root_match = world.configured_match_with("Root", showing(of_root));
        world.as_owner(|ctx| {
            crate::tm_match::match_override_config(ctx, root_match, showing(of_root))
        });
        let shared = world.try_as_owner(|ctx| {
            crate::raw_server::config::raw_server_config_shared_new(
                ctx,
                "Group".into(),
                group,
                showing(of_root),
            )
        });
        assert_eq!(shared, Ok(()));
        // The root cannot share what only the group has.
        let shared = world.try_as_owner(|ctx| {
            crate::raw_server::config::raw_server_config_shared_new(
                ctx,
                "Root".into(),
                COMPETITION,
                showing(of_group),
            )
        });
        assert!(shared.is_err_and(|e| e.contains("of another competition")));

        let refused = world.try_as_owner(|ctx| {
            crate::tm_match::match_override_config(ctx, root_match, showing(of_group))
        });
        assert_eq!(
            refused.unwrap_err(),
            format!("Revision {of_group} is a manialink of another competition.")
        );
        let missing = world.try_as_owner(|ctx| {
            crate::tm_match::match_override_config(ctx, root_match, showing(of_group + 100))
        });
        assert!(missing.is_err_and(|e| e.contains("There is no revision")));
    }

    #[test]
    fn the_bridge_gets_the_manialinks_its_match_shows() {
        let world = World::new();
        let server = world.server(1);
        let revision = create(&world, COMPETITION, "Sponsors", XML);
        server.report_modes(&world, &["rounds-v1"]);
        let match_id = world.configured_match_with("Final", showing(revision));
        world.as_owner(|ctx| crate::tm_match::match_set_preparation(ctx, match_id));

        let shown = my_server_manialinks(&server.view(&world));

        assert_eq!(
            shown,
            vec![ServerManialink {
                revision,
                xml: XML.into(),
                manifest: UiManifest::default(),
            }]
        );
        // Other servers get nothing.
        let other = world.server(2);
        assert_eq!(my_server_manialinks(&other.view(&world)), vec![]);
    }
}
