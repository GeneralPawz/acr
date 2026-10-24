---
name: acr-driver
description: "The ACR game's headless test driver (JSON lines). Covers the blank test field, spawning units, scripted hits, camera close-ups, recording frames, profiling, and the house rules for test shots. Use when a change must be seen or measured in the running game."
---

# Driving the game for tests and shots

The full reference is `docs/testing.md`; read its "The driver", "The close-up bench" and "The blank field" sections only when this summary is not enough. Run the steps through `acr-vm-drive` (skill `acr-vm`).

## House rules for every test and render

- **Field:** always the blank field, with units spawned directly and no map, fog or trees. Use a white ground for clean cut-outs and grass only when the ground matters.
- **HUD:** off (`"hud": false`), unless the HUD itself is under test.
- **One field per driver process:** a second `start` in the same process hangs. One `acr-vm-drive` call is one process.
- **Settings:** the showcase settings apply (models=all, gore=full, controls=modern). Put rules like `hit_locations` in the `start`.
- **Paths:** absolute output paths. Trim or sheet the frames before looking (`acr-shots`).
- **Two owners fight on their own.** Give units one owner when they should only stand.

## A typical session

```json
[{"cmd":"start","blank":{"ground":"white","hud":false,"hit_locations":true,"editor":true}},
 {"cmd":"spawn","type":"EngM18","x":7680,"y":7680,"dir":0},
 {"cmd":"spawn","type":"AnmMed","x":7740,"y":7680,"dir":128,"owner":1},
 {"cmd":"wait","ticks":20},
 {"cmd":"camera","x":7700,"y":7680,"zoom":5,"yaw_deg":0,"pitch_deg":10,"lift":27},
 {"cmd":"screenshot","path":"side.png"},
 {"cmd":"hit","unit":0,"cause":"cannonball","region":"torso","dir":[-1,0],"energy":80,"damage":1000},
 {"cmd":"record","dir":"frames","count":20,"every":3},
 {"cmd":"quit"}]
```

The blank field's `centre` is `[7680, 7680]` (normal size). `dir` runs 0..255: 0 east, 64 south, 128 west, 192 north.

## Requests you need most

| Request | Use |
|---|---|
| `start` + `blank`: `ground` (grass, dirt, snow, white, greenscreen), `hud`, `hit_locations`, `body_spacing`, `editor`, `slope`, `hill`, `water`, `corpse_decay`, `decay_stain`, `ground_cover` (off on white/greenscreen unless `true`) | the field |
| `spawn`: `type` (catalog id or a part, e.g. `EngM18`, `ArtPus`, `AnmMedSm`), `x`, `y`, `dir`, `owner`, `pose` (stand, walk, crawl), `lost`, `wounds`, `life`, `hold_fire`, `resources` | one unit; the answer gives `unit.id` |
| `camera`: `x`, `y`, `zoom` (5 means a man about 270 px tall), `yaw_deg` (0 = looks north), `pitch_deg` (10 = near side view), `lift` (px above ground, 27 = waist) | close-up; `"free": true` for free orbit |
| `hit`: `unit`, `cause` (musket, arrow, cannonball, grapeshot, shell, sabre, bayonet, axe, claw), `region` or `part` + `level`, `dir`, `energy`, `damage` (1000 kills) | a scripted hit through the real injury path |
| `impact`: `at`, `cause` (shell, cannonball), `by` (a unit id) | a shell burst or ball graze |
| `command`: `type` move, attack, attack_ground, stop, form, …; `units`, `x`, `y`, `target` | sim orders |
| `wait`: `ticks`, `until` | advance time (10 ticks/s at normal speed) |
| `screenshot`: `path`, `crop` | one frame |
| `record`: `dir`, `count`, `every`, `crop` | a frame sequence `frame000.png`… |
| `state`: `units`, `detail` | positions, life, animation and frame, the reload counter |
| `hud`: `on` | show or hide the interface |
| `profile` | frame timings (also `ACR_PROFILE=1`) |
| `click` / `key` / `drag`: `label`, `pos`, `unit`, `world` | the interface, e.g. editor panel buttons by `pos` |

