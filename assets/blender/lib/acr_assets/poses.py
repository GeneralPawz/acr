"""Placeholder poses and clips for skeleton H (pure data; `armature.pose` applies them).

Angles in degrees about the armature axes of the rest pose, relative to the parent:
+X tips a bone's far end backwards (+Y), -X forwards; +Y swings a left limb inwards (a right
limb outwards); +Z turns about the vertical. "root_loc" moves the root (metres);
"world" fixes a bone's orientation in armature space (the musket socket).

The clips are stand-ins until the user's recorded drills replace them (#166): each is keyed
on the sprite frame numbers of its `.MD` line (`rig.clip_keys`), so the clip's frame i is the
sprite's frame i.
"""

import math

from . import rig

# Musket orientation (socket.hand_r "world" rotation): upright = (0, 0, 0).
SHOULDERED = (-30, 0, 4)
AIMED = (88, 0, 0)
LOWERED = (60, 0, 0)

STAND_MUSKET = {
    "upperarm_l": (11, 20, 0), "forearm_l": (-12, 0, 0),
    "upperarm_r": (0, -24, 0), "forearm_r": (-42, 0, 24), "hand_r": (0, 0, -10),
    "thigh_l": (3, 1, 0), "thigh_r": (4, -3, 0), "spine": (-6, 0, 0), "head": (10, 0, 0),
    "world": {"socket.hand_r": (-30, 8, 0)},
}

def _with(base, **over):
    d = dict(base)
    world = dict(base.get("world", {}))
    world.update(over.pop("world", {}))
    d.update(over)
    d["world"] = world
    return d


# Present (aim): butt in the right shoulder pocket, left hand under the barrel, cheek down
# on the stock. Bladed stance: the whole man turned ~40° to the right of the target (left
# shoulder and left foot forward), weight on the bent left knee, the head turned back
# along the musket and bowed onto the stock.
STANCE = {
    "root": (0, 0, -40), "root_loc": (0.02, -0.04, -0.03),
    "thigh_l": (-22, -4, 8), "shin_l": (20, 0, 0), "foot_l": (2, 0, 0),
    "thigh_r": (10, 6, 0), "shin_r": (4, 0, 0), "foot_r": (-10, 0, 0),
}
AIM_MUSKET = {**STANCE, **{
    "spine": (-10, 0, -4), "spine1": (-4, 0, 0), "spine2": (-2, 0, -6),
    "neck": (14, -6, 18), "head": (12, -18, 26),
    "upperarm_r": (-62, -50, -20), "forearm_r": (-60, 0, 30), "hand_r": (0, 0, 0),
    "upperarm_l": (-70, 50, 30), "forearm_l": (-20, 0, -20),
    "world": {"socket.hand_r": AIMED},
}}

# Reload drill (prime and load, 18th-century manual), keyed in ATTACK1 after the shot. The
# man keeps the bladed stance (STANCE: turned 40° right of the target, so his right hip is
# near (-0.12, 0.10) and his left foot near (-0.11, -0.36) in armature space). Each step
# places the musket ("world" orientation of the socket, `grip` point in armature space or
# the butt with `butt`) and says where each hand goes (`aim.place`): "grip", metres along
# the musket from the grip, or a point. Solved by IK in the unit script (`aim.drill`).
LOAD_BODY = dict(STANCE, spine=(-8, 0, 6), spine2=(-4, 0, 4), neck=(8, 0, 6), head=(10, 0, 10))
LOOK_DOWN = dict(LOAD_BODY, neck=(14, 0, 2), head=(22, 0, 0))
PRIME = (50, 0, -17)  # muzzle forward-up 40°, lock at the right hip
CAST = (-4, 3, 0)  # upright in front, the muzzle leaning a little towards the man
POISE = (-6, -4, 0)  # upright before the right side (make ready)
GRIP_AT_HIP = (-0.17, -0.04, 1.02)
BUTT_AT_FOOT = (-0.12, -0.46, 0.03)  # ~13 cm out in front of the left foot, a little left
ELBOW_UP = (-0.30, 0.05, 0.10)  # the right elbow out and up (working at the muzzle)


def _step(base, musket, grip, right="grip", left=0.40, **kw):
    return dict(base=_with(base, world={"socket.hand_r": musket}), grip=grip, right=right, left=left, **kw)


