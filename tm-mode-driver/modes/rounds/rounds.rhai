// The rounds mode as a script: the same rules as the built-in mode, which the
// simulations hold it to. A starting point for modes of a competition.
//
// Finishers get points by their place, the map ends after a number of rounds and
// the match after a number of maps or once a player reached the points limit.

fn name() { "Rounds" }

fn defaults() {
    #{
        // Points by finish position. Finishers beyond the list get the last value.
        points_repartition: [10, 6, 4, 3, 2, 1],
        // Valid rounds played on every map. 0 means no limit.
        rounds_per_map: 5,
        // Maps played in the match. Without it the playlist once, 0 means no limit.
        maps_per_match: (),
        // The match ends once a player reached this many points. 0 disables it.
        points_limit: 0,
        // "automatic" without it, "disabled" or #{ milliseconds: 5000 }.
        finish_timeout: (),
        // The warmup before the first round of every map.
        warmup: #{ rounds: 0, duration_ms: 0, timeout_ms: () },
    }
}

fn reloadable() {
    ["points_repartition", "rounds_per_map", "maps_per_match", "points_limit", "finish_timeout", "warmup"]
}

fn validate(settings) {
    if settings.rounds_per_map < 0 {
        throw "rounds_per_map cannot be negative";
    }
    for points in settings.points_repartition {
        if type_of(points) != "i32" {
            throw "points_repartition has to be a list of numbers";
        }
    }
    let maps = settings.maps_per_match;
    if maps != () && (type_of(maps) != "i32" || maps < 0) {
        throw "maps_per_match has to be a number of maps";
    }
    // Both reject what they cannot use.
    single_attempt(finish_timeout(settings));
    warm_up(settings.warmup);
}

fn step(cx, standings, event) {
    switch event.kind {
        // The script of the server waits here, so this is where the rules of the map
        // are set. New settings send them again.
        "map_start" | "settings_changed" => {
            standings.info = info(cx, standings);
            [single_attempt(finish_timeout(cx.settings)), warm_up(cx.settings.warmup)]
        }
        "round_end" => round_end(cx, standings, event.round),
        "map_end" => {
            if match_is_over(cx, standings) {
                standings.crown_leaders();
                [end_match()]
            } else {
                []
            }
        }
        // Everyone who races shows up in the standings before the first round ends.
        "race_start" => {
            standings.join(event.account_id);
            []
        }
        _ => [],
    }
}

// The finishers so far with the points they would get.
fn live_round(cx, standings, round) {
    // Everyone else who raced joins at the round end.
    let taking_part = round.taking_part(standings);
    let points = #{};
    for finisher in finisher_points(cx, taking_part) {
        points[finisher.account] = finisher.points;
    }
    standings.placed(taking_part, "last", points)
}

fn round_end(cx, standings, round) {
    // Invalid rounds do not count.
    if round.invalid {
        return [];
    }

    // Everyone who raced shows up in the standings, with points or without.
    for racer in round.racers {
        standings.join(racer.account_id);
    }
    for finisher in finisher_points(cx, round) {
        standings.award(finisher.account, finisher.points);
    }

    standings.rounds_on_map += 1;
    // Nobody ties: the race of the round tells players with the same points apart.
    standings.rank(round, "last");
    standings.place_round(round, "last");
    standings.info = info(cx, standings);

    let rounds_per_map = cx.settings.rounds_per_map;
    if points_limit_reached(cx, standings) {
        standings.crown_leaders();
        [end_match()]
    } else if rounds_per_map > 0 && standings.rounds_on_map >= rounds_per_map {
        [end_map()]
    } else {
        []
    }
}

// The finishers of a round in the order of the race with their points.
fn finisher_points(cx, round) {
    let repartition = cx.settings.points_repartition;
    let finishers = [];
    for (account, place) in race_order(round.finishers("last"), round, "last") {
        let points = if place < repartition.len() {
            repartition[place]
        } else if repartition.len() > 0 {
            repartition[repartition.len() - 1]
        } else {
            0
        };
        finishers.push(#{ account: account, points: points });
    }
    finishers
}

fn points_limit_reached(cx, standings) {
    let limit = cx.settings.points_limit;
    limit > 0 && standings.entries.some(|entry| entry.score >= limit)
}

fn match_is_over(cx, standings) {
    if points_limit_reached(cx, standings) {
        return true;
    }
    let maps = cx.maps_per_match(cx.settings.maps_per_match);
    if maps >= 1 {
        standings.maps_played >= maps
    } else {
        // Without any limit the match ends after the first map.
        cx.settings.rounds_per_map == 0 && cx.settings.points_limit <= 0
    }
}

fn info(cx, standings) {
    let limit = cx.settings.rounds_per_map;
    let rounds = if limit == 0 {
        `${standings.rounds_on_map} rounds`
    } else {
        `${standings.rounds_on_map} / ${limit} rounds`
    };
    let map = standings.maps_played + 1;
    let maps = cx.maps_per_match(cx.settings.maps_per_match);
    if maps == 1 {
        rounds
    } else if maps == 0 {
        `Map ${map}, ${rounds}`
    } else {
        `Map ${map} / ${maps}, ${rounds}`
    }
}

fn finish_timeout(settings) {
    if settings.finish_timeout == () { "automatic" } else { settings.finish_timeout }
}
