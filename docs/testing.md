# Testing

How to check that a change works, cheapest first. Agents: follow this order and stop as
soon as a level answers the question.

1. **Unit tests** (`cargo test -p <crate>`): simulation rules, parsers, layouts. Most
   behaviour can be pinned down here with synthetic data (see `crates/acr-sim/tests`).
2. **Driver scenarios** (`cargo xtask e2e`): the real game, headless, driven through its
   menus and HUD with the original data. Seconds per scenario, no GPU, no window, no
   screenshots, no clicking by pixel guesses.
3. **Jev** (`cargo xtask jev ...`, `jev_check` steps): cheap typed judgments (System One
   model by TypeSafe) for routine navigation and "does this state look right?" checks.
4. **Screenshots** only for visual questions (sprites, layout, colours): one or two,
   cropped to the area in question, taken by the driver (`screenshot`, offscreen).
5. **Reasoning (Opus)** only when the above say "needs reasoning": a failing scenario
   whose answer does not explain itself, Jev below its thresholds, a picture to judge.

Never test by sending window messages to a visible game and reading screenshots: it is
slow, expensive, and modal message boxes block it.

## Running the scenarios

```sh
# All scenarios in tests/e2e/, against the original data (German v1.46):
cargo xtask e2e --game-dir "C:\path\to\American Conquest"
# One scenario, with per-step timings; reuse the last build:
cargo xtask e2e --filter build_site -v --no-build --game-dir ...
# A scenario file of your own:
cargo xtask e2e my_check.json --game-dir ...
cargo xtask e2e --list
```

`ACR_GAME_DIR` works instead of `--game-dir`. The runner builds the game in release mode
and copies it to `<target>/e2e/AC-e2e.exe`, so a test run is never mistaken for a
player's `AmericanConquest.exe`. Each scenario gets its own game process with
`ACR_VOLUME=0`, default options and its savegames, player profiles and log in
`<target>/e2e/runs/<scenario>/` (never in the game folder); the transcript of requests and
answers is written there as `transcript.jsonl`.

Without game data (CI) the scenarios are skipped; `cargo test -p xtask` checks the runner
against a fake game and that every scenario file parses, and `cargo test -p acr-game`
checks the driver's protocol parser.

Typical timings (Windows on ARM, release build, headless): starting a random map 0.6 s,
a mission 0.3 s, 100 ticks 0.1–0.3 s, a click 5–15 ms; all eight scenarios together 29 s, 18 s of them `attack_ground`.

## The driver

`AmericanConquest.exe --drive [GAME_DIR]` (or `ACR_DRIVE=1`) runs the game under the
driver: one JSON request per line on stdin, one JSON answer per line on stdout. It is
native only; the browser build has no driver.

| Flag | |
| --- | --- |
| `--drive` | no window, no GPU: menus, HUD and simulation run, nothing is drawn |
| `--drive-gpu` (`ACR_DRIVE_GPU=1`) | a hidden window with a GPU, for `screenshot` |
| `--size WxH` | the window size the game lays out for (default 1024x768) |
| `--dialogs auto\|report` | the dialog policy at the start (default `auto`) |