RELOAD_STEPS = {
    # Recover to the priming position: musket down to the right hip, thumb on the cock.
    "prime": _step(LOOK_DOWN, PRIME, GRIP_AT_HIP, right=0.10),
    "half_cock": _step(LOOK_DOWN, PRIME, GRIP_AT_HIP, right=0.16),
    # Handle cartridge: the right hand back to the box on the right hip.
    "cartridge": _step(LOAD_BODY, PRIME, GRIP_AT_HIP, right=(-0.08, 0.22, 0.98), elbow_r=(-0.30, 0.30, -0.20)),
    # Bite it open at the mouth.
    "bite": _step(dict(LOAD_BODY, head=(4, 0, 6)), PRIME, GRIP_AT_HIP, right=(-0.08, -0.14, 1.58), elbow_r=(-0.25, 0.0, -0.35)),
    # Prime the pan and shut it.
    "pan": _step(LOOK_DOWN, PRIME, GRIP_AT_HIP, right=0.13, elbow_r=(-0.30, 0.10, -0.15)),
    "shut": _step(LOOK_DOWN, PRIME, GRIP_AT_HIP, right=0.18, elbow_r=(-0.30, 0.10, -0.15)),
    # Cast about: musket upright in front, butt by the left foot, left hand on the barrel;
    # the right hand pours charge and ball into the muzzle.
    "cast": _step(LOOK_DOWN, CAST, BUTT_AT_FOOT, right=1.30, left=0.95, butt=True, elbow_r=ELBOW_UP),
    "pour": _step(LOAD_BODY, CAST, BUTT_AT_FOOT, right=1.50, left=0.95, butt=True, elbow_r=ELBOW_UP),
    # Draw the rammer (grasp under the muzzle, out at arm's length), ram twice, return it.
    "grasp_rod": _step(LOOK_DOWN, CAST, BUTT_AT_FOOT, right=1.25, left=0.95, butt=True, elbow_r=ELBOW_UP),
    "rod_out": _step(LOAD_BODY, CAST, BUTT_AT_FOOT, right=1.58, left=0.95, butt=True, elbow_r=ELBOW_UP),
    "ram_up": _step(LOAD_BODY, CAST, BUTT_AT_FOOT, right=1.57, left=0.95, butt=True, elbow_r=ELBOW_UP),
    "ram_down": _step(LOOK_DOWN, CAST, BUTT_AT_FOOT, right=1.32, left=0.95, butt=True, elbow_r=ELBOW_UP),
    "return_rod": _step(LOOK_DOWN, CAST, BUTT_AT_FOOT, right=1.20, left=0.95, butt=True, elbow_r=ELBOW_UP),
    # Make ready: the musket raised upright before the right side, then presented.
    "poise": _step(LOAD_BODY, POISE, (-0.17, -0.16, 1.02), right="grip", left=0.42),
}

# Frames of ATTACK1 (87 sprite frames, 0..86): the shot leaves at the active frame 17
# (SETACTIVEPOINT #ATTACK1 17, rounded up to even by the engine), then the drill.
SHOT_FRAME = 18
# The ramrod: drawn up out of its pipes, turned over, head down the bore for ramming, back.
ROD_PHASES = [(55, "stowed"), (58, "slide"), (59, "swing"), (61, "bore"), (74, "bore"), (76, "stowed")]
RELOAD_FRAMES = [
    (26, "prime"), (29, "half_cock"), (33, "cartridge"), (37, "bite"), (41, "pan"), (44, "shut"),
    (49, "cast"), (52, "pour"), (55, "grasp_rod"), (58, "rod_out"), (61, "ram_up"), (64, "ram_down"),
    (67, "ram_up"), (70, "ram_down"), (73, "rod_out"), (76, "return_rod"), (80, "poise"),
]

