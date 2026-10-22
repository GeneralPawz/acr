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
- **Commands** are plain data (`Move`, `MoveFacing` (a move ending in a direction: `GroupSend` with it), `Stop`, `Attack` (priority 0xFE), `AttackMove`, `Patrol`,
  `StandGround`, `HoldFire`, `FriendlyFire`, `MeleeOnly`, `Form`, `Face`, `Disband`, with the issuing player). They are applied in order at the start of a tick, and commands for units
  the player does not own are ignored. The command stream per tick is therefore a complete
  replay.
- **Iteration order:** per tick, the brigade orders of the nation whose city tick it is
  (`tmtmt & 7`) run first. Then units run player by player (0..7), each player's units in
  creation order, like `LongProcesses` (0x5112aa) walking the per-nation lists.
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
With a direction (a right-drag), each free unit gets a turn order after the move (0x41bfdb
with order type 2, link 0x41bd0d): it turns by `ROTATE` per tick until within `ROTATE` of
the direction (not for transports, `NEWSHEEPS` ships or buildings).

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

**Stance walks.** In a stance `s ≠ 0` a unit walks with `#MOTION_L{s−1}`/`#MOTION_R{s−1}`
(the normal walk if the type has none) and its step is `OneStepDX·2·RATE[s−1] >> 4`
(`RATE i v`, default 16; `SPEED` upgrades change it, 0x5310ce); the look-ahead still counts
the frames of `#MOTION_L`. Stance 0 is never scaled (an AC difference from Cossacks). Units
walk in a stance:
- loaded gatherers in their carry stance (`TAKERESSTAGES RES a b c`: `b`, 0x50af66): Indian
  peasants and hunters with `RATE … 8` carry at half speed;
- units chasing a target with an index that has a walk (`#MOTION_Lk`, pikemen);
- units on an attack-move that have `#MOTION_L0` and no `FASTUNIT` (`GroundState` 1,
  0x52bf0b);
- a cannon driving (stance 1).

### Ships

Ships (`MOTIONSTYLE SHEEPS`, handler 0x4d5c00; `NEWSHEEPS` for canoes, 0x4d5810) keep a speed
(OB+0xe4, 1/16 px per tick) and a velocity (`ship.rs`). `CalculateMotion2` (0x4d4540) runs
every tick:

- with a target farther than 64: speed `+6` while below `MaxV = 2·step·RATE₀/16`, else
  `MaxV` (it overshoots once by less than 6); the heading turns 2 per tick towards the
  target (snapped once closer than 4) and the velocity follows the heading, so a ship never
  stops to turn. `NEWSHEEPS` quantise the target direction to the sprite directions;
- without a target: velocity and speed lose a quarter per tick, zero at speed ≤ 8.

