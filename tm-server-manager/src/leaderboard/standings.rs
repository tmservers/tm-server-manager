use std::collections::HashSet;

use tm_server_types::config::{ModeSettingsV2, TmMode};

use crate::{
    competition::node::NodeHandle,
    leaderboard::{LbEntry, LbEntryKind, LbParams, group_by, renumber_positions},
    raw_server::config::RawServerContigRead,
    tm_match::{MatchRead, state::MatchStateRead},
};

pub(crate) trait LbStandings {
    /// The standings of every match in the leaderboard, one after another in the order the matches first appear.
    /// Entries which already are standings are only sorted, so applying this again changes nothing.
    fn match_standings(&self, ctx: &impl spacetimedb::CtxDbRead) -> Vec<LbEntry>;
}

impl LbStandings for Vec<LbEntry> {
    fn match_standings(&self, ctx: &impl spacetimedb::CtxDbRead) -> Vec<LbEntry> {
        group_by(self.iter().copied(), |e| (e.get_node(), e.kind))
            .into_iter()
            .flat_map(|((node, kind), entries)| match kind {
                LbEntryKind::Round => round_standings(ctx, node, entries),
                LbEntryKind::Standing => sorted_standings(entries),
            })
            .collect()
    }
}

fn sorted_standings(mut entries: Vec<LbEntry>) -> Vec<LbEntry> {
    entries.sort_by_key(|e| e.position);
    entries
}

/// The rounds of a single match, its mode decides how they become standings.
fn round_standings(
    ctx: &impl spacetimedb::CtxDbRead,
    node: NodeHandle,
    entries: Vec<LbEntry>,
) -> Vec<LbEntry> {
    let Some(mode) = match_mode(ctx, node) else {
        log::error!("Rounds of {node:?} have no match mode, they are kept as they are.");
        return sorted_standings(entries);
    };

    let mut standings = match mode {
        TmMode::Rounds => rounds(entries),
        TmMode::ReverseCup => reverse_cup(ctx, node, entries),
        TmMode::Knockout => knockout(entries),
        TmMode::TimeAttack => time_attack(entries),
        TmMode::Unknown => sorted_standings(entries),
    };

    for standing in &mut standings {
        standing.kind = LbEntryKind::Standing;
    }
    standings
}

fn match_mode(ctx: &impl spacetimedb::CtxDbRead, node: NodeHandle) -> Option<TmMode> {
    let NodeHandle::MatchV1(match_id) = node else {
        return None;
    };
    Some(ctx.match_state_find(match_id)?.get_mode())
}

/// Folds the rounds of every user into their first one.
fn per_user(entries: Vec<LbEntry>, mut fold: impl FnMut(&mut LbEntry, LbEntry)) -> Vec<LbEntry> {
    group_by(entries, |e| e.user_id)
        .into_iter()
        .map(|(_, rounds)| {
            let mut rounds = rounds.into_iter();
            let mut standing = rounds.next().unwrap();
            for round in rounds {
                fold(&mut standing, round);
            }
            standing
        })
        .collect()
}

fn latest_round(standing: &mut LbEntry, round: &LbEntry) {
    if round.round > standing.round {
        standing.round = round.round;
    }
}

fn rounds(entries: Vec<LbEntry>) -> Vec<LbEntry> {
    let mut standings = per_user(entries, |standing, round| {
        standing.score += round.score;
        latest_round(standing, &round);
    });

    LbParams::Score.sort(&mut standings);
    renumber_positions(&mut standings);
    standings
}

fn reverse_cup(
    ctx: &impl spacetimedb::CtxDbRead,
    node: NodeHandle,
    entries: Vec<LbEntry>,
) -> Vec<LbEntry> {
    let eliminated: HashSet<u32> = entries
        .iter()
        .filter(|e| e.score <= -2000)
        .map(|e| e.user_id)
        .collect();

    let mut standings = per_user(entries, |standing, round| {
        standing.score += round.score;
        latest_round(standing, &round);
    });

    let starting_points = reverse_cup_starting_points(ctx, node).unwrap_or_else(|| {
        log::error!("Could not find the reverse cup config of {node:?}.");
        0
    });

    for standing in standings.iter_mut() {
        standing.score += starting_points;

        if !eliminated.contains(&standing.user_id) || standing.score > 0 {
            standing.round += 1;
        }
    }

    standings.sort_by_key(|v| -(v.round as i32));
    renumber_positions(&mut standings);
    standings
}

fn reverse_cup_starting_points(ctx: &impl spacetimedb::CtxDbRead, node: NodeHandle) -> Option<i32> {
    let NodeHandle::MatchV1(match_id) = node else {
        return None;
    };
    let tm_match = ctx.match_find(match_id)?;
    let config = ctx
        .raw_server_config(tm_match.get_active_config_id())
        .ok()?;
    match config.get_mode() {
        ModeSettingsV2::ReverseCup(reverse_cup) => Some(reverse_cup.starting_points),
        _ => None,
    }
}

fn knockout(entries: Vec<LbEntry>) -> Vec<LbEntry> {
    let mut standings = per_user(entries, |standing, round| {
        // The round in which the player got knocked out.
        if round.score == -1 {
            *standing = round;
        }
        latest_round(standing, &round);
    });

    standings.sort_by_key(|v| -(v.round as i32));
    renumber_positions(&mut standings);
    standings
}

fn time_attack(entries: Vec<LbEntry>) -> Vec<LbEntry> {
    let mut standings = per_user(entries, |standing, round| {
        if LbParams::time_key(round.time) < LbParams::time_key(standing.time) {
            *standing = round;
        }
    });

    LbParams::Time.sort(&mut standings);
    renumber_positions(&mut standings);
    standings
}
