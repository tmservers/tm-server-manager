//! What the manialinks show, read back from what the driver published for the
//! netvars the way a manialink reads them: entries as columns, players by their
//! index in the netvars of the players.

use tm_server_types::method::{
    DriverEntry, DriverLabelTone, DriverNetEntries, DriverNetRound, DriverNetStandings,
    DriverNetUsers, DriverRound, DriverStandings, DriverStatus, DriverUser, DriverUsers,
};

/// The players by index. Every index is a player of its own.
pub(crate) fn users(users: &DriverNetUsers) -> Result<DriverUsers, String> {
    let count = users.indices.len();
    let lengths = [
        users.account_ids.len(),
        users.names.len(),
        users.club_tags.len(),
        users.zones.len(),
    ];
    if lengths.iter().any(|&length| length != count) {
        return Err("The columns of the published players differ in length".into());
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut shown = Vec::new();
    for (column, &index) in users.indices.iter().enumerate() {
        if !seen.insert(index) {
            return Err(format!("Two published players have the index {index}"));
        }
        shown.push(DriverUser {
            index,
            account_id: users.account_ids[column].clone(),
            name: users.names[column].clone(),
            club_tag: users.club_tags[column].clone(),
            zone: users.zones[column].clone(),
        });
    }
    Ok(DriverUsers { users: shown })
}

pub(crate) fn standings(
    standings: &DriverNetStandings,
    users: &DriverUsers,
) -> Result<DriverStandings, String> {
    Ok(DriverStandings {
        title: standings.title.clone(),
        info: standings.info.clone(),
        rounds_on_map: standings.rounds_on_map,
        maps_played: standings.maps_played,
        map_name: standings.map_name.clone(),
        map_count: standings.map_count,
        paused: standings.paused,
        entries: entries(&standings.entries, users, "standings")?,
    })
}

pub(crate) fn round(round: &DriverNetRound, users: &DriverUsers) -> Result<DriverRound, String> {
    Ok(DriverRound {
        number: round.number,
        live: round.live,
        entries: entries(&round.entries, users, "round")?,
    })
}

/// The entries of the columns. Every player has to be one of the published players.
fn entries(
    entries: &DriverNetEntries,
    users: &DriverUsers,
    what: &str,
) -> Result<Vec<DriverEntry>, String> {
    let count = entries.players.len();
    let lengths = [
        entries.positions.len(),
        entries.scores.len(),
        entries.times.len(),
        entries.statuses.len(),
    ];
    if lengths.iter().any(|&length| length != count)
        || entries.labels.len() != entries.labeled.len()
        || entries.label_tones.len() != entries.labeled.len()
    {
        return Err(format!(
            "The columns of the published {what} differ in length"
        ));
    }
    (0..count)
        .map(|column| {
            let index = entries.players[column];
            let account_id = users
                .users
                .iter()
                .find(|user| user.index == index)
                .map(|user| user.account_id.clone())
                .ok_or(format!(
                    "The published {what} show player {index}, who is no published player"
                ))?;
            let status = DriverStatus::from_code(entries.statuses[column])
                .ok_or(format!("The published {what} have an unknown status"))?;
            let (label, label_tone) = match entries.labeled.iter().position(|&p| p == index) {
                Some(labeled) => (
                    entries.labels[labeled].clone(),
                    DriverLabelTone::from_code(entries.label_tones[labeled])
                        .ok_or(format!("The published {what} have an unknown tone"))?,
                ),
                None => (String::new(), DriverLabelTone::Neutral),
            };
            Ok(DriverEntry {
                account_id,
                position: entries.positions[column],
                score: entries.scores[column],
                time: entries.times[column],
                status,
                label,
                label_tone,
            })
        })
        .collect()
}
