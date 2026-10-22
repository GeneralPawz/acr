"""First 3D run on humans (M2, #169/#178): skeleton H, layered musketeer (EngM18) and peasant
(EngKri), decay prototype, renders from the original camera, glTF export.

    blender -b --factory-startup --python assets/blender/humans/build_humans.py -- \
            --out OUT_DIR [--refs REF_DIR] [--only stand,decay,free,turn,glb] [--samples N]

REF_DIR holds `acr model-ref` folders named <Type>-<ANIM> (ref.json: canvas size and anchor
per animation); without it the default canvases below are used. Writes PNG renders,
engm18.glb and stats.json into OUT_DIR; tools/compose.py builds the comparison sheets.
Uses the pipeline's conventions (assets/blender/lib/acr_assets, docs/assets.md).
"""

import json
import math
import os
import sys
import time

import bpy
from mathutils import Matrix, Vector

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, "..", "lib"))

from acr_assets import conventions as cv  # noqa: E402

import h_body  # noqa: E402
import h_decay  # noqa: E402
import h_rig  # noqa: E402

# Canvas (width, height, anchor) per type/animation when no refs are given (from model-ref).
DEFAULT_CANVAS = {
    ("EngM18", "STAND"): (64, 110, (29, 87)),
    ("EngM18", "DEATHLIE1"): (112, 83, (61, 38)),
    ("EngKri", "STAND"): (57, 77, (27, 62)),
}

# Colours (sRGB) picked from the sprites (tools/sprite_study.py), toned for the lighting.
SKIN = (0.80, 0.58, 0.45)
COL = {
    "coat": (0.66, 0.09, 0.06), "waistcoat": (0.86, 0.85, 0.80), "breeches": (0.86, 0.85, 0.80),
    "gaiters": (0.10, 0.10, 0.10), "belts": (0.85, 0.75, 0.30), "tricorn": (0.07, 0.07, 0.07),
    "cartridge": (0.08, 0.07, 0.06), "knapsack": (0.40, 0.27, 0.14), "wood": (0.42, 0.24, 0.12),
    "metal": (0.55, 0.55, 0.57), "bone": (0.85, 0.81, 0.69),
    "tunic": (0.36, 0.11, 0.09), "sash": (0.62, 0.07, 0.06), "kbreeches": (0.78, 0.74, 0.60),
    "shoes": (0.22, 0.14, 0.08), "hat": (0.47, 0.36, 0.22), "hair": (0.45, 0.33, 0.20),
}

TORSO = {"hips", "spine", "spine1", "spine2", "clavicle_l", "clavicle_r"}
ARMS = {"upperarm_l", "upperarm_r", "forearm_l", "forearm_r"}
LEGS = {"thigh_l", "thigh_r"}

# ----------------------------------------------------------------------------- poses
# Degrees about the armature axes of the rest pose, relative to the parent (h_rig.pose):
# +X tips a bone's far end backwards (+Y), -X forwards; +Y swings a left limb inwards.

STAND_M18 = {
    "upperarm_l": (0, 28, 0), "forearm_l": (-12, 0, 0),
    "upperarm_r": (-6, -27, 0), "forearm_r": (-38, 0, 6), "hand_r": (0, 0, -10),
    "thigh_l": (0, -2, 0), "thigh_r": (0, 2, 0),
}
STAND_KRI = {
    "upperarm_l": (-4, 22, 0), "forearm_l": (-18, 0, 0),
    "upperarm_r": (-4, -22, 0), "forearm_r": (-18, 0, 0),
    "thigh_l": (0, -4, 0), "thigh_r": (0, 4, 0), "spine": (-3, 0, 0), "head": (6, 0, 0),
}


def _root_for(angle_deg, hip_target):
    """root rotation/location that pitches the body forward by angle about the ground point
    and puts the hip joint at hip_target."""
    a = math.radians(angle_deg)
    hip = Vector((0, -0.95 * math.sin(a), 0.95 * math.cos(a)))
    return tuple(Vector(hip_target) - hip)


