# Simulation

`acr-sim` is the deterministic game simulation. This document describes its model and the
original algorithms it reproduces. Everything marked with an address was recovered from
`dmcr.exe` 1.46 by disassembly (`objdump -d`, virtual addresses). The engine descends from
*Cossacks: Back to War*; strings in the binary name the sources
(`D:\AC_PATCH_146_SOURCES\Cossacks2_Project\Motion.cpp`). The public Cossacks source helped
to name functions, but every rule below was checked against the American Conquest binary,
and differences from Cossacks are noted.

Check on real data: `acr sim-check <game-dir> <map> [ticks]` loads a map, spawns its units,
sends each player's units across the map in groups of 12, runs the simulation twice and
compares state hashes (see [Verification](#verification)).

## Game speed

**The original runs 10 simulation ticks per second at the default game speed**
(`acr_sim::ORIGINAL_TICKS_PER_SECOND`).

- The main loop (0x5ab120) runs exactly one simulation step (0x4a61e7) per rendered frame,
  then the frame limiter (0x4a7589).
- In single player (`NPlayers` u16 0x69e468 < 2) the limiter waits until
  `T() − PrevTime ≥ 2·FPSTime` ms. `T()` (0x5469ce) is `GetTickCount()` plus a network
  clock offset, which is zero in single player.
- `FPSTime` (exported global 0x28cf7c0) defaults to 50 (0x4a8541) and is clamped to 0..=100.
  The options slider shows `100 − FPSTime` (0x5c9e21) and stores `FPSTime = 100 − slider`
  (0x5ca52b). It is the 7th number in `mode.dat`.
- Ticks per second = `1000 / (2·FPSTime)`:

  | slider | FPSTime | ticks/s |
  |---|---|---|
  | 0 (slowest) | 100 | 5 |
  | 50 (default) | 50 | **10** |
  | 100 (fastest) | 0 | uncapped |

- **Why it runs too fast today:** the limiter only ever waits, and only for `2·FPSTime` ms.
  On 2002 hardware rendering took longer than that, so the game ran at 10–20 fps. With
  the slider at maximum (FPSTime 0, as in this installation's `mode.dat`) the speed is
  whatever the PC renders. The exe never calls `timeBeginPeriod`, so with the ~15.6 ms
  `GetTickCount` resolution the default setting actually gives about 9.1 ticks/s.
- In multiplayer `FPSTime` is ignored. The tick period is a network value (0x668f24): 80 ms
  at start (12.5 ticks/s), adapting between 80 and 160 ms (0x4d90e1, 0x4a71ee).
- Per tick, animations advance `FrmDec` = 2 frames and `SpeedSh` = 1 (0x668f10/0x668f14).
  Every game forces these values (0x4a770f, 0x53ddf3). A synchronised command can switch to
  1/0 (0x4db14d), which halves the speed of everything. `acr-sim` uses 2/1 (`FRM_DEC`,
  `SPEED_SH`).
- Other clocks:
  - Script timers count ticks, −16 per tick (0x55a1d2).
  - `GetGlobalTime` returns a counter that goes up by 2 per tick.
  - Peace time (m3d `CAEP`) is in seconds and counts down in real time (0x4bf48b). The
    division by 60 converts it to minutes for the announcement.

## Model

| concept | type | original |
|---|---|---|
| tick | `Tick(u64)`, `World::step(&[Command])` | one pass of the main loop |
| position | `Pos { x, y }`, i32 in 1/16 world pixel | `RealX/RealY` (OB+0xb0/+0xb4), also the unit of `UNI3` |
| direction | `u8`, 0 = east, 64 = south, 128 = west, 192 = north | `RealDir` (OB+0xea) |
| lock cell | 16 × 16 px, `cell = (Real − lx·128) >> 8` | `MotionField` bitmaps |
| area cell | 64 × 64 px | `TOP1`/`TOPW` |
| unit / building | `Unit`, stable `EntityId` = slot, never reused | `OneObject` |
| object type | `ObjectType` from `.MD` + nation member id, in `Catalog` | `NewMonster` |
| formation | `Brigade`, shapes in `Formations` | `Brigade` (0x4F bytes), `OrdDesc` |
| randomness | `SyncRandom` (table walk) | `rando()` (0x584891) |
| state hash | `World::state_hash()`: FNV-1a, `usize` hashed as u64 | — |

- **World:** the map (`MapGrid`: heights, land and water lock maps, area graph), 8 players,
  units in creation order, static map objects (trees, stones, ... from `TRE1`), brigades,
  the unit occupancy grid and the random index. Static data (`Catalog`, `Formations`,
  `MapGrid`) is shared through `Arc`. The dynamic state is hashed.
- **Commands** are plain data (`Move`, `Stop`, `Attack`, `Form`, `Face`, `Disband`, with the
  issuing player). They are applied in order at the start of a tick, and commands for units
  the player does not own are ignored. The command stream per tick is therefore a complete
  replay.
- **Iteration order:** per tick, the brigades' keep-positions handler runs first. Then units
  run player by player (0..7), each player's units in creation order, like `LongProcesses`
  (0x5112aa) walking the per-nation lists.
- **No floats:** the trigonometry tables are literal integers (see below). Fixed-point
  arithmetic uses the engine's own shifts and truncating divisions.

### Randomness: `RANDOM.LST`

The engine draws all synchronised random numbers from a 8192-entry table: `RANDOM.LST`,
loaded at 0x557843. Each call `rando()` returns `table[i]` and sets `i = (i+1) & 0x1fff`.
The table is exactly the first 8192 outputs of the MSVC runtime's `rand()` after `srand(1)`:
`s = s·214013 + 2531011; (s >> 16) & 0x7fff`. This was checked against all 8192 entries, so
`SyncRandom` generates the table instead of reading it. The index is reset to 0 at game start
(0x53de83, 0x4e33fe), or to a seed at 0x546fd4. Motion and path search call `rando()` at the
same places as the original:

- `TryToMove` and `TryToStand`: one call each, plus up to two in the rest branch.
- `CreateFullPath`: one call per refinement, one for the cooldown.
- Detours and failures: one call each.

The PCG32 `Rng` stays available for new code that does not need to match the original.

### Trigonometry

At start-up 0x5577a0 fills three 257-entry i16 tables with the x87 FPU, truncating with
`_ftol`:

- `TSin[i] = trunc(256·sin(i·K))` and `TCos[i] = trunc(256·cos(i·K))`, with
  `K = 0.02454369296875` (0x645520).
- `TAtg[i] = trunc(atan(i/256)·40.74366482962607)` (0x645510).

`K` is slightly larger than 2π/256. As a result `TSin[64]` and `TCos[256]` are 255 instead
of 256, and `TCos[128]` and `TSin[192]` are −255. `TAtg[256]` is 31, so `GetDir` returns 33
for an exact diagonal. Every other entry is at least 0.0019 away from an integer, so the
literal tables in `trig.rs` match regardless of FPU precision.

- `GetDir(dx, dy)` (0x503df1): octant lookup in `TAtg`.
- `Norma(dx, dy)` (0x427210): `(max(|dx|,|dy|) + |dx| + |dy|) >> 1`.

## Passability

Two bitmaps of 16 px cells (`MFIELDS`, 0x277885c): land and water. A set bit means blocked.
In memory the engine stores them column-major; `LockMap` is row-major, which does not change
behaviour. The primitives keep the engine's edge rules:

- `CheckPt` (0x5335ef) treats cells outside the map as blocked.
- `CheckVLine` (0x533710) also treats `x ≤ 0` or `y ≤ 0` as blocked, and any line longer
  than 24 cells.
- `CheckBar` (0x533808) is the box test built on `CheckVLine`.

Map load (`CreateLockMap` 0x44e340, `LockByTerrain` 0x44d800, then 0x44e250 and
`RelockAll` 0x44e4f0), in this order:

1. **Slope.** For each 64 px cell, take the terrain height at its four corners (the height
   function 0x4d0530 interpolates in the triangulated vertex grid; reproduced exactly). If a
   corner differs from their mean (`sum >> 2`) by 23 or more, the 4 × 4 block is locked,
   otherwise cleared.
2. **Blocking sprites.** Rocks are sprite types with a `[LOCK]` polygon in the list's `.RSR`
   and no `[INTERNAL_SOURCE]` entry.
   - The picture origin is `(x − dx, y/2 − dy)`, with `dx dy` from the `.LST`. Polygon y is
     at half scale.
   - A cell is locked if its centre `(16cx+8, 8cy+4)` is inside the polygon. The test
     (0x4cfdc0) is even-odd: edges with `x1 ≤ px < x2`, truncating interpolation.
   - Ordinary trees, palms, bushes and `ST*` stones do not block.
3. **`LOC1`:** the unlocked 64 px cells are cleared first, then the locked ones set.
4. **Quantisation** to 64 px: a block is locked iff at least 3 of its four 32 px quadrants
   contain a lock.
5. **Chamfers.** Two passes over a copy look at each block's N/E/S/W neighbours:
   - Masks 3, 6, 9 and 12 stamp a 45° corner pattern (0x644560..0x64459c).
   - Masks 7, 11, 13 and 14 lock a free block entirely (first pass only).
6. **Water** from the `SEA2` depth (32 px cells), sampled at the four 16 px sub-cells with
   weights 9/3/3/1 (`(3(3p+q+r)+s) >> 4`). Above 128 the land layer is locked; above 130 the
   water layer is free, otherwise locked.
7. **Map borders** on both layers, and the top edge: rows 1..=24 where terrain would be drawn
   above the screen top (`32·ay − height(64ax+32, 64ay) < 16`).
8. **Mines** (`[INTERNAL_SOURCE]` sprites with a polygon) are rasterised after the
   quantisation, at full 16 px resolution (0x44e250).
9. **Buildings** lock their `LOCKPOINTS`.
   - The base cell is `((RealX + 16a) >> 8, (RealY + 32b) >> 8)` with `LOCATION a b`
     (0x516fde).
   - The points are stored as bytes (NM+0x16c6), so only their low 8 bits count.
   - Map buildings are created fully built, so `LOCKPOINTS` (not `BUILDLOCKPOINTS`) apply.

Units do not touch these maps. They use the separate occupancy grid (`UnitsField`,
0x1100d78), described under motion.

## Pathfinding

### Coarse level: areas

`TOP1` areas (64 px cells) come with precomputed next-hop and distance matrices; nothing is
searched at run time. A player move uses `SmartSendTo` (0x52bc95) with link 0x52b84a:

- If the straight line to the target stays inside areas (`TopLineClear` 0x52c3ff), the unit
  walks directly.
- Otherwise the next area is `next[cur][dst]`. Up to 3 further areas are skipped while a
  straight line to them is clear.
- The leg's target is the area centre (`64·c + 32`) plus the unit's group offset.
- A fine-level `SendTo` order is pushed on top. When it completes, the link runs again.
- A group offset longer than 128 px is shrunk to `(dx·120/n, dy·90/n)`.

### Fine level: `CreatePrePath`

This is not A* (0x534031):

1. Draw a Bresenham line from the unit's cell to the target (error term starting at 0). If
   no cell on it is blocked, walk straight. The target cell itself is not tested.
2. Otherwise find the first point whose box with a 1-cell margin (`lx + 2`) touches an
   obstacle, and start from the point before it. The exit reference is the last blocked
   point on the line.
3. Two wall followers start there in lock-step:
   - the right one keeps the obstacle on its left (scanning from `dir + 6` clockwise);
   - the left one is its mirror image.

   They move in 8 directions, all at cost 1, using a neighbour mask of the margin box
   (0x533854). Each follower accumulates the cross product of its steps with the line. A
   follower finishes when that sum returns to ≤ 0 past the exit point. The left follower
   wins a tie in the same step. Each follower may take at most 4088 steps.
4. String pulling on the winner's trace:
   - The start is the trace point with the largest pseudo-angle deviation from the line
     direction (2048 per turn, 0x4ef9c8).
   - The end is the first later point from which the target direction is free (+1 cell).
   - Every second cell in between becomes a waypoint.

   The path is stored reversed: the last element is the next waypoint.
5. On failure the cooldown is set to `200 + rando()&15`. `CreateFullPath` immediately
   overwrites it with `rando()&7` (original bug, kept).

- `CreateFullPath` (0x533f1a) first moves the target out of obstacles with
  `FindBestPosition`: square rings of radius ≤ 40, choosing the free cell nearest to the
  **unit** (`Norma`). It then refines the first leg up to 4 times.
- `CreatePath` (0x538d85) runs every tick from the `SendTo` link. It steers at the next
  waypoint's cell centre and re-plans only when:
  - the target cell changes,
  - the next waypoint's box is blocked, or
  - the path runs out.

  A waypoint counts as reached at Chebyshev distance `< (STOPDISTANCE >> 8) + 1`.
- The `SendTo` link (0x5172c6):
  - It runs `FindBestPosition` (radius 80) when the target box is blocked.
  - It gives up when the cooldown is active and `StandTime > 64`.
  - The order completes at Chebyshev distance `≤ STOPDISTANCE >> 8` from the reserved cell
    (default 256 → 1 cell, cavalry 1024 → 4), once the current animation cycle has
    finished.

### Group moves

The planner `Place` (0x4ab607) builds the slots for a group move:

- Grid size: `s = isqrt(N)`, width `W = 2s` across the movement direction, depth
  `D = s >> 1` (N < 4: one row of N). Then two rounds grow `W` and/or `D` until `W·D ≥ N`.
- Slot spacing is 4 × the largest `GEOMETRY` radius 2 (`r2 << 4`). With mixed unit types the
  radius is capped at 256 first.
- Units are sorted along the front, then each column along the depth (stable sorts on
  `(RealX>>5)·a + (RealY>>5)·b`), so paths do not cross.
- The facing is the vector from the target back to the group centre.

Each unit then routes on its own with `SmartSendTo` and its offset.

## Motion

Every infantry and cavalry unit uses `MOTIONSTYLE` 0 (`FASTROTATE&MOVE`, `MotionHandler1`
at 0x50e823). Only ships, sheep and two special units set another style.

The `GEOMETRY r1 r2 step` handler (0x4f9502) precomputes `OneStepDX[d] = TCos[d]·step >> 8`
and `OneStepDY[d] = TSin[d]·step >> 8`: the distance per animation frame in 1/16 px.

Per tick (`FrmDec` = 2, `SpeedSh` = 1):

- **In a walking cycle** (`frame < frames − 2`):
  `pos += vel << 1; frame += 2`, with `vel = OneStepDX[dir]`.
- **At the end of a cycle** (a decision step):
  1. Aim at the steering point: `best = GetDir(dest − pos)`.
  2. If `|best − dir| > ROTATE` (16, NM+0x2932), turn in place by 16 and stop. A 180° turn
     takes 7 ticks.
  3. Otherwise pick the direction by `MoveStage`: `best`, then while blocked the detours
     `best ± 16, ± 32, ± 48`, then `best` with pushing.
  4. Look ahead a whole cycle: `(#MOTION_L frames >> 1) · (OneStepDX << 1)`.
  5. Clear the unit's cell in the occupancy grid, then test the end-of-cycle cell, first
     against the occupancy grid, then against the land map.
  6. **Free:** reserve that cell and start the cycle (`TryToMove` 0x50b021). The unit moves
     its first step at once and sets `frame = 1`. `#MOTION_L` and `#MOTION_R` alternate.
  7. **Blocked:** stand (`TryToStand` 0x50a8c5) and keep the old cell.
     - After the detours, a map obstacle triggers a re-plan to the current waypoint
       (cooldown `32 + rando()&7`).
     - `RemoveFools` (0x50e2f3) swaps places with an idle friend that has stood for more
       than 20 ticks, or sends other idle friends to the nearest free cell.
- **No target:** stand. At a finished animation, `TryToStand` starts `#REST`/`#REST1` with
  probability 512/32768.

Speeds per tick (only odd animation frames are shown, 2 frames per tick):

| unit | GEOMETRY step | motion frames | px per tick east | per cycle |
|---|---|---|---|---|
| musketeer, pikeman (`EngMus`, `EngPik`) | 40 | 10 | 5 | 5 ticks, 25 px |
| cuirassier (`EngD18`) | 110 | L 6 / R 8 | 13.75 | 3 / 4 ticks |

At 10 ticks/s infantry walks 50 px/s and cuirassiers 137.5 px/s.

Units never compare real positions. The reserved 16 px cells are the only collision
mechanism, and **neither `UNITRADIUS` nor the GEOMETRY radii affect motion**:

- `UNITRADIUS` is only read by projectile hit tests (0x559310).
- The radius check `CheckPosition` is computed in `TryToMove`, but its result is ignored.

`RATE`/`SPEED` upgrades scale the step only in non-zero stances (`NewState`). This is an AC
difference from Cossacks, which also scaled stance 0.

### Directions and sprites

The drawing code (0x5062a3) selects the sprite frame from `RealDir`:

- The GP frame index is `SpriteID·R + k`, with `R` the stored directions. Frames are
  **direction-interleaved**: frame f of direction k is at `f·R + k`. (`docs/formats/definitions.md`
  presumed `d·N/R + f`.)
- **9 directions:** `dir16 = ((RealDir + 71) & 255)·16 >> 8`.
  - If `dir16 < 8`: `k = 8 − dir16`, drawn mirrored.
  - Otherwise: `k = dir16 − 8`.
  - Stored k = 0 is south, 4 west, 8 north; the east half is the mirror image.
- **16 directions:** `dir32 = ((RealDir + 64) & 255)·32 >> 8`, mirrored below 16.
- **`FULLROTATION`:** no mirroring, `k = ((RealDir + 128/R − 64) mod 256)/(256/R)`.

## Formations

Shapes come from `ORDERS.LST` (parser 0x4ed6e0) and `ORD_GROUPS.LST` (0x4ed400).

- **Slots.** `*` is a soldier slot (numbered row by row), `C` an officer, drummer or flag
  bearer slot. Coordinates are `x = 2·col − avgX` and `y = 2·row − avgY` (integer averages
  over the soldier slots). **One character = 32 px**, horizontally and vertically.
- **Symmetry.** `SYM2` builds a 180° slot permutation: the k-th slot from the top of a column
  maps to the k-th from the bottom of the mirrored column. `SYM4` also builds ±90°
  permutations and requires as many columns as lines.
- **Bonuses.** The header numbers are attack/defence bonuses: standing, normal, and extra
  with a flag bearer.
- **Size variants.** A group (`LINE 3 0 1 2 5 #LINE15 … #LINE196`) lists the buttons offered
  and the size variants. Re-forming takes the first variant with enough slots (0x4e1cd3):
  infantry 15/36/72/120/196, cavalry 40/90/160.
- **Placement** (`Arrange` 0x4ad9af), with `c, s = TCos/TSin[dir]`, `L = Norma(c, s)`,
  `cx = c·2048/L`, `sy = s·2048/L`:
  - `X = x0 − ((xi·sy + (avgY − 2r)·cx) >> 3)`
  - `Y = y0 + ((xi·cx + (2r − avgY)·sy) >> 3)`

  Lower text lines stand in front. Diagonal facings shrink by about 6 %, as in the engine.
- **Slot assignment.** Soldiers are sorted by their projection on the facing (rearmost to the
  first line), then left to right within each line. Officer, drummer and flag bearer then
  take the nearest free `C` slot, in that order (`PickCom` 0x41a65d, raw `TCos/TSin`).
- **Moving** (0x41b83a):
  - An order within 16 px of the current centre is dropped.
  - The new facing is the direction to the target.
  - Turns under 32 just re-place the formation.
  - Larger turns re-map soldiers through the symmetry permutations, so they do not cross.
  - Shapes without symmetry turn by only 32 per order (engine quirk).
  - Every member then walks **individually** to its own slot at the target. There is no
    speed matching and the shape is not kept on the way.
- **Keeping positions** (0x41c0a9): members more than 100/16 px off their slot are sent back,
  and members in place turn to the formation's facing.
- **Forming.** In the game an officer needs exactly N free soldiers of one type within
  1024 px (0x4198b3). The drummer and flag bearer types come from the officer's data. The
  formation is placed centred on the officer, facing 0.

## Deviations and simplifications

- **`Form` command.** It takes the units to use rather than searching the officer's
  surroundings. Drummers among them become the drummer. Flag bearers are not assigned
  (the officer→flag-bearer type table is not loaded). Fewer than N soldiers are accepted,
  using the smallest variant that fits.
- **Several brigades in one move.** Each keeps its offset from the group centre; the
  original's block placement (0x41db45) is not reproduced.
- **Keep-positions** only re-sends idle members. The original re-sends every tick and checks
  the target cell first.
- **`ApplyOffset`** (0x52b0cd) and the 10-ring spiral table (0x28a6f10) are approximated by
  square rings and a simple free-box test.
- **`RemoveFools`.** Pushed friends get a `SendTo` to the nearest free cell (the original uses
  its `Rarr` push tables). Enemies in the way are not attacked: there is no combat yet, and
  `CheckMotionThroughEnemyAbility` (0x41df98) is not reproduced.
- **Animations.**
  - Only `#STAND`, `#REST`, `#REST1`, `#MOTION_L`, `#MOTION_R` are modelled.
  - Stance changes (`NewState`), `BREAKANIMATION`, attack and build animations are not.
  - Units without `#MOTION_L` are treated as 1-frame cycles.
- **Not reproduced yet:**
  - motion styles 1–5 (ships and sheep use style 0 on the water layer here);
  - `GotoFinePosition` (0x4a0670, at `StandTime == 48`);
  - unit heights (`RZ`);
  - the optional terrain pass 0x449aa4;
  - `STAGELOCKPOINTS`, construction (`BUILDLOCKPOINTS`), walls and gates;
  - deleting sprites under new buildings (0x4d1df0).
- **Iteration order.** Units within a player run in creation order. The engine's per-nation
  lists can be reordered by deaths and new units.
- **Initial random index** is 0, as in a single-player game start.

## Verification

- **Unit tests** (`cargo test -p acr-sim`) check:
  - trig quirks, `GetDir`, `Norma`, the `RANDOM.LST` sequence, FNV-1a;
  - height interpolation, lock map edge rules, sprite polygons, building lock cells;
  - the path search (wall detour, target relocation);
  - formation compilation, permutations and slot geometry.
- **World tests** (`tests/world.rs`) on a synthetic flat map check:
  - exactly 5 px per tick for a step-40 unit;
  - 7 ticks of turning before walking west;
  - arrival and stop;
  - a detour around a wall;
  - a 20-unit group move ending in 20 distinct cells;
  - forming and moving a 15-man line;
  - identical hashes for identical command streams.
- **`acr sim-check`** on all 63 maps, 3000 ticks (5 minutes of game time):
  - Hashes are identical between runs at every checkpoint on every map.
  - 0 units outside the map.
  - Shared cells: 0, except 4 on `battle4` from units placed overlapping in the map.
  - 75–100 % of units per map, 93.9 % of all 108 625, reach their group's target
    (≤ 320 px). The remainder are still queued in crowds of up to 6000 units (Saratoga) or
    have targets across water.
  - Speed: 380 (Saratoga, 6000 units) to 100 000+ ticks/s in a release build, against the
    original's 10.

## Open questions

- Motion styles 1–5 (`SLOWROTATE`, `SHEEPS`, `COMPLEXROTATE`, `ROTATE&MOVE`, `NEWSHEEPS`);
  ship pathing on the water layer (0x535b04).
- Where non-zero stances (`NewState`, e.g. pikemen's `#MOTION_L0`) are entered, which decides
  when `RATE` and the `SPEED` upgrade apply.
- What runs the brigade order list each tick, and `0x41d090` at the end of `PlaceBrigade`.
- The exact `Rarr` push tables of `RemoveFools`, `ApplyOffset`, the 10-ring spiral table.
- The first move of a unit standing on a locked cell (0x538c4e) and the repulsion vector
  OB+0xd4/+0xd8 of the other motion variants.
- Whether `LeftLeg` starts at 0 for map units (the first cycle then plays `#MOTION_R`).
