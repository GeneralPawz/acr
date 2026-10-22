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

# Present (aim): butt at the right shoulder, left hand under the barrel.
AIM_MUSKET = {
    # Torso turned so the left shoulder comes forward (the firing stance), head turned back
    # along the musket and bowed onto the stock.
    "spine": (-6, 0, -20), "spine2": (-4, 0, -14), "head": (16, -8, 26), "neck": (8, 0, 10),
    "upperarm_r": (-62, -50, -20), "forearm_r": (-60, 0, 30), "hand_r": (0, 0, 0),
    "upperarm_l": (-70, 50, 30), "forearm_l": (-20, 0, -20),
    "thigh_l": (-8, -3, 0), "shin_l": (8, 0, 0), "thigh_r": (6, 3, 0),
    "world": {"socket.hand_r": AIMED},
}

LOADING = {
    "spine": (-6, 0, 0),
    "upperarm_r": (-20, -20, 0), "forearm_r": (-90, 0, 10),
    "upperarm_l": (-30, 30, 10), "forearm_l": (-60, 0, -20),
    "world": {"socket.hand_r": LOWERED},
}

THRUST = {
    "spine": (-14, 0, 8), "spine1": (-6, 0, 0),
    "upperarm_r": (-40, -40, -10), "forearm_r": (-30, 0, 20),
    "upperarm_l": (-80, 40, 30), "forearm_l": (-5, 0, -10),
    "thigh_l": (-30, -3, 0), "shin_l": (25, 0, 0), "thigh_r": (14, 3, 0), "shin_r": (10, 0, 0),
    "root_loc": (0, -0.12, -0.04),
    "world": {"socket.hand_r": (95, 0, 0)},
}


def _with(base, **over):
    d = dict(base)
    world = dict(base.get("world", {}))
    world.update(over.pop("world", {}))
    d.update(over)
    d["world"] = world
    return d


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


def musketeer_clips(md_lines, stand=STAND_MUSKET, aim=None):
    """{clip: [(frame, pose)]} for a European musketeer from its `.MD` lines
    ({key: "@DEATH 9 4 0 70"}): STAND, MOTION_L, MOTION_R, PATTACK, UATTACK, PSTAND,
    ATTACK (fire and reload), ATTACK1 (bayonet), DEATH, DEATHLIE1."""
    lie = lie_prone()
    cl, pl, cr, pr = walk_cycle(stand)
    # `aim`: the present pose with both hands on the musket (`aim.hold`); the data pose
    # without it.
    AIM_MUSKET = aim or globals()["AIM_MUSKET"]
    fire = _with(AIM_MUSKET, spine=(4, 0, 6), head=AIM_MUSKET.get("head", (12, 0, -10)), root_loc=(0, 0.03, 0))
    clips = {
        "STAND": [(0.0, stand)],
        "MOTION_L": [(0.0, cl), (0.5, pl), (1.0, cr)],
        "MOTION_R": [(0.0, cr), (0.5, pr), (1.0, cl)],
        # Backwards: the forward halves reversed (the .MD plays R's and L's frames backwards).
        "MOTION_LB": [(0.0, cl), (0.5, pr), (1.0, cr)],
        "MOTION_RB": [(0.0, cr), (0.5, pl), (1.0, cl)],
        "PATTACK": [(0.0, stand), (0.5, _with(AIM_MUSKET, world={"socket.hand_r": (40, 0, 0)})), (1.0, AIM_MUSKET)],
        "UATTACK": [(0.0, AIM_MUSKET), (0.5, _with(AIM_MUSKET, world={"socket.hand_r": (40, 0, 0)})), (1.0, stand)],
        "PSTAND": [(0.0, AIM_MUSKET)],
        "ATTACK": [(0.0, AIM_MUSKET), (0.06, fire), (0.15, AIM_MUSKET), (0.3, LOADING), (0.7, LOADING),
                   (0.85, _with(AIM_MUSKET, world={"socket.hand_r": (40, 0, 0)})), (1.0, AIM_MUSKET)],
        "ATTACK1": [(0.0, AIM_MUSKET), (0.35, THRUST), (0.55, AIM_MUSKET), (0.8, THRUST), (1.0, AIM_MUSKET)],
        "DEATH": death_keys(stand, lie),
        "DEATHLIE1": [(0.0, lie)],
    }
    out = {}
    for key, keys in clips.items():
        line = md_lines.get(key)
        n = rig.md_clip_frames(line)[0] if line else 1
        out[key] = rig.clip_keys(n, keys)
    return out
