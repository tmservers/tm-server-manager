#[cfg(feature = "spacetime")]
#[cfg_attr(feature = "spacetime", spacetimedb::table(name = competition_schedule, scheduled(on_tournament_event_schedule)))]
struct CompetitionSchedule {
    // Mandatory fields:
    // ============================
    /// An identifier for the scheduled reducer call.
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,

    /// Information about when the reducer should be called.
    scheduled_at: spacetimedb::ScheduleAt,

    // Custom fields:
    // ============================
    /// The text of the scheduled message to send.
    text: String,
}

#[cfg(feature = "spacetime")]
#[cfg_attr(feature = "spacetime", spacetimedb::reducer)]
fn on_tournament_event_schedule(
    ctx: &spacetimedb::ReducerContext,
    arg: CompetitionSchedule,
) -> Result<(), String> {
    /* let message_to_send = arg.text;

    _ = send_message(ctx, message_to_send); */

    Ok(())
}
