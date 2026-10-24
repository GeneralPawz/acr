"""Exact clip poses for proof renders, checked against the build (needs `bpy`).

Renders made by importing a built glb, or by opening its `--blend`, used to show other poses
than the game: Blender keeps a bone that the shown clip does not key in whatever pose it had
last (another clip, or a tool's own finger curl); the packer drops a clip's channels that equal
the node transform (`gltf_pack`); a tool that switched bones to quaternion rotation silenced
every Euler-keyed clip; slot 0 of an imported action can belong to another node. The game
plays a clip over the node transforms (the exported STAND pose). This module does the same:

- `dump(path)` (at build time, after the export, from `build.py`): for every armature with
  clips, the pose of every bone at the first, middle and last frame of every clip, played over
  the export pose, into `<name>.poses.json`;
- `import_glb(path)`: imports a glb through `gltf_pack.REST_CLIP`;
- `Poser(scene)` with `play(clip, frame)`: puts every bone and node back to its export pose,
  then applies only `clip` at clip frame `frame` (0 = the clip's first frame, at the build's
  fps), for an imported glb or an opened `--blend` alike;
- `verify(poses, clip, frame)`: compares the shown pose with the dump and raises
  `PoseMismatch` beyond `TOL_M` / `TOL_DEG` (`play(..., check=poses)` does it for you).

A bone's pose is compared as the world deformation it applies (pose x inverse rest, the
matrix skinning uses), so it does not depend on the bone axes the importer picks: its
skinned head position, rotation and scale.
"""

import json
import math
import os

import bpy
from mathutils import Matrix

from . import armature as am
from . import gltf_pack

TOL_M = 0.005
TOL_DEG = 2.0
FORMAT = "acr-poses/1"
# Dumped clip frames: every DUMP_STEP-th, the middle and the last.
DUMP_STEP = 4


class PoseMismatch(RuntimeError):
    pass


# ----------------------------------------------------------------------------- measuring


def _deform(arm, pb):
    w = arm.matrix_world
    return w @ pb.matrix @ pb.bone.matrix_local.inverted() @ w.inverted()


def bone_pose(arm, pb):
    """[head x, y, z (world m), quaternion w, x, y, z, scale] of a pose bone's deformation."""
    d = _deform(arm, pb)
    head = arm.matrix_world @ pb.head
    s = abs(d.to_3x3().determinant()) ** (1.0 / 3.0)
    q = d.to_3x3().normalized().to_quaternion() if s > 1e-6 else Matrix.Identity(3).to_quaternion()
    return [round(x, 5) for x in (*head, *q, s)]


def compare(a, b):
    """(head distance m, rotation angle deg, scale difference) of two `bone_pose` lists."""
    dist = math.dist(a[:3], b[:3])
    dot = abs(sum(x * y for x, y in zip(a[3:7], b[3:7])))
    ang = math.degrees(2 * math.acos(min(1.0, dot)))
    ds = abs(a[7] - b[7])
    if min(a[7], b[7]) < 1e-3:  # a hidden socket: only its scale matters
        dist = ang = 0.0
    return dist, ang, ds


def snapshot(arm):
    return {pb.name: bone_pose(arm, pb) for pb in arm.pose.bones}


# ----------------------------------------------------------------------------- posing


def _clip_armatures(scene):
    return sorted((o for o in scene.objects if o.type == "ARMATURE"), key=lambda o: o.name)


def _slot_for(obj, act):
    slots = [sl for sl in getattr(act, "slots", []) if sl.target_id_type == "OBJECT"]
    named = [sl for sl in slots if sl.name_display == obj.name]
    if named:
        return named[0]
    named = [sl for sl in slots if obj.name in sl.name_display]
    return named[0] if named else (slots[0] if len(slots) == 1 else None)


def _keyed_objects(act):
    """Names of the objects an action has a slot for (an imported glTF animation)."""
    return {sl.name_display for sl in getattr(act, "slots", []) if sl.target_id_type == "OBJECT"}


