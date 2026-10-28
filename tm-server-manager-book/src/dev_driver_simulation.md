# Simulation Testing

Every mode tests itself with deterministic simulations. `tm-driver-sim` runs the real host with the
real wasm of a mode against a fake server, which plays the driver modescript and its players. The
matches come from seeds or are written down as scenarios. Checks run after every input the driver
gets, the rules of the mode at the end of every round and map, and a match has to replay and resume
exactly. The reports of some matches are kept as snapshots, so changes to a mode or the driver show
up in review.

## Why it works

Deterministic simulation testing needs a system that is a function of its inputs. The driver is
one: modes run without WASI and with canonical NaNs, the host keeps no wall clock and no
randomness, and everything it gets is a `DriverInput` (see [Resuming](./dev_wasm_driver.md#resuming)).
So a seed always gives the same match, and every simulation is an input log the driver can replay.

## The layer

| Layer | Input | Simulated there? |
|---|---|---|
| Server and bridge | XML-RPC callbacks and methods | No. Transport without rules, tested by parsing tests. A fake XML-RPC server could test the bridge later. |
| **Driver** | `DriverInput` in, `DriverReply` out | **Yes.** The host and the wasm mode run like on a server. |
| Mode | WIT events | No. The SDK's `testing::Harness` runs modes natively for unit tests, but without the host. |

The fake server sends `DriverInput`s and follows the `DriverCommand`s the driver answers with, so
the driver steers the match like on a real server: eliminated players stop racing, `EndMap` ends the
map, `SetNextMap` picks the next one. Without that loop a generated stream would soon describe a
match the driver never asked for.

## Who tests what

| Crate | Tests |
|---|---|
| `tm-driver-sim` | Nothing about modes. A library: the fake server, worlds, scenarios, the checks every mode has to keep and the `Rules` trait. |
| `tm-driver-modes/<mode>` | Unit tests of its rules with the SDK's harness in `src/tests.rs`, and simulations in `tests/simulation`: its `Rules`, random settings, scenarios and snapshots. |
| `tm-wasm-driver` | The host: holds, publishing, players, sessions, replays, the playlist, limits. It runs the rounds mode for that, but does not test its rules. |

A new mode, also one from outside the repository, brings its own simulation without touching the
simulator.

## The fake server

It follows `TM_Driver.Script.txt` at the level of the callbacks the driver handles:

- A match plays maps. A map waits for the driver at its start (`MapStart` hold), plays the warmup the
  driver set and then rounds until the driver ends it, waiting at the end of every round and map.
- Commands change it like the globals of the script: the round format, the warmup, spawn
  permissions, `EndMap` and `EndMatch` once the hold continues, the next map. A hold the driver does
  not continue is asked again, like after `S_DriverDecisionTimeout`.
- Players are on the server when the driver loads (`DriverInput::Players`, like the runner hands
  them over), join, leave or only watch. Only players who can spawn race.
- Rounds of one try end once everyone finished or gave up, or when the finish timeout after the
  first finish ran out. Time attack rounds end with their time limit and players start again after
  every race. Warmups with a duration work like time attack rounds, warmups without one like rounds
  of one try. Players who do not move give up after 30 seconds, like idle players being kicked to
  spectators.

## Time

Time is a virtual clock in milliseconds. A race is planned when its player starts, and its
checkpoints happen at their race times. The happening that comes first happens next, so the
waypoints of all players interleave like on a server. Happenings at the same moment come in a
random order, which gives every permutation of a stream that the rules allow. The finish timeout
and time limits cut off the races still running. The driver itself never sees time except in the
callbacks, so holds are answered at once.

## Worlds

A random world varies everything with its seed, like swarm testing:

- The settings of the mode, from the random settings the mode hands to the simulator.
- One to three maps with two to five checkpoints, and a playlist of some of them or none.
- Mostly up to sixteen players, sometimes a crowd of up to 120, the most a server holds. Every
  player has a pace, how much their sectors vary and a chance to give up. Some join later, leave,
  only watch, have colored names or the same name as someone else.
- Chances of ties, idle players, holds that come twice, rounds an admin forces to end, start lines
  that do not reach the driver, the bridge restarting (the driver resumes by replaying its inputs),
  the modescript restarting and the playlist changing. Most worlds are plain, some have every fault.
- One or two matches.

A full server is a random world with 120 players. Crowded matches stop after 40 rounds, which shows
enough of them and keeps the tests fast.

Every part of a world draws from its own random stream forked from the seed, e.g. every player or
the races of a player in a round. A change in one place does not reshuffle the whole match.

A scenario writes a world down instead, e.g. for a rule of a mode or a match that went wrong:

```text
# Two players drive exactly the same race. Accounts decide, not names.
settings {"rounds_without_elimination": 0}
maps m1:10/10/10
players x=bob y=Alice z
round x 10/20/30 | y 10/20/30 | z 29
round z 30 | x 31
```

Races are finish times, checkpoint times, `dnf <checkpoints>` or `idle`. Players can `join` and
`leave` before rounds, and `repeat-holds`, `restart-bridge`, `restart-script` and `playlist` put
in faults. The documentation of `tm_driver_sim::scenario` has the whole format. The commands of the
driver still steer the match, so a scenario only says how players race, not who races.

## Checks

After every input, for every mode:

- Replies to holds end with their only `Continue`. A hold that comes again gets the same commands
  and no round result. `EndMap` and `EndMatch` only come at holds, round results only for rounds
  that counted, and the next map is on the server.
- What the driver publishes is what it has: the json of `SetStandings`, `SetRound` and `SetUsers`
  equals `standings()`, `last_round()` and `users()`.
- The standings have every player once, positions from 1 without gaps, players still playing before
  eliminated ones and spectators without a position. The last round places participants from 1.
- Nobody leaves the standings of a match, eliminated players and spectators stay what they are, and
  none of them can spawn. The driver knows the name of every player the server told it about.

The driver publishes whatever changes, so most of these run only after replies with commands. That
keeps waypoints cheap, of which a full server sends thousands per round.

At the end of every round and map, the `Rules` of the mode, against the races the simulation knows:

- **Rounds:** finishers get the points of their place, in race order and with the time of their
  race. The points add up, ranks follow them, the points limit and the maps per match end the
  match and the rounds per map end maps.
- **TimeAttack:** every round ends the map, finishers are ordered by their best race and get the
  points of their place, the maps per match end the match.
- **ReverseCup:** players only lose points, players without points are labeled `Last Chance`, a
  player in last chance is eliminated by losing the most of a round, and the match ends once only
  the winners are left. With no more participants than winners the match waits for players: no
  round counts and `info` says so.
- **Knockout:** the players who lose a life are the last ones of the race and exactly as many as the
  settings take, with players without a finish and the player who keeps the last life. Lives stay
  within the settings and the match ends once one player is left.

Rules test properties against the races. They do not compute the standings a second way, except for
the number of lives a knockout round takes, the heart of that mode.

Once a match ended, replaying every input has to give the same driver, and a driver resumed at a
random input has to answer the rest the same way.

To make sure the checks catch something, bugs were planted one at a time and the simulation tests of
the affected mode ran on 150 seeds: a knockout taking one life too many, rounds giving the points of
the next place, the host stepping repeated holds again, the SDK counting the slowest race as the
best, a reverse cup eliminating every loss in last chance, a reverse cup not waiting for players,
the host folding a restart into the finished race and a time attack playing one map too many. All of
them were caught. The first check of waiting missed its bug: it looked at the standings after the
round, which the wrong round had already taken out of registration.

## Testing a mode

A mode keeps its simulation in `tests/simulation`:

```text
tests/simulation/main.rs        the simulator for the mode, its random settings and the tests
tests/simulation/rules.rs       its Rules
tests/simulation/scenarios/     scenarios, one per file
tests/simulation/snapshots/     the reports insta keeps
```

`main.rs` builds the mode for wasm and hands the simulator what only the mode knows:

```rust
fn simulator() -> &'static Simulator {
    static SIMULATOR: OnceLock<Simulator> = OnceLock::new();
    SIMULATOR.get_or_init(|| {
        Simulator::new("knockout", &tm_driver_sim::wasm!())
            .settings(settings)
            .rules(rules::Knockout::boxed)
    })
}

#[test]
fn random_matches_follow_the_rules() {
    simulator().check(seeds("SIM_SEEDS", 100..125), Simulator::world);
}
```

`tm_driver_sim::wasm!()` builds the crate whose tests call it for `wasm32-unknown-unknown` into
`target/tmp/modes`, where the host tests build the rounds mode as well. Without random settings a
mode plays its defaults, without `Rules` only the checks for every mode run. The modes so far test
random matches, full servers, the matches of seeds 1 to 3 as snapshots and every scenario as a
snapshot.

## Snapshots

The report of a match has one block per round: the races as the driver saw them, the places of the
round, the standings after it and what the driver did. The snapshots are
[insta](https://insta.rs) files in `tests/simulation/snapshots` of every mode, the same in debug and
release builds. Reports of full servers are not kept, they are hundreds of kilobytes.

When a mode or the driver changes on purpose, review the new reports with `cargo insta review` or
accept them with `INSTA_UPDATE=always`. Changing the simulator itself changes the matches of the
seeds as well.

## Running

```sh
cargo test -p tm-driver-mode-knockout                     # unit tests and simulations
SIM_SEEDS=5000 cargo test --release -p tm-driver-mode-knockout --test simulation
SIM_FULL_SERVERS=50 cargo test --release -p tm-driver-mode-knockout --test simulation
SIM_SEED=42 cargo test -p tm-driver-mode-knockout --test simulation random_matches_follow -- --nocapture
```

`SIM_SEEDS` and `SIM_FULL_SERVERS` set how many random matches and full servers run, `SIM_SEED`
runs only that seed and prints its report. A failure prints the report of the first failing seed.
To keep a case as a test, write the part that matters down as a scenario.

The first run builds the mode for wasm. In debug the simulations of a mode take about 20 seconds, a
full server about 13. A sweep of thousands of matches is a matter of minutes in release.

## Findings

- A reverse cup with no more participants than `number_of_winners` never started: rounds in which
  nobody lost points did not count, so it waited for players without telling them. Waiting is now
  the rule: `info` shows it, and registration stays open across maps until a round counts.
- When many players join at once, e.g. in the first round on a full server, the host publishes the
  standings after every one of them. With 120 players that is 120 times the standings of up to 120
  entries within seconds. The netvars of a real server might need the host to publish less often.

## Limits

- The fake server is a model of the modescript. Recorded matches of a real server would show where
  it differs (phase 5, parity with recorded matches): the runner could write its inputs, and
  replaying them would keep real matches as snapshots.
- The simulated bridge restarts at once, so no callbacks get lost while it is down. A real resume
  can miss some, e.g. the finish of a player in the round that runs.
- No laps, teams or Cup mode yet, and failing seeds are not shrunk to a smaller match.
