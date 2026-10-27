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
  - Peace time (m3d `CAEP`) is in seconds and counts down in real time (0x4bf48b); ACR
    counts it in game seconds (see Deviations → Peace time). Its minutes (0x6e45d8) are
    for the scripts (`GetPeaceTimeLeft`).

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
- **Commands** are plain data (`Move`, `MoveFacing` (a move ending in a direction: `GroupSend` with it), `ScriptMove` (`GroupSend` with all its options, for scripts), `Stop`, `Attack` (priority 0xFE), `AttackMove`, `Patrol`,
  `StandGround`, `HoldFire`, `FriendlyFire`, `MeleeOnly`, `Form` (scripts: the units given),
  `FormNear` (the officer's icons: the units near it), `Reform`, `Fill`, `Face`, `Disband`,
  with the issuing player). They are applied in order at the start of a tick, and commands for units
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

**Water pre-path** (`CreateWaterPrePath` 0x535b04). `CreatePrePath` hands the search to it
for units on the water lock map (OB+0x54 = 1, ships) and for `MOTIONSTYLE COMPLEXROTATE`
units (NM+0x28e1 = 3, the cannons) on theirs (0x534056). It is the same search with these
differences:

- **Visibility.** After each step, a follower's cell counts as visible when the neighbour
  mask of its previous cell is open in the direction of the target (0x536845). No box is
  tested.
- **End of the detour.** It is the first cell of the winner's last visible run. If the
  winner is not visible when it crosses the line, it is the crossing cell. The +1 cell and
  the clamp to the start follow as on land.
- **Waypoints.** Every fourth cell becomes a waypoint (0x536f54), not every second.
- **Failure.** The cooldown is `64 + rando()&15` (0x537330).
- **No `g_searched`.** The flag is never set: `CreatePrePath` clears it before it hands
  over. So the `SendTo` push sets no 32..39 cooldown, and `CreateFullPath` draws no
  closing `rando()`.
- **Own footprint (OB+0x29 bit 3).** It is taken out of the lock map for the search. When
  the start box is locked, the early exit clears it a second time instead of putting it
  back. ACR keeps units out of the lock maps, so this has no effect here.

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

`GroupSend` (0x4ac776) around it, with the facing in `0x66aeb8` (0x200: none) and the
absolute-facing flag `0x11eaf5c`:

- The brigades' mean facing is the direction of the sum of their facing vectors / 4
  (0x4aca49, at most 32 brigades).
- The facing: the given one, or (0x4acc12) the direction from the centre of all units to
  the target, replaced by the brigades' mean facing when the two differ by less than 16
  (signed bytes subtracted as ints, not wrapped). Without a given facing the planner's vector
  stays target → group centre; with one it is `(cos, sin) << 4`. So every move, the
  interface's plain move too, ends with the free units turning to the travel direction.
- The target is shifted by the offset `(offX, offY)` along and across that vector
  (0x4accbd, `SGP_MoveToPoint`).
- Each brigade keeps its place relative to the brigades' centre, turned by `facing − mean
  facing` (offsets over 1000 px by `Norma` are cut to 256 px, 0x4ace5f), and faces its own
  facing turned the same way, or the facing itself with `0x11eaf5c` (`SelSendTo` type bit 4).
- Free units (`Send` 0x4ac182) by order type (`CreateOrder` 0x525752: 0 replaces the orders,
  1 goes in front, 2 is appended; only 0 and 1 set the unit's priority OB+0xef): type 0 and
  2 move and then get the turn appended, type 1 gets the turn in front and then the move in
  front of it, types 3..7 send no free unit (brigades still go). The turn (0x41bfdb, link
  0x41bd0d) turns by `ROTATE` per tick until within `ROTATE` of the direction (not for
  transports, `NEWSHEEPS` ships or buildings).

Callers: the interface's move (`Move`, `MoveFacing`), `SelSendTo` / `SelSendAndKill` through
0x4db0d3 (priority 0x10 / 0x80, facing always given, order type `type & 7`), and
`GroupMovePurpose` (0x5785a9: `SGP_MoveToPoint`, `SGP_MakeOneStepTo`; priority 0x90, or 0x81
with `flags & 1`; order type 0). Scripts send these as `ScriptMove`.

Deviations: brigade orders ignore the order type and keep ACR's priority handling (the
engine's 0x41db45 gets 0x8e / 0x80 and the type); the `0x11eaf5c` flag is reset for
`SGP_*` moves, while the engine keeps the value of the last `SelSendTo`; `GroupMovePurpose`'s
`AttEnm` mark for `flags & 1` (0x5786b0) is not set.

**The 32-formation cap.** `GroupSend` collects at most 32 brigades (0x4ac997: `cmp
[ebp-8], 0x20; jge` past the brigade loop). Members of the 33rd and later selected
formations are not free units either, so they get no order at all: select a large army
(e.g. "Das französische Heer") and send it, and every formation beyond the 32nd stays
put. ACR keeps this by default (`tests/group_move.rs`
`the_original_sends_only_32_formations`).

**Optional rule `group_move`** (`Rules::group_move`, settings `group_move=on|off`, by
default on with `controls=modern`; savegame rules bit 4; driver `start` option
`"group_move": true`). Off, nothing changes and no state is added. On:

- every selected formation is sent (no 32 cap);
- with two or more formations, each gets its own place around the target instead of its
  old place relative to the group (`motion::group_places`). Lines (frontage wider than
  depth, `Shape::extent`: 32 px per file and per rank) stand three deep, one behind the
  other with a 96 px gap (about 3 m), as many side by side (32 px apart) as needed; once a
  row would be wider than 2400 px the block grows deeper. Columns and squares stand side by
  side in a row behind the lines. Formations most to the front take the front row and keep
  their left-to-right order inside a row, so paths do not cross. The block is centred on
  the target and every formation faces the travel direction;
- the free units' grid (`Place`, unchanged) stands one gap behind the rear row instead of
  on the target, so they do not march into the formations;
- one formation (with or without free units) moves exactly as in the original.

No `rando()` is drawn on either path. Tests: `tests/group_move.rs`.

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
  after the transport docked once, with room left. The unit walks to the landing point's
  16 px cell until it is closer than `StopDistance` to the point, then straight over the
  water (unlimited) to the transport and hides inside once closer than
  `StopDistance + 32 px` (`GoToTransportLink` 0x54dbb0; both tests strict, as in the
  engine: a unit exactly `StopDistance` away walks on to the cell centre, which lies
  closer). A full transport sends it back to the shore.
- `LeaveTransport` (0x54e000; `LeaveAll` 0x54df93 one order per passenger): at a shore the
  transport docks there; after 3 ticks one passenger appears at its centre, walks straight to
  the 3/4 point towards the landing point and then to the landing point ± 16 px.
  `ClearUnlimitedLink` (0x527dec) moves a unit still on a locked cell to the nearest free
  9 × 9 land box (`FINDNEAREMPTY` 0x527d02).

**AI landing** (`desant.rs`; Brigade.cpp "DESANT", engine side of the AI DLLs'
`AssignDesantStartGroup` / `AssignDesantDestGroup` / `SetMaxInTransport`):

- **Islands** (`ResearchIslands` 0x41f701): the land areas (`TOP1`) connected by their links,
  numbered in area order; from an unnumbered area the links are followed at most 100 deep,
  then again from every area numbered so far. A nation's island (`CheckIsland` 0x41fd11,
  `City+0xeb15`) is the one with most of its objects (first maximum, 0xFF without objects):
  for every nation at `StartAI`/AI load, then one nation every 256 ticks (`tmtmt & 0xff == 1`,
  nation `tmtmt >> 8 & 7`); a computer player's known island stays during a mission.
- **Coast places** (`CreateCostPlaces` 0x423bcd, at map load): each 32 px cell 15..msx−15
  whose four `SEA2` depths are partly deep (> 128) gives the coast direction
  `(DX, DY) = −10·(Z0+Z2−Z1−Z3, Z0+Z1−Z2−Z3) / Norma` (outer loop x); it is a place when no
  place was taken within its 20 × 20 box, a 12 × 12 box at `2·cell + (DX, DY)` is open water,
  one at `2·cell − (DX, DY)` free land. That gives the water point and the land point
  (16 px cells), the land point's island and the free radius (6, grown while the box around
  the land point stays free; `CheckVLine` stops it at 12). Every tick one place
  (`tmtmt % NCost`, `ProcessCostPoints` 0x4243fd) is refreshed: accessible when its radius
  box is free land and at most 6 land units of nation 0 (no `CAPTURE` types) stand in the
  5 × 5 cells of 128 px around the land point; its transport is the first living transport
  of the first such cell around the water point that holds units (0xFFFF when that cell has
  none or all are empty).
- **Registration** (`RegisterNewUnit` 0x42dda3 from `CreateNewMonsterAt`): a new transport of
  a computer player gets a brigade with the diversion order (`MakeDiversion` 0x425654):
  load `SetMaxInTransport` (80 by default), phase 0. Its link (0x424957) runs on the owner's
  city tick with the brigade orders, only while the city has a start group:
  - a transport idle for more than 200 ticks away from a shore, or with `delay` in
    101..129, sinks (`Die`); a dead transport ends the brigade;
  - phase 0, not docked: sail to the water point of the nearest accessible place of its own
    island without another transport, more than 20 cells away (`FindCostPoint` 0x42469f);
    idle within 18 cells, to the land point (which docks it);
  - phase 0, docked: while it has fewer passengers than its load, the docked transport that
    loads (`City+0x185`, one per city) takes the last unit of the start group per call
    (cleaned first: alive or hidden), takes it out of its formation (`UnRegisterNewUnit`)
    and sends it to the landing point. Passengers that are idle (or gathering) are sent
    again: within 160 px to board (`GoToTransport`, priority 0x90), else to the landing
    point. With passengers aboard and nobody on the way for 256 ticks the load is complete
    (the units not aboard go back to the city). Full, or nobody on the way for 512 ticks, it
    sets off (phase 1);
  - phase 1, not docked: with chance 1/64 per call the place is forgotten; sail to the
    nearest place on the island of a nation 0..6 that is not allied (`FindCostPointEx`
    0x424789), within 40 cells to the land point;
  - phase 1, docked: everybody off (`LeaveAll`); a passenger on land (not hidden, not
    walking unlimited) joins the destination group. Empty, the transport is free again
    (phase 0, `delay` 350, so it sinks about 110 ticks later).

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

**Fleeing.** Every tick, `FishingLink` first checks `GetShipDanger` (0x4d5320) at the boat.
The danger counts every ship in the 128 px cells of rings 0..=4: any lock-type-water unit
except `USAGE FISHER`, of any nation, own ships included.

- Each ship adds `10000 / max(d, 80)` (0x4d51a0). `d` is the least distance from the point
  to the ship's position stepped 0..=10 times by four times its velocity.
- Any danger makes the boat look for a safer spot (`FindSafePlace` 0x54e679). It tries
  the eight points 256 px away in this order: east, west, south, north, NW, NE, SW, SE.
- A point qualifies when its 6 × 6 cell box is open water (`MFIELDS[1]`) and free of
  units (`UnitsField`).
- The boat takes the qualifying point with the least danger over rings 0..=6; the first
  wins a tie.
- It goes there by `NewMonsterSendTo` with priority 0x90 and order type 0, which replaces
  the fishing order. The idle boat starts fishing again with the next `ProcessFishing`.

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
- **Bonuses.** The header `#NAME SYMn n1 n2 n3 n4 [n5 n6]` (`%s%s%d%d%d%d%d%d`,
  0x4ee7cd; missing numbers are 0) stores `n1`..`n6` at shape+0x36, +0x38, +0x37, +0x39,
  +0x3a, +0x3b: attack and defence bonus **standing** (`n1`, `n3`), **normal** (`n2`, `n4`)
  and the flag bearer's extra (`n5`, `n6`, added while the brigade has one). `#LINE15
  19 2 9 3 1 2`: normally +2 attack, +3 defence; standing +19, +9; a flag bearer adds
  +1, +2. The members carry the current pair in OB+0x61 / +0x62 (the brigade in +0x1c /
  +0x1d); `MakeDamage` adds the sender's +0x61 for `O` weapons and subtracts the victim's
  +0x62 (see Combat).
- **`#KARE`.** shape+0x3d is set for every shape whose name does **not** contain `#KARE`
  (`strstr`, 0x4ee76b); only those scale the bonuses by the angle (0x4e9064).