# The reload stance (#255). A `SLOWRECHARGE` musketeer (EngM18, FrnM18, UsaM18, IroStr ...:
# an `#ATTACK3` line) reloads by playing `#ATTACK3` over and over, each play taking its
# frames off the reload counter (`ATTACK_PAUSE` x personal delay / 100: EngM18 450 = five
# plays of 88 frames), straight after `#ATTACK1` (no transition: `combat.rs`, stance 4), then
# `#TRANS3X` (stance 4 to the firing stance, `USETRANSX`) and the next `#ATTACK1`. So:
# ATTACK1 after the shot primes, casts about, loads, draws the rammer and enters the bore;
# ATTACK3 rams (a loop: it starts and ends on `RAM_HOLD`); TRANS3X returns the rammer and
# presents, ending on the aim ATTACK1 starts from. TRANSX3, PATTACK3, UATTACK3 (into and
# out of the stance from the aim or from STAND) and PSTAND3 (held) for interrupted reloads.
# Keys: (frame, step) in ATTACK1, (fraction, step) in the others; "@aim" and "@stand" are
# the present and STAND. Rod: (frame or fraction, `aim._rod` mode).
RAM_HOLD = "ram_up"
SLOW_ATTACK1 = [
    (24, "prime"), (27, "half_cock"), (31, "cartridge"), (35, "bite"), (39, "pan"), (42, "shut"),
    (48, "cast"), (52, "pour"), (57, "pour"), (61, "grasp_rod"), (65, "rod_out"), (70, RAM_HOLD),
    (77, "ram_down"),
]
SLOW_ROD_ATTACK1 = [(61, "stowed"), (65, "slide"), (67, "swing"), (70, "bore")]
STANCE4_CLIPS = {
    "ATTACK3": ([(0.0, RAM_HOLD), (1 / 6, "ram_down"), (2 / 6, RAM_HOLD), (3 / 6, "ram_down"), (4 / 6, RAM_HOLD),
                 (5 / 6, "ram_down"), (1.0, RAM_HOLD)], [(0.0, "bore")]),
    "PSTAND3": ([(0.0, RAM_HOLD)], [(0.0, "bore")]),
    "TRANS3X": ([(0.0, RAM_HOLD), (0.2, "rod_out"), (0.45, "return_rod"), (0.7, "poise"), (1.0, "@aim")],
                [(0.0, "bore"), (0.12, "swing"), (0.25, "slide"), (0.45, "stowed")]),
    "TRANSX3": ([(0.0, "@aim"), (0.3, "cast"), (0.5, "grasp_rod"), (0.7, "rod_out"), (1.0, RAM_HOLD)],
                [(0.5, "stowed"), (0.7, "slide"), (0.8, "swing"), (1.0, "bore")]),
    "PATTACK3": ([(0.0, "@stand"), (0.25, "prime"), (0.45, "cast"), (0.6, "grasp_rod"), (0.78, "rod_out"),
                  (1.0, RAM_HOLD)], [(0.6, "stowed"), (0.78, "slide"), (0.86, "swing"), (1.0, "bore")]),
    "UATTACK3": ([(0.0, RAM_HOLD), (0.15, "rod_out"), (0.35, "return_rod"), (0.6, "poise"), (1.0, "@stand")],
                 [(0.0, "bore"), (0.08, "swing"), (0.15, "slide"), (0.35, "stowed")]),
}


def slow_recharge(md_lines):
    """Whether the musketeer reloads in the stance of `#ATTACK3` (`STANCE4_CLIPS`)."""
    return "ATTACK3" in md_lines


def _fit(keys, n):
    """`keys` [(frame, x)] of SLOW_ATTACK1 (87 frames) on an ATTACK1 of `n` frames: the drill
    after the recoil (from its first key) stretched or squeezed to end before the last frame
    (DelS15 49, SpnS15 109), so a short clip keeps every step and the rammer drawn; keys that
    fall on one frame keep the later."""
    start, end = SLOW_ATTACK1[0][0], 86
    span = max(n - 1 - start, 1) / (end - start)
    out = {}
    for f, x in keys:
        g = f if f <= start else start + int(round((f - start) * span))
        out[min(g, n - 2)] = x
    return sorted(out.items())


def reload_plan(md_lines):
    """{clip: (keys [(frame, step)], rod [(frame, mode)])} of the reload drill on the clip
    frames of `md_lines`: ATTACK1's drill after the shot (`SLOW_ATTACK1`, ending on
    `RAM_HOLD` on its last frame; else the whole drill `RELOAD_FRAMES`, ending in the aim)
    and the stance-4 clips of a slow-recharge musketeer."""
    n = rig.md_clip_frames(md_lines["ATTACK1"])[0] if "ATTACK1" in md_lines else 87
    if not slow_recharge(md_lines):
        return {"ATTACK1": (list(RELOAD_FRAMES), list(ROD_PHASES))}
    out = {"ATTACK1": (_fit(SLOW_ATTACK1, n) + [(n - 1, RAM_HOLD)], _fit(SLOW_ROD_ATTACK1, n))}
    for clip, (keys, rod) in STANCE4_CLIPS.items():
        if clip in md_lines:
            last = max(rig.md_clip_frames(md_lines[clip])[0] - 1, 0)
            out[clip] = ([(int(round(t * last)), s) for t, s in keys], [(int(round(t * last)), m) for t, m in rod])
    return out


def rod_phases(md_lines):
    """{clip: rod phases} of the clips that draw the rammer (`reload_plan`; FIRE_AIMED and
    FIRE_HIP play ATTACK1's)."""
    plan = reload_plan(md_lines)
    out = {clip: rod for clip, (_k, rod) in plan.items()}
    for clip in ("FIRE_AIMED", "FIRE_HIP"):
        out[clip] = plan["ATTACK1"][1]
    return out


