use spacetimedb::{ReducerContext, ScheduleAt, Table, TimeDuration, Timestamp, reducer, table};

use crate::tm_match::{MatchWrite, tab_match};

// Responsible of redistributing the match if the old server does not come back.
#[table(accessor= tab_match_auto_recovery, vis_private, scheduled(on_match_auto_recovery))]
pub struct TabMatchAutoRecovery {
    #[auto_inc]
    #[primary_key]
    pub scheduled_id: u64,

    scheduled_at: ScheduleAt,

    match_id: u32,
}

pub(super) trait RecoveryWrite {
    fn match_auto_recovery_insert(
        &self,
        match_id: u32,
        now: Timestamp,
        duration: TimeDuration,
    ) -> Result<(), String>;
}
impl<Db: spacetimedb::CtxDbWrite> RecoveryWrite for Db {
    fn match_auto_recovery_insert(
        &self,
        match_id: u32,
        now: Timestamp,
        duration: TimeDuration,
    ) -> Result<(), String> {
        self.db()
            .tab_match_auto_recovery()
            .try_insert(TabMatchAutoRecovery {
                scheduled_id: 0,
                scheduled_at: ScheduleAt::Time(now + duration),
                match_id,
            })?;
        Ok(())
    }
}

#[reducer]
fn on_match_auto_recovery(ctx: &ReducerContext, args: TabMatchAutoRecovery) {
    let tm_match = ctx.db.tab_match().id().find(args.match_id).unwrap();
    if !tm_match.is_recovery() {
        //Match is no longer in recovery so it was brought back some other way.
        log::info!(
            "Auto Recovery: Match {} is no longer in recovery so it was brought back some other way.",
            tm_match.id
        );
        return;
    }
    // If this is triggered we are in the following path:
    // -> Bridge Disconnected
    // -> Match entered Recovery
    // -> Bridge did not come back online in the duration so we are still in recovery
    // -> We need to switch server now.

    ctx.match_recovery_exit_forced(args.match_id);
}