- **Size variants.** A group (`LINE 3 0 1 2 5 #LINE15 … #LINE196`) lists the groups a
  formed brigade can turn into (here LINE, SQUARE, KARE; group `i` is also line `i` of
  `NRES.DAT [ORDERICONS]`) and the size variants: infantry 15/36/72/120/196, cavalry
  `#PRUS3`/`#PRUS2`/`#PRUS` etc. Every shape remembers the (last) group listing it
  (shape+0x3c, 0x4ed6c4).
- **Officers.** The `[OFFICERS]` section of the nation files gives, per officer type
  (`NM+0xa`): its drummer type (`NONE`: none needed), its flag bearer type (optional),
  then per formation the group, the size variants and the soldier types it takes:

  ```text
  EngOf17(EN) EngB17(EN) EngF17(EN)   3
  SQUARE 5 #SQUARE15 #SQUARE36 #SQUARE72 #SQUARE120 #SQUARE196 3 EngPik(EN) SpnS15(EN) EngMus(EN)
  EngKOf18(EN) NONE EngKF18(EN)    3
  PRUS   3 #PRUS3 #PRUS2 #PRUS       1 EngD18(EN)
  ```
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
- **Free units near a point** (0x4191a3 / 0x41901d, counted by 0x52c9d8): the 128 px
  cells ring by ring (`r/2048 + 1` rings of the ring table 0x28a6f10) around the point,
  in cell order; a unit counts if it is alive, not inside anything (OB+0x39), in no
  formation, of the nation and type asked and within `r` (`Norma`). Forming and filling
  use `r` = 0x4000 (1024 px) and take the first units found.
- **Forming** (`CreateBrigade` 0x4198b3; network command 0x3a, executor 0x4e174a): the
  officer must be alive and in no formation. It takes
  - exactly the shape's slot count of free soldiers of the type within 1024 px, else
    nothing happens;
  - a free unit of its drummer type within 1024 px, **required** unless the type is `NONE`;
  - a free flag bearer within 1024 px if there is one (optional; it adds `n5`, `n6`).

  The brigade takes the first slot of the owner's brigade list that is not enabled
  (`GetFreeBrigade` 0x42c845, `Brigade+0x20`); the interface shows the slot + 1 as the
  formation's number, so a dissolved formation's number goes to the next one (`Brigade::number`).
  Officer, drummer and flag bearer stand ground (OB+0x25 bit4); the soldiers do not.
  Everybody gets the normal bonuses. The brigade remembers the soldier type (+0x1a) and
  `FORMLIKESHOOTERS` (+0x2e bit3); it is placed centred on the officer facing 0
  (`PlaceBrigade` 0x41a7be with `Arrange`) and its members walk to their slots with
  priority 0x90 (`AddKeepPositions` 0x41c63e). (The engine's special case of a soldier
  type equal to the drummer type, which leaves the officer out, is not reproduced.)
- **Re-forming** (`ReformBrigade` 0x4e1cd3, command 0x41): the living soldiers take the
  first variant of the chosen group with room for all of them, in their order, placed at
  their mean position with the brigade's facing; they walk to their slots (0x90) and the
  formation stops standing ground (0x4e0ad7), which sets the new shape's normal bonuses.
- **Filling** (`FillBrigade` 0x4193a1, command 0x52): free soldiers of the brigade's type
  within 1024 px of its soldiers' mean position take the empty or dead slots with the
  brigade's bonuses (+0x1c/+0x1d); the formation stops standing ground (0x4e0c3f), is
  placed there anew with its facing, and its members walk to their slots (0x90).
- **Stand ground** (`MakeStandGroundTemp` 0x4dfe94, once per tick): unless a soldier
  stands already, every member stands with the normal bonuses and the stand-still
  countdown +0x22 starts at 50; a `#KARE` formation's members then clear their orders and
  face away from its centre. `ProcessBrigades` (0x434486) runs the countdown on the
  nation's city tick: without a brigade order and while fewer than 3 members have orders
  it drops by `FrmDec` (2); at 0 every member gets the **standing** bonuses (0x4dfd35).
  `CancelStandGround` (0x4e0c3f, or 0x4e0ad7 once per tick) ends standing for every member
  with the normal bonuses and stops the countdown; group moves do it for every formation
  they send (0x4ad0b5). So a formation earns `n1`/`n3` only by standing ground and
  staying put for 25 city ticks (200 ticks).
