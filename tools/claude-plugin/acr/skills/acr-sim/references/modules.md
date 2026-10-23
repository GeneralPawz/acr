# `crates/acr-sim/src`: tick order and module map

## One tick: `World::step` (`world.rs`)

The original's `LongProcesses` (0x5112aa) order. Each line runs once per tick, top to bottom.

| # | Call | Notes |
|---|---|---|
| 1 | clear `sounds`, `born`, `captured`, `shouts`; `combat::forget_hits` | per-tick outputs for the host |
| 2 | `morale::decay` | panic timers |
| 3 | `wounds::bleed`, `body::get_up`, `ground_fire::tick` | return at once without `hit_locations` |
| 4 | `World::apply` for each `Command`, in order | only the player's own units (`World::owned`) |
| 5 | `peace::tick`, `desant::tick`, `economy::city_tick`, `armies::tick`, `market.process` | city and AI helpers |
| 6 | `formation::run_brigades` | brigade orders of the nation whose city tick it is (`tick & 7`) |
| 7 | `buckets.rebuild` | 128 px unit buckets and the start-of-tick snapshot |
| 8 | `economy::census`, `economy::upkeep` | census of nation `tick & 7`; food per head |
| 9 | `combat::search_victims` | auto-targeting (`SearchVictim` 0x52622b) |
| 10 | `ship::process_fishing`; `economy::bind_mines` every 256 ticks | |
| 11 | players 0..7, each player's units in creation order | buildings: `building_attack_link` / `leave_mine_link` / `building_link`, then reload and shot timers; mobile units: `motion::process_unit` |
| 12 | `spacing::separate` | returns at once without `body_spacing` |
| 13 | `capture::run` | `TryCapture` 0x528868 |
| 14 | `combat::object_loop` | reload counters, dying, corpses (0x5118b3) |
| 15 | `sprite::time_step` | fields and carcasses change |
| 16 | `projectile::process` | flight, explosions, round damage (`ProcessExpl` 0x557fa0) |
| 17 | `tick += 1` | |

`World::state_hash` (end of `world.rs`) hashes tick, `random`, units, brigades, occupancy, players, nation masks, market, sprites, morale, projectiles, bars and `ai`. Formation groups, thrown corpses, slow deaths, non-default `rules`, ground fires and a non-default peace are folded in only when present.

## Modules

| Module | What it holds (original functions named in the file header) |
|---|---|
| `world.rs` | `World`, `Unit`, `Order`, `Player`, `spawn`, `kill`, `step`, `apply` (commands), `state_hash` |
| `command.rs` | `Command` enum: plain data, one variant per player order |
| `replay.rs` | `Replay` (`ACRR` format): varint encoding, command tags, checkpoints |
| `hash.rs` | `StateHasher` (FNV-1a, `usize` as u64), `state_hash` |
| `rng.rs` | `SyncRandom` (`rando()` 0x584891 over `RANDOM.LST`); PCG32 `Rng` (unused by the world) |
| `fixed.rs` | `Pos` (i32, 1/16 px), `SUBPIXEL` |
| `trig.rs` | the engine's integer `TSin`/`TCos`/`TAtg`, `GetDir`, `norma` |
| `clock.rs` | `FixedClock`, `Tick`: real time to whole ticks |
| `catalog.rs` | `ObjectType::from_def` (`.MD` to integers), `Catalog`, `Attack`, `Adv` (per-player upgradable stats) |
| `assets.rs` | `SimAssets`: which files to read, `build()` to `SimData` (catalog, shapes, formations) |
| `weapon.rs` | `WEAPON.NDS` / `WEAPON.ADS` / `TEXTURES.LST`: projectile types (`Weapons::parse`) |
| `tech.rs` | nation-file tech tree: produce lists, enables, access control, upgrades |
| `upgrade.rs` | research in buildings and upgrade effects on `Adv` (`PerformNewUpgrade` 0x532281) |
| `map.rs` | `MapGrid`: heights, lock maps, areas; `MapGrid::open` is the flat test map |
| `path.rs` | fine path search (`CreatePrePath` 0x534031, wall followers) |
| `motion.rs` | land motion (`MotionHandler1` 0x50e823), `send_to`, `group_send`, `patrol` |
| `formation.rs` | brigades: `form`, `form_near`, `reform`, `fill`, `face`, `disband`, `run_brigades` |
| `grouping.rs` | grouped formations (`CM_DOGRP`/`CM_UNGRP`) |
| `states.rs` | stand ground, hold fire, melee only, friendly-fire avoidance, AI guard |
| `combat.rs` | `search_victims`, `attack_obj`, attack links, `make_damage` (0x4e8990), `trace_line`, `die`, `object_loop`, `HitRecord`/`DeathRecord` |
| `predict.rs` | `PredictShot` 0x559030: what a shot would hit first; side-step |
| `projectile.rs` | `AnmObject` slots: `create`, `process`, `explode`, `round_damage`, thrown corpses, `Plough` |
| `artillery.rs` | cannon: canister sector, `attack_point`, `COMPLEXROTATE` motion, `GunMode`, `gun_shot` |
| `bars.rs` | building 3D boxes (`3DBARS`) that stop shots and hide units |
| `morale.rs` | `Fear`, `TryPanic`, `PanicLink`, `MORALE.DAT` |
| `wounds.rs` | `Rules` (both optional rules), `pick_region`, `bleed` |
| `body.rs` | the damage model (`Body`, `injure`, outcomes per cause, mauling, `get_up`); custom `Hash` |
| `melee.rs` | men on the ground as targets, strike kinds, vital zones, finishing blows (rule) |
| `shrapnel.rs` | shell fragments, ball grazes, bones (rule) |
| `ground_fire.rs` | fires left by bursting shells (rule) |
| `spacing.rs` | body spacing push-apart (rule `body_spacing`) |
| `economy.rs` | stock, census, upkeep, starvation, prices, production, mines, entering buildings |
| `gather.rs` | peasants gathering from map sprites |
| `construction.rs` | placing buildings, builders, stages |
| `market.rs` | `ECONOMY.TXT` rates (the one place with IEEE doubles) |
| `sprite.rs` | changing resource sprites: fields, carcasses |
| `capture.rs` | capture of undefended `CAPTURE` objects and idle sites |
| `ship.rs` | ship motion, transports, broadsides, ports, fishing |
| `desant.rs` | AI landings by sea |
| `aiplace.rs` | AI building placement |
| `armies.rs` | enemy armies for the scripts (`GlobalEnemyInfo`) |
| `peace.rs` | peace-time countdown |
| `fog.rs` | fog of war for the local player (read by nothing in the sim) |
| `prof.rs` | frame profiler scopes; never touches state |
| `lib.rs` | crate rules (determinism), re-exports, `ORIGINAL_TICKS_PER_SECOND` = 10 |
