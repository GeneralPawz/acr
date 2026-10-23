---
name: acr-map
description: "One-page map of the ACR repository (crates, key modules, docs and what each covers) for finding the right file fast. Use before searching ACR code or docs."
---

# ACR map

## Crates (`crates/`)

| Crate | Owns |
|---|---|
| `acr-sim` | the deterministic fixed-tick simulation: units, motion, combat, projectiles, artillery, body and injuries, formations, morale, economy, upgrades, commands and replay |
| `acr-game` | the game app: menus, HUD, input, editor, driver, settings, saves, sounds, and the 3D views (`src/ingame/`) |
| `acr-render` | the GPU renderer (wgpu; native and WebGPU/WebGL2): sprites, terrain, models, skinning, shadows, decals |
| `acr-physics` | XPBD active ragdolls (render side, native and wasm) |
| `acr-formats` | readers for the original's formats: GP sprites, GSC archives, .MD/.NDS definitions, m3d maps, fonts, Bink |
| `acr-app` | the platform layer: GPU setup, asset loading |
| `acr-audio` | WAV/Vorbis decoding, the mixer, output on the `acr-audio` thread |
| `acr-mapgen` | the random map generator (0x45CD31) |
| `acr-mission`, `acr-script` | running the original mission and AI DLLs on the simulation |
| `acr-tool` | the `acr` CLI that inspects and extracts original data (`acr model-ref`, `acr model-check`, …) |
| `acr-viewer`, `acr-video`, `acr-serve` | the sprite/map viewer, the Bink decoder, the local web server |
| `xtask` | `cargo xtask assets`, `e2e`, `drive`, `jev`, `serve` |

## `crates/acr-game/src/ingame/`: the 3D and gameplay views

`three_d.rs` is the 3D frame. These are its views:

| Area | Modules |
|---|---|
| Units | `strike_view` (melee and finishing clips), `crew_view` (gun crews), `gait_view`, `gear_view` (lagging gear), `cloth_view`, `maul_view` (animals) |
| Death | `ragdoll_view`, `damage_view`, `gore`, `debris` (pieces), `tears`, `living_wounds`, `piles` |
| Ground | `splatter`, `ground_marks`, `kept_marks`, `seep` (decay stains), `trample`, `trail`, `water_view` |
| Effects | `particle_view`, `effects` |
| Guns | `gun_marks` (mode overlays), `canister_mark` |
| Interface | `interface` (selection and panel logic), `panels`, `controls` (original/modern), `editor` (test editor), `probe` (driver queries), `save`, `shouts` |

The HUD is drawn in `crates/acr-game/src/hud/`. The driver is `crates/acr-game/src/drive/`.

## Assets

- `assets/**.py` are the asset scripts (one per unit, gun or animal).
- `assets/blender/lib/acr_assets/` is the Blender library: rig, clips, drape, grips, outfit, families.
- `assets/blender/tests/` holds the Python tests. `assets/sounds/` holds our own sounds.

## Docs (`docs/`): read the section you need, not the whole file

| Doc | Covers |
|---|---|
| `simulation.md` (2100 lines) | game speed, model, pathfinding, motion, formations, combat, projectiles, artillery, hit locations, economy, fog, deviations |
| `damage-model.md` | injuries, split bodies, debris, cannonball outcomes, shell fragments, gun modes |
| `melee.md` | men on the ground, strike kinds, critical hits, finishing blows |
| `body-sim.md`, `body-spacing.md` | ragdolls and physics; the body-spacing rule |
| `rendering-3d.md` (1190 lines) | settings, model files, architecture, shadows, skinning, animation states, decay and gore, camera math, performance |
| `ground.md`, `water.md` | 3D ground materials and marks; water |
| `assets.md` | asset pipeline, units and axes, the original camera, sprite check, export |
| `assets-humans.md` | skeleton H, export contract, faces, held gear, grips, clips, straps, layers, decay |
| `clothing.md` | sewn garments and drape, the roll-out table |
| `humans-roster.md`, `families/*.md` | every human unit and its family recipe |
| `ui.md`, `controls.md` | menus, dialogs, HUD, settings pages; keys, editor |
| `testing.md` | the driver, bench, blank field, scenarios |
| `formats/*.md` | the original's file formats |
| `script-api.md`, `script-runtime.md`, `mapgen.md` | missions, AI, random maps |

## Outside the repo

- `C:\Users\verwalter\WIP\American Conquest`: the original game, read-only.
- `C:\Users\verwalter\WIP\ACR-analysis\`: frames, sheets and analysis scripts by topic.
- `showcase\` in there: the user's play build, models and settings.