def lie_pose():
    """#DEATHLIE1: prone, head towards the facing direction, arms flung out (as the sprite),
    feet near the ground point."""
    return {
        "root": (88, 0, 6), "root_loc": _root_for(88, (0.03, -0.52, 0.115)),
        "spine": (0, 6, 4), "spine1": (0, 3, 0), "neck": (-4, 0, 0), "head": (-10, 18, 62),
        "clavicle_l": (0, -10, 0), "upperarm_l": (0, -105, 0), "forearm_l": (0, -20, 0),
        "clavicle_r": (0, 6, 0), "upperarm_r": (0, 55, 0), "forearm_r": (0, 35, 0),
        "thigh_l": (-4, -9, 0), "shin_l": (22, 0, 0), "foot_l": (55, 0, 0),
        "thigh_r": (2, 7, 0), "shin_r": (48, 0, 0), "foot_r": (55, 0, 0),
    }


def death_keys(stand):
    """Placeholder #DEATH clip keyed on sprite frame numbers 0..70 (EngM18 plays 71 frames):
    hit, knees give, fall forward, land, settle into #DEATHLIE1."""
    lie = lie_pose()
    hit = dict(stand)
    hit.update({"spine": (10, 0, 0), "spine1": (8, 0, 0), "head": (14, 0, 0),
                "upperarm_l": (-20, 10, 0), "forearm_l": (-40, 0, 0)})
    buckle = dict(stand)
    buckle.update({"root_loc": (0, 0.05, -0.22), "thigh_l": (-45, -4, 0), "thigh_r": (-38, 4, 0),
                   "shin_l": (80, 0, 0), "shin_r": (70, 0, 0), "foot_l": (-30, 0, 0),
                   "foot_r": (-30, 0, 0), "spine": (-12, 0, 0), "spine1": (-6, 0, 0),
                   "head": (-10, 0, 0), "upperarm_l": (-30, 10, 0), "forearm_l": (-30, 0, 0)})
    fall = {k: v for k, v in lie.items() if k not in ("root", "root_loc")}
    fall.update({"root": (50, 0, 3), "root_loc": _root_for(50, (0.02, -0.35, 0.35)),
                 "thigh_l": (-25, -6, 0), "shin_l": (40, 0, 0), "thigh_r": (-20, 5, 0),
                 "shin_r": (55, 0, 0), "upperarm_l": (-60, -60, 0), "upperarm_r": (-50, 40, 0)})
    land = dict(lie)
    land["root_loc"] = tuple(Vector(lie["root_loc"]) + Vector((0, 0, 0.03)))
    land["head"] = (-25, 10, 40)
    return [(0, stand), (10, hit), (28, buckle), (46, fall), (60, land), (70, lie)]


# ----------------------------------------------------------------------------- characters

def mats(prefix):
    m = {}

    def mk(key, rgb, layer, **kw):
        m[key] = h_decay.material(f"{prefix}{key}", rgb, layer, **kw)

    mk("skin", SKIN, "flesh", roughness=0.7)
    mk("bone", COL["bone"], "bone", roughness=0.6)
    for k in ("coat", "tunic", "sash"):
        mk(k, COL[k], "wool")
    for k in ("waistcoat", "breeches", "kbreeches"):
        mk(k, COL[k], "linen")
    for k in ("gaiters", "belts", "tricorn", "cartridge", "knapsack", "shoes", "hat"):
        mk(k, COL[k], "leather")
    mk("hair", COL["hair"], "hair")
    mk("wood", COL["wood"], "wood")
    mk("metal", COL["metal"], "metal", roughness=0.4, metallic=0.6)
    return m


def _band(c, a, b, half=0.03):
    """c lies within `half` of the plane through the line a-b that contains the Y axis."""
    n = (b - a).normalized().cross(Vector((0, 1, 0))).normalized()
    return abs((c - a).dot(n)) < half


def _finish(arm, layers, prefix, stand):
    for objs in layers.values():
        for o in objs:
            h_decay.add_mask(o, o.data.materials[0]["acr_decay"]["layer"],
                             seed=sum(map(ord, o.name)))
    add_actions(arm, stand)


