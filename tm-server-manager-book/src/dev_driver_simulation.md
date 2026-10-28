# Simulation Testing

`tm-driver-sim` tests the driver and its modes with deterministic simulations. The real host with
the real wasm modes plays matches against a fake server, which plays the driver modescript and its
players. The matches come from seeds or are written down as scenarios. Checks run after every input
the driver gets, and a match has to replay and resume exactly. The reports of some matches are kept
as snapshots, so changes to a mode or the driver show up in review.

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

- The settings of the mode, within the ranges it accepts and so that matches end.
- One to three maps with two to five checkpoints, and a playlist of some of them or none.
- Up to sixteen players with a pace, how much their sectors vary and a chance to give up. Some join
  later, leave, only watch, have colored names or the same name as someone else.
- Chances of ties, idle players, holds that come twice, rounds an admin forces to end, start lines
  that do not reach the driver, the bridge restarting (the driver resumes by replaying its inputs),
  the modescript restarting and the playlist changing. Most worlds are plain, some have every fault.
- One or two matches.

Every part of a simulation draws from its own random stream forked from the seed, e.g. the races of
a player in a round. A change in one place does not reshuffle the whole match.

A scenario writes a world down instead, e.g. for a rule of a mode or a match that went wrong:

```text
# Two players drive exactly the same race. Accounts decide, not names.
mode knockout
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

After every input:

- Replies to holds end with their only `Continue`. A hold that comes again gets the same commands
  and no round result. `EndMap` and `EndMatch` only come at holds, round results only for rounds
  that counted, and the next map is on the server.
- What the driver publishes is what it has: the json of `SetStandings`, `SetRound` and `SetUsers`
  equals `standings()`, `last_round()` and `users()`.
- The standings have every player once, positions from 1 without gaps, players still playing before
  eliminated ones and spectators without a position. The last round places participants from 1.
- Nobody leaves the standings of a match, eliminated players and spectators stay what they are, and
  none of them can spawn. The driver knows the name of every player the server told it about.

At the end of every round and map, the checks of the mode, against the races the simulation knows:

- **Rounds:** finishers get the points of their place, in race order and with the time of their
  race. The points add up, ranks follow them, the points limit and the maps per match end the
  match and the rounds per map end maps.
- **TimeAttack:** every round ends the map, finishers are ordered by their best race and get the
  points of their place, the maps per match end the match.
- **ReverseCup:** players only lose points, players without points are labeled `Last Chance`, a
  player in last chance is eliminated by losing the most of a round, and the match ends once only
  the winners are left.
- **Knockout:** the players who lose a life are the last ones of the race and exactly as many as the
  settings take, with players without a finish and the player who keeps the last life. Lives stay
  within the settings and the match ends once one player is left.

The checks of a mode test properties of its rules. They do not compute the standings a second way,
except for the number of lives a knockout round takes, the heart of that mode.

Once a match ended, replaying every input has to give the same driver, and a driver resumed at a
random input has to answer the rest the same way.

To make sure the checks catch something, bugs were planted one at a time: a knockout taking one life
too many, points of the wrong place in rounds, the host stepping repeated holds again, the SDK
counting the slowest race as the best, a reverse cup eliminating every loss in last chance, the host
folding a restart into the finished race and a time attack playing one map too many. 200 seeds
caught each of them. A planted bug in the host that kept the spawn permissions of an old modescript
session went unnoticed, because every new session starts a new match, which clears them anyway.

## Snapshots

The report of a match has one block per round: the races as the driver saw them, the places of the
round, the standings after it and what the driver did. `tests/seeds.rs` keeps the matches of seeds 1
to 3 of every mode, `tests/scenarios.rs` every scenario in `tests/scenarios`. The snapshots are
[insta](https://insta.rs) files in `tests/snapshots`, the same in debug and release builds.

When a mode or the driver changes on purpose, review the new reports with `cargo insta review` or
accept them with `INSTA_UPDATE=always cargo test -p tm-driver-sim`. Changing the simulation itself
changes the matches of the seeds as well.

## Running

```sh
cargo test -p tm-driver-sim                                       # 25 seeds per mode, snapshots
SIM_SEEDS=2000 cargo test -p tm-driver-sim --release --test seeds # more seeds
cargo run -p tm-driver-sim --release -- knockout 1..10000         # a sweep, prints failures
cargo run -p tm-driver-sim -- knockout 42                         # the report of a seed
cargo run -p tm-driver-sim -- knockout 42 inputs.json             # and its inputs
cargo run -p tm-driver-sim -- tests/scenarios/<file>.scn          # the report of a scenario
```

The first run builds the modes for `wasm32-unknown-unknown` into `target/tm-driver-sim`. A sweep of
300 matches per mode takes a few seconds in release.

A failing seed prints its report up to the failure. To keep it as a test, write the part that
matters down as a scenario.

## Findings

- A reverse cup with no more participants than `number_of_winners` never starts: rounds in which
  nobody loses points do not count, so registration stays open and the match waits for more
  players. On a server nothing tells the players. `reverse_cup_waits_for_more_players_than_winners`
  keeps the behavior.

## Limits

- The fake server is a model of the modescript. Recorded matches of a real server would show where
  it differs (phase 5, parity with recorded matches): the runner could write its inputs, and
  replaying them would keep real matches as snapshots.
- The simulated bridge restarts at once, so no callbacks get lost while it is down. A real resume
  can miss some, e.g. the finish of a player in the round that runs.
- No laps, teams or Cup mode yet, and failing seeds are not shrunk to a smaller match.
