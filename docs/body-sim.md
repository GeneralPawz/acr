# Body simulation: physical soldiers (#276)

The 3D models are not sprite generators. Every soldier is a simulated body: hits carry
force and land where they land, deaths and movement follow from the situation (loading,
kneeling, crawling, riding), damage is messy and never the same twice, bodies get dirty
from what they touch, and no two men look alike. Decisions (user, 2026-10-22): our own
XPBD engine; the physics budget is found test-driven; maximum realism and gore.

## Split of responsibility

| Layer | Owns | Determinism |
|---|---|---|
| `acr-sim` (lockstep, replays, saves) | who is hit, where, how badly (`acr_sim::body`, #262); actions and states (reloading, staggered, knocked down, holding a wound, mounted, crawling); what a body can still do | deterministic, hashed |
| body simulation (`acr-physics`, new crate, render side) | how the body moves: animation targets, muscles, impulses, falls, contacts, crawling and limping motion | per frame, never fed back into the sim |
| renderer (`acr-render`) | skinning the simulated skeleton, torn volumes, gore, dirt, faces | — |

The sim never waits for physics. It decides that a man is knocked down for N ticks; the body
simulation shows the fall and the getting up within that time. Hit records (`HitRecord` with
`injury`, point and direction) are the bridge.

## 1. XPBD active ragdoll (`crates/acr-physics`)

- **Bodies:** one rigid capsule per deforming bone of skeleton H (22) and of the horse rig.
  Mass by body part (Dempster tables). Joints are XPBD constraints (ball or hinge) with
  anatomical cone and twist limits per joint.
- **Solver:** XPBD with substeps (about 8 per frame), contact against the terrain height
  field and simple capsule–capsule contacts between nearby bodies (piles of the fallen).
  `f32`, no allocation per step, data laid out per body for cache use. Runs the same in
  wasm.
- **Muscles:** each joint drives towards the animation's target pose with a PD controller.
  The stiffness and damping per joint (the "tone") come from the soldier's state:
  - healthy: high tone;
  - wounded limb: low tone;
  - stunned, unconscious or dead: tone falls to zero over a short time.
- **Balance:** the centre of mass is kept over the support (feet; knees; hands and elbows
  when crawling) by a simple balance controller (hip and ankle strategy, plus a stepping
  trigger). Losing balance becomes a stumble or a fall.
- **Hits:** an impulse at the hit point along the projectile direction, scaled by the
  momentum:

  | Projectile | Mass and speed |
  |---|---|
  | musket ball | 30 g, about 300 m/s |
  | cannonball | by calibre, with the remaining energy of #258 |
  | melee and thrown weapons | by weapon |

  The impulse is followed by a tone drop in the struck region and a reaction chosen from the
  sim state: flinch, spin, knee buckling, thrown back, or dropped where he stood.
- **Death:** the muscles give out from whatever pose he is in, with a last reflex (hands
  to the wound, a knee buckling first). A crawler killed in melee stays down.
- **Settling:** a body at rest is frozen into a static pose (it becomes a corpse instance,
  the piles of #231). It is woken again only by new impulses, such as a cannonball through
  a pile.
- **Budget, test-driven:** a bench measures the step cost per body (native and wasm). The
  budget is the number of fully simulated bodies within a frame-time target, scaled by
  the measured speed of the machine. It covers the bodies nearest the camera and those
  with an active reaction. All others run the animation-only path, with cheap procedural
  reactions (additive flinch, pre-baked falls) that hand over to full physics when they
  come close.

## 2. Physical locomotion (`acr-physics::gait`)

- **Gait generator:** step length and frequency come from the sim speed, and foot targets
  come from the terrain (foot IK, no sliding). The root moves with the planted feet.
- **Limp:** asymmetric timing from leg function (`acr_sim::body`). The step onto the bad
  leg is short and quick, the body leans over the good leg, and the hand holds the wound.
  A broken lower leg means hopping with support, or crawling. A lost foot means hobbling
  on the stump.
- **Crawl:** the hands and elbows are planted as contacts, and the body is pulled forwards
  by them. The speed is the result of the pull cycle, so nothing slides:
  - legs drag, or one leg pushes;
  - with one arm: a one-armed pull;
  - with none: writhing.
- **Action layers:** keyframed actions (the reload drill, aiming, melee) drive the upper
  body as targets while the gait drives the legs. Physics blends both.
- Recorded drills (#167) replace the keyframed base motions when available.

## 3. Volumetric torn damage

- **Wound volumes** replace the cut planes of #262 as the general case. Each is a capsule
  or sphere in rest space (centre, axis, radius), plus a noise seed. A hit adds volumes
  along its path:
  - a musket ball adds an entry hole and a larger, ragged exit cavity;
  - a cannonball adds a big capsule along its track, e.g. through the left shoulder, taking
    the arm, the shoulder and part of the chest.
- **Shader:** fragments inside a volume are discarded, with a noisy, fibrous boundary. The
  back faces of the cut-open shell are shaded as the inside: meat, fat and bone by depth.
  Ribs and bone ends show as instanced bone pieces. Tatters of skin and cloth are generated
  at the boundary.
- **Removed tissue** becomes chunks (gibs of #262, sized by volume). A limb whose root is
  inside a volume detaches as a piece; physics then takes over from the thrown arc of S5.
- **Clean cut planes** remain only for sabre and axe blows.

## 4. Interruptible actions (sim)

- **Interruptions:** an action (reload, aim, melee swing, build) has phases, and a hit in a
  phase interrupts it:
  - loading: drops the cartridge or the ramrod, staggers, and must restart;
  - aiming: the shot goes wild or does not go off.
- **New states** (deterministic, ticks):

  | State | Meaning |
  |---|---|
  | `Staggered` | can't act for a moment |
  | `KnockedDown` | down, then gets up if able |
  | `HoldingWound` | busy with the wound, can't fight |
  | `Unconscious` | down, not moving |
  | `Unhorsed` | thrown off the horse; the horse runs on or stands |

- **Riders:** a rider with a broken or lost leg, or a heavy hit, may fall from the saddle.
  From then on he is a foot soldier on the ground.

## 5. Dirt from contact and wear

- **Per soldier, a dirt atlas:** a small texture in the body's UV space. Every ground
  contact of the body simulation stamps the terrain's material where it touches:
  - grass stains, green;
  - mud, wet brown;
  - dust, pale;
  - blood from the ground.

  Contacts include falls, crawling and lying. It persists for the soldier and then the
  corpse.
- **Wear from a seed and the time in the field:**
  - march dust on the legs;
  - sweat;
  - powder soot on the face and the right hand after firing;
  - worn and faded cloth.

## 6. Individual faces and bodies

- **Head:**
  - face morphs: jaw, cheekbones, nose, brow, eyes;
  - age: wrinkles, sagging;
  - beards and stubble, moustaches, hair, queues;
  - skin tones per nation's recruitment;
  - scars, pock marks, missing teeth.
- **Body:** height (1.58–1.80 m) and build.
- All chosen per soldier from a hash of the unit id, and built in Blender on the render VM.

## Tools

- A driver camera command (#237).
- A close-up **bench scene** in the driver: spawn a few soldiers on chosen terrain, apply
  scripted hits (weapon, point, direction, energy) and actions, and record image sequences
  and GIFs.
- Every phase is judged on the bench before it reaches the user.

## Phases

| Phase | Issue | Content |
|---|---|---|
| 0 | #277 | bench and camera; wounds visible on the living; devastating musket and axe hits; no standing up to die; riders never crawl |
| 1 | #278 | XPBD active ragdoll |
| 2 | #279 | physical locomotion |
| 3 | #280 | volumetric torn damage |
| 4 | #281 | interruptible actions and states |
| 5 | #282 | contact dirt and wear |
| 6 | #283 | faces and bodies |
