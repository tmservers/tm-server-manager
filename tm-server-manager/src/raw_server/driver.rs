//! The modes the bridge of a raw server plays with the mode driver.
//!
//! A bridge reports them after it logged in. An older bridge does not know newer
//! versions of the modes and never gets a match which plays one. A bridge which
//! reports nothing, e.g. one from before the mode driver, gets no driver matches.

use spacetimedb::{CtxDbRead, CtxDbWrite, ReducerContext, Table, reducer, table};
use tm_server_types::config::{BuiltInMode, DriverMode};

use crate::{authorization::Authorization, competition::connection::start::NodeStartWrite};

#[table(accessor = tab_raw_server_driver, vis_private)]
struct RawServerDriver {
    #[primary_key]
    server_id: u32,
    modes: Vec<BuiltInMode>,
}

/// The bridge of the server tells which modes it plays, by their names like `rounds-v1`.
/// Names this manager does not know, e.g. of a newer bridge, are left out.
#[reducer]
pub(crate) fn raw_server_driver_modes(
    ctx: &ReducerContext,
    modes: Vec<String>,
) -> Result<(), String> {
    let server_id = ctx.server_id()?;

    let mut known: Vec<BuiltInMode> = modes
        .iter()
        .filter_map(|name| BuiltInMode::named(name))
        .collect();
    known.sort();
    known.dedup();

    ctx.db.tab_raw_server_driver().server_id().delete(server_id);
    ctx.db.tab_raw_server_driver().try_insert(RawServerDriver {
        server_id,
        modes: known,
    })?;

    // Nodes which waited for a server that plays their mode may start now.
    ctx.node_starts_retry();

    Ok(())
}

pub(crate) trait RawServerDriverRead {
    /// Whether the bridge of the server plays the mode.
    fn raw_server_plays(&self, server_id: u32, mode: DriverMode) -> bool;
}

impl<Db: CtxDbRead> RawServerDriverRead for Db {
    fn raw_server_plays(&self, server_id: u32, mode: DriverMode) -> bool {
        let Some(driver) = self
            .db_read_only()
            .tab_raw_server_driver()
            .server_id()
            .find(server_id)
        else {
            return false;
        };
        match mode {
            DriverMode::BuiltIn(mode) => driver.modes.contains(&mode),
        }
    }
}

/// A bridge which logs in reports its modes again, it may be another version than before.
pub(crate) fn raw_server_driver_forget(ctx: &impl CtxDbWrite, server_id: u32) {
    ctx.db().tab_raw_server_driver().server_id().delete(server_id);
}

