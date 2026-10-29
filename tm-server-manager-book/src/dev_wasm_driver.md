# WASM Mode Driver

Status: phases 0–2 and the TimeAttack, ReverseCup, Knockout and Endurance modes are implemented on
the `wasm-driver/*` branches. The bridge integration (phase 3) comes later, until then the runner in
`tm-wasm-driver/src/main.rs` drives a server with a mode.
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
| Durability | No snapshots. Drivers only act on their settings and their inputs, so replaying the inputs of a match rebuilds the same driver (see [Resuming](#resuming)). |
| Database | Untouched until the feature is fleshed out. The manager integration comes last. |
| Who writes modes | First-party modes first (in-repo Rust crates). The host is sandboxed from day one (fuel, epoch deadline, memory cap, no WASI) so user uploads can be added later. |
| ABI | WIT component model (`tm-wasm-driver/wit/driver.wit`). |
| Scoring | One `standings` structure every mode shares (see [Standings](#standings)). The host keeps it and hands it to the mode with every event, the mode applies its rules and returns it. Points never exist in the modescript and modes keep nothing but their settings. |
| What the modescript adds | Only what the standard XML-RPC protocol lacks. Everything else uses the standard callbacks and methods. |
| UI | The host publishes the standings, the round and the players it knows, which the modescript writes into typed netvars, players by an index. Manialinks the config selects render them, defaults or a tournament's own, so every manialink works with every mode. Modes bring none (see [UI](#ui)). |
| Live leaderboards | While a round runs, the host lets the mode place it as it stands once the races changed, at most once a second of server time, and publishes it like the rest (see [Live rounds](#live-rounds)). Which leaderboard shows is up to the manialinks. |
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
| `round-end` | `Driver.Hold` (`RoundEnd`) with every race of the round, from the start lines and waypoints | yes |
| `map-end` | `Driver.Hold` (`MapEnd`) | yes |
| `match-end` | `Maniaplanet.EndMatch_Start` | no |

`round-end` carries the races of the round as they happened. `racers` lists everyone who started a
race in the order of their first start, with every race they started, e.g. one per restart in
time attack rounds. A race holds the race times at the checkpoints passed (the finish included,
from the `curracecheckpoints` of the waypoints), whether it `finished` and the `laps` it completed
(counted from the checkpoints at every waypoint with `isendlap`, so a missed waypoint loses no
lap). How many checkpoints
there are tells how far a player got, the times tell players who got equally far apart.
`present` lists the players on the server when the round ended, racers or not. Players who left
stay in the races of the round they started in.

The host builds the races from the standard callbacks: `Trackmania.Event.StartLine` starts a race
and every `Trackmania.Event.WayPoint` passes a checkpoint, with `isendrace` the finish. It does not
interpret them, the modescript sends no scores either. Which race counts is up to the mode: with
the SDK's `CountedRace::Last` the last finished race (single attempt rounds), with
`CountedRace::Best` the fastest one (time attack rounds). The players on the server come from
`ManiaPlanet.PlayerConnect`, `PlayerDisconnect` and `PlayerInfoChanged`. The runner hands the
players that are on the server when a mode loads over as `DriverInput::Players`, which the host
learns like their `PlayerInfoChanged` callbacks but publishes once. The list is complete: players
the host thought on the server but missing from it left, e.g. while the bridge was down.

A round end whose round start the host did not see is `invalid`, so it does not count. That happens
when the driver restarted during the round and missed its races. A round the script forced to end
or skipped is invalid as well.

While a round runs, the host also asks the mode for the round as it stands, with the races so far
(`live-round`, see [Live rounds](#live-rounds)). That is no event: the mode changes nothing with it.

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
  the hold again periodically. The host answers a repeated hold with the commands it already
  decided, so nothing is decided twice.
- Once the bridge tells the host that the reply reached the server (`DriverInput::Delivered`), a
  repeated hold only gets its `Continue`. Running the commands twice would do harm: `EndMap` is the
  `NextMap` of the server and would skip the next map, an `EndMatch` arriving after the next match
  started would end it, chat messages would show twice. Without it, e.g. when the reply was lost
  with the bridge, the hold gets every command again.

## Commands

`tm_server_types::method::DriverCommand` is what a mode can make happen. `DriverCommand::call()` is
the only place that knows how each command reaches the server.

| Command | Sent as | Why custom |
|---|---|---|
| `Continue` | `Driver.Continue [seq]` | Holds don't exist in the standard protocol. Only sent by the host. |
| `SetRoundFormat` | `Driver.SetRoundFormat [single_attempt, auto\|off\|ms]` or `[time_attack, ms]` | Round shape and finish timeout. |
| `SetWarmUp` | `Driver.SetWarmUp [rounds, duration_ms, auto\|timeout_ms]` | The built-in warmup only takes its settings from the script. |
| `SetRaceRules` | `Driver.SetRaceRules [map\|infinite\|laps, 1\|0]` | Laps and give ups, which the script sets in the engine (see [Race rules](#race-rules)). |
| `SetCanSpawn` | `Driver.SetCanSpawn [account_id, 1\|0]` | Eliminations without forcing spectators, which the allowlist sync manages. The host leaves out permissions it sent before in the match. |
| `EndMap` | standard `NextMap` | – |
| `SetNextMap` | standard `SetNextMapIdent [uid]` | – |
| `EndMatch` | `Driver.EndMatch` | NextMap only ends the map. |
| `SetLayer` / `RemoveLayer` | `Driver.SetLayer [name, manialink, normal\|scores_table]` / `Driver.RemoveLayer [name]` | Only layers created by the script can read its netvars. `scores_table` layers become the scores table, see [Scores table](#scores-table). Sent by the runner or the bridge for the manialinks of the config, not by modes. |
| `SetUiModuleVisible` | standard `Common.UIModules.SetProperties` | – |
| `SetStandings` | `Driver.SetStandings [json]` | Writes the netvars of the standings and bumps `Net_Driver_StandingsSerial` (see [UI](#ui)). Only sent by the host. |
| `SetRound` | `Driver.SetRound [json]` | Writes the netvars of the round and bumps `Net_Driver_RoundSerial`. Only sent by the host. |
| `SetUsers` | `Driver.SetUsers [json]` | Writes the netvars of the players and bumps `Net_Driver_UsersSerial`. Only sent by the host. |
| `SetNetState` | `Driver.SetNetState [json]` | Writes `Net_Driver_State` and bumps `Net_Driver_StateSerial`. For data of a mode the standings can not hold. |
| `ChatMessage` | standard `ChatSendServerMessage` | – |

Round format, finish timeout, race rules and spawn permissions reset to their defaults when a match
starts.
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
| `round` | The round that ended with the event, filled by the mode at `round-end`. Output only: the host takes it out after every event, keeps it as the last round and reports it to the manager. The round that runs comes from `live-round` (see [Live rounds](#live-rounds)). |
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

  It publishes the standings, the round (live while it runs) and the players whenever they change,
  and everything again when a match starts. It reports every round that counted to the manager: the points and
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
then at the one before and so on (of the race that counts for finishers, of the last race for
everyone else). Races that are equal all the way go by account rather than by name, which players
can change during a match.

- Points and losses of a round follow that order, so players with the same time get the points of
  different places.
- `Standings::place_round` places the round in that order.
- `Standings::rank` ranks the players in the match by points and players with the same points by
  that order, so nobody shares a position. Eliminated players keep the positions the mode gave
  them, which differ as well.
- A mode can break ties of points its own way first with `Standings::rank_with`, e.g. by the
  rounds a player won. The race of the round only decides the ties it leaves. The modes so far
  use none.
- Players who joined since the last round have no position yet (0, shown as `-`). The host
  publishes them after the ranked players by name, spectators last, so modes need no names for
  that.
- No match is played on because of a tie: leaders with the same points at the points limit or
  after the last map are told apart by the race like everyone else.

Modes become small functions of their settings, the standings and an event, which makes them
easy to test. The manager and the manialinks read one format for every mode, and admin
corrections can later edit the standings without the mode knowing about it.

## Live rounds

Some modes are followed live: the best times of a time attack map change with every finish, an
endurance race with every checkpoint, without a round ending in between. While a round runs, the
host lets the mode place it as it stands and shows it like a round that ended.

- `live-round(standings, races)` is an export of its own, not an event. The mode gets the races so
  far as a round end that counts and returns the places, points, times and labels it would give if
  the round ended now, with the SDK e.g. `Standings::placed`, the round of `place_round` without
  changing the standings. The standings are read only, so a live round never changes the match and
  the round end still decides. Modes reuse the code of their round end for it.
- The host asks once the races changed, with a start line or a waypoint of a round (not in
  warmups), at most once per `DriverLimits::live_round_interval`, 1 second of the server time of
  the callbacks. A change within the interval shows with the next start or checkpoint after it, and
  the round end always shows the final round, so the live round is eventually consistent. No wall
  clock is involved, so a replay asks at the same inputs and rebuilds the same live round.
- The round that runs goes out in the netvars of the round with `Net_Driver_RoundLive` whenever it
  changed: the number it gets if it counts and the places so far, empty when the round starts.
  Once the round ended, they show the last round that counted again, also after a round that did
  not count.
- Live rounds only show players who take part (`RoundEnd::taking_part`). Everyone else who raced
  joins at the round end at the earliest, e.g. a racer whose start the host missed.

| Mode | Live round |
|---|---|
| Rounds | finishers so far with the points they would get, everyone else by how far they got |
| TimeAttack | the best times so far with the points of their rank |
| ReverseCup | finishers so far with the points they would lose. Who loses the dnf points and who is eliminated is only known at the end. |
| Knockout | the race so far, nobody loses a life before the end |
| Endurance | everyone by the checkpoints they collected, with their lap |

How it was decided:

| Option | Verdict |
|---|---|
| Only Nadeo's scores table, which shows best times live | Native, but only its own columns, not the standings every mode shares and invisible to the manager. |
| An event stream of changes the manialinks apply | Netvars hold state, not streams: a late joiner needs a snapshot and every change since, with sequence numbers in ManiaScript. |
| Every waypoint as an event for the mode | A call per checkpoint, and modes would need incremental logic. |
| The host ranks live without the mode | Cannot know how a mode scores, e.g. checkpoint sums or provisional points. |
| A `race-update` event the mode handles in `step` | The mode could change the match during a round, the host would throw away all but the round. |
| **`live-round` at most once per interval, published like the rest** | Chosen. The code of the round end, the inputs decide the cadence, the match cannot change. |

Changes are streamed where the transport streams anyway: the manager is going to get the live round
with the replies like the standings, and SpacetimeDB subscriptions push its rows to web UIs.

Which leaderboard shows is up to the manialinks. A `focus` in the standings telling manialinks
whether to show the match or the round was considered and dropped: it would put a decision of the
UI into the data every mode shares, and one leaderboard cannot fit every mode anyway, e.g. an
endurance race shows laps. The host publishes the whole state, the match and the round:

- The default manialinks always show the match, with the points of the round next to it, which now
  change while the round runs, like in Nadeo's modes.
- For a mode whose round players follow, the config selects `live-standings` in place of
  `standings` (see [UI](#ui)), which shows the round instead of the match.

A live round costs a call with every race so far and, when it changed, the netvars of the round,
mostly integers, at most once a second. On a full server that is a fraction of a millisecond, also
late in a long endurance race (see [Costs](#costs)).

## UI

| Approach | Verdict |
|---|---|
| Every mode writes its own json for its own manialinks | Each mode reinvents its UI and depends on it. |
| The modescript mirrors the points into Nadeo's scores table | Native look, but points would live in the script again. |
| The bridge renders manialinks and sends them with `SendDisplayManialinkPage` | No netvars, but every update resends whole pages and those manialinks cannot read the netvars of the script. |
| **The host publishes the shared standings, manialinks the config selects render them** | Chosen. Any manialink works with any mode, a tournament can bring its own. |

Modes produce data, the standings, the round and `set-net-state`, and bring no manialinks. Which
manialinks show and how they look is up to the config of a match, like the mode and its settings.

### Netvars

The netvars are the interface of every manialink, so they are typed and small rather than json:

- A client only gets what changed, mostly integers: a typical entry is five integers instead of
  about 140 bytes of json with the account in it. A live round of a full server is a few kilobytes
  instead of about 17 per update and player.
- A new netvar does not break manialinks that do not read it, unlike a new field of json that
  `fromjson` may refuse.
- Players are an index. The accounts, names, club tags and zones only go out when players change,
  the standings and the round only carry the index.

`tm_server_types::method::DRIVER_NETVARS` lists them by name and ManiaScript type, all
`for Teams[0]`. Every group has a serial that increases once all its netvars changed together:

| Group | Netvars |
|---|---|
| Players (`Net_Driver_UsersSerial`) | `Net_Driver_UserAccountIds`, `Net_Driver_UserNames`, `Net_Driver_UserClubTags`, `Net_Driver_UserZones`: `Text[Integer]` by index |
| Standings (`Net_Driver_StandingsSerial`) | `Net_Driver_Title`, `Net_Driver_Info`, `Net_Driver_MapName` (`Text`), `Net_Driver_MapCount`, `Net_Driver_MapsPlayed`, `Net_Driver_RoundsOnMap` (`Integer`), `Net_Driver_Paused` (`Boolean`) and the entries of the match |
| Round (`Net_Driver_RoundSerial`) | `Net_Driver_RoundNumber` (`Integer`), `Net_Driver_RoundLive` (`Boolean`) and the entries of the round as it stands while it runs, otherwise of the last round that counted |
| State of the mode (`Net_Driver_StateSerial`) | `Net_Driver_State` (`Text`), whatever a mode sends with `set-net-state` |

The entries are columns with one value per entry, standings ordered by position (unranked players
last), the round by place: `…Players` (the index), `…Positions`, `…Scores`, `…Times` (-1 without a
time) and `…Statuses` (0 playing, 1 eliminated, 2 spectator) are `Integer[]`, e.g.
`Net_Driver_StandingsScores`. `…Labels` (`Text[Integer]`) and `…LabelTones` (`Integer[Integer]`,
0 neutral, 1 positive, 2 negative) are maps by player, for the players with a label.

```maniascript
declare netread Integer[] Net_Driver_StandingsPlayers for Teams[0];
declare netread Integer[] Net_Driver_StandingsScores for Teams[0];
declare netread Text[Integer] Net_Driver_UserNames for Teams[0];
foreach (Entry => Player in Net_Driver_StandingsPlayers) {
	log(Net_Driver_UserNames[Player]^" "^Net_Driver_StandingsScores[Entry]);
}
```

- The host sends what changed as json (`DriverNetStandings`, `DriverNetRound`, `DriverNetUsers`)
  over XML-RPC, the modescript parses it with `fromjson` and writes the netvars. That json stays
  between the host and the script, which change together, and never reaches a client. Tests check
  that the script writes every netvar with its type and parses every field.
- The netvars are global state, sent to every client once per change however many manialinks read
  them, which compute the view of the local player themselves, e.g. its row. Every manialink reads
  the columns directly, there is no json to parse on the client. A hidden manialink that prepares
  the data once for the others would add a dependency between manialinks and save next to nothing.
- A test checks that every default manialink only reads netvars of the list with their types and
  watches the serials of what it reads.

### Player indices

The host gives every player the manialinks show an index, a `u32`: the players it learns about and
everyone else in the standings or the round, in the order it first publishes them, from 0. An index
is never reused while the driver runs and the players go out before the standings, so an entry
always refers to a known player. Replays give the same indices.

The manager numbers its players with `u32` too, so it could dictate the indices later:

- The bridge would hand the host the manager's index of a player with an input, e.g.
  `DriverInput::PlayerIndex(account, index)`, before the player shows up, so replays stay exact.
  The host would only number players itself without one, from a range the manager does not use.
- The same index would then name a player in the netvars, in the tables of the manager and in its
  web UIs, across matches and servers, and external UIs could join them without accounts.
- Manialinks do not change: an index is an opaque key of the maps of the players. ManiaScript only
  has signed 32 bit integers, so indices from 2³¹ on would arrive negative but stay unique keys.

Until then the host numbers them, which is all the netvars of a server need.

### Manialinks

- `tm-driver-manialinks` has the defaults, by name:
  - `standings`: the top 10 of the match with positions (ties included), names, points, points
    of the round (live while it runs) and statuses, spectators left out. It highlights the local
    player and always shows them, in the last row if needed.
  - `my-standing`: place, points or status and the round of the local player.
  - `scores-table`: replaces the scores table of the game, see below.
  - `live-standings`: the round instead of the match, the top 10 of the round as it stands while
    it runs and of the last round otherwise, with their labels (e.g. the lap of an endurance race),
    times and points, and `Live` while the round runs. For modes whose round players follow, e.g.
    time attack and endurance, in place of `standings`.
- The config selects manialinks by name (`Driver.manialinks`, `["standings", "scores-table"]` by
  default). A tournament can bring its own, e.g. with another style, which replace defaults with
  the same name. A name ending with `scores-table` makes a manialink the scores table.
- The runner shows the ones of `TM_MANIALINKS` (the defaults of the config without it), from
  `TM_MANIALINKS_DIR` or the defaults, when it starts and whenever the modescript (re)starts.
  `manialinks <name> ...` switches them while it runs and reads them again from their files, e.g.
  after changing a style. Once the manager stores manialinks, the bridge does the same with the
  names of the config.
- Data of a mode the standings can not hold goes through `SetNetState` (`Net_Driver_State`) to
  manialinks that know the mode.

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
  to the layers, the runner uses it whenever it installs layers.
- `scores-table.xml` lists up to 100 players with position, club tag, name, round points and points.
  Labels and statuses replace the points like in the original modes: labels by their tone
  (`Last Chance` in red, `Winner` in gold), `Eliminated` in dark red and `Spectator` in green.
  18 rows are visible and the mouse wheel scrolls
  through the rest (a frame with `scroll="1"`). The local player is highlighted and gets an extra
  row below the list while the visible rows do not show them, the spectated player is
  highlighted too.
- Names and club tags come from the netvars of the players, so they stay known after a player
  left.
  Nothing fills in club tags and zones yet, so the table falls back to the club tags of the
  connected players from `Scores` on the client. Logins for spectating always come from there.
- While a player watches someone else (`InputPlayer != GUIPlayer`, as a spectator or after
  finishing), clicking a player who still races spectates them with
  `ClientUI.Spectator_SetForcedTarget_Clear()` and `SetSpectateTarget(login)`, like the live
  ranking of the original reverse cup.
- Race times, avatars and the ping are not shown.

The manager comes with the database integration. Until then the runner only logs the round
results the host derives. `Event::DriverRoundResult` already exists for the integration.

## Resuming

There are no snapshots. The host and the modes are deterministic: modes run without WASI,
NaNs are canonicalized and relaxed SIMD uses its deterministic lowering. Everything a driver gets
is a `DriverInput`, applied with `Driver::apply`:

- `Event`: a callback of the server. Only the ones `Driver::handles` matter: `StartMatch_Start`,
  `StartMap_Start`, the start and end of the warmup, `StartRound_Start`,
  `Trackmania.Event.StartLine`, `Trackmania.Event.WayPoint`, `PlayerConnect`, `PlayerDisconnect`,
  `PlayerInfoChanged`, `EndMatch_Start` and the holds. `Driver::handle` takes a callback directly.
- `Playlist`: the maps the match plays from now on, also when they change during the match.
- `Players`: the players on the server when a mode loads or resumes, since they do not connect
  again.
- `Delivered`: the reply to the last hold reached the server (see above).
- `Resumed`: the bridge resumed the driver after a restart. Replies to the last inputs before it
  went down may never have reached the server, and the modescript may have changed meanwhile. So
  the host sends everything it set again: the spawn permissions of the match and the standings,
  the round and the players for the manialinks.

A bridge that restarts replays the inputs it stored, then applies `Resumed` and `Players` with
who is on the server now. Callbacks sent while it was down are lost, e.g. the finish of a player
in the round that runs. A hold is sent again by the modescript, and a round end without its round
start does not count.

Replaying the inputs of a match in order therefore rebuilds exactly the same driver, including
its standings, the players it knows and the reply to the last hold.
`DriverEngine::replay(module, settings, inputs)` does that. It needs the same mode module,
settings and limits. Matches will pin the module by hash once the manager stores modes.

`DriverInput` is serializable, so the database integration can store every input as it is
applied. The manager already stores the standard callbacks, but no playlists and players, and
holds only need to be stored once per seq although the script repeats them while it waits.

## Pauses

The pause is the one of the base script, `TrackmaniaRoundsBase`. An admin starts and ends it with
`Maniaplanet.Pause.SetActive`, and the script answers with `Maniaplanet.Pause.Status`, as it does
`Maniaplanet.Pause.GetStatus`.

- The round a pause interrupts does not count. The base script skips it (`Round_SkipPauseRound`),
  and the driver script sends its end as a forced hold, so the mode gets an `invalid` round end
  like for a round an admin forced to end. Players drive during the pause outside of a round, so
  their races count nowhere.
- The host follows the status callback and publishes `paused` with the standings. The manialinks
  show "Paused" instead of the info of the mode. Modes do not see the pause, they get nothing but
  the invalid round. A restarted modescript is not paused.
- The status is a callback like every other one, so a replay knows whether the match was paused.
  The script only sends it as an answer, so whoever pauses asks for it: the runner does after
  `pause` and `resume` and when it loads a mode.
- `tm-server-controller` sent `"ture"` to pause, so pausing never worked through it, also not the
  pause of the bridge when it loses the database. It sends `true` now and has
  `pause_get_status`.

## Changing settings during a match

Modes keep nothing but their settings, the scores live in the standings the host keeps. So new
settings can replace the ones of a match while it runs, without losing anything:
`DriverInput::Settings(json)`, from the runner with `settings <settings>`.

- New settings apply from the next round on. When no round runs, the mode takes them right away.
  During a round the mode only checks them (`check-settings`), so the caller learns at once when
  it rejects them, and takes them with the first input after the round ended, e.g. the
  `Delivered` of its end hold. The round that runs ends with the settings it started with, and a
  round end and new settings never come with the same input. Later settings replace ones that
  still wait.
- The mode takes them with `change-settings` and gets `settings-changed`, e.g. to show them in its
  info and send the round format and the warmup again. The standings stay as they are: the points
  of past rounds count as they were. Limits apply from the next round or map end on, e.g. a lower
  points limit ends the match after the next round.
- A mode lists the settings that can change during a match (`reloadable-settings`), with the SDK
  as `Mode::RELOADABLE`. The SDK compares new settings with the ones of the match, every default
  filled in, and rejects a change of anything else, e.g. `lives cannot change during a match`.
  Rejected settings change nothing, `DriverReply::rejected` tells why. The runner prints the list
  when it loads a mode, a manager can show which settings can be edited during a match.
- The settings are inputs like every other, so a replay takes them again at the same moment.
  `Driver::settings` and `Driver::pending_settings` tell what the mode plays with and what waits.

| Mode | Can change during a match | Cannot |
|---|---|---|
| Rounds | every setting | |
| TimeAttack | every setting, the time limit and the warmup apply from the next map | |
| ReverseCup | everything but the points players start with | `starting_points` |
| Knockout | everything but the lives, which players already hold | `lives` |

More rounds without elimination take no lives for a while, but they do not open the registration
of a knockout again once lives were taken. More winners than players left end a reverse cup with
the next round.

## Playlist

The host plays the uids of the playlist it gets as `DriverInput::Playlist`. The runner starts with
the map list of the server and changes it with `maps`. Once the bridge integrates the driver, the
map pool of the server config (`MapPoolConfig`) becomes the playlist of a match.

- The host remembers the current map from `Maniaplanet.StartMap_Start` and publishes its name
  right away, and the size of the playlist, with the standings (`map_name`, `map_count`). Modes get
  the size from `map-count`.
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
- Implements the twelve `Driver.*` methods above and nothing else, and writes what the host
  publishes into the typed netvars.
- Applies the race rules when a round starts, see [Race rules](#race-rules).

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

## Race rules

`SetRaceRules` sets the laps and the give ups of the races in the rounds that start, like the round
format. Every match starts with the laps of the map and give ups.

- `laps` is `map`, `count(n)` or `infinite`. When a round starts, the script sets
  `LapCountOverride` of the engine to a count and `CheckpointBehaviour` to `InfiniteLaps` for
  infinite laps. On maps without laps the finish still ends the race. The automatic finish timeout
  counts the laps of the race.
- Without `give-up`, the script sets `GiveUpBehaviour` to `DoNothing` and `RespawnBehaviour` to
  `AlwaysRespawn`: respawning goes back to the last checkpoint and nobody gives up.
- The script only changes what the driver asked for. What the base script had set comes back once
  the driver wants the defaults again.
- The members come from the script API of the game (`doc.h` of Trackmania 3.3.0). Nadeo's modes take
  the laps and the respawn behaviour from the settings `S_ForceLapsNb`, `S_InfiniteLaps` and
  `S_RespawnBehaviour`, but settings outlive a match, while the script keeps its state per match
  like the round format.

## TimeAttack

There is no separate TimeAttack script. TimeAttack is a round format of the generic modescript,
the same trick `TM_RoundsThenTimeattack` from the community modes uses:

1. The mode replies to `map-start` with `SetRoundFormat(TimeAttack(time_limit_ms))`.
2. When the round starts, the script clears the scores (so best races start fresh), sets
   `EndTime = StartTime + time_limit` and shows the time limit and best times in the scores table.
3. Players who finished or gave up are started again with `Race::Start`. The round only ends with
   the time limit (or when an admin forces it).
4. At the end the round end carries every race of the round. The mode counts the best one of
   every player (`CountedRace::Best`), awards points by it and ends the map.

Points exist so the standings and round results rank players like the other modes do. By
default the best of N finishers gets N points, so the points keep the order of the times. Equal
best times go by the checkpoints of those races (see [No ties](#no-ties)).

While the round runs, the live round shows the best times so far with the points of their rank.
With `live-standings` in place of `standings`, players follow the times of the map rather than the
points of the match, which the scores table shows with the points of the map so far.

## ReverseCup

A port of the community `TM_ReverseCup` script with the same settings and rules:

- The players who start a race before the first round counts take part. They show up in the
  standings with their starting points from their first race on, so the scores table lists them
  during the first round already. Once a round counted, everyone else cannot spawn anymore. A
  round counts once it takes points from someone, a map ending before does not close the
  registration.
- A match needs more participants than `number_of_winners`, otherwise nobody could lose. Until
  then rounds do not count, players can still join, a map ending does not end the match and
  `info` shows `Waiting for players, 2 / 3`. A single participant plays alone until eliminated.
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
- The original waits for `S_NbOfPlayers` players before it starts. Here a match starts with more
  participants than winners.
- Not ported: the live ranking, dossard colors, the podium and big messages. Warmup is the
  modescript's.

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
  round have no position yet and the host lists them by name. With one life players in the match
  are labeled with the time of their last race like in the scores table of the original, with more
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

## Endurance

A long race on a map with laps, the player who collects the most checkpoints wins. The race is the
match.

- With infinite laps (without `laps`), the race is a time attack round of `time_limit_ms` with
  infinite laps: nobody finishes, so nobody starts again, and the time limit ends it. Players who
  join during the race start at the start line.
- With `laps`, it is a round of one try over that many laps. It ends once everyone finished or the
  `finish_timeout` after the first finish ran out.
- Nobody can give up, respawning at a checkpoint stays possible.
- Players count the checkpoints of every race of the round, e.g. also of the race before they left
  and came back. Equal ones go to whoever got there first: the race times at their last checkpoints
  added up, for a single race the race time at its last checkpoint, then the account.
- The round shows live: places, the checkpoints as points, the time and the lap as a label
  (`Lap 3 / 50`, `Lap 12` with infinite laps, `Finished` after the last lap). A config for an
  endurance race selects `live-standings` in place of `standings`.
- Once the race ended, its order is the result of the match: positions, the checkpoints as points
  and `Winner` for the first. A race that does not count, e.g. after a pause, is driven again.

Limits:

- A number of laps needs a map with laps, on other maps the finish ends the race after one lap. With
  infinite laps on a map without laps, players start again after every finish, which counts as a
  lap, so the time limit still decides.
- A number of laps with a time limit on top would need a time limit for rounds of one try, which
  the modescript does not have yet.
- The default `standings` and `scores-table` show the checkpoints as the points of the match and of
  the round and no laps. A scores table for endurance would show the race like `live-standings`.

## Costs

The host measures every call into a mode (`Driver::stats()`, by kind: `init`, every event,
`live-round`, `check-settings`, `change-settings`):

- the fuel the mode used, roughly its instructions: the same on every machine, so it shows how far
  a call stays below its budget (`fuel_per_call`, 10⁹ by default);
- the wall clock time of building the arguments (e.g. the races of a round), of the call itself
  with copying the arguments into the mode and the result back, and of publishing afterwards;
- the most racers and checkpoints a call got.

The stats are no part of what a replay rebuilds. The simulation reports show the most fuel of every
kind of call, so a change in what a mode costs shows up in the snapshots, and every simulation fails
when a call uses more than a tenth of its budget.

`cargo run --release -p tm-driver-sim --bin bench [scenario ...]` simulates a full server of 120
players for every mode with its default settings and prints the stats, and how long replaying the
match takes. The endurance race lasts `BENCH_ENDURANCE_MINUTES` (120 by default) with infinite laps
on a map with laps of 40 seconds. On a desktop machine:

| Scenario | Most expensive call | Fuel (of the budget) | Host time | Replay of the match |
|---|---|---|---|---|
| Rounds, 5 rounds | `round-end` | 4.1M (0.41 %) | 0.4 ms | 0.06 s |
| TimeAttack, a map of 5 minutes | `round-end`, 3060 checkpoints | 3.3M (0.33 %) | 0.4 ms | 0.09 s |
| ReverseCup, 34 rounds | `round-end` | 4.4M (0.44 %) | 0.5 ms | 0.16 s |
| Knockout, 15 rounds | `round-end` | 4.2M (0.42 %) | 0.8 ms | 0.07 s |
| Endurance, 2 hours | `live-round`, 82445 checkpoints | 1.9M (0.19 %) | 0.3 ms | 1.9 s |

- A live round of a full server late in a two hour race costs about 0.2 ms of host time. The 6618
  live rounds of the race took 1.3 s of calls, 0.14 s of building the races and 0.56 s of
  publishing, together less than the time between two of them.
- Fuel barely grows with the checkpoints: a list of numbers goes into the mode as one copy, the
  calls cost what the modes do with the players. A live round late in the race costs about what it
  costs early.
- Replaying the whole race, like a bridge that resumes, takes 1.9 s with every live round.
- Every call stays below half a percent of its budget, so there is no need yet for modes to tell
  what they need live, for summaries of the races instead of every checkpoint or for replays that
  skip live rounds.
- A race start costs up to 0.2M fuel with 120 players in a knockout: the mode gets the standings of
  every player with every event.

## WIT interface and host

`tm-wasm-driver/wit/driver.wit` defines world `mode`:

```wit
export init: func(settings: string) -> result<_, string>;
export name: func() -> string;  // shown above the standings
export step: func(standings: standings, event: event) -> decision;  // standings + commands
export live-round: func(standings: standings, races: round-end) -> list<entry>;  // the round so far
export check-settings: func(settings: string) -> result<_, string>;  // new settings during a match
export change-settings: func(settings: string) -> result<_, string>;
export reloadable-settings: func() -> list<string>;
import host;  // log, find-user, map-count
```

- `Driver::handles(&Event)` tells the runner (later the bridge) which callbacks matter. `Driver::apply(&DriverInput)`
  (or `Driver::handle(&Event)` for a callback) returns a `DriverReply` with the commands for the
  server and the round result for the manager. Replies to holds end with `Continue(seq)`.
  `Driver::standings()`, `Driver::round()` and `Driver::users()` return what the host publishes,
  with accounts and the index of every player.
- `DriverEngine::replay` rebuilds a driver from the inputs it got before.
- Commands of a mode are converted with exhaustive matches, so both sides have to stay in sync.
- The host accepts a component or a core module built for `wasm32-unknown-unknown` with the SDK
  (core modules are wrapped with `wit-component` on load).
- Sandbox: no WASI imports, fuel per call, epoch deadline per call, memory limit. Time only comes
  from events, so a mode replays identically from its recorded inputs.

## Guest SDK and modes

- `tm-driver-sdk`: wit-bindgen bindings, the `Mode` trait (a mode names its `Settings` type and
  the SDK parses the json of the config into it, empty settings are `{}`, lists the settings
  that can change during a match in `RELOADABLE` and places the live round in `live_round`),
  `export_mode!`, helpers on the
  standings (`join`, `award`, `round_entry`, `rank`, `rank_with`, `place_round`, `placed`,
  `crown_leaders`, `in_match`, `block_others`), `RoundEnd::racer`, `RoundEnd::taking_part` and
  `CountedRace` for the race that counts,
  `race_order` for the order of a round without ties, the host functions (`user`, `name`,
  `map_count`), `ordinal` for chat
  messages and shared `settings`. Mode
  logic compiles natively too. `testing::Harness` runs a mode with the lifecycle of the host, so
  modes are unit tested without wasm, `Harness::change_settings` and `Harness::live_round`
  included.
  `testing::assert_reloadable_settings_exist` makes sure `RELOADABLE` only names settings.
- `tm-driver-modes/rounds`: Rounds (points repartition, rounds per map, maps per match, points
  limit, finish timeout).
- `tm-driver-modes/time-attack`: TimeAttack (time limit, maps per match, points by rank).
- `tm-driver-modes/reverse-cup`: ReverseCup (see [ReverseCup](#reversecup)).
- `tm-driver-modes/knockout`: Knockout with lives (see [Knockout](#knockout)).
- `tm-driver-modes/endurance`: Endurance (see [Endurance](#endurance)).
- `tm-driver-sim`: simulates whole matches of a mode against a fake server and runs logs of
  inputs with the same checks. Every mode tests its rules with it in `tests/simulation` (see
  [Simulation Testing](./dev_driver_simulation.md)). The integration tests of `tm-wasm-driver` test
  the host, with the rounds mode as the mode and the callbacks of `tm_driver_sim::events`.

## Configuration

Until the bridge integrates the driver, the runner loads a mode by the name of its crate in
`tm-driver-modes`, which it builds first, or by the path of a `.wasm` file, with settings as
inline json or a json file (see [Next steps](#next-steps)). The modes build by hand with:

```sh
cargo build -p tm-driver-mode-rounds -p tm-driver-mode-time-attack -p tm-driver-mode-reverse-cup \
  -p tm-driver-mode-knockout -p tm-driver-mode-endurance --target wasm32-unknown-unknown --release
```

For the integration the config of a match already has
`ModeSettingsV2::Driver(Driver { mode, settings, decision_timeout, manialinks })`, which selects
the driver script. `mode` names the wasm module, `settings` is the JSON passed to `init` and
`manialinks` names the manialinks shown next to the mode (`["standings", "scores-table"]` by
default, e.g. `["live-standings", "my-standing", "scores-table"]` for an endurance race), the
defaults of `tm-driver-manialinks` or ones the manager stores for a tournament. A registry in the manager
(pinned by hash per match) is going to provide the modes.

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

Settings of the endurance mode (every field is optional):

```json
{
  "laps": null,
  "time_limit_ms": 3600000,
  "finish_timeout": "automatic",
  "warmup": {"rounds": 0, "duration_ms": 0, "timeout_ms": null}
}
```

Without `laps` (or with `null`) the race has infinite laps and ends with `time_limit_ms`, with a
number of laps it ends once everyone finished or the `finish_timeout` ran out.

`extra_losses` is `{"above_players_left": [...]}`, `{"above_lives_left": [...]}` or
`{"before_round": [...]}`. `rounds_per_map` 0 means no limit. The Cup of the Day eliminates with
`"extra_losses": {"above_players_left": [8, 16, 16]}` (4 players per round above 16 players, 2
above 8 and 1 from there on), the Cup of the Week on Black tracks with `[16]`. The multi lives
script starts with `"lives": 3`.

## Phases

0. ✅ Typed boundary in `tm-server-types` and the controller.
1. ✅ Generic modescript (not run on a server yet).
2. ✅ `tm-wasm-driver` host, SDK and the Rounds mode with tests running the real wasm.
3. Bridge integration behind `ModeSettingsV2::Driver`, after testing with the runner, which stands
   in for it until then.
4. Database integration: store the inputs of the driver (holds once per seq) and resume by
   replay, record round results, a `TmMode` for driver matches, pin modes by hash, team commands.
5. Port the remaining modes: ✅ TimeAttack, ✅ ReverseCup, ✅ Knockout, Cup. Check parity with
   recorded matches. New ones: ✅ Endurance.
6. More manialinks on top of the standings (✅ live rounds, manialinks of the modes); remove the
   legacy script swapping; user uploads.

## Next steps

1. **Run the modescript and the manialink on a real server.** The runner in
   `tm-wasm-driver/src/main.rs` does that without the bridge and the manager: it installs the
   modescript, shows the manialinks and drives the server with a mode it builds from the
   repository (`load <mode> [settings]`, `reload`, `settings <settings>`, `maps [map ...]`,
   `manialinks [name ...]`, `script`, `next`, `pause`, `resume`).

   ```sh
   TM_SERVER_URL=127.0.0.1:5000 cargo run -p tm-wasm-driver -- reverse-cup '{"starting_points": 10}'
   ```

   Check:
   - The script compiles. `ServerShutdownRequested` and the raw `UIManager.UILayerCreate` API
     are not used by any script we checked.
   - Holds block and continue as expected, and a missing driver shows the waiting message.
   - Every waypoint carries the race times of all checkpoints so far (`curracecheckpoints`), the
     finish included, because their count is how far a player got and the last one of a finish
     is its race time. Players already on the server when the mode loads show up with their
     names.
   - `NextMap` sent during the round end hold ends the map once the hold continues.
   - Time attack rounds start players again after a finish or give up and stop at the time limit.
   - A warmup set by a mode runs before the first round of every map with the warmup UI of the
     game, `duration_ms: 0` gives every player one try, and eliminated players can not warm up.
   - The script parses what the host publishes with `fromjson` and writes the typed netvars,
     including the maps by index (`Text[Integer]`, `Integer[Integer]`), the manialinks read them,
     `LocalUser.WebServicesUserId` matches the account ids and their positions do not cover
     Nadeo's UI. How much a live round of a full server costs on the network.
   - The scores table key opens `scores-table` instead of Nadeo's table, also for spectators, it
     opens by itself for 3 seconds after every round and on the podium, and Nadeo's table comes
     back with a config without it.
   - The mouse wheel scrolls the scores table (`ScrollMax` of `Frame_Rows`), club tags show up
     and clicking a player as a spectator switches the camera to them.
   - A config change between driver modes applies the new round format on the next map.
   - The playlist runs in order: `SetNextMapIdent` sent while a map runs picks the map after it,
     and a new match starts with the first map of the playlist.
   - The `StartLine` and `WayPoint` callbacks arrive, so round ends know every race and how far
     it got.
   - `SetCanSpawn(false)` keeps eliminated players and spectators from spawning, also on the
     next map, and chat messages of the mode show up.
   - The standings manialink shows the labels of players in last chance and of the winners, and
     the status of eliminated players.
   - The race times and hearts a knockout shows as labels fit into the status cells of the
     manialinks, and the hearts (icons of the game font) render in `GameFontBlack`.
   - The race rules on a map with laps: `LapCountOverride` sets the laps of the race,
     `InfiniteLaps` never finishes it, `GiveUpBehaviour` and `RespawnBehaviour` stop give ups but
     keep respawns, and the base script does not set them back during the round. The waypoints
     carry `isendlap` and `checkpointinlap`, so the laps count.
   - The live round reaches the manialinks about once a second, e.g. of an endurance race
     (`load endurance {"time_limit_ms": 600000}` on a map with laps, then
     `manialinks live-standings scores-table`): `live-standings` shows it with the laps, the times
     and `Live`, and the round points of `standings` change while it runs.
2. **Database integration (phase 4)** once the feature is fleshed out.

## Risks

- `TrackmaniaRoundsBase` ships in the game's title pack and can change with game updates. Keep the
  script small.
- Netvars cost bandwidth. They are typed, the players only go out when they change, the standings
  and the round when they change, the live round at most once a second, and the round history
  stays with the host and the manager. Whether the game sends a whole array or only what changed in
  it is unknown, a full server needs a check.
- Replaying a match asks for every live round again. Two hours of an endurance race with 120
  players replay in about 2 seconds (see [Costs](#costs)). Longer races or slower machines could
  ask only for the last live round while replaying.
- The epoch deadline (wall clock limit per call) is configured but not covered by a test,
  because none of the test modes runs long enough to hit it.
