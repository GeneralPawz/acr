# ACR — American Conquest, Rebuilt

A from-scratch Rust reimplementation of the *American Conquest* (GSC Game World, 2002)
engine that runs on the original game data. Targets native desktop (Windows on ARM
included) and the browser (WebAssembly + WebGPU/WebGL2).

No original game files are part of this repository. Players supply their own copy.

## Status

| Milestone | State |
|---|---|
| 1. Archives + virtual file system (`ALL.GSC`, `*.GS1`, loose files) | done |
| 2. Palettes and `GPAK` sprite decoder, sprite viewer | next |
| 3. Map format (`.m3d`) and terrain rendering | |
| 4. Unit/building/nation definitions (`.MD`, `.NDS`, `.LNK`, ...) | |
| 5. Deterministic simulation: movement, pathfinding, combat, economy | |
| 6. Mission/AI scripting API (258 functions) | |
| 7. UI, sound, music | |
| 8. Multiplayer (lockstep over WebSocket/WebRTC) | |

## Architecture

```
crates/
  acr-formats   pure data readers, no platform deps (native + wasm32)
  acr-tool      `acr` CLI: list / cat / extract game files
planned:
  acr-sim       deterministic fixed-tick game simulation (no rendering, no I/O)
  acr-script    mission/AI API + x86 interpreter for the original DLLs
  acr-render    wgpu renderer (Vulkan/DX12/Metal native, WebGPU/WebGL2 in browser)
  acr-app       winit shell, input, audio; native binary and wasm entry point
```

Key design decisions:

- **Fixed simulation tick, decoupled from rendering.** The original ties game speed to
  frame rate, which is why it runs too fast on modern hardware. Here the simulation
  advances at a fixed rate; the renderer interpolates. Game speed is a tick-rate
  multiplier, never a side effect of CPU speed.
- **Deterministic simulation** (integer/fixed-point math, seeded RNG, ordered iteration)
  so replays and lockstep multiplayer work without DirectPlay.
- **Data readers operate on `Read + Seek`.** In the browser the player's files are copied
  into the Origin Private File System once; a worker reads them through synchronous
  access handles, which satisfies `Read + Seek` without loading 700 MB into memory.

### Dropped from the original

DirectPlay, Wintab (tablet support), the GameSpy/`iChat`/`DipServer` internet lobby,
`binkw32` video (to be converted or skipped), the external `vopl.exe` music process,
DirectDraw palette tricks and copy protection.

## Original data formats

### `GSCfmt` archive (`ALL.GSC`, `Patch01.GS1`, `Patch02.GS1`)

Little-endian. 14-byte header `"GSCfmt" | u16 0x0200 | u8 0xCD | u8 xor_key | u32 count`,
then `count` entries of 81 bytes:

| offset | size | field |
|---|---|---|
| 0 | 4 | checksum (not verified yet) |
| 4 | 64 | NUL-padded name, `\` separated |
| 68 | 4 | bitwise NOT of offset, relative to end of entry table |
| 72 | 4 | size |
| 76 | 4 | reserved, always 0 |
| 80 | 1 | 1 = XOR-encrypted with `xor_key` (0x78) |

Everything except `.GP` sprite packs is encrypted. Later archives and loose files
override earlier ones.

Contents of `ALL.GSC` (4659 entries): `GP` sprite packs (1665), `SMP`, `LNK`, `MD` unit
definitions (plain text), `TXT`, `BMP`, `DAT`, `GRD`, `LST`, `RLC`, `PAL`, `MSP`, `TERR`, `NDS`, ...

### Missions and AI

Each mission (`Missions/**/Mission.dll`) and each AI nation (`AI/*.dll`) is a native
32-bit x86 DLL. They import 258 functions from `dmcr.exe` by name (`CreateObject0`,
`SetResource`, `GetUnitsByNation`, `ShowVictory`, ...) plus a few `KERNEL32` calls.
Plan: implement that API in `acr-script` and execute the original DLL code in a small
embedded x86 interpreter, so all original missions run unchanged on ARM and in the
browser. Missions can later be ported to native Rust/scripts one by one.

## Usage

```sh
cargo run --release -p acr-tool -- list    "C:\path\to\American Conquest" .MD
cargo run --release -p acr-tool -- cat     "C:\path\to\American Conquest" ANMBIZ.MD
cargo run --release -p acr-tool -- extract "C:\path\to\American Conquest" extracted
```
