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
| `kernel32` | shims for all 83 KERNEL32 functions the DLLs import |
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
2. **`OnInit()`**: in most missions it repeats the same registration sequence (the DLLs'
   global objects register in their constructors and `OnInit` registers again), e.g. 2,634
   `RegisterVar` calls in `Miss7E02`.
3. **`ProcessScenary()`** per game tick: trigger and timer polling (`Trigg(n)`,
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

## KERNEL32 emulation

The process is a single-threaded Windows XP (`GetVersion` = 5.1.2600) with code page 1252,
an empty environment, command line `"C:\American Conquest\dmcr.exe"`, a fixed clock
(1 January 2002), no files and no other DLLs (`LoadLibraryA` fails, which the CRT handles).
Critical sections are no-ops, TLS is a host-side table, standard output and
`OutputDebugStringA` are collected in `Process::output`. Every run is reproducible.

## Open problems

* **Game API**: `RecordingHost` is a stub. The real implementation on `acr-sim` needs
  signatures and semantics for the 210 functions; argument counts are known, types are not.
* **Engine structures exposed to scripts**: AI code reads `GAMEOBJ`/`OneObject` fields
  through pointers returned by `GetOBJ` and similar functions, and scripts hand the engine
  pointers to their own data (`RegisterVar`, `RegisterDynGroup`). The API layer must
  mirror those memory layouts inside guest memory.
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
* `DipServer.dll` imports (`AddCannon`, `SetAIForDiplo`, ...) go to the same host as the
  game API.
