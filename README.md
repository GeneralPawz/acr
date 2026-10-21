# ACR — American Conquest, Rebuilt

A from-scratch Rust reimplementation of the *American Conquest* (GSC Game World, 2002)
engine that runs on the original game data. Targets native desktop (Windows on ARM
included) and the browser (WebAssembly + WebGPU/WebGL2).

No original game files are part of this repository. Players supply their own copy.

## Status

| Milestone | State |
|---|---|
| 1. Archives + virtual file system (`ALL.GSC`, `*.GS1`, loose files) | done |
| 2. Palettes and `GPAK` sprites, GPU sprite viewer (native + browser) | done; blend strengths approximate |
| 3. Map format (`.m3d`) and terrain rendering | next |
| 4. Unit/building/nation definitions (`.MD`, `.NDS`, `.LNK`, ...) | |
| 5. Deterministic simulation: movement, pathfinding, combat, economy | clock + RNG in place |
| 6. Mission/AI scripting API (210 functions, see `docs/script-api.md`) | PE reader in place |
| 7. UI, sound, music | |
| 8. Multiplayer (lockstep over WebSocket/WebRTC) | |

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
- [Mission/AI scripting API](docs/script-api.md) (generated)
