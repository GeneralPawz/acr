# ACR — American Conquest, Rebuilt

A from-scratch Rust reimplementation of the *American Conquest* (GSC Game World, 2002)
engine that runs on the original game data. Targets native desktop (Windows on ARM
included) and the browser (WebAssembly + WebGPU/WebGL2).

No original game files are part of this repository. Players supply their own copy.

## Status

Progress is tracked in GitHub issues; each tracking issue shows its sub-issues and how many are done. All of them belong to [milestone M1](https://github.com/GeneralPawz/acr/milestone/1).

- [Archives and virtual file system](https://github.com/GeneralPawz/acr/issues/1) (#1)
- [Sprites and sprite viewer](https://github.com/GeneralPawz/acr/issues/7) (#7)
- [Maps and terrain](https://github.com/GeneralPawz/acr/issues/14) (#14)
- [Unit, building and nation definitions](https://github.com/GeneralPawz/acr/issues/20) (#20)
- [Deterministic simulation](https://github.com/GeneralPawz/acr/issues/26) (#26)
- [Mission and AI scripting](https://github.com/GeneralPawz/acr/issues/35) (#35)
- [UI, sound and music](https://github.com/GeneralPawz/acr/issues/43) (#43)
- [Build, CI and packaging](https://github.com/GeneralPawz/acr/issues/55) (#55)

## Roadmap

1. [**M1 – Modern architecture**](https://github.com/GeneralPawz/acr/milestone/1): replace
   everything possible of the original engine with the new architecture (mostly Rust). The
   original data, maps, missions and AI run on modern hardware and in the browser.
2. [**M2 – 3D models**](https://github.com/GeneralPawz/acr/milestone/2): 3D models for all
   buildings, combatants, animals, vegetation, … so the camera can orbit, with animations and
   state variants (construction, destruction). One tracking issue per category, one
   sub-issue per model.
3. [**M3 – LLM mission AI**](https://github.com/GeneralPawz/acr/milestone/3): far better
   mission and opponent AI with LLMs and System One models.
4. [**M4 – New content**](https://github.com/GeneralPawz/acr/milestone/4): new nations,
   missions and campaigns.

Deferred: [**Multiplayer**](https://github.com/GeneralPawz/acr/milestone/5)
([tracking issue #51](https://github.com/GeneralPawz/acr/issues/51)), lockstep networking
replacing DirectPlay, after single player is complete.

Everything after M1 is an optional layer: the original game must stay playable exactly as
it was. Where 3D models are missing, a fixed camera matching the original view keeps the
illusion intact.

## Layout

```
crates/
  acr-formats   original data formats; pure, no I/O required (native + wasm32)
  acr-mapgen    random map generator, reproduced from the original (pure, native + wasm32)
  acr-sim       deterministic fixed-tick simulation: world, original pathfinding, motion, formations
  acr-script    mission/AI scripting: PE reader, x86 interpreter, KERNEL32 shims
  acr-mission   the game API of the scripts on acr-sim; runs missions and AI DLLs
  acr-video     Bink 1 video + audio decoder for the intro movies (native + wasm32)
  acr-render    wgpu renderer: palette-indexed sprite atlas, layer blend modes, map terrain
  acr-viewer    sprite and map viewer app (winit); native binary and wasm entry point
  acr-tool      `acr` CLI for inspecting game data and validating decoders
  acr-serve     local web server: `cargo xtask serve` and the `AmericanConquest-Web` launcher
xtask/          build automation in Rust (`cargo xtask web|serve|dist|assets`)
assets/         Blender scripts that build the 3D models (M2), see docs/assets.md
docs/formats/   reverse-engineered format specifications with evidence
```

Everything is done through the workspace: decoders are validated against the full game
data by `acr` subcommands rather than ad-hoc scripts, and the web build needs no tools
beyond `cargo`.

## Design rules

- **Fixed simulation tick, decoupled from rendering.** The original ties game speed to
  frame rate, which is why it runs too fast on modern hardware. Game speed here is a
  tick rate (`acr_sim::FixedClock`), never a side effect of CPU or display speed.
- **Deterministic simulation** (integers/fixed point, seeded PCG32, ordered iteration) so
  replays and lockstep multiplayer work without DirectPlay.
- **Sans-I/O formats.** Parsers take bytes; `GscIndex` + `Vfs::locate` let the browser
  fetch byte ranges asynchronously, so the 700 MB archive is never loaded whole.
- **Palette on the GPU.** Sprites stay 8-bit indices; player colours and blend modes are
  resolved in the shader.

### Dropped from the original

DirectPlay, Wintab, the GameSpy/`iChat`/`DipServer` lobby, `binkw32.dll` (the `.bik`
videos play through our own decoder, `acr-video`), the external `vopl.exe` music process, DirectDraw palette tricks,
copy protection.

## Play

**From a release** ([GitHub releases](https://github.com/GeneralPawz/acr/releases)): unzip
`AmericanConquest-windows-x64.zip` (or `-arm64` for Windows on ARM) into your American
Conquest folder, or anywhere else.

- `AmericanConquest.exe` — the game. Double-click it; it looks for the game next to
  itself, in `ACR_GAME_DIR`, in the folder remembered from an earlier run, and otherwise
  asks once with a folder picker.
- `AmericanConquest-Web.exe` — the browser version. Double-click it: it serves the `web`
  folder on localhost and opens your browser, with the game files of the folder it finds
  the same way (or a folder picker in the page). Close its window to stop it.

The original game files are not included; you need your own installation. See the
[release notes](docs/release-notes.md) for what works and what does not yet.

**From source**:

```sh
cargo xtask dist --game-dir "C:\path\to\American Conquest" --install
```

builds both executables (with the game's original icon) and the browser build into
`dist/` and copies them into the game folder; double-click them there. Release builds from
CI carry a stand-in icon.

## Controls

The game keeps the original keys and mouse by default; an optional modern scheme (wheel
zoom, drag panning, W/A/S/D) can be chosen in the options. Both are listed in
[docs/controls.md](docs/controls.md).

## Usage

```sh
# Inspect data
cargo run --release -p acr-tool -- list     "C:\path\to\American Conquest" .MD
cargo run --release -p acr-tool -- cat      "C:\path\to\American Conquest" ANMBIZ.MD
cargo run --release -p acr-tool -- gp-sheet "C:\path\to\American Conquest" ATCCEN.GP sheet.png
cargo run --release -p acr-tool -- gp-check "C:\path\to\American Conquest"
cargo run --release -p acr-tool -- script-api "C:\path\to\American Conquest" > docs/script-api.md
cargo run --release -p acr-tool -- bik-check "C:\path\to\American Conquest"
# Run a map's units headless for 3000 ticks, twice, and compare state hashes
cargo run --release -p acr-tool -- sim-check "C:\path\to\American Conquest" Missions\England\01\eng01.m3d 3000
# Run a mission with its original script headless and print what it does
cargo run --release -p acr-tool -- mission-run "C:\path\to\American Conquest" "#columb1@" --ticks 3000

# Sprite viewer (Left/Right file, Up/Down frame, Space play, G grid, +/- zoom, 1-8 player)
cargo run --release -p acr-viewer -- "C:\path\to\American Conquest" ENGB17C.GP
# Map viewer (arrows/window edge/right drag scroll, wheel or +/- zoom, PageUp/PageDown map, Tab: sprites)
cargo run --release -p acr-viewer -- "C:\path\to\American Conquest" --map eng01 --zoom 0.5

# Browser build: pick the game folder in the page ...
cargo xtask serve --release
# ... or let the local server provide the archives: open http://127.0.0.1:8080/?game=/game/
cargo xtask serve --release --game-dir "C:\path\to\American Conquest"
```

Requires the `wasm32-unknown-unknown` target for the browser build
(`rustup target add wasm32-unknown-unknown`).

## Formats

- [`GSCfmt` archives](docs/formats/gsc.md)
- [`GPAK` sprites](docs/formats/gp.md)
- [Fonts (`RLCFont`, `.GPI`, `.RLC`)](docs/formats/fonts.md)
- [`.m3d` maps](docs/formats/m3d.md)
- [Terrain rendering](docs/formats/terrain.md)
- [Units, buildings, nations, sounds, map pieces](docs/formats/definitions.md)
- [Random map generator](docs/mapgen.md)
- [Mission/AI scripting API](docs/script-api.md) (generated)
- [Simulation: game speed, pathfinding, motion, formations](docs/simulation.md)
- [3D assets: pipeline, original camera, sprite check](docs/assets.md)
