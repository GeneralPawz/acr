# Body spacing (optional rule)

The original engine has no body-to-body separation. It keeps at most one unit per 16 px
lock cell (the occupancy grid, `World::occupancy`), but two units in neighbouring cells can
stand a pixel apart, and units walking through locks (out of a building, off a ship,
panicking) ignore the grid. With 2D sprites that hardly shows; with full-size 3D models a
crowd of men standing inside each other does.

The optional rule `body_spacing` (`acr_sim::Rules`, setting `body_spacing=on`) gives men,
horses and animals round bodies that push each other apart. With it off nothing runs, no
state changes and every pinned hash stays as it was (the rule is hashed only when on,
like `hit_locations`).

Code: `crates/acr-sim/src/spacing.rs`, called once per tick in `World::step` after the
units moved. Tests: `crates/acr-sim/tests/spacing.rs`.

## Radii

World pixels are converted at the models' scale, 30 px per metre
(`acr_render::view3d::PIXELS_PER_METRE`):

| body | radius | px | mass |
|---|---|---|---|
| man (infantry, peasants, officers) | 0.30 m | 9 | 1 |
| horse and rider (`USAGE` FASTHORSE, HARDHORCE, HORSE-STRELOK) | 0.50 m, one circle | 15 | 5 |
| small animal (jaguar, young of a kind) | 0.35 m | 10.5 | 2 |
| elk, bear, alligator | 0.50 m | 15 | 4 |
| bison | 0.70 m | 21 | 8 |

None: buildings, cannons (`COMPLEXROTATE`), ships, water units, the eagle (it flies), dead
and dying units, units inside buildings or transports.

A horse is about 2.4 m × 0.7 m. One 0.5 m circle is a compromise: two circles along the
body would keep riders further apart nose to tail, but would not fit the 32 px formation
slots (below), and the motion code turns units in place, so a long body would sweep through
its neighbours while turning.

## Formations

`ORDERS.LST` slots are 32 px (1.07 m) apart in both directions (`formation.rs`, test
`positions_are_32_px_apart_facing_east`). Two men need 18 px, two riders 30 px, so every
body fits its slot and the radii were chosen to stay under that: formations keep the
original spacing and never fight the push (the keep-positions tolerance is only 100/16 px,
so a push out of the slot would make the brigade re-send the man every few ticks).
Widening the slots instead would change the formation bonuses' geometry and the original
look of a brigade for no gain.

## The push

Per tick:

1. Bodies are gathered in unit order; a spatial hash with 64 px cells (more than the
   largest pair distance, 42 px) is built by sorting `(cell, body)` keys, and each body
   checks the 3 × 3 cells around it, each pair once. O(n log n) for the sort, O(n) pairs.
2. For a pair closer than the sum of radii, the overlap is shared by mass (a body moves by
   `overlap · m_other / (m_a + m_b)`) and only half of it is applied per tick (soft, no
   jitter). Two bodies on the same point split along one of 8 fixed directions chosen from
   their indices.
3. Each body's summed push is capped at 2 px per tick and applied if it keeps the body off
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
- In the game (frames in `ACR-analysis/spacing/off|on`) the men's models are wider than
  0.6 m across shoulders, pack and musket, so a dense crowd still looks tight; and one
  0.5 m circle is far too small for a horse: cavalry riding into infantry still ends with
  horses inside each other and men under the horses. Two circles per horse (or a capsule)
  and turning that respects them are the next step.
- A body is pushed while standing, without a step animation (it slides, at most 2 px per
  tick).

## Cost

`cost_of_2000_men_in_one_crowd` (2000 men converging, release build, one desktop core):
about 5.0 ms per tick for the whole step with the rule off, 6.2 ms with it on, so about
1.2 ms per tick for the rule in the densest case.

## Using it

- Setting `body_spacing=on` in `settings.txt` turns it on for every new game (it goes into
  the game setup, so savegames — bit 1 of the rules word — and restarts keep it).
- Driver: `{"cmd": "start", "blank": {"body_spacing": true}}` or
  `{"cmd": "start", "random": {"seed": 5, "body_spacing": true}}`.