def musketeer(prefix="engm18.", state="stand"):
    """EngM18. state 'stand' (gear on sockets) or 'lie' (#DEATHLIE1, dropped gear placed on
    the ground). Returns (armature, {layer: [objects]}, {name: gear object})."""
    M = mats(prefix)
    arm = h_rig.build(prefix + "H")
    body = h_body.body(arm, prefix + "body")
    bones = h_body.bones_mesh(arm, prefix + "bones")
    d = h_body.derive

    def front(c):
        return c.y < -0.04 and abs(c.x) < 0.075 and c.z < 1.41

    waistcoat = d(body, prefix + "waistcoat", lambda c, n, h: h in TORSO and 0.97 < c.z < 1.47, 0.012)
    coat = d(body, prefix + "coat", lambda c, n, h: (h in TORSO and 0.97 < c.z < 1.49 and not front(c))
             or h in ARMS, 0.026)
    skirt = h_body.skirt(arm, prefix + "coat_skirt", 1.0, 0.55, (0.17, 0.13), (0.215, 0.19),
                         gap_deg=60, flare_back=0.03)
    breeches = d(body, prefix + "breeches", lambda c, n, h: (h in {"hips"} | LEGS and c.z < 1.0)
                 or (h in {"shin_l", "shin_r"} and c.z > 0.40), 0.010)
    gaiters = d(body, prefix + "gaiters", lambda c, n, h: (h in {"shin_l", "shin_r"} and c.z <= 0.44)
                or h in {"foot_l", "foot_r", "toe_l", "toe_r"}, 0.014)
    a1, b1 = Vector((0.13, 0, 1.44)), Vector((-0.17, 0, 0.97))
    a2, b2 = Vector((-0.13, 0, 1.44)), Vector((0.17, 0, 0.97))
    belts = d(body, prefix + "crossbelts", lambda c, n, h: h in TORSO and 0.95 < c.z < 1.46
              and (_band(c, a1, b1, 0.028) or _band(c, a2, b2, 0.028)), 0.036)
    for o, k in ((body, "skin"), (bones, "bone"), (waistcoat, "waistcoat"), (coat, "coat"),
                 (skirt, "coat"), (breeches, "breeches"), (gaiters, "gaiters"), (belts, "belts")):
        h_body.set_material(o, M[k])
    # Gear on sockets, placed in the standing pose.
    h_rig.pose(arm, STAND_M18)
    hat = h_body.tricorn(prefix + "tricorn")
    h_body.set_material(hat, M["tricorn"])
    gun = h_body.musket(prefix + "musket")
    gun.data.materials.append(M["wood"])
    gun.data.materials.append(M["metal"])
    box = h_body.box(prefix + "cartridge_box", (0.15, 0.05, 0.11))
    h_body.set_material(box, M["cartridge"])
    pack = h_body.box(prefix + "knapsack", (0.27, 0.09, 0.30))
    h_body.set_material(pack, M["knapsack"])

    def at(b):
        return (arm.matrix_world @ arm.pose.bones[b].matrix).translation

    h_body.attach(hat, arm, "socket.hat", Matrix.Translation(at("socket.hat") + Vector((0, 0, 0.02)))
                  @ Matrix.Rotation(math.radians(-6), 4, "X"))
    h_body.attach(gun, arm, "socket.hand_r",
                  Matrix.Translation(at("socket.hand_r") + Vector((-0.03, 0.0, 0.0)))
                  @ Matrix.Rotation(math.radians(-14), 4, "X") @ Matrix.Rotation(math.pi, 4, "Z"))
    h_body.attach(box, arm, "socket.hip_r", Matrix.Translation(at("socket.hip_r") + Vector((0.0, 0.07, 0.02)))
                  @ Matrix.Rotation(math.radians(30), 4, "Z"))
    h_body.attach(pack, arm, "socket.back", Matrix.Translation(at("socket.back") + Vector((0, 0.06, 0.0))))
    layers = {"bones": [bones], "flesh": [body],
              "clothes": [waistcoat, coat, skirt, breeches, gaiters, belts],
              "gear": [hat, gun, box, pack]}
    _finish(arm, layers, prefix, STAND_M18)
    gear = {"hat": hat, "musket": gun, "cartridge_box": box, "knapsack": pack}
    if state == "lie":
        set_state(arm, "DEATHLIE1")
        drop(hat, Matrix.Translation((0.30, -1.62, 0.05)) @ Matrix.Rotation(math.radians(-70), 4, "X")
             @ Matrix.Rotation(math.radians(40), 4, "Z"))
        drop(gun, Matrix.Translation((-0.42, -0.85, 0.035)) @ Matrix.Rotation(math.radians(80), 4, "Z")
             @ Matrix.Rotation(math.radians(90), 4, "Y"))
    else:
        set_state(arm, "STAND")
    return arm, layers, gear


