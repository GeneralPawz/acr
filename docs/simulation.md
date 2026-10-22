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
| animation | `AnimKind` (`name()` = `.MD` slot, e.g. `#ATTACK1`), `frame`, `anim_frames` | `NewAnm`, `NewCurSprite` |
| stance, reload, death | `stance`/`local_stance`, `delay`, `death` (`living()` = exists and `death == 0`) | `NewState`/`LocalNewState`, `delay`, `Sdoxlo` |
| randomness | `SyncRandom` (table walk) | `rando()` (0x584891) |
| state hash | `World::state_hash()`: FNV-1a, `usize` hashed as u64 | — |

- **World:** the map (`MapGrid`: heights, land and water lock maps, area graph), 8 players,
  units in creation order, static map objects (trees, stones, ... from `TRE1`), brigades,
  the unit occupancy grid and the random index. Static data (`Catalog`, `Formations`,
  `MapGrid`) is shared through `Arc`. The dynamic state is hashed.
- **Commands** are plain data (`Move`, `Stop`, `Attack` (priority 16), `Form`, `Face`, `Disband`, with the
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

## Combat

Implemented in `combat.rs`, recovered from `SearchVictim` (0x52622b), `AttackObj`
(0x51a930), `AttackObjLink` (0x51bcaf), `MakeDamage` (0x4e8990), `TraceObjectsInLine`
(0x559310), `Die` (0x4e77a0) and the object loop (0x5118b3).

**Per tick:**

1. `SearchVictim` runs for every unit.
2. Player by player, each unit runs its order link (`AttackObjLink` for attack orders), then
   the motion handler. The motion handler also advances strike animations by 2 frames.
3. The object loop counts down reloads and plays death animations.

### Data

- **Attack index `i` is stance `i + 1`.** Its animation is `#ATTACKi` (`#ATTACK` for 0), with
  `#PATTACKi`/`#UATTACKi` to enter and leave the stance and `#PSTANDi` to stand in it.
  `ATTACK_RADIUS i r1 r2` also sets the detection range `DET_RADIUS`, so a `DET_RADIUS`
  must come after it. `CANKILL` overwrites every `ATTMASK`.
- **`ATTACK_PAUSE` counts animation frames.** It drops by 2 per tick, except for
  `SLOWRECHARGE` units (musketeers). These reload only by playing `#ATTACK3`; each loop
  subtracts its frame count.
- **`NOPAUSEDATTACK i`:** index `i` (the bayonet) can be used while reloading and does not
  reset the reload.
- **Weapon kinds** come from `NRES.DAT [WEAPONS]`, in line order. Their flags:
  - `H`: height adds range.
  - `R`: 4 % instant kill.
  - `O`: the formation attack bonus applies.
  - `B`: against buildings it only hurts the garrison.
- **Auto-targeting.** `SEARCH_ENEMY_RADIUS r` / `_SHIFT p` give the search circle
  `SR = (r<<4)·100/(p+100)`, centred `SS = SR·p/100` ahead of the unit (in 1/16 px). The
  musketeer gets 500 px, centred 350 px ahead. Units without `SEARCH_ENEMY_RADIUS` never
  attack on their own.

### Rules

- **`SearchVictim`.** On a tick a unit draws `rando()`, and anything above `0x2000` skips
  the search (about 3 ticks in 4). Otherwise it scans the `(2·(SR>>11)+1)²` cells of 128 px
  around the search centre.
  - In each cell it takes the first living enemy whose material it can kill.
  - It keeps the nearest one to the centre with `MinR·16 < d < SR`.
  - It attacks that enemy with priority 1.
- **`AttackObj`** refuses a target the unit cannot hurt, or an order with priority below the
  current one. Player orders have priority 16, so moving units do not get distracted. With
  priority < 5 the unit only switches to a strictly closer enemy. (The engine compares the
  1/16 px distance with `MinR` in pixels; the quirk is kept.)
- **`AttackObjLink`**, every tick:
  1. **At the active frame of `#ATTACKi`** (`SETACTIVEPOINT`, rounded up to even): face the
     enemy and check the range again. `dst ≥ r1` and
     `dst − ADDSHOTRADIUS (− 2·Δz for H weapons) ≤ r2 + ADD_ATTACK_RADIUS`.
     - In range, set the reload (`delay = ATTACK_PAUSE`) and strike or shoot.
     - A melee victim answers with an attack of priority 2.
  2. **Otherwise act only at the end of an animation.**
     - Choose the **last** index whose detection range and mask fit. While reloading,
       prefer a `NOPAUSEDATTACK` index.
     - Musketeers play `#ATTACK3` reload loops while the reload lasts.
     - **In range:** turn (snapping within `min(16, 1.5·ROTATE)`), switch stance if needed
       (the transition animation plays at the cycle end), then start `#ATTACKi`.
     - **Out of range:** walk to the enemy's cell (stance 0).
- **Damage** (`MakeDamage`):
  - Start with `DAMAGE`, plus `min(kills·SKILLDAMAGEBONUS/10, 19·damage)` if the mask
    allows, minus `SHIELD`.
  - `R` weapons: `rando() < 1310` (4 %) kills outright, except buildings and `NO25` units.
  - Then subtract `PROTECTION[kind]`, kept as a byte.
  - The formation bonuses (shape header numbers 3 and 4) are scaled by the angle to the
    brigade's facing: full within 40, half below 70, none beyond; `#KARE` gets full.
  - At least 1. A surviving victim counter-attacks with priority 1.
- **Musket shots** are instant line traces:
  - The aim point is dispersed per axis by
    `((rando()>>5) − 512)·(Norma>>5)·RAZBROS/32000`, about ±57 px at 400 px.
  - Samples every 16 px run out to twice the distance, each 128 px cell checked once.
  - A unit qualifies if `0 < z − ground < 90`, it is more than 60 px from the shooter (150
    for friends), and the line passes within `UNITRADIUS` (doubled for friends within
    250 px). The qualifying unit closest to the shooter takes the hit.
  - Damage halves every `DAMAGEDEC` px.
  - `FRIENDLYFIRE` means *avoid* friendly fire: a friend hit absorbs the ball unharmed.
  - Random numbers are drawn as in the original: 2 + 2 + 4 per shot, plus 1 for the `R`
    test.
- **Enemy contact while walking** (`CheckMotionThroughEnemyAbility` 0x41df98): if an enemy
  stands within 32 px in the 128 px cell at the end of the next cycle, the walker stops and
  attacks it (priority 0x8F). Its cell is left unreserved, as in the original.
- **Death:**
  - The unit turns by `(rando()>>8) − 64`, frees its cell and plays `#DEATH`, then lies as
    `#DEATHLIE1`. After 1200 / 1600 it switches to `#DEATHLIE2` / `#DEATHLIE3` where the
    type has them.
  - The death counter (`Unit::death`) grows by 2 per tick while lying, and the corpse is
    removed above 2000 (about 100 s).
  - Attackers keep striking a target whose counter is still ≤ 8.

### Worked timing (musketeer, 10 ticks/s)

- First shot: `#PATTACK1` (63 frames, 32 ticks), then 7 ticks to the active frame 14. The
  test `musketeers_fire_on_schedule_and_hit` checks this window.
- Steady fire: about 307 ticks (31 s) per shot: the rest of `#ATTACK1`, 16 reload loops of
  16 ticks, `#TRANS3X`, and the next shot.
- The bayonet strikes every 26 ticks; a pikeman every 10 ticks.

## Economy

Implemented in `economy.rs`. The sources are `City::Census` (0x42e658), the upkeep at
0x4a6b8d, starvation (0x511ab3), `GetUnitCost` (0x514781), `Produce` / `ProduceObjLink` /
`UnProduce` (0x49e649, 0x49eeb7, 0x4dbd47), mines (0x5f75b3) and `GoToMine` (0x5f43d0).

- **Resources.** They are indexed in `NRES.DAT [RESTYPES]` order: wood 0, gold 1, stone 2,
  food 3, iron 4, coal 5 (`economy::WOOD` …).
  - Every player starts with the `[RESTYPES]` amount of 500 000 each (`START_STOCK`). The
    map's `RESU` chunk (8 players × 8 values, same order) then replaces it.
  - There is no upper limit. Missions use 0x4876E7FF as "unlimited".
- **Census**, every 8 ticks per player (player `tick & 7`), also after a unit is produced:
  - housing (`NFarms`): the sum of `FARM` = 15 / `MFARM n` over finished buildings;
  - population (`NGidot`): living non-buildings, including units inside buildings;
  - the summed `RESCONSUMER` rates;
  - the object count per type, used for prices.
  - When a player's gold drops below 2, `NAEMNIK` mercenaries desert to player 7 for good,
    and `TURNOFF` units stop working.
- **Upkeep**, every tick:
  - Food: `Harch += 3·population`; every whole 1000 costs one food. When food runs out,
    `Harch` becomes 0.
  - `RESCONSUMER RES v` costs `v·100/1 000 000` per tick, with the remainder carried. Peasants
    (`FOOD 32`) eat 0.0062 food per tick in total; other units 0.003.
  - With `Harch == 0`, every unit draws `rando()` each tick, and `≤ 50` kills it unless it is
    `NO_HUNGRY`.
- **Prices.**
  - The price is `PRICE`, multiplied `n` times by `COSTPERCENT/100`, truncated at each step.
    `n` is the player's count of that type, and growth stops once a ×1000 tracker passes 10⁶.
  - With `LOWCOSTRADIUS`, a building placed far from others of its type pays the base price.
  - The price is paid when a unit is queued.
- **Production queues.**
  - The head order gains 2 per tick, so a unit takes `ceil(BUILDSTAGES/2)` ticks (peasant:
    100). It waits while `housing < population + 1`.
  - A `PEASANTCONVERTOR` building producing a non-peasant instead uses up a peasant from
    inside.
  - The unit appears at the first `BORNPOINT`, a cell corner relative to the lock base cell
    `((RealX + 16a) >> 8, (RealY + 32b) >> 8)` with `LOCATION a b`, facing the second. It
    walks through locks and other units ("unlimited motion", 0x527caa) to every further born
    point, one cell inside each (`(cell + 1) << 8`), takes its cell (0x527fed) and steps to
    the last born point proper.
  - **Rally point** (`DstX/DstY`, 0x49fcc6): on a building that takes units in, the producer
    itself takes the unit straight inside, another building gets a `GoToMine`; on a resource
    source (`DetermineResource`, 64 px) a peasant draws random points within ±128 px until
    one lies at a source of the same type and gathers there; otherwise the unit walks
    (`SmartSendTo`) to the first free spot of `FindUnitPosition` (0x51508c).
  - `FindUnitPosition` walks the rings of the `Rarr` table (0x28a6f10, built at 0x546ac0:
    ring `r` holds the offsets with `trunc(sqrt(dx² + dy²)) == r`, ordered by `dx`, then
    `dy`) at 32 px spacing, rings 0..50, each forwards if `rando() < 0x4000`, else backwards.
    A spot is free (0x513b2c) on the map, with the footprint unlocked (0x50a60d) and no unit
    within `r2 + 8 + r2'` (`GEOMETRY` radii, 0x50a495) in the 3×3 128 px cells around.
  - Cancelling refunds the price at the count when it was queued; progress is lost. There is
    no queue limit.
- **Market** (`market.rs`; `ECONOMY.TXT` loaded at 0x4a8f82, `PerformOperation` 0x4a9d80,
  `Process` 0x4aa0af). The rates are global, not per player (unlike Cossacks).
  - Each of six slots has `RES BUY b0 b1 b2 CHANGE c ex TIMERETURN t SELL s0 s1 s2 CHANGE c'
    ex' TIMERETURN t'`, read as floats and kept as doubles: `ExpBUY = ex / (c·100)`. The
    rates start at `b1`/`s1`. The shipped file has `CHANGE 100 1.0 TIMERETURN 0.0001`
    everywhere; e.g. wood buys at 50 and sells at 30, gold 300/210, food 25/15.
  - A trade of `amount` (at most the stock) gives `trunc(amount · Sell[sold] / Buy[bought])`.
    Then with `V = got · ExpBUY[bought]` the bought resource's buy and sell rates become
    `(r + limit₂ · V) / (1 + V)` (towards `b2`/`s2`), and with `V = amount · ExpSELL[sold]`
    the sold resource's rates move towards `s0`/`b0` the same way.
  - Every tick each rate above its normal value drops by `TIMERETURN` and each below rises,
    sell before buy (here right after the tick's commands).
  - The doubles follow the engine's (x87 at double precision); intermediate products are
    rounded to double here, where the engine keeps 80 bits, which can rarely change a
    truncated result by 1. No market building is required by the simulation (the game's
    interface asks for one).
- **Tech tree** (`tech.rs`; nation loader 0x60e7f0, `City::RefreshAbility` 0x42ffed). Every
  nation file goes into one namespace of types and upgrades, as in the engine.
  - A building produces only the types of its `[FIXED_PRODUCE]` list (the produce command
    0x4dbac2 checks it through 0x49e060), and only enabled ones (the command panel offers
    no others). Peasants build only what their own list names. `Rally` works only for
    `[CANSETDEST]` buildings.
  - A type is `Enabled` from `[ENABLED]` (which also sets `CondEnabled`) or an `ENABLE`
    upgrade. With every census (`ProcessCreation` 0x432582 = `EnumUnits` +
    `RefreshAbility`), and after every placement, each `[ACCESSCONTROL]` target is first
    disabled, then enabled again if every required type has a finished object
    (`ReadyAmount`: living units and finished buildings) and every required upgrade is done:
    a type only if `CondEnabled` or a building, an upgrade only if `PermanentEnabled` and
    not yet done. `[UNITLOCK] ID n LOCK` disables `ID` while the player has `n` objects of
    `LOCK` (steamships: 3), otherwise it follows `CondEnabled`.
  - `[STAGEDEPENDENT] ID mask` (GeneralObject+0xad, NewUpgrade+0x4d): the producing building
    must have done every stage in the mask (state 2 in its five 3-bit fields OB+0xaa,
    raised by `UPSTAGEn` upgrades).
  - A queued type that gets disabled is dropped when its turn comes, without refund
    (0x49eeb7).
- **Upgrades** (`upgrade.rs`; parser 0x52f798, `PerformUpgrade` 0x54f25b,
  `PerformUpgradeLink` 0x54f6bb, `StopUpgrade` 0x4e19ad, `PerformNewUpgrade` 0x532281).
  - **Definition.** `ID ICON #POSITION x y #LEVEL n MESSAGE #COST n (RES v)*n #TIME t`, then
    the effect: a category, its attack indices (`WEAPONKIND k`, `ATTINDEX i`, `ALL`) or
    protection kinds, the types (`GRP n ID*n` or one id) and a value (`n`, `+n`, `-n`, `n%`,
    `+n%`, `-n%`; applied by `UseValue` 0x530fe7 with truncating division). Costs and `#TIME`
    are u16.
  - **Starting** needs a finished building that is not researching, the upgrade in its
    `[UPGRADEPLACE]` list and enabled for the player, the building's stages for the
    upgrade's `[STAGEDEPENDENT]` mask, and the price. A `[PRIVATE]` upgrade also needs the
    building's upgrade count (`SingleUpgLevel`, OB+0xe9) to equal its `#LEVEL`; a stage
    upgrade (`[UPGRADESTAGE]`) needs its stage not done yet. The price is paid at once. A
    nation-wide upgrade is then no longer offered (`Enabled`, `PermanentEnabled` off,
    `IsDoing` on). The player command (0x4dc386) tries every selected building.
  - **Research** is an order in front of the building's queue: production and mining wait.
    Progress starts at 1 and gains 2 per tick, so it takes `ceil((#TIME − 1) / 2)` ticks
    (`#TIME 750`: 375). A stage upgrade marks its stage "in progress" (1) meanwhile.
    Cancelling refunds everything and offers the upgrade again.
  - **Completion.** A nation-wide upgrade is `Done`. The effect is applied with the building
    as its maker, then the `[UPGRADELINKS]` are enabled (`Enabled`, `PermanentEnabled`,
    `Done` cleared) and the `[AUTOUPGRADE]`s performed. The building's upgrade count rises
    by one for every upgrade.
  - **Effects** change the player's copy of a type's characteristics (`AdvCharacter`,
    `World::adv`), which combat, prices, production and construction read:
    - `DAMAGE` (`MaxDamage`, i16, indices without damage unchanged), `ATTPAUSE` (u16),
      `PROTECTION` (u8 per kind), `SHIELD` (u16), `RAZBROS`, `INSIDE` (`MaxInside`),
      `COST` (one value per resource), `MOREFARMS` (housing);
    - `ATTRANGE`: maximum attack and detection range of the index; the next index's minimum
      range becomes the scaled minimum detection range (the engine's bug: it overwrites the
      scaled `AttR1` with the scaled `DetR1`);
    - `BUILD` (`MaxStage`, u16): unfinished buildings of the type step back one stage
      (or to the new maximum − 1) and redo it, finished ones take the new maximum; running
      productions rescale their progress;
    - `LIFE` sets life and maximum of all objects of the type (capped at 65535),
      `MORELIFE` only the maximum, `HEALING` adds a tenth of the maximum;
    - `ENABLE UNITS` enables types (`Enabled`, `CondEnabled`) and reruns every census;
    - `EFFECT_FOOD`/`EFFECT_WOOD`/`EFFECT_STONE` change the player's delivery factors (u16,
      no caps unlike Cossacks); `GEOLOGY` sets a flag;
    - on the researching building: `SINGLE_INSIDE` (`AddInside`, OB+0xa5),
      `MOREFARMS_SINGLE` (OB+0xf3 = the type's housing changed by the value, added to the
      housing), `PERSONALLIFE` (life and maximum), `UPSTAGEn` (0x53268e: if the type has
      that `ADDSECTION`, the stage becomes done; life and maximum become the type's life
      plus the third `ADDSECTION` number of every done stage).
- **Mines.**
  - A `PRODUCER` building binds to a deposit within 128 px: an `[INTERNAL_SOURCE]` sprite of
    a resource it can mine. Binding happens when it is placed and every 256 ticks.
  - Each tick it adds `(free + inside·per_peasant) << 3` to a u16 accumulator and pays out per
    1000. That is 0.08 per peasant per tick for `PRODUCER 1 GOLD 0 10`. No randomness;
    deposits never run out.
- **Construction** (`construction.rs`; `CreateObject` 0x5155c5, `BuildObj`/`BuildObjLink`
  0x523c98/0x524654, `NextStage` 0x523666).
  - **Placement.** The position snaps so that `RealX + 16a` and `RealY + 32b` fall on a 16 px
    corner. The site must lie on the map, the finished footprint must be free, and the price
    (with `LOWCOSTRADIUS`) must be affordable; it is paid at once.
  - **The site** starts at stage 0 with 2 life (`#BUILDLO_0`) and locks its
    `BUILDLOCKPOINTS`.
  - **Builders.** A builder (a unit with `#WORK`) walks to the free `BUILDPOINT` nearest to
    it and faces the building. It adds one stage per `#WORK` cycle: `ceil((frames−2)/2)`
    ticks, 10 for the English peasant. Builders add up.
  - **Life** is `stage·LIFE/BUILDSTAGES`; damage taken during construction is overwritten.
    The animation is `#BUILDLO_k` with `k = 4·stage/BUILDSTAGES`.
  - **Completion.** At `BUILDSTAGES` the `BUILDLOCKPOINTS` are cleared, the `LOCKPOINTS` set,
    and the building works and houses.
  - **Repair.** On a finished, damaged building each cycle repairs 20 life, for free.
- **Gathering** (`gather.rs`; `TakeResource` 0x4d2e10, `TakeResLink` 0x4d2fb0).
  - **Sources** are `[SOURCES]` sprites, e.g. trees `Dnn WOOD 4 30`: 4 wood per work cycle
    within 30 px. Trees never run out, because their `[WORKTRANSFORM]` is empty.
  - **`Gather` command.** It picks the source type nearest the click (within 64 px). The
    source nearest to the click of that type is found in rings of 128 px cells
    (`FindResourceObject` 0x4d2200 searches from the clicked point, not from the peasant);
    later searches start from the last source.
  - **Work.** It faces the source (16 per cycle) and enters the work stance `c` of
    `TAKERESSTAGES RES a b c`, which plays `#ATTACK{c−1}`. Each cycle adds the source's
    amount: 13 ticks per cycle for the peasant's 28-frame chop.
  - **Carrying.** At `PORTION` (wood 100) it walks to the nearest finished `RESOURCEBASE`
    building and delivers: within 5 cells of its last born point, else within 6 of a
    `CONCENTRATOR` point. Wood counts 1:1 at the default factors (food ×(1010 mod 100)/10).
    Then it goes back.
- **Fields and carcasses** (`sprite.rs`). They are sprites of the `COMPLEX` and `MEAT` groups
  whose type changes (`World::sprites`). Each type has an `ObjCharacter` (loader 0x5011af)
  from the group's `.RSR`: `[SOURCES]` (source type, amount per cycle, work radius),
  `[WORKTRANSFORM] ID NEXT n` and `[TIMETRANSFORM] ID NEXT n` (`NONE`: removed; without an
  entry the type stays itself).
  - **Time** (`TimeReq::Handle` 0x4d4440, every tick after the units): `TimePassed += 2`;
    at the type's `TimeAmount` the sprite becomes the next type (with no work done) or
    disappears. So `TIMETRANSFORM 500` takes 250 ticks.
  - **Work** (`PerformWork` 0x4d2cb0): each cycle adds the old type's amount to the load
    and counts `WorkOver`; at `WorkAmount · amount / 100` cycles the sprite becomes the next
    type. `amount` is `FoodEff / 100 · 10` for food from fields (100 at the start; the
    `EFFECT_FOOD +2000` upgrades make fields last 3×), else 100.
  - **Fields** (`COMPLEX.RSR`): `KUK00 → … → KUK05` in 5 × 250 ticks, then 5 cycles of 3
    food (`#ATTACK3`, 28 ticks each for the peasant), then `KUD00 → … → KUD05` regrows and
    gives 5 × 3 more, then `KUB00` vanishes after one tick. There are three variants.
  - **Planting.** A `SPRITEOBJECT` type (`FIELD(EN)`, `EXSPRITES KUK00 3 64 4 64`) is placed
    by `CreateObject` (0x5155c5): `rando()`, the price check and another `rando()`, one
    more, then for the sprite `rando()`; the point snaps to the `[DISCRET]` diamond grid
    (0x514fea, 1 px by default); the 8 × 15 lock cells from `(cx − 4, cy − 3)` must be free
    and the heights 64 px left, right, above and below must differ from their mean by less
    than 40, 32, 32, 32; the type is `KUK00 + rando() % 3`; no other sprite may lie closer
    than its list radius (30) plus the other's (only deposits have one; 0x4d1aa0); then the
    price (`GOLD 1`) is paid.
  - **Mills** (`USAGE MELNICA`). A finished mill (0x5239ec) and the "plant fields" button
    (`PlantFields`, 0x4e0d88) call 0x51522f: on the diamond grid `(i + j)·step,
    (i − j)·step` for `i, j` in `−n..=n` (`n` 64, `step` 4 px) plus `rando()·spread >> 11`
    on each axis (`spread` 64 px), a field is planted where possible: about 200 around an
    English mill.
  - **Hunting.** When a unit's victim is dying and its `TRANSFORMTOMEAT` sprite is a source
    the unit can gather (0x51bf0b in `AttackObjLink`), the attack ends: the hunter walks to
    the body and takes 60 gathering orders (`TakeResource` order type 1, which keeps the
    order even when nothing is found yet and draws a random number each time). At the end
    of the death animation the body becomes the `MEAT` sprite (0x511f4b): `M00` gives 50
    meat per cycle for 10 cycles, then `M01` 250 more cycles; an untouched `M00` rots after
    3750 ticks. Meat is source type 8; bases take it with `RESOURCEBASE … #R8` and it counts
    as food.
- **Entering buildings.** A unit walks to the first `CONCENTRATOR` point and disappears inside
  (`Unit::inside_of`). Peasants count against `PEASANTABSORBER`, other units against
  `UNITABSORBER`. Units inside still eat and count. `Leave` puts them back at the born points.

## Fog of war

Recovered from `RevealUnit` (0x484f7b), the blur (0x483c2a) and its consumers. The fog is
**not part of the simulation**:

- There is one grid, for the local player only.
- No unit, combat, AI or pathing code reads it.
- It is not saved in savegames.
- It only darkens the screen and the minimap, mutes sounds and answers the script function
  `GetFogSpotVisibility`.

`acr_sim::fog::Fog` therefore lives beside the `World`, owned by the host, and is not
hashed.

**Grid.**
- `u16` per 128 px cell. The width is `0x86 << (shift − 1)`.
- Cell address: `((x + 64) >> 7) + 3`, clamped to `[1, (msx >> 2) + 3]`, where `msx` is the
  playable side in 32 px units.
- 0 is black. There is no "explored" state: areas fade back to black when no unit lights
  them.

**Reveal**, every tick: every living object of the viewer stamps 8000 into the cells of its
`VISION` pattern.
- The stamp position is `(x, y − 2·height)`: hills shift the spot north, with no range bonus.
- VISION 0 stamps the centre; 1 the four axis neighbours; 2 the four diagonals.
- From 3 up it stamps the axes at `±128·(v−1)` and the diagonals at `±128·(v−2)`.
- VISION 7 and 8 keep the original's misplaced diagonal point (`y+4`, `y+3`).
- Units without `VISION` use 0.
- Outside missions the original also stamps allies' units. ACR has no alliances yet.

**Blur**, every rendered frame: each cell becomes `(s >> 2) − (s >> 10)`, where `s` is the
sum of its four neighbours (centre excluded). This spreads the stamps and decays them by
1/256. Script light spots (`SetLightSpot`, 64 slots) are stamped first.

**Use:**
- Overlay shade `v ≥ 1500 → 5` (clear), `1500 − v ≤ 300 → 5 + (1500 − v)·153/300`, else 158
  (black) (`Fog::shade`).
- The minimap is black where `v ≤ 1300`.
- `GetFogSpotVisibility` returns `v ≥ 900` (`Fog::visible`).
- Enemy units are drawn normally; only the overlay hides them.

**Switches in the original:**
- the `F` hotkey and the cheats `qwe`/`viewall`;
- a skirmish "no fog" option (tens digit of the packed options);
- in multiplayer, a defeated player sees the whole map.

## Using the simulation

- **Setup.** `acr_sim::assets::SimAssets` collects the static data without I/O: the host
  feeds it files until it is ready. `World::from_map(&m3d, &shapes, catalog, formations)`
  then places the map's units.
- **Running.** `World::step(&commands)` runs one tick (`ORIGINAL_TICKS_PER_SECOND` = 10 for
  the original speed, paced by `FixedClock`). Commands are applied first, in order.
- **Drawing.** For each unit with `alive` set (corpses included), use:
  - `pos` (1/16 px; `pos.pixels()`) and `dir` (0 = east, 64 = south);
  - `anim.name()`, the `.MD` animation slot such as `#MOTION_L`, `#ATTACK1`, `#DEATH`,
    `#DEATHLIE1`, with `frame` its frame index.
- **Queries for scripts:**
  - `World::living()`, `units_of(player)`, `units_in_circle(x, y, r)`: all in id order.
  - `unit(id)`; `Unit::living()`, `Unit::target()`.
  - `World::spawn(kind, owner, pos, dir)`, `kill(id)` (with corpse), `remove(id)` (gone at
    once).
  - Unit types: `Catalog::find("EngMus(EN)")`.
  - Economy: `World::players[p]` (`resources`, `population`, `housing`), each building's
    `queue` / `inside` / `mine` / `stage`, `economy::unit_cost`, and `Unit::carried`. For
    the interface: `construction::site` (where a building would stand, or `None`),
    `construction::needs_work` and `economy::can_enter`. The
    commands are `Produce`, `Unproduce`, `Enter`, `Leave`, `Rally`, `Build`, `Repair`,
    `Gather`, `Upgrade`, `CancelUpgrade`, `Trade` and `PlantFields` (`World::market` holds the rates,
    `Market::quote` the price of a trade). `economy::producible(w, building)` lists what a
    building can produce now, `upgrade::available(w, building)` what it can research;
    `Unit::research` is the running research; `Player::enabled`, `Player::upgrades` and
    `tech::Tech` hold the tech tree; `World::adv(player, kind)` the upgraded
    characteristics.

  These calls change the state directly. Call them between ticks, in the same order on
  every machine.
- **Replays** (`acr_sim::replay::Replay`):
  - `record(tick, &commands)` stores the commands of each tick; empty ticks cost nothing.
  - `checkpoints` hold `(tick, World::state_hash())` pairs.
  - `encode()`/`decode()` use a compact binary format: `ACRR`, version, map path, random
    index, then varint-coded tick deltas and commands.
  - Playing back means calling `step(replay.commands_at(tick))` for each tick on the same
    map and data. `acr sim-check` records run A, decodes the replay, plays it back as run B
    and compares every checkpoint.
  - `World::state_hash()` is identical on every platform for identical inputs, so it also
    detects lockstep desyncs.
- **Savegames** (`crates/acr-game/src/savegame.rs`, #78) build on the same property. A save
  is not a memory dump; it holds the inputs needed to reach the state again:
  - the `GameSetup` (mission, or random map with its seed and players);
  - the player's commands per tick, as an embedded `Replay` (script orders are not
    stored: the scripts give them again);
  - every closed mission window (page, question, choice) as `(tick, answer)`. A question rolls its
    tick back (`Scripts::answer`), so the answer belongs to the tick that is re-run;
  - every input of the diplomacy panel (a DipServer request or a click on a tribe
    building) with the tick it came before: it changes the scripts' state (alliances,
    purchases) without being a command;
  - the saved tick and `World::state_hash()` there, the camera, the selection and the
    control groups, the name and the date.

  Format: `ACRS`, version, name, unix time, local date and time, setup, tick, hash (u64
  LE), then the `ACRR` replay blob, the dialog closes (tick deltas), from version 2 the
  diplomacy inputs (tick delta, 0 + request bytes or 1 + building index), camera,
  selection and groups; integers are LEB128 varints like the replays. The header comes first so that the
  load dialog lists files without decoding them. Native: `Save\<name>.sav` in the game
  folder (the original's `.sav` files start with `KASF` and are not listed); browser:
  `localStorage` key `acr-save:<name>`, base64.

  Loading starts the setup as a new game. Once world and scripts are there (the world does
  not step before the mission script and the AI DLLs are loaded), `FastForward` steps to
  the saved tick, 100 ms per frame behind the loading screen, without drawing or sounds:
  the recorded commands are applied at their ticks and each dialog that opens is closed as
  recorded. Then the hash is compared (a differing state is logged and shown for a few
  seconds), view, selection and groups come back, and recording continues from the saved
  log. Replaying is as fast as the simulation: 18 000 ticks (30 minutes of the first
  Columbus mission) take about 5 s.

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
  its `Rarr` push tables). Enemies in the way are left to `CheckMotionThroughEnemyAbility`.
