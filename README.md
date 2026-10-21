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
- [Multiplayer](https://github.com/GeneralPawz/acr/issues/51) (#51)
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

Everything after M1 is an optional layer: the original game must stay playable exactly as
it was. Where 3D models are missing, a fixed camera matching the original view keeps the
illusion intact.

## Layout

```
crates/
  acr-formats   original data formats; pure, no I/O required (native + wasm32)
  acr-sim       deterministic fixed-tick simulation (clock, RNG; game rules to come)
  acr-script    mission/AI scripting: PE reader now, x86 interpreter + API later
  acr-render    wgpu renderer: palette-indexed sprite atlas, layer blend modes
  acr-viewer    sprite viewer app (winit); native binary and wasm entry point
  acr-tool      `acr` CLI for inspecting game data and validating decoders
xtask/          build automation in Rust (`cargo xtask web|serve`)
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

DirectPlay, Wintab, the GameSpy/`iChat`/`DipServer` lobby, `binkw32` video (to be
converted or skipped), the external `vopl.exe` music process, DirectDraw palette tricks,
copy protection.

## Play

```sh
cargo xtask dist --game-dir "C:\path\to\American Conquest" --install
```

builds `AmericanConquest.exe` with the game's original icon and copies it into the game
folder; double-click it there. The executable also works from anywhere else: it looks for
the game next to itself, in `ACR_GAME_DIR`, in the folder remembered from an earlier run,
and otherwise asks once with a folder picker. Prebuilt executables (with a stand-in icon)
and a browser build are attached to GitHub releases (`v*` tags).

## Usage

```sh
# Inspect data
cargo run --release -p acr-tool -- list     "C:\path\to\American Conquest" .MD
cargo run --release -p acr-tool -- cat      "C:\path\to\American Conquest" ANMBIZ.MD
cargo run --release -p acr-tool -- gp-sheet "C:\path\to\American Conquest" ATCCEN.GP sheet.png
cargo run --release -p acr-tool -- gp-check "C:\path\to\American Conquest"
cargo run --release -p acr-tool -- script-api "C:\path\to\American Conquest" > docs/script-api.md

# Sprite viewer (Left/Right file, Up/Down frame, Space play, G grid, +/- zoom, 1-8 player)
cargo run --release -p acr-viewer -- "C:\path\to\American Conquest" ENGB17C.GP

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
- [`.m3d` maps](docs/formats/m3d.md)
- [Mission/AI scripting API](docs/script-api.md) (generated)
