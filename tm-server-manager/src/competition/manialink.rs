//! Manialinks a competition writes for its driver matches.
//!
//! A manialink has revisions which never change: a config shows a revision, so a match
//! shows what it was configured with, however the manialink changes later. A manialink
//! runs on the machines of the players, so writing them is a permission of its own.
//! Every revision is checked before it is saved, see `tm_mode_driver_manialinks::check`.

use spacetimedb::{
    CtxDbRead, ReducerContext, SpacetimeType, Table, Timestamp, ViewContext, reducer, table, view,
};
use tm_server_types::config::ServerConfigV2;

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
    pub created_at: Timestamp,
}

/// A revision of a manialink as the bridge of a server gets it.
#[derive(Debug, Clone, PartialEq, Eq, SpacetimeType)]
pub struct ServerManialink {
    pub revision: u32,
    pub xml: String,
}

#[reducer]
pub(crate) fn manialink_create(
    ctx: &ReducerContext,
    competition_id: u32,
    name: String,
    xml: String,
) -> Result<(), String> {
    ctx.competition_find(competition_id)?;
    ctx.auth_builder(competition_id)
        .permission(CompetitionPermissionsV1::MANIALINK_EDIT)
        .authorize()?;
    tm_mode_driver_manialinks::check(&xml)?;

    let manialink = ctx.db.tab_manialink().insert(CompetitionManialink {
        id: 0,
        competition_id,
        name,
    });
    ctx.db.tab_manialink_revision().insert(ManialinkRevision {
        id: 0,
        manialink_id: manialink.id,
        xml,
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
) -> Result<(), String> {
    let manialink = editable(ctx, manialink_id)?;
    tm_mode_driver_manialinks::check(&xml)?;
    ctx.db.tab_manialink_revision().insert(ManialinkRevision {
        id: 0,
        manialink_id: manialink.id,
        xml,
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
        })
        .collect()
}

pub(crate) trait ManialinkRead {
    /// A config of the competition may only show manialinks of it and of the
    /// competitions above it.
    fn manialinks_check(&self, competition_id: u32, config: &ServerConfigV2) -> Result<(), String>;
}

impl<Db: CtxDbRead> ManialinkRead for Db {
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

