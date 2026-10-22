# Script runtime: running the original mission/AI DLLs

The 73 mission and AI scripts (`Missions/**`, `History_battl/**`, `UserMissions/**`,
`AI/*.dll`) are 32-bit x86 DLLs built with MSVC around 2002. `acr-script` runs their
original machine code in an interpreter written in safe Rust, so they work on any host
(Windows on ARM, Linux, the browser) and deterministically. Imported functions trap into
Rust.

```sh
acr script-check <game-dir>                    # decode + initialise all 73 DLLs
acr script-run   <game-dir> <dll> [--ticks N]  # run one DLL, log its game API calls
ACR_GAME_DIR=<game-dir> cargo test -p acr-script --test original_dlls
```

## Components (`crates/acr-script`)

| module | role |
|---|---|
| `pe` | PE32 reader: sections, imports, exports, base relocations |
| `memory` | 4 GiB guest address space in 64 KiB pages, allocated on demand; unmapped access is a `Fault` |
| `x86::decode` | instruction decoder (own implementation, no dependency) |
| `x86` | integer execution, eager and exact flags |
| `x86::fpu` | x87 on `f64` registers, `libm` for transcendental functions |
| `loader` | maps a DLL at its image base, applies relocations, points every IAT slot at a thunk |
| `runtime` | `Machine` (CPU + memory + process per DLL), interpreter loop, import dispatch, `Host` trait |
| `kernel32` | shims for all 83 KERNEL32 functions the DLLs import, plus `GetTickCount` (deterministic, for DipServer) |
| `seh` | Win32 structured exception handling (`RaiseException`, `RtlUnwind`) for C++ `throw`/`catch` |
| `heap` | deterministic first-fit allocator behind `HeapAlloc` and `_ExMalloc` |
| `dmcr` | `RecordingHost`: stub game API that logs calls; argument counts of all API functions |
| `analysis` | static recursive disassembly to measure decoder coverage |

## Memory layout (one `Machine` per DLL)