def peasant(prefix="engkri."):
    M = mats(prefix)
    arm = h_rig.build(prefix + "H")
    body = h_body.body(arm, prefix + "body", build={"width": 0.97, "belly": 1.05})
    bones = h_body.bones_mesh(arm, prefix + "bones")
    d = h_body.derive
    tunic = d(body, prefix + "tunic", lambda c, n, h: (h in TORSO and 0.95 < c.z < 1.48) or h in ARMS, 0.018)
    skirt = h_body.skirt(arm, prefix + "tunic_skirt", 1.0, 0.70, (0.17, 0.13), (0.205, 0.17))
    sash = d(body, prefix + "sash", lambda c, n, h: h in TORSO and 1.00 < c.z < 1.10, 0.032)
    breeches = d(body, prefix + "breeches", lambda c, n, h: (h in {"hips"} | LEGS and c.z < 1.0)
                 or (h in {"shin_l", "shin_r"} and c.z > 0.11), 0.009)
    shoes = d(body, prefix + "shoes", lambda c, n, h: h in {"foot_l", "foot_r", "toe_l", "toe_r"}
              or (h in {"shin_l", "shin_r"} and c.z <= 0.11), 0.012)
    hair = d(body, prefix + "hair", lambda c, n, h: h in {"head", "neck"} and (
        (c.z > 1.63 and c.y > -0.075) or (c.y > -0.01 and c.z > 1.50)), 0.016)
    for o, k in ((body, "skin"), (bones, "bone"), (tunic, "tunic"), (skirt, "tunic"),
                 (sash, "sash"), (breeches, "kbreeches"), (shoes, "shoes"), (hair, "hair")):
        h_body.set_material(o, M[k])
    h_rig.pose(arm, STAND_KRI)
    hat = h_body.slouch_hat(prefix + "hat")
    h_body.set_material(hat, M["hat"])
    head = (arm.matrix_world @ arm.pose.bones["socket.hat"].matrix).translation
    h_body.attach(hat, arm, "socket.hat", Matrix.Translation(head + Vector((0, 0.005, 0.03)))
                  @ Matrix.Rotation(math.radians(-8), 4, "X"))
    layers = {"bones": [bones], "flesh": [body, hair],
              "clothes": [tunic, skirt, sash, breeches, shoes], "gear": [hat]}
    _finish(arm, layers, prefix, STAND_KRI)
    set_state(arm, "STAND")
    return arm, layers, {"hat": hat}


def add_actions(arm, stand):
    """Clips STAND, DEATH, DEATHLIE1 as separate actions (glTF animations of those names)."""
    arm.animation_data_create()
    names = []
    for name, keys in (("STAND", [(0, stand)]), ("DEATH", death_keys(stand)),
                       ("DEATHLIE1", [(0, lie_pose())])):
        act = bpy.data.actions.new(name)
        act.use_fake_user = True
        arm.animation_data.action = act
        for f, spec in keys:
            h_rig.pose(arm, spec, frame=f)
        names.append(act.name)
    arm["acr_clips"] = ["STAND", "DEATH", "DEATHLIE1"]
    arm["_acts"] = names
    arm.animation_data.action = None