## The studio (#299): exact poses, framed cameras, sheets, measurements

The best tool for looking at models and animations. These commands need `--drive-gpu` (which `acr-vm-drive` always passes) and `models=all`. The details are in `docs/testing.md`, "The studio (#299)".

| Request | Use |
|---|---|
| `{"cmd":"studio","ground":"white","units":[{"type":"EngM18","x":7680,"y":7680,"dir":0}]}` | a blank field with only these units, HUD off, sim held, no ground cover on white (`"ground_cover":true` adds it); answers the ids and a framing camera |
| `{"cmd":"pose","id":0,"clip":"ATTACK1","frame":12}` (or `"t"` in seconds, `"phase"` 0..1; `"run":true`, `"release":true`) | holds the model at that clip time |
| `{"cmd":"camera","frame":[0],"view":"side","part":"hands","margin":0.15}` | fits the posed model or a part (hands, head, feet, weapon) into the window. Views: front, back, side (his right), three_quarter, top; relative to his facing |
| `{"cmd":"sheet","path":"s.png","ids":[0],"views":["front","side","three_quarter"],"clips":[{"clip":"ATTACK1","frames":[0,10,20]}],"cell":[360,500]}` | a labelled grid rendered in the game: rows are unit × time, columns are views |
| `{"cmd":"measure","id":0}` | JSON: the clip, time and frame, the hand sockets, each hand's distance to each held item (origin, along the axis, gap in mm) |
| `{"cmd":"measure","id":0,"ticks":40}` | per-tick feet and ground point while it moves, plus `sliding` (raw, not yet calibrated) |

Prefer `studio` + `pose` + `sheet` + `measure` over `start` + `wait` + `record` for any model or animation question. A held pose can still be overridden by strike, ragdoll or crew layers.


## Live sessions: the `acr-studio` MCP tools

For looking around interactively (pose, frame, look, adjust, look again), keep one game running instead of a new `acr-vm-drive` run per look:

| Tool | Does |
|---|---|
| `open_session(worktree, build=true, models="")` | builds the worktree's game on the VM (incremental builds take seconds; `build=false` reuses the last build) and starts it on the GPU. `models` takes a glb folder or `wt:target/assets/units`. Returns the session id |
| `send(session_id, requests)` | driver requests in order (`studio`, `pose`, `measure`, `camera`, `hit`, `wait`, …); one short answer per request |
| `shot(session_id, camera={...}, max_px=640)` | applies the camera fields, screenshots, and returns the image trimmed to its content (`save_to` keeps the full PNG) |
| `fetch_image(session_id, path)` | returns a file the game wrote (e.g. a `sheet`) |
| `close_session(session_id)` | always, when done |

- **One try, then `acr-vm-drive`.** `open_session` returns `session ...` or `ERROR: ...` (with the reason) within 6 min in total, sync and build included; while it works it reports progress ("building: Compiling acr-game (1:10 of max 6:00)"). After **one** ERROR, do not retry or wait: run the same requests as a steps file with `acr-vm-drive` (a scene preset below is a good start). Sessions of different worktrees run in parallel (own run folder per session; one game per session on the shared GPU display).
- **Timeouts:** `open_session` 6 min total (the ready line within 90 s of it); `send` 60 s per request (`timeout=` raises it for a long `wait`, `sheet` or `record`); `shot`/`fetch_image` 60 s. On a send/shot timeout the tool returns `ERROR: ...` with the tail of the game log and closes the session (game killed by the PID in `<run folder>/pid`), because a late answer would desync it. A build that hits the limit keeps running on the VM, so a later `open_session` is quick. `open_session` first closes any earlier session of the same worktree, and sessions idle for 30 min. Env overrides: `ACR_STUDIO_OPEN_TIMEOUT`, `_START_TIMEOUT`, `_SEND_TIMEOUT`, `_IDLE_REAP`.
- A typical round trip (open without a build, studio, pose, measure, shot) takes about 5 s.
- Use `acr-vm-drive` instead for fixed scripts, batches and before/after runs that must be repeatable.
- One session is one field: to change the field, close and open another.

