"""Contact sheet of the clips by weapon (`acr_assets.clips`) and the lace and body-paint
masks (`acr_assets.masks`): a MakeHuman test figure posed at each clip's key frame (the
release, hit or a stroke) with its held items, then a line soldier with lace and bare
figures with the paint sets. Cycles, orthographic, one cell per pose, composed in numpy.

    blender -b --factory-startup --python assets/blender/tools/clips_sheet.py -- --out sheet.png [--part clips|masks]
"""

import math
import os
import sys

import bpy
import numpy as np
from mathutils import Euler, Matrix, Vector

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "lib"))

from acr_assets import armature as am  # noqa: E402
from acr_assets import clips, gear, line, masks, outfit, scene  # noqa: E402
from acr_assets.humans import LAYER_OF, Character  # noqa: E402

CELL = (360, 560)
SKIN = (0.60, 0.40, 0.28)
COLOURS = {"wood": (0.42, 0.24, 0.12), "metal": (0.62, 0.62, 0.66), "brass": (0.78, 0.62, 0.27),
           "facings": (0.62, 0.08, 0.07), "colour": (0.70, 0.12, 0.10), "cord": (0.80, 0.74, 0.60),
           "stone": (0.12, 0.12, 0.14), "feather": (0.85, 0.82, 0.76), "vellum": (0.86, 0.80, 0.66),
           "hide": (0.62, 0.48, 0.32), "skin": SKIN}


def material(key):
    m = bpy.data.materials.get("sheet." + key)
    if m:
        return m
    m = bpy.data.materials.new("sheet." + key)
    m.use_nodes = True
    rgb = COLOURS.get(key, (0.5, 0.5, 0.5))
    p = m.node_tree.nodes["Principled BSDF"]
    p.inputs["Base Color"].default_value = (*[c ** 2.2 for c in rgb], 1.0)
    p.inputs["Roughness"].default_value = 0.7
    return m


def paint(obj, key=None):
    keys = list(obj.get("acr_material_keys") or [key])
    obj.data.materials.clear()
    for k in keys:
        obj.data.materials.append(material(k))


def setup(width=1.0, height=2.1, yaw=-30):
    sc = bpy.context.scene
    cam = bpy.data.objects.get("cam")
    if cam is None:
        cam = bpy.data.objects.new("cam", bpy.data.cameras.new("cam"))
        sc.collection.objects.link(cam)
        sun = bpy.data.objects.new("sun", bpy.data.lights.new("sun", "SUN"))
        sun.data.energy = 3.5
        sun.rotation_euler = Euler((math.radians(40), math.radians(20), math.radians(-30)))
        sc.collection.objects.link(sun)
        world = bpy.data.worlds.new("w")
        world.use_nodes = True
        world.node_tree.nodes["Background"].inputs["Color"].default_value = (1, 1, 1, 1)
        world.node_tree.nodes["Background"].inputs["Strength"].default_value = 0.7
        sc.world = world
        bpy.ops.mesh.primitive_plane_add(size=40, location=(0, 0, 0))
        g = bpy.context.object
        g.name = "ground"
        g.data.materials.append(material("ground"))
        g.data.materials[0].node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = (1, 1, 1, 1)
    cam.data.type = "ORTHO"
    cam.data.ortho_scale = height * 1.05
    cam.rotation_euler = Euler((math.radians(82), 0, math.radians(yaw)))
    d = cam.rotation_euler.to_matrix() @ Vector((0, 0, 1))
    cam.location = Vector((0, -0.45, height / 2 - 0.02)) + d * 10
    sc.camera = cam
    sc.render.engine = "CYCLES"
    sc.cycles.samples = 32
    sc.cycles.device = "GPU" if os.environ.get("ACR_GPU", "1") == "1" else "CPU"
    sc.render.resolution_x, sc.render.resolution_y = CELL
    sc.render.film_transparent = False


def render(path, label):
    sc = bpy.context.scene
    sc.render.filepath = path
    bpy.ops.render.render(write_still=True)
    print("ACR sheet cell:", label, path)
    return path, label


def figure(name, build=None):
    root = scene.root(name)
    ch = Character(root, name, build or {"width": 1.0, "shoulders": 1.05, "limbs": 1.15, "hips": 1.0})
    ch.body()
    paint(ch.flesh, "skin")
    return root, ch