# The mounted reload (dragoons: EngD18, FrnD17/18, SpnD17/18, UsaD18, EngD17, SiuKStr). Their
# `.MD` has `SLOWRECHARGE` and `#ATTACK3` commented out ("/SLOWRECHARGE"): the reload is in
# `@ATTACK1` itself (156 frames of sprite set 4), the shot on `SETACTIVEPOINT #ATTACK1 19`
# (frame 20), then the sprites bring the carbine down and load it in the saddle (frames
# 88-155 are the ram the commented `#ATTACK3`/`#TRANS3X` would have looped). Drill after the
# cavalry exercise: the carbine brought across the body, butt on the right thigh, muzzle up
# to the left front, held with the bridle hand (the reins stay in the left hand, which takes
# the fore-end); half-cock, cartridge from the pouch, bitten, the pan primed and shut; the
# carbine raised steeper, charge and ball poured, the rammer drawn, rammed once, returned;
# poised on the thigh and presented. Steps (`cavalry.reload_steps` places them): the
# carbine's butt and muzzle direction by anchors of the seated rider, each hand a distance
# along the carbine from the grip (metres; the muzzle at `CARBINE_MUZZLE`) or an anchor.
MOUNTED_SHOT = 20
CARBINE_MUZZLE = 0.93
MOUNTED_FRAMES = 156
MOUNTED_RELOAD = [
    (0, "@aim"), (19, "@aim"), (MOUNTED_SHOT, "@fire"), (23, "@aim"),
    (34, "across"), (40, "half_cock"), (47, "cartridge"), (54, "bite"), (61, "pan"), (66, "shut"),
    (76, "upright"), (82, "pour"), (88, "grasp_rod"), (94, "rod_out"), (100, "ram_up"), (106, "ram_down"),
    (112, "ram_up"), (118, "rod_out"), (126, "return_rod"), (138, "poise"), (155, "@aim"),
]
# The rammer: in its pipes, drawn up along the barrel, turned over, down the bore (rammed
# once), back (`aim._rod` modes); shown only while out of its pipes.
MOUNTED_ROD = [(88, "stowed"), (94, "slide"), (97, "swing"), (100, "bore"), (112, "bore"), (115, "swing"),
               (118, "slide"), (126, "stowed")]


def mounted_reload_plan(md_lines):
    """(keys [(frame, step)], rod [(frame, mode)]) of the mounted reload on the frames of
    `md_lines["ATTACK1"]` (156 for every dragoon; others scaled), or None without ATTACK1."""
    if "ATTACK1" not in md_lines:
        return None
    n = rig.md_clip_frames(md_lines["ATTACK1"])[0]
    k = (n - 1) / (MOUNTED_FRAMES - 1)

    def fit(seq):
        out = {}
        for f, x in seq:
            out[int(round(f * k))] = x
        return sorted(out.items())

    return fit(MOUNTED_RELOAD), fit(MOUNTED_ROD)


THRUST = {
    "spine": (-14, 0, 8), "spine1": (-6, 0, 0),
    "upperarm_r": (-40, -40, -10), "forearm_r": (-30, 0, 20),
    "upperarm_l": (-80, 40, 30), "forearm_l": (-5, 0, -10),
    "thigh_l": (-30, -3, 0), "shin_l": (25, 0, 0), "thigh_r": (14, 3, 0), "shin_r": (10, 0, 0),
    "root_loc": (0, -0.12, -0.04),
    "world": {"socket.hand_r": (95, 0, 0)},
}


def walk_cycle(stand):
    """Four keys of a 20-frame step cycle (MOTION_L = frames 0..9, MOTION_R = 10..19)."""
    contact_l = _with(stand, thigh_l=(-24, -3, 0), shin_l=(6, 0, 0), foot_l=(-6, 0, 0),
                      thigh_r=(16, 3, 0), shin_r=(22, 0, 0), foot_r=(14, 0, 0),
                      upperarm_l=(14, 28, 0), root_loc=(0, 0, -0.025))
    pass_l = _with(stand, thigh_l=(-4, -3, 0), shin_l=(4, 0, 0),
                   thigh_r=(-18, 3, 0), shin_r=(48, 0, 0), foot_r=(-10, 0, 0), root_loc=(0, 0, 0.0))
    contact_r = _with(stand, thigh_r=(-24, 3, 0), shin_r=(6, 0, 0), foot_r=(-6, 0, 0),
                      thigh_l=(16, -3, 0), shin_l=(22, 0, 0), foot_l=(14, 0, 0),
                      upperarm_l=(-14, 28, 0), root_loc=(0, 0, -0.025))
    pass_r = _with(stand, thigh_r=(-4, 3, 0), shin_r=(4, 0, 0),
                   thigh_l=(-18, -3, 0), shin_l=(48, 0, 0), foot_l=(-10, 0, 0), root_loc=(0, 0, 0.0))
    return contact_l, pass_l, contact_r, pass_r


