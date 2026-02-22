use spacetimedb::{SpacetimeType, ViewContext, table, view};

use crate::worker::tm_worker__view;

#[table(accessor=tm_worker_jobs,index(accessor= worker_id,btree(columns= [tm_login])))]
#[derive(Debug)]
pub struct TmWorkerJobs {
    // The
    pub tm_login: String,

    map_uid: String,
}

impl TmWorkerJobs {
    pub fn new(map_uid: String) -> Self {
        Self {
            tm_login: "joestestcellar".into(),
            map_uid,
        }
    }
}

#[derive(Debug, SpacetimeType)]
pub enum JobKind {}

#[view(accessor= my_jobs,public)]
pub fn my_jobs(ctx: &ViewContext) -> Vec<TmWorkerJobs> {
    log::error!("Identity: {}", ctx.sender());
    let Some(worker) = ctx.db.tm_worker().identity().find(ctx.sender()) else {
        log::error!("Worker not found");
        return Vec::new();
    };
    let jobs = ctx
        .db
        .tm_worker_jobs()
        .worker_id()
        .filter(&worker.tm_login)
        .collect::<Vec<_>>();
    log::error!("{jobs:?}");

    jobs
}
