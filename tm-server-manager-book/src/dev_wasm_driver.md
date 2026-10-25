# WASM Mode Driver

Status: phases 0–3 and the TimeAttack mode are implemented on the `wasm-driver/*` branches.
Everything except the modescript is covered by tests, but nothing has run against a real server
yet. See [Next steps](#next-steps).

## Idea

Trackmania servers are driven by modescripts. Today we load a different premade script per mode
(Rounds, Cup, ReverseCup, TimeAttack, ...). The bridge swaps scripts on every config change, which is
convoluted and can crash when swapping between incompatible modes.

Instead, one **generic driver modescript** stays loaded on every server. It keeps no mode state
(no points, no elimination rules, no end conditions). At a few key moments it emits typed `Driver.*`
callbacks and waits for decisions. Those decisions come from a **WASM blob** executed by
`tm-wasm-driver` inside the bridge, which sends typed commands back into the script.

## Decisions

| Topic | Decision |
|---|---|
| Where the logic runs | `wasmtime` inside the bridge (`tm-wasm-driver`). Decisions are local and fast. |
| Durability | The driver is a deterministic state machine with `snapshot`/`restore`. Snapshots are stored in SpacetimeDB at round boundaries so a crashed bridge can resume. |
| Who writes blobs | First-party modes first (in-repo Rust crates). The host is sandboxed from day one (fuel, epoch deadline, memory cap, no WASI) so user uploads can be added later. |
| ABI | WIT component model (`tm-wasm-driver/wit/driver.wit`). |
| Scoring source of truth | The driver. It decides points; the script only displays them. |
| Boundary typing | No `Event::Custom` and no hand-built JSON. Every callback and command is a Rust type in `tm-server-types`. |

## Architecture

```text
TM server ── TM_Driver.Script.txt (#Extends TrackmaniaRoundsBase, loaded once)
   │ Driver.* callbacks                        ▲ Driver.* TriggerModeScriptEventArray
   ▼                                           │
tm-server-controller   Event::Driver(DriverEvent)  /  driver_commands(&[DriverCommand])
   ▼                                           │
tm-wasm-driver         one wasm instance per match, seq dedup, auto-ack, snapshots
   ▼                                           ▲
tm-server-bridge ── post_event / snapshots ──▶ tm-server-manager
```

## Typed boundary

### Script → controller: `Event::Driver(DriverEvent)`

All driver callbacks are one nested variant of `tm_server_types::event::Event`:

```rust
Event::Driver(DriverEvent::RoundEnd(DriverRoundEnd { .. }))
```

Nesting instead of flat `Event::DriverX` variants means `DriverEvent` can be converted to the WIT
event type with an exhaustive `match` (no `_ =>` arm). Adding a callback on one side fails to
compile until the other side handles it.

Every callback carries:

- `session`: random number chosen when the script starts. A new session resets the host's
  sequence tracking (server or script restart).
- `seq`: increases by one per callback within a session.
- `time`: server time (`Now`) in milliseconds.

| Callback | Decision | Payload |
|---|---|---|
| `Driver.Hello` | no | `script_version` |
| `Driver.MatchStart` | yes | `players` |
| `Driver.MapStart` | yes | `map_uid`, `map_name`, `players` |
| `Driver.RoundStart` | yes | `round`, `players` |
| `Driver.RoundEnd` | yes | `round`, `forced`, `results` (best time of the round or DNF, checkpoints of that race) |
| `Driver.MapEnd` | yes | – |
| `Driver.MatchEnd` | yes | – |
| `Driver.PlayerJoin` | no | `player` |
| `Driver.PlayerLeave` | no | `account_id` |

**Decision callbacks block the script** until `Driver.Ack` with the same `seq` arrives. While
waiting, the script keeps yielding (`MB_Yield`) so commands can be applied. If no ack arrives
within `S_DriverDecisionTimeout`, the script shows a waiting message and re-sends the callback
periodically. It never guesses. The host answers a re-sent callback with the cached reply
instead of stepping the mode twice, so points are never applied twice.

There is no separate `Decide` callback: the lifecycle callbacks themselves are the decision points.

### Controller → script: `DriverCommand`

`tm_server_types::method::DriverCommand` is sent with one `TriggerModeScriptEventArray` per command.
It uses `Text[]` params like Nadeo's own `Trackmania.*` methods, so the script never has to parse
JSON for commands. `DriverCommand::method_name()` and `DriverCommand::params()` are the only place
that encodes them.

| Command | Params |
|---|---|
| `Driver.Ack` | `seq` (host only, never produced by a mode) |
| `Driver.SetPlayerPoints` | `account_id`, `round`, `map`, `match` (empty string = keep) |
| `Driver.SetCanSpawn` | `account_id`, `1`/`0` |
| `Driver.SetFinishTimeout` | `auto`, `off` or milliseconds |
| `Driver.ForceEndRound` | – |
| `Driver.EndMap` | – |
| `Driver.EndMatch` | – |
| `Driver.SetMessage` | text (empty clears) |
| `Driver.SetLayer` | `name`, manialink |
| `Driver.RemoveLayer` | `name` |
| `Driver.SetNetState` | JSON text |
| `Driver.SetRoundFormat` | `single_attempt`, or `time_attack` and the time limit in milliseconds |

`EndMap`/`EndMatch` only set flags. The script acts on them at the right moment in its flow,
after the current decision was acked.

The round format, finish timeout and spawn permissions reset to their defaults when a match
starts. A config change does **not** start a new match: NextMap only ends the map, so the
modescript keeps its match and its state. Modes therefore send their round rules on `MapStart`
as well, not only on `MatchStart`.

Planned later: `SetTeamPoints`, `SetPlayerTeam` (team modes).

## Generic modescript

`tm-server-types/external_modes/TM_Driver.Script.txt`, installed as `Modes/Trackmania/TM_Driver`.

- Extends `TrackmaniaRoundsBase`, so Nadeo still handles spawning, checkpoints, finishes, replays,
  records, warmup and the standard `Trackmania.Event.*` callbacks.
- Points are only ever set by `Driver.SetPlayerPoints`. After the `RoundEnd` decision is applied the
  script sends the standard `Trackmania.Scores` (`PreEndRound`) callback, so the manager keeps
  recording round points through its existing path.
- UI: `Driver.SetNetState` writes `netwrite Text Net_Driver_State for Teams[0]` and bumps
  `Net_Driver_StateSerial`. Manialinks pushed with `Driver.SetLayer` read it with `netread`.
  Nadeo's scoreboard keeps working because points live in the normal `Scores` lib.
- TimeAttack runs on RoundsBase as a time attack round (see below), so one base covers every
  mode we use.

## TimeAttack

There is no separate TimeAttack script. TimeAttack is a round format of the generic modescript,
the same trick `TM_RoundsThenTimeattack` from the community modes uses:

1. The mode replies to `MatchStart` and `MapStart` with
   `SetRoundFormat(TimeAttack(time_limit_ms))`.
2. When the next round starts, the script sets `EndTime = StartTime + time_limit` and shows
   the time limit and best times in the scores table.
3. During the round, players who finished or gave up are started again with `Race::Start`, so
   everyone races as often as they like. The round does not end when everyone finished, only
   when the time limit runs out (or an admin forces it).
4. The script tracks the best race of every player. `RoundEnd` reports that best time and its
   checkpoints, finishers sorted by time first. The script is the source of truth for times, so
   no per-finish callbacks are needed and a bridge restart during the round loses nothing.
5. The time attack mode turns the ranking into points and replies with `EndMap`. On `MapEnd` it
   ends the match after `maps_per_match` maps.

Points exist only because `TmMode::Driver` ranks by summed round points (phase 4 replaces this
with explicit driver results). By default the best of N players gets N points, so the points
keep the order of the times. The manager still records each player's best time from the
standard waypoint callbacks.

## WIT interface and host

`tm-wasm-driver/wit/driver.wit` defines world `mode`:

```wit
export init: func(settings: string) -> result<_, string>;
export step: func(event: event) -> list<command>;
export snapshot: func() -> list<u8>;
export restore: func(snapshot: list<u8>) -> result<_, string>;
import host;  // log
```

- The guest-facing `event`/`command` types mirror `DriverEvent`/`DriverCommand` without the
  transport fields (`session`, `seq`, `Ack`). The host converts with exhaustive `From` impls.
- The host accepts a component or a core module built for `wasm32-unknown-unknown` with the SDK
  (core modules are wrapped with `wit-component` on load).
- Sandbox: no WASI imports, fuel per call, epoch deadline per call, memory limit. Time only comes
  from event timestamps, so a mode replays identically from a recorded event log.
- `Driver::handle(&DriverEvent)` returns the commands to send. For decision callbacks it appends
  `Ack(seq)` and caches the reply for re-sends.

## Guest SDK and modes

- `tm-driver-sdk`: wit-bindgen bindings plus a `Mode` trait and `export_mode!` macro.
  Mode logic compiles natively too, so it can be unit tested without wasm.
- `tm-driver-modes/rounds`: first-party Rounds mode (points repartition, rounds per map, maps per
  match, points limit, finish timeout).
- `tm-driver-modes/time-attack`: first-party TimeAttack mode (time limit, maps per match, points
  by rank).

Build a mode: `cargo build -p tm-driver-mode-rounds --target wasm32-unknown-unknown --release`.

## Configuration

`ModeSettingsV2::Driver(Driver { mode, settings, decision_timeout })` selects the driver script.
`mode` names the blob and `settings` is the JSON passed to `init`. `TmMode::Driver` tells the
manager the match is points based (driver decided points, summed like Rounds).

For now the bridge loads blobs from `TM_DRIVER_MODES_DIR` (default `./modes`) as `<mode>.wasm`.
A blob registry in the manager (pinned by hash per match) replaces this later.

```sh
cargo build -p tm-driver-mode-rounds --target wasm32-unknown-unknown --release
cp target/wasm32-unknown-unknown/release/tm_driver_mode_rounds.wasm $TM_DRIVER_MODES_DIR/rounds.wasm
```

Example settings for the rounds mode (every field is optional):

```json
{
  "points_repartition": [10, 6, 4, 3, 2, 1],
  "rounds_per_map": 5,
  "maps_per_match": 1,
  "points_limit": 0,
  "use_tie_break": true,
  "finish_timeout": "automatic"
}
```

`finish_timeout` is `"automatic"`, `"disabled"` or `{ "milliseconds": 5000 }`.

Example settings for the time attack mode (every field is optional):

```json
{
  "time_limit_ms": 300000,
  "maps_per_match": 1,
  "points_repartition": []
}
```

An empty `points_repartition` gives the best of N players N points, the second N - 1 and so on.

## Phases

0. ✅ Typed boundary in `tm-server-types` and the controller.
1. ✅ Generic modescript (not run on a server yet).
2. ✅ `tm-wasm-driver` host, SDK and the Rounds mode with tests running the real wasm.
3. ✅ Bridge integration behind `ModeSettingsV2::Driver`.
4. Manager: snapshot table and resume, explicit driver results (`finalize` without per-mode
   branches), team commands.
5. Port the remaining modes: ✅ TimeAttack (`wasm-driver/6-time-attack`), Cup, ReverseCup,
   Knockout. Check parity with recorded matches.
6. UI library on top of `Net_Driver_State`; remove the legacy script swapping; user uploads.

## Next steps

1. **Regenerate the client bindings** with the fork CLI that produced the committed ones. The
   generated `Event`, `ModeSettingsV2` and `TmMode` don't know the new variants yet. Until then:
   - Driver configs can't reach the bridge. The stale bindings can't decode
     `ModeSettingsV2::Driver`.
   - The bridge doesn't forward `Event::Driver` to the manager, because transmuting it into the
     stale type would be undefined behaviour. Remove that filter in
     `tm-server-bridge/src/state.rs` after regenerating.

   The stock 2.13.0 CLI (commit `08e1aba`) doesn't generate the `*_async` procedure methods
   the bridge uses, so it can't be used for this.
2. **Run the modescript on a real server.** Check:
   - The script compiles. These APIs are not used by any script we checked:
     `ServerShutdownRequested`, `Scores::SetPlayerMapPoints` and the raw
     `UIManager.UILayerCreate` API.
   - Decision callbacks block and unblock as expected, and a missing driver shows the
     waiting message.
   - RoundsBase awards no points of its own.
   - The manager records the points from the `PreEndRound` scores callback.
   - Time attack rounds start players again after a finish or give up, ignore the "everyone
     finished" end and stop at the time limit.
   - A config change between driver modes applies the new round format on the next map.
3. **Phase 4**: snapshots in SpacetimeDB and resuming after a bridge restart.

## Risks

- `TrackmaniaRoundsBase` ships in the game's title pack and can change with game updates. Keep the
  script small and log the base and script versions in `Driver.Hello`.
- Large JSON in netvars costs bandwidth. Prefer per-player netvars and serial counters.
- The epoch deadline (wall clock limit per call) is configured but not covered by a test,
  because none of the test modes runs long enough to hit it.