def run_cycle(stand):
    """Four keys of a running step cycle (same order and frames as `walk_cycle`): the trunk
    leant forward, a longer reach at the strike, the knee flexed in mid-stance with the swing
    heel tucked up high, the left arm swinging wide. The carrying foot stays on the ground
    (both keys put it within 1 cm of it), so the gait layer's phase lock
    (`acr_physics::gait::Cycle`) measures the whole stride as travel (#295)."""
    lean = {"spine": (8, 0, 0), "spine1": (4, 0, 0), "head": (-10, 0, 0)}

    def lean_in(p):
        out = dict(p)
        for b, (x, y, z) in lean.items():
            bx, by, bz = out.get(b, (0, 0, 0))
            out[b] = (bx + x, by + y, bz + z)
        return out

    contact_l = _with(stand, thigh_l=(-28, -3, 0), shin_l=(6, 0, 0), foot_l=(-8, 0, 0),
                      thigh_r=(24, 3, 0), shin_r=(34, 0, 0), foot_r=(20, 0, 0),
                      upperarm_l=(24, 28, 0), root_loc=(0, 0, -0.08))
    pass_l = _with(stand, thigh_l=(-4, -3, 0), shin_l=(30, 0, 0), foot_l=(-24, 0, 0),
                   thigh_r=(-34, 3, 0), shin_r=(100, 0, 0), foot_r=(-6, 0, 0), root_loc=(0, 0, -0.05))
    contact_r = _with(stand, thigh_r=(-28, 3, 0), shin_r=(6, 0, 0), foot_r=(-8, 0, 0),
                      thigh_l=(24, -3, 0), shin_l=(34, 0, 0), foot_l=(20, 0, 0),
                      upperarm_l=(-24, 28, 0), root_loc=(0, 0, -0.08))
    pass_r = _with(stand, thigh_r=(-4, 3, 0), shin_r=(30, 0, 0), foot_r=(-24, 0, 0),
                   thigh_l=(-34, -3, 0), shin_l=(100, 0, 0), foot_l=(-6, 0, 0), root_loc=(0, 0, -0.05))
    return tuple(lean_in(p) for p in (contact_l, pass_l, contact_r, pass_r))


# Frames per `MOTION_*` step from which a clip is a walk: the European line walks 10 frames a
# step at 1.67 m/s (GEOMETRY step 40, Rate 16, measured 0.167 m per tick); the native foot
# runs 8 frames a step on its running sprites at 2.7-3.3 m/s (step 64-80), and a native
# stance with `RATE <s> 8` (SiuKri, PueKri `MOTION_L4`) walks 10 frames a step at 1.67 m/s.
WALK_FRAMES = 10


def motion_gait(line):
    """"run" or "walk" for a `MOTION_*` `.MD` line, from its frames a step (#295)."""
    return "walk" if rig.md_clip_frames(line)[0] >= WALK_FRAMES else "run"


def step_keys(md, stand):
    """{MOTION_* clip: [(fraction, pose)]} for every `MOTION_L/R/LB/RB[digit]` line in `md`:
    the run cycle or the walk cycle from `stand` by the line's frames (`motion_gait`). LB/RB
    play the step backwards."""
    cycles = {"walk": walk_cycle(stand), "run": run_cycle(stand)}
    out = {}
    for k, line in md.items():
        base = k.rstrip("0123456789")
        if base not in ("MOTION_L", "MOTION_R", "MOTION_LB", "MOTION_RB"):
            continue
        c_l, p_l, c_r, p_r = cycles[motion_gait(line)]
        out[k] = {"MOTION_L": [(0.0, c_l), (0.5, p_l), (1.0, c_r)],
                  "MOTION_R": [(0.0, c_r), (0.5, p_r), (1.0, c_l)],
                  "MOTION_LB": [(0.0, c_l), (0.5, p_r), (1.0, c_r)],
                  "MOTION_RB": [(0.0, c_r), (0.5, p_l), (1.0, c_l)]}[base]
    return out


def lie_prone(arms_out=True):
    """#DEATHLIE1 of the European foot: prone, head towards the facing direction, arms flung
    out above the head, feet near the ground point (fitted to EngM18's sprite with
    tools/silhouette.py --shift 1,3, the hips kept 13 cm above the ground)."""
    return {
        "root": (91, -8, 4), "root_loc": (-0.05, 0.39, 0.13),
        "spine": (0, -2, 6), "spine1": (0, 3, 0), "neck": (-4, 0, 0), "head": (-10, 18, 62),
        "clavicle_l": (0, -10, 0), "upperarm_l": (12, -45, 0) if arms_out else (0, 10, 0), "forearm_l": (0, -16, 0),
        "clavicle_r": (0, 6, 0), "upperarm_r": (-2, 47, 0) if arms_out else (0, -10, 0), "forearm_r": (0, 50, 0),
        "thigh_l": (-14, -5.5, 0), "shin_l": (13.5, 0, 0), "foot_l": (55, 0, 0),
        "thigh_r": (-10.5, 8, 0), "shin_r": (15, 0, 0), "foot_r": (50, 0, 0),
        # Skirt bones: the coat tails lie flat on the legs.
        "skirt_fl_01": (-80, 0, 0), "skirt_fr_01": (-80, 0, 0), "skirt_bl_01": (-70, 0, 0), "skirt_br_01": (-70, 0, 0),
        "world": {"socket.hand_r": (90, 0, 22)},
    }


