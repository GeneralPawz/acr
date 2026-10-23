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

## 2a. Finishing blows (m2-finish)

A man on the ground is helpless: one blow ends him.

- **Which blow.** Under the rule, a blow by a melee index (an attack without a projectile)
  of a man (not an animal) at a man on the ground (`melee::on_ground`) is a **finishing
  blow** (`combat::injure`): no region roll, no critical roll; the blow is aimed at the
  part of `melee::finishing_vital` and kills (`Outcome::Kill`, or the torn outcome the
  injury tables already gave): a thrust (bayonet, point, spear, lance) the heart (`Chest`,
  `Vital::Heart`), a cut (tomahawk, sabre, sword edge, axe) the throat (`Head`,
  `Vital::Neck`, dead at once here, not the timed bleeding), a smash (club, butt) the skull
  (`Head`, `Vital::Brain`). The injury is drawn by `body::injure_aimed` (its usual
  `rando()`s), energy 100. Riders finish too (from the saddle, at their reach).
- **No failure chance.** The man lies at the attacker's feet and cannot parry or roll away;
  a miss would only bring back the endless jabbing this replaces. The point-blank shot of a
  loaded musketeer is not a finishing blow (its own damage, `Shot`/`Down`, unchanged).
- **Closing in.** A man on foot finishing a man on the ground closes in to
  `melee::FINISH_REACH` (27 px, ~0.9 m at 30 px/m) instead of striking from his weapon's
  reach (~2 m): within the reach but not over him yet he steps straight in
  (`motion::step_over`: the victim's cell does not block, the terrain does), turning first.
  Over him no minimum attack range holds him off (`combat::stands_over`, also at the hit
  frame). Riders keep their reach. A unit standing ground does not step in.
- **Record.** `HitRecord::strike` is `Strike { kind, height: Down, mounted, finishing: true }`
  and `critical` the vital (`Heart`, `Neck` or `Brain`).
- Tests (`tests/melee.rs`): `one_finishing_blow_kills_a_crawler_from_close` (one blow kills a
  crawler with 5000 life, struck from ≤ 30 px), `finishing_vitals_by_kind` (and the lancer
  keeps his reach), `a_standing_duel_is_unchanged` (no finishing flag, struck from the
  pike's reach), `rule_off_no_strikes_and_crawlers_untouched` and the pinned hashes.

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
pub struct Strike { pub kind: StrikeKind, pub height: StrikeHeight, pub mounted: bool, pub finishing: bool }
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

### Clip first, then a residual layer (m2-melee5)

Method: each strike clip is measured offline at its hit frame, without the game, then the
layer is checked on the same pose. `strike_view::probe` poses a built `.glb` at the origin
and returns the weapon's point (`socket.tip` / `socket.muzzle`) from the clip alone and
after `aim`; the test `probe_clips` prints it for `ACR_PROBE_MODELS` (a folder of `.glb`s)
and `ACR_PROBE` (`model:clip:frame:right,fwd,up;...`, metres from the attacker's ground
point); without the variables it does nothing.

- **Melee distance** (driver logs, `ACR_STRIKE_DEBUG` now logs `victim at`): riders strike a
  man on the ground at 1.7–2.0 m (centre to centre: PueVsdA 1.9, EngD18 1.7, also when
  they ride in from 5 m); musketeers fence at 1.67–1.73 m. The victim is straight ahead.
- **The rider's rig was off.** The layer's joints are skeleton H's (`Rig::joint`); a rider's
  model is bound elsewhere (seated), so his rig hand lay ~0.3–0.5 m behind and 0.2–0.4 m
  below the real hand socket (musketeer: 0.06 m). The layer corrected that phantom error and
  swung the lance flat and past the man: the cause of the lance's ~20°. `Held::grip` now
  carries the socket's offset from the rig hand (in the hand's frame) and the point, the arm
  IK and the hand's turn use it.
- **`lance_down` was too steep:** the clip alone drove the point 0.9 m into the ground
  (66° down from a hand 1.41 m up). A 2.5 m lance meets the ground ~1.8 m out only from a
  hand ~1.9 m up at ~44°: the new pose raises the hand (`side(-0.20, -0.08, 0.52)`), the
  lance 44° down and 8° out (`(44, 8, 0)`).
- **The horse turns** (`strike_yaw`, render side; a rider with a `_DOWN`/`_DOWN_R` strike
  clip): the model's yaw is set so the man lies `TURN_LANCE` (25°) right of the facing for a
  lancer (the point passes the horse's right shoulder), `TURN_SABRE` (75°) for a sabre
  (beside the horse). It snaps with the clip (no easing yet).
- **`sabre_down_r`** leans further out (spine Y -30°, chest -14°), the hand aimed low beside
  the horse and the blade near vertical (`(80, 15, 0)`); the IK does not get the hand below
  ~1.45 m, so the clip alone still ends 0.46 m short of the near part of the man.
- **Residual layer:** on a strike clip (`How::residual`) the trunk bends at most
  `RESIDUAL_BEND` (~14°) and the hand turns at most `RESIDUAL_TURN` (~10°), and after the
  arm IK the hand is turned back onto the clip's weapon axis: the layer moves the weapon
  for distance and height but can never flatten the clip's angle. Test
  `the_layer_takes_up_a_residual` (lance 2.5 m at 43° down, sabre 0.9 m at 69°, target
  0.2 m off: point within 15 cm, angle kept within 15°).
- **Targets** (`target_point`): a standing man is struck at his body's surface
  (`BODY_RADIUS` 0.17 m short of his axis), so a bayonet stops there; a man on the ground at
  his ground point, by a rider's sabre (reach < 1.5 m) `NEAR_BODY` (0.5 m) nearer: the near
  part of his body.

Measured at frame 2 (`probe_clips`, physics metres right/forward/up of the attacker):

| Clip | Target | Clip alone: point, off | With the layer: off |
|---|---|---|---|
| PueVsdA `ATTACK_THRUST_DOWN` (old) | 0.50/1.50/0.20 | 0.48/1.17/-0.90, 1.15 | 1.09 |
| PueVsdA `ATTACK_THRUST_DOWN` (new) | man at 1.9 m, 25°: 0.81/1.73/0.20 | 0.76/1.79/0.16, **0.08** | **0.06** |
| same, target 0.24 m off | 0.75/1.60/0.30 | 0.24 | 0.12 |
| EngD18 `ATTACK_CUT_DOWN` (old) | 0.80/0.60/0.20 | 0.98/0.39/0.75, 0.62 | 0.53 |
| EngD18 `ATTACK_CUT_DOWN` (new) | near part, 1.2 m at 75°: 1.16/0.31/0.20 | 0.94/0.22/0.60, 0.46 | 0.20 |
| EngM18 `ATTACK_THRUST_MID` | body surface at 1.67 m: 0/1.50/1.15 | 0.11/1.79/1.27, 0.33 | (in game: stops at the chest) |
| EngM18 `ATTACK_THRUST_DOWN` | 0/1.60/0.20 | 0.07/1.39/0.02, 0.29 | 0.06 (before `grip`) |

Renders (`ACR-analysis/melee5`, white field, HUD off, `BENCH_DIST` in `melee_bench.py`):
`sheet_lance_crawl_hit.png` (frame 9: the lance ~44° down, its point at the man's legs),
`sheet_horse_crawl_hit.png` (frames 1–3: the horse turned, the man at its right; the
sabre hangs short of him), `run1/sheet_fence.png` and `run1/crop.png` (frame 26: the
bayonets stop at the chests). Rebuilt on the VM: EngD18 and PueVsdA (old ones in
`showcase/backup-melee5/`).

### Shoulders as posed, sides and the ready stance (m2-melee6)

- **Why the sabre hand stopped ~1.45 m up:** `cavalry.reins_world` solved the arm from the
  shoulder *at rest*; the arm's orientations are absolute, so a leaning trunk carried the
  whole arm (and the hand) off its target. It now takes `shoulders` (as posed), and the
  strike poses are solved at the weapon's point (`strike` in `cavalry.make`: trunk first,
  the wrist moved until the point lies on the target, the blade turned straight at the
  point when the arm cannot reach; the build log prints `STRIKE pose: ... off`).
- **`sabre_down_r`:** standing in the stirrups (knees opened, seat 0.22 m over the right
  stirrup), leaning far out (spine/spine1/spine2 Y -38/-22/-14°), the left hand on the
  pommel, the arm extended down and the blade ~64° down. A man 1.2 m out (the old
  `NEAR_BODY` 0.5) is beyond a seated man's arm and sabre (shoulder ~1.6 m up: 0.26 m
  short even at that lean), so `NEAR_BODY` is 0.75: the near part of a man 1.7 m off,
  ~0.95 m from the rider's middle.
- **`sabre_down_l`** (across the neck, man 40° left, `TURN_SABRE_L`): solved the same way,
  but the right arm across the body does not get there (below).
- **Side by the shorter turn** (`strike_view::down_turns`): a sabre rider cutting down takes
  `ATTACK_CUT_DOWN_L` when turning the horse so the man lies 40° to the left is shorter
  than turning him 75° to the right. `yaw_with_side` takes a signed turn.
- **The turn eases** (`strike_view::Turns`, in `three_d`): the yaw goes towards the blow's
  at `TURN_EASE` (6/s) and back to the facing after it, no snap.
- **Fade:** on a strike clip the layer fades in at `FADE_STRIKE` (30/s), so the first blow
  lands at ~full weight instead of ~0.55.
- **Ready stance** (`How::ready`: PSTAND, or a model with strike clips outside them): the
  layer only turns the trunk about the vertical to face the enemy; no step, no arm IK, no
  turn of the weapon, no left hand.
- **`lance_down`** solved at its point too: the hand held out right of the shoulder
  (x -0.71), the lance 3° out (was 8°), so the shaft behind the grip passes outside the
  rider's torso.

Measured at frame 2 (`probe_clips`, metres right/forward/up of the attacker):

| Clip | Target | Clip alone: point, off | With the layer: off |
|---|---|---|---|
| PueVsdA `ATTACK_THRUST_DOWN` | 0.81/1.73/0.20 | 0.79/1.75/0.22, **0.03** | 0.00 |
| EngD18 `ATTACK_CUT_DOWN` (=`_R`) | 0.95 m at 75°: 0.92/0.25/0.20 | 0.89/0.26/0.32, **0.12** | 0.07 |
| EngD18 `ATTACK_CUT_DOWN_L` | 0.95 m at 40° left: -0.61/0.73/0.20 | -0.56/0.50/0.68, 0.54 | 0.24 |
| EngM18 `ATTACK_THRUST_MID` | 0/1.50/1.15 | 0.11/1.79/1.27, 0.34 | 0.02 |
| EngM18 `ATTACK_THRUST_DOWN` | 0/1.60/0.20 | 0.07/1.39/0.02, 0.29 | 0.01 |

(Blender's own solve gave 0.05 for the sabre; the exported clip reads 0.12: the glTF bake
and the `fist_centre` estimate differ by a few cm.)

Renders (`ACR-analysis/melee6`, white field, HUD off, default bench distances):
`sheet_horse_crawl.png` (frames 2–24: the cut in frames 18–22), `sheet_lance_crawl.png`
(frames 4–11: the hit at frame 6–8), `sheet_fence.png` (frames 10–30; ready frames 10–20
without the arm IK). Rebuilt on the VM: EngD18 and PueVsdA (old ones in
`showcase/backup-melee6/`).

### Shared moves: forward stoop, sidearms, the visible socket (m2-shared-moves)

- **The shared stoop leaned back.** Positive spine X bends the trunk *forward* on skeleton
  H; `clips.STOOP` and the cut's lean in `sword_strikes` / `club_strikes` used negative X,
  so the shared blows down fell 0.45–0.9 m short (officers, native, meso, inca, pike and
  conquista kept local forward versions). Now `clips.STOOP` is the officers' long lunge
  (hips 0.80 m forward and 0.27 m down, spine +30/+10/+4°), `clips.LEAN` the forward cut
  lean, `clips.CLUB_STOOP` a deeper lunge for the shorter club (hips 1.08 m forward, spine
  +42°), `stoop_raised` the wind-up, and `clips.TARGET_DOWN` is 2.0 m ahead (the sim's
  distance at a man on the ground; 2.57 m after walking up: the layer's step does the rest).
  The club's mid hack leans forward too (spine +16°, hips 0.44 m forward).
- **Points off the axis.** An axe's or a club's `socket.tip` lies 0.15–0.17 m before its
  handle (`held.tomahawk` (0, -0.146, 0.38), `held.war_club` (0, -0.165, 0.39)); a clip that
  took only the length along the axis missed by ~0.2 m. `clips.grip_for` takes the point as
  a vector in the item frame; the sidearms pass it.
- **Sidearms** (`line.py` SPEC `sidearm`, docs/assets-humans.md): the musket hidden in the
  hand and slung on the back, the sidearm on `socket.blade_r` for `ATTACK` / `ATTACK_*`.
  Units: FrnBuk, EngTap, EngTapDragon, UsaMil (tomahawk), SpnS15, FrnS15 (sword, through
  `frontier.build(..., make_fn=line.with_sidearm(frontier.make, ...))`), EngMus, SpnMus,
  FrnS17 (sword, `pike.musketeer_spec(..., sidearm="sword")`).
- **The visible socket** (`strike_view::weapons` / `pick`): a model's weapon sockets, best
  first, are `socket.hand_r`, `socket.blade_r`, `socket.club_r`; the layer takes the first
  one shown in the current pose (its scale against its parent's ≥ 0.5). Archers with a club
  on `socket.club_r` (native, plains) aimed the hidden arrow before and missed by 2+ m.
  Test `the_shown_socket_holds_the_weapon`. The club archers' `.glb`s also baked a 0 scale
  into the club's node (native.py / plains.py set the socket to 0 before the export, the
  m2-melee4 trap): they now reset every bone's scale to 1.
- **Mounted bow and club** (`cavalry.py`): `held` "bow" (`bow`, `bow_poses`, `bow_keys`:
  the trunk turned left, the bow arm out at shoulder height, draw, loose on the `.MD`'s
  release, reach to the quiver on the back, nock), `blade` "club" (a war club drawn on
  `socket.blade_r`). SiuKLut (bow and club), SiuKVog (club).

Measured at frame 2 (`probe_clips`, metres right/forward/up; `ACR-analysis/shared-moves/probe_*.txt`):

| Clip | Target | Clip alone: off | With the layer: off |
|---|---|---|---|
| EngMus (sword) `ATTACK_CUT_MID` / `_HIGH` / `ATTACK_THRUST_MID` | 0/1.50/1.15, 1.55 | 0.00 / 0.00 / 0.00 | 0.00 / 0.00 / 0.01 |
| EngMus `ATTACK_THRUST_DOWN` | 0/2.00/0.20 (2.57) | **0.00** (0.57) | 0.03 (0.00) |
| SpnS15 (sword) `ATTACK_CUT_MID` / `ATTACK_THRUST_DOWN` | 1.50/1.15 / 2.00/0.20 | 0.00 / 0.00 | 0.00 / 0.03 |
| FrnBuk (tomahawk) `ATTACK_CUT_MID` | 0/1.50/1.27 | 0.00 (0.15 with the length only) | 0.01 |
| FrnBuk `ATTACK_CUT_DOWN` | 0/2.00/0.20 (2.57) | **0.00** (0.57; 0.21 with the length only) | 0.04 (0.02) |
| SiuLut, IroLut (club on `socket.club_r`) `ATTACK_CUT_MID` | 0/1.50/1.27 | 0.00 (was 2.64: the hidden arrow) | 0.01 (was 2.10) |
| SiuLut, IroLut `ATTACK_CUT_DOWN` (native's own `club_down`) | 0/1.80/0.20 (2.0) | 0.24 (0.44) | 0.03 (0.03) |
| IroTam `ATTACK_CUT_MID` / `ATTACK_CUT_DOWN` (own) | 1.50/1.27 / 1.80/0.20 | 0.00 / 0.23 | 0.01 / 0.02 |
| EngB18 `ATTACK_THRUST_DOWN` / `_MID` (stick, sword length) | 2.00/0.20 / 1.50/1.15 | 0.58 / 0.58 | 0.01 / 0.05 |
| SiuKVog, SiuKLut (club) `ATTACK_CUT_DOWN` | 0.92/0.25/0.20 | 0.59 | 0.33 |
| SiuKVog `ATTACK_CUT_DOWN_L` | -0.61/0.73/0.20 | 0.48 | 0.22 |
| PueVsd `ATTACK_THRUST_DOWN` | 0.81/1.73/0.20 | 0.03 | 0.00 |

Blender's solve on the shell body (`ACR-analysis/shared-moves/scripts/shared_solve.py`,
officer body; `frontier_solve.py`, FrnBuk's): `sword_strikes` all 0.00 (down at 2.0 m);
`club_strikes` 0.00 / 0.00 (mid / down at 2.0 m, tomahawk point vector).

Families with local forward blows that can switch to the shared ones now: officers
(`stab_down` = `clips.STOOP` + the same numbers; `cut` = `clips.LEAN`), native
(`club_down` at 1.8 m: the shared hack reaches 2.0 m), meso and inca (their down blows
copy the officers' stoop), pike (`DOWN_BASE`); conquista's halberd stays its own (a
two-handed pole). Not switched here (not this slice's files).

Renders (`ACR-analysis/shared-moves/`, white field, HUD off): `sheet_sidearm_mus.png`
(EngMus: the sword drawn, the musket slung, the stab down; then back to the musket),
`sheet_sidearm_buk.png` (FrnBuk's tomahawk), `sheet_sidearm_s15.png` (SpnS15),
`sheet_club_archer.png` (SiuLut's war club blow, then the bow), `sheet_mounted_archer.png`
(SiuKLut: draw, loose, nock; the arrows fly), `sheet_mounted_club.png` (SiuKVog),
`sheet_drummer_idle.png` (EngB18: the left one in STAND, ready), `sheet_puevsd_lance.png`;
frames in `run1/<scene>/`, the bench `scripts/bench.py`. Rebuilt on the VM: EngB18, EngMus,
FrnBuk, SpnS15, IroTam, IroLut, SiuLut, SiuKVog and new SiuKLut, PueVsd (old ones in
`showcase/backup-shared-moves/`).

### Mounted war club (m2-cav17)

The war club's head lies 0.165 m off its handle (`held.war_club` `socket.tip`
(0, -0.165, 0.39)); `cavalry.solve_strike` took a point `CLUB_TIP` + 0.04 = 0.56 m along the
axis, so the solved pose was wrong by the offset as well as short. It now takes the point
as a vector in the hand's rest frame (`cavalry.weapon_tip`: the club's and the sword's from
`held`, the sabre's (0, -0.94, 0.03)) and turns the weapon so the point, not the axis, lies
on the line to the target. The cut down right has its own lean, `cavalry.CLUB_DOWN_R`: the
seat 0.40 m right, 0.05 forward and 0.18 down, the trunk hanging out over the right side
(spine X/Y 20/-80°, spine1 10/-35°, spine2 5/-20°), the club 60° down and 70° out
(Blender's solve: 0.06 m). Probe (`probe_clips`, frame 2, `ACR-analysis/cav17/probe_*.txt`):

| clip | target | clip alone: off | with the layer: off |
|---|---|---|---|
| SiuKLut, SiuKVog `ATTACK_CUT_DOWN` | 0.92/0.25/0.20 | **0.09** (was 0.59) | **0.04** (was 0.33) |
| SiuKLut `ATTACK_CUT_DOWN_L` | -0.61/0.73/0.20 | 0.39 (was 0.48) | 0.17 (was 0.22) |
| EngD17, SpnRei (sword) `ATTACK_CUT_DOWN` / `_L` | as above | 0.12 / 0.05 | 0.07 / 0.02 |

Frames: `ACR-analysis/cav17/sheet_club.png` (SiuKVog, `melee_bench` horse_crawl, old above,
new below). In that run the victim stood (the crawl pose did not take) and the sim played
the mid cut, so old and new look alike there; the probe is the check for the down cut.

### Finishing clips (m2-finish)

The user saw sabres and swords lunge at a crawler and hang in the lunge before the next
jab. Now (sim, 2a) the attacker stands over the man and one blow ends it, and the models
carry a finishing clip per weapon (`assets/blender/lib/acr_assets/finish.py`):

- **Names:** `ATTACK_<KIND>_FINISH` (`THRUST`, `CUT`, `SMASH`) and `ATTACK_FINISH`.
  `strike_view::strike_clip` prefers them for a man on the ground on foot
  (`finish_names`: the kind's, then `ATTACK_FINISH`; before the first blow the kind is
  unknown), never for a point-blank shot (`ATTACK_SHOT_DOWN` stays) or a rider.
- **Clip:** the wind-up at frame 0 (raised, upright), the blow at frame 2 (the sim's hit
  frame) on `finish.TARGET_FINISH` (0.9 m ahead, 0.2 m up, the man's chest), held one frame,
  back to the stand by 30 % of the clip and kept: no lunge, nothing held.
  - sword / sabre / hanger (`sword`): standing over him, the point stabbed down
    one-handed, 30° off vertical (`SWORD_DOWN` 150°);
  - spear / pike / halberd / spontoon (`spear`): both hands, the point driven down (156°);
  - axe / tomahawk / war club / macuahuitl / stone club (`club`, `tomahawk`, meso's
    macuahuitl): from overhead, a chop down (128°, the striking face's point vector);
  - knife (`knife`, no unit uses it yet): down on the right knee, a stab;
  - musket with bayonet (`finish.musket_clips`, line musketeers through `line._musket_clips`):
    both hands high, the bayonet driven down, solved by `aim.place` (grip 1.44 m from the
    point);
  - point-blank shot: the existing `ATTACK_SHOT_DOWN`.
- **Registration:** `finish.register(clips)` (the end of `clips.py`) wraps `sword_strikes`,
  `club_strikes`, `spear_strikes` and `clips.MELEE`, so officers, sidearms, natives, inca and
  meso spears get them; meso's own club hack and line's musket call `finish` directly.
- **Layer** (`How::finish`): on a `_FINISH` clip no step in and no crouch (he already stands
  over the man), the arm IK at `FINISH_ARM` (0.5) on top of the residual limits: small
  corrections only. With the kill on the first blow the attack ends and he goes back to his
  stand; the clip itself ends standing, so nothing freezes in a lunge.

### Not yet

- **Sabre down left:** fixed in family 5 (docs/families/cavalry.md): a cross-body cut,
  the trunk bent forward and turned left; 0.06 m with the clip alone (was 0.54), 0.01 with
  the layer. m2-melee6's negative X on the spine leaned the rider *back*.
- **Sabre down right:** 0.12 m with the clip alone (target 0.10), at the nearer
  `NEAR_BODY` 0.75; at the old 1.2 m out it cannot reach.
- **Ready stance:** the trunk turn is checked only at the bench's distance; in the `.MD`
  ready frames the muskets still lie as the clip carries them (horizontal over the
  shoulder, `melee6/sheet_fence.png` frames 10–20).
- **Club from the saddle:** fixed in m2-cav17 (below) for the cut down right; the cut down
  left still falls 0.39 m short with the clip alone (0.17 with the layer).
- **Drummer's stick jab** (EngB18 `ATTACK_THRUST_*`): `line.support_clips` passes `melee="sword"`
  to `clips.unit_clips`, which solves with `SWORD_TIP` (0.86 m) for a 0.42 m stick (0.58
  short with the clip alone). The fix is a `tip` argument through `unit_clips` to
  `sword_strikes` with `held.drumstick`'s tip, and an EngB18 rebuild: line.py and the
  drummers are not cavalry, so m2-cav17 left it.
- **Native and plains down hacks** still use `native.club_down` (1.8 m, 0.24 short at
  frame 2); the shared `club_strikes` reaches 2.0 m with the club's point vector.
- **Lance through the rider:** solved clear in Blender (hand 0.71 m right, 3° out) but only
  checked from the left side in the render; a view from the front would confirm it.
- **Looks:** the bayonets still pass through bodies in the `.MD` ATTACK/PSTAND frames,
  which the layer does not guard. The smash pose is unchecked in a render (the sim never
  picks `Smash`).
- Other riders with gun and sabre (SpnRei) do not draw the sabre (`drawn` only on EngD18).
- No officer sword clips: no foot officer is built (EngKF18 is a mounted standard bearer).
  Which `.MD` would be the first foot officer to build is not settled yet (the unit lists sit
  in the game archives; not looked up in this slice).
- No native club or tomahawk hack; SiuKStr not given strike clips.
- The sim never picks `Smash` for musketeers, so `ATTACK_SMASH_MID` is unused.
