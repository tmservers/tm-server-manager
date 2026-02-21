use spacetimedb::{ReducerContext, ScheduleAt, Table, table};

use crate::tournament::{TournamentStatus, TournamentV1, tab_tournament};

#[table(accessor= tab_tournament_status_schedule, scheduled(on_tournament_status_schedule_triggered))]
pub struct TournamentStatusScheduleV1 {
    #[primary_key]
    #[auto_inc]
    pub scheduled_id: u64,

    pub scheduled_at: ScheduleAt,

    #[unique]
    pub tournament_id: u32,
    pub new_status: TournamentStatus,
}

impl TournamentStatusScheduleV1 {
    pub(crate) fn get_tournament(&self) -> u32 {
        self.tournament_id
    }

    pub(crate) fn get_new_status(&self) -> TournamentStatus {
        self.new_status
    }
}

#[cfg_attr(feature = "spacetime", spacetimedb::reducer)]
fn on_tournament_status_schedule_triggered(
    ctx: &ReducerContext,
    arg: TournamentStatusScheduleV1,
) -> Result<(), String> {
    if !ctx.sender_auth().is_internal() {
        return Err("Only the Databse is permitted to call this reducer.".into());
    }

    let Some(mut tournament) = ctx.db.tab_tournament().id().find(arg.get_tournament()) else {
        return Err("Invalid tournament".into());
    };

    tournament.status = arg.get_new_status();

    tournament = ctx.db.tab_tournament().id().update(tournament);

    // Schedule the next status change if applicable
    schedule_tournament_status_change(ctx, &tournament)?;

    Ok(())
}

// Internal function to automatically schedule the next status change for a tournament
pub fn schedule_tournament_status_change(
    ctx: &ReducerContext,
    tournament: &TournamentV1,
) -> Result<(), String> {
    let (new_status, scheduled_at) = match tournament.status {
        TournamentStatus::Announced => (TournamentStatus::Ongoing, tournament.starting_at),
        TournamentStatus::Ongoing => (TournamentStatus::Ended, tournament.ending_at),
        _ => {
            // No scheduling needed for Planning or Ended status
            return Ok(());
        }
    };

    let schedule = TournamentStatusScheduleV1 {
        scheduled_id: 0,
        scheduled_at: ScheduleAt::Time(scheduled_at),
        tournament_id: tournament.id,
        new_status,
    };

    // If there is an existing scheduled status change for this tournament, remove it
    ctx.db
        .tab_tournament_status_schedule()
        .tournament_id()
        .delete(schedule.tournament_id);

    ctx.db
        .tab_tournament_status_schedule()
        .try_insert(schedule)?;

    Ok(())
}
