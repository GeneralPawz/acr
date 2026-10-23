---
name: acr-sim
description: "How to change game mechanics in crates/acr-sim correctly: determinism, the pinned state hashes, optional rules, commands and replay tags, the .MD catalog, tick order, module map, tests and known pitfalls. Load before editing or reviewing anything in acr-sim."
---

# Changing the simulation (`crates/acr-sim`)

The sim is the lockstep core: replays, savegames and (later) multiplayer depend on it being bit-identical everywhere. Load `acr-workflow` for the working rules and `acr-vm` to run tests. The tick order and a one-line map of every module are in `references/modules.md`.

## Determinism (`src/lib.rs` header)

- **Positions:** `Pos { x, y }` (`fixed.rs`) is i32 in 1/16 world pixel (`Pos::from_pixels`, `pixels()`); directions are `u8`, 0 = east, 64 = south. Distances use the engine's integer `trig::norma`, angles `trig.rs` tables.
- **No floats in state.** The only exception is `market.rs` (the original's doubles, IEEE `+ * /` only). Physics, ragdolls and gore are render-side (`acr-physics`, `acr-game`) and never feed back.
- **Randomness:** game logic draws only from `World::random: SyncRandom` (`rng.rs`). `rando()` (0x584891) walks the 8192-entry `RANDOM.LST` table, which `rng.rs` generates (MSVC `rand()` after `srand(1)`). Every draw moves the index for everything after it, so draw exactly where the original draws, in its order. `Rng` (PCG32) exists but the world does not use it.
- **Order:** iterate in a defined order (unit index, `BTreeMap`). `World::deaths` is a `HashMap`, render only; never iterate one in sim logic. No I/O, clocks or threads (the wasm build must compile).
- **Render-only fields** on `World` (`hits`, `impacts`, `fragments`, `deaths`, `hit_dir`, `hit_energy`, `hit_height`, `hit_cause`, `plough_carry`, `shot_carry`, `desant`, `armies`) are not hashed. Savegames re-run the commands, so they come back by themselves.

## The pinned state hashes

`World::state_hash()` (end of `world.rs`) is FNV-1a (`hash.rs`) over the dynamic state. Two tests pin it on the original rules:

| Test | Scene | Pinned |
|---|---|---|
| `tests/wounds.rs` `off_by_default_the_fight_is_the_original_one` | 10 pikemen + musketeers per side, 1500 ticks | `0x91d6_a8c1_1ead_2450` |
| `tests/plough.rs` `the_rule_off_keeps_the_original_hash` | a cannonball through a file of men | `0x6bbc_90f6_d730_18e6` |

- Anything that changes the original rules changes them: a new or removed `rando()`, a new field in a hashed struct (`Unit`, `Order`, `Player`, ...), a changed constant. Past examples: `22cc0c6` (a new `Order::Attack` field), `1aa61a2` (`GetDamagePoint`'s two draws), `c33cd90` (one draw fewer per destroyed building).
- When a parity fix changes them on purpose: update the constant, extend the comment above it with the reason, and say so in the commit message ("state hashes change (intended)").
- **An optional rule must leave both hashes unchanged when it is off.** Never update them for a rule.

## Optional rules (how `hit_locations` and `body_spacing` do it)

1. Add a `bool` to `Rules` in `src/wounds.rs` (yes, there; re-exported as `acr_sim::Rules`). `World::state_hash` folds `rules` in only when it differs from `Rules::default()`.
2. Gate every new path on `w.rules.<rule>`, early, with no `rando()` drawn when off (`body::get_up`, `spacing::separate` and `ground_fire::tick` return at their first line; `wounds::pick_region` returns `None` before drawing).
3. New state must not hash while unused: the custom `impl Hash for Body` (`body.rs`) hashes nothing while the body is default; new `World` lists are folded in only while non-empty (`ground_fires`, `thrown`, `slow_deaths` in `state_hash`).
4. Wire it in `acr-game`: `ui/settings.rs` (`<rule>=on|off`), `lib.rs` `start_game` (setting into `setup.rules`), `savegame.rs` (the rules varint: bit 0 `hit_locations`, bit 1 `body_spacing`, written last since save version 3), `ui/screens/game_options.rs` (menu entry), `drive/protocol.rs` and `drive/mod.rs` (`start` options for `random` and `blank`).
5. Tests: off keeps the pinned hashes, on is deterministic (two runs, equal hashes) and differs from off (`tests/wounds.rs` `with_hit_locations_hits_land_on_regions_deterministically`, `tests/spacing.rs`).
6. Prefer changing what an existing command does under the rule over a new command (`240afe8`: gun modes reuse the attack and `ATTACKGROUND` commands).

## Adding a command

1. Variant in `Command` (`command.rs`), doc comment naming the original's network command and executor (e.g. `Destruct`: network command 0x21, `KillSelected` 0x4dc83c).
2. Apply it in `World::apply` (`world.rs`). Filter units with `World::owned(player, ids)` (own, active, not `unlimited`, not out of the fight), sorted and deduplicated.
3. Replay encoding in `replay.rs`: a tag byte, then the player byte, then fields (`ids`, zigzag varint `i` for i32, `u` for unsigned, `byte`, `str`). Add it to `Writer::command`, `Reader::command` and the `round_trips_every_command` test. Tags in use: 0–32 and 40–46; free: 33–39, 47+. Never renumber a tag (old replays and saves hold them). `VERSION` stays 1 for a new tag.
4. Senders: the HUD (`acr-game/src/ingame/interface.rs`), the driver (`acr-game/src/drive/state.rs` `command`, for tests). Scripts (`acr-mission`) call sim functions directly between ticks and are not recorded.
5. There is no network transport yet; commands per tick are the lockstep unit, and `World::state_hash` is the desync check.

## The catalog from the original data

- `SimAssets` (`assets.rs`) is sans-I/O: `start(paths)` lists the files (`NMLIST.TXT`, `ORDERS.LST`, `ORD_GROUPS.LST`, `NRES.DAT`, `ECONOMY.TXT`, `MORALE.DAT`, `WEAPON.NDS`, `WEAPON.ADS`, `TEXTURES.LST`, the sprite lists, every nation `.NDS`); `add_file` asks for each `.MD` named in `NMLIST.TXT`; `build()` makes `SimData { catalog, shapes, formations }`.
- `build()` adds one `ObjectType::from_def(member_id, &UnitDef)` per nation member (`catalog.rs`), so a type id is like `EngMus(EN)` (`Catalog::find`). `.MD` keywords are read there into plain integers; for a new keyword add a field to `ObjectType` and read it in `from_def`; its argument grammar is in `grammar()` of `acr-formats/src/defs/md.rs` (from the original's `sscanf` formats).
- Per-player stats that upgrades change live in `Adv` (`catalog.rs`, `AdvCharacter`): attacks, protection, life, cost, rates. Read them with `World::adv(player, kind)`, not from `ObjectType`, wherever the original reads `AdvCharacter`. Upgrades (`upgrade.rs`) replace the player's `Arc<Adv>`; the tech tree is `tech.rs`.
- Weapons (`weapon.rs`) come from `WEAPON.NDS` (projectiles) and `NRES.DAT [WEAPONS]` (weapon kinds, `Catalog::set_weapon_kinds`).

## Tests (`crates/acr-sim/tests/*.rs`)

- One file per topic (`combat`, `projectile`, `predict`, `plough`, `switches` (canister), `body`, `melee`, `wounds`, `shrapnel`, `ground_fire`, `spacing`, `formation`, `morale`, `economy`, `upgrade`, `ships`, `world`, ...). Each starts with a `//!` line saying what it covers.
- **Synthetic catalogs, no game data:** `.MD` text as Rust string constants with `\r\n` line ends, `Catalog::new()`, `c.add(ObjectType::from_def("Pik(EN)", &UnitDef::parse(...)))`, `c.set_weapon_kinds(&weapon_kinds(NRES))`, `c.set_weapons(Weapons::parse(NDS, Some(ADS), None))`, then `World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default()))`, `w.spawn(kind, owner, Pos::from_pixels(x, y), dir)`, `w.step(&[...])`. Copy `setup()` from the closest test.
- Run one file on the VM: `acr-vm-cargo -- cargo test -q -p acr-sim --test wounds`. Full landing checks: `acr-vm-cargo -- acr-check`.
- Real data: `acr sim-check <game-dir> <map> [ticks]` runs a map twice and compares hashes. In-game behaviour: `tests/e2e/*.json` scenarios (`docs/testing.md`) or a driver run on the blank field (skills `acr-driver`, `acr-vm`).

## Docs to read on demand (one section, not the whole file)

| Doc | Sections |
|---|---|
| `docs/simulation.md` (2100 lines) | Game speed · Model (Randomness, Trigonometry) · Passability · Pathfinding · Motion · Formations · Combat (Data, Rules, Capture, Friend and foe, Priorities and unit states, Formation melee, Morale, Projectiles, Buildings that shoot, Artillery, Fire, Hit locations) · Economy · Fog of war · Using the simulation (replays, savegames) · Deviations and simplifications · Verification |
| `docs/damage-model.md` | 1. Simulation: `acr_sim::body` (layout, state, outcomes, shell fragments, grazes, Gun modes, mauling, animals) |
| `docs/melee.md` | 1–6: men on the ground, finishing blows, strike kinds, vital zones, render contract |
| `docs/body-sim.md` | "Split of responsibility" (what is sim, what is render), 4. Interruptible actions |
| `docs/body-spacing.md` | the rule: bodies, formations, the push, limits |

## Known pitfalls (from `git log -- crates/acr-sim`)

- Synthetic `.MD` lines need `\r\n`: a pattern that missed the CR left `RAZBROS` in a test that meant to remove it (`1aa61a2`).
- The data's `USAGE` is not the role: dragoons are `STRELOK`, so riders are also found by `GEOMETRY` radius 15 (`e76ad1c`). Check the real `.MD` (skill `acr-original`).
- Upkeep runs everywhere: units without food starve, which looks like friendly deaths on test fields (`e76ad1c`).
- A UI-visible problem may belong in `acr-game`, not the sim: dead brigade members stay in the brigade as in the original, so `dbf8981` fixed the counts in the UI and kept the hashes.
- An e2e scenario must not depend on timings that random draws move (`3a9381c`).
- A deliberate deviation from the original goes into `docs/simulation.md` "Deviations and simplifications" with the address it deviates from (`ac01955`, the skipped friendly-fire look 0x559a40).
- Cite the `dmcr.exe` addresses you followed, in the code comments and the doc (style: `` `KillSelected` (0x4dc83c) ``, object fields `OB+0x8c`, type fields `NM+0x172a`).