def set_state(arm, clip, frame=0):
    i = list(arm["acr_clips"]).index(clip)
    arm.animation_data.action = bpy.data.actions[arm["_acts"][i]]
    bpy.context.scene.frame_set(frame)
    bpy.context.view_layer.update()


def drop(obj, world):
    """Detach gear from its socket and lay it on the ground (what the game does at death)."""
    obj.parent = None
    obj.matrix_world = world
    obj["acr_dropped"] = True


def set_decay(arm, layers, gear, decay):
    """Per-corpse decay: the shader parameter on every object, sinking on the transforms."""
    z = h_decay.sink_offset(decay)
    arm.location.z = z
    for o in [o for v in layers.values() for o in v] + list(gear.values()):
        o["decay"] = decay
        if o.get("acr_dropped"):
            if "z0" not in o:
                o["z0"] = o.location.z
            o.location.z = o["z0"] + z
    bpy.context.view_layer.update()


def place(arm, gear, x, y, real_dir):
    arm.location.x, arm.location.y = x, y
    arm.rotation_euler = (0, 0, cv.yaw(real_dir))
    for o in gear.values():
        if o.get("acr_dropped"):
            if "m0" not in o:
                o["m0"] = [list(r) for r in o.matrix_world]
            m0 = Matrix([list(r) for r in o["m0"]])
            o.matrix_world = Matrix.Translation((x, y, 0)) @ Matrix.Rotation(cv.yaw(real_dir), 4, "Z") @ m0
    bpy.context.view_layer.update()


# ----------------------------------------------------------------------------- scene/render

def reset():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    s = bpy.context.scene
    s.render.engine = "CYCLES"
    s.cycles.device = "CPU"
    s.cycles.use_denoising = False
    s.cycles.filter_width = 0.7
    s.cycles.max_bounces = 3
    s.cycles.transparent_max_bounces = 24
    s.render.film_transparent = True
    s.view_settings.view_transform = "Standard"
    s.render.image_settings.file_format = "PNG"
    s.render.image_settings.color_mode = "RGBA"
    w = bpy.data.worlds.new("w")
    if w.node_tree is None:
        w.use_nodes = True
    bg = w.node_tree.nodes["Background"]
    bg.inputs["Color"].default_value = (0.42, 0.42, 0.45, 1)
    bg.inputs["Strength"].default_value = 0.9
    s.world = w
    sun = bpy.data.lights.new("sun", "SUN")
    sun.energy = 3.2
    sun.angle = math.radians(8)
    o = bpy.data.objects.new("sun", sun)
    # Light from the upper left and the viewer's side, like the sprites.
    o.rotation_euler = (math.radians(40), 0, math.radians(-50))
    s.collection.objects.link(o)


def original_camera(w, h, anchor, scale=1):
    """The pipeline's original camera (docs/assets.md); scale > 1 renders the same view at a
    higher resolution (same ortho_scale and location)."""
    loc, rot, ortho = cv.camera_setup(w, h, anchor)
    cam = bpy.data.objects.get("orig")
    if cam is None:
        cam = bpy.data.objects.new("orig", bpy.data.cameras.new("orig"))
        bpy.context.scene.collection.objects.link(cam)
    cam.data.type = "ORTHO"
    cam.data.sensor_fit = "AUTO"
    cam.data.ortho_scale = ortho
    cam.data.clip_start = 1.0
    cam.data.clip_end = cv.CAMERA_DISTANCE * 2.0
    cam.location = loc
    cam.rotation_euler = rot
    s = bpy.context.scene
    s.camera = cam
    s.render.resolution_x = w * scale
    s.render.resolution_y = h * scale
    s.render.resolution_percentage = 100
    return cam


def render(path, samples):
    s = bpy.context.scene
    s.cycles.samples = samples
    s.render.filepath = path
    bpy.ops.render.render(write_still=True)