- **Animations.**
  - Units walk only in stance 0: an attacker leaves its stance before walking. The engine
    walks pikemen in stance 1 with `#MOTION_L0`/`#MOTION_R0` at the same speed.
  - `#TRANS01`/`#TRANS10`, `BREAKANIMATION` and build animations are not modelled.
  - Units without `#MOTION_L` are treated as 1-frame cycles.
- **Economy:**
  - Stone sprites are not simulated (the shipped data has none). Field planting skips the
    territory permission (0x45f767, only active with territories set up) and the picture
    lock polygons of sprites (fields have none). `FindResourceObject` takes the nearest source
    of the first ring without `FindPlaceForPeasant`'s stand points and the `Surrounded`
    marks; the random number of a failed search is drawn. Peasants carry in
    stance 0, not with their carry walk (`#MOTION_L{b−1}`), and deliver at the entrance
    rather than walking inside along the born points.
  - Builders do not re-path around blocking units (`CheckBlocking`). Placement checks only
    the finished footprint against locks and units (0x513b2c and the permission check
    0x45f767 are not reproduced). Some of `CreateObject`'s random draws are not made.
  - `Produce` takes one building; the original command picks among the selected buildings
    (an idle one first, else the shortest queue).
  - Synthetic catalogs without nation files have no tech tree: everything is enabled and
    buildings produce anything.
  - Not reproduced yet: `RESSUBST` type changes, ports and ships; the AI
    difficulty factor on production time; the 10× cannon price lobby option.
  - Upgrades without effect here: `SPEED` (`Rate`, which no motion code reads), `WEAPON`,
    `PRECISE`, `GETRES`, `SINGLE_ATTPAUSE`, `FISHING`, `SHAR`, `FEAR`, `UNCONSTRUCT`. `ENABLE
    UPGRADES` does nothing (the engine indexes the type table with upgrade ids). The
    `STAGELOCKPOINTS` of building stages are not set.
  - Rally point on a building: the original hit-tests the building's picture in rings 0..3
    of 128 px cells (0x49ed8e); here a building with a lock cell under the point counts.
    The random search for a source spot is capped at 4096 tries (the engine would retry
    forever). `GotoFinePosition` is not called.
