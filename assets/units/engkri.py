"""English peasant (`EngKri`, `ENGKRI.MD`): dark red-brown tunic to mid-thigh with a red
sash, light breeches and stockings, brown shoes, long brown hair under a slouch hat. Skeleton
H with all layers of #178/#183, like the musketeer (`engm18.py`), no gear in the hands yet
(the work packs `_Drv`, `_Str`, ... show axe, pick, sickle: later).

Clips on ENGKRI.MD's frame counts: STAND, REST, MOTION_L/R/LB/RB, WORK (a two-handed swing),
ATTACK (a club blow), DEATH, DEATHLIE1; the numbered work variants fall back to these.
"""

import math

from mathutils import Matrix, Vector

from acr_assets import crawl
from acr_assets import armature as am
from acr_assets import body as bd
from acr_assets import gear, outfit, poses, rig
from acr_assets import faces, heads
from acr_assets.humans import LAYER_OF, Character, prepare_export

TYPE = "EngKri"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

MD = {
    "STAND": "#STAND 9 1 12 0",
    "REST": "@REST 9 12 0 52",
    "MOTION_L": "@MOTION_L 9 0 0 9",
    "MOTION_R": "@MOTION_R 9 0 10 19",
    "MOTION_LB": "@MOTION_LB 9 0 19 10",
    "MOTION_RB": "@MOTION_RB 9 0 9 0",
    "WORK": "@WORK 9 9 0 21",
    "ATTACK": "@ATTACK 9 7 0 53",
    "DEATH": "@DEATH 9 11 0 45",
    "DEATHLIE1": "#DEATHLIE1 9 1 11 45",
}

PALETTE = {
    "skin": (0.80, 0.58, 0.45), "bone": (0.85, 0.81, 0.69), "hair": (0.45, 0.33, 0.20),
    "tunic": (0.36, 0.11, 0.09), "sash": (0.62, 0.07, 0.06), "breeches": (0.78, 0.74, 0.60),
    "shoes": (0.22, 0.14, 0.08), "hat": (0.47, 0.36, 0.22),
}

BUILD = {"width": 0.97, "belly": 1.05, "limbs": 1.1}

TORSO = {"hips", "spine", "spine1", "spine2", "clavicle_l", "clavicle_r"}
ARMS = {"upperarm_l", "upperarm_r", "forearm_l", "forearm_r"}
LEGS = {"thigh_l", "thigh_r"}
SHINS = {"shin_l", "shin_r"}
FEET = {"foot_l", "foot_r", "toe_l", "toe_r"}

# Fitted to #STAND with tools/silhouette.py (head nod capped).
STAND = {
    "upperarm_l": (20, 22, 0), "forearm_l": (-2, 0, 0),
    "upperarm_r": (20, -21, 0), "forearm_r": (11, 0, 0),
    "thigh_l": (0, -4, 0), "thigh_r": (0, 1, 0), "spine": (-12, 0, 0), "head": (20, 0, 0),
}
SWING_UP = dict(STAND, upperarm_l=(-150, 10, 0), upperarm_r=(-150, -10, 0), forearm_l=(-20, 0, 0), forearm_r=(-20, 0, 0),
                spine=(-8, 0, 0))
SWING_DOWN = dict(STAND, upperarm_l=(-50, 30, 0), upperarm_r=(-50, -30, 0), forearm_l=(-10, 0, 0), forearm_r=(-10, 0, 0),
                  spine=(30, 0, 0), spine1=(10, 0, 0), thigh_l=(-10, -4, 0), thigh_r=(-10, 4, 0), shin_l=(15, 0, 0), shin_r=(15, 0, 0))
REST_POSE = dict(STAND, head=(14, 0, 8), spine=(2, 0, 4), upperarm_l=(-2, 30, 0), upperarm_r=(-2, -30, 0))