def ground(size=4.0, rgb=(0.36, 0.40, 0.20)):
    me = bpy.data.meshes.new("ground")
    h = size / 2
    me.from_pydata([(-h, -h, 0), (h, -h, 0), (h, h, 0), (-h, h, 0)], [], [(0, 1, 2, 3)])
    o = bpy.data.objects.new("ground", me)
    bpy.context.scene.collection.objects.link(o)
    m = bpy.data.materials.new("ground")
    p = m.node_tree.nodes["Principled BSDF"]
    p.inputs["Base Color"].default_value = h_decay.lin(rgb)
    p.inputs["Roughness"].default_value = 1.0
    me.materials.append(m)
    return o


def canvas(refs, typ, anim):
    if refs:
        p = os.path.join(refs, f"{typ}-{anim}", "ref.json")
        if os.path.exists(p):
            r = json.load(open(p))
            return r["width"], r["height"], tuple(r["anchor"])
    return DEFAULT_CANVAS[(typ, anim)]


def free_camera(name, loc, target, lens, res):
    cam = bpy.data.objects.new(name, bpy.data.cameras.new(name))
    bpy.context.scene.collection.objects.link(cam)
    cam.data.lens = lens
    cam.location = loc
    cam.rotation_euler = (Vector(target) - Vector(loc)).to_track_quat("-Z", "Y").to_euler()
    s = bpy.context.scene
    s.camera = cam
    s.render.resolution_x, s.render.resolution_y = res
    return cam


# ----------------------------------------------------------------------------- stats/export

def stats(layers, gear):
    out = {}
    for layer, objs in list(layers.items()) + [("gear", list(gear.values()))]:
        for o in objs:
            o.data.calc_loop_triangles()
            out[o.name] = {"layer": layer, "tris": len(o.data.loop_triangles),
                           "verts": len(o.data.vertices)}
    return out


def export_glb(path, arm, layers, gear):
    for o in bpy.data.objects:
        o.select_set(False)
    objs = [arm] + [o for v in layers.values() for o in v] + list(gear.values())
    for o in objs:
        o.select_set(True)
        if o.type == "MESH":
            for i, m in enumerate(o.data.materials):
                o.data.materials[i] = h_decay.gltf_material(m)
        for k in ("decay", "z0", "m0", "_acts"):
            if k in o:
                del o[k]
    arm["acr_asset"] = "engm18"
    arm["acr_type"] = "EngM18"
    arm["acr_pixels_per_metre"] = cv.PIXELS_PER_METRE
    arm["acr_decay_param"] = ("uniform decay 0..1 per corpse; per material extras acr_decay; "
                              "COLOR_0 = (mask, fray noise, layer code/8, 1)")
    bpy.context.view_layer.objects.active = arm
    bpy.ops.export_scene.gltf(
        filepath=path, export_format="GLB", use_selection=True, export_extras=True,
        export_yup=True, export_skins=True, export_animations=True,
        export_animation_mode="ACTIONS", export_force_sampling=True,
        export_vertex_color="ACTIVE", export_cameras=False, export_lights=False)


# ----------------------------------------------------------------------------- main