def held_items(ch):
    am.pose(ch.arm, {})
    items = {}

    def put(key, item, socket):
        items[key] = gear.hold(ch, item, f"{ch.name}.{key}", socket)
        paint(items[key])

    put("bow", "longbow", "socket.hand_l")
    put("arrow", "arrow", "socket.hand_r")
    put("tomahawk", "tomahawk", "socket.hand_r")
    put("spear", "spear", "socket.hand_r")
    put("sling", "sling", "socket.hand_r")
    put("blowpipe", "blowpipe", "socket.hand_r")
    put("stick_r", "drumstick", "socket.hand_r")
    put("stick_l", "drumstick", "socket.hand_l")
    put("colour", "colour", "socket.hand_r")
    put("sword", "smallsword", "socket.hand_r")
    put("club", "war_club", "socket.hand_r")
    put("shield", "shield", "socket.hand_l")
    # The drum on the hips: its head's centre at `clips.DRUM_HEAD`, its axis `DRUM_ROT`.
    from acr_assets import held

    it = held.build("drum")
    drum = gear.held(it, f"{ch.name}.drum")
    paint(drum)
    r = am.euler(clips.DRUM_ROT)
    head = Vector(it.sockets["socket.head"])
    m = Matrix.Translation(Vector(clips.DRUM_HEAD) - r @ head) @ r.to_4x4()
    am.attach(drum, ch.arm, "hips", m)
    items["drum"] = drum
    return items


SHOW = {
    "bow": ("bow", "arrow"), "throw": ("tomahawk",), "spear_throw": ("spear",), "sling": ("sling",),
    "blowpipe": ("blowpipe",), "heal": (), "drum": ("drum", "stick_r", "stick_l"), "colour": ("colour",),
    "sword": ("sword",), "club": ("club",), "spear": ("spear",), "shield": ("club", "shield"),
}


def pose_at(ch, items, pose, show):
    spec = clips.solve(ch.arm, pose)
    am.pose(ch.arm, spec)
    for pb in ch.arm.pose.bones:
        pb.scale = (1, 1, 1)
    for sock, v in pose.get("show", {}).items():
        ch.arm.pose.bones[sock].scale = (max(float(v), 1e-4),) * 3
    for k, o in items.items():
        o.hide_render = k not in show
        for c in o.children_recursive:
            c.hide_render = o.hide_render
    bpy.context.view_layer.update()


def clip_cells(out_dir):
    root, ch = figure("T")
    items = held_items(ch)
    cells = []
    bow = clips.bow(79, 44)
    throw = clips.throw(50, 16)
    spear_throw = clips.throw(55, 28, "spear")
    sling = clips.sling(68, 56)
    pipe = clips.blowpipe(61, 18)
    heal = clips.heal(200, 1)
    roll = clips.drum_roll(70)
    march_l, _ = clips.drum_march(10, 10)
    wave = clips.colour_wave(120)
    carry_l, _ = clips.colour_carry(10, 10)
    sword = clips.sword_strikes(52)
    club = clips.club_strikes(56)
    spear = clips.spear_strikes(69)
    jobs = [
        ("bow ATTACK1 f0 nocked", "bow", dict(bow)[0]), ("bow ATTACK1 f43 full draw", "bow", dict(bow)[43]),
        ("bow ATTACK1 f44 loose", "bow", dict(bow)[44]),
        ("tomahawk ATTACK1 f15 wind-up", "throw", dict(throw)[15]), ("tomahawk ATTACK1 f16 release", "throw", dict(throw)[16]),
        ("spear ATTACK1 f27 wind-up", "spear_throw", dict(spear_throw)[27]),
        ("sling ATTACK1 f40 whirl", "sling", next(p for f, p in sling if f >= 40)),
        ("sling ATTACK1 f56 cast", "sling", dict(sling)[56]),
        ("blowpipe ATTACK1 f18 blow", "blowpipe", dict(pipe)[18]), ("heal ATTACK f2", "heal", dict(heal)[2]),
        ("heal ATTACK f22 raised", "heal", dict(heal)[22]),
        ("drum REST f0", "drum", dict(roll)[0]), ("drum REST f2", "drum", dict(roll)[2]),
        ("drum MOTION_L f0", "drum", dict(march_l)[0]),
        ("colour STAND", "colour", clips.STAND_COLOUR), ("colour REST f7 sweep", "colour", next(p for f, p in wave if f >= 7)),
        ("colour MOTION_L f0", "colour", dict(carry_l)[0]),
    ]
    for name, clip in sword.items():
        jobs.append((f"sword {name} f2", "sword", dict(clip)[2]))
    for name, clip in club.items():
        if name != "ATTACK_DOWN":
            jobs.append((f"club {name} f2", "club", dict(clip)[2]))
    for name, clip in spear.items():
        if name != "ATTACK_DOWN":
            jobs.append((f"spear {name} f2", "spear", dict(clip)[2]))
    jobs.append(("shield block", "shield", clips.with_shield(dict(club["ATTACK_CUT_MID"])[0])))
    setup(height=2.3, yaw=-35)
    for i, (label, kind, pose) in enumerate(jobs):
        pose_at(ch, items, pose, SHOW[kind])
        cells.append(render(os.path.join(out_dir, f"clip_{i:02d}.png"), label))
    return cells


