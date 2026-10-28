# Simulation Testing

Every mode tests itself with deterministic simulations. `tm-driver-sim` runs the real host with the
real wasm of a mode against a fake server, which plays the driver modescript and its players. The
matches come from seeds. Inputs can also be written down directly as logs, e.g. for a callback
order the fake server does not produce. Checks run after every input the driver gets, the rules of
the mode at the end of every round and map, and a match has to end in time, replay and resume
exactly. The reports of some matches are kept as snapshots, so changes to a mode or the driver show
up in review.

## Why it works

Deterministic simulation testing needs a system that is a function of its inputs. The driver is
one: modes run without WASI and with canonical NaNs, the host keeps no wall clock and no
randomness, and everything it gets is a `DriverInput` (see [Resuming](./dev_wasm_driver.md#resuming)).
So a seed always gives the same match, and every simulation is an input log the driver can replay.
The simulator limits calls with fuel only, without the wall clock deadline a server uses, so a busy
machine cannot fail a seed that passes on its own.

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
| `tm-driver-sim` | Nothing about modes. A library: the fake server, worlds, input logs, the checks every mode has to keep, the `Rules` trait and the tests every mode runs. |
| `tm-driver-modes/<mode>` | Unit tests of its rules with the SDK's harness in `src/tests.rs`, and simulations in `tests/simulation`: its `Rules`, random settings, logs and snapshots. |
| `tm-wasm-driver` | The host: holds, publishing, players, sessions, replays, resuming, the playlist, limits. It runs the rounds mode for that, with the callbacks of `tm_driver_sim::events`, but does not test its rules. |

A new mode, also one from outside the repository, brings its own simulation without touching the
simulator.

## The fake server

It follows `TM_Driver.Script.txt` at the level of the callbacks the driver handles:

- A match plays maps. A map waits for the driver at its start (`MapStart` hold), plays the warmup the
  driver set and then rounds until the driver ends it, waiting at the end of every round and map.
- Commands change it like the globals of the script: the round format, the warmup, spawn
  permissions, `EndMap` and `EndMatch` once the hold continues, the next map. A hold the driver does
  not continue is asked again, like after `S_DriverDecisionTimeout`. Once the reply to a hold
  reached the server, the bridge tells the driver with `DriverInput::Delivered`.
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
- Chances of ties, idle players and faults:
  - Holds that come twice, rounds an admin forces to end, start lines that do not reach the driver
    and the playlist changing.
  - The bridge going down. The reply to the input before may never reach the server, and while it
    is down the bridge misses race and player callbacks. It is back before any other callback,
    replays its inputs, gets `DriverInput::Resumed` and the players on the server.
  - The modescript restarting before a round, during a round or while a hold waits for its reply.
  - An admin pausing the match. The script answers with its status and skips the round, players
    drive without a round for a while, and the pause ends with the next status.

  About 40 % of worlds are plain, without any fault. The others draw every fault on its own, so
  some have all of them.
- One or two matches.

A full server is a random world with 120 players. Crowded matches stop after 40 rounds, which shows
enough of them and keeps the tests fast.

Every part of a world draws from its own random stream forked from the seed, e.g. every player or
the races of a player in a round. A change in one place does not reshuffle the whole match: one
racer more or less leaves the races of the others as they are.

## Input logs

An `InputLog` is the settings of a match and the inputs of its driver, with the replies that got
lost. It is json, so logs can be saved and loaded. `tm_driver_sim::events` writes them:

```rust
let mut log = InputLog::new(r#"{"time_limit_ms": 100000}"#);
log.players(&["a", "b"]).match_start().map_start("m1");
log.round(&[
    ("a", Race::finish(&[15_000, 31_000])),
    ("a", Race::finish(&[15_000, 30_000])),
    ("b", Race::finish(&[15_000, 30_500])),
    ("b", Race::dnf(&[14_000])),
]);
let outcome = simulator().log("the best race counts", &log);
```

A player named twice drives two races one after the other. The builder numbers the holds and
orders the callbacks of all races by their time. `hold_again`, `delivered`, `lose_reply`,
`resumed`, `restart_script`, `join` and `leave` put in what the fake server does not do on its own.

Unlike a simulation, a log does not follow the driver: who races and when maps end are written
down. Every check still runs on it, and inputs no server could have sent fail like a check: a player
who cannot spawn starting a race, a round starting after the driver ended the match, a hold older
than the last one. A log that no longer fits a mode fails instead of testing a match the driver
never asked for.

When a seed fails, `SIM_SAVE=<dir>` saves its inputs as a log, shrunk to the shortest log the
simulator finds that fails the same way. `Simulator::shrink` leaves out ever smaller runs of inputs
while the failure stays. Kept in `tests/simulation/logs`, a log is a regression test that does not
change when the simulator does, unlike a seed.

## What the checks see

The checks and the report only use what the driver got and the commands that reached the server,
like the modescript sees them: who can spawn, the round format, the next map, the standings the
manialinks show. The fake server is not asked. So the checks are the same for a simulation, for a
log written by hand and later for a match recorded on a server.

## Checks

After every input that can change the standings, the last round or the players, for every mode.
That leaves out waypoints, of which a full server sends thousands per round.

- Replies to holds end with their only `Continue`. A hold that comes again gets the same commands
  and no round result, or only its `Continue` once its reply reached the server. `EndMap` and
  `EndMatch` only come at holds, round results only for rounds that counted, and the next map is
  on the server.
- The server shows what the driver has: the standings, the last round and the players it got last
  equal `standings()`, `last_round()` and `users()`. So every change was published.
- The standings have every player once, positions from 1 without gaps, players still playing before
  eliminated ones and spectators without a position. The last round places participants from 1.
- Nobody leaves the standings of a match and eliminated players stay eliminated. Players still
  playing can spawn, everyone else cannot.
- A round that did not count, forced or without its round start, changes nobody's points or status.
- The driver knows the name of every player the server told it about.
- The standings show the match paused exactly while the modescript said it is.

After a reply that never reached the server, what the server shows is not compared, it still shows
what it had.

At the end of every round and map, the `Rules` of the mode, against the races the driver saw:

- **Every mode:** the round places its players in the order of their races, the one order every
  mode shares (`rules::in_race_order`): finishers by the time of the race that counts, then everyone
  else by the checkpoints they passed, ties by the times at the checkpoints from the last one
  backwards and finally by account. Everyone who raced and takes part has a place.
- **Rounds:** finishers get the points of their place with the time of their race. The points add
  up, ranks follow them, the points limit and the maps per match end the match and the rounds per
  map end maps.
- **TimeAttack:** every round ends the map, finishers are ordered by their best race and get the
  points of their place, the maps per match end the match.
- **ReverseCup:** players lose the points of their place, from the repartition for the players left
  with fast forward, and the dnf points without a finish, except in last chance with
  `only_least_checkpoints`. Players without points are labeled `Last Chance`, a player in last
  chance is eliminated by losing the most of a round, the rounds per map end maps and the match ends
  once only the winners are left. With no more participants than winners the match waits for
  players: no round counts and `info` says so. Spectators stay spectators.
- **Knockout:** the players who lose a life are the last ones of the race and exactly as many as the
  settings take, with players without a finish and the player who keeps the last life. Lives stay
  within the settings, the rounds per map end maps and the match ends once one player is left.
  Spectators stay spectators.

Every mode also tells within how many rounds a match ends (`Rules::round_limit`): the lives of every
participant in a knockout, the points of every participant in a reverse cup, the maps and their
rounds otherwise. A match that goes on longer fails, so a mode that stops making progress shows up.

Rules test properties against the races. They do not compute the standings a second way, except
for the order of a race, which every mode shares, the number of lives a knockout round takes and
the points a reverse cup place loses, the heart of those modes.

Once a match ended, replaying every input has to give the same driver, and a driver resumed at a
random input has to answer the rest the same way.

## Do the checks catch bugs?

Bugs were planted one at a time and the simulations of the affected modes ran on 150 seeds:

| Planted bug | Caught by |
|---|---|
| The SDK orders finishers slowest first | the race order, in every mode |
| The SDK orders players without a finish by the fewest checkpoints | the race order, in every mode |
| A knockout takes lives in rounds that do not count | rounds that do not count change nothing |
| A reverse cup takes the points of the next place | the points a place loses |
| A knockout plays a round too many on every map | the rounds per map |
| The host forgets to publish after a race start | what the server shows |
| A resumed host does not publish everything again | what the server shows, in 2 of 150 knockout seeds |
| The host sends every command again for a hold whose reply arrived | holds that come again |
| The host ignores the pause | the pause the standings show |
| A resumed host does not send the spawn permissions again | nothing |

The first three passed every seed before the race order and rounds that do not count were checked.
The last one cannot happen yet: modes only block players in the reply to a hold, and a hold whose
reply got lost is asked again and gets every command. Sending the permissions again on resume keeps
a mode safe that blocks players at other times.
Earlier, these were caught as well: a knockout taking one life too many, rounds giving the points of
the next place, the host stepping repeated holds again, the SDK counting the slowest race as the
best, a reverse cup eliminating every loss in last chance, a reverse cup not waiting for players,
the host folding a restart into the finished race and a time attack playing one map too many.

## Testing a mode

A mode keeps its simulation in `tests/simulation`:

```text
tests/simulation/main.rs        the simulator for the mode, its random settings and its tests
tests/simulation/rules.rs       its Rules
tests/simulation/logs/          saved logs, each a test
tests/simulation/snapshots/     the reports insta keeps
```

`main.rs` builds the mode for wasm, hands the simulator what only the mode knows and runs the tests
every mode runs:

```rust
fn simulator() -> &'static Simulator {
    static SIMULATOR: OnceLock<Simulator> = OnceLock::new();
    SIMULATOR.get_or_init(|| {
        Simulator::new("knockout", &tm_driver_sim::wasm!())
            .settings(settings)
            .rules(rules::Knockout::boxed)
    })
}

tm_driver_sim::simulation_tests!(simulator);
```

`simulation_tests!` adds random matches, full servers, the matches of seeds 1 to 3 as snapshots
and the saved logs. Tests of logs written by hand go next to it. `tm_driver_sim::wasm!()` builds the
crate whose tests call it for `wasm32-unknown-unknown` into `target/tmp/modes`, where the host
tests build the rounds mode as well. Without random settings a mode plays its defaults, without
`Rules` only the checks for every mode run.

## Snapshots

The report of a match has one block per round: the races as the driver saw them, the places of the
round, the standings after it and what the driver did. Notes tell what happened around it, e.g.
players joining, the bridge going down or callbacks it missed. The snapshots are
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
SIM_SEED_START=90000 cargo test --release -p tm-driver-mode-knockout --test simulation
SIM_SEED=42 cargo test -p tm-driver-mode-knockout --test simulation random_matches_follow -- --nocapture
SIM_SAVE=tests/simulation/logs cargo test -p tm-driver-mode-knockout --test simulation
```

`SIM_SEEDS` and `SIM_FULL_SERVERS` set how many random matches and full servers run,
`SIM_SEED_START` where their seeds start, e.g. a number that changes with every run of a CI job so
it explores new worlds. `SIM_SEED` runs only that seed and prints its report. A failure prints the
report of the first failing seed, and a panic, e.g. in the rules of a mode, fails its seed like a
check. With `SIM_SAVE`, the first failing seed is shrunk and saved as a log.

The first run builds the mode for wasm. In debug the simulations of a mode take about 10 seconds. A
sweep of thousands of matches is a matter of seconds in release.

## Findings

- A reverse cup with no more participants than `number_of_winners` never started: rounds in which
  nobody lost points did not count, so it waited for players without telling them. Waiting is now
  the rule: `info` shows it, and registration stays open across maps until a round counts.
- When many players join at once, e.g. in the first round on a full server, the host publishes the
  standings after every one of them. With 120 players that is 120 times the standings of up to 120
  entries within seconds. The netvars of a real server might need the host to publish less often.
- A hold the modescript asked again got every command again, also when the first reply had
  arrived. `EndMap` is the `NextMap` of the server, so it would skip a map, and a late `EndMatch`
  could end the next match. The bridge now tells the host with `DriverInput::Delivered`.
- Replies lost when the bridge went down were never sent again, so the manialinks could stay
  stale, e.g. without the name of a player who just joined. `DriverInput::Resumed` sends
  everything again, and `DriverInput::Players` now replaces who is on the server, for players who
  left meanwhile.
- The standings show the name of the map, but the host only published it at the hold after the map
  started. It publishes at `StartMap_Start` now.

## Limits

- The fake server is a model of the modescript. Recorded matches of a real server would show where
  it differs (phase 5, parity with recorded matches): the runner could write its inputs, and the
  checks run on them like on any log.
- A bridge that is down only misses race and player callbacks. It is back before a round, map or
  match starts, which a real bridge might not be.
- No laps, teams or Cup mode yet. Failing seeds are shrunk as logs, not to smaller worlds.