def clips():
    cl, pl, cr, pr = poses.walk_cycle(STAND)
    lie = poses.lie_prone()
    keys = {
        "STAND": [(0.0, STAND)],
        "REST": [(0.0, STAND), (0.4, REST_POSE), (0.7, REST_POSE), (1.0, STAND)],
        "MOTION_L": [(0.0, cl), (0.5, pl), (1.0, cr)],
        "MOTION_R": [(0.0, cr), (0.5, pr), (1.0, cl)],
        "MOTION_LB": [(0.0, cl), (0.5, pr), (1.0, cr)],
        "MOTION_RB": [(0.0, cr), (0.5, pl), (1.0, cl)],
        "WORK": [(0.0, STAND), (0.45, SWING_UP), (0.65, SWING_DOWN), (1.0, STAND)],
        "ATTACK": [(0.0, STAND), (0.4, SWING_UP), (0.55, SWING_DOWN), (1.0, STAND)],
        "DEATH": poses.death_keys(STAND, lie),
        "DEATHLIE1": [(0.0, lie)],
    }
    return {k: rig.clip_keys(rig.md_clip_frames(MD[k])[0], v) for k, v in keys.items()}


def make(root, name):
    ch = Character(root, name, BUILD)
    ch.body()
    ch.shell("tunic", "tunic", lambda c, n, h, p: (h in TORSO and 0.95 < c.z < 1.48) or h in ARMS, 0.018)
    ch.shell("sash", "sash", lambda c, n, h, p: h in TORSO and 1.00 < c.z < 1.10, 0.032)
    ch.shell("breeches", "breeches", lambda c, n, h, p: (h in {"hips"} | LEGS and c.z < 1.0) or (h in SHINS and c.z > 0.11), 0.009)
    ch.shell("shoes", "shoes", lambda c, n, h, p: h in FEET or (h in SHINS and c.z <= 0.11), 0.012)
    skirt = bd.tube(ch.arm, f"{name}.tunic_skirt", rig.SKIRT_TOP, 0.70, (0.17, 0.13), (0.205, 0.17), columns=rig.SKIRT_COLUMNS)
    ch.add("clothes", skirt, "tunic", symmetric=True)
    ch.bone_layer()
    ch.caps()
    am.pose(ch.arm, STAND)
    hat = gear.slouch_hat(f"{name}.hat")
    am.attach(hat, ch.arm, "socket.hat", Matrix.Translation(am.world(ch.arm, "socket.hat").translation + Vector((0, 0.005, 0.03)))
              @ Matrix.Rotation(math.radians(-8), 4, "X"))
    ch.held["hat"] = hat
    ch.keys[hat.name] = "hat"
    return ch


def pose_for(ch, clip):
    ks = clips()[clip]
    ch._pose_spec = ks[-1][1] if clip == "DEATH" else ks[0][1]
    am.pose(ch.arm, ch._pose_spec)


def build(ctx):
    ch = make(ctx.root, ctx.name)
    heads.variants(ch)  # hair and beard variants (#283), before the morphs
    ch.morphs()
    ch.masks()
    for clip, keys in clips().items():
        am.add_clip(ch.arm, clip, keys)
    # Crawl clips of the wounded (#262): keyframed, looping.
    for clip, keys in crawl.clips().items():
        am.add_clip(ch.arm, clip, keys)
    am.set_clip(ch.arm, "STAND")
    colours = ch.colours(ctx.ref, PALETTE, mix=0.5)
    outfit.dress(ch, colours, LAYER_OF)
    heads.finish(ch)  # face and body morphs, `_FACE`, variants (#283)
    ch.arm.animation_data.action = None
    am.pose(ch.arm, STAND)
    tris = ch.triangles()
    prepare_export(ch.all_meshes(), ctx.root, acr_triangles=sum(tris.values()), acr_clips=list(MD) + list(crawl.CLIPS),
                   acr_limb_radius=ch.limb_radius, acr_layers=list(ch.layers),
                   **faces.extras("civilian"))
    ctx.stats = {"triangles": tris, "colours": colours}