def preview(objs, img):
    """Plain Principled materials on the outfit atlas (the decay materials render for the
    game, not for Cycles); other meshes a flat colour."""
    m = bpy.data.materials.new(f"preview.{img.name}")
    m.use_nodes = True
    nt = m.node_tree
    tex = nt.nodes.new("ShaderNodeTexImage")
    tex.image = img
    uv = nt.nodes.new("ShaderNodeUVMap")
    uv.uv_map = "atlas"
    nt.links.new(uv.outputs["UV"], tex.inputs["Vector"])
    bsdf = nt.nodes["Principled BSDF"]
    nt.links.new(tex.outputs["Color"], bsdf.inputs["Base Color"])
    bsdf.inputs["Roughness"].default_value = 0.8
    for o in objs:
        if o.type != "MESH":
            continue
        if o.data.materials and o.data.materials[0] and o.data.materials[0].get("acr_colour_from") == "outfit":
            o.data.materials.clear()
            o.data.materials.append(m)
        else:
            keys = list(o.get("acr_material_keys") or ["metal"])
            o.data.materials.clear()
            for k in keys:
                o.data.materials.append(material(k))


def hide_all_but(objs):
    keep = set(objs) | {bpy.data.objects[n] for n in ("cam", "sun", "ground") if n in bpy.data.objects}
    for o in bpy.data.objects:
        o.hide_render = o not in keep


def mask_cells(out_dir):
    cells = []
    # A line soldier with lace (EngM18's spec, lace on): white worsted lace on a red coat.
    s = line.spec({}, lace={"hat": 0.012, "buttonholes": True}, palette={"lace": (0.92, 0.90, 0.84)})
    root = scene.root("L")
    ch = line.make(root, "L", s)
    am.pose(ch.arm, {k: v for k, v in line.poses.STAND_MUSKET.items()})
    colours = ch.colours(None, s["palette"], fixed=s["fixed"])
    img = outfit.dress(ch, colours, LAYER_OF, size=1024)
    objs = ch.all_meshes() + [o for o in ch.held.values()]
    for o in ch.layers["bones"] + ([ch.caps_obj] if getattr(ch, "caps_obj", None) else []):
        objs.remove(o)
    preview(objs, img)
    hide_all_but(objs)
    setup(height=1.95, yaw=-20)
    cells.append(render(os.path.join(out_dir, "lace_front.png"), "EngM18 + lace (front)"))
    setup(height=0.5, yaw=-20)
    cam = bpy.data.objects["cam"]
    cam.location.z += 1.45 - 0.5 / 2 + 0.02
    cells.append(render(os.path.join(out_dir, "lace_zoom.png"), "lace: hat, lapels"))
    # Bare figures with the paint sets.
    for i, name in enumerate(("red_black", "black_stripes", "red_hands", "white_dots")):
        root, ch = figure(f"P{i}")
        ch.paint_masks = masks.body_paint(name)
        am.pose(ch.arm, clips.bare(clips.STAND_FREE))
        colours = ch.colours(None, {"skin": SKIN})
        img = outfit.dress(ch, colours, LAYER_OF, size=1024)
        objs = [o for o in ch.all_meshes() if o not in ch.layers["bones"] and o is not getattr(ch, "caps_obj", None)]
        preview(objs, img)
        hide_all_but(objs)
        setup(height=1.95, yaw=-15)
        cells.append(render(os.path.join(out_dir, f"paint_{name}.png"), f"paint {name}"))
        setup(height=0.75, yaw=-15)
        cam = bpy.data.objects["cam"]
        cam.location.z += 1.30 - 0.75 / 2
        cells.append(render(os.path.join(out_dir, f"paint_{name}_zoom.png"), f"paint {name} (zoom)"))
    return cells


def compose(cells, out, cols=6):
    w, h = CELL
    rows = -(-len(cells) // cols)
    sheet = np.ones((rows * (h + 40), cols * w, 4), np.float32)
    for i, (path, label) in enumerate(cells):
        img = bpy.data.images.load(path)
        px = np.array(img.pixels[:], np.float32).reshape(img.size[1], img.size[0], 4)
        r, c = i // cols, i % cols
        y0 = (rows - 1 - r) * (h + 40) + 40
        sheet[y0:y0 + px.shape[0], c * w:c * w + px.shape[1]] = px
        bpy.data.images.remove(img)
    img = bpy.data.images.new("sheet", cols * w, rows * (h + 40), alpha=True)
    img.pixels.foreach_set(sheet.ravel())
    img.filepath_raw = out
    img.file_format = "PNG"
    img.save()
    with open(os.path.splitext(out)[0] + ".txt", "w", encoding="utf-8") as f:
        for i, (_p, label) in enumerate(cells):
            f.write(f"{i // cols},{i % cols}: {label}\n")
    print("ACR sheet:", out)


def main():
    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    out = argv[argv.index("--out") + 1] if "--out" in argv else "clips_sheet.png"
    part = argv[argv.index("--part") + 1] if "--part" in argv else "clips"
    out_dir = os.path.splitext(out)[0] + "_cells"
    os.makedirs(out_dir, exist_ok=True)
    scene.reset()
    cells = clip_cells(out_dir) if part == "clips" else mask_cells(out_dir)
    compose(cells, out, cols=6 if part == "clips" else 5)


main()