class Poser:
    """Plays clips exactly as the game does: export pose first, then only the asked clip.

    Built right after opening a `--blend` or `import_glb`, it records the pose every bone and
    animated node has then (the export pose; for an import it plays `REST_CLIP` to get it)."""

    def __init__(self, scene=None):
        self.scene = scene or bpy.context.scene
        self.objects = sorted((o for o in self.scene.objects if o.animation_data or o.type == "ARMATURE"),
                              key=lambda o: o.name)
        rest = bpy.data.actions.get(gltf_pack.REST_CLIP)
        self.imported = rest is not None
        if rest is not None:
            for o in self.objects:
                o.animation_data_create()
                self._assign(o, rest if o.name in _keyed_objects(rest) else None)
            self.scene.frame_set(0)
        # Only reads the scene otherwise: `build.py` makes one before the export.
        self.base = {}
        for o in self.objects:
            pose = {}
            if o.type == "ARMATURE":
                for pb in o.pose.bones:
                    pose[pb.name] = (pb.rotation_mode, pb.location.copy(), pb.rotation_quaternion.copy(),
                                     pb.rotation_euler.copy(), pb.scale.copy())
            self.base[o.name] = (o.matrix_basis.copy(), pose)

    def _assign(self, o, act):
        ad = o.animation_data
        ad.action = act
        if act is not None and hasattr(ad, "action_slot"):
            sl = _slot_for(o, act)
            if sl is not None:
                ad.action_slot = sl

    def reset(self):
        """Every bone and animated node back to the export pose, no action, NLA muted."""
        for o in self.objects:
            ad = o.animation_data_create()
            for tr in ad.nla_tracks:
                tr.mute = True
            self._assign(o, None)
            mb, pose = self.base[o.name]
            o.matrix_basis = mb
            for name, (mode, loc, q, e, s) in pose.items():
                pb = o.pose.bones[name]
                pb.rotation_mode = mode
                pb.location, pb.rotation_quaternion, pb.rotation_euler, pb.scale = loc, q, e, s
        self.scene.frame_set(0)

    def clip_range(self, name):
        act = bpy.data.actions[name]
        return int(round(act.frame_range[0])), int(round(act.frame_range[1]))

    def play(self, name, frame=0, check=None):
        """Shows clip `name` at its frame `frame` (0 = first; at the build's fps, which is the
        scene fps of both the build and Blender's import). `check`: a poses dump (dict or path)
        to verify against; raises `PoseMismatch`."""
        self.reset()
        act = bpy.data.actions.get(name)
        if act is None:
            raise KeyError(f"no clip {name!r}; have {sorted(a.name for a in bpy.data.actions)}")
        keyed = _keyed_objects(act)
        for o in self.objects:
            if o.name in keyed if self.imported else o.type == "ARMATURE":
                self._assign(o, act)
        f0, _f1 = self.clip_range(name)
        self.scene.frame_set(f0 + int(frame))
        bpy.context.view_layer.update()
        if check is not None:
            verify(check, name, int(frame), self.scene)
        return f0 + int(frame)


# ----------------------------------------------------------------------------- dump / verify


