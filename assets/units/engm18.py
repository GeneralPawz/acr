"""English musketeer, 18th century (`EngM18`, `ENGM18.MD`): red coat with open skirts,
white waistcoat and breeches, black gaiters, tricorn, buff crossbelts, knapsack, cartridge
box, Brown Bess. Skeleton H with all layers of #178/#183 (docs/assets-humans.md).

Colours are projected from the `#STAND` sprites (lighting reduced, mirrored directions for
the symmetric layers); without a game folder the palette below is used.
"""

import math

from mathutils import Matrix, Vector

from acr_assets import armature as am
from acr_assets import gear, poses, rig
from acr_assets.humans import Character, prepare_export

TYPE = "EngM18"
# Sprite checks run by `cargo xtask assets --check` (arguments of `acr model-check`).
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

# Animation lines of ENGM18.MD: the clips are keyed on their frame counts.
MD = {
    "STAND": "#STAND 9 1 5 0",
    "MOTION_L": "@MOTION_L 9 1 0 9",
    "MOTION_R": "@MOTION_R 9 1 10 19",
    "MOTION_LB": "@MOTION_LB 9 1 19 10",
    "MOTION_RB": "@MOTION_RB 9 1 9 0",
    "PATTACK": "@PATTACK 9 5 0 35",
    "UATTACK": "@UATTACK 9 5 35 0",
    "PSTAND": "#PSTAND 9 1 5 35",
    "ATTACK": "@ATTACK 9 6 0 70",
    "ATTACK1": "@ATTACK1 9 3 0 86",
    "DEATH": "@DEATH 9 4 0 70",
    "DEATHLIE1": "#DEATHLIE1 9 1 4 70",
}

# Flat colours (sRGB) picked from the sprites, used without a game folder and for faces no
# sprite view sees.
PALETTE = {
    "skin": (0.80, 0.58, 0.45), "bone": (0.85, 0.81, 0.69),
    "coat": (0.66, 0.09, 0.06), "waistcoat": (0.86, 0.85, 0.80), "breeches": (0.86, 0.85, 0.80),
    "gaiters": (0.10, 0.10, 0.10), "belts": (0.85, 0.75, 0.30), "hat": (0.07, 0.07, 0.07),
    "cartridge": (0.08, 0.07, 0.06), "knapsack": (0.40, 0.27, 0.14), "wood": (0.42, 0.24, 0.12),
    "metal": (0.55, 0.55, 0.57),
}

# Body build and part sizes fitted to the sprite silhouette (#STAND; tools/silhouette.py).
BUILD = {"width": 1.02, "shoulders": 1.12, "limbs": 1.23, "hips": 1.0}
FIT = {
    "hat_z": -0.02, "hat_scale": 0.925, "musket_len": 1.69,
    "skirt_top": [0.245, 0.175], "skirt_bot": [0.21, 0.15], "coat": 0.033,
    "pack": [0.32, 0.10, 0.32], "pack_y": 0.1925, "skirt_gap": 72,
}

TORSO = {"hips", "spine", "spine1", "spine2", "clavicle_l", "clavicle_r"}
ARMS = {"upperarm_l", "upperarm_r", "forearm_l", "forearm_r"}
LEGS = {"thigh_l", "thigh_r"}
SHINS = {"shin_l", "shin_r"}
FEET = {"foot_l", "foot_r", "toe_l", "toe_r"}