- **Combat:**
  - Shots start at the unit's position. The `SETACTIVEPOINT` muzzle offsets (and the
    sprite frame offsets) are not added.
  - Every `WEAPON` is treated like the musket: an instant trace with the attack's damage.
    Arrows, cannon balls and other slow projectiles (`ProcessExpl` 0x557fa0) are not
    simulated.
  - Not reproduced: the friendly-fire check before a shot and the search for a firing
    position (0x559a40, 0x51e359); buildings, terrain and 3D bars blocking shots
    (`PredictShot`); `BackSteps` when an enemy is too close.
  - Also missing: formation melee (`Bitva` 0x41ee89), morale and fear (0x601960,
    `MORALE.DAT`); garrisons, towers, artillery, ships; fire damage; healers; `SINGLE_ATTPAUSE` and
    `PersonalDelay`.
  - Shot damage falloff uses an integer `2^(−d/dec)` (16-step table, linear interpolation)
    instead of the x87 `exp`, so values can differ by 1.
  - All players use the human `SearchVictim` path. AI nations scan every 8th tick and use
    random cells in the original.
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
  - formation compilation, permutations and slot geometry;
  - combat data (attack slots, search radius, active frames), the damage falloff.
- **Combat tests** (`tests/combat.rs`):
  - two pikemen find each other and fight until one dies (7 damage per hit), and the corpse
    lies as `#DEATHLIE1`;
  - a musketeer fires its first shot 38–45 ticks after the order, reloads 550 frames and
    kills at 300 px;
  - a 20-unit battle gives identical hashes in two runs.
