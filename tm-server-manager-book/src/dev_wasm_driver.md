# WASM Mode Driver

Status: phases 0–3 and the TimeAttack mode are implemented on the `wasm-driver/*` branches.
Everything except the modescript and the manialink is covered by tests, but nothing has run
against a real server yet. See [Next steps](#next-steps).

## Idea

Trackmania servers are driven by modescripts. Today we load a different premade script per mode
(Rounds, Cup, ReverseCup, TimeAttack, ...). The bridge swaps scripts on every config change, which is
convoluted and can crash when swapping between incompatible modes.

Instead, one **generic driver modescript** stays loaded on every server. It keeps no mode state:
no points, no elimination rules, no end conditions. The match flow reaches a **WASM mode**,
executed by `tm-wasm-driver` inside the bridge, through the standard callbacks of the base script.
The mode decides what happens and sends commands back.

## Decisions

| Topic | Decision |
|---|---|
| Where the logic runs | `wasmtime` inside the bridge (`tm-wasm-driver`). Decisions are local and fast. |
| Durability | The mode is a deterministic state machine with `snapshot`/`restore`. Snapshots are stored in SpacetimeDB at round boundaries so a crashed bridge can resume. |
| Who writes modes | First-party modes first (in-repo Rust crates). The host is sandboxed from day one (fuel, epoch deadline, memory cap, no WASI) so user uploads can be added later. |
| ABI | WIT component model (`tm-wasm-driver/wit/driver.wit`). |
| Scoring source of truth | The mode. Points never exist in the modescript. |
| What the modescript adds | Only what the standard XML-RPC protocol lacks. Everything else uses the standard callbacks and methods. |
| UI | Manialinks selected next to the mode read `Net_Driver_State`, which the mode writes. |
| Boundary typing | No `Event::Custom` and no hand-built JSON. Every callback and command is a Rust type in `tm-server-types`. |

## Architecture

```text
TM server ── TM_Driver.Script.txt (#Extends TrackmaniaRoundsBase, loaded once)
   │ standard callbacks + Driver.Hold        ▲ Driver.* methods + standard NextMap
   ▼                                         │
tm-server-controller   Event                 /  driver_commands(&[DriverCommand])
   ▼                                         │
tm-wasm-driver         one wasm instance per match, hold dedup, Continue, snapshots
   ▼                                         ▲
tm-server-bridge ── post_event / DriverRoundResult / snapshots ──▶ tm-server-manager
```

## What a mode gets told

| Mode event | Source | Waits |
|---|---|---|
| `match-start` | `Maniaplanet.StartMatch_Start` | no |
| `map-start` | `Driver.Hold` (`MapStart`) | yes |
| `round-start` | `Maniaplanet.StartRound_Start` | no |
| `round-end` | `Driver.Hold` (`RoundEnd`) + the `Trackmania.Scores` (`PreEndRound`) sent right before it | yes |
| `map-end` | `Driver.Hold` (`MapEnd`) | yes |
| `match-end` | `Maniaplanet.EndMatch_Start` | no |

`round-end` carries every player of the scores with the time and checkpoints of the last race and
of the best race. Single attempt rounds count the last race, time attack rounds the best one.
Race times stay in Nadeo's `Scores` library because that is race data, not mode state.

### `Driver.Hold`, the only custom callback

The standard callbacks are fire and forget, so the modescript would run into the next round before
the mode decided to end the map. `Driver.Hold` makes the script wait:

```json
{ "session": 123, "seq": 4, "section": "RoundEnd", "forced": false }
```

- `section` is `MapStart`, `RoundEnd` or `MapEnd`.
- `forced` (round end only) says the round was forced to end or skipped and should not count.
  The standard `EndRound_End.isvalid` would only arrive after the hold.
- `session` is random per script start, so a restarted script resets the host's tracking.
- The script keeps yielding while it waits, so commands get applied. If `Driver.Continue` with the
  same `seq` doesn't arrive within `S_DriverDecisionTimeout`, it shows a waiting message and sends
  the hold again periodically, together with the scores for round ends. The host answers a
  repeated hold with the commands it already decided, so nothing is decided twice.

## Commands

`tm_server_types::method::DriverCommand` is what a mode can make happen. `DriverCommand::call()` is
the only place that knows how each command reaches the server.

| Command | Sent as | Why custom |
|---|---|---|
| `Continue` | `Driver.Continue [seq]` | Holds don't exist in the standard protocol. Only sent by the host. |
| `SetRoundFormat` | `Driver.SetRoundFormat [single_attempt, auto\|off\|ms]` or `[time_attack, ms]` | Round shape and finish timeout. |
| `SetCanSpawn` | `Driver.SetCanSpawn [account_id, 1\|0]` | Eliminations without forcing spectators, which the allowlist sync manages. |
| `EndMap` | standard `NextMap` | – |
| `EndMatch` | `Driver.EndMatch` | NextMap only ends the map. |
| `SetLayer` / `RemoveLayer` | `Driver.SetLayer [name, manialink]` / `Driver.RemoveLayer [name]` | Only layers created by the script can read its netvars. |
| `SetNetState` | `Driver.SetNetState [json]` | Writes `Net_Driver_State` and bumps `Net_Driver_StateSerial`. |

Round format, finish timeout and spawn permissions reset to their defaults when a match starts.
A config change does **not** start a new match: NextMap only ends the map, so the modescript keeps
its match and its state. Modes therefore set their round rules on `map-start`.

Planned later: team commands (`SetPlayerTeam`, team points through the standings).

## Points and UI

The modescript never sees points. A mode keeps them and publishes them in two places:

- **Players:** `SetNetState` with the standings JSON of `tm_driver_sdk::standings`:

  ```json
  {"title": "Rounds", "info": "2 / 5 rounds", "players": [{"account_id": "...", "name": "...", "points": 16, "round_points": 10}]}
  ```

  `tm-driver-modes/manialinks/standings.xml` renders it. Manialinks are selected next to the mode in
  the config (`Driver.manialinks`). The bridge installs them as layers when the config loads and
  whenever the modescript (re)starts. Nadeo's scores table shows race times only.
- **Manager:** `round-result` from the mode becomes `Event::DriverRoundResult`, which sets points
  and positions of the round players. For driver matches the manager ignores the points of
  `Trackmania.Scores`.

## Generic modescript

`tm-server-types/external_modes/TM_Driver.Script.txt`, installed as `Modes/Trackmania/TM_Driver`.

- Extends `TrackmaniaRoundsBase`, so Nadeo still handles spawning, checkpoints, finishes, replays,
  records, warmup and the standard callbacks.
- Waits with `Driver.Hold` at map start, round end and map end.
- Sends `Trackmania.Scores` (`PreEndRound`) right before the round end hold.
- Implements the seven `Driver.*` methods above and nothing else.

## TimeAttack

There is no separate TimeAttack script. TimeAttack is a round format of the generic modescript,
the same trick `TM_RoundsThenTimeattack` from the community modes uses:

1. The mode replies to `map-start` with `SetRoundFormat(TimeAttack(time_limit_ms))`.
2. When the round starts, the script clears the scores (so best races start fresh), sets
   `EndTime = StartTime + time_limit` and shows the time limit and best times in the scores table.
3. Players who finished or gave up are started again with `Race::Start`. The round only ends with
   the time limit (or when an admin forces it).
4. At the end the scores carry everyone's best race of the round. The mode ranks by it, reports
   the round result, publishes standings and ends the map.

Points exist because `TmMode::Driver` standings sum round points. By default the best of N
finishers gets N points, so the points keep the order of the times.

## WIT interface and host

`tm-wasm-driver/wit/driver.wit` defines world `mode`:

```wit
export init: func(settings: string) -> result<_, string>;
export step: func(event: event) -> list<command>;
export snapshot: func() -> list<u8>;
export restore: func(snapshot: list<u8>) -> result<_, string>;
import host;  // log
```

- `Driver::handles(&Event)` tells the bridge which callbacks matter. `Driver::handle(&Event)`
  returns a `DriverReply` with the commands for the server and an optional round result for the
  manager. Replies to holds end with `Continue(seq)`.
- Commands of a mode are converted with exhaustive matches, so both sides have to stay in sync.
- The host accepts a component or a core module built for `wasm32-unknown-unknown` with the SDK
  (core modules are wrapped with `wit-component` on load).
- Sandbox: no WASI imports, fuel per call, epoch deadline per call, memory limit. Time only comes
  from events, so a mode replays identically from a recorded event log.

## Guest SDK and modes

- `tm-driver-sdk`: wit-bindgen bindings, a `Mode` trait, `export_mode!` and the `standings` helper.
  Mode logic compiles natively too, so it can be unit tested without wasm.
- `tm-driver-modes/rounds`: Rounds (points repartition, rounds per map, maps per match, points
  limit with tie break, finish timeout).
- `tm-driver-modes/time-attack`: TimeAttack (time limit, maps per match, points by rank).

## Configuration

`ModeSettingsV2::Driver(Driver { mode, settings, decision_timeout, manialinks })` selects the driver
script. `mode` names the wasm module, `settings` is the JSON passed to `init` and `manialinks`
names the manialinks shown next to the mode (`["standings"]` by default).

For now the bridge loads modes from `TM_DRIVER_MODES_DIR` (default `./modes`) as `<mode>.wasm` and
manialinks from `TM_DRIVER_MANIALINKS_DIR` (default `./manialinks`) as `<name>.xml`. A registry
in the manager (pinned by hash per match) replaces this later.

```sh
cargo build -p tm-driver-mode-rounds --target wasm32-unknown-unknown --release
cp target/wasm32-unknown-unknown/release/tm_driver_mode_rounds.wasm $TM_DRIVER_MODES_DIR/rounds.wasm
cp tm-driver-modes/manialinks/standings.xml $TM_DRIVER_MANIALINKS_DIR/
```

Settings of the rounds mode (every field is optional):

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

Settings of the time attack mode (every field is optional):

```json
{
  "time_limit_ms": 300000,
  "maps_per_match": 1,
  "points_repartition": []
}
```

## Phases

0. ✅ Typed boundary in `tm-server-types` and the controller.
1. ✅ Generic modescript (not run on a server yet).
2. ✅ `tm-wasm-driver` host, SDK and the Rounds mode with tests running the real wasm.
3. ✅ Bridge integration behind `ModeSettingsV2::Driver`.
4. Manager: ✅ round results of the driver, snapshot table and resume, team commands.
5. Port the remaining modes: ✅ TimeAttack, Cup, ReverseCup, Knockout. Check parity with recorded
   matches.
6. More manialinks on top of `Net_Driver_State`; remove the legacy script swapping; user uploads.

## Next steps

1. **Run the modescript and the manialink on a real server.** Check:
   - The script compiles. `ServerShutdownRequested` and the raw `UIManager.UILayerCreate` API
     are not used by any script we checked.
   - Holds block and continue as expected, and a missing driver shows the waiting message.
   - The `PreEndRound` scores carry the race of the round as the previous race (the base resets
     it every round) and the best race since `Scores::Clear` in time attack rounds.
   - `NextMap` sent during the round end hold ends the map once the hold continues.
   - Time attack rounds start players again after a finish or give up and stop at the time limit.
   - The standings manialink parses the JSON with `fromjson` and renders its rows.
   - A config change between driver modes applies the new round format on the next map.
   - The manager records the round results of the driver as points and positions of the round.
2. **Phase 4**: snapshots in SpacetimeDB and resuming after a bridge restart.

## Risks

- `TrackmaniaRoundsBase` ships in the game's title pack and can change with game updates. Keep the
  script small.
- Large JSON in netvars costs bandwidth. The standings are only published when they change.
- The epoch deadline (wall clock limit per call) is configured but not covered by a test,
  because none of the test modes runs long enough to hit it.