## Recipes

- **Animation check:** `studio` with the unit, then `sheet` over the clip's frames and views. For motion over time (walks, deaths): `record` with a framed `camera`, then `acr-shots sheet`.
- **Before/after:** run the same steps against main and against your branch. Use `--no-build` with `--models` when only models changed. Sheet the two side by side.
- **Kill for corpse or decay checks:** `hit` with `damage: 1000`, then `wait` the ticks you need (`corpse_minutes` in the settings; decay runs over the corpse's lifetime).
- **Measure, don't just look:** read `answers.jsonl` (`state` with `detail` gives the animation, frame and positions) and `game.log`, and judge by numbers where you can.

## Scene presets

Tested steps files in `presets/` (this skill's folder), verified on main 2026-10-24. Blank grass field (walk: white), HUD off, hit locations on, owner 0 shoots east at owner 1. Run one as is:
`acr-vm-drive --steps <plugin>/skills/acr-driver/presets/<file> --out C:/abs/out [--no-build] [--models ...]`,
or copy it, change unit types and camera, and feed the requests to studio `send` (drop `start`; use `studio` or keep `start` as the first request of a fresh session).

| File | Shows | Checked by (answers.jsonl) |
| --- | --- | --- |
| `archer_volley.json` | 5 `HurLut` archers volley at 5 stationary targets 460 px away; frames in `volley/` | archers stay at x 7450; target life 2000 to 1884 |
| `musket_volley.json` | 5 `EngM18` fire a volley at targets 500 px away; `volley/` | shooters stay put, reload (`delay`) runs; target life drops |
| `cannon_ground.json` | `ArtPus` fires at a ground point 1000 px east; `fire/`, `after.png` | `AttackPoint` order, reload counter running, iron and coal paid |
| `mortar_ground.json` | `ArtMrt` (rule `mortars`) shells a ground point 1400 px east; `fire/` | as the cannon; craters at the point |
| `building_artillery.json` | `ArtPus` shoots a `SpnDo1` house (life 20000 so it lasts); `fire/` | house life 20000 to 8116 over 780 ticks |
| `melee_clash.json` | 12 `EngM18` against 12 `SpnMus`, both `hold_fire` (bayonets only), meet in the middle; `clash/` | 24 men down to 9 by tick 286, all between x 7590 and 7800 |
| `walk_line.json` | one `EngM18` walks 1400 px east in a straight line, side camera, 12 frames in `gait/` | x 7080 to 8472, `Motion*` animation |

Why the presets look like this (the traps that cost earlier runs their time box):
- **Stationary targets** are `pose: "crawl"` men of owner 1 with `life: 2000`: they take no orders and do not fight back. Shooters do not pick crawlers on their own, so each shooter gets an explicit `attack` command. A standing enemy, even with `hold_fire`, charges into melee; a hit crawler may crawl a little.
- **Archers charge** when the target is beyond their range (they walk up to about 480 px): keep archers within about 460 px, muskets within about 500 px.
- **Guns** need `resources` on the spawn (a shot costs iron and coal), else they never fire. A gun reloads slowly (`delay` 1600 ticks for `ArtPus`, 2400 for `ArtMrt`): expect one shot per 300 ticks.
- `attack_ground` and `attack` work on guns; a mortar needs `"mortars": true` in `start.blank`.

Missing in the driver (`command`, `drive/state.rs`): no `stand_ground`, `hold_fire` (the sim's `HoldFire`), `melee_only`, `face` or `move_facing` orders, although the sim has `Command::StandGround`, `HoldFire`, `MeleeOnly`, `Face` and `MoveFacing`. Commands are always sent as player 0, so owner 1's units cannot be ordered (an order for them is queued but does nothing). Spawn has only `hold_fire` (melee only).
