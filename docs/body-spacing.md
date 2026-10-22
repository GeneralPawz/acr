# Body spacing (optional rule)

The original engine has no body-to-body separation. It keeps at most one unit per 16 px
lock cell (the occupancy grid, `World::occupancy`), but two units in neighbouring cells can
stand a pixel apart, and units walking through locks (out of a building, off a ship,
panicking) ignore the grid. With 2D sprites that hardly shows; with full-size 3D models a
crowd of men standing inside each other does.

The optional rule `body_spacing` (`acr_sim::Rules`, setting `body_spacing=on`) gives men
and animals round bodies and horses long ones (capsules) that push each other apart. With
it off nothing runs, no
state changes and every pinned hash stays as it was (the rule is hashed only when on,
like `hit_locations`).

Code: `crates/acr-sim/src/spacing.rs`, called once per tick in `World::step` after the
units moved. Tests: `crates/acr-sim/tests/spacing.rs`.

## Bodies

World pixels are converted at the models' scale, 30 px per metre
(`acr_render::view3d::PIXELS_PER_METRE`). Men and animals are circles, a horse and rider is
a capsule (a segment along the unit's heading `dir`, swept by a radius):

| body | shape | px | mass |
|---|---|---|---|
| man (infantry, peasants, officers on foot) | circle 0.30 m | radius 9 | 1 |
| horse and rider | capsule 2.4 m × 0.7 m | radius 10.5, segment 2 × 25.5, reach 36 | 8 |
| small animal (jaguar, young of a kind) | circle 0.35 m | 10.5 | 2 |
| elk, bear, alligator | circle 0.50 m | 15 | 4 |
| bison | circle 0.70 m | 21 | 8 |

None: buildings, cannons (`COMPLEXROTATE`), ships, water units, the eagle (it flies), dead
and dying units, units inside buildings or transports.

**Who is a rider** (`spacing::is_rider`): `USAGE` alone misses most riders. Dragoons and
their officers (`ENGD18`, `FRND17`, `SPNKOF18`, ...) are `STRELOK`, the Sioux riders
`SIUKPIK`/`SIUKVOG` `LIGHTINFANTRY`; only cuirassiers and a few others are `HARDHORCE`.
The first version of this rule tested `USAGE` FASTHORSE/HARDHORCE/HORSE-STRELOK and so gave
dragoons a man's 9 px circle, which is why the cavalry frames showed horses inside each
other. All 26 riders of the original data (every `.MD` with the horse sounds `MANHOR` or
`PUSTIHKAKON`) have `GEOMETRY` radius 15, every man 10 (animals 30 and more, handled
first), so a rider is `USAGE` 8/22/24 or `GEOMETRY` radius >= 15. (`combat::injure` still
marks riders as `mounted` by `USAGE` only, so a dragoon hit under `hit_locations` counts as
a man on foot; not changed here.)

**The horse** (`engd18.glb`): the horse mesh is 2.33 m nose to tail, 0.60 m across the
body, the saddle 0.72 m. The capsule is 2.4 m × 0.7 m: radius 0.35 m, segment 1.7 m,
centred on the unit's position (the mesh centre is 0.1 m ahead of it, ignored).

**The man: 0.30 m, kept** (`engm18.glb`, bind pose): the torso (waistcoat, breeches) is
0.40–0.45 m across, shoulders with the arms hanging about 0.55 m; coat front to the
knapsack 0.43 m deep (cartridge box and pack 0.24 m behind the spine). A 0.6 m circle
covers that. 0.35–0.40 m (21–24 px apart) would fight the engine's own spacing: the
occupancy grid holds one man per 16 px cell, and a crowd pressing on it stands at a 16 px
pitch (22.6 px diagonally), so bodies further than 16–18 px apart could only be kept by
pushing men out of their cells against the grid every tick; and the 32 px infantry slots
leave a margin of 14 px at 0.30 m but only 8–11 px at 0.35–0.40 m for the 6 px keep
tolerance and a man stepping. What still looks tight in a dense crowd is the musket held
at the shoulder or levelled (1.4 m long), which no round body can cover.

## Formations and group moves

`ORDERS.LST` slots (infantry under officers) are 32 px (1.07 m) apart in both directions
(`formation.rs`, test `positions_are_32_px_apart_facing_east`). Two men need 18 px, so
infantry keeps its original spacing and never fights the push (the keep-positions tolerance
is only 100/16 px, so a push out of the slot would make the brigade re-send the man every
few ticks).

Cavalry does not form brigades; a selected squadron is placed by the group move
(`motion::place`, `Place` 0x4ab607): a grid `2·√n` wide across the way and `√n/2` deep,
with a pitch of 4 × `GEOMETRY` radius in both directions (with mixed types at most
4 × 16). Measured: men 40 px, riders 60 px (2.0 m). Across the way 60 px is plenty for a
0.7 m (21 px) horse; along the way the riders face the direction of travel and a horse is
72 px long, so the original rows overlap by 12 px nose to tail (test
`a_squadron_keeps_its_horses_apart`, rule off: horses 20 px inside each other). **With the
rule on only**, rows of a group that holds a rider are `spacing::RIDER_ROW` = 76 px apart
(the horse's 72 px and 4 px); across stays 60 px. Turning in place in such a grid needs
the reach (36 px) plus a neighbour's half width (10.5 px) across, 46.5 < 60, and twice the
reach along, 72 < 76, so a squadron can wheel without its horses sweeping through each other.

## The push

Per tick:

1. Bodies are gathered in unit order; a spatial hash with 64 px cells is built by sorting
   `(cell, body)` keys. A body whose reach is over half a cell (a horse, 36 px) checks the
   5 × 5 cells around it, every other body 3 × 3 (man–horse at most 45 px apart, bison–horse
   57, horse–horse 72 < 128). Each pair once: by the lower index, or by the only one of the
   two that searches far enough. O(n log n) for the sort, O(n) pairs.
2. The distance of two bodies is that of their segments (a circle's segment is a point):
   point to segment by the clamped projection, segment to segment as the smallest of the
   four end-point-to-segment distances in a fixed order, plus a crossing test by cross
   products (crossing segments are pushed by `radii + distance`, centre to centre). All in
   i64 1/16 px; two circles skip the segment math.
3. For a pair closer than the sum of radii, the overlap is shared by mass (a body moves by
   `overlap · m_other / (m_a + m_b)`; a horse moves a man 8/9 of it) and only half of it is
   applied per tick (soft, no jitter). Two bodies on the same point split along one of 8
   fixed directions chosen from their indices.
4. Horse against horse, the push is turned across the first horse's heading: the lengthwise
   part keeps a quarter, and nose to tail (less than 3/8 across) the horses step aside to
   the side the other one is on (on the axis: chosen by the indices). Riders side by side
   spread, and a column does not shove its front rank along.
5. Each body's summed push is capped at 2 px per tick and applied if it keeps the body off
   locked map cells; otherwise only its x or y part, otherwise nothing. A standing unit
   that holds its occupancy cell takes the new cell's reservation along, only into a free
   cell; a walking unit keeps the cell it reserved for the end of its step.

All of it is integer math in 1/16 px, in stable order.

Enemies are pushed like friends: men in melee end up at body contact (18 px), inside every
melee weapon's reach, so the fight goes on; they are no longer inside each other.

## Units out of a building

Produced units walk out with `UnlimitedMotion` (through locks and units) until their
`ClearUnlimited` order. Units in that state have no body here: they neither push nor are
pushed, so they come out of the door even into a crowd. Once they take a cell they are
ordinary bodies and spread out (test `units_out_of_a_building_are_not_stuck`).

## Limits

- The occupancy grid stays the original's: a pressing crowd keeps close to its 16 px cell
  pitch, so a few pairs end at 14..16 px instead of 18 (200 men converging on one point:
  206 pairs under 16 px with the rule off, 8 with it on; smallest gap 1 px off, 14 px on).
- Riding through men, a man can be up to about 6 px inside the horse for a tick or two
  (the push is capped at 2 px per tick); standing, men end at the horse's side (test
  `a_horse_pushes_men_aside`; 60 musketeers ridden into by 20 dragoons: at most 2.4 px
  overlap, 19.5 px with the rule off).
- A body is pushed while standing, without a step animation (it slides, at most 2 px per
  tick). The sim does not tell the renderer that a standing unit was displaced; playing
  the walk clip for it would need that per-tick displacement passed to the view.

## Frames

`ACR-analysis/spacing2/{off,on}` (`scenes.py`, blank field, HUD off): `cavalry_slots.png`
(a group move of 20 dragoons), `cavalry.png`/`cavalry_close.png` (the same riding into 60
musketeers of their own side, so nobody fights), `companies*.png` (120 men onto one point),
`squadrons*.png` (two squadrons of 15 passing through each other).

## The blank field and hunger

The blank test field (`acr-game/src/blank_map.rs`) used to start every player with no
resources, like the editor's new map. With no food a player's units starve
(`economy::starve`, 51/32768 per unit and tick): the companies scene lost 26 of 120 men in
400 ticks (93–94 alive, rule off and on alike), which looked like men killing their own.
The blank field now gives every player 1 000 000 of each resource; the same scene keeps
all 120.

## Cost

`cost_of_2000_men_in_one_crowd` (2000 men converging, release build, one desktop core):
about 4.6–5.0 ms per tick for the whole step with the rule off, 6.2–6.7 ms with it on, so
1.2–1.7 ms per tick for the rule in the densest case (the runs are noisy).

## Using it

- Setting `body_spacing=on` in `settings.txt` turns it on for every new game (it goes into
  the game setup, so savegames — bit 1 of the rules word — and restarts keep it).
- Driver: `{"cmd": "start", "blank": {"body_spacing": true}}` or
  `{"cmd": "start", "random": {"seed": 5, "body_spacing": true}}`.
