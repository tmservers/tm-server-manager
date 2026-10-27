# WASM Mode Driver

Status: phases 0–3 and the TimeAttack, ReverseCup and Knockout modes are implemented on the
`wasm-driver/*` branches.
Everything except the modescript and the manialinks is covered by tests. The modescript runs on a
real server with the runner, the rest still needs a real match. The manager (and its database) stays untouched until the feature is
fleshed out. See [Next steps](#next-steps).

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
| Durability | No snapshots. Modes only act on their settings and the event stream, so replaying the events of a match rebuilds the same driver (see [Resuming](#resuming)). |
| Database | Untouched until the feature is fleshed out. The manager integration comes last. |
| Who writes modes | First-party modes first (in-repo Rust crates). The host is sandboxed from day one (fuel, epoch deadline, memory cap, no WASI) so user uploads can be added later. |
| ABI | WIT component model (`tm-wasm-driver/wit/driver.wit`). |
| Scoring | One `standings` structure every mode shares (see [Standings](#standings)). The host keeps it and hands it to the mode with every event, the mode applies its rules and returns it. Points never exist in the modescript and modes keep nothing but their settings. |
| What the modescript adds | Only what the standard XML-RPC protocol lacks. Everything else uses the standard callbacks and methods. |
| UI | The host publishes the standings, the last round and the players it knows to `Net_Driver_Standings`, `Net_Driver_Round` and `Net_Driver_Users`. Manialinks selected next to the mode render them, so every manialink works with every mode (see [UI](#ui)). |
| Boundary typing | No `Event::Custom` and no hand-built JSON. Every callback and command is a Rust type in `tm-server-types`. |

## Architecture

```text
TM server ── TM_Driver.Script.txt (#Extends TrackmaniaRoundsBase, loaded once)
   │ standard callbacks + Driver.Hold        ▲ Driver.* methods + standard NextMap
   ▼                                         │
tm-server-controller   Event                 /  driver_commands(&[DriverCommand])
   ▼                                         │
tm-wasm-driver         one wasm instance per match, standings, hold dedup, Continue, replay
   ▼                                         ▲
tm-server-bridge ── post_event (standard callbacks, as before) ──▶ tm-server-manager
```

## What a mode gets told

| Mode event | Source | Waits |
|---|---|---|
| `match-start` | `Maniaplanet.StartMatch_Start` | no |
| `map-start` | `Driver.Hold` (`MapStart`) | yes |
| `round-start` | `Maniaplanet.StartRound_Start` | no |
| `race-start` | the first `Trackmania.Event.StartLine` of a player in a round (not in warmups) | no |
| `round-end` | `Driver.Hold` (`RoundEnd`) with the races the host built from the waypoints of the round | yes |
| `map-end` | `Driver.Hold` (`MapEnd`) | yes |
| `match-end` | `Maniaplanet.EndMatch_Start` | no |

`round-end` carries everyone who started in the round and every other player on the server, with
the time and checkpoints of their last finished race and of their best race of the round. Single
attempt rounds count the last race, time attack rounds the best one. The host builds them from
the standard callbacks: `Trackmania.Event.StartLine` starts a race and a
`Trackmania.Event.WayPoint` with `isendrace` finishes it. The modescript sends no scores.

Every result also says whether the player `started` a race in the round and holds the race times
at the checkpoints they passed in their last race (`round_checkpoints`, the finish included, from
the `curracecheckpoints` of the waypoints). How many there are tells how far a player got, the
times tell players who got equally far apart without a finish. Players who left stay in the
results of the round they started in. The players on the server come from
`ManiaPlanet.PlayerConnect`, `PlayerDisconnect` and `PlayerInfoChanged`. The bridge hands the
players that were on the server when the mode loaded to `Driver::add_players`, which learns them
like their `PlayerInfoChanged` callbacks but publishes them once.

A round end whose round start the host did not see is `invalid`, so it does not count. That happens
when the bridge restarted during the round and missed its races. A round the script forced to end
or skipped is invalid as well.

### `Driver.Hold`, the only custom callback

The standard callbacks are fire and forget, so the modescript would run into the next round before
the mode decided to end the map. `Driver.Hold` makes the script wait:

```json
{ "session": 123, "seq": 4, "section": "RoundEnd", "forced": false }
```

- `section` is `MapStart`, `RoundEnd` or `MapEnd`.
- `forced` (round end only) says the round was forced to end or skipped, so the mode gets it as
  an `invalid` round end. The standard `EndRound_End.isvalid` would only arrive after the hold.
  The name stays `forced` while the manager stores the holds as they are.
- `session` is random per script start, so a restarted script resets the host's tracking.
- The script keeps yielding while it waits, so commands get applied. If `Driver.Continue` with the
  same `seq` doesn't arrive within `S_DriverDecisionTimeout`, it shows a waiting message and sends
  the hold again periodically. The host answers a
  repeated hold with the commands it already decided, so nothing is decided twice.

## Commands

`tm_server_types::method::DriverCommand` is what a mode can make happen. `DriverCommand::call()` is
the only place that knows how each command reaches the server.

| Command | Sent as | Why custom |
|---|---|---|
| `Continue` | `Driver.Continue [seq]` | Holds don't exist in the standard protocol. Only sent by the host. |
| `SetRoundFormat` | `Driver.SetRoundFormat [single_attempt, auto\|off\|ms]` or `[time_attack, ms]` | Round shape and finish timeout. |
| `SetWarmUp` | `Driver.SetWarmUp [rounds, duration_ms, auto\|timeout_ms]` | The built-in warmup only takes its settings from the script. |
| `SetCanSpawn` | `Driver.SetCanSpawn [account_id, 1\|0]` | Eliminations without forcing spectators, which the allowlist sync manages. The host leaves out permissions it sent before in the match. |
| `EndMap` | standard `NextMap` | – |
| `SetNextMap` | standard `SetNextMapIdent [uid]` | – |
| `EndMatch` | `Driver.EndMatch` | NextMap only ends the map. |
| `SetLayer` / `RemoveLayer` | `Driver.SetLayer [name, manialink, normal\|scores_table]` / `Driver.RemoveLayer [name]` | Only layers created by the script can read its netvars. `scores_table` layers become the scores table, see [Scores table](#scores-table). |
| `SetUiModuleVisible` | standard `Common.UIModules.SetProperties` | – |
| `SetStandings` | `Driver.SetStandings [json]` | Writes `Net_Driver_Standings` and bumps `Net_Driver_StandingsSerial`. Only sent by the host. |
| `SetRound` | `Driver.SetRound [json]` | Writes `Net_Driver_Round` and bumps `Net_Driver_RoundSerial`. Only sent by the host. |
| `SetUsers` | `Driver.SetUsers [json]` | Writes `Net_Driver_Users` and bumps `Net_Driver_UsersSerial`. Only sent by the host. |
| `SetNetState` | `Driver.SetNetState [json]` | Writes `Net_Driver_State` and bumps `Net_Driver_StateSerial`. For data of a mode the standings can not hold. |
| `ChatMessage` | standard `ChatSendServerMessage` | – |

Round format, finish timeout and spawn permissions reset to their defaults when a match starts.
A config change does **not** start a new match: NextMap only ends the map, so the modescript keeps
its match and its state. Modes therefore set their round rules on `map-start`.

Planned later: team commands (`SetPlayerTeam`, team points through the standings).

## Standings

Every mode works on the same structure, the `standings` record of the WIT. Its entries are the
same for the match and for a round:

| Entry field | Meaning |
|---|---|
| `position` | Rank in the match or place in the round. Nobody shares it (see [No ties](#no-ties)), 0 while a player has none, e.g. right after joining. Set by the mode with `Standings::rank()` and `Standings::place_round()`. |
| `score` | Points of the match or of the round. |
| `time` | E.g. the race that counted in the round. |
| `status` | Whether the player takes part and can still race: `playing`, `eliminated` (with a final position) or `spectator` (takes no part, no position). |
| `label` | Optional text shown in place of the points, e.g. `Last Chance` or `Winner`, with a tone (`neutral`, `positive`, `negative`). Display only. |

| Standings field | Meaning |
|---|---|
| `entries` | The match: one entry for every player taking part. |
| `round` | The round that ended with the event, filled by the mode at `round-end`. Output only: the host takes it out after every event, keeps it as the last round and reports it to the manager. |
| `rounds_on_map` | Rounds of the current map that counted, as the mode counts them. |
| `rounds_played` | Valid rounds of the match, counted by the host. |
| `maps_played` | Maps that ended in this match. |
| `info` | Progress as text, e.g. `2 / 5 rounds`. |

What the host knows itself is no mode state. A mode asks for it with the host functions
`find-user` (name, club tag and zone of a player, e.g. for chat messages) and `map-count`. The
title and the current map only go into what the host publishes.

Who does what:

- **Host:** keeps the standings and hands them to `step` with every event. Before the mode gets an
  event the host clears what belongs to it, so every mode has the same lifecycle:

  | Event | Cleared before the mode gets it |
  |---|---|
  | `match-start` | everything, also the last round |
  | `map-start` | `rounds_on_map` |
  | every event | `round` |
  | `round-end` | `rounds_played` counts the ending round unless it is invalid |
  | `map-end` | `maps_played` counts the ending map |

  It publishes the standings, the last round and the players whenever they change, and everything
  again when a match starts. It reports every round that counted to the manager: the points and
  places of everyone the mode placed. Spawn permissions it sent before in the match are left out,
  so modes can simply repeat them. The host knows nothing about how a mode scores or ranks.
- **Mode:** applies its rules and returns the standings: points, statuses, labels, positions,
  the round, counted rounds, `info`, and when maps and matches end. `Standings::award` adds points
  to the match and the round, `Standings::place_round` places the round by the race.
  `Mode::step` only gets `&self`, so a mode keeps nothing besides its settings.

Who wins and loses is told by the positions, never by special point values like the original
ReverseCup's `-1000` for last chance. A state a mode needs beyond the status, e.g. last chance, has
to follow from the standings, and the mode shows it with a label.

### No ties

Nobody ties in any mode. `race_order` of the SDK orders the races of a round: finishers by the
time of the race that counts (`CountedRace::Last` in single attempt rounds, `CountedRace::Best` in
time attack rounds), then everyone else by the checkpoints they passed in their last race, and
players who did not start last. Equal races go to the better race time at the last checkpoint,
then at the one before and so on (`previous_checkpoints` or `best_checkpoints` of finishers,
`round_checkpoints` of everyone else). Races that are equal all the way go by name, the case
aside, then by account.

- Points and losses of a round follow that order, so players with the same time get the points of
  different places.
- `Standings::place_round` places the round in that order.
- `Standings::rank` ranks the players in the match by points and players with the same points by
  that order, so nobody shares a position. Eliminated players keep the positions the mode gave
  them, which differ as well.
- A mode can break ties of points its own way first with `Standings::rank_with`, e.g. by the
  rounds a player won. The race of the round only decides the ties it leaves. The modes so far
  use none.
- Players who joined since the last round have no position yet (0, shown as `-`) and follow the
  ranked players by name (`Standings::list_newcomers`).
- No match is played on because of a tie: leaders with the same points at the points limit or
  after the last map are told apart by the race like everyone else.

Modes become small functions of their settings, the standings and an event, which makes them
easy to test. The manager and the manialinks read one format for every mode, and admin
corrections can later edit the standings without the mode knowing about it.

## UI

| Approach | Verdict |
|---|---|
| Every mode writes its own json for its own manialinks | Each mode reinvents its UI and depends on it. |
| The modescript mirrors the points into Nadeo's scores table | Native look, but points would live in the script again. |
| The bridge renders manialinks and sends them with `SendDisplayManialinkPage` | No netvars, but every update resends whole pages and those manialinks cannot read the netvars of the script. |
| **The host publishes the shared standings, selectable manialinks render them** | Chosen. Any manialink works with any mode. |

- The host sends three json texts, each to a netvar of its own with a serial the manialinks
  watch, so a manialink parses only what it needs and the players are not repeated with every
  update of the standings:

  | Netvar | Command | Json |
  |---|---|---|
  | `Net_Driver_Standings` | `SetStandings` | `DriverStandings`: title, info, map and the match entries |
  | `Net_Driver_Round` | `SetRound` | `DriverRound`: the number and the entries of the last round that counted |
  | `Net_Driver_Users` | `SetUsers` | `DriverUsers`: every player the host knows with name, club tag and zone |

  ```json
  {"title": "Rounds", "info": "2 / 5 rounds", "rounds_on_map": 2, "maps_played": 0,
   "map_name": "...", "map_count": 3,
   "entries": [{"account_id": "...", "position": 1, "score": 16, "time": -1, "status": "playing",
                "label": "", "label_tone": "neutral"}]}
  {"number": 2, "entries": [{"account_id": "...", "position": 1, "score": 10, "time": 30123, ...}]}
  {"users": [{"account_id": "...", "name": "...", "club_tag": "", "zone": ""}]}
  ```

  Missing values have defaults (`-1` for a time, empty texts) because the manialinks parse into
  structs.
- Manialinks parse them with `fromjson`. A test checks that every manialink reading one of the
  netvars declares a struct member for every field of its json.
- Manialinks are selected next to the mode in the config (`Driver.manialinks`). The bridge installs
  them as layers when the config loads and whenever the modescript (re)starts.
  `tm-driver-modes/manialinks` has:
  - `standings`: the top 10 of the match with positions (ties included), names, points, points
    of the last round and statuses, spectators left out. It highlights the local player and
    always shows them, in the last row if needed.
  - `my-standing`: place, points or status and the last round of the local player.
  - `scores-table`: replaces the scores table of the game, see below.
- Data of a mode the standings can not hold goes through `SetNetState` (`Net_Driver_State`) to
  manialinks of that mode.

### Styling

Nadeo's TM2020 UI modules are not public, so the manialinks follow the conventions of Nadeo's
TM2020 manialinks and of community modes that copy them, e.g. the live ranking of the original
reverse cup: `GameFontBlack` and `GameFontSemiBold` with `$i` (and `$t` for titles), `OswaldMono`
for numbers, black backgrounds between 0.5 and 0.85 opacity, cells for ranks and points, a green
accent line under the header and colored cells instead of the points for special statuses.
Nadeo's slanted backgrounds are `.dds` images and not reproduced.

Every manialink starts with its style:

- A `<stylesheet>` with classes for fonts, text colors and backgrounds (`title`, `info`, `name`,
  `number`, `status`, `header`, `accent`, `row`, `cell`, ...).
- `C_Style_*` constants at the top of the script for what changes at runtime: the highlight of
  the local and the spectated player, gained and lost round points, the color of every label
  tone and the color and the text of the statuses. Colors are hex like in the stylesheet.

Highlights and status colors are drawn on extra quads above the rows, so the script never
overwrites a color of the stylesheet.

### Scores table

The game does not draw its scores table itself. It shows every UI layer of the type
`CUILayer::EUILayerType::ScoresTable` while a player holds the scores table key (whatever they
bound) and whenever the script forces it with `UIManager.UIAll.ScoreTableVisibility`. Nadeo's
scores table is the UI module `Race_ScoresTable`, a manialink in such a layer.

- A manialink whose name ends with `scores-table` becomes a layer of that type
  (`Driver.SetLayer [name, manialink, scores_table]`), so it opens with the same key.
- The modescript forces the scores table visible during the 3 seconds after every counted round,
  like the Nadeo modes, and back to normal afterwards. The podium of the base script shows it at
  the end of the match.
- While such a manialink is installed, the host hides Nadeo's module with the standard
  `Common.UIModules.SetProperties` (`{"uimodules": [{"id": "Race_ScoresTable", "visible": false,
  "visible_update": true}]}`) and shows it again otherwise. `DriverCommand::show_layers` adds that
  to the layers, the bridge and the runner use it whenever they install layers.
- `scores-table.xml` lists up to 100 players with position, club tag, name, round points and points.
  Labels and statuses replace the points like in the original modes: labels by their tone
  (`Last Chance` in red, `Winner` in gold), `Eliminated` in dark red and `Spectator` in green.
  18 rows are visible and the mouse wheel scrolls
  through the rest (a frame with `scroll="1"`). The local player is highlighted and gets an extra
  row below the list while the visible rows do not show them, the spectated player is
  highlighted too.
- Names and club tags come from `Net_Driver_Users`, so they stay known after a player left.
  Nothing fills in club tags and zones yet, so the table falls back to the club tags of the
  connected players from `Scores` on the client. Logins for spectating always come from there.
- While a player watches someone else (`InputPlayer != GUIPlayer`, as a spectator or after
  finishing), clicking a player who still races spectates them with
  `ClientUI.Spectator_SetForcedTarget_Clear()` and `SetSpectateTarget(login)`, like the live
  ranking of the original reverse cup.
- Race times, avatars and the ping are not shown.

The manager comes with the database integration. Until then the bridge only logs the round
results the host derives, and the manager records driver matches like Rounds: positions follow
the scores, points stay at zero. `Event::DriverRoundResult` already exists for the integration.

## Resuming

There are no snapshots. The host and the modes are deterministic: modes run without WASI,
NaNs are canonicalized and relaxed SIMD uses its deterministic lowering. Replaying the events a
driver handled therefore rebuilds exactly the same driver, including its standings and the reply
to the last hold.
`DriverEngine::replay(module, settings, events)` does that.

A replay needs:

- The same mode module and settings. Matches will pin the module by hash once the manager
  stores modes.
- Every event the driver handled (`Driver::handles`): `StartMatch_Start`, `StartRound_Start`,
  `Trackmania.Event.StartLine`, `Trackmania.Event.WayPoint`, `PlayerConnect`,
  `PlayerDisconnect`, `PlayerInfoChanged`, `EndMatch_Start` and the holds. The manager already
  stores the standard callbacks. The bridge does not forward holds yet because the script repeats
  them while it waits, and the players it hands over when a mode loads are no callbacks. Storing
  both is part of the database integration.

## Playlist

The map pool of the server config (`MapPoolConfig`) is the playlist of a match. The bridge
downloads its maps and puts them into the match settings like for every other mode, and hands
their uids to the driver with `Driver::set_playlist`. The runner starts with the map list of the
server and changes it with `maps`.

- The host remembers the current map from `Maniaplanet.StartMap_Start` and publishes its name
  and the size of the playlist with the standings (`map_name`, `map_count`). Modes get the size
  from `map-count`.
- When a map starts, the host sets the next map of the playlist with the standard
  `SetNextMapIdent`, wrapping around at its end. Whenever a mode ends the match, the next map is
  the first one, so every match plays the playlist from the start. A map outside the playlist
  continues with its first map.
- A mode picks the next map itself with `set-next-map(index)`, e.g. for map picks. The host then
  leaves the next map alone until the following map starts.
- Rounds and time attack play the playlist once unless `maps_per_match` says otherwise, the
  reverse cup and the knockout go on until only the winners are left.
- The maps have to be in the map list of the server, `SetNextMapIdent` only chooses among them.
  The host does not check that.

## Generic modescript

`tm-server-types/external_modes/TM_Driver.Script.txt`, installed as `Modes/Trackmania/TM_Driver`.

- Extends `TrackmaniaRoundsBase`, so Nadeo still handles spawning, checkpoints, finishes, replays,
  records, warmup and the standard callbacks.
- Waits with `Driver.Hold` at map start, round end and map end.
- Runs the built-in warmup after the map start hold, see [Warmup](#warmup).
- Keeps race times in Nadeo's `Scores` library only for the scores table of the game. The host
  gets them from the waypoint callbacks.
- Implements the eleven `Driver.*` methods above and nothing else.

## Warmup

The driver uses the built-in warmup instead of emulating one. The base script already runs
`MB_WarmUp` before the first round of every map, which brings the warmup UI of the game, the
`Trackmania.WarmUp.*` callbacks and its own rounds that never reach the round labels of the
script, so warmup races are never scored. Emulating it with rounds that do not count would
rebuild all of that for nothing.

- Every mode has a `warmup` setting (`tm_driver_sdk::settings::WarmUpSetting`) and sends it with
  `set-warm-up` in reply to `map-start`, so it applies to every map, the first one included.
  The script hands it to `MB_WarmUp` right after the hold. Without a driver command the script
  uses its `S_WarmUpNb`, `S_WarmUpDuration` and `S_WarmUpTimeout` settings, which are reset when
  a match starts.
- `rounds` is the number of warmup rounds, 0 for none. `duration_ms` is how long one warmup
  round lasts, 0 lets every player drive once. `timeout_ms` is the time to finish after the
  first player finished in those rounds of one try, computed from the author time without it.
- The warmup has its own spawn rules, so the script copies the spawn permissions into
  `WarmUp::CanPlay` before the warmup. Eliminated players and spectators of a reverse cup or a
  knockout do not warm up, like in the originals.
- The host follows `Trackmania.WarmUp.Start` and `End` and never sends `race-start` for warmup
  races, so a reverse cup or a knockout does not register players because they warmed up.

## TimeAttack

There is no separate TimeAttack script. TimeAttack is a round format of the generic modescript,
the same trick `TM_RoundsThenTimeattack` from the community modes uses:

1. The mode replies to `map-start` with `SetRoundFormat(TimeAttack(time_limit_ms))`.
2. When the round starts, the script clears the scores (so best races start fresh), sets
   `EndTime = StartTime + time_limit` and shows the time limit and best times in the scores table.
3. Players who finished or gave up are started again with `Race::Start`. The round only ends with
   the time limit (or when an admin forces it).
4. At the end the scores carry everyone's best race of the round. The mode awards points by it
   and ends the map.

Points exist so the standings and round results rank players like the other modes do. By
default the best of N finishers gets N points, so the points keep the order of the times. Equal
best times go by the checkpoints of those races (see [No ties](#no-ties)).

## ReverseCup

A port of the community `TM_ReverseCup` script with the same settings and rules:

- The players who start a race before the first round counts take part. They show up in the
  standings with their starting points from their first race on, so the scores table lists them
  during the first round already. Once a round counted, everyone else cannot spawn anymore.
- Every participant starts with `starting_points` and loses points by finish position. The
  repartition depends on the players left (`complex_points_repartition`, otherwise
  `points_repartition`). While more players are left than it has values, every eliminated player
  removes its first value (`fast_forward_points_repartition`). Giving up costs `dnf_points_loss`.
- A player who runs out of points is in last chance (or eliminated with `disable_last_chance`).
  A player in last chance is eliminated by losing the most points of the round. Players eliminated
  in the same round get the last ranks of the players left in the order of the race. With
  `last_chance_dnf_mode: "only_least_checkpoints"` only the players in last chance with the fewest
  checkpoints lose points for giving up.
- Eliminated players get `SetCanSpawn(false)` and a chat message. The match ends once
  `number_of_winners` players are left (a single participant plays until eliminated).
- A round only counts for `rounds_per_map` if someone lost points. Maps change until the match
  is over.
- Lost points are the negative round points of the standings. Players in the match are
  `playing`: with points, or in last chance with 0 points and the label `Last Chance`. Eliminated
  players keep the rank they were eliminated with. The remaining players get the label `Winner`
  when the match ends.

Differences to the original:

- The participants are the players who start a race before the first round counts, not the
  players on the server when the first map starts. Everyone else in the scores cannot spawn anymore from then on
  and shows up as `Spectator` like in the original. Someone joining later can race the round
  they joined in, which does not count for them.
- The original skips a round that would put everyone in last chance anyway. The mode does that
  right after the previous round, so the round is not started at all.
- Participants who did not start in a round (for example because they left) always lose the dnf
  points. The checkpoints of `only_least_checkpoints` are counted over the race, not the lap.
- Players eliminated in the same round share a rank in the original. Here their race decides, like
  every other tie (see [No ties](#no-ties)).
- Not ported: waiting for `S_NbOfPlayers`, the live ranking, dossard colors, the podium and big
  messages. Warmup is the modescript's.

## Knockout

A port of the community `TM_MultiLivesKnockout` script, which gives the players of Nadeo's
`TM_Knockout_Online` lives. With one life, the default, it is the original Knockout with its
settings and rules:

- Rounds have one try. The last players of every round lose a life until one player is left, who
  wins. A player who loses their last life is eliminated. Everyone without a finish loses a life
  as well, even beyond the losses of the round. If that would eliminate everyone left, the player
  who got the furthest keeps their last life and wins.
- Every participant starts with `lives` lives. One player loses a life per round, plus one for
  every milestone of `extra_losses` still ahead, in any order. `above_players_left` counts the
  players left: the default `[4, 16, 16]` (the original's `S_EliminatedPlayersNbRanks`) takes a
  life from 4 players per round above 16 players, from 2 above 4 players and from 1 from there on.
  `above_lives_left` counts the lives the players left have together, `before_round` the rounds
  of the match: `[8, 16]` takes a life from 3 players per round before round 8, from 2 before
  round 16 and from 1 from there on. These are the `S_EliminationMode`s 0, 1 and 2 of the multi
  lives script. One player always stays.
- The first `rounds_without_elimination` rounds of the match take no lives. The players who start
  a race before the first round that takes lives ended take part. They show up in the standings
  from their first race on. Everyone else cannot spawn anymore from then on and shows up as
  `Spectator`.
- The race orders the players without ties (see [No ties](#no-ties)), which decides who loses a
  life.
- Eliminated players get `SetCanSpawn(false)`, a chat message and the last ranks of the players
  left, in the order of the race. Players eliminated in the same round get different ranks like in
  the originals.
- Maps change after `rounds_per_map` rounds (no limit by default) until the match is over.
- The points of a player are the lives they have left, a lost life is `-1` in the round. The round
  is placed in the order of the race. Players in the match are `playing`, ranked by their lives and
  then the order of their last race, so nobody shares a position. Players who joined since the last
  round have no position yet and follow by name. With one life players in the match are
  labeled with the time of their last race like in the scores table of the original, with more
  lives with hearts for their lives (text above 5 lives) like in the multi lives script, the last
  life with a negative tone. Eliminated players are labeled `K.O.`, the last player `Winner`.
- `info` shows the round that runs or comes next, the players left and how many lose a life in
  it. With one life it also estimates the rounds of the match like the original, e.g.
  `Round 2 / 22, 64 left, 4 K.O.`. Lives make the match too unpredictable for that:
  `Round 2, 64 left, 4 lose a life`.
- The rounds of the match are `rounds_played` of the standings, which the host counts.

Differences to the originals:

- The participants are the players who start a race before the first round that takes lives
  ended, not every player on the server. As long as nobody takes part, players can join.
- Invalid rounds, e.g. forced to end, do not count, like in the multi lives script. The original
  Knockout counts them, so such a round can use up a round without elimination.
- If everyone left would be eliminated, the originals eliminate everyone and rank the player who
  got the furthest first. The mode keeps that player in the match as the winner, the ranks are the
  same.
- The milestones count in any order, the originals expect them sorted.
- Not ported: the live ranking, the display of the eliminated players, dossard colors, the
  podium, big messages, the `Trackmania.Knockout.Elimination` and `Trackmania.Knockout.LostLife`
  callbacks, `S_MatchName` and `S_AlternativeMatchInfosPosition` of the multi lives script and
  `S_MatchPosition` of the Cup of the Day. Warmup is the modescript's.

## WIT interface and host

`tm-wasm-driver/wit/driver.wit` defines world `mode`:

```wit
export init: func(settings: string) -> result<_, string>;
export name: func() -> string;  // shown above the standings
export step: func(standings: standings, event: event) -> decision;  // standings + commands
import host;  // log, find-user, map-count
```

- `Driver::handles(&Event)` tells the bridge which callbacks matter. `Driver::handle(&Event)`
  returns a `DriverReply` with the commands for the server and the round result for the manager.
  Replies to holds end with `Continue(seq)`. `Driver::standings()`, `Driver::last_round()` and
  `Driver::users()` return what the host publishes.
- `DriverEngine::replay` rebuilds a driver from the events it handled before.
- Commands of a mode are converted with exhaustive matches, so both sides have to stay in sync.
- The host accepts a component or a core module built for `wasm32-unknown-unknown` with the SDK
  (core modules are wrapped with `wit-component` on load).
- Sandbox: no WASI imports, fuel per call, epoch deadline per call, memory limit. Time only comes
  from events, so a mode replays identically from a recorded event log.

## Guest SDK and modes

- `tm-driver-sdk`: wit-bindgen bindings, the `Mode` trait (a mode names its `Settings` type and
  the SDK parses the json of the config into it, empty settings are `{}`), `export_mode!`, helpers on the
  standings (`join`, `award`, `round_entry`, `rank`, `rank_with`, `list_newcomers`, `place_round`,
  `crown_leaders`, `in_match`, `block_others`), `race_order` with `CountedRace` for the order of a
  round without ties, the host functions (`user`, `name`, `map_count`), `ordinal` for chat
  messages and shared `settings`. Mode
  logic compiles natively too. `testing::Harness` runs a mode with the lifecycle of the host, so
  modes are unit tested without wasm.
- `tm-driver-modes/rounds`: Rounds (points repartition, rounds per map, maps per match, points
  limit, finish timeout).
- `tm-driver-modes/time-attack`: TimeAttack (time limit, maps per match, points by rank).
- `tm-driver-modes/reverse-cup`: ReverseCup (see [ReverseCup](#reversecup)).
- `tm-driver-modes/knockout`: Knockout with lives (see [Knockout](#knockout)).

## Configuration

`ModeSettingsV2::Driver(Driver { mode, settings, decision_timeout, manialinks })` selects the driver
script. `mode` names the wasm module, `settings` is the JSON passed to `init` and `manialinks`
names the manialinks shown next to the mode (`["standings", "scores-table"]` by default, e.g.
`["standings", "my-standing", "scores-table"]`).

For now the bridge loads modes from `TM_DRIVER_MODES_DIR` (default `./modes`) as `<mode>.wasm` and
manialinks from `TM_DRIVER_MANIALINKS_DIR` (default `./manialinks`) as `<name>.xml`. A registry
in the manager (pinned by hash per match) replaces this later.

```sh
cargo build -p tm-driver-mode-rounds -p tm-driver-mode-time-attack -p tm-driver-mode-reverse-cup \
  -p tm-driver-mode-knockout --target wasm32-unknown-unknown --release
cp target/wasm32-unknown-unknown/release/tm_driver_mode_rounds.wasm $TM_DRIVER_MODES_DIR/rounds.wasm
cp target/wasm32-unknown-unknown/release/tm_driver_mode_time_attack.wasm $TM_DRIVER_MODES_DIR/time-attack.wasm
cp target/wasm32-unknown-unknown/release/tm_driver_mode_reverse_cup.wasm $TM_DRIVER_MODES_DIR/reverse-cup.wasm
cp target/wasm32-unknown-unknown/release/tm_driver_mode_knockout.wasm $TM_DRIVER_MODES_DIR/knockout.wasm
cp tm-driver-modes/manialinks/*.xml $TM_DRIVER_MANIALINKS_DIR/
```

Settings of the rounds mode (every field is optional):

```json
{
  "points_repartition": [10, 6, 4, 3, 2, 1],
  "rounds_per_map": 5,
  "maps_per_match": null,
  "points_limit": 0,
  "finish_timeout": "automatic",
  "warmup": {"rounds": 0, "duration_ms": 0, "timeout_ms": null}
}
```

`finish_timeout` is `"automatic"`, `"disabled"` or `{ "milliseconds": 5000 }`. Without
`maps_per_match` (or with `null`) the match plays the playlist once, `0` means no limit.

Settings of the time attack mode (every field is optional):

```json
{
  "time_limit_ms": 300000,
  "maps_per_match": null,
  "points_repartition": [],
  "warmup": {"rounds": 0, "duration_ms": 0, "timeout_ms": null}
}
```

Settings of the reverse cup mode (every field is optional, these are the defaults of the original):

```json
{
  "starting_points": 100,
  "points_repartition": [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20],
  "complex_points_repartition": {"2": [4, 8], "3": [2, 6, 9], "4": [0, 4, 7, 10], "5": [0, 4, 7, 10, 13]},
  "fast_forward_points_repartition": true,
  "dnf_points_loss": 20,
  "disable_last_chance": false,
  "last_chance_dnf_mode": "all_players",
  "allow_fast_forward_rounds": true,
  "number_of_winners": 1,
  "rounds_per_map": 5,
  "finish_timeout": "automatic",
  "warmup": {"rounds": 0, "duration_ms": 0, "timeout_ms": null}
}
```

A key of `complex_points_repartition` can list several player counts, e.g. `"4,5"`.
`last_chance_dnf_mode` is `"all_players"` or `"only_least_checkpoints"`.

Settings of the knockout mode (every field is optional, these are the defaults of the original
Knockout):

```json
{
  "lives": 1,
  "rounds_without_elimination": 1,
  "extra_losses": {"above_players_left": [4, 16, 16]},
  "rounds_per_map": 0,
  "finish_timeout": {"milliseconds": 5000},
  "warmup": {"rounds": 0, "duration_ms": 0, "timeout_ms": null}
}
```

`extra_losses` is `{"above_players_left": [...]}`, `{"above_lives_left": [...]}` or
`{"before_round": [...]}`. `rounds_per_map` 0 means no limit. The Cup of the Day eliminates with
`"extra_losses": {"above_players_left": [8, 16, 16]}` (4 players per round above 16 players, 2
above 8 and 1 from there on), the Cup of the Week on Black tracks with `[16]`. The multi lives
script starts with `"lives": 3`.

## Phases

0. ✅ Typed boundary in `tm-server-types` and the controller.
1. ✅ Generic modescript (not run on a server yet).
2. ✅ `tm-wasm-driver` host, SDK and the Rounds mode with tests running the real wasm.
3. ✅ Bridge integration behind `ModeSettingsV2::Driver`.
4. Database integration: store the events the driver handles (holds once per seq) and resume by
   replay, record round results, a `TmMode` for driver matches, pin modes by hash, team commands.
5. Port the remaining modes: ✅ TimeAttack, ✅ ReverseCup, ✅ Knockout, Cup. Check parity with
   recorded matches.
6. More manialinks on top of the standings; remove the legacy script swapping; user uploads.

## Next steps

1. **Run the modescript and the manialink on a real server.** The runner in
   `tm-wasm-driver/src/main.rs` does that without the bridge and the manager: it installs the
   modescript, shows the manialinks and drives the server with a mode it builds from the
   repository (`load <mode> [settings]`, `reload`, `maps [map ...]`, `script`, `next`).

   ```sh
   TM_SERVER_URL=127.0.0.1:5000 cargo run -p tm-wasm-driver -- reverse-cup '{"starting_points": 10}'
   ```

   Check:
   - The script compiles. `ServerShutdownRequested` and the raw `UIManager.UILayerCreate` API
     are not used by any script we checked.
   - Holds block and continue as expected, and a missing driver shows the waiting message.
   - The finish waypoints carry the race time and the checkpoints, and players already on the
     server when the mode loads show up with their names.
   - `NextMap` sent during the round end hold ends the map once the hold continues.
   - Time attack rounds start players again after a finish or give up and stop at the time limit.
   - A warmup set by a mode runs before the first round of every map with the warmup UI of the
     game, `duration_ms: 0` gives every player one try, and eliminated players can not warm up.
   - The manialinks parse the standings with `fromjson`, `LocalUser.WebServicesUserId` matches
     the account ids and their positions do not cover Nadeo's UI.
   - The scores table key opens `scores-table` instead of Nadeo's table, also for spectators, it
     opens by itself for 3 seconds after every round and on the podium, and Nadeo's table comes
     back with a config without it.
   - The mouse wheel scrolls the scores table (`ScrollMax` of `Frame_Rows`), club tags show up
     and clicking a player as a spectator switches the camera to them.
   - A config change between driver modes applies the new round format on the next map.
   - The playlist runs in order: `SetNextMapIdent` sent while a map runs picks the map after it,
     and a new match starts with the first map of the playlist.
   - The `StartLine` and `WayPoint` callbacks arrive, so round results know who started and how
     far they got.
   - `SetCanSpawn(false)` keeps eliminated players and spectators from spawning, also on the
     next map, and chat messages of the mode show up.
   - The standings manialink shows the labels of players in last chance and of the winners, and
     the status of eliminated players.
   - The race times and hearts a knockout shows as labels fit into the status cells of the
     manialinks, and the hearts (icons of the game font) render in `GameFontBlack`.
2. **Database integration (phase 4)** once the feature is fleshed out.

## Risks

- `TrackmaniaRoundsBase` ships in the game's title pack and can change with game updates. Keep the
  script small.
- Large JSON in netvars costs bandwidth. The standings, the last round and the players are only
  published when they change, and the round history stays with the host and the manager. The
  size limit of a netvar text is unknown, a big lobby (about 150 bytes per player in the
  standings) needs a check.
- The epoch deadline (wall clock limit per call) is configured but not covered by a test,
  because none of the test modes runs long enough to hit it.