- **Upgrade tests** (`tests/upgrade.rs`): research is paid at the start, blocks the
  building and ends on the 10th tick for `#TIME 21`; damage and protection change for the
  researching player only; links enable the next level; cancelling refunds; `BUILD`,
  `LIFE`, `[PRIVATE]` levels and `UPSTAGE0` with a `[STAGEDEPENDENT]` unit.
- **Economy tests** (`tests/economy.rs`):
  - 10 peasants eat 62 food in 1000 ticks;
  - famine kills peasants but not `NO_HUNGRY` units;
  - a centre produces a peasant on the 10th tick after the order, paid when queued, and it
    walks out;
  - a third centre costs 4× (`COSTPERCENT 200`);
  - production waits for housing, and cancelling refunds;
  - 5 peasants in a gold mine earn 400 gold in 1000 ticks and can leave again;
  - two peasants build a storehouse: paid at placement, life follows the stages, the locks
    switch on completion, and it then houses 25;
  - a peasant chops 25 cycles and delivers 100 wood;
  - a centre produces only its list and only enabled types, pikemen once barracks are
    finished (`[ACCESSCONTROL]`), and barracks take no rally point;
  - a trade of 1000 wood gives 100 gold and moves both rates exactly as the formulas say;
    they drift back over time;
  - a produced unit ends at its rally point; one with the rally point on trees gets a
    gathering order.