def death_keys(stand, lie):
    """Fractions of the #DEATH clip: hit, knees give, fall forward, land, settle."""
    hit = _with(stand, spine=(10, 0, 0), spine1=(8, 0, 0), head=(14, 0, 0),
                upperarm_l=(-20, 10, 0), forearm_l=(-40, 0, 0))
    buckle = _with(stand, root_loc=(0, 0.05, -0.22), thigh_l=(-45, -4, 0), thigh_r=(-38, 4, 0),
                   shin_l=(80, 0, 0), shin_r=(70, 0, 0), foot_l=(-30, 0, 0), foot_r=(-30, 0, 0),
                   spine=(-12, 0, 0), spine1=(-6, 0, 0), head=(-10, 0, 0), upperarm_l=(-30, 10, 0),
                   forearm_l=(-30, 0, 0))
    fall = {k: v for k, v in lie.items() if k not in ("root", "root_loc")}
    a = 50
    fall.update({"root": (a, 0, 3),
                 "root_loc": (0.02, -0.35 + 0.95 * math.sin(math.radians(a)), 0.35 - 0.95 * math.cos(math.radians(a))),
                 "thigh_l": (-25, -6, 0), "shin_l": (40, 0, 0), "thigh_r": (-20, 5, 0),
                 "shin_r": (55, 0, 0), "upperarm_l": (-60, -60, 0), "upperarm_r": (-50, 40, 0)})
    land = dict(lie)
    land["root_loc"] = (lie["root_loc"][0], lie["root_loc"][1], lie["root_loc"][2] + 0.03)
    land["head"] = (-25, 10, 40)
    return [(0.0, stand), (0.14, hit), (0.40, buckle), (0.65, fall), (0.85, land), (1.0, lie)]


# Charge bayonets: the musket held level at the hip, both hands on it (`aim.hold` with a
# low pocket), the body leaning in.
CHARGE_MUSKET = {
    "spine": (-10, 0, -16), "spine2": (-4, 0, -10), "head": (6, 0, 20), "neck": (4, 0, 6),
    "thigh_l": (-14, -3, 0), "shin_l": (10, 0, 0), "thigh_r": (8, 3, 0),
    "world": {"socket.hand_r": (84, 0, 0)},
}


# Hip fire (#296, close range): the musket level at the right hip, the trunk upright over
# it, the eyes on the man in front (`aim.hip`).
HIP_MUSKET = {
    **STANCE, "spine": (-6, 0, -16), "spine2": (-2, 0, -10), "neck": (2, 0, 8), "head": (4, 0, 18),
    "world": {"socket.hand_r": (90, 0, 0)},
}


def _steps(reload):
    return reload or {k: _with(v["base"], world_loc={"socket.hand_r": v["grip"]}) for k, v in RELOAD_STEPS.items()}


def attack1_keys(aim, fire, reload=None, n=87, drill=None):
    """(fraction, pose) keys of ATTACK1 (and FIRE_AIMED, FIRE_HIP: #296): present, keyed on
    every frame up to the shot (the IK-solved hold, acr-blender-pipeline pitfall 5), recoil,
    the reload drill (`drill` = [(frame, step)] ending in the reload stance; default
    `RELOAD_FRAMES` and the present again; `reload` = the IK-solved steps, else the unsolved
    ones)."""
    steps = _steps(reload)
    last = n - 1
    keys = [(f / last, aim) for f in range(SHOT_FRAME)]
    keys += [(SHOT_FRAME / last, fire), ((SHOT_FRAME + 3) / last, aim)]
    keys += [(f / last, steps[name]) for f, name in (drill or RELOAD_FRAMES)]
    if drill is None:
        keys.append((1.0, aim))
    return keys


def stance4_clips(md_lines, aim, stand, reload=None):
    """{clip: [(frame, pose)]} of a slow-recharge musketeer's reload stance (`STANCE4_CLIPS`
    on the frames of `md_lines`); empty for the others."""
    if not slow_recharge(md_lines):
        return {}
    steps = dict(_steps(reload), **{"@aim": aim, "@stand": stand})
    return {clip: [(f, steps[s]) for f, s in keys] for clip, (keys, _rod) in reload_plan(md_lines).items()
            if clip != "ATTACK1"}


