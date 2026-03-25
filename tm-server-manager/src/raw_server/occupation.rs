use spacetimedb::{DbContext, Local, table};

use crate::competition::node::NodeHandle;

#[table(
    accessor=tab_raw_server_occupation,
    index(accessor=node_handle, hash(columns=[node_variant,node_id]))
)]
struct RawServerOccupation {
    #[primary_key]
    server_id: u32,

    node_id: u32,

    node_variant: u8,
}

impl RawServerOccupation {
    fn new(node_handle: NodeHandle, server_id: u32) -> Self {
        let (node_variant, node_id) = node_handle.split();
        Self {
            server_id,
            node_id,
            node_variant,
        }
    }
}

pub(crate) trait TabRawServerOccupationRead {
    fn raw_server_is_occupied(&self, server_id: u32) -> bool;
    fn raw_server_occupation(&self, server_id: u32) -> Option<NodeHandle>;
    fn occupation_with_occupier(&self, node_handle: NodeHandle) -> Option<NodeHandle>;
}
pub(crate) trait TabRawServerOccupationWrite: TabRawServerOccupationRead {
    fn raw_server_occupation_add(
        &self,
        node_handle: NodeHandle,
        server_id: u32,
    ) -> Result<(), String>;
    fn raw_server_occupation_remove(&self, node_handle: NodeHandle) -> Result<(), String>;
}

impl<Db: DbContext> TabRawServerOccupationRead for Db {
    fn raw_server_is_occupied(&self, server_id: u32) -> bool {
        self.db_read_only()
            .tab_raw_server_occupation()
            .server_id()
            .find(server_id)
            .is_some()
    }

    fn raw_server_occupation(&self, server_id: u32) -> Option<NodeHandle> {
        self.db_read_only()
            .tab_raw_server_occupation()
            .server_id()
            .find(server_id)
            .map(|o| NodeHandle::combine(o.node_variant, o.node_id))
    }

    fn occupation_with_occupier(&self, node_handle: NodeHandle) -> Option<NodeHandle> {
        self.db_read_only()
            .tab_raw_server_occupation()
            .node_handle()
            .filter(node_handle.split())
            .next()
            .map(|o| NodeHandle::combine(o.node_variant, o.node_id))
    }
}

impl<Db: DbContext<DbView = Local>> TabRawServerOccupationWrite for Db {
    fn raw_server_occupation_add(
        &self,
        node_handle: NodeHandle,
        server_id: u32,
    ) -> Result<(), String> {
        self.db()
            .tab_raw_server_occupation()
            .server_id()
            .try_insert_or_update(RawServerOccupation::new(node_handle, server_id))?;

        Ok(())
    }

    fn raw_server_occupation_remove(&self, node_handle: NodeHandle) -> Result<(), String> {
        self.db()
            .tab_raw_server_occupation()
            .node_handle()
            .delete(node_handle.split());
        Ok(())
    }
}