- **Food tests** (`tests/food.rs`): a field ripens on the 250th tick, gives 5 × 3 food,
  regrows in 250 ticks, gives 15 more and vanishes; `PlantFields` plants a 7 × 7 diamond and
  pays one gold per field, and finds no room the second time; a hunter kills a deer, which
  becomes a carcass, and carries the meat home.
- **Fog test** (`tests/fog.rs`): a VISION 3 unit lights its cell and 200 px around but not
  1000 px away, and the light fades once the unit is gone.
- **Replay tests:** every command round-trips through the binary format; damaged data is
  rejected; a recorded 200-tick match with commands at three ticks replays to the recorded
  hash.
- **Savegame tests** (`acr-game`): a game with player commands, a question and a page,
  saved at several ticks (before, at and after the dialogs), loads to the saved hash, also
  when replayed in small steps; flipping the recorded answer changes the result. With
  `ACR_GAME_DIR`, the ignored `mission_save_loads_to_the_same_state` plays the first
  Columbus mission with its script (`ACR_SAVE_TICKS`, default 1500), saves and loads.
- **World tests** (`tests/world.rs`) on a synthetic flat map check:
  - exactly 5 px per tick for a step-40 unit;
  - 7 ticks of turning before walking west;
  - arrival and stop;
  - a detour around a wall;
  - a 20-unit group move ending in 20 distinct cells;
  - forming and moving a 15-man line;
  - identical hashes for identical command streams.
- **`acr sim-check`** on all 63 maps, 3000 ticks (5 minutes of game time), with combat:
  - Hashes are identical between runs at every checkpoint on every map (189 of 189).
  - 3672 units die in the fights where players' units meet.
  - 0 units outside the map.
  - 115 units in all share a cell with another. Most of these come from the enemy-contact
    rule leaving the walker's cell unreserved, as the original does. Before combat the count
    was 4 (units the map itself places overlapping).
  - 92.5 % of the 104 956 surviving units reach their group's target (≤ 320 px). The rest
    are fighting, queued in crowds of up to 6000 units (Saratoga), or have targets across
    water.
  - Speed: 179 (Saratoga, 6000 units) to 100 000+ ticks/s in a release build, against the
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
