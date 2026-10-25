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

### Engine (phase 1, #278)

`crates/acr-physics` has no dependencies and runs the same code on native and wasm32. It uses the axes of skeleton
H: metres, `+X` left, `-Y` forward, `+Z` up.

- `World`: capsule bodies (`Body`: position, rotation, mass, principal inertia; local `+Y`
  along the bone) and a `Config` (fixed `dt` of 1/60 s, 4 substeps, 4 joint passes per substep,
  friction, passive joint damping, sleeping). `step(&ground)` follows Müller et al. 2020 (XPBD rigid
  bodies): integrate, contacts (ground first, then capsule pairs, with static friction), joints,
  velocities, then dynamic friction, restitution without bounce, and joint damping. It does not
  allocate; the buffers are sized when bodies are added.
- Joints: the anchors are held hard. `JointLimit::Hinge { lo, hi }` is used for the knee, elbow and toes, and
  `JointLimit::Ball { cone_axis, cone, twist_lo, twist_hi }` for the hip, shoulder, neck and spine (with a
  small cone for the wrist and ankle). The muscle drive is a soft angular constraint towards `target` with
  compliance `1/stiffness`, plus damping: `Tone { stiffness, damping }`.
- `Ground` is a height field: `height(x, y)`, with the normal taken by default from differences. Any
  `Fn(f32, f32) -> f32` can serve as one.
- `Ragdoll::spawn(world, &skeleton_h(), mass, pos, rot)` builds one capsule per body bone of
  skeleton H (22), with Dempster mass fractions and limits per `JointSpec`. Parent–child pairs and pairs that touch at rest
  never collide. The rest of the API:
  - `set_target` and `set_pose`: the rotation relative to the parent, as a delta from rest, in the bone's frame. `joint_axis` gives the hinge axis.
  - `set_tone`, `set_tone_region` and `set_tone_all`.
  - `World::apply_impulse(body, point, impulse)`.
  - `bone_transform` and `skin_matrix` (`current · rest⁻¹`, column-major): maps a
  skeleton-space rest vertex to the world.
- Light end bodies get an inertia floor of 0.004 kg·m², so that they converge against heavy
  neighbours.
- Sleeping: a ragdoll whose kinetic energy stays under 0.002 J/kg for 0.5 s falls
  asleep and costs nothing: no integration, joints or ground contacts, only its place in the
  broad phase. `World::apply_impulse`, a target or tone change, or an awake body touching
  it wakes it. `Ragdoll::freeze` puts one to sleep at once as a corpse that still blocks
  others; `Ragdoll::pose` gives its skinning matrices to keep.