def _frames(act):
    f0, f1 = (int(round(x)) for x in act.frame_range)
    return sorted({*range(0, f1 - f0, DUMP_STEP), (f1 - f0) // 2, f1 - f0}), f0


def dump(path, poser):
    """Writes the build's clip poses next to the glb (`<name>.poses.json`); returns its path or
    None without clips. `poser`: made before the export (it holds the export pose); run after
    the export, since it changes the shown pose."""
    scene = poser.scene
    arms = _clip_armatures(scene)
    if not arms or not bpy.data.actions:
        return None
    out = {"format": FORMAT, "fps": scene.render.fps, "clips": {}}
    for act in sorted(bpy.data.actions, key=lambda a: a.name):
        if not am._fcurves(act) or not any(fc.data_path.startswith("pose.bones") for fc in am._fcurves(act)):
            continue
        frames, _f0 = _frames(act)
        clip = {}
        for k in frames:
            poser.play(act.name, k)
            clip[str(k)] = {a.name: snapshot(a) for a in arms}
        out["clips"][act.name] = clip
    poser.reset()
    p = os.path.splitext(path)[0] + ".poses.json"
    with open(p, "w", encoding="utf-8") as f:
        json.dump(out, f, separators=(",", ":"))
    return p


def load(poses):
    if isinstance(poses, dict):
        return poses
    with open(poses, encoding="utf-8") as f:
        return json.load(f)


def _armature_named(scene, name):
    """The armature for a dumped one: same name (glTF keeps object names), else the only one."""
    arms = _clip_armatures(scene)
    for a in arms:
        if a.name == name:
            return a
    return arms[0] if len(arms) == 1 else None


def verify(poses, clip, frame, scene=None, tol_m=TOL_M, tol_deg=TOL_DEG):
    """Worst (m, deg) of the shown pose against the dump at `clip`/`frame` (a dumped frame:
    every `DUMP_STEP`-th, the middle or the last). Raises `PoseMismatch` beyond the tolerances."""
    scene = scene or bpy.context.scene
    d = load(poses)["clips"].get(clip, {})
    want = d.get(str(frame))
    if want is None:
        raise KeyError(f"{clip} frame {frame} not dumped (have {sorted(d, key=int)})")
    worst = (0.0, 0.0, "")
    for aname, bones in want.items():
        arm = _armature_named(scene, aname)
        if arm is None:
            raise PoseMismatch(f"no armature {aname} in the scene")
        for bname, ref in bones.items():
            pb = arm.pose.bones.get(bname)
            if pb is None:
                continue
            dist, ang, ds = compare(bone_pose(arm, pb), ref)
            if ds > 0.05:
                dist = max(dist, 1.0)
            if dist / tol_m + ang / tol_deg > worst[0] / tol_m + worst[1] / tol_deg:
                worst = (dist, ang, bname)
    print(f"ACR pose check {clip} frame {frame}: worst {worst[2]} {worst[0] * 1000:.1f} mm {worst[1]:.2f} deg")
    if worst[0] > tol_m or worst[1] > tol_deg:
        raise PoseMismatch(f"{clip} frame {frame}: {worst[2]} off by {worst[0] * 1000:.1f} mm, {worst[1]:.1f} deg")
    return worst


def import_glb(path):
    """Imports an ACR glb with the extra clip `gltf_pack.REST_CLIP` (the export pose of every
    animated node), so `Poser` can restore it."""
    import tempfile

    with open(path, "rb") as f:
        data = gltf_pack.with_rest_clip(f.read())
    with tempfile.TemporaryDirectory() as tmp:
        p = os.path.join(tmp, os.path.basename(path))
        with open(p, "wb") as f:
            f.write(data)
        bpy.ops.import_scene.gltf(filepath=p)
    game_colours()


def game_colours():
    """Shows imported materials the way the game does: Blender's glTF importer always
    multiplies `COLOR_0` into the base colour, but on a material with `extras.acr_decay`
    `COLOR_0` is the decay mask (noise in R and G, B = 0), which the game never uses as a
    colour (`model.wgsl` `shade`, `flags.y`); multiplied in, it turned bare skin green.
    Cuts those links (the socket falls back to white); other materials keep their vertex
    colour. Returns the number of links cut."""
    n = 0
    for mat in bpy.data.materials:
        if "acr_decay" not in mat.keys() or not mat.node_tree:
            continue
        tree = mat.node_tree
        for node in tree.nodes:
            if node.bl_idname != "ShaderNodeVertexColor":
                continue
            for link in list(node.outputs[0].links):
                sock = link.to_socket
                tree.links.remove(link)
                if hasattr(sock, "default_value") and len(sock.default_value) == 4:
                    sock.default_value = (1.0, 1.0, 1.0, 1.0)
                n += 1
    print(f"ACR game colours: {n} decay-mask colour links cut")
    return n


def poses_path(glb_or_blend):
    p = os.path.splitext(glb_or_blend)[0] + ".poses.json"
    return p if os.path.isfile(p) else None
