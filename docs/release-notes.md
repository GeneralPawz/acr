# ACR release notes

ACR runs *American Conquest* (GSC Game World, 2002; data of version 1.46) on a new engine
written in Rust, natively on Windows (x64 and ARM64) and in the browser. **The original game
files are not included**: you need your own installation.

## Download and start

| File | Contents |
|---|---|
| `AmericanConquest-windows-x64.zip` | Windows on Intel/AMD |
| `AmericanConquest-windows-arm64.zip` | Windows on ARM (e.g. Surface Pro) |
| `AmericanConquest-web.zip` | the browser build alone, for your own web server |

Unzip a Windows package into your American Conquest folder (or anywhere else) and
double-click:

- **`AmericanConquest.exe`** — the game. It finds the game data next to itself, in the
  folder named by `ACR_GAME_DIR`, in the folder it used last time, or asks once.
- **`AmericanConquest-Web.exe`** — the browser version on your own computer: it starts a
  local server, opens the browser and loads the game files from the folder it finds the
  same way (or lets you pick it in the page). Close its window to stop it.

Settings, savegames and campaign progress are stored in `%LOCALAPPDATA%\ACR\` and the game
folder's `Save\` and `Players\` folders (in the browser: the browser's local storage).

## What works

- Intro videos, the original menus (German), options, player profiles.
- Single missions, campaigns with the original mission scripts and progress (a victory
  opens the next missions), random maps generated exactly like the original.
- The original in-game interface: resource panel, unit and command icons, minimap, "M"
  drop-down, game menu, unit and game panels (U, I), message history, mission windows,
  objectives (F1), tribe diplomacy (F8).
- Simulation at the original speed (10 ticks per second, game speed option), deterministic:
  pathfinding, formations, combat with muskets, arrows and artillery, morale, economy
  (gathering, construction, production, upgrades, market, fields), fog of war.
- Computer players (the original AI DLLs) and DipServer, run in an x86 interpreter.
- Save and load; unit, ambient and interface sounds; music.
- Controls: the original ones, or an optional modern scheme (wheel zoom, drag panning,
  W/A/S/D) — see [controls.md](controls.md).

## Not yet

- Multiplayer and public hosting of the browser version (milestone *Later – Multiplayer*).
- Some script functions and simulation details are simplified or missing; see the open
  issues of [milestone M1](https://github.com/GeneralPawz/acr/milestone/1) and the
  deviations in `docs/simulation.md` and `docs/ui.md`.
- The original game's savegames and replays cannot be loaded.