| range | contents |
|---|---|
| `0x0000_0000..0x0001_0000` | unmapped (null pointer accesses fault) |
| `0x0001_0000` | thread information block (`fs:`; `fs:[0]` is the SEH chain), process strings |
| `0x0020_0000..0x0030_0000` | stack, 1 MiB |
| `0x0100_0000..0x1000_0000` | heap (`HeapAlloc`, the game's `_ExMalloc`) |
| `0x1000_0000` | the DLL; all 73 use this image base, so no relocation is needed in practice |
| `0x2000_0000..0x7000_0000` | `VirtualAlloc` regions |
| `0x7FF0_0000..0x7FF1_0000` | import thunks (4 bytes each), return sentinels for host calls and SEH |

Thunks contain no code. When `eip` enters the thunk range the loop looks up the import
bound to that slot, reads the arguments from the stack, calls the Rust function, puts the
result in `eax` and returns to the caller. `Machine::call` pushes a sentinel return address
and runs until the guest returns to it; the stack pointer and SEH chain are restored after
every call, also after errors, so a failed tick does not poison the next one.

Decoded instructions are cached per address of the image (code is never modified).

## Calling conventions found

Determined from the stack cleanup after every import call site in all DLLs (debug builds
of the AI clean up after each call, which makes the counts unambiguous there):

* **KERNEL32**: `stdcall`, as on Windows; the shim table records the argument count and the
  dispatcher pops it.
* **`dmcr.exe` and `DipServer.dll`**: `cdecl` throughout. Every call site is followed by
  `add esp, 4n` (or a merged cleanup in optimised mission code); the C++ exports carry the
  `YA` (`__cdecl`) code in their mangled names (`?_ExMalloc@@YAPAXH@Z`). The host only
  returns `eax`. `dmcr::ARG_COUNTS` lists the inferred argument count of 203 game functions
  (for the other 7 imports no direct `call [iat]` site was found; 20 entries are marked
  ambiguous, several of them variadic like `ShowPageParam`).
* **DLL exports**: missions export `OnInit()` and `ProcessScenary()`; AI nations export
  `InitAI()`, `ProcessAI()`, `GetDataForSave`, `SetSaveData`, `FreeSaveData`,
  `FreeAICityData`. `CMS_Start.dll` additionally exports helper functions (`RunTimerEx`,
  `ShowPageEx`, ...). `OnInit`/`ProcessScenary`/`InitAI`/`ProcessAI` take no arguments.
  Return values are often in `al` only (`bool`).
* The entry point is the static CRT's `_DllMainCRTStartup(hinst, reason, reserved)`.

## Interpreter coverage

`acr script-check` disassembles everything reachable from the entry point and exports,
following direct branches and calls, switch jump tables, relocated function pointers in data
(vtables, CRT initialiser tables) and relocated code addresses in immediates.

* **1,097,086 reachable instructions in 73 DLLs, 103 distinct mnemonics, 0 unsupported.**
* The decoder was cross-checked against `iced-x86` on all reachable instructions
  (length, mnemonic and branch target identical for 1,096,174 of 1,096,174 in iced's own
  traversal); this check lives outside the repository to avoid the dependency.
* Integer semantics were checked against the real CPU: 49,645 randomised and edge-case
  vectors over 126 instruction forms (ALU ops in 8/16/32 bits, inc/dec/neg/not,
  mul/imul/div/idiv, all shifts and rotates by `cl`, shld/shrd, bt*, bsf/bsr, xadd,
  cmpxchg, setcc, cmovcc, sahf/lahf, ...) executed natively and compared on result registers
  and every architecturally defined flag: 0 mismatches. Undefined flags keep their previous
  value.
* x87: the DLLs use `fld/fst(p)/fild/fist(p)` in all formats including 80-bit, the full
  arithmetic group, `fcom(p)` + `fnstsw ax` + `sahf`/`test ah`, `fldcw` (MSVC's `_ftol`
  switches to truncation), `fsin/fcos/fsqrt/fpatan/fprem1`. All x87 instructions MSVC emits
  are implemented except BCD and environment save/restore (`fnsave`, `fnstenv`), which are
  unreachable.
* Not implemented (none reachable): segment register loads, far calls, I/O, BCD
  (`daa`, `aaa`, ...), `bound`, `int n`, SSE/MMX, `cpuid`, `rdtsc`.

## Results on the original data

| stage | DLLs ok |
|---|---:|
| decode all reachable code | 73 / 73 |
| `DllMain(DLL_PROCESS_ATTACH)` (CRT start-up, static constructors) | 73 / 73 |
| `OnInit` / `InitAI` | 73 / 73 |
| first `ProcessScenary` / `ProcessAI` with a stub API returning 0 | 72 / 73 |

The exception is `AI/Siu.dll`: it dereferences the pointer returned by `GetOBJ`, which is 0
in the stub. CRT start-up costs about 10–40k instructions for missions and 390k for the
debug-built AI DLLs.

`UserMissions/CMS_Start.dll` reads `UserMissions\start.dat` in a static constructor, which
does not exist in the installation; the `throw` is caught by the DLL's own `catch`, which
reports it through the game API: `MissErrorMessage("Mission script error", "Cannot open
start.dat file.")`. This exercises the whole C++ exception path (`_CxxThrowException` →
`RaiseException` → `__CxxFrameHandler` → `RtlUnwind` → catch funclet → continuation).

## What a mission looks like from the API side

Logged with `acr script-run` (stub returns 0 everywhere):

1. **`DllMain`**: the CRT start-up, then C++ static constructors that already call the API:
   `RegisterUnitType(&desc, "EngKri(EN)")`, `RegisterUpgrade(&desc, "UsaM18(US)ATTACK")`,
   `RegisterDynGroup(&group)`, `RegisterVar(&var, size)`, `RegisterFormation`,
   `RegisterUnits(&group, "GOff1")`, `RegisterZone`, `ChangeFriends`, `SetPlayerName`,
   `DisableUpgrade`, `EnableUnit`. The first argument is a pointer into the DLL's data: the
   engine is told where the script keeps its state (groups, variables, unit type handles)
   so it can fill and save it.
2. **`OnInit()`**: called by the DLL's own `DllMain` (the engine never calls it), so the
   second registration sequence `acr script-run` shows is an artefact of the tool calling it
   again.
3. **`ProcessScenary()`** every 8th game tick: trigger and timer polling (`Trigg(n)`,
   `TimerDoneFirst(n)`, `RunTimer`), unit queries (`GetTotalAmount0/1`, `GetUnitsAmount0`,
   `GetInsideAmount`), selection-based commands (`SelectUnitsType`, `SaveSelectedUnits`,
   `ClearSelection`, `SelSendTo`, `SelDie`), spawning (`CreateObject0`) and victory checks
   (`ShowVictory`, `LooseGame`). Small missions need ~30 instructions per tick when nothing
   happens, the 7 Years' War missions up to 28k instructions and 750 API calls.

AI nations: `DllMain` registers 22 unit types and 92 upgrades; `InitAI` asks
`GetAINation()`; every `ProcessAI` walks all objects with `GetOBJ(i)` (512 calls) and reads
**fields of the returned engine object directly** (e.g. `[obj+0x9C]`), then manages groups
(`RegisterDynGroupEx`, `CleanGroup`), economy (`TryUpgrade`, `SGP_TakeResources`) and
memory through `_ExMalloc`/`_ExFree`.

## Game API on the simulation (`crates/acr-mission`)

`acr-mission` implements the 210 `dmcr.exe` functions and the 6 `DipServer.dll` functions the
DLLs import on `acr_sim::World` and drives the scripts like the original. It is a separate
crate so that `acr-script` stays a generic Win32/x86 sandbox and `acr-sim` stays free of
scripting; only this crate knows the engine's script-facing structures.

```sh
acr mission-run <game-dir> <mission-id|map|dll> [--ticks N] [--trace] [--ai] [--dip] [--dip-click TICK]  # headless run, event log
acr mission-api                                                               # coverage table below
ACR_GAME_DIR=<game-dir> cargo test -p acr-mission --test missions
```

| module | role |
|---|---|
| `api` | the function table (`ApiFn`: name, original address, `Status`, handler) and the `Host` that dispatches calls; one module per area: `core` (triggers, timers, heap), `game` (registration, pages, questions, victory, players, RNG, fog, camera), `groups` (selection, groups, orders, `SGP_*` processes), `units` (counting, `CreateObject0`, unit info), `economy` (resources, production, upgrades, market), `tactics` (top zones, danger maps, safe ways, attacks), `aiobj` (`GetOBJ`, `OBJ_*`) |
| `state` | `ScriptState`: the engine side of `SCENINF` (triggers, timers, groups, zone groups, light spots, victory flags) plus the per-nation selections and alliance masks |
| `guest` | `OneObject` mirrors in guest memory for the AI DLLs |
| `vm` | one loaded DLL (`Machine`, per-tick export, failure state) |
| `world_ops` | the few direct world changes scripts make (kill, erase, change owner, create) |
| `texts`, `events` | mission text files (`#PAGE0 picture` + body) and what the UI is told |
| `lib` | `Scripts`: loading, the tick cadence, questions, AI scheduling |

The semantics come from the original functions (disassembly of `dmcr.exe` 1.46; the Cossacks
source `ActiveScenary.cpp` served as a map, the binary decides). Every handler names the
address it reproduces.

### How the engine runs scripts

* **Loading** (`ScenaryInterface::Load`, 0x55afdd): `LoadLibrary` runs `DllMain`, and the
  DLLs call their own `OnInit` from `DllMain`; the engine only resolves `ProcessScenary`. The
  text file named in `Missions.txt` (5th line of a record) is split into `#ID header` entries
  whose body runs to the next `#`.
* **Cadence**: the simulation step 0x4a61e7 runs `HandleMission` (0x55a1d2) only when
  `tmtmt & 7 == 7`: `ProcessDipRelations` (DipServer), `ProcessScenary`, the central-text and
  timer countdown (`8 << SpeedSh` = 16 per call, i.e. `GetGlobalTime` units, +2 per tick), then
  the head process of every group (`SGP_*`). The other phases run the AI city of nation
  `tmtmt & 7` (nation 7 never gets an AI). `Scripts::tick` runs before `World::step` with
  `tmtmt = tick + 1`.
* **Handles** (`GAMEOBJ`, 8 bytes in DLL memory): `u16 Index, u16 Serial, u32 Type` with the
  magics `'UNIT'` (groups), `'ZONE'` (zone groups), `'UTYP'`, `'UPGR'`, `'FORM'`, and point zones
  `0x40000000 | R` with x/y in Index/Serial (pixels). Wrong handles go to `IntErr`, which in
  the original only shows a box when `ERR.LOG` exists, and increment `SCENINF.NErrors`, which
  has no effect after loading; ACR logs them in `ScriptState::error_log`.
* **Triggers** are stored inverted, so all 256 read 0xFF at the start; scripts rely on it
  (`if (Trigg(99)) { SetTrigg(99, 0); ... }`).
* **Units** are identified by index and serial. ACR's entity slots are never reused, so the
  serial is derived from the index and only bit 0 changes (on capture, as in the original).
* **Random numbers**: `RandNew`/`GetRND` draw from the simulation's synchronised table
  (`World::random`), as in the original; the call order matters for determinism.
* **Orders** (`SelSendTo`, `OBJ_SendTo`, attacks) become `Command`s for the current tick, so
  the tick's command stream holds the players' and the scripts' orders. Object creation,
  death and owner changes change the world at once because scripts query the result in the
  same call.
* **Pages and questions**: `ShowPage`/`AskQuestion` are modal dialogs inside the script call
  in the original, so the game stands still. ACR reports a `MissionEvent::Page` (the UI pauses
  while it shows it); a question rolls the script call back (machine, script state and world
  are restored from a snapshot taken for missions that import `Ask*`) and reruns it with the
  answer once `Scripts::answer` is called. `ShowVictory`/`LooseGame` stop the game
  (`Scripts::ended`); the original shows WINNER/LOOSER for 7 s and unlocks the next campaign
  missions after a victory.
* **AI DLLs**: the DLL of a computer nation is `GetTextByID("<NATIONID>.0")` from
  `TEXT\AI.TXT` (`SPAIN.0 AI\Spain.dll`), the nation found through the unit id suffix of its
  first peasant in `NATIONS.LST`; missions start AIs with `StartAI`. A DLL is loaded once and
  shared by the nations that use it (`LoadLibrary` semantics). `InitAI` runs at load; the AI
  of nation k runs on ticks with `tmtmt % 8 == k` once the nation owned a peasant
  (`CenterFound`).
* **Engine memory in the DLLs**: the AI DLLs read 20 `OneObject` fields directly (no pointer
  chains): `GetOBJ(i)` returns a 0x100-byte mirror at `0x7000_0000 + 0x100·i`, refreshed when
  handed out and before every `ProcessAI`, and written fields (`Life` for the AI's "immortal
  group" trick, the stance, the carried load, `UnlimitedMotion`) are copied back afterwards.
  Field table in `guest.rs`. Pointers into engine data that DLLs read (`GetListOfNearZones`,
  `GetInsideList`, `GetUnitExCaps`' unit id, `GetTextByID`, `GetGroupName`) point to copies in
  the guest heap of the calling DLL.
* **AI city** (`aicity.rs`, `City` 0x92df18): the DLL only *proposes* projects. Before every
  `ProcessAI` the city lists what it can start now (`EnumProp`, Megapolis.cpp): every type an
  idle finished building (or any peasant) of the nation can produce according to the
  nations' `[FIXED_PRODUCE]` and can afford, with its price in percent of the stock
  (`GetMonsterCostPercent`). `TryUnit(type, max, costPercent, probability)` accepts one when
  the census counts fewer than `max` (`City+0xeb73`), `rando() < PERCONV[probability]` and
  `costPercent >= percent`; with game option 2 at 1 it refuses artillery and towers.
  Upgrade handles, `UpgIsDone`, `InitialUpgrade` and the enable functions work on the tech
  tree's per-player states; the scripts' manual enable/disable flags are reapplied after
  every census.
  After the call the city carries the accepted projects out (0x432871): units are queued in
  their producer (paying the price; with a `UnitsCollector` the unit joins it when born),
  buildings go to acr-sim's `aiplace` (see "AI building placement" in `simulation.md`). One
  building project is pending at a time (`TryUnit` refuses buildings meanwhile). New
  buildings join the nation's `BuildingsCollector` and the `EnemyBuildingsCollector` of
  every non-allied nation (0x588569); with a buildings collector the DLL sends the builders
  (`SGP_RepairBuilding`), without one the engine sends the nearest peasants. Build lists and availability come from acr-sim's tech tree.
  Upgrades the idle buildings offer are proposed too (`TryUpgrade`) and researched with
  acr-sim's `PerformUpgrade`; `InitialUpgrade` applies the effect at once.
* **AI landing**: `AssignDesantStartGroup` / `AssignDesantDestGroup` name two of the DLL's
  groups and `SetMaxInTransport` the load (`City+0xf4e`, `+0xf50`, `+0x189`); the engine
  side (a diversion brigade per new AI transport, `acr_sim::desant`, see `simulation.md`)
  takes the start group's units aboard and puts them into the destination group ashore.
  The host hands the start group to the simulation before each step and applies the units
  taken and landed before the next scripts run (`ScriptState::sync_desant`). The stock
  DLLs only go to sea (ports, ferries, the start group) with land type 2 or 4
  (`GetLandType`, `City+0x184`), which only `StartAI` and the playback of recorded games
  set; a random map started from the menu keeps 0. `acr mission-run --land N` sets it for
  the computer nations to exercise this.
* **Hidden units**: units inside buildings have `Sdoxlo` = 1 and `Hidden` set in the
  original, so functions that test `Sdoxlo == 0` skip them and others accept them
  ("alive or hidden"); ACR follows each function's test (`Api::member` vs `member_h`).
* **Danger maps, top lists, armies** (`api/tactics.rs`, `api/dip.rs`): the seven
  `CreateDangerMap*` variants share one loop over the objects (their area by `GetTopZone`,
  the caller's `weight[CATEGORY]`, a per-variant term for enemies, garrisons, mines,
  resource bases, nation-7 buildings and peasants), the in-place spreading over the area
  links and the clamp to [−6, 30000] (not `ForDef`); `CreatePeasantsTopList` and the storm
  top lists keep one object per area. `EnumEnemyArmies` copies the enemy armies the
  simulation clusters every 256 ticks (`acr_sim::armies`, see
  [simulation.md](simulation.md)); `SetDangerMap`'s engine map is only read by a debug
  overlay.
* **DipServer.dll** (diplomacy with neutral tribes, `$PSB` map groups of random maps; from the
  Cossacks II source tree) runs in its own machine like the scripts: `DllMain` and
  `InitDipForThisMap()` at game start, `ProcessDipRelations()` before every `ProcessScenary`,
  `PerformDipCommand` for the commands it sent with `SendDipCommand` (net command 0x5d) at the
  next script call. The AI DLLs' calls into it (`AddStorm`, `AddTomahawks`, `AddFirers`,
  `AddPsKillers`, `AddCannon`, `SetAIForDiplo`) are forwarded into its machine with the
  `GAMEOBJ` copied over; it then drives those squads itself. It imports 48 more `dmcr.exe`
  functions (named map groups, file access to `Dip\*`, danger maps for its squads, art
  attacks, `ChangeUnitColor`, ...), implemented in `api/dip.rs`. Its dialogs are CML pages
  that the game shows in the diplomacy panel (`ActivateDipDialog` →
  `MissionEvent::Diplomacy`); `acr_mission::diplomacy` calls `CheckDipBuilding` for a click
  on a building and passes the panel's link requests to `DS_SendRequest` /
  `DS_GetRequestResult` (see [ui.md](ui.md#diplomacy-panel)). `mission-run --dip-click TICK`
  exercises the dialogs headless.

### What real missions do (3000 ticks headless, no player input)

The 51 missions of `Missions.txt` whose maps ship load and run without script errors
(Single9–11 reference maps that are not in the installation). Examples:

* `#columb1@`: shows `#PAGE0` ("AYE KOLUMBUS! ...") at tick 39, switches four objective
  sections, lands 40 natives with `CreateObject0` and waits for the player to build 35
  peasants.
* `#columb4@`, `#Tekumse_I2@`, `#Tekumse_I4@`: count down with a hint every script call
  ("Verbleibende Zeit 12:07").

Random maps with computer players (`acr mission-run <game> "RN1 3281 123132 00101111
5002012.m3d" --ai --dip --ticks 6000`): four `AI\Spain.dll` nations build 13–17 buildings in
the first 1000 ticks and grow from 130 to about 300 units by tick 6000, while DipServer runs
the tribe settlements.

Gaps that follow from the simulation (no upgrades, fog, morale, transports, burning
buildings yet): the attack helpers order `Command::Attack` on the nearest
enemy instead of the original's target rules, and the functions listed as stubs return
neutral values. They are marked `TODO(acr-sim)` in the code and become real as the
simulation grows; the table below is generated from the code.

## KERNEL32 emulation

The process is a single-threaded Windows XP (`GetVersion` = 5.1.2600) with code page 1252,
an empty environment, command line `"C:\American Conquest\dmcr.exe"`, a fixed clock
(1 January 2002), no files and no other DLLs (`LoadLibraryA` fails, which the CRT handles).
Critical sections are no-ops, TLS is a host-side table, standard output and
`OutputDebugStringA` are collected in `Process::output`. Every run is reproducible.

## Open problems

* **Game API**: see the coverage table; the partial and stubbed functions wait for
  simulation features (combat, economy, production, garrisons, fog, upgrades).
* **DipServer dialogs**: `CheckDipBuilding` and the page requests run outside the
  simulation tick on the local machine, like in the original (only `SendDipCommand` is a
  network command 0x5d). ACR keeps the `SendDipCommand` data in the script state until
  the next `ProcessDipRelations`; it is not yet a `Command` of the tick stream, so replays and
  multiplayer do not carry the player's diplomacy choices.
* **Files**: `CreateFileA` always fails. If a DLL needs data files (`CMS_Start` wants
  `start.dat`), add a read-only file provider to `Process`.
* **Saving**: `Machine` is `Clone`, which is enough for snapshots; a compact
  serialisation (guest pages + CPU + heap) is still to do. Script state saved by the
  original game goes through `GetDataForSave`/`SetSaveData` and the registered variables.
* **Hardware exceptions** (access violations, divide errors) stop the interpreter instead
  of being dispatched to SEH `__except` blocks; none of the DLLs relies on that so far.
* **Performance**: not profiled yet; a debug build runs `script-check` (static analysis
  plus about 6 M executed instructions) in 7.5 s. Flags are computed eagerly; lazy flags and block
  caching are possible if ticks get expensive.

## API coverage

Generated by `acr mission-api`: 264 functions: 253 done, 10 partial, 1 stubbed

| function | original | status | notes |
|---|---|---|---|
| `?ActivateDipDialog@@YAXPAD@Z` | 0x5c3980 | done |  |
| `?AddCannon@@YAXPAUGAMEOBJ@@E@Z` | DipServer | done |  |
| `?AddFirers@@YAXPAUGAMEOBJ@@E@Z` | DipServer | done |  |
| `?AddPsKillers@@YAXPAUGAMEOBJ@@E_N@Z` | DipServer | done |  |
| `?AddStorm@@YAXPAUGAMEOBJ@@EH@Z` | DipServer | done |  |
| `?AddTomahawks@@YAXPAUGAMEOBJ@@EGHH@Z` | DipServer | done |  |
| `?AssignHint1@@YAXPADH@Z` | 0x4942db | done |  |
| `?AssignHint@@YAXPADH@Z` | 0x494283 | done |  |
| `?Gclose@@YAXPAVGFILE@@@Z` | 0x40501a | done |  |
| `?GetTextByID@@YAPADPAD@Z` | 0x60a020 | done |  |
| `?Ggetch@@YAHPAVGFILE@@@Z` | 0x404fde | done |  |
| `?Gopen@@YAPAVGFILE@@PAD0@Z` | 0x404f14 | done |  |
| `?RBlockRead@@YAKPAX0K@Z` | 0x40247f | done |  |
| `?RBlockWrite@@YAKPAX0K@Z` | 0x4024b0 | partial | written files live in memory |
| `?RClose@@YAXPAX@Z` | 0x40250c | done |  |
| `?RFileSize@@YAKPAX@Z` | 0x40242d | done |  |
| `?RReset@@YAPAXPBD@Z` | 0x402338 | done |  |
| `?RRewrite@@YAPAXPBD@Z` | 0x402401 | partial | written files live in memory |
| `?SendDipCommand@@YAXPADH@Z` | 0x5c3b00 | done |  |
| `?SetAIForDiplo@@YA_NEHH@Z` | DipServer | done |  |
| `?_ExFree@@YAXPAX@Z` | 0x46b11e | done |  |
| `?_ExMalloc@@YAPAXH@Z` | 0x46b1c7 | done |  |
| `?_ExRealloc@@YAPAXPAXH@Z` | 0x46b220 | done |  |
| `AI_Torg` | 0x569083 | done |  |
| `AddOneUnitToGroup` | 0x5670a8 | done |  |
| `AddResource` | 0x562dea | done |  |
| `AddUnitToSel` | 0x56c8c7 | done |  |
| `AddUnitsInZoneToGroup` | 0x56c914 | partial | USAGE filter not modelled (any type) |
| `AllowAttack` | 0x562234 | done |  |
| `ArtAttackGroud` | 0x5709b5 | done |  |
| `ArtAttackObject` | 0x570b32 | done |  |
| `AskMultilineQuestion` | 0x5662de | done |  |
| `AskQuestion` | 0x562765 | done |  |
| `AskQuestionParam` | 0x562868 | done |  |
| `AssignDesantDestGroup` | 0x570224 | done |  |
| `AssignDesantStartGroup` | 0x5701ea | done |  |
| `Attack3` | 0x57ea15 | done |  |
| `Attack4` | 0x580042 | done |  |
| `Attack6` | 0x580b71 | done |  |
| `AttackBuildingsInZone` | 0x5617df | done |  |
| `AttackByTomahawks` | 0x574667 | done |  |
| `AttackEnemyInZone` | 0x5642b7 | done |  |
| `AttackEnemyInZone2` | 0x57dfb9 | done |  |
| `AttackEnemyPeasantsInZone` | 0x564d83 | partial | approximation on acr-sim (see the function's comment) |
| `AttackGrpByGrp` | 0x57385f | done |  |
| `CancelAllPurposes` | 0x578bc5 | done |  |
| `ChangeFriends` | 0x561df9 | done |  |
| `ChangeUnitColor` | 0x56c778 | done |  |
| `CheckBuildingsComplete` | 0x566a30 | done |  |
| `CheckDef` | 0x573c0d | done |  |
| `CheckIfBuildingIsFired` | 0x57c39e | done |  |
| `CheckIfNotBusy` | 0x56f78f | partial | approximation on acr-sim (see the function's comment) |
| `CheckLandLocking` | 0x56f65a | done |  |
| `CheckLeaveAbility` | 0x560b17 | done |  |
| `CheckProduction` | 0x5672cc | done |  |
| `CleanGroup` | 0x56cf09 | done |  |
| `ClearLightSpot` | 0x564121 | done |  |
| `ClearSelection` | 0x55f51f | done |  |
| `CollectUnits` | 0x56d814 | done |  |
| `CopyUnits` | 0x56ea22 | done |  |
| `CreateDangerMap` | 0x57c3ea | done |  |
| `CreateDangerMapForDef` | 0x57c7c0 | done |  |
| `CreateDangerMapForFire` | 0x57d6c4 | done |  |
| `CreateDangerMapForStorm` | 0x57cd88 | done |  |
| `CreateDangerMapForStormHard` | 0x57d19e | done |  |
| `CreateDangerMapForTom` | 0x57c9ee | done |  |
| `CreateDangerMapWithoutPeasants` | 0x57daac | done |  |
| `CreateFarmList` | 0x57c20d | done |  |
| `CreateFields` | 0x577ff4 | done |  |
| `CreateFormation` | 0x56fbf8 | done |  |
| `CreateFriendBuildingsTopList` | 0x57b859 | done |  |
| `CreateMeatList` | 0x57bff5 | done |  |
| `CreateNonFiredEnemyBuildingsTopList` | 0x57b2fd | done |  |
| `CreateObject0` | 0x55eded | done |  |
| `CreatePeasantsTopList` | 0x57bd81 | done |  |
| `CreateTopListEnArmy` | 0x57bad9 | done |  |
| `CreateTopListForStorm` | 0x57b459 | done |  |
| `CreateTopListForStormHard` | 0x57b5e9 | done |  |
| `DeleteHiddenUnits` | 0x56d8b9 | done |  |
| `DeleteUnitFromGroup` | 0x56ce96 | done |  |
| `DetectPanic` | 0x56d9b7 | done |  |
| `DieUnit` | 0x5701ae | done |  |
| `DisableMission` | 0x5622fb | done |  |
| `DisableUpgrade` | 0x55c335 | done |  |
| `EnableMission` | 0x562318 | done |  |
| `EnableUnit` | 0x55c5bb | done |  |
| `EnumEnemyArmies` | 0x578245 | done |  |
| `EraseGroup` | 0x56d7e1 | done |  |
| `FilterUnitsByCategory` | 0x56df95 | done |  |
| `FindNextZoneOnTheSafeWay` | 0x579414 | done |  |
| `FindNextZoneOnTheSafeWayToLimitedObject` | 0x57ab1a | stub | 0xFFFF: no way found |
| `FindNextZoneOnTheSafeWayToObject` | 0x57a33c | done |  |
| `FreeTimer` | 0x563739 | done |  |
| `GetAINation` | 0x56827b | done |  |
| `GetAlliesForce` | 0x571dc4 | partial | approximation on acr-sim (see the function's comment) |
| `GetAmountOfGroups` | 0x56bdda | done |  |
| `GetAmountOfWarriors` | 0x55eaba | done |  |
| `GetBuildingEntryPoint` | 0x56ea8c | done |  |
| `GetBuildingOposit` | 0x57b9ae | done |  |
| `GetCurrentPurpose` | 0x578b24 | done |  |
| `GetCurrentUnits` | 0x578173 | done |  |
| `GetDiff` | 0x56957d | done |  |
| `GetDifficulty` | 0x569555 | done |  |
| `GetEnemy3` | 0x5818a5 | done |  |
| `GetEnemyForce` | 0x571aa8 | done |  |
| `GetExtraction` | 0x5699eb | done |  |
| `GetFogSpotVisibility` | 0x56e6a4 | done |  |
| `GetGlobalTime` | 0x5637e7 | done |  |
| `GetGroupCharge` | 0x574575 | done |  |
| `GetGroupForce` | 0x56f574 | done |  |
| `GetGroupName` | 0x56be12 | done |  |
| `GetGrpCenter` | 0x5789cb | done |  |
| `GetInsideAmount` | 0x57819e | done |  |
| `GetInsideList` | 0x56c713 | done |  |
| `GetLandType` | 0x5695ae | done |  |
| `GetLastFullWay` | 0x57c3d7 | partial | the path of the last safe-way search is not kept: 0 |
| `GetListOfNearZones` | 0x579281 | done |  |
| `GetMaxAttRadius` | 0x56f41e | done |  |
| `GetMaxDamage` | 0x575250 | done |  |
| `GetMaxInsideAmount` | 0x5781e5 | done |  |
| `GetMaxUnits` | 0x578148 | done |  |
| `GetMinDamage` | 0x57521b | done |  |
| `GetMonUsage` | 0x5751e5 | done |  |
| `GetMoney` | 0x568f03 | done |  |
| `GetMoneyString` | 0x56f6fe | done |  |
| `GetMyNation` | 0x5658b6 | done |  |
| `GetNInside` | 0x5604e2 | done |  |
| `GetNUnits` | 0x5679d8 | done |  |
| `GetNZones` | 0x579399 | done |  |
| `GetNatNMASK` | 0x56f689 | done |  |
| `GetNextZone` | 0x5792c5 | done |  |
| `GetOBJ` | 0x56f07f | done |  |
| `GetOrderType` | 0x56f3f5 | done |  |
| `GetPeaceTimeLeft` | 0x569882 | done |  |
| `GetRND` | 0x570700 | done |  |
| `GetReadyUnits` | 0x5690f0 | done |  |
| `GetResource` | 0x562d7a | done |  |
| `GetSearchRadius` | 0x56f46b | done |  |
| `GetTCos` | 0x56f6e9 | done |  |
| `GetTSin` | 0x56f6d4 | done |  |
| `GetTargetObject` | 0x56f4b7 | done |  |
| `GetTopDist` | 0x579370 | done |  |
| `GetTopDst` | 0x5658c7 | done |  |
| `GetTopZRealCoor` | 0x581847 | done |  |
| `GetTopZone` | 0x5791cb | done |  |
| `GetTopZoneCoor` | 0x579229 | done |  |
| `GetTorgResult` | 0x5690c0 | done |  |
| `GetTotalAmount0` | 0x55e6c6 | done |  |
| `GetTotalAmount1` | 0x55e7f2 | done |  |
| `GetUnitCost` | 0x562f80 | done |  |
| `GetUnitExCaps` | 0x56be77 | done |  |
| `GetUnitGlobalInfo` | 0x567cb9 | done |  |
| `GetUnitInfo` | 0x567a0e | done |  |
| `GetUnitInfo2` | 0x578f2c | done |  |
| `GetUnitNMASK` | 0x56bc78 | done |  |
| `GetUnitStorm` | 0x575285 | done |  |
| `GetUnits` | 0x568f4d | done |  |
| `GetUnitsAmount0` | 0x55c70a | done |  |
| `GetUnitsAmount1` | 0x55dd05 | done |  |
| `GetUnitsAmount2` | 0x55e05c | done |  |
| `GetUnitsByNation` | 0x566ca6 | done |  |
| `GetUnitsTypeInGroup` | 0x570862 | done |  |
| `GetUpgradeCost` | 0x56304a | done |  |
| `GetZoneCoor` | 0x56c266 | done |  |
| `GetZoneR` | 0x56c319 | done |  |
| `GetZonesDist` | 0x57931b | done |  |
| `GrpAttackObject` | 0x570d0c | done |  |
| `InitialUpgrade` | 0x55c02f | done |  |
| `InitialUpgradeNew` | 0x55c202 | done |  |
| `InsertUnitToGroup` | 0x56bf98 | done |  |
| `InviteUnitIntoBuilding` | 0x56c638 | done |  |
| `IsResourceEnabled` | 0x577225 | done |  |
| `LeaveBuilding` | 0x56c6ca | done |  |
| `LooseGame` | 0x562456 | done |  |
| `MakeEnemyAgrTopGrp` | 0x573730 | done |  |
| `MakeOneShotToBuild` | 0x573da9 | done |  |
| `MissErrorMessage` | 0x56bc22 | done |  |
| `NationIsErased` | 0x5695ce | done |  |
| `OBJ_AttackObj` | 0x56f13f | done |  |
| `OBJ_AttackPoint` | 0x56f3a2 | done |  |
| `OBJ_LeaveMine` | 0x56f1d6 | done |  |
| `OBJ_SendTo` | 0x56f0a1 | done |  |
| `OBJ_SmartSendTo` | 0x56f0ef | partial | a single-unit Command::Move (offsets added) |
| `Patrol` | 0x561cfb | done |  |
| `ProduceUnit` | 0x566dd2 | done |  |
| `ProduceUnitFast` | 0x566f36 | done |  |
| `PushAllUnitsAway` | 0x56083e | done |  |
| `RandNew` | 0x584891 | done |  |
| `RegisterDynGroup` | 0x56780c | done |  |
| `RegisterDynGroupEx` | 0x5678e9 | done |  |
| `RegisterFormation` | 0x55c67c | done |  |
| `RegisterUnitType` | 0x55be77 | done |  |
| `RegisterUnits` | 0x55b73d | done |  |
| `RegisterUpgrade` | 0x55bf0c | done |  |
| `RegisterVar` | 0x55bac8 | partial | recorded for savegames (#78); the engine never touches the variable otherwise |
| `RegisterVisibleZone` | 0x55bce5 | done |  |
| `RegisterZone` | 0x55bb53 | done |  |
| `RemoveAttackingUnits` | 0x570728 | done |  |
| `RemoveGroup` | 0x568031 | done |  |
| `RemoveNUnits` | 0x56d757 | done |  |
| `RemoveNearestNotBusyUnit` | 0x56e265 | done |  |
| `RemoveNearestUnit` | 0x56e48c | done |  |
| `RemovePanicUnits` | 0x56da24 | done |  |
| `RemoveUnitFromGroup` | 0x56c17e | done |  |
| `RemoveUpperGroundUnits` | 0x56eeef | done |  |
| `RunTimer` | 0x563598 | done |  |
| `SGP_AttackByArchers` | 0x5790f3 | done |  |
| `SGP_ComeIntoBuilding` | 0x577c2a | done |  |
| `SGP_MakeOneStepTo` | 0x578a6f | done |  |
| `SGP_MoveToPoint` | 0x578a14 | done |  |
| `SGP_RepairBuilding` | 0x577b9c | done |  |
| `SGP_TakeResources` | 0x577be1 | done |  |
| `SaveSelectedUnits` | 0x5671ad | done |  |
| `SearchCapturedTopToDeffend` | 0x5734df | done |  |
| `SelAttackGroup` | 0x560258 | done |  |
| `SelCenter` | 0x563981 | done |  |
| `SelChangeNation` | 0x561eaf | done |  |
| `SelDie` | 0x5601c0 | done |  |
| `SelErase` | 0x56021c | done |  |
| `SelSendAndKill` | 0x561be4 | done |  |
| `SelSendTo` | 0x561ad6 | done |  |
| `SelectTypeOfUnitsInZone` | 0x55d669 | done |  |
| `SelectUnits` | 0x55f865 | done |  |
| `SelectUnitsInZone` | 0x55cf09 | done |  |
| `SelectUnitsType` | 0x55feda | done |  |
| `SendUnitsToTransport` | 0x56093a | done |  |
| `SetAI_Guard` | 0x570251 | done |  |
| `SetBuildingsCollector` | 0x577c6f | done |  |
| `SetDangerMap` | 0x56fba1 | done |  |
| `SetDestPoint` | 0x567609 | done |  |
| `SetEnemyBuildingsCollector` | 0x577e71 | done |  |
| `SetGroupNMASK` | 0x56bceb | done |  |
| `SetGroupStartNMASK` | 0x56f6a2 | done |  |
| `SetLightSpot` | 0x564017 | done |  |
| `SetMaxInTransport` | 0x57033a | done |  |
| `SetNMASK` | 0x56bcb3 | done |  |
| `SetPlayerName` | 0x5697eb | done |  |
| `SetResource` | 0x562eca | done |  |
| `SetStandGround` | 0x5621cd | done |  |
| `SetStartPoint` | 0x564192 | done |  |
| `SetToInternalResourcesZero` | 0x56eba8 | done |  |
| `SetTrigg` | 0x563160 | done |  |
| `SetUnitEnableState` | 0x55c4f9 | done |  |
| `SetUnitInfo` | 0x567e8a | done |  |
| `SetUnitsCollector` | 0x577d70 | done |  |
| `SetUnitsState` | 0x578c09 | done |  |
| `SetUpgradeEnableStatus` | 0x55c437 | done |  |
| `ShowCentralText` | 0x5624a6 | done |  |
| `ShowPage` | 0x56255a | done |  |
| `ShowPageParam` | 0x56261d | done |  |
| `ShowVictory` | 0x5623df | done |  |
| `StartAI` | 0x565a1a | partial | the AI DLL is loaded by the runtime (Scripts::ai_requests); island research not done |
| `TakeFood` | 0x5664dd | done |  |
| `TakeWood` | 0x566637 | done |  |
| `TimerDone` | 0x5635f6 | done |  |
| `TimerDoneFirst` | 0x563669 | done |  |
| `Trigg` | 0x563117 | done |  |
| `TryUnit` | 0x56829b | done |  |
| `TryUpgrade` | 0x5684d6 | done |  |
| `TryUpgradeEx` | 0x568629 | done |  |
| `UnitsCenter` | 0x5637f1 | done |  |
| `UpgIsDone` | 0x569259 | done |  |
| `UpgIsRun` | 0x5692a9 | done |  |
| `isUnitAbsorber` | 0x5752be | done |  |
