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
| cannonball | off at the hit level 85 (e ≥ 30), crushed fracture otherwise | e ≥ 60: torn in two, e ≥ 90: pulverized; else chest crushed (kill) or belly open | headless |
| shell (explosion) | 1–3 fragments: wound / fracture / off by distance | belly open 20 | kill |
| sabre (cavalry melee) | off hand/forearm 20, deep wound 60, fracture 20 | wound, belly open 10 | kill 50 |
| bayonet / pike | wound 85, fracture 15 | deep wound, belly open 20 | kill |
| thrown axe, tomahawk, spear | hand/foot off 12, deep wound 70, fracture 18 | deep wound, belly open 10 | kill 60, else deep wound |
| claw (bear, jaguar) | e ≥ 80: arm off at the forearm or above the elbow 15 (arms only); deep gashes 65, fracture 20 (open half), wound 15 without the arm off | deep gashes; belly or pelvis torn open 35 (e < 80: 12), chest 10 | face and scalp torn (deep wound); kill 25 (e < 80: 10) |
| bite (alligator, jaguar, bear) | e ≥ 90: off at the bite 30; crushed: open fracture 40, closed 25, deep wound the rest | chest crushed (kill) 20 at e ≥ 90; belly open 25; else deep wound | neck or skull: kill 70, else deep wound |
| gore (elk, bison) | deep puncture 60, closed fracture 25, open 15 | belly or pelvis punctured open 45 (e < 80: 25); chest: kill 15 at e ≥ 80, else deep wound | kill 40, else deep wound |

The values are constants in `body.rs` and tests pin their spread.

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
- **Mist:** for pulverized and point-blank cannonball hits, a short-lived red particle burst
  that drifts and settles as fine spray on the ground and on nearby troops (the spray of
  #239).
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
