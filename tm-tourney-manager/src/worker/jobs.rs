use spacetimedb::{SpacetimeType, ViewContext, view};

use crate::worker::tm_worker__view;

#[cfg_attr(feature = "spacetime", spacetimedb::table(name=tm_worker_jobs,index(name= worker_id,btree(columns= [tm_login]))))]
pub struct TmWorkerJobs {
    // The
    pub tm_login: String,

    map_uid: String,
}

#[derive(Debug, SpacetimeType)]
pub enum JobKind {}

#[view(name= my_jobs,public)]
pub fn my_jobs(ctx: &ViewContext) -> Vec<TmWorkerJobs> {
    let Some(worker) = ctx.db.tm_worker().identity().find(ctx.sender) else {
        return Vec::new();
    };
    ctx.db
        .tm_worker_jobs()
        .worker_id()
        .filter(&worker.tm_login)
        .collect()
}