def make(root, name, build=BUILD):
    """The musketeer under `root` (also used by other units of the same cut)."""
    ch = Character(root, name, build)
    ch.body()

    def front(c):
        return c.y < -0.04 and abs(c.x) < 0.075 and c.z < 1.41

    ch.shell("waistcoat", "waistcoat", lambda c, n, h, p: h in TORSO and 0.97 < c.z < 1.47, 0.012)
    ch.shell("coat", "coat", lambda c, n, h, p: (h in TORSO and 0.97 < c.z < 1.49 and not front(c)) or h in ARMS, FIT["coat"])
    ch.shell("cuffs", "coat", lambda c, n, h, p: h in {"forearm_l", "forearm_r"} and c.z < 1.02, 0.045)
    ch.shell("breeches", "breeches", lambda c, n, h, p: (h in {"hips"} | LEGS and c.z < 1.0) or (h in SHINS and c.z > 0.40), 0.010)
    ch.shell("gaiters", "gaiters", lambda c, n, h, p: (h in SHINS and c.z <= 0.44) or h in FEET, 0.016)
    from acr_assets import body as bd

    skirt = bd.tube(ch.arm, f"{name}.coat_skirt", rig.SKIRT_TOP, rig.SKIRT_BOTTOM, FIT["skirt_top"], FIT["skirt_bot"],
                    gap_deg=FIT["skirt_gap"], flare_back=0.03, columns=rig.SKIRT_COLUMNS)
    ch.add("clothes", skirt, "coat", symmetric=True)
    b1 = bd.strap(ch.arm, f"{name}.crossbelts", ch.flesh, Vector((0.12, 0, 1.47)), Vector((-0.17, 0, 0.95)), 0.06, 0.038)
    b2 = bd.strap(ch.arm, f"{name}.crossbelt2", ch.flesh, Vector((-0.12, 0, 1.47)), Vector((0.17, 0, 0.95)), 0.06, 0.042)
    ch.add("clothes", b1, "belts", symmetric=True)
    ch.add("clothes", b2, "belts", symmetric=True)
    ch.bone_layer()
    ch.caps()

    # Strapped gear (rest pose, rigid on deforming bones: follows `deflate`).
    box = gear.box(f"{name}.cartridge_box", (0.16, 0.06, 0.12))
    gear.rigid(box, ch.arm, "hips", Matrix.Translation((-0.15, 0.17, 0.97)) @ Matrix.Rotation(math.radians(30), 4, "Z"))
    ch.add("gear", box, "cartridge")
    pack = gear.box(f"{name}.knapsack", tuple(FIT["pack"]))
    gear.rigid(pack, ch.arm, "spine2", Matrix.Translation((0, FIT["pack_y"], 1.28)))
    ch.add("gear", pack, "knapsack")

    # Held gear on sockets, placed in the standing pose (drops at death).
    am.pose(ch.arm, poses.STAND_MUSKET)
    hat = gear.tricorn(f"{name}.tricorn")
    am.attach(hat, ch.arm, "socket.hat", Matrix.Translation(am.world(ch.arm, "socket.hat").translation + Vector((0, 0, FIT["hat_z"])))
              @ Matrix.Rotation(math.radians(-6), 4, "X") @ Matrix.Scale(FIT["hat_scale"], 4))
    ch.held["hat"] = hat
    ch.keys[hat.name] = "hat"
    gun = gear.musket(f"{name}.musket", length=FIT["musket_len"], bayonet=0.0)
    gun["acr_material_keys"] = ["wood", "metal"]
    # The socket's orientation is "upright" in its rest frame; the clips turn it.
    sock = am.world(ch.arm, "socket.hand_r")
    rest_up = sock.to_3x3() @ ch.arm.pose.bones["socket.hand_r"].bone.matrix_local.to_3x3().inverted()
    am.attach(gun, ch.arm, "socket.hand_r", Matrix.Translation(sock.translation + Vector((0.0, -0.01, 0.0))) @ rest_up.to_4x4())
    ch.held["musket"] = gun
    ch.keys[gun.name] = "wood"
    return ch


def build(ctx):
    ch = make(ctx.root, ctx.name)
    ch.morphs(extra_deflate=ch.layers["gear"])
    ch.masks()
    for clip, keys in poses.musketeer_clips(MD).items():
        am.add_clip(ch.arm, clip, keys)
    am.set_clip(ch.arm, "STAND")
    report = {}
    colours = ch.colours(ctx.ref, PALETTE, report=report)
    # The exported scene pose (glTF node transforms) is STAND.
    ch.arm.animation_data.action = None
    am.pose(ch.arm, poses.STAND_MUSKET)
    tris = ch.triangles()
    prepare_export(ch.all_meshes(), ctx.root, acr_triangles=sum(tris.values()), acr_clips=list(MD),
                   acr_light=report.get("light_dir_camera", []), acr_layers=list(ch.layers))
    ctx.stats = {"triangles": tris, "light": report, "colours": colours}


def pose_for(ch, clip):
    """Shows the first frame of `clip` (the last of DEATH) for the silhouette tool."""
    keys = poses.musketeer_clips(MD)[clip]
    ch._pose_spec = keys[-1][1] if clip == "DEATH" else keys[0][1]
    am.pose(ch.arm, ch._pose_spec)