- Cost measures (#278 slice 2): the world inverse inertia is computed once per substep;
  the ground is sampled once per step as a tangent plane per capsule end; the limits are
  checked without trigonometry (cosine of the cone, `tan` of the half twist, hinge range as
  a cosine test) and corrected only when violated; one anchor projection per joint pass;
  orientation corrections are renormalized to first order; capsule pairs come from a
  hashed grid over `(x, y)` (counting sort, no allocation).
- Settings against the tests (peak limit overshoot in the limp drop, hinge overshoot
  under 100 rad/s spins, separation):

  | substeps × joint passes | drop limits | hinge stress | separation | tests |
  |---|---|---|---|---|
  | 8 × 2 (before) | 8.3° | 5.1° | 0.5 cm | pass |
  | 4 × 3 | 4.1° | 1.4° | 1.6 cm | hinge separation 1.01 cm fails |
  | 4 × 4 (default) | 3.5° | 0.5° | 1.1 cm | pass |

- Tests (`tests/ragdoll.rs`, `ACR_PHYS_SUBSTEPS` and `ACR_PHYS_PASSES` override the
  settings):
  - A limp drop from 1 m settles. The energy goes from 1423 J to about 84 J, and the limits hold within 0.5° at rest and 3.5° at the feet-first impact.
  - With full tone and the root pinned, it holds a pose with bent elbows within 1°.
  - A 60 N·s push at the back of the right shoulder spins him clockwise and throws him backwards.
  - The hinge stops hold within 0.5° under spins of up to 100 rad/s.
  - Stacked capsules do not interpenetrate, and a body falling across a frozen corpse comes to lie on it; the corpse stays asleep and does not move (static geometry, m2-perf2).
  - A resting ragdoll falls asleep (after about 1.1 s), does not move while asleep, and an impulse wakes it.
  - Identical runs are bit-identical.
- Bench (`cargo run -p acr-physics --release --example bench`): limp ragdolls falling
  and then lying on uneven ground for 4 s, with sleeping off so they stay active; "half
  frozen" adds as many frozen corpses. The bench is best of three runs. The dev machine is a
  Snapdragon X1P64100 at about 3 GHz, at 100 % load from other work during the
  measurement. That is why the thread's own cycles (`QueryThreadCycleTime`) are the
  reliable figure, and the wall time is inflated.

  | Scene | before (8 × 2) | after (4 × 4) |
  |---|---|---|
  | 1 active | 208 kcycles (146 µs wall) | 95 kcycles (66 µs wall, ≈ 32 µs of CPU at 3 GHz) |
  | 16 active | 239 kcycles (168 µs wall) | 103 kcycles (72 µs wall, ≈ 34 µs CPU) |
  | 16, half frozen | — | 105 kcycles per active (75 µs wall) |
  | 64, half frozen | — | 111 kcycles per active (78 µs wall) |
  | 256, half frozen | — | 114 kcycles per active (80 µs wall, ≈ 38 µs CPU) |

  Frozen corpses add almost nothing per active ragdoll; the slow growth with the scene
  is the broad phase and the cache.

  **m2-perf2: corpses static, islands in parallel.** In a melee the corpses lie packed, and
  a falling man used to wake every corpse he touched: they joined the solve, woke their
  neighbours, and a step in the pile cost 260–580 µs per falling ragdoll. Now a frozen
  ragdoll (`World::freeze`) is static collision geometry: infinite mass, its capsules in a
  hashed grid of their own (rebuilt only when a group falls asleep or wakes), contacts
  against it one-sided, never woken by a touch (only by an explicit wake: an impulse, a
  new pose or tone). A natural sleeper (`update_sleep`) still wakes when touched. A step
  visits only the awake bodies (their own hashed grid for awake–awake pairs); awake groups
  joined by contacts or joints form islands, each gathered into compact buffers (the
  frozen capsules it touches copied in read-only) and solved apart, in parallel with
  rayon on native (from 64 awake bodies; wasm serial), then written back. The buffers are
  reused, so a step allocates only when it meets more than ever before. Bench (wall,
  per active ragdoll step, same loaded machine; "pile" = active ragdolls dropped onto 96
  frozen corpses packed 0.7 m apart, "60 apart" = 60 active spread out):

  | Scene | main | m2-perf2 serial (`RAYON_NUM_THREADS=1`) | m2-perf2 |
  |---|---:|---:|---:|
  | 16 active over a pile of 96 | 583 µs | 97 µs | 45–59 µs |
  | 60 active over a pile of 96 | 240 µs | 101 µs | 48–50 µs |
  | 60 active apart | 87 µs | 81 µs | 20–36 µs |
  | 256, half frozen | 168 µs | 87 µs | 20–22 µs |

  Physics is render-side; the simulation and its pinned hashes are untouched.
- wasm32 (`examples/bench_wasm.rs`, a cdylib timed by `examples/bench_wasm.mjs` under node
  22, see the file header for the commands), on the same loaded machine: 69 µs (1 active), 77 µs (16),
  81 / 86 / 87 µs per active ragdoll with 16 / 64 / 256 half frozen. That is about the native wall time, so
  wasm costs roughly the same as native here.
- **Budget rule:** active ragdolls = floor(frame budget / measured cost per active
  ragdoll step). The cost is measured at startup with a short calibration (for example 30
  steps of 8 limp ragdolls in a hidden world, best of three). With 4 ms and about 35 µs,
  that is about 110 active ragdolls (2400 bodies). Sleeping ones cost close to nothing.
  The rest get the animation-only path.

### In the game (#278)

`crates/acr-game/src/ingame/ragdoll_view.rs`, called from `three_d.rs` after the corpses
are piled. The renderer takes a palette from it per instance
(`ModelRenderer::set_custom_poses`, instance index → palette) instead of sampling the clip.

- **Mapping:** every body bone of skeleton H must be a joint of the model (exact names; a
  rider's prefixed bones fail, so mounted models stay on their clips). Each palette slot is
  carried by its nearest body-bone ancestor (fingers by the hand, sockets by their bone,
  the root by the hips). Axes: physics = world pixels / 30 with `y` negated (right-handed,
  `y` north, `z` up); skeleton space turned by the facing about `z` and moved to the ground
  point is physics (tested), glTF → skeleton is `(x, y, z) → (x, −z, y)`. The palette is
  `T⁻¹ · world · S · glTF→skeleton · bind` with `S` the ragdoll's skinning matrix and `T`
  the instance transform; the animated pose goes the other way into the bodies
  (`Ragdoll::place`, `target_skins`).
- **Hits:** a hit on a drawn man (not mounted, not a building) takes his animated pose and
  his velocity (ground points of the last two ticks) into a pooled ragdoll and applies the
  impulse at the struck bone (the `Injury` part, else the region), on its surface towards
  the attacker. Impulses: musket ball 4.5–9 N·s by energy, arrow 4, grapeshot 5–20,
  cannonball 60–300, shell 40–160, melee 12, thrown 12. The struck region's tone drops to 5 %.
  A melee or thrown weapon's blow speeds up the struck body by at most 2 m/s
  (`Ragdoll::apply_blow`); the rest of the impulse goes to the trunk, so a struck hand or
  head is not whipped and the man falls where he stands or backwards, at most a step further
  than he would collapse without the blow (m2-corpses: 18 N·s on a 0.45 kg hand gave it
  9.5 m/s; test `melee_blow_knocks_down_but_never_launches`).
- **Survivors:** the hips follow the animation on a damped spring kicked by the blow
  (stagger), sinking for leg hits (knees buckle) and a little for body hits, leaning with
  the sway; the muscles track the clip and the struck region's tone returns over 0.5–1.3 s
  (by energy); then the clip takes over again.
- **Deaths:** the hips are freed, the tone runs out over 0.2–0.6 s (per man) with a last
  reflex (elbows bent, arms forward, towards the body), he falls, settles and sleeps; his
  ragdoll is let go from the pinned root at his ground velocity (`Ragdoll::set_velocity`:
  the root's kinematic jumps leave 8–12 m/s spikes in the limbs of a walking survivor); his
  pose is then kept as the corpse's (no `DEATHLIE1`, no `lie_flat` morph). Balance,
  stepping and knock-downs of survivors: §2, first slice. Crawlers and the
  lying start from their current pose and stay down. Frozen corpses stay in the physics
  (others fall onto them) until the pool of 96 is full; the oldest then keeps its pose
  without physics.
- **Ground:** the terrain height (`view3d::ground_point`) as the physics height field.
- **Clock:** the game's ticks (10 per second) smoothed by the frame time, fixed 1/60 s
  steps, at most 4 per frame; paused with the game; a jump back (a loaded game) clears.
- **Budget:** `ragdoll_view::calibrate` at the first frame with models: 8 limp ragdolls,
  30 steps, best of three; awake ragdolls = floor(4000 µs / cost). Logged as
  `3d: ragdoll budget N awake (C µs per ragdoll step, ...)` (66 at 60 µs on the loaded dev
  machine). New hits beyond it keep their clips; hits nearest the window centre go first.
  The budget then follows the measured cost (m2-perf): in a dense pile a step cost about
  260 µs per awake ragdoll (4 × the calibration) and a slow frame runs up to 4 steps, so
  56 awake ragdolls took 43 ms of every frame 30 s into a 700-against-700 melee. Now new
  ragdolls start only while awake < 4000 µs / (measured µs per ragdoll step × steps per
  frame), both running means, at most the calibration's and at least `MIN_AWAKE` (12).
  m2-perf2 (static corpses, parallel islands): the calibration gives 82 (48 µs), the
  pile no longer raises the cost (about 35 µs per awake ragdoll step in the clash), and
  the pool of ragdolls kept in the physics is 192 (frozen corpses cost a step nothing but
  the contacts made with them). A frozen corpse's palette is made once and reused while
  its instance stays put. m2-rag-perf: on native the steps run on a worker thread between
  one frame's update and the next (`Ragdolls::dispatch`, `join`; `ACR_RAG_INLINE` keeps them
  on the render thread), so the 4 ms of physics overlap the render and the GPU; the budget
  follows the worker's measured time (`docs/rendering-3d.md`, "Ragdoll steps off the render
  thread").
- **Bench:** `ACR-analysis/ragdoll/bench_ragdoll.py` (with `ACR_EXE`): six men in a row
  seen from the side — a musket wound from the front, musket kills from the side and the
  front, a cannonball kill, a crawler killed, a musket wound in the leg; 24 frames, one per
  tick, then the corpses after 4 s, from the side and from above.
- **Animals** (`m2-animalsim`): any rig gets a ragdoll of its own.
  - `acr_physics::skeleton_generic` builds one from a bone list (`RigBone`: name, parent,
    rest head and tail, flesh radius). `bone_kinds` names each bone by its name and place:
    root, spine, neck, head, jaw, tail, and per leg girdle, upper, middle and foot. Legs
    hanging from the chest ahead of the root are the front legs (regions `ArmL`/`ArmR`), the
    others the hind legs; the side is by `x`.
  - Capsules: the bone's length, and the radius 0.85 × the median distance of its skinned
    vertices (by their heaviest joint) from the bone, else a quarter of its length. Masses by
    volume at 850 kg/m³ (the capsules overlap at the joints): the bear 446 kg, the elk 492, the
    alligator 184, the jaguar 105. Shaggy coats are part of the mesh, so a share of the
    measured radius is fur (`fur_share`): the bison 0.22 (879 kg instead of 1536), the elk
    0.1 (386 kg), the bear 0.08 (367 kg).
  - Limits: spine cone 14°, twist 8° (stiff); neck 35°/30°, head 30°/25°, tail 40°/20°
    (flexible); girdle 15°; upper leg 55°/20°; middle leg bones hinges of ±95° (the rest pose
    is already bent, so both ways), bending vertical bones fore and aft and level ones
    (the alligator's splayed legs) up and down; feet 35°/15°; jaw hinge −5..35°.
  - `ragdoll_view`: a model with no skeleton-H rig, an `Anm*` type (`extras.acr_type`) and a
    skin is mapped generically (`Mapping::generic`): the skin's joints without details
    (digits, claws, tongue, eyes, ears, whiskers, `_end`, helpers such as `Reference`,
    `RigRoot`, shoulder blades) are the bones; their rest segments come from the inverse
    bind matrices (to the joint that continues the chain, else along the joint's own axis as
    far as its details reach); every other joint rides on its nearest kept ancestor. The
    pool keeps animal ragdolls apart by model (a slot is reused only by the same rig).
  - Hits: the struck bone by the injury's part (head, the chest end of the spine, its
    middle, the root; a leg by its level), the impulse of the weapon. A survivor staggers on
    a damped spring around its clip with the struck region limp; a dying animal goes limp
    over 0.2–0.6 s, falls from its pose with the shot's impulse and freezes as the corpse's
    pose. Before a corpse is frozen it relaxes for 0.7 s (`RELAX_S`): an arm left propped
    up (hand 10 cm or more above the shoulder, raised by the death reflex) is pulled at 6 %
    towards the rest pose; the rest stays limp in the pose it fell in. (Until m2-corpses
    the whole body was pulled towards the rest pose: the legs straightened and the corpse
    rolled over about a second after it had come to rest; test
    `a_settled_corpse_keeps_its_heading_while_it_relaxes`.) No
    death reflex, steps or knock-downs for animals. Riders and horses keep their
    clips (their types are not `Anm*`). The horse's `DEATH` clip slides the body to where the
    sprites' corpse lies (`cavalry.centre_fall`) over the stumble only (`rig.fall_slide`,
    the first 30 % of the clip); spread over the whole clip the dead horse slid on the
    ground for a second after it had fallen (the cavalry models need a re-export). Bones: bear 24, elk 29, jaguar 35, alligator 37.
- **Mauling** (`m2-animalsim`, see `docs/damage-model.md`): an animal's blow (`Injury::cause`
  `Claw`, `Bite`, `Gore`) pushes a man with 28, 22 or 60 N·s (half at no energy) away from the
  animal, a goring also upwards; at 19 N·s or more, or whenever the simulation knocks him down
  (`Body::down`), he falls. While `Body::down` lasts he stays on the ground (the fall phase is
  held, at 12 % tone instead of 30 %, so he lies limp) and the animal's further blows strike the
  lying body.