- **Grouping** (`grouping.rs`; `CM_DOGRP` / `CM_UNGRP`, network commands 0x58 / 0x59). A
  player's city keeps groups of formations (`GroupsSet` City+0xf63, `NGroupsInSet` +0xf67,
  `NGroups` +0xf6b); the interface selects a group's formations together (`ui.md`).
  `GroupFormations` (0x43d91d) takes the distinct formations of the selected units (alive,
  not hidden, at most 256, in selection order), removes each from its group and appends
  them as a new group; `UngroupFormations` (0x43d717) only removes them.
  `DeleteFromGroups` (0x43d592) removes an id from the first group that holds it and goes
  on with the next groups; a group that becomes empty is freed and the following groups
  move down, but their counts do not (a bug kept): each later group gets the count of the
  group before it (the emptied group's 0 for the first), and the search ends there.

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
  - With `hit_locations` on, a man on the ground (crawling or down) is a candidate only
    within 150 px of the searcher (`docs/melee.md` §1). A shooter without orders therefore
    leaves a crawler in range alone. In the mixed scene of #194 (unit 16, enemy 56 at
    490 px) this was the whole cause: the search ran as scheduled, the crawler lay in its
    square and passed every other test, and only the reach test dropped him. With the rule
    off he is taken like any other enemy, as in the original.
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
     - **Too close** (closer than the index's `r1`, 0x520689): the unit stands for 32 ticks
       (counter OB+0x5b), then backs away in up to 11 steps (counter OB+0x5a, one step per
       tick through 0x50e4b2). More than 16 px short it turns its back and walks off; closer,
       it either turns away or steps back still facing the enemy (0x50b6fe with the back
       flag), whichever needs less turning; a step into a taken cell stands instead. Both
       counters only change here. This is what frees a reloading musketeer pressed into a
       melee: the bayonet (`ATTACK_RADIUS 0 40 80`) and the musket (`250 850`) both need
       room. ACR plays the normal walk for the backward step (the original's back-step
       animation lists at type +0xac7/+0xc3b are not read).
- **Damage** (`MakeDamage`):
  - Start with `DAMAGE`, plus `min(kills·SKILLDAMAGEBONUS/10, 19·damage)` if the mask
    allows, minus `SHIELD`.
  - `R` weapons: `rando() < 1310` (4 %) kills outright, except buildings and `NO25` units.
  - Then subtract `PROTECTION[kind]`, kept as a byte.
  - The formation bonuses (the sender's OB+0x61 for `O` weapons, the victim's OB+0x62;
    see Formations) are scaled by the angle to the brigade's facing: full within 40, half
    below 70, none beyond; `#KARE` shapes get full.
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
- **Looking before shooting** (`PredictShot` 0x559030, `predict.rs`): a weapon is fired only
  along a free line, checked twice.
  - **What blocks.** The flight of the weapon (or of its first `FLY`/`IMMEDIATE`/`ANGLE`
    child or sync weapon, 0x558e20: the musket's smoke `SHOTFOGFORPISTOL` has the ball
    `ENDSHOT`) is followed from the shooter to the aim point in 16 px samples (0x558f30):
    a sample is blocked when the ground (lowered by 45 px where the building box is below
    20 px) plus the box there rises above the line (0x558eb0); a box blocks as its
    building, the ground as 0xFFFF. Only straight shots are followed: a `FLY` weapon that
    arrives in the first tick (the musket ball) and `IMMEDIATE` ones; an `ANGLE` arc only
    reports a height it cannot reach (0xFFFE); arrows, cannon balls in flight and the rest
    are never blocked. Units do not block here. A shot whose first step covers less than
    200 px (`|dx| + |dy| + |dz|`, the global 0x28a8018) is *close*.
  - **Friends.** In the engine, before the strike starts, a `FRIENDLYFIRE` unit whose
    avoidance is on (`Command::FriendlyFire` switches it off) first looks for the nearest
    unit on the line out to twice the distance, as the ball's trace does but counting only
    ¾ of the distance from the line (0x559770); a friend there blocks. **Deviation:** ACR
    skips this look (`predict::predict_shot_ff` is kept but not called). With it, musketeers
    standing behind their own pikemen dropped their attacks every few ticks and never fired
    (Die Armada: about 6000 dropped attacks per 1500 ticks, half the shots), which players
    of the original do not see. Their balls now fly and a friend in the way absorbs them
    unharmed while avoidance is on (`combat::trace_line`); switched off, friends are hit.
  - **When the strike would start** (0x51f9e9, after turning, before the shot's cost): from
    the unit's position at its `ZPOINTS` source height to `GetDamagePoint` (two random
    numbers, now also for shots that go). The strike starts on a free line, if the target
    itself is in the way, or if an enemy is in the way of a close shot. A unit standing in
    a one-frame pose (frame 0, the musketeer's `#PSTAND1`) only shoots past an enemy
    building with a garrison; otherwise it shows its stance's `#PSTAND` and drops the
    attack (an idle unit then finds targets again by itself and drops them again).
  - **At the shot** (0x51e359, per muzzle set, after the reload was set): from the muzzle,
    with the terrain and boxes only; the same exceptions.
  - **Blocked** (0x51fce6 / 0x51e499): a unit standing ground drops the attack. Otherwise it
    looks for a free line from up to ten 20 px steps to its left and right of the line
    (left first; a side ends at a lock cell; terrain and boxes only), walks one step beyond
    the first one (`NewMonsterSendTo`, priority 0x90, in front of the attack) and attacks
    again from there; without one it drops the attack, and a unit that was not reloading
    gets a reload of 20 frames (before the strike) or at most 16 (at the shot). The engine
    also shows the local player a hint ("cannot shoot", with the obstacle's name); ACR's
    interface does not.
- **A number of shots** (`nTimes`, `AttackObj`'s last argument at order+0xf, `Order::Attack`
  `times`; also the shots of an attack on the ground): the link reads the count once
  (0x51c2dc); after every muzzle set's shot (0x51ea72, after the sector weapon) or melee hit
  (0x51ec30, after the counter-attack) the order counts one down, and when the count read
  was 1 the attack ends, even if the target lives. 0 means until the target is dead. With
  two muzzle sets a count of 2 or more is counted twice per volley and can pass 0, which
  then means no limit (kept). A building's shot counts the same way (0x51cc94); its next
  order runs at once. A ship's gun (0x521edd) ends the attack after its last shot, and a
  counted attack never sails after a target out of range (0x521f34). The interface and the
  replays give 0; the scripts give `OBJ_AttackObj`'s count, `Attack3` and `AttackByTomahawks`
  1 per order, `ArtAttackObject` and `MakeOneShotToBuild` 1, `AttackBuildingsInZone` 10. The
  AI's own callers (the ship search 0x52688d, the city 0x4215bb, the army 0x43ce96) are not
  ported.
- **Enemy contact while walking** (`CheckMotionThroughEnemyAbility` 0x41df98): if an enemy
  stands within 32 px in the 128 px cell at the end of the next cycle, the walker stops and
  attacks it (priority 0x8F). **Deviation:** the original leaves the walker's cell
  unreserved here, so friends walk into the cells of units that stopped to fight and melee
  crowds pile up (in *Die Armada* up to 25 pikemen in one 32 px square). ACR reserves the
  cell again: at most 8 in that square, units on top of each other (closer than 6 px) drop
  from about 300 to 170 of 2100 (#195). The rest comes from the occupancy being one bit per
  cell: when one of two overlapping units leaves, it frees the cell for the other too.
- **Death:**
  - The unit turns by `(rando()>>8) − 64` (buildings and artillery do not and draw no
    number for it, 0x4e78e1), frees its cell and plays `#DEATH`, then lies as
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
  - **A destroyed building explodes** (end of `Die`, 0x4e84c7..0x4e8678,
    `combat::destroyed_building_explodes`): after its garrison died, unless its `delay`
    (OB+0x8c) is 1500 or more (the player's destroy order sets 6000, 0x4dcbb4; ACR has no
    such order) or it was not built half way (`stage < BUILDSTAGES / 2`), every
    `CHECKPOINTS` cell (NM+0x170a.., bytes, relative to the lock base cell 0x516fde) draws
    two `rando()`: the first picks one of the `DESTRUCT` weapons (`r·n >> 15`), the second
    below the `DESTRUCT` probability creates it (0x558d60, no sender) at the cell, 16 px
    above the ground, towards `(x, y, 1000)`. Types without `DESTRUCT` draw nothing; 105
    loaded building types have both lists. Wall pieces (OB+0x29 bit 0) instead take the
    pieces in the 3 × 3 wall cells around them along, each with 10 explosions at random
    offsets up to 32 px (0x4e7efb..0x4e8247); the shipped data has no wall types, so this
    is not reproduced.

### Capture

Objects that can be captured (`CAPTURE`, NM+0x172a bit1: the European peasants, the Dutch
and Portuguese buildings, the Spanish mines; and every building site nobody works on)
change sides when an enemy is near and no friend (`capture.rs`):

- **When** (last loop of `LongProcesses` 0x51175d, after the units, before the object
  loop): every object not dying nor inside whose index `& 31` equals `tmtmt & 31`, if it is
  of a `CAPTURE` type or a building site (not `Ready`) without orders.
- **Capturers** (`TryCapture` 0x528868): the 5 × 5 cells of 128 px around it, walked as a
  linear index without wrapping at the row ends. In a cell where a nation outside its
  friends is present, the first unit that is an enemy, not `CAPTURE`, not `CANTCAPTURE`
  (animals) and not of nation 7 (0x528072) counts if it stands within 250 px (`Norma`);
  the last one counted takes the object. No random numbers.
- **Defence:** a non-peasant object's reload becomes 100 (towers stop shooting while enemies
  are close). In the 7 × 7 cells the first friend (not `CAPTURE`, not dying, 0x52817b) of
  each cell where a friend is present defends, at any distance. An AI's artillery piece
  blows itself up when outnumbered (1 defender and at least 1 capturer; 2 and more than 3;
  3 and more than 7; 4 and more than 10).
- **Change of sides** (0x528d96..0x5294ad), undefended: a peasant captured by an AI dies
  instead if its own nation plays at difficulty 3 or more (`DestructBuilding`). Otherwise a
  `USAGE PEASANT` loses its orders, the object leaves its formation, takes the new nation
  and its mask, and a building keeps its share of construction (`stage · new BUILDSTAGES /
  old`). Its serial number's bit 0 flips (`World::captured`; scripts' groups lose it). The
  garrison changes sides with it and loses its orders; with a capture rule other than 0
  the building is destroyed instead.
- **The capture rule** (0x6e45cb, digit 5 of the random map's options, 0 in missions;
  `World::capture_rule`): 1 spares peasants, 2 also town centres and mines, 3 everything
  but artillery.
- **Walking to capture** (`AttackObjLink` 0x51f3df): a unit with no attack that fits its
  target walks there only if the target could be captured now (0x52853e: capturable, not
  spared, no defender in the 7 × 7 cells); otherwise the attack just ends.
- Not reproduced: the score (`VES·5` from the old to the new owner: the interface's
  statistics follow the owner), the hints and alarms of the local player, the nation's
  per-type counters (0x11041bd), OB+0x21/+0x22 and the `Zombi` bit, a unit walking into a
  building (OB+0x29 bit 1) that dies instead, `AutoKill` and wall objects (destroyed first),
  the territory checks (0x45f65c, only with territories), and the search rules that make
  units go after undefended `CAPTURE` units they cannot hurt (0x525ac1 mode 2, `AttackObj`
  0x51ab51). The interface does not offer the capture rule.

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
| stand ground | OB+0x25 bit4 | `AttackObjLink` drops a target out of range (0x5204aa) instead of chasing it; group moves (0x4ac20d) end it; a formation member switches its formation with its bonuses (`MakeStandGroundTemp` 0x4dfe94, see Formations); on a `#KARE` formation every member then clears its orders and turns away from the members' mean position |
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
- **Thrown corpses** (`[DETONATOR] w f`: `SKOKAT3` and `SHIPIT`, 8000): while such a weapon
  deals its damage (its explosion until the children are made, 0x557909..0x557c6e; only the
  target's damage of a low `DAMAGEHEIGHT` object, 0x558212; the damage of a hitscan on its
  target building, from its starting point, 0x558b09) the blast point and the force are
  set (globals 0x671314..0x67131c). A land unit (not a building) that dies then (end of
  `Die`, 0x4e86b9 → 0x55a070) flies away from the point: at distance `r` and `c = r + 20`,
  `vz = 20·f/c`, `vx = (20·f·dx/r)/c`, `vy` likewise (1/256 px per tick), starting 1 px
  above the ground; at most 128 at once, none at the point itself (0x559e30). At the start of
  every `ProcessExpl` (0x559f20) each moves, then `vz −= 1000`; the corpse is put there and
  lands when the ground reaches its height (`World::thrown`, hashed while not empty). No
  random numbers, the cells and lock maps are not touched. A 100 px throw of 8000 carries a
  corpse 21 px in 4 ticks, 7 px high; the drawing does not show the height.
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

- **Crowd sounds** (`SOUNDLIST.TXT` `INGROUP n` / `FORGROUP n min max`; `acr-game`
  `crowd_sounds.rs`): a placed sound of an `INGROUP n` group counts towards group `n` each
  time it is asked to play in earshot (0x584f34, called from the placed play 0x5864ff even
  when every variant is busy). The counts run in windows of 11 game frames (0x585347, once
  per frame from the main loop 0x4a6287: the counter 0x28b8950 rolls the table +0x1b84 into
  +0x1784 when it passes 10). While the last window counted more than `min`, each further
  sound of the group starts one of its `FORGROUP n` sounds (two random tries for one not
  yet playing; 0x585058) as a silent loop (-10000, `DSBPLAY_LOOPING`, 0x584ea4) at the
  sound's x, or pulls the playing one's x towards it by 1/16 (0x58513d), so it sits where
  most of the group is heard. Each frame a crowd sound is panned like any placed sound
  (`2 (x − centre)`, ±4000; 0x58526a) and its volume (hundredths of a dB) moves by
  `max(|v| / 5, 10)`: up from at most -8000 to 0 while the group's count is above `min`,
  else down; at -8000 or below it stops (0x5853f0..0x5854ad). `max` has no effect. The
  shipped list makes five crowds: walking Indians (`PUSTIHKAIND`, group 4; 19 `.MD`, e.g.
  `IROLUT.MD` `SOUND 1 #MOTION_L0 PUSTIHKAIND`) above 75 bring war cries (`PUSTIHA`,
  `Sound\new\indscrm1.wav`), walking and galloping horses (`PUSTIHKAKON` 3, `LOBEG` 2)
  above 75 a herd (`HORSIDUT` `horses.wav`, `HORSVINA` `horsbeg5.wav`), marching European
  infantry (`PUSTIHKA` 5, 14 `.MD` such as `ENGM18.MD`) above 254 and death cries (`DEASE`
  1) above 5 an infantry crowd (`DEASEB`, `DEASEA`: `Sound\new\pehota.wav`). Each
  crowd sound plays at most once, so any number of marching men adds at most one looping
  voice per group (four samples). Drummers' `BARABAN` (`SOUND 1 #MOTION_L BARABAN`) and
  officers' `ORDER_SOUND OFICERCOM` are plain unit sounds and already played. ACR runs a
  frame per tick, as for the ambience. Measured on the blank field (render VM, steps in
  `ACR-analysis/march-shouts/`): 20 marching Iroquois (`IroLut`) stay below 75; 40 make 89
  a window and start the war cries; 120 Iroquois and 1000 `EngM18` make 360 and 1216, one
  voice each; when they halt the counts drop and each crowd fades out and stops 33 frames
  later.

- **Hit and pain sounds** (ACR only, #243; `acr-game` `hit_sounds.rs`): the hits in
  `World::hits` after the last one heard play an impact where the victim stands (placed like
  other world sounds): musket balls, arrows, thrown weapons and grapeshot `ACR_HIT_BODY`,
  cannonballs and shells `ACR_HIT_HEAVY`, any projectile on a building `ACR_HIT_WOOD`; a
  soldier who survives a hit (also in melee) cries out (`ACR_PAIN`). Kills keep their
  `#DEATH` sounds. Rounds that strike the ground (`World::impacts`, #226: balls that missed,
  landing cannonballs, bursting shells) thud where they land: cannonballs, shells and
  mortars `ACR_GROUND_HEAVY`, the rest `ACR_GROUND_SOFT` (at most 2 per tick, own cursor).
  Per tick only the 3 impacts and 1 scream nearest the view centre play,
  and a group's variants that still play are skipped. The samples are CC0 files in the
  repository's `assets/sounds` (sources in its `CREDITS.txt`), read as the game files
  `ACR\Sounds\<file>`: in the game folder (natively and in the browser; `cargo xtask dist
  --install` copies them there), natively also from `ACR_SOUNDS_DIR`, else `ACR\Sounds`
  next to the executable, else in debug builds from `assets/sounds` itself. Missing files
  only leave the hits silent.
- **Animal voices** (ACR only; `acr-game` `animal_sounds.rs`): the original's animal types
  (`ANM*.MD`) have no `SOUND` lines and `SOUNDLIST.TXT` no animal groups, so its animals
  were silent. ACR's pain cry above is a man's voice and is never played for an animal
  (`NIKAKIXMAM` or an `ANM*` type); neither are the agony and panic voices below. Each
  species has its own groups instead: when it starts a strike (`#ATTACK`) it growls, when a
  hit wounds it it cries out in the pain cry's place (same 1-per-tick limit), and when a hit
  kills it it roars or bellows: bears `ACR_BEAR_GROWL`/`ACR_BEAR_ROAR`, the black bear
  (`ANMMEDSM`) its own higher `ACR_BLACK_BEAR_*`, alligators `ACR_GATOR_HISS`/`_BELLOW`,
  bison `ACR_BISON_GRUNT`/`_BELLOW`, the elk `ACR_ELK_BUGLE`, the hind `ACR_HIND_CALL`
  and the jaguar the game's unused `Sound\new\Hepard1-2.wav` (`ACR_JAGUAR_SNARL`). The
  samples are public-domain recordings (NPS) in `assets/sounds`. An animal type without a
  voice (the eagle) stays silent.
- **Agony, panic and din** (ACR only, #248; `acr-game` `agony.rs`), from the same files:
  - A share of the soldiers killed by a hit (not animals, `NIKAKIXMAM`) lies dying where he
    fell: a scream at the fall (`ACR_AGONY_SCREAM`), then moans and gasps (`ACR_AGONY_MOAN`,
    `ACR_AGONY_GASP`) every 1.2–2.8 s, fading from 85 % to 25 % of the placed volume. Who
    and how long is fixed by the unit index and the cause: gunshots 25 % for 3–8 s, arrows
    and thrown weapons 40 % for 3–10 s, melee 45 % for 4–10 s, shells 35 % for 4–10 s,
    grapeshot 55 % for 5–12 s; other deaths are silent — a man struck dead by a cannonball
    is torn apart and makes no sound (who survives one screams in pain like any wounded). Only the 4 agonies
    nearest the view centre (among those in hearing) make sounds; the others wait silently
    without a backlog, and one that was never heard at the fall does not scream later.
  - Units starting a flight leg in panic (`World::shouts`, the panic shouts of 0x600bc8)
    shout in fear (`ACR_FEAR`): one shout per brigade (or loose unit) every 4 s and one
    in all every 1.2 s, the one nearest the view centre.
  - Din: with at least 6 kills in hearing within the last 15 s, a quiet scream or fearful
    shout from beyond the view's left or right edge (volume 0.15–0.35, pan 0.5–0.9), every
    `24 s / kills` (1–4 s) plus up to 0.7 s.
  - Badly wounded survivors (hit-locations rule, #253): a living man whose life is not full
    and whose body scores at least 4 (`agony::severity`: a limb off 3, an open break 3, a
    closed one 2, a joint out 1, a flesh wound 1 up to 3 a limb, a head or chest wound 2,
    the belly open 6, bleeding out from a vital hit 4, the spine 4, a lung 3, bleeding
    fragment wounds 1) moans or gasps where he is every 5–10 s at 42–60 % of the placed
    volume (louder the worse). He falls silent when healed (full life, `HEALING`), dead
    (a death may start his agony above) or out of hearing. A man in his agony or crying
    from a fragment wound is not heard twice.
  - Burning men (`Unit::burning`: shell fires, `X`/`F` weapons; #253) scream with the
    agony screams (no fire samples are approved) every 1.2–2.2 s at full volume.
  - Caps: only the 2 nearest badly wounded and the 2 nearest burning men in hearing are
    voiced (the burning first), their sounds in all at most once every 1.5 s and none in
    an update the dying are heard in. All these sounds together (agony, wounded, burning,
    panic, din) keep a budget of 2 a second on average and 4 at once; over it the din gives
    way first, then the wounded and burning, the moans of the dying, fear, and last the
    screams of the falling.
  - Tuning: every number above is in `agony::Mix` (`Mix::DEFAULT`). Natively the variable
    `ACR_AGONY_MIX` overrides any of them without a build, as `key=value` pairs separated
    by commas (times in ticks, ranges `from-to`), e.g.
    `ACR_AGONY_MIX="max_per_second=3,burst=5,max_dying=6,wounded_pause=30-60,din_min_kills=10"`;
    keys `max_per_second`, `burst`, `max_dying`, `max_wounded`, `max_burning`,
    `suffer_gap`, `wounded_pause`, `burning_pause`, `wounded_volume`, `burning_volume`,
    `severity`, `panic_group`, `panic_gap`, `din_window`, `din_min_kills`, `din_ticks`,
    `din_pause`. With `RUST_LOG=acr_game::agony=debug` the log has one `agony mix` line per
    second of game time with a sound: what played, what the budget left out and how many
    dying, badly wounded and burning men were in hearing.
  - Measured (render VM, Monongahela `battle3.m3d`, 1500 ticks = 150 s from the start,
    the default camera, showcase settings with `hit_locations=on`; steps and the summing
    script in `ACR-analysis/agony-snd/`): before the budget 2.36 sounds/s (0.36 screams,
    0.50 moans, 0.26 gasps, 0.24 wounded, 0.49 fear, 0.51 din; up to 9 in one second);
    with it 1.39/s (0.19 screams, 0.27 moans, 0.15 gasps, 0.17 wounded, 0 burning,
    0.31 fear, 0.31 din; at most 4 in one second, 0.95/s left out), with up to 37 dying
    and 43 badly wounded in hearing at once.

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
- **Shooting at a garrison.** Against a `CANBEKILLEDINSIDE` building (NM+0x172c bit 2: the
  houses, mills, blockhouses, forts) with someone inside (OB+0xf0), every attack index with a
  weapon fits besides those whose `ATTMASK`/`CANKILL` covers its material (`AttackObjLink`
  0x51c232..0x51c2aa, 0x51f12e), so musketeers ordered to attack a garrisoned house shoot it
  and each ball goes through the `B` roll above. An attacker with a fire weapon kind (`F`/`X`,
  NM+0x172e bit 0, 0x5005fe) gets this only while the building burns (OB+0x26 bit 1). Once the
  last man inside is dead the index no longer fits and the attack ends (0x51f3df).
  `AttackObj` itself checks no materials; the search (`SearchVictim`, kill mask NM+0x2861 at
  0x52718d against NM+0x2862 at 0x525c5a) has no such exception, so riflemen do not pick
  garrisons by themselves. A unit dying inside leaves the inside list (`Die`
  0x4e7bf8..0x4e7d08). Cannon balls (`IADRO`, no `B`) hurt the building itself; its garrison
  dies with it.
- Building boxes (`3DBARS x y l w h`, `bars.rs`; `Add3DBar` 0x443697 at creation, removed at
  death): 45°-turned rectangles in 256 px cells, entered in the cells of nine sample points
  only, as in the engine. They stop projectiles and traces and hide units from searches.

### Artillery

The cannon (`ArtPus`, `USAGE PUSHKA`, `MOTIONSTYLE COMPLEXROTATE`; `artillery.rs`):

- **Motion** (0x50f7cb): at the end of a cycle a cannon facing its target within a 16th
  drives one cycle in stance 1 (`#MOTION_L0/R0`); otherwise it changes to stance 2 and turns
  16 per turn animation (`#MOTION_L1`: +16 at its end, `#MOTION_R1`: −16 at its start).
  The drive step (`TryToMove` 0x50b021) needs the cell at the end of the cycle free, except
  in unlimited motion, so a new cannon drives out of the building that made it; the order
  links (`SendTo`, `SmartSendTo`, `ClearUnlimited`) run before the handler as for any unit.
- **Attack:** the same 16th-steps while aiming (the engine calls `TryToStand` on the target
  there, a random number, kept); each shot of indices 4 and 5 costs 50 iron and 100 coal
  (`RASTRATA_NA_VISTREL2`, paid when the strike starts; without them the order ends). Index
  4 fires the cannon ball beyond 1500 px, index 5 canister below.
  With the hit-locations rule the fire mode decides instead (`artillery::gun_shot`,
  `docs/damage-model.md`, "Gun modes").
- **Why a turn starts late** (checked on the blank field, `ACR-analysis/cannon`): the attack
  link acts only at the end of the current animation, and a turn first changes stance
  through `ARTPUS.MD`'s transition animations at two frames per tick: a deployed gun
  (stance 5 or 6) limbers in `#UATTACK4/5` (171 frames, 8.5 s), lifts the trail in
  `#PATTACK1` (71 frames, 3.5 s), turns 2 s per 16th (`#MOTION_L1/R1`, 40 frames), lowers
  the trail in `#UATTACK1` (3.5 s) and deploys in `#PATTACK4/5` (8.5 s) before the shot; a
  gun still in its 420-frame `#ATTACK4/5` (the shot and the reload drill) finishes it first.
  The reload counter does not hold the turn. This is the original's data and order, kept.
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

**Burning down** (`SLOWDEATH`, NM+0x172a bit7, 114 building types; 0x4e7410, every tick
after `ProcessCostPoints`, `combat::slow_death`): every 128 ticks (`tick & 0x7f == 0x11`)
each finished building of such a type below 2000 life that is not dying joins a list of at
most 64 (index order). Each tick a listed building above half its life leaves the list;
above 10 life it loses 5 (also when it leaves, without damage rules or hits), at 10 or
below it leaves and dies. A building that is gone stays listed; after a building leaves,
the next one is skipped that tick (the engine shifts the list but counts on). A building
at 1999 so burns down in 40 s at 10 ticks/s unless repaired above half. The scripts'
`DestructBuilding` (0x4e7720) only sets a finished (or busy) `SLOWDEATH` building to 1999.

Fire and smoke are drawn at the `FIRES` and `SMOKE` points of a finished building
(0x50618e → 0x505ad5, picture coordinates from `LOCATION`) with the `[FIRES]` animations
of `WEAPON.NDS` (`FIRE`: `#DFIRE1` then `#DFIRE2` in a loop; `SMOKE`: two variants by
point). A burning building shows `n·(M − L)/M` of its `n` points (at least 4), a damaged
one `n·(T − L)/T` below `T = M/2 + M/4`; one more point catches fire every 8..23 ticks
(the first after 1..4), points beyond the count go out at once; they are sorted 400 rows
below the building plus the point's index. The state and its random numbers (C `rand()`)
are the drawing's, not the simulation's; the original only advances buildings it draws
(the game: those near the view). `M` is the type's life for the owner (`AdvCharacter`
+0xdc), not the object's maximum.

A building with sections (OB+0xaa ≠ 0, so every object of an `ADDSECTION` type; 0x505af5 →
0x5052b5) burns differently:
- The band is that of its last done section `c − 1`: `lo` = the type's life plus the done
  sections before it, `hi` = `lo` plus the section's life; without a done section
  `0..type life`. Ruins and unbuilt sections do not count.
- The points are the section's `STAGE_FIRES` / `STAGE_SMOKE` (record +0x25/+0x2d count
  +0x35, +0x29/+0x31 count +0x37), or `FIRES` / `SMOKE` without a done section.
- `n·(T − L)/(3(hi − lo)/4)` points burn below `T = (3·hi + lo)/4`, at most `n`, whether the
  building burns or not (no minimum of 4). So the fires of a section fill up as life falls
  towards the point where it becomes a ruin (`lo`, see Building sections), then the
  previous done section's points take over.
- The fire state (OB+0xfd) keeps `c` in its first byte; when `c` changes both kinds start
  over (0x505487).

### Hit locations (optional rule, #225)

Not in the original game, so it is a rule of the game setup (`GameSetup::rules`,
`acr_sim::Rules { hit_locations }`, copied to `World::rules` when the world is built) and
off by default. Off, nothing changes: no random number is drawn, units carry no injuries and
the state hash is the original's (`World::rules` and `Unit::body` add nothing to the hash
while they are the default; `tests/wounds.rs` pins the hash of a fight). On:

- Every hit by a unit (melee or a projectile with a sender, also an instant `RANDOM_KILL`)
  on a unit with a body (not a building, a cannon (`COMPLEXROTATE`) or a ship) draws one
  `rando()` in `MakeDamage` after the damage is known and before the hit is recorded:
  `rando()·100 >> 15` picks the region torso (< 45), legs (< 70), arms (< 90) or head.
- A low rolling ball (`World::hit_height` below `LOW_BALL` 20 px at its ground contact)
  takes the legs 80 % of the time, else the torso, from the same `rando()`.
- Every hit then injures the body (`body::injure`, below); a hit that kills records it too.
- The region is in `HitRecord::region` and the injury in `HitRecord::injury` for the
  renderer (wounds and blood, #183); both are `None` without the rule.
- Savegames keep the rule (version 3, at the end of the file); older saves load with the
  original rules. Starting a game with it: set `rules.hit_locations` in the `GameSetup`, or
  through the driver `{"cmd": "start", "random": {"seed": 5, "hit_locations": true}}`, or
  Spieleinstellungen page 4 (`Trefferzonen`, applies from the next battle). There
  Spieleinstellungen page 4 (`Trefferzonen`, applies from the next battle).
#### The body: injuries, crawling, bleeding out (#254, #262)

The full model is in `docs/damage-model.md` (section 1); `acr_sim::body` implements it.
`Unit::body` (`Body`, hashed, nothing while unhurt) has four limbs measured by a level from
the root (0) to the tip (255; joints `ELBOW` 110, `WRIST` 205, `KNEE` 120, `ANKLE` 215),
each with the level it ends at, a fracture (closed, open, dislocated) and flesh wounds,
plus an open belly, head and chest wounds, the burning of shell fires and whether the long
weapon was dropped.

- `body::injure` draws, for a limb hit, the side, the level on what is left of the limb
  and the outcome (three `rando()`); for a trunk hit the level (chest, belly, pelvis) and
  the outcome; for a head hit nothing, or the outcome for a sabre. The outcome table by
  cause (musket ball, grapeshot, cannonball, shell, sabre for a mounted attacker's melee,
  bayonet/pike/thrown) is the doc's; the energy is the ploughing ball's share
  (`World::hit_energy`: 100, 80, … 20), else 100. A head hit kills (a sabre's half the
  time), a cannonball tears the head off, a trunk at energy ≥ 60 in two and at ≥ 90 to
  pieces; these kill whatever the damage.
- What is left decides the rest: the leg and arm functions give the walking pace
  (`move_percent`, the motion handler's rate), crawling (a leg at 0: 15 % on two arms,
  8 % on one, 0 writhing), the dropped musket (a long weapon needs two working arms and a
  man on his feet: then no ranged attack, melee instead) and out of the fight.
- Out of the fight (a crawler or no working arm): the player's commands skip him
  (`World::owned`), `attack_obj` refuses him (so he neither attacks nor hits back), and
  when the hit puts him out he drops his orders and drags himself 600 px away from the
  nearest enemy (`body::leave_fight`, a plain `SendTo`; he stays in his brigade).
- Bleeding, once a second (`BLEED_EVERY` 10 ticks, `wounds::bleed`): a leg stump above
  the knee 6 life, below 4, a lost foot 2; a man's arm bleeds slower (#254, `BLEED_ARM`:
  above the elbow 4, below 3, a lost hand 1; an animal's front legs bleed as legs); an
  open fracture 2, an open belly 5; at no life he dies with `DeathCause::BledOut`. A man
  who lost a leg therefore dies after ⌈life / rate⌉ seconds unless something else kills him
  first; his life bar runs down with it (`Unit::life`).
- Limb loss and crawling (#254) come out of the outcome tables above: a heavy hit (a
  cannonball below `BALL_TEARS`, grapeshot at 25 %, a rider's sabre at 20 %, a close
  musket ball rarely) takes the limb off at a level (`Limb::lost`); a leg at function 0
  makes `Body::crawling` true, which is the crawl state the renderer maps to its clips
  (`damage_view::crawl_clip`: `CRAWL2`, `CRAWL1`, `CRAWLARM`, `WRITHE`). One arm lost:
  he drops the musket (`Body::disarmed`), fires no more and fights on with the bayonet.
- Render hooks (not hashed): the body (`Unit::body`, its `limbs[..].lost` levels for the
  missing limb and the stump cap), `World::hits` (each `HitRecord::injury` with part,
  level and outcome) and `DeathRecord::body`. There is no separate bleed stream: the
  renderer samples every living unit with `Body::bleeding() > 0` each `trail::EVERY`
  ticks at its position (`ingame/trail.rs`, a smear for crawlers, drops for walkers).
- `DeathRecord::body` is the body at death (with the killing injury) and
  `DeathRecord::torn` how it was torn apart (in two at a level, pulverized, headless).

In 3D a crawler lies prone (the `DEATHLIE1` pose) and drags himself along head first, and
the bleeding leave a drop of blood every second that dries and fades over five minutes
(render only). Cut limbs, fractures and debris on the models come with the render slices of
#262. The setting `hit_locations=on` (Spieleinstellungen page 4, `settings.txt` / browser storage)
turns the rule on for every new game; the setup and savegames keep it.

A second optional rule, `body_spacing` (men, horses and animals take up space and push
each other apart), is described in `docs/body-spacing.md`.

#### Ploughing cannonballs (#256)

The original already lets a low ball hurt along its path: an object with `DAMAGEHEIGHT`
(the bouncing ball `SKOKAT`, 500 within 16 px below 95 px) does round damage every tick
while that low, to everyone near it at full damage, the same man again on each tick he is
in reach, with no limit on the number of victims; only the ground contact (or the end of
its animation) stops it. That stays so with the rule off.

With `hit_locations` on, such a ball ploughs instead (`projectile::Plough`, hashed with the
projectile, nothing while empty): each unit is hit at most once per ball (dying units and
the shooter do not count), the n-th body takes `damage · (100 − 20·n) / 100`, so 100, 80,
60, 40 and 20 % (`PLOUGH_LOSS`), and after five bodies (`PLOUGH_BODIES`) the ball is spent
and flies on harmlessly. Every ground contact costs it another 10 % (`BOUNCE_LOSS`,
`Plough::bounces`). A bounce (`explode` → `REFLECT` child) carries on what is left of its
ball (`World::plough_carry`, not hashed). A 500 ball thus kills three men of 300 life and
wounds two, as a 6-pounder ball down a file. The first strike of an aimed ball (`NUCL2P`,
340 within 35 px, no `DAMAGEHEIGHT`; any round of a `DeathCause::Cannonball` weapon kind)
ploughs too: its victims count against the five bodies and its bounce goes on with what is
left (`projectile::ploughs`). The energy is in `HitRecord::injury.energy` (100, 80, 50, …),
so the outcomes go from torn apart to a severed limb to a wound down the file. No random
number is drawn beyond the rule's body regions. `tests/plough.rs` shows both and pins the
original hash.

#### Shell fires (#256)

The original sets units and buildings on fire only by weapon kinds with the `X` (at once)
or `F` (after `FIRELIMIT` hits) flag of `NRES`: `Unit::burning` goes on, a burning building
loses `max(1, MaxLife >> 11)` a tick, a burning unit 1 life every 32 ticks while above a
tenth of its life (so it never burns to death), and a unit that dies burning is recorded as
`DeathCause::Burned`. Shells leave nothing on the ground. With the rule on
(`ground_fire.rs`):

- A round of a shell weapon kind (`IADROMOR`, `VAFLIA`: `DeathCause::Explosion`, unlike
  round shot `IADRO` and grapeshot `KARTECH`) that bursts on the ground (`ExplodeAnmObject`
  with terrain contact and damage) leaves a fire there (`World::ground_fires`), unless the
  ground is water or a fire already burns within its radius (a shell's children burst at
  the same point; a child bouncing on starts its own; a mortar's shell, rule `mortars`,
  starts none). Radius 40 px (about 1.3 m),
  burning `BURN_TICKS` 300 ticks (30 s). No random number is drawn.
- Once a second (after the bleeding) every soldier (not a building, cannon or ship) inside a
  fire catches fire: `Unit::burning` goes on and `Body::burn` is set to 5 s. While it
  runs he loses 8 life a second through `MakeDamage` without a sender, so he can burn to
  death (`DeathCause::Burned`); out of the fire he goes out after the 5 s.
- **Round shot (`Rules::cannon_fire`, setting `cannon_fire`, savegame rules bit 5, page 5
  "Kugelbrand"):** `IADRO` has the `X` flag, so in the original every man a ball hits
  without killing gets the never-ending burning flag. Under hit locations that made him
  panic, burn to death and show flames. With `cannon_fire` off (the default) a round-shot
  hit sets no man with a body on fire while hit locations are on (`MakeDamage` drops `X`/`F`
  for `DeathCause::Cannonball`); buildings still catch fire. Without hit locations the
  original's flag is set either way, so the pinned hashes hold. Shells (`IADROMOR`,
  `VAFLIA`) and their ground fires are not affected by this switch. Saves made before it
  replay with it off. `tests/body.rs` `round_shot_lights_men_only_with_cannon_fire`.
- The fires are part of the state hash only while some burn (`Body::burn` only while not
  0). They are not saved: savegames replay their commands and the fires come back.
- 2D draws three flames of the burning-building loop (`[FIRES]` `#DFIRE2`) per fire, fewer
  as it burns down (`effects::ground_fire_sprites`, sprite path); the 3D view draws flames, smoke and a
  scorched patch (`docs/rendering-3d.md` "Fire: shell fires and burning men").
- A soldier already burning from an `X`/`F` weapon when a shell fire catches him keeps
  `Unit::burning` after his 5 s are up (`Body::burn_kept`): the original's flag never goes
  out. The renderer shows a burning man by `Unit::burning` (and `Body::burn`, the seconds
  left from a shell fire).
- **Burning men panic** (`ground_fire::panic`, once a second after the burn). In the
  original only kills raise fear (`Fear` 0x601960 is called from `MakeDamage` 0x4e8f92 and
  0x4e9c4d and from `PanicLink` 0x600720; `StartPanic` 0x600e20 only from `TryPanic`
  0x6017a0), so burning frightens no one there. With the rule a living soldier on fire
  (a shell fire or an `X`/`F` weapon) who is not running yet gets the original's panic order
  (`morale::start_panic`: he leaves his formation, which breaks up below 5 others, and
  takes no orders while his 128 px legs run with `UnlimitedMotion`) and `Body::flee` = 4 s
  (`FLEE_SECONDS`). While `flee` runs, `PanicLink` skips its calm-down draw (`PSIXOZ`), and
  its `Danger` (0x600670) also counts his own side's living units like enemies
  (`1000/(d+50)`) and every shell fire like a garrisoned building (`20000/(d+50)`), with a
  random term `rando() & 63` instead of `& 7` (the same draw): he runs an erratic path away
  from the flames and his comrades. His second leg frightens his comrades through `Fear`
  (fear type 0, 256 px) as any panic does, so neighbours may flee too. When `flee` is up
  one `rando()`: below `OUT_CHANCE` 16384 (one half) rolling and running put the flames out
  (`Unit::burning` off, even an `X`/`F` weapon's flag, `Body::burn` 0) and he stops where he
  is, his orders cleared; otherwise he falls, burned to death (`MakeDamage` of his life,
  `DeathCause::Burned`). A shell fire's flames that end during the flight end it without a
  draw. The renderer needs nothing new: the flight shows the run clip, the fall `DEATH`.
  `Body::flee` hashes only while not 0. `tests/burn_panic.rs`.

#### Direct hits: torn apart or a blood mist (#256)

The hit record already says how far a man was torn (`HitRecord::injury.outcome`,
`DeathRecord::torn`): `Outcome::Off` takes a limb off at `injury.level`, `Torn::InTwo` splits
the trunk at a level, `Torn::Headless` takes the head, and `Torn::Pulverized` leaves nothing
whole. The renderer reads these (`debris`, `tears`, `damage_view`, `particle_view`): parts
thrown with stump caps (#207) for `Off`, `InTwo` and `Headless`, the blood mist and gibs
for `Pulverized`, scaled by `injury.energy`; the gore setting decides how much is drawn.

Round shot reaches them through the ploughing energy (`body::BALL_SEVERS`, `BALL_TEARS`,
`BALL_PULVERIZES`). A shell's blast used the fragment table, which never tears a body;
with the rule on, a man (not an animal) whose centre is within `SHELL_DIRECT` 18 px of the
burst is torn in two at a drawn trunk level (one `rando()`), within `SHELL_MIST` 8 px
pulverized (no draw) (`body::shell_direct`, from the burst distance
`World::hit_close`, not hashed). Off, the blast is the original's (`ExplodeAnmObject`
0x5578f0 round damage, no injuries). `tests/ground_fire.rs`
`a_shell_at_a_mans_feet_leaves_a_mist_closer_ones_are_torn_in_two`.

#### Balls against trees and stones (optional rule `ball_terrain`)

**The original:** balls fly through woods and rocks. `ProcessExpl` (0x557fa0) calls only the
terrain height (0x4d0530), the building boxes (`Bar3DHeight`/`Bar3DOwner` 0x443b06 /
0x443d37), `MakeDamage`, `MakeRoundDamage` and the explosion; no map-sprite lookup. Trees
never run out when cut (empty `[WORKTRANSFORM]`, "Gathering"), ordinary trees and `ST*`
stones never block paths ("Passability"), and nothing in the engine has a line of sight
(the fog is a radius stamp, "Fog of war").

**The rule** (`Rules::ball_terrain`, `acr_sim::ball_terrain`; acts only with
`hit_locations`, on ploughing balls, `Plough`): after each move (`projectile::process`, before
the destination test) the ball's way of this tick is tested against `World::objects`, the
nearest first, while the ball is below `TRUNK_TOP` 100 px (stones `STONE_TOP` 40 px):

- **Trees** (`TREELIST` objects that are wood sources; reach `TRUNK_REACH` 10 px): girth
  20..80 % from a hash of the place (`ball_terrain::girth`, no `rando()`). A ball with that
  much energy (`Plough::energy`) fells the tree towards its flight and loses half the girth
  (`Plough::lost`, also slowing it by that share); a weaker ball splinters it and stops.
- **Small stones** (ground cell free; reach 12 px): knocked `STONE_PUSH` 40 px, 45° off the
  ball's way to the side it lay; the ball loses `STONE_LOSS` 20 %. **Big rocks** (a locked
  ground cell, i.e. a `[LOCK]` rock; reach 24 px) chip and stop the ball.
- State: `World::struck` (`Struck::Felled/Splintered/Moved/Chipped` with tick and direction),
  hashed only while not empty; `Plough::lost` hashes only while non-zero. Off, nothing is
  tested, nothing drawn, and the pinned hashes hold.
- Paths and resources stay consistent: a felled tree is still a wood source (its trunk lies
  there) and trees never blocked; knocked stones are the non-blocking kind; rocks stay.
- Switched on by `ball_terrain=on|off` in `settings.txt` (default: on with `controls=modern`,
  only together with `hit_locations`), savegame rules bit 6, the driver's `start` key
  `ball_terrain`. The driver's `map_object` request puts trees and stones on the blank field
  (`ball_terrain::add_object`); `state` lists `struck` and the last `impacts`.
- Tests: `tests/ball_terrain.rs`.

### Mortar (optional rule, #301)

Not in the original game. Period field guns fired solid shot almost flat, so indirect fire
comes as its own unit, not as a high-arc mode of `ArtPus`. With `Rules::mortars` on
(setting `mortars=on`, driver `start` option `mortars`, savegame rules bit 2) the game adds,
before the world is made, an `ArtMrt(XX)` type after the original types for every
`ArtPus(XX)` (`mortar::append_types`; off, the catalog, the type ids and the state hash
stay as they were):

- A copy of the field gun (`md_name` stays `ArtPus`, so it draws with the gun's sprite and
  clip keys; in 3D the type id finds its own model, `artmrt.glb`, see rendering-3d.md "Gun
  crew") without canister: `WEAPONINSECTOR` and the `KARTECH` attack go.
- The ball attack fires the original's unused mortar shell `NUCL3M` (`WEAPON.NDS` line 38:
  `#NUCL2 800 60 NONE HI_GRAVITY3 ANGLE 75 -1`; at the burst its `[SYNCWEAPON]` throws the
  `NUCLM` fragments and the `PU2` blast) as weapon kind `IADROMOR`
  (`DeathCause::Explosion`), from `MIN_RANGE` 600 to `MAX_RANGE` 4000 px, reloading
  `RELOAD_PCT` 150 % of the gun's `ATTACK_PAUSE`.
- The shell flies at `ELEVATION` 60° (`GunMode::Mortar`, carried by the projectile like the
  other gun modes); the scatter is the gun's `RAZBROS` through `CreateRazbros` (0x51b917),
  which grows with the range. With hit locations also on, the burst throws shell fragments
  but leaves no ground fire, so nobody catches fire or panics from it (user decision; a
  howitzer's `IADROMOR` shell still does, `ground_fire::shell_burst` asks
  `mortar::fired_by`); with mortars alone it does the original's round damage. Neither
  `IADROMOR` (`NRES` flags `-`) nor its children carry the `X`/`F` fire flags. The shell's
  impacts and fragments carry `mortar: true` (render only, not hashed): the 3D view draws
  a smaller burst (rendering-3d.md, "particles of a hit"); damage and radius are the
  shell's own.
- It gets the gun's `TypeTech` and access rules and follows it in every build list that
  holds the gun. Upgrades of the gun do not reach it yet.

**Zones.** `Command::MortarZones` (replay tag 33) gives mortars up to `MAX_ZONES` 8 circles
(radius clamped to 32..800 px; none clears them), `Command::MortarFire` (tag 34) starts or
stops their fire. After the commands of a tick (`mortar::tick`), every firing mortar that is
idle (no order) looks at its zones in turn from the one after its last target, takes the
first whose enemies (living, on the map, no buildings) have their centre within its reach
and no nearer than its minimum, and lays one round there through `NewAttackPoint`
(0x51b4aa, priority 0x90, one shot). The zones (`World::mortar_zones`) are hashed only
while some mortar has them; a dead mortar's go. A ground attack inside the minimum range
fires only once the gun has drawn back beyond it, like the field gun's ball inside its
`ATTACK_RADIUS` minimum. No random number is drawn beyond the shot's own. `tests/mortar.rs`
covers the types, the zone fire, the minimum range, the rule off, and that a mortar's
shell starts no fire while a howitzer's still does.

**Controls.** The rule is the line `Mörser` of the game options' rules
(docs/ui.md, "Game menu"); selected mortars get the icons "Zielzone setzen", "Zonen
löschen", "Feuer frei" and "Feuer einstellen", which send these two commands, and the marks
of their zones and minimum range (docs/ui.md, "Mortar buttons"). The driver's `state` with
`units` shows a mortar's `mortar: {zones: [[x, y, r], ...], firing}`.

### Corpse piles (optional rule, #222)

Not in the original game, where corpses never hinder anyone (`Die` 0x4e77a0 frees the
dead man's cell). With `Rules::corpse_piles` on (setting `corpse_piles=on`, driver `start`
option `corpse_piles`, savegame rules bit 3) dense piles of bodies hinder the living
(`crates/acr-sim/src/piles.rs`):

- **Count.** The sim counts corpses per 32 px pile cell (about 1 m² at 30 px per metre).
  A body counts from `Die` until the object loop (0x5118b3) removes it: after
  `CORPSE_LIFETIME` 2000 ticks of lying, as a carcass, or at once for a type without
  `#DEATHLIE1`; a script's `RemoveUnits` and a garrison or storm defender who is "gone at
  once" never count. A body thrown by a `DETONATOR` blast (0x559f20) counts where it lands.
  Men, riders and animals on land count; buildings, guns and ships do not. Updates happen on
  death, removal and landing only; nothing scans the units.
- **Rough ground.** From `ROUGH` 4 bodies in a cell, a man standing on it walks at half his
  rate (after the stance rate and leg wounds; `motion::process_unit`).
- **Obstacle.** From `BLOCKED` 7 bodies the cell's 2 × 2 lock cells are set in a copy of the
  land lock map (made when the first pile blocks, cleared again where the land itself is
  free when the count drops). Land walkers use that map for their path search
  (`CreatePrePath`) and steps, so they walk around the pile like around a rock; a detour is
  as long as the pre-path search finds. A man standing on a blocked pile uses the plain land
  map, so he can climb out; a goal inside a pile moves to the nearest free spot
  (`FindBestPosition`).
- **Decay** is the corpses' own: the count drops as the object loop removes them.
- **Off** (the default): nothing is counted, no lock map is made, no random number is
  drawn; `World::piles` and the rule are hashed only when non-empty or on, so the pinned
  hashes stay. Tests: `crates/acr-sim/tests/piles.rs`.

Not covered: cannons and the push of formations (`spacing`) ignore the piles, the
coarse area routing (`TOP1`) does not know them, and `RemoveFools` may still swap a man into
a blocked cell. In Spieleinstellungen it is the line `Leichenhaufen` (`Aus (Original)` /
`An: als Hindernis`) on page 4, Realismus ([ui.md](ui.md)); the setting is `corpse_piles=on`.

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
  - The AI scripts trade on the same market: `AI_Torg` (0x569083 → 0x4aa090) calls
    `PerformOperation` for the city's nation (`City+0x11dcc`) and `GetTorgResult` (0x5690c0
    → 0x4aa039) quotes `trunc(amount · Sell / Buy)`, both after mapping the script's resource
    ids through `INVECO` (0x67169c). Deviations: an amount of 0 or less trades nothing (the
    engine would trade a negative amount), ids past the six resources do nothing and quote 0
    (the engine reads past `INVECO`), and `AI_Torg` from a mission script does nothing (the
    engine uses whichever city ran last).
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
  - **Ruin explosions** (0x4e952f..0x4e98bd, `combat::section_ruin`): a section that
    becomes a ruin and has `EXPLPOINTS` cells (relative to the lock base cell 0x516fde)
    draws two `rando()` per cell: the first picks one of the type's `DESTRUCT` weapons
    (`r·n >> 15`), the second below the `DESTRUCT` probability creates it (0x558d60 →
    `Create3DAnmObjectEX`, no sender, from the cell to `(x, y, 1000)`). The explosions
    are lifted by `h = done y + 80 − 8·(mean cell y)` (the section's state-2 sort `y`):
    `y` 2h further down the map, `z` = ground + h. Both numbers are drawn also without
    `DESTRUCT`. The section loop reads the object anew for every section, since the
    explosions may hurt it again.
  - Then the engine frees (`BClrPt`, 0x5181bb) the `STAGELOCKPOINTS` cells of the stage
    numbered by the cell loop's counter, which is the cell count by then (a bug): a stage
    0..4 for up to 4 cells. With more it reads past the five lists (NM+0x16ce + 2·count);
    for the only file with `EXPLPOINTS` (PORTKRE, 298 cells) that is the middle of
    `OneStepDX[50]` (`cos·STEP >> 8` = 10), so 0 cells, and nothing is freed.
  - **`STAGELOCKPOINTS`** (0x53281d): finishing a section (`UPSTAGEn`) locks the cells of
    every done section (each twice). Only HOLDO1 has them (for its undeclared stage 1).
  - Neither PORTKRE nor HOLDO1 is in `NMLIST.TXT`, so in the shipped game no section
    explodes and no stage cells are locked; `STAGE_FIRES` are drawn for 9 loaded forts.
  - `UPSTAGEn` does nothing while the building burns (OB+0x26 bit 1, 0x5326a1): the
    research ends, the section stays unbuilt and its upgrade is offered again.
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
  - **Enemy armies** (`armies.rs`, `GlobalEnemyInfo` 0x6ba1b0), what `EnumEnemyArmies`
    reports. Every nation 0..6 with units gets an `EnemyInfo` at the start (`Setup`
    0x427373), shared with the other such nations whose mask meets its creator's; nation 7
    has none. `GlobalEnemyInfo::Process` (0x4275e1) lets the `EnemyInfo` of nation `k`
    research when `(tmtmt & 255) == 8k + 1`, i.e. once every 256 ticks
    (`ResearchArmyDistribution` 0x4423d0): the living objects outside the `EnemyInfo`
    (units inside buildings have `Sdoxlo` 1 and are left out) whose `USAGE` has an army
    category (table 0x644454: `TOWER`, `FASTHORSE`, `MORTIRA`, `PUSHKA`, `GRENADER`,
    `LIGHTINFANTRY`, `STRELOK`, `HARDHORCE`, `PEASANT`, and — read past the table's 26
    entries — every usage from 26 on: `GALERA` ... `DIPLOMAT`) are counted per 256 px cell
    (`FORCE` sum and number). Cells with more than 3 units that touch (the cells `+1`, `+W`,
    `−1` if the index is above 1, `−W` if above `W`) take the smallest id until nothing
    changes; then twice every such cell gives its id to the small cells next to it. Each id
    in cell order becomes an army: bounds in cells (bytes, starting at 250/0), force and
    unit sums, and the mean cell's centre `mean·256 + 128` in pixels (`i16`). a mine placed within 64 px of a
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
- **Entering buildings** (`GoToMine` 0x5f43d0, `economy::enter`): only peasants and
  `CANSTORM` types go in, mobile ones, not a `HIGHUNIT` into a `HIGHUNITCANTENTER` building;
  the building is not dying and finished or busy, takes units (`UNITABSORBER`) or peasants
  (`PEASANTABSORBER`) and has `CONCENTRATOR` points; a friend's must have room, an enemy's
  need not. The order keeps `prio & 0x7f`; below the unit's priority it is refused, and a
  unit already going there keeps its order. The player's command (0x4de4cc) sends the
  selected units that are not on their way into a building, as many as there are places
  (1000 more for another nation's building). A unit walks to the first `CONCENTRATOR` point
  (`GoToMineLink` 0x5f46f7, lock-cell distance below 2) and disappears inside
  (`Unit::inside_of`). Units inside still eat and count. `Leave` puts them back at the born
  points.
- **Storming** (0x5f4ad1..0x5f502e): at the door of another nation's building, a guarded
  one (`SetAI_Guard`) kills the unit; an empty one is taken (its orders cleared, see
  Capture for the change of sides) and the unit goes in; a garrisoned one is stormed: the
  unit plays `#ATTACK` to its end (the frame is not reset), then one defender `D`
  (`rando()·n >> 15` of the `n` inside) falls when `STORMFORCE(D) · (a + (b − a)·n /
  MaxInside) · rando() >> 15` (`OPPOSIT a b` of the building) is below the stormer's
  `STORMFORCE`, and the stormer dies in any case (a type without `STORMFORCE` without the
  draws). The fallen defender leaves the garrison at once. With Ctrl the interface's right
  click on such a building sends units in (the door cursor, 0x4c4808). Not reproduced: the
  walk along the further `CONCENTRATOR` points into the building (OB+0x29 bit 1, also for
  friends' buildings) and the way back to the first point when the building is lost on the
  way, the storm flag (OB+0xf2 bit 1, no reader found), throwing out a unit of a friend's
  overfull building (0x5f518b), the transport test (0x54d6a2), `AutoKill` and the territory
  checks.

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
  The game's frame (`acr-game` `Game::redraw`): winit delivers the window's input events
  first (they only record state and queue commands), then `advance` runs the due ticks
  (at most 4 per frame; time beyond is dropped, the game slows instead of spiralling) and
  builds the 3D frame, then the cursor is set (a system cursor natively, docs/rendering-3d.md
  "Input latency"), then the frame is drawn and presented. `acr_sim::prof` times the
  parts (`ACR_PROFILE`, the driver's `profile`).
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
- **Script API on the simulation.**
  - `InviteUnitIntoBuilding` (0x56c638) is `GoToMine(building, 0xFF, ordType)` with
    `CreateOrder`'s placements (`economy::enter_ex`); `GoToMine`'s own gaps are listed under
    Economy (the walk inside, the transport test).
  - `GetTotalAmount1` (0x55e7f2) reads the census's per-type count (`City+0xeb73`,
    `Player::type_count`); a type handle beyond the catalog reports a script error, where
    the engine returns 0 silently for an index ≥ `NMONS`.
  - `SelAttackGroup` (0x560258) tests `EnemyID == 0xFFFF` as "no current attack order"
    (`Unit::target`); a unit whose attack was placed behind other orders would be offered
    the next enemy too.
  - `GetUnitInfo` / `GetUnitGlobalInfo` (0x567a0e, 0x567cb9) accept a dying object only when
    it is `Hidden`; ACR's units inside buildings are not dying, so `living()` is the same test.
  - `GetUnitExCaps` (0x56be77) reports the executed order as the index of its link in the
    engine's table of 34 order links (0x66a258, looked up by 0x4b0f8c). ACR keeps a
    building's research, `UNCONSTRUCT`, production and mine work as state rather than
    orders; they are reported in the order its building link runs them (research 4,
    unconstruct 33, production 1, a bound mine 7), and a building with none of them reports
    −1. The engine's order chain may rank production and mining differently.
  - `OBJ_SendTo` (0x56f0a1 → `NewMonsterSendTo` 0x517045, `motion::send_to_typed`) skips the
    engine's early returns for ships (`ParkWaterNewMonster` 0x54aa9c runs later, in
    `ship::check_park`), the transport test 0x54d6a2, the `OB+0x29` bit 7 lock and the
    re-placement of a unit on a locked cell (0x518246/0x513574); a dying object is ignored
    (the engine would give it the order). Buildings and `UnlimitedMotion` walkers refuse
    the order as `CreateOrder` does, and the priority test ignores a stale `PrioryLevel`
    when the unit has no orders.
  - `OBJ_LeaveMine` (0x56f1d6 → `LeaveMine` 0x5f518b, link 0x5f522e, `economy::leave_mine`)
    lets the unit out with the spawn's walk-out (`economy::walk_out`, 0x49eeb7): it stands at
    the first born point and walks the further points one cell in; the link sends it
    through every point one cell in (0x517045, prio 0x10). Neither takes the second point
    set for artillery (NM+0x178e, `NM+0x172a` bit 5). The rally point follows the spawn's
    rules except on the building itself: the leaving unit goes back in only when the
    building is full (0x5f56f7). The flag OB+0x29 bit 1 the link clears and sets again is
    not kept. The player's leave command (`Command::Leave`) still lets everybody out at once.
    `LeaveBuilding` (0x56c6ca) gives the same `LeaveMine` to any object with someone inside
    (OB+0xf0), without the `LeaveMine` order test of `OBJ_LeaveMine`; a dead object's slot
    is empty here, where the engine would still find the dying object.
  - `SetEnemyBuildingsCollector` (0x577e71) appends every building not allied with `nat`
    that no earlier call has collected (OB+0x23 bit1, kept in the script state's per-object
    data as `collected`) and marks it, without looking for it in the group; a dying building
    is collected as in the engine. New buildings join through the creation hook (0x588569),
    which ignores the mark and checks the group instead.
  - `SetStandGround` (0x5621cd → 0x4e09c7) and `SetUnitsState` (0x578c09) switch a formation
    member's whole formation through `MakeStandGroundTemp`, so the flag bearer's extra
    (`n5`, `n6`) is part of the bonuses; dead selected units are skipped, where the engine
    would set the flag on any object still in the slot.
  - `CreateFormation` (0x56fbf8 → `CreateFromGroup` 0x41a00e, `formation::create_from_group`)
    forms the brigade at once, owned by the owner of the group's first unit; the engine
    takes a brigade of nation `nat` (`GetFreeBrigade` 0x42c845) whatever the units' owner,
    and fails when that nation has no free brigade, which ACR never does. The member flags
    `OB+0x29` bit 7 and the brigade fields +0x1a and +0x2e bit 3 have no ACR counterpart;
    the shape name is compared exactly, `#` included.
  - Light spots (`SetLightSpot` 0x564017) are stamped once per tick with the fog's blur
    (`MissionLink::update_fog`); the engine stamps them every frame (0x55a2f1 in the main
    loop, 0x4bee14), which only differs while the game is paused.
  - `MakeOneShotToBuild` (0x573da9) reads the building map (`BLDList` 0x27743a0: per
    128 px cell the last living building whose position lies in it, rebuilt with the
    buckets by 0x50a283) as `Buckets::buildings_in(..).last()`. Its lookup 0x56144f adds the
    border offset a second time, so ring cell (x, y) tests the building of cell
    (x + 1, y + 1); ACR keeps that. A member that gets a target leaves its brigade's orders
    (`ClearBOrders` 0x415cca); the `OB+0x25` bit 0 the engine sets then has no ACR field,
    and the "walking into a building" bit (`OB+0x29` bit 1) that spares an attacking member
    from release is read as `unlimited`.
  - `GetEnemyForce` (0x571aa8) sums the unit buckets of the zone's rings (no buildings), the
    radius and the area-distance limit `(r >> 5) + 3` (0x43c7d8) as the engine does; it
    tells friends by the object's alliance mask after `sync_alliances`.
  - `CreateTopListEnArmy` (0x57bad9) counts in a `u16` per area; an undamaged building
    (`Life` against the nation's `Adv` life) with `GetBuildingOposit` adds that value and
    is lifted to `minForce`.
  - `GetBuildingEntryPoint` (0x56ea8c) uses the first `CONCENTRATOR` point from the lock
    base cell (0x516fde); the engine does not test whether the building is dying, ACR's
    object lookup skips removed objects.
  - `TryUnit` (0x56829b) does not write the byte `GeneralObject+0xaa = 0xFF` (no ACR field;
    its readers are not ported). With game option 2 at 1 (0x6e45c9, `CAEP`) it refuses
    artillery and `USAGE TOWER` types.
  - `GetPeaceTimeLeft` (0x569882) returns the minutes 0x6e45d8, which the host refreshes
    every 30 s as `PeaceTimeLeft / 60` (0x4a7573, network command 0x4d843f → 0x4d84b5).
- **Peace time** (`acr_sim::peace`, `PeaceTimeLeft` 0x66795c, from the map's `CAEP`,
  0x545705). The engine counts it down by wall-clock seconds (0x4bf51e, `GetTickCount`
  through 0x5469ce) and refreshes the scripts' minutes every 30 s of wall-clock time;
  ACR counts one second per 10 ticks after tick 20 and refreshes the minutes every 300
  ticks, so a faster game speed does not shorten the peace. The end command 0x4d8293 also
  clears bit 0 of the matching records of the list 0x1100d6c (0x4cfa30); that is not
  reproduced, nor the `h:mm:ss` display (0x4bf767). While it lasts, the AI's diversions
  (`MakeDiversionLink` 0x424957) do nothing; the other blocks (the city's attack code
  0x422d3d and 0x4391a9, the territory checks 0x45f65c/0x45f767) wait for the city's
  armies and the territories.
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
  - Enemy armies: the `EnemyInfo` groups are formed on the first tick from the nations
    that have units then (the engine at map load, also counting a nation whose `City+0x88ba`
    flag is set) and never change, as in the engine; `StartAI`'s temporary masks (all
    nations but 0 together) are not reproduced. An object left of or above the map is
    skipped (the engine kills it there).
  - AI placement: the planning maps are built at the first use (the engine at game start);
    `CheckBuildPossibility` (peace-time cordons) and `HandleConstructions` (re-sending
    builders without a collector) are not reproduced; the deposit test of mines uses the
    deposit list (radius of the site test instead of each sprite's work radius); the
    builder team ignores its minimum size.
  - Not reproduced yet: the AI difficulty factor on production time; the 10× cannon price
    lobby option.
  - AI landing: the script groups live in the script host, which hands the start group's
    units to the simulation before a step and takes the units taken and landed back
    afterwards; the AI DLL runs before the brigades of its city tick (the engine runs
    them first). The transport is not a member of its brigade (`BrigadeID` stays unset),
    and taking a unit out of its formation empties only its slot. The other branches of
    `RegisterNewUnit` (the city's armies and lists) are not reproduced; the stock DLLs set
    a units collector, which makes them do nothing for land units. There is no peace time
    (`PeaceTimeLeft` would stop the landing). The cell lists of `ProcessCostPoints` hold the
    active units in id order. A place whose land point has no area gets no island (the
    engine leaves the byte uninitialised), an unrefreshed place has transport 0. The load is
    not limited to the order's 320 slots. Islands and coast places are not hashed (they are
    derived from the map and the units); the landings' orders are.
  - Grouping: brigade ids are never reused here, so a group never takes in a new
    formation that gets a dead one's id; reads behind a group's ids (counts moved by the
    `DeleteFromGroups` bug) stop at its ids.
  - Upgrades without effect here: `SHAR` (`WEAPON` and
    `PRECISE` have none in the engine either). `ENABLE
    UPGRADES` does nothing (the engine indexes the type table with upgrade ids).
  - Rally point on a building: the original hit-tests the building's picture in rings 0..3
    of 128 px cells (0x49ed8e); here a building with a lock cell under the point counts.
    The random search for a source spot is capped at 4096 tries (the engine would retry
    forever). `GotoFinePosition` is not called.
- **Combat:**
  - Not reproduced: `BackSteps` when an enemy is too close; the territory checks; the AI's
    random-cell search (`Zombi` units) and its artillery pickers; healers. `PredictShot`
    looks into the 128 px cells as the ball's trace does (the engine skips the last row and
    column of its cell grid); the hints of a blocked shot are not shown.
  - Ships: `MoveObject` and docking use the order's cell centre rather than the clicked
    point; `EraseLodki` has no caller in AC. `GetShipDanger` reads the ships of the 128 px
    cells from the tick's bucket grid (built at the tick start), with their current
    positions and velocities; the engine's cell lists follow the units as they move. The
    flight's priority bit 0x80 is dropped by `send_to`.
  - Projectiles: the explosion's water test samples the `SEA2` depth with the rows shifted
    by the height (the engine reads a screen-space water map); the height of a thrown
    corpse is not drawn; `extra` (the shot point's visual height) is kept but unused, as in
    the engine.
  - Buildings: the cells of a building for round damage and searches are those of its
    position (the engine keeps building lists by cell); see Capture and Storming for what of
    them is not reproduced.
  - Morale: the fear scan takes the units of the 128 px buckets built at the start of the
    tick (the engine's cell lists change during the tick); the cell number in the panic
    seed uses the bucket grid's width. Fleeing units walk in stance 0 (the engine runs them
    in the stance-1 motion where the type has it). The panic order's removal from the
    selection is left to the interface.
  - Formation melee: the AI's army melee `Army::Bitva` (0x43ba30) is not reproduced. It
    works only on the AI city's armies (`City+0xae7d`, 0x79-byte records), which ACR does
    not have: the engine calls it from `MakeDamage` (0x4e8b62) when a hurt unit has the
    `Zombi` bit (OB+0x25 bit 6) and its brigade belongs to an army (brigade +0x2a), and from
    the city's army code (0x4383d8, 0x4391a9, 0x43a171). At most once per 64 ticks it gives
    the army an order listing the enemies in the members' bounding box widened by 6 cells
    of 128 px (0x43ab87); its link 0x43af30 sets the archers on `FindArcherTarget` targets
    (0x43ef4b) and gives every member the nearest enemy in front of it with `AttackObj`
    priority 0xC0 (0x51a8ec), collecting the enemies anew on `rando() < 0x2000`. It needs
    the city's army system first: armies and their orders (0x437f52/0x437fff, the links
    0x43cdc3/0x43d0cd/0x43a9f6), the army branches of `RegisterNewUnit` (0x42dda3) that
    give brigades to armies, the `Zombi` bit and the army step of `City::Process`
    (0x432582). The close-combat formations have their own melee (`formation::bitva`). Keep positions
    counts a member in place when it reached its slot's cell (as far as a walk gets).
  - Brigades are appended, never reused (the engine takes a free slot of the nation's 512).
  - Shot damage falloff uses an integer `2^(−d/dec)` (16-step table, linear interpolation)
    instead of the x87 `exp`, so values can differ by 1.
- **Not reproduced yet:**
  - motion styles 1 and 4 (`SLOWROTATE`, `ROTATE&MOVE`);
  - `GotoFinePosition` (0x4a0670, at `StandTime == 48`);
  - the optional terrain pass 0x449aa4;
  - walls and gates (no shipped type can build them, see **Walls**);
  - deleting sprites under new buildings (0x4d1df0).
- **Walls.** AC 1.46 can hold walls (the `WAL2` map chunk, savegames, the wall build mode
  0x553621 and its network command, executed by 0x4dc126), but none can come into being
  with the shipped data: the build menu enables wall placement only for a type with
  `WALLOBJECT` (GO+0x1729 bit 1, set by the `.MD` loader at 0x5000a0; checked at 0x4bdcce),
  and an object becomes a wall piece (OB+0x29 bit 0) only from such a type (0x51623b).
  None of the 252 `.MD` definitions has `WALLOBJECT`, no shipped map has a `WAL2` wall,
  and no script or AI function builds one. So the wall passes of `AttackEnemyInZone`
  (0x564a60..0x564b61) and of `FindArcherTarget` for `SGP_AttackByArchers` (0x43e3cc) can
  never find a wall and are left out; the ports are complete for the shipped game.
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
- **Looking before shooting** (`tests/predict.rs`): a musketeer with a friend 200 px ahead in
  the line walks 40 px to its left and hits the enemy from there, the friend unhurt; one
  already standing in `#PSTAND1` behind a friend drops every attack and never fires; a
  300 px ridge between musketeer and enemy stops the shot (reload 20 frames, no hit).
- **Thrown corpses** (`tests/detonator.rs`): a `DETONATOR` 8000 blast 100 px away throws
  the dead victim with velocity (1333, 0, 1333), 6, 7 and 4 px high, down after four ticks
  21 px further, without random numbers; other blasts throw nobody.
- **Capture** (`tests/capture.rs`): an undefended peasant 200 px from an enemy goes over at
  the tick of its index, without orders and with the new mask; a friend 350 px away, an
  enemy 260 px away, an animal, nation 7 or capture rule 1 prevent it, a friend 600 px away
  does not; an idle building site is captured with its stage, a finished house is not.
- **Storming** (`tests/storm.rs`): a soldier sent into an empty enemy barracks takes it and
  is inside; one sent into a garrisoned one kills the defender (`STORMFORCE` 10 against 5)
  and dies, the next one takes it; a unit without `CANSTORM` and a `HIGHUNIT` do not go in.
- **Burning down** (`tests/slowdeath.rs`): a `SLOWDEATH` house at 1500 is listed at the
  next 0x11 tick and dies 299 ticks later, a house without the flag keeps its life; a
  repaired one leaves the list (losing 5 once more) and the next one is skipped that tick.
- **Counted attacks** (`tests/ntimes.rs`): a musketeer ordered to fire twice counts down
  after the first shot and stops after the second although the target lives; a pikeman
  with one strike stops after it, and switching its target keeps the count.
- **Formation tests** (`tests/formation.rs`): forming takes exactly the shape's slot count
  of free soldiers within 1024 px (14 are not enough, units 1100 px away do not count), the
  drummer is required unless the officer has none, a flag bearer near adds its bonus; the
  normal bonuses `n2`/`n4`, standing ground and 25 city ticks of standing still give
  `n1`/`n3`, a move ends it; re-forming as a `#KARE` and back keeps the soldiers; filling
  replaces fallen soldiers by free ones near; another player cannot fill the formation.
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
- **Enemy armies test** (`tests/armies.rs`): two touching cells of 5 and 4 units and a single
  unit next to them form one army, two units far away another, units without a counted
  `USAGE` are left out; nation 1 researches 8 ticks later; allies share an `EnemyInfo`.
- **AI placement test** (`tests/aiplace.rs`): the centre is the peasants' mean, a house is
  placed near it, a second keeps its build bar clear of the first, one project at a time.
- **Switch tests** (`tests/switches.rs`): a stopped barracks keeps its queue and produces
  after resuming; the canister buttons set the stance and attack 256 px ahead.
- **Ship tests** (`tests/ships.rs`): a ship accelerates by 6 per tick (overshooting once)
  to `2·step` and arrives; a ferry ordered onto the shore docks with its land point on land,
  a soldier boards it, it sails along the shore, docks again and lets him off on the far
  shore; a soldier exactly 16 px from the landing point boards too. **City test** (`tests/city.rs`): the gathering speed is `amount·25/dt` after
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

- Motion styles 1 and 4 (`SLOWROTATE`, `ROTATE&MOVE`).
- What runs the brigade order list each tick, and `0x41d090` at the end of `PlaceBrigade`.
- The exact `Rarr` push tables of `RemoveFools`, `ApplyOffset`, the 10-ring spiral table.
- The first move of a unit standing on a locked cell (0x538c4e) and the repulsion vector
  OB+0xd4/+0xd8 of the other motion variants.
- Whether `LeftLeg` starts at 0 for map units (the first cycle then plays `#MOTION_R`).