There are no movies. The game only advances while a request needs it, in fixed steps as
fast as it can: one simulation tick per frame in a running game, one menu tick (1/60 s)
otherwise. Input goes through the same handlers as the window's events: a `click` is a
mouse move, press and release at the widget's centre. After every input the driver
**settles**: it keeps stepping until requested files are loaded, the top screen is built
and idle (e.g. the main menu's slide is over) and a started game is shown, so the next
request sees the result.

`cargo xtask drive --game-dir ...` starts it for you and passes stdin through (lines
starting with `#` are skipped), e.g. `cargo xtask drive --no-build < session.jsonl`.

### Answers

The first line is `{"event": "ready", "protocol": 1, "gpu": false, "size": [1024, 768],
"screen": "MainMenu", ...}`. Every answer carries:

| Key | |
| --- | --- |
| `ok`, `error` | success, or why not (an unknown label lists the clickable ones in `clickable`) |
| `seq` | echoed from the request, if given |
| `screen` | the top screen (`MainMenu`, `RandomMap`, `MessageWindow`, ...) or `game` |
| `tick` | the simulation tick (`null` in the menus) |
| `frame`, `frames`, `ms` | frames run so far, frames this request ran, its wall time |
| `dismissed` | modal messages pressed away since the last answer: `dialog`, `text`, `button`, `tick` |
| `blocking` | with `dialogs report`: the open modal message (`screen`, `text`, `buttons`, `default`) |
| `log` | warnings and errors the game logged since the last answer |
| `settled: false` | settling gave up (30000 frames or 180 s) |

### Requests

Targets of `click`, `rclick` and `move` (also `from`/`to` of `drag`): `"label": "Laden"`
(an exact match, ignoring case and spacing, wins; else the first label containing it;
`"exact": true` allows only exact matches, `"index": n` picks the n-th), `"id":
"MainMenu/5"`, `"pos": [x, y]` (window pixels), `"unit": 805` or `"unit": {"owner": 0,
"type": "Kre"}` (the first living match; the view centres on it if needed), `"world": [x,
y]` (world pixels), `"near": {"owner": 0, "type": "Kri"}, "offset": [0, 700]` (the
matches' mean position plus an offset). A click waits up to `timeout` frames (600) for
its label to appear.

| Request | Does |
| --- | --- |
| `{"cmd": "ui"}` | the widget tree, see below; `"all": true` adds invisible and decorative widgets |
| `{"cmd": "click", "label": "Einzelspieler"}` | left click; `"button": "right"`, `"double": true`, `"mods": ["shift"]` |
| `{"cmd": "rclick", "unit": {"owner": 0, "type": "Kre"}}` | right click (orders) |
| `{"cmd": "move", "pos": [500, 300]}` | mouse move only (hover, hints) |
| `{"cmd": "drag", "from": [100, 100], "to": [300, 250]}` | press, move in steps, release |
| `{"cmd": "drag", "around": {"owner": 0, "type": "Kri", "max": 5}}` | a selection rectangle over matching objects |
| `{"cmd": "key", "key": "F12", "mods": ["ctrl"]}` | `Enter`, `Escape`, `F1`..`F12`, arrows, `PageUp`, `Home`, `Space`, `Pause`, single characters |
| `{"cmd": "text", "text": "e2e"}` | types into the focused input box |
| `{"cmd": "wheel", "lines": -1}` | mouse wheel |
| `{"cmd": "wait", "ticks": 100}` | also `"frames": n`, `"until": {...}` (all must hold), `"timeout": frames` (6000) |
| `{"cmd": "state"}` | see below; `"units": true` lists every object, `"owner": n` and `"type": "Mus"` filter, `"visible_only": true` respects the fog, `"detail": true` adds each object's orders, animation and frame, stances, reload counter (`delay`), direction and switches, and the listed objects' `activity` |
| `{"cmd": "command", "command": {"type": "move", "units": [1, 2], "x": 3000, "y": 9000}}` | a simulation command for setup (below) |
| `{"cmd": "select", "units": [10, 11]}` | replaces the selection (setup; prefer `drag`/`click` to test the interface) |
| `{"cmd": "start", "random": {"nation": 1, "seed": 5}}` | a random map without the menus; also `opponents`, `resources`, `size`, `difficulty`, `hit_locations` (the optional rule, #225) |
| `{"cmd": "start", "map": "Missions\\Singles\\Single7\\Single7.m3d"}` | a map file (alias `load_map`) |
| `{"cmd": "save", "name": "x"}` / `{"cmd": "load", "name": "x"}` | savegame shortcuts |
| `{"cmd": "screenshot", "path": "out.png", "crop": [x, y, w, h]}` | only with `--drive-gpu` |
| `{"cmd": "dialogs", "policy": "report"}` | `auto` or `report` |
| `{"cmd": "camera", "x": 3000, "y": 9000, "zoom": 5, "yaw_deg": 0, "pitch_deg": 10}` | the close-up bench's camera, see below |
| `{"cmd": "spawn", "type": "EngM18", "x": 3000, "y": 9000, "dir": 0, "pose": "stand"}` | places a unit (bench) |
| `{"cmd": "hit", "unit": 541, "cause": "musket", "region": "torso", "dir": [-1, 0], "energy": 80}` | a scripted hit (bench) |
| `{"cmd": "record", "dir": "frames", "count": 20, "every": 3, "crop": [x, y, w, h]}` | `count` screenshots, one every `every` frames (= ticks in a game), as `<dir>/frame000.png`...; `prefix` changes `frame`; answers `paths` (only with `--drive-gpu`) |
| `{"cmd": "quit"}` | ends the game |

`wait` conditions: `"screen": "RandomMap"` (or `"game"`), `"dialog": true | false |
"MessageWindow"`, `"in_game": true`, `"menus": true`, `"label": "text"` (on a visible
widget, HUD hint or message), `"tick": n`, `"selected": n`, `"formations": n`,
`"ended": "won" | "lost" | true`, `"units": {"owner": 0, "type": "Kre", "building":
true, "finished": true, "min": 1}`. A wait that times out fails and lists the conditions
still unmet in `unmet`.

Simulation commands (`type`, with `units` as a list of ids, `"selection"` (the default) or a
filter `{"owner": 0, "type": "Mus"}` for every living match):
`move`, `attack_move`, `patrol` (`x`, `y`), `stop`, `attack` (`target`), `gather` (`x`,
`y`), `repair` / `enter` (`building`), `leave` (`building`), `build` (`kind`, `x`, `y`),
`produce` (`building`, `kind`, `count`), `form` (`officer`, `kind` such as `#LINE15`),
`disband` (`officer`), `friendly_fire` (`on`: shoot through own units). Type names are catalog ids such as `SpnKri(SP)`.

### The close-up bench (#277)

For judging bodies, wounds and gore up close: a few soldiers placed on open ground, hit by
script through the simulation's own body path, seen from the side and recorded frame by
frame. Start a random map with the hit-locations rule and the settings `models=all`,
`gore=full` (an `ACR_SETTINGS_DIR` of your own), then:

- **`camera`**: `x`, `y` (or `"world": [x, y]`) is a ground point in world pixels; the
  point `lift` true pixels above it (default 27, 0.9 m: a standing man's middle) comes to the
  middle of the window. `zoom` sets the scale (output pixels per original pixel) beyond the
  wheel's 0.5..2: at 5 a standing man is about 270 px tall (1.8 m × 30 px/m × 5). `yaw`,
  `pitch` (radians) or `yaw_deg`, `pitch_deg`, or `"free": true`, switch to the free camera
  (`camera=free`) turned and tilted so: yaw 0 looks north, pitch down to 0.05 rad (below the
  keys' 15° limit) for a side view. The view is not kept inside the map.
- **`spawn`**: a unit of `type` (a catalog id such as `EngM18(EN)`, or a part of one:
  exact first, else the shortest match) for `owner` (0) at `x`, `y`, facing `dir` (0..255,
  0 = east, 64 = south). `pose`: `stand` (default), `walk` (patrols `walk` px, default 200,
  east and back) or `crawl` (left leg off below the knee). `lost` (`arm_l`, `arm_r`, `leg_l`,
  `leg_r`; `foot_l`/`foot_r` at the ankle) and `wounds` (one flesh wound per entry) shape
  his body; `patrol: true` makes any pose move (a crawler is sent directly, as he takes no
  orders). Answers `unit.id`.
- **`hit`**: `unit` (an id) is struck by `cause` (`musket`, `arrow`, `cannonball`,
  `grapeshot`, `shell`, `sabre`, `bayonet`, `axe`) on `region` (`legs`, `arms`, `torso`,
  `head`; drawn as in battle if left out) from direction `dir` (towards the victim, world
  axes) with `energy` 0..100 (default 100) and `damage` life points (default 1: only the
  injury's outcome kills; give e.g. 1000 to kill). `part` (`arm_l`, `arm_r`, `leg_l`, `leg_r`, `head`, `chest`,
  `belly`, `pelvis`) aims the hit there instead of a region, and `level` (0..255, from the
  limb's root or from the shoulders down the trunk; needs `part`) at that level; the outcome
  is still rolled as in battle (`acr_sim::body::injure_aimed`). It runs `acr_sim::combat::scripted_hit`:
  `body::injure` and the `HitRecord`/`DeathRecord` of battle, drawing from the world RNG
  only, so the same script gives the same wounds. Answers the region, injury (`part`,
  `level`, `outcome`), `killed` and `life`. Needs the hit-locations rule for regions.
- **`record`**: see the table; the frames are PNGs (make a GIF elsewhere if needed).

A ready script, 4 soldiers side by side seen from the side, each hit by a different weapon,
20 frames: `ACR-analysis/bench/bench.py` (`python -I bench.py [out_dir]`, with
`ACR_EXE` pointing at a release build); `drive.py` next to it runs a JSON-lines session.

```text
> {"cmd": "start", "random": {"nation": 0, "seed": 7, "hit_locations": true}}
> {"cmd": "spawn", "type": "EngM18", "x": 2717, "y": 11753, "dir": 0}
< {"ok": true, "unit": {"id": 541, "type": "EngM18(EN)", "pose": "stand"}, ...}
> {"cmd": "wait", "ticks": 20}
> {"cmd": "camera", "x": 2800, "y": 11753, "zoom": 5, "yaw_deg": 0, "pitch_deg": 10}
> {"cmd": "hit", "unit": 541, "cause": "cannonball", "region": "legs", "dir": [-1, 0], "energy": 80, "damage": 1000}
< {"ok": true, "hit": {"cause": "Cannonball", "region": "Legs", "injury": {"part": "Limb(LegL)", "level": 58, "outcome": "Off", "energy": 80}, "killed": true, "life": 0, ...}}
> {"cmd": "record", "dir": "frames", "count": 20, "every": 3, "crop": [180, 170, 1100, 470]}
```

Limits: the hit is applied between ticks by the driver, not as a lockstep command, so it is
not in replays; there is no `reload` pose yet; the HUD is drawn over the view (crop it).

### The widget tree (`ui`)

```json
{"screen": "RandomMap", "stack": ["MainMenu", "RandomMap"], "built": true, "busy": false,
 "in_game": false, "loading": false, "dialog": null, "window": [1024, 768],
 "widgets": [
   {"id": "RandomMap/11", "kind": "combo", "label": "Spanien", "rect": [223, 162, 126, 24], "clickable": true},
   {"id": "RandomMap/76", "kind": "button", "label": "Mission spielen!", "rect": [899, 706, 59, 59], "clickable": true}]}
```

Widgets come from the top screen's dialog in creation order; ids are `<Screen>/<index>`,
list rows `<Screen>/<list>#<row>`, open drop-down rows `<Screen>/<combo>~<row>`, HUD icons
`hud/<slot>` (label = the icon's hint, e.g. `Fort - Holz: 500 Stein: 100`, with `count`
and `right_click`). Kinds: `text`, `button`, `row`, `list`, `combo`, `option`, `input`,
`viewer` (scrolling text), `slider`, `scrollbar`, `picture`, `icon`. Flags appear only
when not the default: `clickable`, `enabled: false`, `hover`, `selected`. Labels are
UTF-8 (the game's CP1252 decoded). Rects are window pixels of the visible part. Over a
running game the tree also has `messages` (mission hints and texts over the map) and
`cursor` (`pos`, `mode`: `normal`, `placing <type>`, `rally`, `patrol`,
`attack_ground`).

### State (`state`)

`mode` (`menus`, `loading`, `game`), `screen`, `tick`, `player`, `resources` (`wood`,
`gold`, `stone`, `food`, `iron`, `coal`), `population`, `housing`, `selection` (`id`,
`type`), `formations` (`brigade`, `members`), `counts` per owner of living `units`,
finished `buildings`, building `sites` and `burning` objects by type, `mission` (`ended`: `won`/`lost`/null,
`objectives`). With `"units": true`: `units` with `id`, `type`, `owner`, `x`, `y`,
`life`, `building`, `finished`, `inside`, `burning`, `brigade` and `screen` (window position, when
in view). With `"detail": true`, `activity` sums up the listed objects: `units`, `attacking`
(an attack is the current order), `aside` (a move in front of an attack it resumes, e.g. a
musketeer stepping out of a friend's way), `idle` (no orders) and `reloading`.

### Modal messages

Mission pages ("Nachricht"), questions and choices stop the game until answered. With
`dialogs auto` (the default) the driver presses the default button (OK, else the first)
as soon as one is built and reports it under `dismissed` with its text. With `dialogs
report` it stays open, every answer carries it as `blocking`, and the test answers it
itself (`click` its button). Use `report` when the dialog is what is tested.

## Scenario files

`tests/e2e/<name>.json`:

```json
{
  "name": "build_site",
  "description": "what it shows",
  "args": ["--dialogs", "report"],
  "step_timeout_s": 120,
  "strict_log": false,
  "steps": [
    {"cmd": "start", "random": {"nation": 1, "seed": 5}},
    {"cmd": "state", "capture": {"wood": "state.resources.wood"}},
    {"cmd": "click", "label": "Fort", "expect": {"path": "target.kind", "eq": "icon"}},
    {"cmd": "state", "expect": [{"path": "state.counts.0.sites", "len_eq": 1},
                                 {"path": "state.resources.wood", "lt": "${wood}"}]}
  ]
}
```

Each step is a driver request plus runner keys: `expect` (one check or a list),
`capture` (`{"name": "dotted.path"}` of the answer, used later as `"${name}"`), `note`,
`optional` (failures do not count), `timeout_s`. A step must answer `ok: true` unless it
expects `{"ok": false}`. Checks: `{"path": "a.b.0", OP: value}` with `eq`, `ne`, `gt`,
`ge`, `lt`, `le`, `contains` (substring, list element or object key), `exists`,
`nonempty`, `len_eq`, `len_ge`; shorthands `{"screen": "game"}`, `{"dismissed": true}`,
`{"blocking": false}`, `{"no_errors": true}`. The scenario stops at the first failing
step.

A `jev_check` step asks Jev whether the current `ui` and `state` match a sentence:
`{"cmd": "jev_check", "expectation": "A game is running and the player has peasants",
"min": 0.7}`. Below `min` it fails with "needs reasoning"; without a key it is skipped.

The scenarios: `menu_random_map` (menus to a random map game), `mission_briefing` (a
single mission's briefing blocks the game until OK), `build_site` (drag-select peasants,
HUD build icon, placement: the site appears at once and is paid), `produce_unit` (build
the Fort, queue a soldier, send peasants in, the soldier comes out), `formation` (officer
→ line formation → fusiliers: one formation of officer, drummer, flag bearer and
soldiers), `formation_group` (two formations: `Formation gruppieren` groups them, the icon
turns into `Gruppierung auflösen`, which ungroups them), `musket_line_of_fire` (in *Die
Armada* musketeers behind their pikemen step aside or hold fire, and keep reloading when
friendly fire is switched on), `attack_ground` (a random map built
up to the Festung, a cannon trained and ordered through `Boden angreifen` onto the ground
pays its shots; the Festung's cannon ball sets a neutral building burning, which burns down
and disappears), `save_load` (save and load through the game menu), `army_list` (two formations: the army icon
at the top left opens the list of formations with their numbers; a click selects one, whose
command panel icon shows the same number).

## Jev

[Jev](https://docs.typesafe.ai) answers typed questions about JSON state (and images)
with probabilities, in about 0.3 s for a few hundred tokens. The helper reads the API key
at run time from `~/.config/typesafe/api_key` (`TYPESAFE_API_KEY_FILE` overrides) and only
hands it to `curl` on stdin; it is never printed, logged or committed. Without a key the
helper says so and scenarios skip their `jev_check` steps: fall back to the driver's
answers or to reasoning.

```sh
cargo xtask jev status
cargo xtask jev explore --goal "start a random map game" --game-dir ...
cargo xtask jev shot --png out.png --expect "the main menu over a map"
```

- **`explore`**: per screen one request with two questions, a Choice over the labelled
  clickable widgets (plus `none`) and a Noul "is the goal reached?". It clicks when the
  choice has p ≥ 0.5 and confidence ≥ 0.4, stops at p(reached) ≥ 0.8, and stops with
  "needs reasoning" otherwise, or when the same screen comes back three times.
  Measured: "start a random map game" 5 calls, 3150 input + 334 output tokens, 2.2 s;
  "start the single mission 'Die Indianeraufstände'" 6 calls, 4642 + 539 tokens; "open
  the load game screen" 2 calls, 1264 + 157 tokens.
- **`jev_check`**: a Noul over `ui` and `state`, ~850 input tokens, 0.3 s. Word the
  expectation in what the state shows: objects are named by type ids (`SpnKri(SP)`), so
  "the player has peasants" got p 0.31 (needs reasoning), while "the local player (owner
  0) has more than 100 units and neither buildings nor building sites" got a pass.
- **`shot`**: a Noul "does this look broken?" over a PNG (~2400 input tokens). It ranks
  well (main menu 0.07; half black 0.96; noise 0.99) but also flags a correct in-game
  view with a black fogged minimap (0.84): use it as a filter for "have a look", never as
  proof either way.

Treat Jev's answers as routine decisions, not verdicts: a scenario's assertions are what
pass or fail a change.

## Example session

```text
> {"cmd": "click", "label": "Einzelspieler"}
< {"ok": true, "screen": "MainMenu", "target": {"id": "MainMenu/5", "kind": "text", "label": "Einzelspieler", ...}, "frames": 26, "ms": 3, ...}
> {"cmd": "click", "label": "Zufallskarte"}
< {"ok": true, "screen": "PlayerName", ...}
> {"cmd": "click", "label": "OK"}
< {"ok": true, "screen": "RandomMap", ...}
> {"cmd": "click", "label": "Start"}
< {"ok": false, "error": "no clickable widget labelled \"Start\"", "clickable": ["EINSTEIN", "Spanien", ..., "Mission spielen!", "Zurück zum Menü."], ...}
> {"cmd": "click", "label": "Mission spielen!"}
< {"ok": true, "screen": "game", "tick": 1, "ms": 822, ...}
> {"cmd": "drag", "around": {"owner": 0, "type": "Kri"}}
< {"ok": true, "around": 30, "rect": [[432, 270], [592, 334]], ...}
> {"cmd": "click", "label": "Fort"}
< {"ok": true, "target": {"id": "hud/0", "kind": "icon", "label": "Fort - Holz: 500 Stein: 100", ...}}
> {"cmd": "click", "near": {"owner": 0, "type": "Kri"}, "offset": [0, 700]}
> {"cmd": "wait", "ticks": 1}
> {"cmd": "state"}
< {"ok": true, "state": {"counts": {"0": {"sites": {"EngKre(SP)": 1}, ...}}, "resources": {"wood": 500, ...}, ...}}
```

With `dialogs auto` a mission briefing shows up in the next answer as
`"dismissed": [{"screen": "MessageWindow", "text": "Nachricht\nEin wildes und großes Land
...", "button": "OK", "tick": 39}]`.

## Limitations

- The driver sees what the menus and the HUD offer through their widgets; free drawing
  (the minimap's content, the diplomacy pages, the game panels) is not in the tree.
  Click those by position.
- `--drive` without GPU draws nothing, so nothing visual is tested; use `--drive-gpu` and
  `screenshot` for that. The GPU mode is slower (sprite uploads every frame).
- Clicking a unit in a crowd may select a neighbour that stands in front of it; use
  `drag around` or `select` when the exact unit matters.
- `start` shortcuts skip the menus' setup (player names, colours, teams): use the menus
  when they are what is tested.
- Jev's image questions are a preview feature: coarse, see above.

## Recipe for agents

1. Write or extend unit tests for the rule you changed; run `cargo test -p <crate>`.
2. If the change shows in the game, add or extend a scenario in `tests/e2e/` (copy the
   closest one; `start` shortcuts for setup, menus/HUD clicks for what is tested) and run
   `cargo xtask e2e --filter <name> -v --game-dir <data>`. Read failures from the answer
   (`error`, `clickable`, `unmet`, `blocking`, `log`) and `transcript.jsonl`.
3. To find your way through unknown screens use `ui` (or `cargo xtask jev explore --goal
   "..."`) instead of screenshots; use `jev_check` for fuzzy state checks.
4. Only for visual questions: `--gpu` and one `screenshot` with `crop`; look at it
   yourself only if `jev shot` or the question needs it.
5. Escalate to reasoning when Jev says "needs reasoning" or a scenario fails without a
   clear message.