# Strike clips (docs/melee.md, Strike clips): the game plays `ATTACK_<KIND>_<HEIGHT>` (kind
# THRUST, CUT, SMASH, SHOT; height HIGH, MID, LOW, DOWN), else `ATTACK_<HEIGHT>`, else
# `ATTACK`, by the sim's strike of the attacker's last blow at this victim
# (`strike_view::strike_clip`). Each has the frame count of the `.MD` line ATTACK and is played
# on the same sprite frames. The blow lands at the active frame of ATTACK: without a
# `SETACTIVEPOINT #ATTACK` that is frame 1, rounded up to even: frame 2. So the clip opens at
# the end of the wind-up, hits at frame 2, recovers and winds up again for the next blow
# (ATTACK loops while the fight goes on).
STRIKE_HIT_FRAME = 2


def strike_keys(n, windup, hit, recover):
    """(fraction, pose) keys of a strike clip of `n` frames: the wind-up at frame 0, the blow
    at `STRIKE_HIT_FRAME`, held a frame, the recovery, the wind-up again at the end."""
    last = max(n - 1, 1)
    h = STRIKE_HIT_FRAME / last
    return [(0.0, windup), (h, hit), ((STRIKE_HIT_FRAME + 2) / last, hit), (0.35, recover), (0.8, windup),
            (1.0, windup)]


def musketeer_strikes(ready, lunge, n):
    """{clip: keys} of a musketeer's melee blows (attack index 0), from his charge pose `ready`
    and its `lunge`: the bayonet thrust, the thrust down at a man on the ground (a step in,
    both hands driving the point down), the butt smash and the point-blank shot down
    (`Shot`/`Down`: the sim fires a loaded musket at a man on the ground)."""
    drawn_back = _with(ready, spine=(-4, 0, -12), root_loc=(0, 0.06, -0.02), world={"socket.hand_r": (80, 0, 0)})
    # The point down at a man on the ground: a long step in with the left foot, the knees
    # bent, the trunk over the musket, the muzzle 45° down.
    down_ready = _with(ready, spine=(-22, 0, -14), spine1=(-8, 0, 0), root_loc=(0, 0.02, -0.08),
                       thigh_l=(-24, -3, 0), shin_l=(30, 0, 0), thigh_r=(10, 3, 0), shin_r=(18, 0, 0),
                       world={"socket.hand_r": (112, 0, 0)})
    stab_down = _with(ready, spine=(-38, 0, -12), spine1=(-16, 0, 0), spine2=(-8, 0, -8), head=(-10, 0, 18),
                      root_loc=(0, -0.30, -0.24), thigh_l=(-62, -3, 0), shin_l=(78, 0, 0), foot_l=(-14, 0, 0),
                      thigh_r=(22, 3, 0), shin_r=(46, 0, 0), foot_r=(-12, 0, 0),
                      world={"socket.hand_r": (138, 0, 0)})
    pull_out = _with(down_ready, root_loc=(0, -0.12, -0.14), thigh_l=(-40, -3, 0), shin_l=(50, 0, 0))
    # The butt smash: the musket swung round, butt first, from the right shoulder across.
    # Drawn back: the musket still pointing forward and up past the right hip, the butt low
    # behind; the hit: the butt driven forward and up at chest height, the muzzle tipped back
    # over the right shoulder (the musket turns about its own grip, no wild spin).
    butt_back = _with(ready, spine=(-4, 0, 18), spine2=(0, 0, 8), head=(4, 0, -14),
                      world={"socket.hand_r": (62, 0, 18)})
    butt_hit = _with(ready, spine=(-14, 0, -20), spine2=(-6, 0, -10), head=(6, 0, 20),
                     root_loc=(0, -0.10, -0.04), thigh_l=(-30, -3, 0), shin_l=(24, 0, 0),
                     world={"socket.hand_r": (-28, 0, -8)})
    # Point-blank down: the musket at the hip, the muzzle on the man at the feet, the kick.
    shot_aim = _with(down_ready, world={"socket.hand_r": (128, 0, 0)})
    shot_kick = _with(shot_aim, spine=(-14, 0, -14), root_loc=(0, 0.06, -0.06), world={"socket.hand_r": (118, 0, 0)})
    return {
        "ATTACK_THRUST_MID": strike_keys(n, drawn_back, lunge, ready),
        "ATTACK_THRUST_DOWN": strike_keys(n, down_ready, stab_down, pull_out),
        # By height alone (the first blow at a man on the ground): the thrust down.
        "ATTACK_DOWN": strike_keys(n, down_ready, stab_down, pull_out),
        "ATTACK_SMASH_MID": strike_keys(n, butt_back, butt_hit, ready),
        "ATTACK_SHOT_DOWN": strike_keys(n, shot_aim, shot_kick, shot_aim),
    }