- **Pinning** (`maul_view.rs`, render only): a living animal whose attack target is down
  (`Body::down`) within 5 m, or the man it held down while it stays and does not walk,
  is placed relative to where his ragdoll lies (`Ragdolls::bone_world`: `spine`, `hips`,
  the limb of the last hit), eased in and out at 3 per second:
  - bear: across his body from its side, its forepaws (the rig's front feet) under his
    chest, the front raised by 11 cm and pitched so that the paws rest on him;
  - jaguar: on top of him along his body, its neck at his neck, leaning forward (slope
    0.45) and lowered 10 cm (its pouncing clip lifts the body);
  - alligator: its jaws (neck + 0.55 × the trunk) at the limb of the last hit; after each
    bite a roll of up to ~60° over 1.4 s, dragging him back 25 cm;
  - elk and bison: the head at his belly.
  The clips stay the simulation's (`ATTACK`, `PATTACK`). The ragdoll sees the new
  transform (the placement runs before it).
- **Wounds of mauling and on animals** (`tears.rs`, `living_wounds.rs`,
  `acr_render::damage::Rig::beast`): the wound's shape comes from `Injury::cause`. Claws rake
  3–4 parallel slanted gashes 5–6 cm apart, 15–30 cm long and 3–5 cm deep on the side
  facing the animal (zoom-16 close-ups: shallower ones only opened the coat onto the shirt
  and read as one pale patch); jaws crush a core between two rows of three tooth holes (a
  broken bone, a bigger core; on an unbroken limb at most 0.6 of its radius, so the sleeve
  keeps its outline around the teeth);
  horns punch a deep hole along the blow with a ragged entry; a limb ripped off leaves a
  ragged tear. The newest hits' holes and marks are kept first (the cap is 8), so a man on
  the ground shows each new blow. Animals (`acr_animal` models) get a wound rig guessed from
  their skeleton (trunk from the pelvis to the neck, the four legs as the chains that reach
  the ground, radii from the skinned vertices): it places marks, spurts, drips, pools and
  torn holes, never cuts; an animal corpse's pool grows with its girth (0.6–2.5 × a man's).
  The volumes carry the animal kinds of `damage::VolumeKind` (claws `Claw`, the bite's core
  `Crush`, tooth holes and the horn's track `Puncture`): seen through them is dark wet meat
  full of blood (`mauled_tissue` in `model.wgsl`: no pale fat at the lips, a bite bruised, a
  puncture nearly black), the torn lips and the shreds hanging in are raw dark red, and the
  cloth around is soaked dark for 0.9 of the radius; their marks are not ragged (a ragged
  mark's ring of exit flesh made claw slits read pink-white) and bleed at once (soak 0.55,
  run 14 cm at the blow, spreading within a minute). Balls (`Tear`) and blades (`Gash`) keep
  their look.
- **Cutting animals (contract, not built yet).** What a cannonball needs to take a leg or
  tear an animal open:
  - assets (`animals.py`): `_PART` per vertex with the human ids for the legs (front legs
    as `arm_l`/`arm_r`, hind legs as `leg_l`/`leg_r`, `+x` left as in `Rig::beast`; trunk,
    neck, head and tail as their parts), from the deform weights of the chain the vertex
    weighs most on, the chains chosen as `Rig::beast` chooses them (from the pelvis, the
    four that reach the ground); `_LIMB` = the projection of the rest position onto that
    leg's chain (shoulder or hip to the hoof or paw tip), trunk −1; root extras
    `acr_limb_radius` with 16 outer radii per leg (fur included); stump caps on the parent
    side of each leg joint (`_PART` 16 + j, 32 + j) like the humans', scaled by the
    profile; the inner meat and bone layers get the same `_PART`/`_LIMB` as the fur.
  - renderer: `Rig::beast` must name the chains in the same order as the asset (or read them
    from extras `acr_limb_chains`: bone names per leg) so that `_LIMB` and the cut levels
    agree; the cut plane, caps and thrown limb (`debris.rs`) then work as for a man. The
    trunk cut needs a trunk level along the spine (pelvis 0 .. neck 1), which `_LIMB` −1
    does not give: a second float `_TRUNK` or the projection at load as for limbs without
    `_LIMB`.
  - simulation (`acr_sim::body`): animals today lose no legs to balls or blades and big
    bodies are never torn in two (`Body::animal`); a leg off needs its own outcome table
    (by `Beast::size`), a three-legged gait or a fall, and the hashes pinned for animals
    change.
- **Not yet:** severed pieces (`debris.rs`) keep their own throw (they could become small
  ragdoll bodies later); no kneeling or reloading pose exists to be killed in; horses and
  riders; the corpse's decay sinking is not applied to physics poses.

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

### First slice (#279)

Kinematic layers over the clips, render side only (`acr_physics::gait`,
`crates/acr-game/src/ingame/gait_view.rs`, called from `ragdoll_view` for the 48 drawn men
nearest the window centre that no ragdoll holds; `ACR_NO_GAIT` turns it off).

- **Cycle from the clip:** each `MOTION_*` and `CRAWL*` clip is sampled once (48 samples) in
  the body's frame. The lowest effector (feet when walking, hands when crawling) within 5 cm
  of the lowest point carries; how far it slides back under the body is the loop's travel
  (`MOTION_L`/`_R` of `engm18`: 1.15 m per 0.38 s loop; the native foot's run, `poses.run_cycle`
  on its 8-frame running sprites (#295): 1.37 m per 0.29 s loop; `CRAWL2` 0.64 m per 1.0 s;
  `CRAWLARM` 0.32 m per 1.17 s; `WRITHE` travels nothing and keeps its clip).
- **Phase lock:** the sim position is smoothed over its ticks, and the distance covered
  plays the clip. The visible body sits where the clip's pushes and pulls have brought it: it
  advances only while a carrying limb moves back, leading or lagging the sim position by less
  than one loop. A crawler moves in surges with each pull.
- **Contacts:** a carrying foot or hand is planted on the terrain at the clip's own height
  over the ground and held there by two-bone IK (thigh–shin–foot, upperarm–forearm–hand);
  lifted, it blends back to the clip over 0.12 s. The hips sink where a planted leg cannot
  reach, and planted feet tilt with the ground. Bench: planted contacts miss their point by
  1.3–2.9 mm on average (24–55 mm at worst, where a leg is out of reach).
- **Own animation of the other slots:** the posed body goes back to the palette by
  `ragdoll_view::repose`: each slot (fingers, sockets and the gear on them) moves with its
  body bone and keeps its clip pose relative to it. A socket the clip keys to scale 0 (the
  peasant's tools and sack while walking, a slung musket, `line.key_show`) stays hidden;
  `palette_of` rebuilt them from the rest pose and showed every tool (EngKri, EngMus,
  EngTap). A ragdoll has no clip: the slots its clip had collapsed when the physics took
  over stay collapsed (`hidden_slots`, `hide`).
- **Limp** (`Body::leg` < 100 on both legs): the loop's samples on the bad leg take
  `1 − 0.6 · severity` of their time (a short, quick step); in its swing the bad leg is held
  straighter and the hip hikes; the trunk leans over the good leg. The hand on the side of a
  wounded leg holds the thigh (arm IK); the right hand holds a chest or open-belly wound. A
  leg ending in a stump (`lost` at the ankle) plants the stump's end on the ground.
- **Dragged foot:** in a heavy limp (severity over 0.4, fully at 0.8; two flesh wounds in a
  leg give `Body::leg` 30, severity 0.7, drag 0.75) the bad foot does not lift in its swing:
  its IK target is pulled down to the ground by the drag, it hangs toe down (pitched by
  `0.7 · drag` towards the ground) and scrapes forwards; the hip hike gives the stiff leg
  room, and the dragged leg does not sink the hips. A stump never drags. Sheet (EngM18, two
  wounds in the left leg, blank white field, `ACR-analysis/locomotion/limp2.json`):
  `ACR-analysis/locomotion/limp_legs_sheet.png`, main above, branch below.
- **Balance for survivors** (`ragdoll_view`): the hip sway is stronger (6× the momentum over
  the mass, about 1 Hz). The centre of mass is checked against the support between the feet;
  12 cm outside it, the foot farther from the landing point takes a 0.3 s recovery step by
  IK under the centre of mass and stays there. An impulse of 19 N·s or more (close grapeshot,
  cannonball, shell) knocks him down: the root is freed, the legs go to 25 % tone with the
  knees and hips folding, and he falls to his knees and over. He lies until 2.2 s, then the
  hips blend from where he lay back to the clip over 1.5 s while the tone returns, and the
  clip takes him back.
- **Bench:** `ACR-analysis/gait/bench_gait.py` (with `ACR_EXE`) records a walker, a limper
  (two wounds in the left leg), a man with the right foot lost, a crawler without legs, a
  one-armed crawler and a grapeshot hit on a standing man. The bench `spawn` takes `lost`
  (`arm_l`, `arm_r`, `leg_l`, `leg_r`; `foot_l`, `foot_r` lose the leg at the ankle),
  `wounds` (one flesh wound per entry) and `patrol` (any pose moves; crawlers, who take no
  orders, are sent directly).
- **Not yet:** step length does not change with speed (the cadence does, with the
  distance); no slope on the bench (the random map's ground only); the sim has no
  knocked-down state, so a knocked-down survivor always gets up; the recovery step does not
  move the hips' spring origin and the hips are still held kinematically (no ankle torque
  or real support forces); the hop has no landing squash and no arm swing for balance; the
  dragged legs do not collide with each other or the body. Writhing keeps its clip.

### Second slice (#279): dragged legs, the push, the hop

- **Dragged legs** (`gait_view::crawl_legs`): a crawling man's leg with `Body::leg` 0 (or its
  stump) is a chain hip → knee → foot after the IK of the hands. Each frame the knee and
  foot fall at 1.5 m/s, keep their bones' lengths from the hip outwards (follow the leader:
  they stay where they lay until the hips pull them) and rest on the terrain (joints 6 cm
  and 7 cm over it); the shin straightens by 15 % of the thigh's direction per frame. The
  bones are turned onto the points; nothing of the clip's legs is left. Per man two points
  per leg, kept over the clip's swaps.
- **One-leg push:** a working leg of a crawler (`CRAWL1`, or `CRAWLARM` with legs) is planted
  while the hands pull (the body travels faster than its loop mean, from `Cycle::progress`)
  and between pulls drawn up beside the hips (half a leg back, 0.3 leg out), knee bent out
  to the side; the plant holds while the body moves over it.
- **Hop** (`Limp::hop`): `acr_sim::body` has breaks that still carry, a closed break at the
  ankle or below (`Fracture::level >= ANKLE`) and a joint put out (`Dislocated`, knee or
  ankle): `Body::leg` 20. Such a man hops on the good leg, two hops per clip loop, the last
  30 % of each in the air with the hips lifted up to 7 cm; the good foot lands under its hip
  18 cm ahead and stays planted; the hurt foot is held behind and up (0.62 leg down, 0.3
  back, knee forward). No limp warp and no drag. A hopping or crawling man keeps his plants,
  phase and legs when the sim swaps `MOTION_L` and `MOTION_R` (every step); the walk and
  limp still restart there as before. The bench `spawn` takes `broken: ["leg_l"]` (a closed
  ankle break).
- **Measured** (`measure` with `ticks` now reads the drawn, gait-posed joints; RX 9060 XT
  VM, blank white field, EngM18 ×5, `ACR-analysis/locomotion2/steps.json`, 40 ticks each;
  slide of planted feet in m / planted ticks):

  | Man | main | branch |
  |---|---|---|
  | walk | 0.009 / 16, 0.007 / 15 | same |
  | limp (two wounds, left) | 0.009 / 16, 0.004 / 15 | same |
  | broken ankle (left) | drag limp 0.009 / 16, 0.003 / 15 | hop: good foot 0.001 / 26 |
  | crawl, no legs | clip legs 1.01, 1.01 | dragged 0.81, 0.81 (body moved 0.80) |
  | crawl, left leg lost | clip legs 0.37, 0.77 | stump dragged 0.79; push foot 0.038 / 15 |

  The clip's feet (before the fix to `measure`) slid 1.7–1.8 m in the same walks.
  Cost: `rag.gaits` in the 700 vs 700 clash (`ACR-analysis/locomotion/clash_br/steps.jsonl`,
  one run each) 0.04–0.52 ms per frame on main and 0.03–0.52 ms on the branch. Sheets:
  `ACR-analysis/locomotion2/ba_c2_*.png`, `ba_c1_*.png`, `ba_hop_side.png` (before above).

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

**Status (first cut, `m2-volumes`):** wound volumes, the shader's ragged boundary, shreds,
soaked rim and tissue-shaded back faces, the hit → volume mapping per cause, severing by
volume (ragged stumps and pieces, caps as fallback) and gibs by volume are in; see
`docs/rendering-3d.md`, "Torn-out tissue". Still open: instanced bone pieces (ribs, bone
ends), physics for the detached pieces, depth-correct inner geometry (the hollow is the
shell's back faces, so a hole straight through a thin part shows the ground behind), and
tuning on the bench.

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

`KnockedDown` exists in a first, minimal form: `Body::down` (ticks), set by animals' blows
(`docs/damage-model.md`, mauling) with the hit-locations rule; while it lasts he cannot move,
attack or hit back, and the renderer keeps him on the ground.

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
