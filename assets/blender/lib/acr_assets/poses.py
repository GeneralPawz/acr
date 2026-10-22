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
AIM_MUSKET = dict(STANCE, **{
    "spine": (-10, 0, -4), "spine1": (-4, 0, 0), "spine2": (-2, 0, -6),
    "neck": (14, -6, 18), "head": (12, -18, 26),
    "upperarm_r": (-62, -50, -20), "forearm_r": (-60, 0, 30), "hand_r": (0, 0, 0),
    "upperarm_l": (-70, 50, 30), "forearm_l": (-20, 0, -20),
    "world": {"socket.hand_r": AIMED},
})

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
BUTT_AT_FOOT = (-0.10, -0.33, 0.03)
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
    "rod_out": _step(LOAD_BODY, CAST, BUTT_AT_FOOT, right=1.92, left=0.95, butt=True, elbow_r=ELBOW_UP),
    "ram_up": _step(LOAD_BODY, CAST, BUTT_AT_FOOT, right=1.80, left=0.95, butt=True, elbow_r=ELBOW_UP),
    "ram_down": _step(LOOK_DOWN, CAST, BUTT_AT_FOOT, right=1.52, left=0.95, butt=True, elbow_r=ELBOW_UP),
    "return_rod": _step(LOOK_DOWN, CAST, BUTT_AT_FOOT, right=1.20, left=0.95, butt=True, elbow_r=ELBOW_UP),
    # Make ready: the musket raised upright before the right side, then presented.
    "poise": _step(LOAD_BODY, POISE, (-0.17, -0.16, 1.02), right="grip", left=0.42),
}

# Frames of ATTACK1 (87 sprite frames, 0..86): the shot leaves at the active frame 17
# (SETACTIVEPOINT #ATTACK1 17, rounded up to even by the engine), then the drill.
SHOT_FRAME = 18
RELOAD_FRAMES = [
    (26, "prime"), (29, "half_cock"), (33, "cartridge"), (37, "bite"), (41, "pan"), (44, "shut"),
    (49, "cast"), (52, "pour"), (55, "grasp_rod"), (58, "rod_out"), (61, "ram_up"), (64, "ram_down"),
    (67, "ram_up"), (70, "ram_down"), (73, "rod_out"), (76, "return_rod"), (80, "poise"),
]

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


def attack1_keys(aim, fire, reload=None, n=87):
    """(fraction, pose) keys of ATTACK1: present until the shot, recoil, the reload drill
    (`RELOAD_FRAMES`; `reload` = the IK-solved steps, else the unsolved ones), present."""
    steps = reload or {k: _with(v["base"], world_loc={"socket.hand_r": v["grip"]}) for k, v in RELOAD_STEPS.items()}
    last = n - 1
    keys = [(0.0, aim), ((SHOT_FRAME - 1) / last, aim), (SHOT_FRAME / last, fire), ((SHOT_FRAME + 3) / last, aim)]
    keys += [(f / last, steps[name]) for f, name in RELOAD_FRAMES]
    keys.append((1.0, aim))
    return keys


def musketeer_clips(md_lines, stand=STAND_MUSKET, aim=None, charge=None, reload=None):
    """{clip: [(frame, pose)]} for a European musketeer from its `.MD` lines
    ({key: "@DEATH 9 4 0 70"}): STAND, MOTION_L, MOTION_R, the bayonet (attack index 0,
    `PIKA`: PATTACK, UATTACK, PSTAND, ATTACK), the musket shot (index 1, `VISTREL`:
    PATTACK1, UATTACK1, PSTAND1, ATTACK1 = fire and reload, with `SETACTIVEPOINT #ATTACK1`),
    DEATH, DEATHLIE1. `aim` and `charge` are the IK-solved two-handed poses (`aim.hold`)."""
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
        "ATTACK1": attack1_keys(AIM_MUSKET, fire, reload, rig.md_clip_frames(md_lines["ATTACK1"])[0] if "ATTACK1" in md_lines else 87),
        "DEATH": death_keys(stand, lie),
        "DEATHLIE1": [(0.0, lie)],
    }
    out = {}
    for key, keys in clips.items():
        line = md_lines.get(key)
        n = rig.md_clip_frames(line)[0] if line else 1
        out[key] = rig.clip_keys(n, keys)
    return out