def musketeer_clips(md_lines, stand=STAND_MUSKET, aim=None, charge=None, reload=None, hip=None):
    """{clip: [(frame, pose)]} for a European musketeer from its `.MD` lines
    ({key: "@DEATH 9 4 0 70"}): STAND, MOTION_L, MOTION_R, the bayonet (attack index 0,
    `PIKA`: PATTACK, UATTACK, PSTAND, ATTACK), the musket shot (index 1, `VISTREL`:
    PATTACK1, UATTACK1, PSTAND1, ATTACK1 = fire and reload, with `SETACTIVEPOINT #ATTACK1`),
    DEATH, DEATHLIE1. `aim` and `charge` are the IK-solved two-handed poses (`aim.hold`,
    `aim.present`). With `hip` (`aim.hip`) also the two presentations of the shot (#296) on
    ATTACK1's frames: FIRE_AIMED (= ATTACK1, from the shoulder) and FIRE_HIP (from the hip,
    close range); the game picks one by the target's distance."""
    lie = lie_prone()
    cl, pl, cr, pr = walk_cycle(stand)
    # `aim`: the present pose with both hands on the musket (`aim.hold`); the data pose
    # without it.
    AIM_MUSKET = aim or globals()["AIM_MUSKET"]
    ready = charge or CHARGE_MUSKET
    lunge = _with(ready, spine=(-18, 0, -16), spine1=(-6, 0, 0), root_loc=(0, -0.14, -0.05),
                  thigh_l=(-32, -3, 0), shin_l=(26, 0, 0), thigh_r=(16, 3, 0), shin_r=(12, 0, 0))
    # The kick: shoulder and body pushed back, the muzzle jumping up a little.
    rl = AIM_MUSKET.get("root_loc", (0, 0, 0))
    fire = _with(AIM_MUSKET, spine=(-4, 0, -2), root_loc=(rl[0], rl[1] + 0.04, rl[2]))
    n1 = rig.md_clip_frames(md_lines["ATTACK1"])[0] if "ATTACK1" in md_lines else 87
    # A slow-recharge musketeer's ATTACK1 ends in the reload stance (#255, `reload_plan`).
    drill = reload_plan(md_lines)["ATTACK1"][0] if slow_recharge(md_lines) else None
    clips = {
        "STAND": [(0.0, stand)],
        "MOTION_L": [(0.0, cl), (0.5, pl), (1.0, cr)],
        "MOTION_R": [(0.0, cr), (0.5, pr), (1.0, cl)],
        # Backwards: the forward halves reversed (the .MD plays R's and L's frames backwards).
        "MOTION_LB": [(0.0, cl), (0.5, pr), (1.0, cr)],
        "MOTION_RB": [(0.0, cr), (0.5, pl), (1.0, cl)],
        # Bayonet (index 0).
        "PATTACK": [(0.0, stand), (0.5, _with(ready, world={"socket.hand_r": (40, 0, 0)})), (1.0, ready)],
        "UATTACK": [(0.0, ready), (0.5, _with(ready, world={"socket.hand_r": (40, 0, 0)})), (1.0, stand)],
        "PSTAND": [(0.0, ready)],
        "ATTACK": [(0.0, ready), (0.35, lunge), (0.55, ready), (0.8, lunge), (1.0, ready)],
        # Musket shot (index 1): present, fire, reload, present again.
        "PATTACK1": [(0.0, stand), (0.5, _with(AIM_MUSKET, world={"socket.hand_r": (40, 0, 0)})), (1.0, AIM_MUSKET)],
        "UATTACK1": [(0.0, AIM_MUSKET), (0.5, _with(AIM_MUSKET, world={"socket.hand_r": (40, 0, 0)})), (1.0, stand)],
        "PSTAND1": [(0.0, AIM_MUSKET)],
        "ATTACK1": attack1_keys(AIM_MUSKET, fire, reload, n1, drill),
        "DEATH": death_keys(stand, lie),
        "DEATHLIE1": [(0.0, lie)],
    }
    if hip is not None and "ATTACK1" in md_lines:
        hl = hip.get("root_loc", (0, 0, 0))
        hip_fire = _with(hip, spine=(-2, 0, -14), root_loc=(hl[0], hl[1] + 0.03, hl[2]))
        clips["FIRE_AIMED"] = clips["ATTACK1"]
        clips["FIRE_HIP"] = attack1_keys(hip, hip_fire, reload, n1, drill)
    out = {}
    for key, keys in clips.items():
        line = md_lines.get("ATTACK1" if key.startswith("FIRE_") else key)
        n = rig.md_clip_frames(line)[0] if line else 1
        out[key] = rig.clip_keys(n, keys)
    if "ATTACK" in md_lines:
        n = rig.md_clip_frames(md_lines["ATTACK"])[0]
        for key, keys in musketeer_strikes(ready, lunge, n).items():
            out[key] = rig.clip_keys(n, keys)
    out.update(stance4_clips(md_lines, AIM_MUSKET, stand, reload))
    return out