The ship occupies its `Lx × Lx` box of 16 px cells. A step tests the box at the next position
against the water lock map and the other units; when it is blocked,
`CalculateMotion2Limited` (0x4d4cf0) once tries the nearest free of the eight directions
(`TryDir` with the engine's direction tables, turning 4 per tick). Otherwise the ship stops
(`StopObject`, one `rando`) and after `StandTime > 15` pushes the ships in its way
(`TryToPushShipsBy` 0x4d56d0: speed 200 along their own heading; docked transports are not
pushed). A successful step (`MoveObject`) draws one `rando`. Group moves of ships route over
the `TOPW` areas, and `ApplyOffset` tests a 14 × 14 box for them (4 × 4 for land units).

**Transports** (`TRANSPORT n`, capacity `n`) have a docking point (`DstX/DstY`): unset at
creation, away (−1) while sailing, or the land point 256 px ahead of the centre once docked.
Every new move (`NewMonsterSendTo` 0x517045) first goes through `ParkWaterNewMonster`
(0x54aa9c): open water around the target is an ordinary move; otherwise the nearest free box
on the 40 rings around it becomes a docking order (`WaterNewMonsterSendToLink` 0x54ac93): sail
there until within 8 cells of a box touching land, then turn 2 per tick to the mean direction
of the blocked boxes 8 cells around (`GetBestDirection` 0x54a52f) and dock if a 4 × 4 land box
ahead is free.

- `GoToTransport` (0x54d78d, order 243; a formation member breaks up its formation): only
  after the transport docked once, with room left. The unit walks to the landing point, then
  straight over the water (unlimited) to the transport and hides inside within
  `StopDistance + 32 px`. A full transport sends it back to the shore.
- `LeaveTransport` (0x54e000; `LeaveAll` 0x54df93 one order per passenger): at a shore the
  transport docks there; after 3 ticks one passenger appears at its centre, walks straight to
  the 3/4 point towards the landing point and then to the landing point ± 16 px.
  `ClearUnlimitedLink` (0x527dec) moves a unit still on a locked cell to the nearest free
  9 × 9 land box (`FINDNEAREMPTY` 0x527d02).

**Broadsides** (`WaterAttackLink` 0x520ef8, ships with `MULTIWEAPON`): the attack index is
the last whose detection range holds the target. In range the ship turns the nearer side to
it (2 per tick) and, within 8, fires a random gun (`MULTIWEAPON` offsets rotated by the
heading, height `z`); `ZALP n` shots make one volley before the reload. Out of range it sails
closer unless it stands ground.

**Ports** (`PORT`): `FindPortPlace` (0x532cdb) puts the water point at the nearest free
21 × 21 water box on the square rings 5..44 around the port. A ship is only started with a
free 15 × 15 box there; otherwise idle ships nearby are sent 300..700 px away
(`ClearBirthPlace` 0x49e479). New ships appear at the point heading 96 and sail to the rally
point ± 1024 px.

**Fishing** (`FISHING speed amount`, `ProcessFishing` 0x54f1b0, `FishingLink` 0x54eb16):
every 51 ticks idle boats start fishing; the catch grows by `speed/128` per tick
(`FISHING` upgrades change both values, `speed` wraps at 256) while the boat sails between
random spots within 32 lock cells; a full boat brings the catch to the nearest own port's
water point and rests 60 ticks.

### Directions and sprites

The drawing code (0x5062a3) selects the sprite frame from `RealDir`:

- The GP frame index is `SpriteID·R + k`, with `R` the stored directions. Frames are
  **direction-interleaved**: frame f of direction k is at `f·R + k` (0x506f13; e.g.
  `EngD18G.gp` starts with the 9 directions of its first frame).
- **Odd `R` (9 for most units, 0x506cd5):** `n = 2R − 2` (16),
  `i = ((RealDir + 64 + 255/(2n)) & 255)·n >> 8`, i.e. `((RealDir + 71) & 255)·16 >> 8` for 9.
  - If `i < R − 1`: `k = R − 1 − i`, drawn mirrored (frame | 0x1000, placed at `x − dx`).
  - Otherwise: `k = i − (R − 1)`.
  - Stored k = 0 is south, 4 west, 8 north; the east half is the mirror image.
- **Even `R`:** `n = 2R`, `i = ((RealDir + 64) & 255)·n >> 8`; `k = R − 1 − i` mirrored below
  `R`, else `k = i − R`.
- **`FULLROTATION` (type flag 0x172d bit 3, the cannon):** no mirroring,
  `k = ((RealDir + 128/R − 64) mod 256)/(256/R)` (0x50663f).
- The direction is `RealDir` of the moment: walking, standing, attacking and dying units
  show the frame of their own animation in that direction, so a unit only turns when the
  simulation turns it (by `ROTATE` steps, or `Die`'s one random turn). A corpse keeps its
  direction.
- **Corpses fade:** once the death counter `Sdoxlo` passes 100 (0x506ed5, a few seconds after
  the fall) the frame goes to the draw list with options 2 (0x49c67e), which draws it with
  `ShowGPTransparentLayers` (0x48e180): half transparent until it is removed (see Death).
- The selection does not depend on the animation: aiming, firing and reloading
  (`#PATTACKk`, `#ATTACKk`, `#ATTACK3`, `#TRANSX3`) use the same 9 interleaved directions as
  walking, and a shooter faces its target (`AttackObjLink` turns it by `ROTATE` first and the
  strike frame sets `RealDir` again). The artists drew a musketeer firing west or north-west
  from behind, so a line firing at a target beside it shows mostly backs.
- ACR: `acr_render::scene::sprite_direction`; corpses use the transparent sprite mode.

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

1. The brigades of the nation whose city tick it is run their orders (keep positions,
   formation melee).
2. `SearchVictim` runs for every unit.
3. Player by player, each unit runs its order link (`AttackObjLink` for attack orders), then
   the motion handler. The motion handler also advances strike animations by 2 frames.
4. The object loop counts down reloads and plays death animations.

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

- **`SearchVictim`** (in the first pass of `LongProcesses`, after the fire damage). Human
  nations search every tick (peasants, `USAGE PEASANT`, only when `(index & 15) == (tmtmt &
  15)`); AI nations every 8th tick (`(index & 7) == (tmtmt & 7)`) except towers. A unit draws
  `rando()`, and anything above `0x2000` skips the search (about 3 ticks in 4). Otherwise it
  scans the `(2·(SR>>11)+1)²` cells of 128 px around the search centre.
  - Only cells where a nation outside its friend mask is present (`NPresence`) are looked at.
  - In each cell it takes the first living enemy, unit or building, whose material it can
    kill (guards only while they attack).
  - It keeps the nearest one to the centre with `MinR·16 < d < SR`.
  - Searchers with a square wider than 10 cells (forts: 3500 px) skip the 3-in-4 draw and
    try 6 random cells instead (two random numbers each, plus up to three discarded ones),
    after a 3 × 3 scan when their minimum range is below 140 px.
  - A garrisoned building with shot points takes one random enemy from each enemy cell of its
    square (a random number per such cell) and then a random one of those, every call.
  - The target must be visible: no building box higher than 18 px (`CheckVisibility`
    0x556af4, 32 px steps). It is attacked with priority 1.
  - Cannons (`WEAPONINSECTOR`) never search; they only fire canister on their own (see
    Artillery).
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
     - **Out of range:** walk to the enemy's cell, in the index's walking stance if the type
      has `#MOTION_Lk` (pikemen with the pike lowered), else in the ground stance.
- **Damage** (`MakeDamage`):
  - Start with `DAMAGE`, plus `min(kills·SKILLDAMAGEBONUS/10, 19·damage)` if the mask
    allows, minus `SHIELD`.
  - `R` weapons: `rando() < 1310` (4 %) kills outright, except buildings and `NO25` units.
  - Then subtract `PROTECTION[kind]`, kept as a byte.
  - The formation bonuses (shape header numbers 3 and 4) are scaled by the angle to the
    brigade's facing: full within 40, half below 70, none beyond; `#KARE` gets full.
  - At least 1. A surviving victim counter-attacks with priority 1.
- **Shots** (`Shoot` 0x51ddc3) leave from the muzzle: the `SETACTIVEPOINT` point of the
  current direction (9-direction sets mirrored for the east half) plus the `USERLC` offset
  of the frame's sprite pack, `y + 2·(point y + offset y + ZPOINTS source)`, at the ground
  height plus the source height. `SETACTIVEPOINT2` (canoes) fires two shots. The aim point
  (`GetDamagePoint` 0x52557e) is the target at its `ZPOINTS` aim height, or the centre of a
  random one of a building's `3DBARS` boxes (one more random number), then two unused
  random numbers; it is dispersed per axis by
  `((rando()>>5) − 512)·(Norma>>5)·RAZBROS/32000`, about ±57 px at 400 px. The attack's
  `WEAPON` then becomes a projectile (see Projectiles).
- **Musket balls** are instant line traces (`TraceObjectsInLine` 0x559310), because the
  musket's `ENDSHOT` flies 3000 px per tick:
  - Samples every 16 px run out to twice the distance, each 128 px cell checked once; a
    building box above the line stops the ball.
  - A unit qualifies if `0 < z − ground < 90`, it is more than 60 px from the shooter (150
    for friends), and the line passes within `UNITRADIUS` (doubled for friends within
    250 px). The qualifying unit closest to the shooter takes the hit; a building without a
    garrison absorbs it.
  - Damage halves every `DAMAGEDEC` px.
  - `FRIENDLYFIRE` means *avoid* friendly fire: a friend hit absorbs the ball unharmed (off
    with `SetUnitsState`'s fourth flag).
  - Random numbers per shot: 2 (aim point) + 2 (dispersion) + 1 (the smoke) + 1 (its child
    count) + 1 (the pick) + 1 (the ball), plus 1 for the `R` test.
- **Enemy contact while walking** (`CheckMotionThroughEnemyAbility` 0x41df98): if an enemy
  stands within 32 px in the 128 px cell at the end of the next cycle, the walker stops and
  attacks it (priority 0x8F). Its cell is left unreserved, as in the original.
- **Death:**
  - The unit turns by `(rando()>>8) − 64`, frees its cell and plays `#DEATH`, then lies as
    `#DEATHLIE1`. After 1200 / 1600 it switches to `#DEATHLIE2` / `#DEATHLIE3` where the
    type has them.
  - The death counter (`Unit::death`, `Sdoxlo`) grows by 1 per tick during `#DEATH` and by 2
    per tick while lying (0x512090, 0x5121f5).
  - Above 1200 a type without `#DEATHLIE2` loses its corpse (0x5120ab), about 61 s after the
    death at 10 ticks/s (most units have only `#DEATHLIE1`). Above 1600 the engine sets the
    `#DEATHLIE3` slot (an empty slot draws nothing; ACR removes such a corpse), and above
    2000 every corpse is removed (about 100 s).
  - Lying corpses are drawn half transparent ([Directions and sprites](#directions-and-sprites)).
  - Attackers keep striking a target whose counter is still ≤ 8.

### Friend and foe

Every object carries an alliance mask (`NMask`, OB+0x9a; `Unit::nmask`), taken from its
nation's mask (`World::nation_mask`, `1 << n` by default) when it is created. Two objects are
enemies when their masks share no bit. `AttackObj`, `SearchVictim`, enemy contact while
walking and the friendly-fire rules of shots all test the masks, not the owners. The script
functions `ChangeFriends` and `SetNMASK` set them; `acr-mission` hands its masks to the world
after every script tick.

### Priorities and unit states

Every order carries a priority; a unit's `PrioryLevel` (`Unit::prio`) is the priority of the
order it runs, 0 without one (0x511569). `AttackObj` (0x51a930) checks, in this order:

1. Priority 0xFE (the interface's attack order, 0x43ce14) becomes 0x90 and ends stand ground.
2. A member of a close-combat formation hands the attack to its formation (see below) unless
   the priority is 0x8F, the unit is artillery or the target a building. A member of a
   shooting formation (`FORMLIKESHOOTERS`) is ignored while the formation's order has a
   priority.
3. With `NoSearchVictim` an attack below 0x80 is accepted without effect (the caller sees
   success); one of 0x80 or more clears the flag.
4. An attack below the current priority is refused; below 5 the unit only switches to a
   strictly closer enemy.
5. The order keeps `prio & 0x7f`. A unit with `InPatrol` (OB+0x2a bit0, a move of priority
   below 5 among its orders) gets the attack *in front* of its orders, so it resumes the
   move after the kill.

Moves keep `prio & 0x7f` too. A player's move has 16, so `SearchVictim` (priority 1) and
counter-attacks (1) cannot interrupt it; only enemy contact (0x8F) can. An attack-move
(`SelSendAndKill`, `Command::AttackMove`, priority 0x80 → 0) sets `InPatrol`, so the unit
takes the enemies `SearchVictim` finds and continues afterwards. `Patrol` (0x4ea2c0, order
type 0x51, link 0x4ea3f0) alternates priority-0 moves between the unit's position and its
place at the target, each put in front of the patrol order. `DeleteLastOrder` (0x52582d)
recomputes `InPatrol` from the orders left.

The states (`Unit` fields; `states.rs`):

| state | field | effect |
|---|---|---|
| hold fire | `NoSearchVictim` (OB+0x25 bit1) | no `SearchVictim`; attacks below 0x80 have no effect |
| stand ground | OB+0x25 bit4 | `AttackObjLink` drops a target out of range (0x5204aa) instead of chasing it; group moves (0x4ac20d) end it; on a `#KARE` formation (`MakeStandGroundTemp` 0x4dfe94) every member clears its orders and turns away from the members' mean position |
| melee only | OB+0x2a bit2 | only attack index 0, except against buildings (0x51bdc6); only `ARMATTACK` types |
| no FF avoidance | OB+0x2a bit1 | `FRIENDLYFIRE` units shoot through friends |
| AI guard | OB+0xf2 bit2 | never searches targets; others' searches skip it unless it attacks (0x525b70) |

`AllowAttack(n, v)` is `SetSearchVictim(n, v ? 2 : 3)` (0x4e0330): free units get
`NoSearchVictim = !v` and stop attacking either way; formation members always get
`NoSearchVictim = 1` (an engine quirk). `SetUnitsState` sets the flags without stopping
attacks. `AUTO_NO_ATTACK` / `AUTO_STAND_GROUND` give new units the flags.

### Formation melee

A brigade has an order stack (Brigade+0x43): keep positions (0x41c0a9) or melee (`Bitva`
0x41ee89, link 0x41e50c). Brigade orders run only on the owner's city tick,
`tmtmt & 7 == owner` (`City::Process` 0x432582 → 0x434486; nation 7 never), and every
brigade of that nation draws two random numbers around its order link.

- **Start.** When `AttackObj` hands an attack to a close-combat formation without a
  brigade order, or with one while `AttEnm` is set (an attack-move) and the enemy is
  within 0x640, the brigade lists the enemy land units in the box of 128 px cells around
  its members, grown by 4 (`AddCell` 0x41e173: up to 512, each only once, skipping cells in
  no area or in areas more than 30 away by path).
- **Each city tick.** With `rando() < 0x400` the box is recomputed, with another
  `rando() < 0x400` dead enemies are removed from the list, then 64 random cells of the box
  (two random numbers each) are scanned again. Every member without a target attacks the
  nearest listed enemy beyond its minimum range, preferably one within its maximum range
  (an enemy walking away does not count as the nearest), with priority 0x88. Members leave
  their slots and fight on their own.
- **End.** When no member fights, the melee order goes; keep positions resumes if it is
  below.

### Morale

Recovered from `Morale.cpp` (0x600400..0x602600). `MORALE.DAT` (loader 0x600e90) sets the
factors; the shipped file only changes `FKF1` from 200 to 500.

- **Fear.** When a unit is killed (also by the 4 % instant kill), before `Die`, the killer's
  `FEARTYPE[attack index]` (0xFF: none) frightens the victim's surroundings
  (`Fear` 0x601960): the 128 px cells of Euclidean rings 0..3 around the victim, each unit
  within 256 px. Each one's courage (i8, OB+0x53) drops by one if it belongs to the
  victim's nation and rises by one otherwise (not for fear type 0). A unit of the victim's
  nation within its own `FEARRADIUS[type]` may panic (`TryPanic` 0x6017a0): with the chance
  `p` below, `p < 327` draws nothing, else
  `(rando() + dir + y + x + StandTime + seed + index) & 0x7fff < p` panics.
- **Chance** (`PanicP` 0x601400, x87 doubles, truncated; cached per unit slot and type for
  64 ticks as u16): `FEARFACTOR[type] · 32.769`, raised after a death by hunger
  (`+HF1·p/(dt² + HF2)`), lowered near a own `COMMANDCENTER`
  (`p/((CCA − 1)·exp(−(d/CCR0)²) + 1)` within `4·CCR0`), divided by `c + 1` for courage
  `c > 5`, multiplied by `1 − c` below −5, raised after a lost command centre
  (`+CF1·p/(dt² + CF2)`). In a formation it is multiplied by `OFN1`, `DRF1`, `FLF1` % for a
  missing officer, drummer or flag bearer, by `FKF1` % when more than `FP1` % of the slots
  are empty, by `FA1/(FA1 + kills)` for the formation's kills and by
  `100/(100·(n − 2)/FB1 + 100)` for its size; alone it is `A1/(A1 + kills)`.
- **Panic** (`StartPanic` 0x600e20) is an order (type 0x8e, priority 0xFF) on top of the
  unit's orders; the unit leaves its formation (a formation left with fewer than 5 breaks
  up). Each call of `PanicLink` (0x600720): the second leg spreads fear of type 0; with
  `rando() < PSIXOZ << 5` the unit calms down, before leg `STEPS` where it stands, later by
  walking to the nearest own command centre (two random numbers, y first); otherwise it
  runs a 128 px leg to the free spot of Euclidean ring 4 (lock cells × 2) with the least
  danger (`Danger` 0x600670: `1000/(d + 50)` per enemy within rings 0..6, `20000/(d + 50)`
  per enemy building with a garrison; one random number per spot), or straight on when
  nothing threatens. Legs run with `UnlimitedMotion`, during which the unit takes no orders.
- **Every 256 ticks** (0x601a20, when the counter before the step ends in 0x3d) courage
  drifts one step towards 0 and the command-centre table is rebuilt (up to 16 per nation).

The scripts' `DetectPanic` and `RemovePanicUnits` test `UnlimitedMotion` with a panic order.

### Projectiles

`WEAPON.NDS` (loader 0x60b390) defines the projectiles, with the frame counts of their
animations from `WEAPON.ADS` and the explosion media of the terrain textures from
`TEXTURES.LST` (`weapon.rs`). Up to 8192 objects fly at once (`projectile.rs`); every one is
simulated, smoke and debris too, because each takes a slot (which decides the processing
order) and draws random numbers.

- **Creation** (`Create3DAnmObjectEX` 0x558360): the first free slot from a ring pointer,
  one random number, the damage (a `FULLPARENT` weapon takes the shooter's `DAMAGE` of the
  attack index — for a garrisoned building `MinDamage..MaxDamage` by the shooters inside —
  others the weapon's own), gravity `−8000·G` (pixels << 14 per tick²). By propagation:
  `STAND`/`SLIGHTUP` stay at the start (`HotFrame 255`: explode at once); `RANDOM` draws three
  random velocities; `FLY` flies a straight or ballistic line in `t = dist/(2·speed)` ticks,
  or is a line trace when `t = 0`; `ANGLE` flies an arc at `speed` degrees (cannon balls,
  `T = 4·sqrt((e << 11)/az)`); `REFLECT` bounces off the ground with the parent's velocity
  mirrored at the terrain normal and `speed/64` restitution, or explodes if it hits steeper
  than about 24°.
- **Flight** (`ProcessExpl` 0x557fa0, once per tick after the units, slots in order): trail
  objects (`TILEWEAPON`), then `vz += az` and the move; aimed objects (`FLY`, `ANGLE`)
  explode within 16 px of their destination; contact with the ground or a building box
  (not the shooter's own) explodes them; `DAMAGEHEIGHT` objects (bouncing cannon balls)
  hurt every tick while that low; a `Times`-1 object explodes at its hot frame and dies at
  the end of its animation.
- **Explosion** (`ExplodeAnmObject` 0x5578f0): round damage (`MakeRoundDamage` 0x5253ae):
  every object in the 3 × 3 cells of 128 px within the weapon's radius plus its own
  `EXPLMEDIA` size (buildings 5 px) takes the full damage, friends included, plus the
  target object. The media is then the target's, or water where the four `SEA2` cells
  around are deep, or the texture's (`FORSTRELA`: flash); it is a global that only round
  damage resets, so objects without damage use the last explosion's media, as in the
  engine. The children of that media (`CUSTOMEXPLOSION`, else `CHILDWEAPON`) follow:
  `min + ((max − min)·rando() >> 15)` of them, each picked with a random number (a bounce is
  picked again up to 20 times after a building hit), and the `SYNCWEAPON`s.
- **Examples:** an arrow (`STRELA`, 50 px per tick, high gravity) hurts everything within
  15 px where it lands with the archer's damage; a cannon ball (`PPOINTT` → `NUCL2P`, 10°)
  deals 340 within 35 px, then bounces (`SKOKAT`: 500 within 16 px every tick while below
  95 px) or breaks a wall (`PUWALL`, 900 and debris of 100 each).

- **Sounds and drawing** (not part of the state): creating an object of a weapon with a
  `[SOUND]` group starts it at `(x, y/2 − z)` (0x5583a2), before the slot search, so a
  musket's `SHOTFOG` plays `RUGIE1`, a cannon's smoke `PUSVIS1`, a bounce `KERNEL1..4`,
  an explosion `VZRIV1`. `World::step` collects them in `World::sounds` (cleared at the
  start of each step); the game places them like other world sounds (0x586a48: hearing
  margin, fog). Every object in flight is drawn (0x557cb0, `acr-game` `ingame/effects.rs`):
  frame `frame` of its `WEAPON.ADS` animation (clamped to the last) at `(x, y/2 − z)` plus
  the `USERLC` offset, sorted at the ground row `y/2` plus the shot point's height (+300 for
  `[HILAYER]`). An animation of `n` directions keeps them together (sprite `f·n + d`); the
  direction is the angle of the screen velocity `(vx, vy − 2·vz)`, or of the way to the
  destination while the object stands, folded to 0..π (`d = (⌊a·(2n−2)/π⌋ + 1) / 2`) and
  mirrored when `vx > 0`. `WHITE` weapons (0x24) draw the older `RLC` packs through
  `AGEW_1W.GRD`, which lightens by the pixel's level (`PU3`, `WEXP`); GP packs ignore the
  table (0x4903a2). Animation sounds (`SOUND 1 #DFIRE2 FIRE1` in `WEAPON.ADS`) start when
  `frame/2` reaches the sound's frame (0x5295eb).

### Buildings that shoot

Towers, forts and Indian buildings shoot from their `EX_SHOTS`/`EX_SHOTS2` points
(AttackObjLink's building branch 0x51c30a) and only with a garrison.

- `AttackObj` accepts a finished building that does not research and, for a human nation,
  does not train units; a building not attacking yet gets priority 0x81 and the attack goes
  in front of its orders. Buildings hit back.
- Each tick, once its reload is over, the first free point whose range (`r1..r2` px from the
  point) and sector hold the target fires the weapon of its attack index — Indian buildings
  (`EVROSHOT 2`) only their index-2 points when the first unit inside is European
  (`EVROUNIT`), else the others. One shot per tick; then the building's reload is
  `ATTACK_PAUSE[0]·PersonalDelay/(inside·100)` (counted down 4 per tick) and the point's own
  `reload` starts (2 per tick). A unit target that no free point covers is dropped.
- Damage of index 0: `MinDamage + (MaxDamage − MinDamage)·n/capacity − Shield`, `n` the
  `STRELOK`/`HORSE-STRELOK`/`ARCHER` units inside (`MakeDamage` 0x4e8c43); the shot's own
  falloff is replaced. Buildings shoot through anything and do not disperse.
- A `B` weapon (muskets, arrows) hitting a building only hurts its garrison (0x4e8870):
  `rando()·101 >> 15 < PROMAX[attacker's PROMAXCTG]` misses, else a random unit inside takes
  the damage. A building's garrison dies with it.
- Building boxes (`3DBARS x y l w h`, `bars.rs`; `Add3DBar` 0x443697 at creation, removed at
  death): 45°-turned rectangles in 256 px cells, entered in the cells of nine sample points
  only, as in the engine. They stop projectiles and traces and hide units from searches.

### Artillery

The cannon (`ArtPus`, `USAGE PUSHKA`, `MOTIONSTYLE COMPLEXROTATE`; `artillery.rs`):

- **Motion** (0x50f7cb): at the end of a cycle a cannon facing its target within a 16th
  drives one cycle in stance 1 (`#MOTION_L0/R0`); otherwise it changes to stance 2 and turns
  16 per turn animation (`#MOTION_L1`: +16 at its end, `#MOTION_R1`: −16 at its start).
- **Attack:** the same 16th-steps while aiming (the engine calls `TryToStand` on the target
  there, a random number, kept); each shot of indices 4 and 5 costs 50 iron and 100 coal
  (`RASTRATA_NA_VISTREL2`, paid when the strike starts; without them the order ends). Index
  4 fires the cannon ball beyond 1500 px, index 5 canister below.
- **Canister** (`WEAPONINSECTOR a r1 r2 d1 d2 sector pct`, 0x52d6b8): after the shot every
  object in the touched cells of the sector, friend or foe, takes
  `d1 − (d − r1)·(d1 − d2)/(r2 − r1)`, scaled to `pct` % at the edge. A deployed cannon
  without orders fires a volley by itself (0x526349) when more than 3 enemies and more than
  twice as many enemies as friends are in the cells of its sector out to its search radius.
- **Attack on the ground** (`NewAttackPoint` 0x51b4aa, `Command::AttackGround`,
  `OBJ_AttackPoint`, `ArtAttackGroud`): the attack link with a stand-in target at the point;
  `ADDRANGE` % of the range and `ADDPAUSE` % of the reload (`NRES.DAT`: 200 and 200), a
  number of shots or until stopped.
- Cannons never hit back when hurt, skip the death turn and are never captured.

### Fire

`X` weapons (cannon balls) set the target burning at once, `F` weapons after `FIRELIMIT`
hits (buildings 15). A burning building loses `max(1, MaxLife >> 11)` per tick (no shield)
until it collapses or a repair brings it to full life; a burning unit loses 1 every 32
ticks while above a tenth of its life. Every building below half its life draws a random
number per tick for its smoke (0x511be2).

Fire and smoke are drawn at the `FIRES` and `SMOKE` points of a finished building
(0x50618e → 0x505ad5, picture coordinates from `LOCATION`) with the `[FIRES]` animations
of `WEAPON.NDS` (`FIRE`: `#DFIRE1` then `#DFIRE2` in a loop; `SMOKE`: two variants by
point). A burning building shows `n·(M − L)/M` of its `n` points (at least 4), a damaged
one `n·(T − L)/T` below `T = M/2 + M/4`; one more point catches fire every 8..23 ticks
(the first after 1..4), points beyond the count go out at once; they are sorted 400 rows
below the building plus the point's index. The state and its random numbers (C `rand()`)
are the drawing's, not the simulation's; the original only advances buildings it draws
(the game: those near the view). Not reproduced: the variant for buildings with stages
(`STAGE_FIRES`, 0x5052b5).

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
  - Depot limit (0x49f21b): a head order of a `MAXINDEPO g n` type is cancelled with a refund
    (`UnProduce`) while the owner has `n` · finished `ARTDEPO` objects or more of group `g`.
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
    building's upgrade count (`SingleUpgLevel`, OB+0xe9, 2 at creation 0x515ff8, so the
    first `[PRIVATE]` level is `#LEVEL 2`) to equal its `#LEVEL`; a stage upgrade
    (`[UPGRADESTAGE]`) needs its section not done (state ≠ 2: not built, being built or a
    ruin) (`PerformUpgrade` 0x54f25b). The price is paid at once. A
    nation-wide upgrade is then no longer offered (`Enabled`, `PermanentEnabled` off,
    `IsDoing` on). The player command (0x4dc386) tries every selected building.
  - **Offered** in the command panel (0x609600 in 0x609230): an enabled upgrade; a
    `[PRIVATE]` one only at its `#LEVEL`, a stage upgrade only while its section is not done
    (either suffices for one that is both); and only when the building's sections cover the
    `[STAGEDEPENDENT]` mask (`upgrade::available`).
  - **Research** is an order in front of the building's queue: production and mining wait.
    Progress starts at 1 and gains 2 per tick, so it takes `ceil((#TIME − 1) / 2)` ticks
    (`#TIME 750`: 375). A stage upgrade marks its section "being built" (1) on every tick
    (0x54f7b4) while the type declares that section.
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
    - `SPEED` the walking rates of the stances (`Rate[12]`, u8), `GETRES` the share of a
      load a base credits (`ResEff`, delivery `RAmount·ResEff/100` at the base, 0x4d3170);
    - `EFFECT_FOOD`/`EFFECT_WOOD`/`EFFECT_STONE` change the player's delivery factors (u16,
      no caps unlike Cossacks); `GEOLOGY` sets a flag;
    - on the researching building: `SINGLE_INSIDE` (`AddInside`, OB+0xa5),
      `SINGLE_ATTPAUSE` (`PersonalDelay`, u8 OB+0xa7: the building's reload percentage),
      `MOREFARMS_SINGLE` (OB+0xf3 = the type's housing changed by the value, added to the
      housing), `PERSONALLIFE` (life and maximum), `UPSTAGEn` (0x53268e: if the type has
      that `ADDSECTION`, the section becomes done; life and maximum become the type's life
      plus the third `ADDSECTION` number of every done section, so finishing one also
      repairs the building).
- **Building sections** (`ADDSECTION`, see `docs/formats/definitions.md`): five 3-bit
  states in OB+0xaa (`Unit::stages`, `upgrade::section_state`): 0 not built, 1 being built,
  2 done, 3 ruin. Objects of a type with sections start at 0x8000 (0x516150); bit 15 is no
  section's state (the life sum 0x53268e reads a sixth field there, which is 1, never 2).
  - **Life loss** (`MakeDamage` 0x4e93fe..0x4e9925, `combat::section_damage`): when such a
    building takes damage and lives on, a threshold starts at the type's life for the owner
    (`AdvCharacter` life) and, for sections 0..4 in order, while life ≤ threshold a section
    being built goes back to 0 (0x4e94b3) and a done one becomes a ruin (0x4e94f1); after
    each section that *was* done (a ruin just made included) the threshold grows by that
    section's life. So a section falls when life drops to the type's life plus the done
    sections before it. Life and maximum life are not changed: a ruin keeps its share of
    the maximum and builders repair up to it.
  - A section set back to 0 during its research is marked 1 again on the next research tick;
    the research is not cancelled.
  - A ruin offers its upgrade again; researching it (paying again) makes it done, and life
    becomes the full maximum. `[STAGEDEPENDENT]` products and upgrades need their sections
    done, so a ruin blocks them until rebuilt.
  - The same loop overwrites the building's upgrade count OB+0xe9 with `2 + section` while
    life is above the threshold, else 2 — effectively 2 after the last section, since life
    never exceeds the whole threshold (the type's life plus every done section).
  - TODO: a ruin's `STAGE_FIRES` explosions and fires (0x4e952f..0x4e98bd) and the
    `STAGELOCKPOINTS` of done sections (0x53281d; one file uses them) are not simulated.
  - Taking a building down (`UNCONSTRUCT` link, 0x4a0dc7) clears all sections.
- **Mines.**
  - A `PRODUCER` building binds to a deposit within 128 px: an `[INTERNAL_SOURCE]` sprite of
    a resource it can mine. Binding happens when it is placed and every 256 ticks.
  - Each tick it adds `(free + inside·per_peasant) << 3` to a u16 accumulator and pays out per
    1000. That is 0.08 per peasant per tick for `PRODUCER 1 GOLD 0 10`. No randomness;
    deposits never run out.
- **Construction** (`construction.rs`; `CreateObject` 0x5155c5, `BuildObj`/`BuildObjLink`
  0x523c98/0x524654, `NextStage` 0x523666).
  - **Placement.** The position snaps so that `RealX + 16a` and `RealY + 32b` fall on a 16 px
    corner. The site must lie on the map, the finished footprint must be free, no enemy
    object (nation 7 aside) may stand within `(BRadius + 15)·16 px` of the check-point
    centre (`CheckMonstersInArea` 0x5133bc: the player's own and allied units never block),
    and the price (with `LOWCOSTRADIUS`) must be affordable; it is paid at once.
  - **The site** starts at stage 0 with 2 life (`#BUILDLO_0`) and locks its
    `BUILDLOCKPOINTS`. A type with `BUILDLOCKPOINTS` then moves the units out (0x51374e):
    every unit within `(BRadius + 2)·16 px` of the check-point centre whose 3 × 3 lock cells
    around its cell are not all free jumps to the centre of the first cell on the rings
    0..59 with a free 3 × 3 box, dropping its destination and path (its orders stay).
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
  - **Planting.** A `SPRITEOBJECT` type (`FIELD(EN)` = `FIELDP`, `EXSPRITES PUK00 3 64 4 64`;
    other nations `KUK00`, the numbers are the same) is placed
    by `CreateObject` (0x5155c5): `rando()`, the price check and another `rando()`, one
    more, then for the sprite `rando()`; the point snaps to the `[DISCRET]` diamond grid
    (0x514fea, 1 px by default); the 8 × 15 lock cells from `(cx − 4, cy − 3)` must be free
    and the heights 64 px left, right, above and below must differ from their mean by less
    than 40, 32, 32, 32; the type is the first of the three + `rando() % 3`; no other sprite may lie closer
    than its list radius (30) plus the other's (only deposits have one; 0x4d1aa0); then the
    price (`GOLD 1`) is paid.
  - **Mills** (`USAGE MELNICA`). A finished mill (0x5239ec) and the "plant fields" button
    (`PlantFields`, 0x4e0d88) call 0x51522f: on the diamond grid `(i + j)·step,
    (i − j)·step` for `i, j` in `−n..=n` (`n` 64, `step` 4 px) plus `rando()·spread >> 11`
    on each axis (`spread` 64 px, so the jitter is 0..64 px, never negative), a field is
    planted where possible. `n` is the half-width, not a count: 16 641 attempts on a diamond
    of 512 px, two random numbers each plus those of `CreateObject` (0, 1, 4 or 5 by where it
    fails). Nothing caps the number: the 30 px spacing, the 8 × 15 free lock cells, trees,
    slope, the map edge and the gold limit it, which gives about 546 fields on empty flat
    ground (fewer around the mill's own locks).
    The city tick of nation `k` (on ticks with `tmtmt & 7 == k`, nations 0..6, AI or not,
    0x4a69b4) also calls `CreateFields(k)` (0x577ff4) with chance `0x800/32768`: every
    ready mill of the nation plants again the same way. A computer nation's city first
    draws one `rando` (0x43269e) and, if it has mills, with chance `0x1000/32768`
    (`HandleFields` 0x431ead) lets each of them plant 20 fields at random points of the
    diamond (`CreateFieldsSlow` 0x5153cd).
  - **Object switches** (network commands 0x61..0x65, executors 0x4e0718, 0x4dc8fa,
    0x4dca2a, 0x4e07f9, 0x4e08d2): `StopProduction` sets OB+0xf2 bit0 of `CANSTOPBUILD`
    buildings, and `ProduceObjLink` does nothing while it is set (0x49eec5); `StopFishing`
    sets the same bit of fishing boats (`Fishing` 0x54e606 starts nothing, `FishingLink`
    0x54eb1f drops its order); `StopSowing` sets bit3 of mills, which `CreateFields` skips
    (0x5780d2), and switching it off plants the player's fields at once; `Canister` clears
    a canister unit's orders and attacks the point 256 px ahead once (`NewAttackPoint`
    0x90, 1 shot); `LoadCanister` sets its stance to the canister attack index + 1.
  - **Gathering speeds** (City+0xeb2c..+0xeb44): wood, stone and food delivered count
    `load·ResEff/100` (before the nation factors; fishing boats count their food). On the
    city tick, once `GlobalTime` (2 per tick) is more than 1000 past the last update, the
    speeds become `amount·25/dt` and the amounts restart. `GetExtraction` returns the food
    and wood speeds, and for stone, iron, coal and gold the peasants inside mines of that
    resource.
  - **AI building placement** (`aiplace.rs`, `City::ProcessCreation` 0x432582). The city
    centre is the mean position of the nation's peasants on its first city tick
    (`FindCenter` 0x4407b6, 128 px cells) and never moves. Two planning maps of 128 px cells
    serve all nations: `InfoMap` (no locks, flat — the four corner heights within 20 of
    their mean, the engine sampling one corner twice —, free of sprites, more than 2 trees,
    more than 5 stones; `RenewInfoMap` 0x43f763 updates the 3 × 3 cells around each new
    building) and `CantBuild` (marks of failed places). Each AI nation also keeps an
    enemy map of 1024 px cells (1 enemy, 2 next to one), rebuilt when `tmtmt & 63 == 8k + 6`
    and older than 256 ticks.
    An accepted building gets an approximate cell (`FindApproximateBuildingPlace` 0x4306db):
    the centre if it passes, else the first cell on the rings that is inside the map, away
    from enemies and not more than 60 further along the path areas than the best so far, and
    passes the kind's test: buildings not marked (`CheckBuilding`, 40 rings), mills 3 × 3 free
    cells (25 rings), storehouses a cell next to a wood cell and 8 cells from others (60
    rings; AC tests wood for stone storehouses too), towers four free cells, 9 from other
    towers, on the side towards nation 0 (rings 6..40, threshold 14), ports deep water around
    (60 rings), mines the nearest free deposit of their resources within 156 64-px cells.
    Once per city tick the city tries the cell corner ± 128 px (two `rando`) with the full
    site test (`CheckCreationAbility` 0x513b2c) there and on 9 rings of 32 px around it:
    inside the map, no deposit or hole sprite within the radius `(BRadius + 15)·16 px` of
    the check-point centre (fields 240 px less), no enemy object within it, no overlap of the
    `BUILDBAR` rectangles (compared rotated by 45°), every `CHECKPOINT` with its column of 8
    and row of 6 lock cells free, and at most 50 height difference between the check points.
    The first spot passing is built (`CreateObject`, paid then). A failure marks the cell
    (mills `0x10`, storehouses `1`, ports `0x20`, others `8`) and searches a new cell; after
    6 tries the project is dropped unpaid. A placed mill marks a 13 × 13 diamond of cells for
    mills, storehouses and buildings.
  - **Mines** (`RESSUBST`, only the gold mine has it): a mine placed within 64 px of a
    deposit of another resource is created as that resource's mine — the type `+1` iron,
    `+2` coal, `+3` stone, `+4` food, `+5` wood, the order of every nation file
    (CreateObject 0x515b62); the first deposit near enough decides, and the gold mine's price
    stays paid.
  - **`UNCONSTRUCT`** (the Sioux centre, 0x4a0f76, link 0x4a0d47): the researching building
    drops its queue (no refund), lets its units out one per tick, then loses a stage per tick
    — its first tick reaches the last stage again and re-runs the completion (a mill would
    plant its fields once more) — and at stage 0 it is gone and its price is refunded.
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

`acr_sim::fog::Fog` therefore lives beside the `World` and is not hashed. In the game the
mission link (`MissionLink::update_fog`) owns it and brings it up to the world's tick once
per tick, before the scripts run, and hands it to them for `GetFogSpotVisibility`. Because
a script's answer can depend on it, the fog is updated exactly once per tick (not per
frame, not again while the game stands still), so a savegame's replay sees the same fog.

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
  (black) (`Fog::shade`): a row of `AGEW_1DR.GRD`, which leaves rows below 64 alone.
- The minimap is black where `v ≤ 1300`.
- `GetFogSpotVisibility` returns `v ≥ 900` (`Fog::visible`).
- Enemy units are drawn normally; only the overlay hides them.

**Switches in the original** (`0x775e18`, set at game start):
- the map editor's `F` key (0x4a424c, the editor's key handler) and the chat cheats
  `qwe` (off) and `viewall` (toggle);
- the replay rows `REP_FOG`/`REP_NOFOG` of the "M" drop-down;
- in multiplayer, a defeated player sees the whole map.

No game option writes it. How the map view draws the fog is in
[ui.md](ui.md#fog-of-war-0x485606-0x485a3c-0x485d2d).

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
  - `#TRANS01`/`#TRANS10`, `BREAKANIMATION` and build animations are not modelled.
  - Units without `#MOTION_L` are treated as 1-frame cycles.
- **Economy:**
  - Stone sprites are not simulated (the shipped data has none). Field planting skips the
    territory permission (0x45f767, only active with territories set up) and the picture
    lock polygons of sprites (fields have none). `FindResourceObject` takes the nearest source
    of the first ring without `FindPlaceForPeasant`'s stand points and the `Surrounded`
    marks; the random number of a failed search is drawn. Peasants deliver at the entrance
    rather than walking inside along the born points.
  - Builders do not re-path around blocking units (`CheckBlocking`). A player's placement
    checks the finished footprint's locks and enemy units (the rest of 0x513b2c and the
    permission check 0x45f767 are not reproduced; the AI uses the full site test).
    Units moved out by a new site also move their occupancy bits (the engine leaves them). Some of `CreateObject`'s random draws are not made.
  - `Produce` takes one building; the original command picks among the selected buildings
    (an idle one first, else the shortest queue).
  - Synthetic catalogs without nation files have no tech tree: everything is enabled and
    buildings produce anything.
  - AI placement: the planning maps are built at the first use (the engine at game start);
    `CheckBuildPossibility` (peace-time cordons) and `HandleConstructions` (re-sending
    builders without a collector) are not reproduced; the deposit test of mines uses the
    deposit list (radius of the site test instead of each sprite's work radius); the
    builder team ignores its minimum size.
  - Not reproduced yet: the AI landing process (`AssignDesant*`, `SetMaxInTransport` are
    stored only); fishing boats do not flee from enemy ships (`GetShipDanger`); the AI
    difficulty factor on production time; the 10× cannon price lobby option.
  - Upgrades without effect here: `SHAR` (`WEAPON` and
    `PRECISE` have none in the engine either). `ENABLE
    UPGRADES` does nothing (the engine indexes the type table with upgrade ids). The
    `STAGELOCKPOINTS` of building stages are not set.
  - Rally point on a building: the original hit-tests the building's picture in rings 0..3
    of 128 px cells (0x49ed8e); here a building with a lock cell under the point counts.
    The random search for a source spot is capped at 4096 tries (the engine would retry
    forever). `GotoFinePosition` is not called.
- **Combat:**
  - Not reproduced: the friendly-fire check before a shot and the search for a firing
    position (0x559a40, 0x51e359); terrain and buildings blocking a musket shot before it
    is fired (`PredictShot`; boxes do stop the ball in flight); `BackSteps` when an enemy is
    too close; the territory checks; the AI's random-cell search (`Zombi` units) and its
    artillery pickers; healers.
  - Ships: the water pre-path (`CreateWaterPrePath` 0x535b04) uses the land pre-path on the
    water lock map; `MoveObject` and docking use the order's cell centre rather than the
    clicked point; `EraseLodki` has no caller in AC.
  - Projectiles: the explosion's water test samples the `SEA2` depth with the rows shifted
    by the height (the engine reads a screen-space water map); the corpses thrown by a
    `DETONATOR` blast are not moved; `extra` (the shot point's visual height) is kept but
    unused, as in the engine.
  - Buildings: the cells of a building for round damage and searches are those of its
    position (the engine keeps building lists by cell); `SLOWDEATH`, capture and storming
    are not reproduced.
  - Attacks on objects ignore `nTimes` (attacks on the ground count their shots).
  - Morale: the fear scan takes the units of the 128 px buckets built at the start of the
    tick (the engine's cell lists change during the tick); the cell number in the panic
    seed uses the bucket grid's width. Fleeing units walk in stance 0 (the engine runs them
    in the stance-1 motion where the type has it). The panic order's removal from the
    selection is left to the interface.
  - Formation melee: the AI's army melee (0x43ba30) is not reproduced. Formations get no flag
    bearer, so its extra attack/defence bonus is never added. Keep positions
    counts a member in place when it reached its slot's cell (as far as a walk gets).
  - Shot damage falloff uses an integer `2^(−d/dec)` (16-step table, linear interpolation)
    instead of the x87 `exp`, so values can differ by 1.
- **Not reproduced yet:**
  - motion styles 1 and 4 (`SLOWROTATE`, `ROTATE&MOVE`);
  - `GotoFinePosition` (0x4a0670, at `StandTime == 48`);
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
- **Morale test** (`tests/morale.rs`): a kill with fear type 3 changes the courage of every
  unit around, some comrades of the victim panic, run more than 60 px away ignoring move
  orders, and all calm down again.
- **State tests** (`tests/states.rs`): a standing pikeman never leaves its place while an
  enemy stands 300 px away, and fights after a move; hold fire stops auto-attacks until the
  player's attack order; an attack-move kills the enemy on the way and goes on; a patrol
  reaches both ends; a formation of pikemen starts melee, several members fight, the enemies
  die and the melee ends. Allies do not fight (`tests/combat.rs`).
- **Projectile tests** (`tests/projectile.rs`): an arrow leaves the muzzle, flies about 8
  ticks for 400 px and lands with the archer's damage; a cannon ball's arc peaks at about
  D·tan 10°/4 and it bounces on; an empty tower does not shoot, one with a musketeer inside
  deals 26 per shot paced by the point's reload; a cannon turns towards a point in 16ths
  with its turn animations, bombards it and pays iron and coal; a deployed cannon fires
  canister into a crowd; projectiles are deterministic. A stance walk with `RATE 4 8` goes
  half as fast (`tests/world.rs`).
- **Push-out test** (`tests/pushout.rs`, #143): a centre placed over the player's own
  pikemen is accepted and the pikemen end up next to it, each with a free 3 × 3 box.
- **AI placement test** (`tests/aiplace.rs`): the centre is the peasants' mean, a house is
  placed near it, a second keeps its build bar clear of the first, one project at a time.
- **Switch tests** (`tests/switches.rs`): a stopped barracks keeps its queue and produces
  after resuming; the canister buttons set the stance and attack 256 px ahead.
- **Ship tests** (`tests/ships.rs`): a ship accelerates by 6 per tick (overshooting once)
  to `2·step` and arrives; a ferry ordered onto the shore docks with its land point on land,
  a soldier boards it, it sails along the shore, docks again and lets him off on the far
  shore. **City test** (`tests/city.rs`): the gathering speed is `amount·25/dt` after
  `GlobalTime` passes 1000.
- **Upgrade tests** (`tests/upgrade.rs`): research is paid at the start, blocks the
  building and ends on the 10th tick for `#TIME 21`; damage and protection change for the
  researching player only; links enable the next level; cancelling refunds; `BUILD`,
  `LIFE`, `[PRIVATE]` levels (from 2) and `UPSTAGE0` with a `[STAGEDEPENDENT]` unit; a
  two-section fort: section 1 offered only after section 0, damage resets a section in
  progress, ruins done ones at the documented thresholds without touching life or maximum,
  and a ruin is offered and rebuilt to full life.
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

- Motion styles 1 and 4 (`SLOWROTATE`, `ROTATE&MOVE`); the visibility test of the water
  pre-path (0x535b04).
- What runs the brigade order list each tick, and `0x41d090` at the end of `PlaceBrigade`.
- The exact `Rarr` push tables of `RemoveFools`, `ApplyOffset`, the 10-ring spiral table.
- The first move of a unit standing on a locked cell (0x538c4e) and the repulsion vector
  OB+0xd4/+0xd8 of the other motion variants.
- Whether `LeftLeg` starts at 0 for map units (the first cycle then plays `#MOTION_R`).
