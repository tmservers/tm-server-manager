use tm_server_types::config::ServerConfig;

#[cfg_attr(feature = "spacetime",spacetimedb::table(name=tm_server_config, public))]
pub struct TmServerConfig {
    #[cfg_attr(feature = "spacetime", auto_inc)]
    #[cfg_attr(feature = "spacetime", primary_key)]
    pub id: u64,

    /// Ubi id of the creator
    creator: String,

    config: ServerConfig,
}

impl TmServerConfig {
    pub fn get_config(self) -> ServerConfig {
        self.config
    }
}

#[cfg(feature = "spacetime")]
#[cfg_attr(feature = "spacetime", spacetimedb::reducer)]
pub fn create_server_config(ctx: &spacetimedb::ReducerContext, id: String, config: ServerConfig) {
    use spacetimedb::Table;

    ctx.db.tm_server_config().insert(TmServerConfig {
        id: 0,
        creator: id,
        config,
    });
}
