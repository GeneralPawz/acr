# Damage model: injuries, split bodies and debris (#262)

Part of the realism work (#214). Everything here applies only with the optional rule
`hit_locations` (`acr_sim::Rules`, setting `hit_locations=on`). With it off the game is the
original: no extra random numbers, unchanged state hash.

The goal is damage that is never the same twice. A hit lands somewhere on a limb, not on a
fixed piece. It can wound, cripple, break and misplace, or take the limb off at any point:
the foot, the lower leg halfway, above the knee. Heavy blows tear bodies in two, open the
belly or leave nothing but chunks and mist. What a man can still do follows from what is
left of him.

The work splits into three layers with narrow contracts between them:

1. **Simulation** (`acr-sim`, deterministic, hashed, saved through the replay): the state
   of each body and the outcome of each hit. It drives gameplay: speed, crawling, weapons,
   bleeding, death.
2. **Assets** (`assets/`, Blender): per-vertex limb coordinates, limb radius profiles, a gib
   pack and crawl clips.
3. **Rendering** (`acr-render`, `acr-game/src/ingame`): bodies cut at any level, stump
   caps, bent fractures, detached pieces, guts, dropped weapons, smears and mist. This is
   render-only: placement comes from hashes of the hit and unit, never from the sim's RNG.

## 1. Simulation: `acr_sim::body`

### Body layout

A body has a trunk and four limbs. Each limb is one chain measured by a **level** from its
root (shoulder or hip, 0) to its tip (fingertips or toes, 255), so any point on it is one
byte:

| Limb | 0 | ~95 | ~120 | ~200 | ~215 | 255 |
|---|---|---|---|---|---|---|
| arm (upper arm, forearm, hand) | shoulder | | elbow 110 | | wrist 205 | fingertips |
| leg (thigh, shin, foot) | hip | knee 120 | | ankle 215 | | toes |

The segment boundaries are the constants `ELBOW`, `WRIST`, `KNEE` and `ANKLE`. The
renderer maps levels to bones with the same table (`render/body.rs`), and the asset bake
writes the same coordinate (`_LIMB`, below).

The trunk has the regions head, chest, belly and pelvis.

### State: `Body` (replaces `Wounds`; field `Unit::body`)

```text
Limb {
    lost: u8,                        // 255 = whole; else the limb ends at this level
    fracture: Option<Fracture>,      // { level: u8, kind: Closed|Open|Dislocated, bend: i8 (degrees/2) }
    wounds: u8,                      // flesh wounds (count, saturating)
}
Body {
    limbs: [Limb; 4],                // arm_l, arm_r, leg_l, leg_r
    belly_open: bool,                // guts out: mortal, heavy bleeding
    head_wounds: u8, chest_wounds: u8,
    burn: u8,                        // seconds of burning left (ground fires, #256)
    down: u8,                        // ticks left knocked down by an animal (mauling)
}
```

- `Hash` hashes nothing while the body is default, so the original hash is kept.
- Death keeps the body: `DeathRecord::body` is a snapshot, plus `torn: Option<Torn>`:
  - `InTwo { level }`: split through the trunk;
  - `Pulverized`: nothing whole left;
  - `Headless`.

### What a body can do (derived, pure functions)

- **Leg function** (0..100), per leg:
  - whole: 100;
  - each flesh wound: −35, floored at 30;
  - a dislocated knee or ankle: 20;
  - a closed fracture of the thigh or shin: 0;
  - an open fracture: 0;
  - foot lost (lost ≥ ANKLE): 15, hobbling on the stump;
  - lost above the ankle: 0.
- **Moving:**
  - Both legs above 0: walk at `min(leg) × (0.6 + 0.4 × max(leg) / 100)` percent.
  - Otherwise crawl, at a speed set by the arms:
    - two working arms: 15 %;
    - one working arm: 8 %;
    - none: 0, which means he can only writhe.
- **Arm function**, per arm:
  - whole: 100;
  - each flesh wound: −40;
  - fracture or dislocation: 0;
  - hand lost (lost ≥ WRIST): 0;
  - anything shorter: 0.
- **Weapons:**
  - A long weapon (musket, pike) needs both arms above 0. If either drops to 0 the
    weapon is **dropped**; `Body::dropped` is set once and the renderer leaves it on the
    ground.
  - A one-handed weapon (sabre, pistol) needs one working arm.
  - A crawler always drops his long weapon.
- **Out of the fight:** a crawler, or a man with no working arm:
  - takes no orders;
  - attacks nothing;
  - drags himself away from the nearest enemy towards his own side;
  - does not count for formations.
- **Bleeding** (life per second, summed):

  | Wound | Life per second |
  |---|---|
  | stump above the knee or elbow | 6 |
  | stump below the knee or elbow | 4 |
  | hand or foot lost | 2 |
  | open fracture | 2 |
  | belly open | 5 |

  Dying of it gives `DeathCause::BledOut`.

### Critical hits and men on the ground (`docs/melee.md`)

After the outcome a man struck by a man rolls once for a **vital zone** of the struck part
(brain, neck, heart, lung, liver, spine, femoral or brachial artery) by strike kind
(thrust, cut, smash, shot), doubled against a man on the ground. A brain hit and most
heart hits kill; the others set a timed bleed-out (`Body::bleed_out`, seconds), a lung hit
collapses him (`Body::collapsed`) and a spine hit paralyses (`Body::spine`: legs, or all
four limbs). Both count as legs at 0 (he crawls), a high spine hit also as arms at 0 (he
writhes). Men on the ground are targets after the standing ones, within reach and by at
most two enemies; blows at them come down (`HitRecord::strike`). Tables and the contract
are in `docs/melee.md`.

### Outcomes: `body::injure(w, victim, cause, energy, region_hint)`

Each limb hit draws from the world RNG (`rando()`), only with the rule on:
- the limb (by region weights, or the projectile's height: a low rolling ball takes the
  legs);
- the level within the struck segment;
- the outcome, from this table (percentages per hit; energy 0..100 from the projectile
  or plough, see #258; a musket ball's from the range: full within 200 px, −1 per 10 px
  beyond, so close range is within about 450 px). 18th-century balls are soft, heavy and
  slow: they shatter bone and, close up, tear chunks (#277):

| Cause | Limb hit | Trunk hit | Head |
|---|---|---|---|
| musket ball (close, e ≥ 75) | fracture (open 2/3) 45, off at hand/forearm/foot 6, wound 49 | wound, belly open 8 | kill |
| musket ball (far) | fracture (open 2/3) 45, hand/foot off 2, wound 53 | wound | kill |
| grapeshot | wound 40, fracture 35, off below elbow/knee 25 | wound, belly open 10 | kill |
| cannonball | e ≥ 40: legs: torn in two at the hips (both legs stay, the torso is thrown); arms: on through the chest (as a trunk hit there). Spent (e < 40): off at the hit level 85 (e ≥ 15), crushed fracture otherwise | e ≥ 70: pulverized 65, torn in two 35; e ≥ 40: torn in two; else chest crushed (kill) or belly open | headless |
| shell (explosion) | 1–3 fragments: wound / fracture / off by distance | belly open 20 | kill |
| sabre (cavalry melee) | off hand/forearm 20, deep wound 60, fracture 20 | wound, belly open 10 | kill 50 |
| bayonet / pike | wound 85, fracture 15 | deep wound, belly open 20 | kill |
| thrown axe, tomahawk, spear | hand/foot off 12, deep wound 70, fracture 18 | deep wound, belly open 10 | kill 60, else deep wound |
| claw (bear, jaguar) | e ≥ 80: arm off at the forearm or above the elbow 15 (arms only); deep gashes 65, fracture 20 (open half), wound 15 without the arm off | deep gashes; belly or pelvis torn open 35 (e < 80: 12), chest 10 | face and scalp torn (deep wound); kill 25 (e < 80: 10) |
| bite (alligator, jaguar, bear) | e ≥ 90: off at the bite 30; crushed: open fracture 40, closed 25, deep wound the rest | chest crushed (kill) 20 at e ≥ 90; belly open 25; else deep wound | neck or skull: kill 70, else deep wound |
| gore (elk, bison) | deep puncture 60, closed fracture 25, open 15 | belly or pelvis punctured open 45 (e < 80: 25); chest: kill 15 at e ≥ 80, else deep wound | kill 40, else deep wound |

The values are constants in `body.rs` and tests pin their spread.

### Shell fragments (`acr_sim::shrapnel`)

With the rule on, a shell (`IADROMOR`, `VAFLIA`) bursting on the ground also throws 14–26
fragments (sim RNG, once per shell and tick) on top of the original's round damage, which
stays as it is (with the rule off nothing here runs: no draw, unchanged hash):

- each flies in a random direction for 40–100 % of `REACH` (210 px, about 7 m; at least
  twice the blast radius) and strikes the first man or animal within `BODY` (8 px) of its
  line; buildings, guns and ships are not struck;
- its mass is 1..100, squared towards small (mostly splinters, now and then a large piece);
- energy = `(25 + 3/4 mass)`, falling to half at the end of its flight; damage
  `1 + mass × energy / 200` (1 for a splinter, about 50 for a large piece close by);
- the hit goes through `MakeDamage` with `World::hit_energy` and the fragment's direction,
  so region, outcome and critical roll as for any hit (`Cause::Shell`). The shell row is
  energy-scaled: the flesh-wound share grows by `(100 − e) / 2`, nothing comes off below
  `FRAGMENT_SEVERS` (45), a head hit below `FRAGMENT_HEAD_KILL` (40) is a deep wound and
  the belly opens by `20 × e / 100` %. At energy 100 (the blast itself) the row is unchanged;
- a survivor's wound bleeds (`Body::seep` life per second for `Body::seep_s` seconds,
  counted down in `wounds::bleed`, part of `bleeding()` so blood drops follow him): a
  flesh wound 1/s for 6 s, a deep wound or closed break 2/s for 20 s, an open break, a
  stump or an open belly 1/s more for 40 s (on top of their own bleeding). Hashed only while
  it runs.

`World::fragments` (render only, not hashed) holds each flight; the particles show it as a
short streak of glowing specks, with a blood spurt (and flesh scraps from a large piece)
where it struck. The wounded cry out (`acr-game/src/agony.rs`, `cry_ticks`): a critical
fragment wound (a vital, a limb off, the belly open, an open break) screams and moans on
until he dies, a light one for 3–5 s; they share the four agony voices.

Shell bursts also throw the remains around them (render only, `three_d.rs`, 4 m): up to 12
corpses, nearest first, get a ragdoll (a frozen one wakes, others start from their lying
pose within the awake budget) and fly outwards and up (6 and 5 m/s at the burst, 0 at the
edge); up to 24 lying pieces, gibs and dropped gear are thrown again (`Debris::blast`),
their decay still running from their first fall. Measured (release, blank field, 40
corpses, one burst throwing 12): the blast itself 0.42 ms once; then `rag.physics` 1.7 ms
per frame at the recorder's 4 steps a frame (about 0.45 ms per step with 12 awake), inside
the 4 ms body-physics budget, falling back as they freeze again.

Test driver: `{"cmd":"impact","at":[x,y],"cause":"shell","by":unit}` bursts a shell of
`unit`'s shell attack there (its fragments fly; answers `fragments`).

### A cannonball's grazes and a torn man's bones (`acr_sim::shrapnel::GRAZE`, `BONES`)

AC has almost no shells (the field gun fires roundshot and grapeshot), so what a player sees
"explode" is a solid ball striking the ground. With the rule on that does the small version
of a shell burst (rule off: nothing, no draw, unchanged hash):

- **Graze or bounce** (every ground contact of an `IADRO` round, `projectile::explode`
  with `ground`, once per ball point and tick): 4–9 secondary fragments (`Kind::Earth`:
  clods, stones, wood and bone splinters) within ±110° of the ball's flight, flying
  40–100 % of 105 px (3.5 m), mass up to 30, punch 70 % of a casing fragment of that
  mass. Mostly flesh wounds (energy about 10–35), seeping and screaming as for shells.
- **A man pulverized or torn** by a ball (`body::ball_through`, any `Torn` outcome of a
  `Cannonball` hit, `shrapnel::ball_hit` after each ball hit in `round_damage`): 2–5 of his
  bones (`Kind::Bone`) fly on from where he stood within ±80° of the ball's flight, 75 px
  (2.5 m), mass up to 25, punch 90 %, at the men right behind and beside him (not him).

Both strike through `MakeDamage` like shell fragments; `World::hit_cause` makes the hit a
shell's (`DeathCause::Explosion`, `Cause::Shell`) although the sender's weapon is the
cannon, so the energy-scaled shell row, the seeping and the cries apply unchanged.
`Fragment::kind` tells the renderer what flew: brown clods and pale bone splinters
rising from the ground instead of glowing casing specks.

Each graze also throws the remains lying there (render only, `three_d.rs`, the earth
fragments' burst points): within 82 px (2.75 m) at most 4 corpses and 10 pieces, nearest
first, at 0.65 of a shell's force, so the dead are flung with the mud.
Measured (release, `ACR_PROFILE`, blank field, 40 corpses and a line of 9 men, three
grazes 100 px apart along the ball's line, each throwing 4 corpses): the blast itself
0.16 ms per graze; over the 72 frames after them `3d.update` 1.65 ms per frame (0.23 idle)
of which `rag.physics` 1.2 ms (12 corpses awake), `sim.step` 0.065 ms per tick (0.029
idle); back to idle as the corpses freeze again.

Test driver: `{"cmd":"command","command":{"type":"attack_ground","units":[gun],"x":..,"y":..}}`
bombards a ground point (`ballgraze.py` in the analysis folder).

**Cannonball energy (#262, "a direct hit should shred the troop").** A 6 to 12 pdr roundshot
keeps lethal energy over its whole flight (hundreds of metres, bounces included) and through
several men, so the energy at a man is what the bodies before him left of the ball
(`projectile::Plough`: 100, 80, 60, 40, 20 % for the 1st to 5th body), not the range. Before,
the table took a limb off and left the rest whole for leg and arm hits (45 % of the regions,
80 % legs for a low rolling ball) and for chest hits below e 60, so most direct hits
"tumbled over in one piece". Now (`BALL_SEVERS` 15, `BALL_TEARS` 40, `BALL_PULVERIZES` 70,
`BALL_PULVERIZE_SHARE` 65, `BALL_HIPS` 236, `BALL_CHEST` 70; `body::ball_through`):

| Body along the ball | Energy | Trunk | Legs | Arms | Head |
|---|---|---|---|---|---|
| 1st, 2nd | 100, 80 | pulverized 65, torn in two 35 | torn at the hips | pulverized 65, torn across the chest 35 | headless |
| 3rd, 4th | 60, 40 | torn in two | torn at the hips | torn across the chest | headless |
| 5th (spent) | 20 | chest crushed or belly open | off 85, crushed fracture 15 | off 85, crushed 15 | headless |

Only men (an animal's limb keeps the limb table). The same draws as before, so the rule off
keeps its hashes.

### Mauling (#276)

Animals have no weapon of their own in the original (`WEAPONKIND MECH`, so their hits carry
`DeathCause::Melee` in `HitRecord::cause`). With the rule on, `combat::injure` asks
`body::maul` first: an attacker whose `.MD` name starts with `Anm` is a `Beast`, and its
blow becomes a claw, a bite or a gore with the beast's own energy and aim. `Injury::cause`
tells the renderer which (gashes, a bite, a puncture).

| Beast (`.MD` name) | Cause per attack | Energy (the young `…Sm`, `AnmOlnS`: 2/3) | Regions torso / legs / arms / head (victim down) |
|---|---|---|---|
| bear (`AnmMed*`) | claw 70, bite 30 | claw 100, bite 80 | 40/10/30/20 (45/5/20/30) |
| jaguar (`AnmJug`) | claw 50, bite 50 | claw 75, bite 85 | 30/10/25/35 (25/5/20/50) |
| alligator (`AnmCro*`) | bite | 95 | 20/55/20/5 (35/30/20/15) |
| elk (`AnmOln*`), bison (`AnmBiz`) | gore | 80, 100 | 70/25/5/0 (75/20/5/0) |
| any other `Anm*` (eagle) | claw | 40 | 30/10/30/30 |

Each animal blow draws two more `rando()` (cause and region) after the usual region draw;
the hit record's region is the injury's. Without the rule nothing changes: no draw, plain
melee damage.

**Knocked down** (`Body::down`, ticks left, hashed with the body): a claw from energy 70
keeps him down 40 ticks, a gore 50, a bite 30 ticks in 60 % of bites from energy 90 (else
30 %). A new mauling hit renews it (the longer of the two). Down, he cannot move
(`walk_rate` 0), attack (`attack_obj` refuses; a running attack order waits) or hit back,
so the animal keeps striking the man on the ground and goes for the head, neck and belly
(the regions in brackets). `body::get_up` counts it down once a tick (`World::step`, after
the bleeding). The renderer reads `unit.body.down` (> 0: lying).

### Animals as victims (#276)

An animal hit with the rule on gets a body of its own (`Body::animal`, set at its first
injury from `Beast::size`: bison 4, bear and alligator 3, elk 2, jaguar and eagle 1, the
young one less; hashed only when set, so men hash as before). The hit is rolled on the
tables above, then `body::beast_outcome` turns it into an animal's:

- no ball, blade, claw or jaw takes a leg off (the limb is shattered open or deeply
  wounded, by the roll's parity); cannonballs and shells still do;
- a head hit that would kill kills by 100 / size percent (a decorrelated roll: an animal's
  head hit always draws one), else it is a deep wound;
- from size 3 a ball that would tear the body in two or pulverize it kills it instead.

Its four legs are the four limbs (the front ones in the arm slots). It never crawls, drops
nothing and is never knocked down. Speed (`move_percent`): 60..100 % by its worst leg's
function, 45 % on three legs (`BEAST_LIMP`, `Body::limping`), 10 % on two; an open belly
halves it. It flees (`Body::flees`, so `out_of_fight` and `leave_fight`: it drops its
orders and runs from the nearest enemy) with an open belly, more trunk and head wounds than
its size, or 2 + 2 × size wounds in all. It bleeds `bleeding() × size` a second: the same
share of a body four times a man's.

A hit that kills still records where and how it struck. `DeathRecord` carries the body and
`torn`, so a corpse shows its last injury, not just its earlier ones.

`HitRecord::injury: Option<Injury { limb_or_region, level, outcome, energy }>` is how the
renderer learns what to spray and throw.

## 2. Assets

On top of the existing contract (`docs/assets-humans.md`):

- **`_LIMB`** (float, per vertex): the limb level 0..1 (level / 255) along its chain. It is
  baked from the projection of the rest position onto the chain of shoulder to elbow to
  wrist to fingertip, or hip to knee to ankle to toe. Trunk vertices are −1. Gear strapped
  to a limb (cuffs, gaiters, boots) gets the level of the skin beneath it.
- **Limb radius profiles:** glTF extras `acr_limb_radius`: per limb, 16 samples of the
  outer radius (metres) along the level. Caps are scaled with them.
- **Gib pack `assets/gore/gore.py` → `gore.glb`:** separate meshes with the decay material,
  so they rot with the corpse:
  - intestine coils (3 variants, spline tubes);
  - a spilled-guts heap;
  - organ chunks;
  - flesh lumps (4);
  - bone shards (4);
  - a skull fragment;
  - a jaw;
  - cloth scraps in the uniform colour (tinted per nation);
  - a **stump cap** (a disc: skin rim, red flesh, white bone centre, ragged outline; unit
    radius);
  - a **torn edge** strip.
- **Crawl clips** on the human units:
  - `CRAWL2`: face down, alternating elbows pull, legs drag;
  - `CRAWL1`: one leg pushes, both arms pull;
  - `CRAWLARM`: one-armed drag;
  - `WRITHE`: on the ground, unable to move.

  All four loop. They are keyframed now; recorded drills (#167) replace them later.

## 3. Rendering

- **Cutting:** per instance, a damage block lives in a data texture next to the bone
  palette. The vertex slots are full (16), so per-instance wound data moves there too. It
  holds:
  - per limb: cut level, fracture level and bend (quaternion);
  - a trunk cut level;
  - flags: head gone, belly open;
  - a keep mode: Body / Piece(limb, from, to).

  The shader:
  - discards vertices past a cut, with a ragged edge from `_GORE.x`;
  - bends vertices past a fracture about the fracture point, in rest space before
    skinning;
  - in Piece mode, keeps only the cut-off span.
- **Caps:** the cap mesh is placed at each cut, facing along the limb and scaled by the
  radius profile at the level. There is one cap on the body side and one on the piece's.
  The trunk cut uses the torn-edge strip around the waist. The old joint-only caps
  (`_PART` 16+/32+) are dropped.
- **Pieces:** a severed limb is a second instance of the same model in Piece mode, posed as
  a limp limb. It is thrown on a short ballistic arc from the hit along the blow (hashed
  spread), then lies with a blood pool. A body torn in two is two instances. A pulverized
  body is a mist burst plus gibs.
- **Debris** (render-only lists like the corpses, ageing with `corpse_minutes`):
  - severed pieces;
  - gibs;
  - guts spilling from an open belly, attached at the belly and lying out on the ground;
  - dropped weapons: the musket lies where it fell;
  - hats.
- **Crawlers:** use the crawl clips, face down, head first along the movement, and leave a
  **smear**: a continuous ribbon the width of the body with ragged edges and drag streaks.
  It is denser where they are slow, has a wider patch where they pause and a pool where
  they die, and dries from glossy red to brown-black.
- **Mist:** for pulverized and point-blank cannonball hits (e ≥ 70), a short-lived red
  particle burst that drifts and settles as fine spray on the ground and on nearby troops
  (the spray of #239). Every body torn by a ball (pulverized, torn in two, a limb or the
  head torn off) also throws particles (`particle_view::shred`, full gore only): a red mist
  of soft puffs, blood drops flung along the flight, and scraps of flesh and bone
  (`ParticleKind::Gore`, glossy lumps that lie 20 s without marking the ground); a
  pulverized body twice as many. The torn-in-two lower half (the legs, after a ball at the
  hips) stays on the ground; the upper half is thrown.
- **Gore setting:**
  - `full`: everything;
  - `reduced`: no guts, gibs or mist, and pieces without caps' flesh;
  - `off`: the original.

## Slices

| Slice | Content | Depends on |
|---|---|---|
| S1 | `acr_sim::body`: state, outcomes, effects, out of the fight, death snapshot, tests, docs | — |
| S2 | assets: `_LIMB`, radius profiles, gib pack with cap, crawl clips | — |
| S3 | renderer: instance data texture, cut/discard, caps, fracture bend, Piece mode, debug fixture | S2 (attribute names only) |
| S4 | smear trail of crawlers | — |
| S5 | game: body → instances; pieces, gibs, guts, dropped weapons, mist; crawl clips | S1, S2, S3 |
| S6 | death falls by region and cause; wounds placed at the hit | S1, S3 |
| S7 | horses: wounded falls, riders thrown; limbs | S5 |
