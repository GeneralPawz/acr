"""Cycles proof renders of a built model in exact clip poses (`acr_assets.posecheck`).

    blender -b --factory-startup --python assets/blender/tools/render_pose.py -- \
        <model.glb | model.blend> <out dir> <job>... [--poses <x.poses.json>] [--legacy]

A job is `NAME=CLIP:FRAME[:VIEW[:FOCUS[:SIZE]]]`: clip frame 0 = the clip's first frame;
VIEW `front`, `side`, `3q`, `back` or `AZ,EL` degrees; FOCUS a bone name (default the whole
figure); SIZE the orthographic frame height in metres. Example:
`fire=FIRE_AIMED:6:3q`, `grip=FIRE_AIMED:6:-60,10:hand_r:0.5`.

Every job resets every bone and node to the export pose, applies only its clip and, when the
build's `<name>.poses.json` lies next to the model (or `--poses`), checks the pose against it
and fails on a mismatch. A glb is imported through `gltf_pack.REST_CLIP`; its authored LOD
levels (`extras.acr_lod`) stay hidden. `--legacy` poses the way the old tools did (plain
import, the action on slot 0, no reset), to show the difference.
"""

import math
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "lib"))

import bpy  # noqa: E402
from mathutils import Matrix, Vector  # noqa: E402

from acr_assets import posecheck  # noqa: E402

VIEWS = {"front": (0, 5), "3q": (-40, 10), "side": (-90, 5), "back": (180, 5)}


def studio(sc):
    sc.render.engine = "CYCLES"
    try:
        prefs = bpy.context.preferences.addons["cycles"].preferences
        prefs.compute_device_type = "HIP"
        prefs.get_devices()
        for d in prefs.devices:
            d.use = True
        sc.cycles.device = "GPU"
    except Exception:
        sc.cycles.device = "CPU"
    sc.cycles.samples = 48
    sc.cycles.use_denoising = True
    sc.view_settings.view_transform = "AgX"
    sc.render.resolution_x, sc.render.resolution_y = 700, 900
    w = bpy.data.worlds.new("w")
    sc.world = w
    w.use_nodes = True
    w.node_tree.nodes["Background"].inputs[0].default_value = (0.92, 0.92, 0.92, 1)
    w.node_tree.nodes["Background"].inputs[1].default_value = 0.7
    for rot, e in (((50, 0, 30), 3.0), ((60, 0, 200), 1.2)):
        s = bpy.data.objects.new("sun", bpy.data.lights.new("sun", "SUN"))
        s.data.energy = e
        s.data.angle = math.radians(8)
        s.rotation_euler = [math.radians(a) for a in rot]
        sc.collection.objects.link(s)
    cam = bpy.data.objects.new("cam", bpy.data.cameras.new("cam"))
    sc.collection.objects.link(cam)
    sc.camera = cam
    cam.data.type = "ORTHO"
    cam.data.clip_end = 30
    return cam


def aim(cam, target, view, size):
    az, el = VIEWS[view] if view in VIEWS else (float(x) for x in view.split(","))
    a, e = math.radians(az), math.radians(el)
    d = Vector((math.sin(a) * math.cos(e), -math.cos(a) * math.cos(e), math.sin(e)))
    cam.location = Vector(target) + d * 8.0
    x = Vector((0, 0, 1)).cross(d).normalized()
    cam.rotation_euler = Matrix((x, d.cross(x), d)).transposed().to_euler()
    cam.data.ortho_scale = size


def legacy_play(sc, name, frame):
    """The old tools' way: the action on slot 0 of the armature only, nothing reset."""
    arm = next(o for o in sc.objects if o.type == "ARMATURE")
    act = bpy.data.actions[name]
    ad = arm.animation_data or arm.animation_data_create()
    ad.action = act
    if hasattr(ad, "action_slot") and act.slots:
        ad.action_slot = act.slots[0]
    sc.frame_set(int(act.frame_range[0]) + frame)


def main(argv):
    legacy = "--legacy" in argv
    argv = [a for a in argv if a != "--legacy"]
    poses = None
    if "--poses" in argv:
        i = argv.index("--poses")
        poses = argv[i + 1]
        del argv[i:i + 2]
    model, out, jobs = os.path.abspath(argv[0]), os.path.abspath(argv[1]), argv[2:]
    os.makedirs(out, exist_ok=True)
    poses = poses or posecheck.poses_path(model)
    if model.endswith(".blend"):
        bpy.ops.wm.open_mainfile(filepath=model)
    else:
        bpy.ops.wm.read_factory_settings(use_empty=True)
        if legacy:
            bpy.ops.import_scene.gltf(filepath=model)
            posecheck.game_colours()  # only the pose is the old way, not the colours
        else:
            posecheck.import_glb(model)
    sc = bpy.context.scene
    for o in sc.objects:
        if o.type == "MESH" and ("acr_lod" in o.keys() or ".lod" in o.name.lower()):
            o.hide_render = True
    poser = None if legacy else posecheck.Poser(sc)
    cam = studio(sc)
    arm = next(o for o in sc.objects if o.type == "ARMATURE")
    for job in jobs:
        name, spec = job.split("=", 1)
        parts = spec.split(":")
        clip, frame = parts[0], int(parts[1]) if len(parts) > 1 else 0
        view = parts[2] if len(parts) > 2 else "3q"
        focus = parts[3] if len(parts) > 3 and parts[3] else None
        size = float(parts[4]) if len(parts) > 4 else (0.6 if focus else 2.3)
        if legacy:
            legacy_play(sc, clip, frame)
            if poses and str(frame) in posecheck.load(poses)["clips"].get(clip, {}):
                try:  # glb poses.json bone names match; reports how far off the old way is
                    posecheck.verify(poses, clip, frame, sc)
                except posecheck.PoseMismatch as e:
                    print("ACR legacy pose mismatch:", e)
        else:
            check = poses if poses and str(frame) in posecheck.load(poses)["clips"].get(clip, {}) else None
            if check is None:
                print(f"ACR warning: {clip} frame {frame} not checked (no dump of that frame)")
            poser.play(clip, frame, check=check)
        target = arm.matrix_world @ arm.pose.bones[focus].head if focus else (0.0, 0.0, 1.0)
        aim(cam, target, view, size)
        sc.render.filepath = os.path.join(out, name + ".png")
        bpy.ops.render.render(write_still=True)
        print("ACR rendered", name, clip, frame, "legacy" if legacy else "exact")


if __name__ == "__main__":
    try:
        main(sys.argv[sys.argv.index("--") + 1:])
    except Exception:
        import traceback

        traceback.print_exc()
        sys.exit(1)
