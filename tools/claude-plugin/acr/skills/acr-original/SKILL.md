---
name: acr-original
description: "How to answer \"what does the original American Conquest 1.46 do?\" cheaply and with evidence: where its data lives (.MD, .NDS, WEAPON.NDS, SOUNDLIST.TXT, archives), the acr tool, reading dmcr.exe with objdump read-only, and how to cite addresses. Use for any parity question."
---

# Finding out what the original does

Answer from evidence only: a file and line, a quoted value, or a `dmcr.exe` address. Cheapest first: the extracted text data, then the docs, then the `acr` tool, then the disassembly.

## Never

- Run `dmcr.exe` or `vopl.exe` (a hook blocks it). Reading them as data is fine.
- Write into the game folder or commit anything from it.

## 1. The extracted text data (grep here first)

`C:\Users\verwalter\WIP\ACR-analysis\gamedata\` holds every text file of `ALL.GSC` and the patches (`.MD`, `.NDS`, `.LST`, `.TXT`, `.DAT`, `.INI`, `.CSV`; 905 files), with the patches applied. Read-only.

| Data | File(s) |
|---|---|
| unit and building definitions | `gamedata\*.MD` (252; e.g. `ARTPUS.MD`), localised texts in `gamedata\DESCRIPTION\` |
| nations: members, build lists, tech tree, upgrades | `gamedata\*.NDS` (16, plus `TEXT\IROKEZ.NDS`) |
| projectiles, child weapons, sync weapons, explosion media | `gamedata\WEAPON.NDS` (`WEAPON.ADS` is not extracted: `acr cat`) |
| weapon kinds (`MECH`, `IADRO`, ...), `ADDRANGE`, `ADDPAUSE` | `gamedata\NRES.DAT` |
| sound groups named by `.MD` `SOUND` lines | `gamedata\SOUNDLIST.TXT`; the `.wav` files are loose in `American Conquest\Sound\` |
| unit list, formations, morale, market, random table | `NMLIST.TXT`, `ORDERS.LST`, `ORD_GROUPS.LST`, `MORALE.DAT`, `ECONOMY.TXT`, `RANDOM.LST` |

- Keyword grammar and meaning: `docs/formats/definitions.md` ("Keywords" table: grammar, how many files use it). Text encodings: `MESSAGE` and Russian tokens are CP866, localised texts CP1252 (same doc, "Text encodings").
- Lines are `\r\n`; some `.MD` separate with tabs (`ATTACK_RADIUS\t\t4 1500 6000`), so grep with `-w` or `[[:space:]]`.
- Inventories: `ACR-analysis\3d\all.txt` lists every game file and the archive it comes from; `3d\types.txt` the 252 `.MD` with name and `USAGE`.

## 2. The docs

Reverse-engineered behaviour is already written down with addresses. Grep before disassembling: `grep -rn "0x4dc83c\|KillSelected" docs crates`.

| Doc | Covers |
|---|---|
| `docs/simulation.md` | engine rules: motion, paths, formations, combat, projectiles, artillery, economy, morale; "Deviations and simplifications" |
| `docs/ui.md`, `docs/controls.md` | menus, HUD, panels, keys and their network commands |
| `docs/formats/*.md` | `gsc` archives, `definitions` (.MD/.NDS/sounds), `gp` sprites, `m3d` maps, `terrain`, `fonts`, `bink` |
| `docs/script-runtime.md`, `docs/script-api.md`, `docs/mapgen.md` | mission/AI DLLs and their API, random maps |

## 3. The `acr` tool and the format crates

`acr` is `crates/acr-tool` (subcommand tables per module, `src/main.rs` `MODULES`). Use the prebuilt `C:\Users\verwalter\WIP\ACR-target\release\acr.exe` (it may lag main; fine for reading data), or on the VM (skill `acr-vm`): `acr-vm-cargo -- 'cargo run -q --release -p acr-tool -- unit "$HOME/game/American Conquest" ARTPUS'`. Local `cargo run` is blocked.

| Command (`acr <cmd> <game-dir> ...`) | Gives |
|---|---|
| `unit <NAME>` | one `.MD`: icon, usage, life, price, weapons per index, nation members, sprite sets and animations |
| `list [filter]` | every file and which archive wins (`WEAPON.NDS  Patch02.GS1`) |
| `cat <path>` | one file, decrypted |
| `extract <out-dir> [filter]` | archived files whose path contains the filter, e.g. `.GP`; the way to get binary formats out (sprites, fonts) |
| `defs-check [--keywords] [--all]` | parses every `.MD`/`.NDS`/`.SMP`/`.LNK`; keyword counts |
| `gp-sheet <gp> <out.png>`, `gp-frame <gp> <frame> <out.png>` | sprite pack frames (`crates/acr-formats/src/gp.rs`) |
| `model-ref <TYPE> <out-dir>` | reference views of a unit's sprites for the original camera |
| `sim-check <map> [ticks]`, `mission-run <id>` | ACR's own simulation on real maps and missions |

Readers in `crates/acr-formats/src`: `gsc.rs` and `vfs.rs` (archives: `ALL.GSC`, then `*.GS1` by name, then loose files; everything but `.GP` is XOR `0x78`), `defs/md.rs` (`UnitDef`, `grammar()` from the original's `sscanf` formats), `defs/nds.rs` (`NationDef`), `defs/nres.rs` (weapon kinds), `defs/sounds.rs` (`SOUNDLIST.TXT`), `gp.rs`, `rlc.rs`, `m3d.rs`, `terrain.rs`. `WEAPON.NDS` is parsed by `crates/acr-sim/src/weapon.rs`.

## 4. Reading `dmcr.exe` (read-only)

`C:\Users\verwalter\WIP\American Conquest\dmcr.exe`: 1.46 German, `pei-i386`. Past agents used `objdump -d` with virtual addresses (`docs/simulation.md` intro); no dumps or scripts are kept in ACR-analysis, so the docs are the notes. `objdump` and `strings` are on the Git Bash PATH (Strawberry Perl's binutils).

- **Sections:** `.text` 0x401000, `.rdata` 0x644000, `.data` 0x665000; for all three, VA = file offset + 0x400000.
- **A function:** `objdump -d --no-show-raw-insn --start-address=0x4dc83c --stop-address=0x4dc8c0 dmcr.exe` (AT&T syntax; add `-M intel` for Intel).
- **Who uses a string:** `strings -t x dmcr.exe | grep SHOWDELAY` gives file offset 0x26ddec, so VA 0x66ddec; `objdump -d --no-show-raw-insn dmcr.exe | grep -m5 0x66ddec` (about 5 s) finds `push $0x66ddec` at 0x4fdc95: the `.MD` loader compares the token with it (`call 0x615b40`); the neighbouring strings are the loader's "Unknown animation" errors. Pipe to grep; don't write full dumps anywhere.
- **Names:** strings name the sources (`D:\AC_PATCH_146_SOURCES\Cossacks2_Project\Motion.cpp`). The public Cossacks source helps to name functions; the binary decides.
- Struct fields in the docs: `OB+0x..` (object, `OneObject`), `NM+0x..` (type, `NewMonster`), `ADC+0x..` (`AdvCharacter`), `City+0x..`, `Nat+0x..`.

## Citing

- Functions: name and VA, `` `KillSelected` (0x4dc83c) ``; lowercase hex with `0x`. Fields: `NM+0x172a bit 0`. Constants: the address that holds them.
- Data: file, archive if it matters, and the quoted line: `ARTPUS.MD`: `ATTACK_RADIUS 4 1500 6000`; `WEAPON.NDS` (Patch02.GS1) line 38.
- Say what you did not confirm.

## Worked examples

- **Which shot does the cannon fire?** `ARTPUS.MD` has `WEAPON 4 PPOINTT`, `WEAPONKIND 4 IADRO`, `ATTACK_RADIUS 4 1500 6000` and `WEAPON 5 PSMPOINTTPUS`, `WEAPONKIND 5 KARTECH`, `ATTACK_RADIUS 5 0 1500`, `WEAPONINSECTOR 5 64 1500 400 1 32 50` (`acr unit ... ARTPUS`: `weapons: 4:PPOINTT, 5:PSMPOINTTPUS`). The attack link picks the index by distance alone: canister within 1500 px, the ball from 1500 to 6000 (`docs/damage-model.md` "Gun modes", `docs/simulation.md` "Artillery").
- **Is there a mortar shell?** `WEAPON.NDS` line 38: `NUCL3M #NUCL2 800 60 NONE HI_GRAVITY3 ANGLE 75 -1`, a 75° shell. Its only reference is the `[SYNCWEAPON]` line `DIMMORT1 2 NUCL3M DIMMORT`, and no `.MD` names `NUCL3M` or `DIMMORT1` (`grep -rlw` over `gamedata`), so the shell is in the data but unused. Method: a weapon counts as used only if a `.MD` `WEAPON` line names it, or a child or sync chain from such a weapon reaches it.
- **The reload bar:** the `.MD` flag `SHOWDELAY` (definitions.md: 163 files, among them `ARTPUS.MD` and `ENGM18.MD`) sets `NM+0x172a` bit 0. Such types get a reload bar under the life bar over the unit (0x4bb51a) and four green lines in the panel (0x6086ea), from `delay` (OB+0x8c) and `MaxDelay` (+0x8f) (`docs/ui.md`). ACR: `crates/acr-game/src/hud/mod.rs`, `ingame/interface.rs`.
- **The Delete key:** pseudo key 0xc8, network command 0x21, executor `KillSelected` (0x4dc83c), which runs `DestructBuilding` on the player's own living objects; the script call `SelDie` (0x5601c0) uses it too. ACR: `Command::Destruct`, replay tag 46 (`docs/controls.md`, `crates/acr-sim/src/command.rs`).
