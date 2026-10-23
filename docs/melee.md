# Melee: men on the ground, strike kinds and critical hits

Part of the realism work (#214), on top of the damage model (`docs/damage-model.md`, #262)
and the body simulation (`docs/body-sim.md`, #276). Everything here applies only with the
optional rule `hit_locations`. With it off nothing changes: no extra random numbers, no
change in targeting, `HitRecord::strike` and `critical` stay `None`, and the pinned
original hashes (`tests/wounds.rs`, `tests/plough.rs`) hold.

Code: `crates/acr-sim/src/melee.rs`; wiring in `combat.rs` (`injure`, `search_victims`,
`enemy_in_cell`, `point_blank`), the timed bleeding in `wounds.rs` (`bleed`). Tests:
`melee.rs` unit tests and `tests/melee.rs`.

## 1. Men on the ground as targets

A man is **on the ground** (`melee::on_ground`) when he crawls (a leg gave out, a lower
spine hit, a lung hit), writhes (no working arm either) or is knocked down by an animal.
Riders and animals are never on the ground in this sense.

Such men could always be attacked (`Unit::active` ignores the body); `attack_obj` only
refuses *attackers* who are out of the fight or down. What the rule adds is how the
automatic search (`SearchVictim`) picks them:

- **Standing threats first.** In a cell the search takes a standing enemy (or a building)
  before a man on the ground. Across cells a man on the ground counts as
  `DOWN_OFFSET` (5 000 000) further off, so any standing enemy in the search square wins;
  the offset is taken off again before the search radius test.
- **Only within reach.** A man on the ground is a candidate only within `DOWN_REACH`
  (150 px) of the searcher: men finish off the wounded next to them, they do not walk or
  shoot at crawlers further away.
- **No swarming.** At most `DOWN_ATTACKERS` (2) enemies go for one man on the ground at a
  time. The counts are built once per tick from the attack orders and grow as the search
  hands out new targets.
- An attack order on a man who goes down keeps running: the attacker finishes him.

## 2. The blow at a man on the ground

- **Region.** A blow by blade, ball, arrow or thrown weapon (not cannon or shell) at a man
  on the ground draws its region anew from `DOWN_REGIONS`: torso 55, legs 10, arms 15,
  head 20. The arms come up to shield; the trunk and head lie open.
- **Height** `Down`, and the vital chances are doubled (`DOWN_VITAL`, below).
- **Musketeers** (a firearm `VISTREL*` index and a melee index): at the active frame of a
  melee strike at a man on the ground, a **loaded** musket (no reload running) and two
  working arms fire **point-blank** instead: the firearm's damage and weapon (cause
  `Gunshot`, energy 100, strike `Shot`/`Down`), and the reload (`ATTACK_PAUSE` of the
  firearm × personal delay) starts. Unloaded, he uses the bayonet (`Thrust`/`Down`).
- **Officers** cut standing men and stab down with the point at men on the ground.
- **Riders** (`spacing::is_rider`: USAGE FASTHORSE, HARDHORCE, HORSE-STRELOK, or a horse-
  sized body, so dragoons, their officers and native riders): a lancer (a `.MD` name with
  `lan`, `lnc`, `kop`, `pik` or `spear`) thrusts, wounding as a pike (`body::Cause::Bayonet`),
  the others cut with the sabre. Against a man on the ground the height is `Down`, with
  `mounted: true`: the animation thrusts or cuts down from the saddle.

## 3. Strike kinds

| Weapon (`NRES.DAT` kind) | Attacker | Kind |
|---|---|---|
| `PIKA`, `KOPIE` (pike, spear) | any | Thrust |
| `TAMAGAVK` | any | Cut |
| `KAMEN` | any | Smash |
| `MECH` | rider, lancer | Thrust |
| `MECH` | rider | Cut |
| `MECH` | officer | Cut; Thrust at a man on the ground |
| `MECH` | has a firearm index (bayonet) | Thrust |
| `MECH` | peasant | Smash |
| `MECH` | anyone else (tomahawk, sword) | Cut |
| any | an animal (`Anm*`) | Bite (claws, jaws and horns alike) |
| anything else (balls, arrows, grapeshot, shells) | any | Shot |

Height: `Down` at a man on the ground; otherwise by the region struck: head `High`, torso
and arms `Mid`, legs `Low`.

Scripted hits (`combat::scripted_hit`, the bench) get a strike from their cause (melee:
Thrust, mounted: Cut, thrown: Cut, maul: Bite, else Shot) but roll no critical.

## 4. Critical hits: vital zones

After the injury is rolled on the tables of `docs/damage-model.md`, a man struck by a man
draws one more `rando()` (`melee::critical`): the vitals of the struck part are tested in
order, each with its chance; at most one is hit. Animals (as victims or attackers) and
bodies torn apart roll nothing.

Chances in percent against a man on his feet:

| Part | Vital | Thrust | Cut | Smash | Shot |
|---|---|---|---|---|---|
| head | Brain | 35 | 10 | 30 | 0 |
| head | Neck | 20 | 30 | 0 | 12 |
| chest | Heart | 12 | 2 | 0 | 8 |
| chest | Lung | 18 | 5 | 4 | 15 |
| chest | Spine | 3 | 2 | 2 | 3 |
| belly | Liver (and kidneys) | 20 | 8 | 4 | 14 |
| belly | Spine | 4 | 2 | 2 | 4 |
| pelvis | Femoral (groin) | 8 | 6 | 0 | 6 |
| pelvis | Spine | 3 | 1 | 2 | 3 |
| thigh (leg level < `KNEE`) | Femoral | 10 | 15 | 0 | 8 |
| upper arm (arm level < `ELBOW`) | Brachial | 8 | 14 | 0 | 6 |

Thrusts reach deep vitals, cuts open arteries and limbs, balls go anywhere. A musket ball
to the head already kills by the damage model, so its brain column is 0 (the head kill
stays a plain kill).

Modifiers: × `DOWN_VITAL` (200 %) against a man on the ground; + `OFFICER_VITAL` (20 %)
of the chance for an officer's blow and + `KILLS_VITAL` (2 %) per kill, at most
`KILLS_VITAL_MAX` (20 %); each chance capped at `VITAL_CAP` (90).

What a vital does (the seconds are picked from the range by a second percentage derived
from the same roll):

| Vital | Effect |
|---|---|
| Brain | killed outright (`Outcome::Kill`) |
| Neck | bleeds out in 5–12 s (`NECK_SECONDS`) |
| Heart | killed outright in 60 % (`HEART_KILLS`), else dead in 2–5 s |
| Lung | goes down for good (`Body::collapsed`: legs 0, crawls), bleeds out in 30–60 s |
| Liver | bleeds out in 45–90 s |
| Spine | `Body::spine`: from the chest 2 (all four limbs, he writhes), lower 1 (legs, he crawls on his arms) |
| Femoral | bleeds out in 20–40 s |
| Brachial | bleeds out in 40–80 s |

**Timed bleeding** (`Body::bleed_out`, seconds left; a later vital keeps the shorter):
once a second (`wounds::bleed`) the man loses `life / seconds left` on top of his other
bleeding and the counter goes down; at the last second he dies, `DeathCause::BledOut`. So
the life bar drains evenly and the death comes on time whatever his life.

`Body` hashes `bleed_out`, `spine` and `collapsed` only when one is set, so bodies without
a vital hit hash as before.

## 5. The contract (render side)

```rust
// acr_sim::melee, re-exported from acr_sim::combat
pub enum StrikeKind { Thrust, Cut, Smash, Shot, Bite }
pub enum StrikeHeight { High, Mid, Low, Down }
pub struct Strike { pub kind: StrikeKind, pub height: StrikeHeight, pub mounted: bool }
pub enum Vital { Brain, Neck, Heart, Lung, Liver, Spine, Femoral, Brachial }
// HitRecord gets:
pub strike: Option<Strike>,   // every hit on a body under the rule (melee and ranged)
pub critical: Option<Vital>,  // the vital found, if any
```

As agreed; no change. Additions the renderer can read: `Body::bleed_out`, `Body::spine`,
`Body::collapsed` (a collapsed or spine-hit man crawls or writhes like any crawler;
`melee::on_ground(&body)` tells whether he lies).

## 6. What is not done

- No in-game renders yet (blank-field driver runs).
- The point-blank shot is a direct hit: no projectile, smoke or impact record; the render
  side sees `Strike { kind: Shot, height: Down }` on a melee animation.
- The musket butt (`Smash`) is not chosen by musketeers; the strike kind of `MECH` is by
  unit kind, not by situation.
- Lancers are found by `.MD` name; there is no lance flag in the unit files.
- Scripted hits roll no critical.

## Animation

Render side only (`crates/acr-game/src/ingame/strike_view.rs`, called from `ragdoll_view`
after the gait layer; `ACR_NO_STRIKE` turns the layer off).

### Strike clips

A man playing `ATTACK` (or `ATTACKn`) at a unit plays a strike clip when his model has one
(`strike_view::strike_clip`, picked in `three_d.rs`), else the `.MD` clip. Names, best first:

1. `ATTACK_<KIND>_<HEIGHT>`: kind `THRUST`, `CUT`, `SMASH`, `SHOT` (`BITE` for animals),
   height `HIGH`, `MID`, `LOW`, `DOWN` (`acr_sim::combat::{StrikeKind, StrikeHeight}`);
2. `ATTACK_<HEIGHT>` (by height alone);
3. the `.MD` clip.

Kind and height come from the sim: the latest `HitRecord::strike` of this attacker at this
victim (`last_strike`: a hit on the victim struck from where the attacker stands, `pos -
dir` within 48 px). Before the first blow the kind is unknown and the height comes from the
victim's pose (on the ground `Down`, mounted `High`, else `Mid`); a man on the ground is
always struck `Down`.

**Timing.** A strike clip has the frame count of the `.MD` line `ATTACK` and plays on its
frames. The sim lands the blow at the active frame of `ATTACK` (`combat.rs`, (A)); no unit
we build has a `SETACTIVEPOINT #ATTACK`, so that is frame 1 rounded up to even: **frame 2**
(`poses.STRIKE_HIT_FRAME`). The clips open at the end of the wind-up, hit at frame 2, hold,
recover, and wind up again by the end (`ATTACK` loops while the fight goes on).

Built so far (`poses.musketeer_strikes`, all musketeers built by `musketeer_clips`: EngM18):
`ATTACK_THRUST_MID` (bayonet lunge), `ATTACK_THRUST_DOWN` and `ATTACK_DOWN` (a long step in,
knees bent, the muzzle 48° down), `ATTACK_SMASH_MID` (butt stroke; not chosen by the sim yet),
`ATTACK_SHOT_DOWN` (point-blank from the hip).

### The aiming layer

- **Weapon axis:** from the clip's node transforms (`ragdoll_view::node_frame`): the
  musket's `socket.muzzle` from `socket.hand_r` (axis and reach: 1.44 m for EngM18), else the
  first weapon node under `socket.hand_r` (its local +Y, Blender's +Z) with `REACH` 0.9 m.
  This replaces `slot_direction`, which read an upright musket for a levelled bayonet.
- **Step in:** on foot, when arm and weapon cannot reach the target, the man moves towards
  it by the lack (at most `STEP` 0.5 m), the leading foot along, the other planted (leg IK);
  at a man on the ground he also sinks 0.25 m into his knees.
- **Trunk bend:** as before (lower spine 55 %, chest the rest, 75° in all).
- **Arm IK:** the right hand goes where the weapon's point meets the target (two-bone IK of
  the right arm), then the hand turns the weapon onto the target (at most 90°); a musket's
  left hand follows the stock 0.45 m along it.
- **Riders:** models with `rider.` bones (`rider.hips`) are now passed to the layer
  (`three_d.rs` → `Ragdolls::update(mounted)` → `Strikes::update`), only to it (no ragdoll,
  gait or cloth), mapped by `Mapping::with_prefix`.

Bench: `ACR-analysis/melee-anim2/melee_bench.py` (white blank field, HUD off; the crawling
victim has lost both arms, so he writhes in place). Sheets: `melee-anim2/layer/` (old clips,
new layer), `melee-anim2/clips/` (new EngM18 strike clips).

### Phase, guard and turn (m2-melee-anim3)

- **Arm weight by clip phase** (`strike_view::arm_weight`): on a strike clip the arm IK (and
  the step in at a standing man) weighs 0.35 at the wind-up (first frame), rises to full at
  the hit frame (`HIT_FRAME` 2), holds two frames, eases to 0.15 for the recovery and back
  to 0.35 over the last fifth. The weight is eased per man over time (`ARM_RATE`), so the
  sprite-frame steps do not jerk. The `.MD` clip (no strike clip) keeps full weight.
- **Guard:** against a standing man the point aims `GUARD` (0.45 m) short of him, scaled by
  `1 - arm`: the bayonets stay in front of the men and reach in only with the blow.
- **Hand turn:** at most `MAX_TURN` (30°, was 90°), eased per man (`TURN_RATE`); a butt
  smash (`Smash`) turns nothing and its arm IK weighs 0.3.
- **Riders** bend at most `MAX_BEND_MOUNTED` (46°).
- **Clip fallback:** a blow `High` or `Low` at a standing man falls back to the kind's `_MID`
  clip (`ATTACK_THRUST_LOW` → `ATTACK_LOW` → `ATTACK_THRUST_MID`). Before, a `Thrust`/`Low`
  found no clip and the `.MD` ATTACK played with the layer at full weight.
- `ACR_STRIKE_DEBUG` logs the layer per man and frame (clip, phase, arm weight, weapon).
- **Smash pose** (`poses.musketeer_strikes`): drawn back the muzzle points forward and up
  (socket 62°), at the hit the butt is driven forward, the muzzle tipped back (-28°), with
  little turn about the vertical (was -20/50 → -96/-20).
- **Lance strike clips** (`cavalry.add_clips`, riders with `held: "lance"`: PueVsdA):
  `ATTACK_THRUST_DOWN`, `ATTACK_DOWN`, `ATTACK_THRUST_MID`, `ATTACK_THRUST_HIGH` on
  ATTACK's frames, the blow on frame 2: the lance drawn back high, then driven down past
  the horse's right shoulder (`lance_down`) or level (`lance_mid`).

**Rider debug.** The rider does reach the layer (mounted mapping and weapon found, weight
rises to 1). The EngD18 bench showed no lean because the dragoon *shot* the crawler with his
carbine (`Shot`/`Down`, ATTACK) and killed him within a second; the layer faded with the
victim. With PueVsdA (melee only) the layer bends the rider visibly; at the old 75° limit he
folded flat over the neck, hence `MAX_BEND_MOUNTED`.

Bench: `ACR-analysis/melee-anim3/melee_bench.py` (adds `lance_crawl`, `lance_stand`; crop
taller for riders). Sheets: `melee-anim3/run1` (layer before the fallback fix), `run2`,
`run3` (current: `sheet_foot_stand.png`, `sheet_lance_crawl.png`). Rebuilt on the VM:
EngM18 and PueVsdA, copied to `showcase/models` (old ones in `backup-melee-anim3/`).

### Reach, lance and drawn sabre (m2-melee4)

- **Weapon point:** a held weapon may carry an empty `*socket.tip` at its point (under the
  right hand bone: the lance's head, the drawn sabre). The layer then takes axis and reach
  from `socket.hand_r` to it, like a musket's muzzle but one-handed: the lance reaches
  ~2.3 m instead of `REACH` 0.9 m. A point closer than `HIDDEN` (0.2 m) is a weapon hidden in
  this clip (socket scaled to 0): the socket's axis and `REACH` again.
- **`lance_down`** (`cavalry.make`): the hand turned 65° down (was 40°), the rider leaning
  out over the right side (spine Y -16°, chest -6°) instead of folding over the neck.
- **Drawn sabre** (`drawn: True` in the spec: EngD18): a second right-hand socket
  `socket.blade_r` (a non-deforming bone beside `socket.hand_r`) holds a drawn sabre with its
  `socket.tip`. Every clip keys the scale of `socket.hand_r` (carbine), `socket.hip_l`
  (sheathed sabre) and `socket.blade_r`: `SHOW_REST` (carbine and scabbard shown, blade 0)
  in all clips, `SHOW_MELEE` (blade shown, carbine and sheathed sabre 0) in `ATTACK` and the
  strike clips. Firing clips (`ATTACK1`, `PATTACK1`...) keep the carbine.
- **Sabre clips** (EngD18, on ATTACK's frames, blow on frame 2, from `sabre_up`: the blade up
  and back over the right shoulder): `ATTACK_CUT_DOWN`, `ATTACK_DOWN` and `ATTACK_CUT_DOWN_R`
  (down right of the horse), `ATTACK_CUT_DOWN_L` (across, down left of the neck; nothing
  picks it yet), `ATTACK_CUT_MID` and `ATTACK_CUT_HIGH` (at a standing man).
- **Bench helpers** (`docs/testing.md`, `spawn`): `life` (a victim that lasts several
  blows) and `hold_fire` (`Unit::melee_only`: musketeers fence with the bayonet, dragoons
  cut). A unit set to melee only no longer fires point-blank at a man on the ground
  (`combat::point_blank`, only under `hit_locations`).

Bench: `ACR-analysis/melee4/melee_bench.py` (all attackers `hold_fire`, victims `life` 2000
and `hold_fire`; `foot_fence`: two musketeers fencing), `hitsheet.py` (chosen frames side by
side; the hit frames from `ACR_STRIKE_DEBUG`'s arm weight). Results:
`melee4/run1/sheet_foot_fence_all.png` (they fence with `ATTACK_THRUST_MID`, no shots),
`melee4/run3/sheet_lance_crawl_hit.png` (lance reach 2.54 m read from the tip),
`melee4/run4/sheet_horse_crawl_hit.png` (EngD18 plays `ATTACK_CUT_DOWN`, the sabre in the
hand, carbine and scabbard hidden, the blade raised in the wind-up). Rebuilt on the VM:
EngD18 and PueVsdA (old ones in `showcase/backup-melee4/`).

The exporter takes held objects relative to their bone *as posed*: a socket left at scale 0
by the last keyed clip baked a 0 scale into the drawn sabre's own node. `cavalry.unscale`
resets the rider's bone scales before the export.

### Not yet

- **Lance down still misses:** with the reach right, at the hit the lance still points
  ~20° down and its point lands about a metre past the man on the ground (`run3`); the
  clip's 65° hand turn is not what shows. Either the `lance_down` hand euler does not turn
  the fist the way assumed (unchecked: the bench with `ACR_NO_STRIKE` did not show the
  strike clip either), or the layer's arm IK (aimed with the clip's axis, then at most
  `MAX_TURN` 30°) flattens it. Next: render the clip alone in Blender, then a larger turn
  for riders.
- **Sabre at the ground:** the bench victim lies ~2 m ahead of the horse, out of a sabre's
  reach: at the hit the blade is level by the horse's side. A victim beside the horse is
  needed to judge `ATTACK_CUT_DOWN`; `ATTACK_CUT_MID` is unchecked in a render.
- **Fencing** (`foot_fence`): the men close in and the muskets pass through both bodies
  in the strike frames; the guard is not tuned yet.
- **Looks:** the bayonets still pass through bodies in the `.MD` ATTACK/PSTAND frames,
  which the layer does not guard. The smash pose is unchecked in a render (the sim never
  picks `Smash`).
- **Sabre side:** `ATTACK_CUT_DOWN_L` is built but never chosen: the clip name has no side,
  and the sim's strike carries none. Picking it needs the victim's side of the horse.
- Other riders with gun and sabre (SpnRei) do not draw the sabre (`drawn` only on EngD18).
- No officer sword clips: no foot officer is built (EngKF18 is a mounted standard bearer).
  Which `.MD` would be the first foot officer to build is not settled yet (the unit lists sit
  in the game archives; not looked up in this slice).
- No native club or tomahawk hack; SiuKStr not given strike clips.
- The sim never picks `Smash` for musketeers, so `ATTACK_SMASH_MID` is unused.
