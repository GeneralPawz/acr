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