def main():
    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    opt = {"--out": os.path.join(HERE, "out"), "--refs": "", "--only": "stand,decay,free,turn,glb",
           "--samples": "24"}
    for i in range(0, len(argv) - 1, 2):
        opt[argv[i]] = argv[i + 1]
    out = os.path.abspath(opt["--out"])
    refs = opt["--refs"]
    only = set(opt["--only"].split(","))
    spp = int(opt["--samples"])
    os.makedirs(out, exist_ok=True)
    t0 = time.time()
    report = {}

    if "stand" in only:
        # Standing musketeer and peasant from the original camera, 1x (sprite size) and 4x.
        for typ, build in (("EngM18", musketeer), ("EngKri", peasant)):
            reset()
            arm, layers, gear = build()
            w, h, anchor = canvas(refs, typ, "STAND")
            for k in (0, 2, 4, 6, 8):
                arm.rotation_euler = (0, 0, cv.yaw(cv.stored_direction(k, 9)))
                for scale in (1, 4):
                    original_camera(w, h, anchor, scale)
                    render(os.path.join(out, f"{typ}-STAND-k{k}-x{scale}.png"), spp)
            report["stats_" + typ] = stats(layers, gear)
        # Lying musketeer next to #DEATHLIE1.
        reset()
        arm, layers, gear = musketeer(state="lie")
        w, h, anchor = canvas(refs, "EngM18", "DEATHLIE1")
        for k in (0, 2, 4, 6, 8):
            place(arm, gear, 0, 0, cv.stored_direction(k, 9))
            for scale in (1, 4):
                original_camera(w, h, anchor, scale)
                render(os.path.join(out, f"EngM18-DEATHLIE1-k{k}-x{scale}.png"), spp)

    if "decay" in only:
        reset()
        arm, layers, gear = musketeer(state="lie")
        g = ground()
        w, h, anchor = canvas(refs, "EngM18", "DEATHLIE1")
        place(arm, gear, 0, 0, cv.stored_direction(2, 9))
        stages = [0.0, 0.12, 0.28, 0.40, 0.52, 0.66, 0.82, 0.95]
        report["decay_stages"] = stages
        for i, dec in enumerate(stages):
            set_decay(arm, layers, gear, dec)
            for scale in (1, 3):
                original_camera(w, h, anchor, scale)
                render(os.path.join(out, f"decay-{i}-x{scale}.png"), spp)
        # Close-up of the bones stage without ground, from the free camera.
        g.hide_render = True
        set_decay(arm, layers, gear, 0.70)
        arm.location.z = 0
        free_camera("close", (1.6, -3.4, 1.9), (0, -0.8, 0.1), 50, (640, 400))
        render(os.path.join(out, "decay-close.png"), spp)

    if "free" in only:
        # Free rotation: two standing figures and two corpses at different decay.
        reset()
        ground(8.0)
        a1, l1, g1 = musketeer("f1.")
        a2, l2, g2 = peasant("f2.")
        a3, l3, g3 = musketeer("f3.", state="lie")
        a4, l4, g4 = musketeer("f4.", state="lie")
        a1.location = (-0.6, 0.4, 0)
        a1.rotation_euler = (0, 0, math.radians(-25))
        a2.location = (0.5, 0.9, 0)
        a2.rotation_euler = (0, 0, math.radians(20))
        place(a3, g3, -1.4, -0.4, 40)
        place(a4, g4, 1.5, -0.9, 110)
        set_decay(a3, l3, g3, 0.34)
        set_decay(a4, l4, g4, 0.62)
        bpy.context.scene.render.film_transparent = False
        free_camera("free", (4.2, -6.2, 3.0), (0, 0, 0.6), 50, (1280, 720))
        render(os.path.join(out, "free-view.png"), max(spp * 2, 32))

    if "turn" in only:
        # Turntable from a free camera (15 degrees up), 8 angles.
        reset()
        musketeer()
        for i in range(8):
            a = math.radians(45 * i)
            free_camera(f"turn{i}", (math.sin(a) * 7.5, -math.cos(a) * 7.5, 2.9), (0, 0, 1.0), 85, (240, 400))
            render(os.path.join(out, f"turn-{i}.png"), spp)

    if "glb" in only:
        reset()
        arm, layers, gear = musketeer()
        report["stats_glb"] = stats(layers, gear)
        p = os.path.join(out, "engm18.glb")
        export_glb(p, arm, layers, gear)
        report["glb_bytes"] = os.path.getsize(p)
        report["bones"] = len(arm.data.bones)
        report["deform_bones"] = sum(1 for b in arm.data.bones if b.use_deform)

    report["seconds_" + "_".join(sorted(only))] = round(time.time() - t0, 1)
    sp = os.path.join(out, "stats.json")
    old = json.load(open(sp)) if os.path.exists(sp) else {}
    old.update(report)
    json.dump(old, open(sp, "w"), indent=1)
    print("done in", round(time.time() - t0, 1), "s")


if __name__ == "__main__":
    main()
