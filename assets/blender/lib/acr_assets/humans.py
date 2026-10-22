"""Human characters on skeleton H (needs `bpy`): the body graph, a `Character` that collects
the layers, and the finishing steps every unit shares (cuts, caps, masks, morph targets,
materials from the sprites, export preparation). Unit scripts (`assets/units/*.py`) describe
clothes, gear, poses and clips. See docs/assets-humans.md.
"""

import math

import bpy
from mathutils import Matrix, Vector

from . import armature as am
from . import body as bd
from . import decay, detail, project, rig

# Layer order (#178) and the decay layer of every material key a unit may use.
LAYER_OF = {
    "skin": "flesh", "hair": "hair", "bone": "bone",
    "coat": "wool", "tunic": "wool", "sash": "wool", "cloak": "wool",
    "waistcoat": "linen", "breeches": "linen", "shirt": "linen", "stockings": "linen",
    "gaiters": "leather", "belts": "leather", "hat": "leather", "boots": "leather", "shoes": "leather",
    "cartridge": "leather", "knapsack": "leather", "saddle": "leather", "tack": "leather",
    "wood": "wood", "metal": "metal", "brass": "metal",
}


def graph(build=None):
    """Skin-modifier graph around skeleton H: [(node, parent, position, (rx, ry))]. `build`
    scales it: width (torso), shoulders, hips, limbs (limb radii), belly, head."""
    b = build or {}
    w = b.get("width", 1.0)
    sh = b.get("shoulders", 1.0)
    hp = b.get("hips", 1.0)
    lr = b.get("limbs", 1.0)
    hd = b.get("head", 1.0)
    o = Vector(b.get("origin", (0, 0, 0)))
    g = [
        ("pelvis", None, (0, 0.0, 0.93), (0.155 * w * hp, 0.11)),
        ("waist", "pelvis", (0, 0.0, 1.08), (0.135 * w, 0.095 * b.get("belly", 1.0))),
        ("chest", "waist", (0, -0.005, 1.27), (0.16 * w, 0.105)),
        ("upper", "chest", (0, 0.0, 1.40), (0.165 * w * sh, 0.095)),
        ("neck0", "upper", (0, -0.005, 1.47), (0.052, 0.055)),
        ("neck1", "neck0", (0, -0.01, 1.555), (0.05, 0.05)),
        ("jaw", "neck1", (0, -0.03, 1.60), (0.062 * hd, 0.085 * hd)),
        ("skull", "jaw", (0, -0.015, 1.665), (0.08 * hd, 0.098 * hd)),
        ("crown", "skull", (0, -0.01, 1.725), (0.05 * hd, 0.06 * hd)),
    ]
    for side, sfx in ((1, "_l"), (-1, "_r")):
        shp, elbow, wrist, hand_end, d = (Vector(x) for x in rig.arm_points(side))
        g += [
            ("shoulder" + sfx, "upper", tuple(shp + Vector((0.01 * side, 0, -0.01))), (0.058 * lr, 0.06 * lr)),
            ("elbow" + sfx, "shoulder" + sfx, tuple(elbow), (0.042 * lr, 0.042 * lr)),
            ("wrist" + sfx, "elbow" + sfx, tuple(wrist), (0.03 * lr, 0.026 * lr)),
            ("hand" + sfx, "wrist" + sfx, tuple(wrist + d * 0.075), (0.022, 0.045)),
            ("finger" + sfx, "hand" + sfx, tuple(hand_end + d * 0.05), (0.014, 0.036)),
            ("hip" + sfx, "pelvis", (0.095 * side, 0.0, 0.86), (0.085 * lr, 0.09 * lr)),
            ("knee" + sfx, "hip" + sfx, (0.0945 * side, 0.0, 0.50), (0.056 * lr, 0.058 * lr)),
            ("calf" + sfx, "knee" + sfx, (0.0945 * side, 0.015, 0.36), (0.055 * lr, 0.058 * lr)),
            ("ankle" + sfx, "calf" + sfx, (0.0945 * side, 0.025, 0.10), (0.035, 0.035)),
            ("heel" + sfx, "ankle" + sfx, (0.0945 * side, 0.02, 0.04), (0.04, 0.045)),
            ("toes" + sfx, "heel" + sfx, (0.099 * side, -0.16, 0.035), (0.042, 0.03)),
        ]
    return [(n, p, tuple(o + Vector(pos)), r) for n, p, pos, r in g]


def joint_radii(build=None):
    """Stump cap radius per cut joint (`rig.JOINTS`), from the graph."""
    b = build or {}
    lr = b.get("limbs", 1.0)
    return {"neck": 0.05, "shoulder_l": 0.055 * lr, "shoulder_r": 0.055 * lr, "elbow_l": 0.04 * lr,
            "elbow_r": 0.04 * lr, "hip_l": 0.08 * lr, "hip_r": 0.08 * lr, "knee_l": 0.055 * lr, "knee_r": 0.055 * lr}


class Character:
    """A human on skeleton H under `root`: layers bones/flesh/clothes/gear (mesh objects),
    held gear on sockets (drops at death), material keys per object."""

    def __init__(self, root, name, build=None, prefix="", arm_parent=None, table=None):
        self.root = root
        self.name = name
        self.build = build or {}
        self.prefix = prefix
        self.arm = am.build(f"{name}.rig", table or rig.skeleton_h(prefix, self.build.get("origin", (0, 0, 0))),
                            parent=arm_parent or root)
        self.layers = {"bones": [], "flesh": [], "clothes": [], "gear": []}
        self.held = {}  # name -> object on a socket bone (dropped at death)
        self.keys = {}  # object name -> material key (LAYER_OF)
        self.symmetric = []  # objects that may take colours from mirrored views
        self.flesh = None

    # -- layers ---------------------------------------------------------------------------

    def add(self, layer, obj, key, symmetric=False):
        self.layers[layer].append(obj)
        self.keys[obj.name] = key
        if symmetric:
            self.symmetric.append(obj)
        return obj

    def body(self, key="skin", bones=rig.BODY_BONES):
        P = self.prefix
        self.flesh = bd.hull(self.arm, f"{self.name}.flesh", graph(self.build), bones=[P + b for b in bones])
        bd.split_parts(self.flesh, lambda b: rig.part_of_bone(b[len(P):]))
        self.add("flesh", self.flesh, key, symmetric=True)
        return self.flesh

    def shell(self, name, key, keep_face, offset, symmetric=True):
        P = self.prefix
        obj = bd.derive(self.flesh, f"{self.name}.{name}", lambda c, n, h, p: keep_face(c, n, h[len(P):], p), offset)
        return self.add("clothes", obj, key, symmetric)

    def bone_layer(self):
        return self.add("bones", bd.bones_h(self.arm, f"{self.name}.bones", self.prefix), "bone")

    def caps(self):
        P = self.prefix
        joints = [(j, a, b, P + c) for j, a, b, c in rig.JOINTS]
        obj = bd.caps(self.arm, f"{self.name}.caps", joint_radii(self.build), joints)
        self.caps_obj = obj
        return obj

    # -- finishing ------------------------------------------------------------------------

    def morphs(self, deflate=0.65, emaciated=0.82, extra_deflate=()):
        """`deflate` on flesh, clothes and strapped gear; `emaciated` on flesh (full) and
        clothes (a third: loose clothes hang)."""
        bones = [self.prefix + b for b in rig.BODY_BONES]
        for o in self.layers["flesh"] + self.layers["clothes"]:
            bd.toward_bones(o, self.arm, "deflate", deflate, bones)
        # Strapped gear sinks onto the bones further: nothing holds it up any more.
        for o in extra_deflate:
            bd.toward_bones(o, self.arm, "deflate", 0.3, bones)
        thin = {self.prefix + b: 1.0 - (1.0 - emaciated) * 0.4 for b in ("head", "neck", "hand_l", "hand_r", "foot_l", "foot_r", "toe_l", "toe_r")}
        for o in self.layers["flesh"]:
            bd.toward_bones(o, self.arm, "emaciated", emaciated, bones, thin)
        for o in self.layers["clothes"]:
            bd.toward_bones(o, self.arm, "emaciated", 1.0 - (1.0 - emaciated) / 3, bones)

    def masks(self):
        layers = dict(self.layers)
        layers["held"] = [o for o in self.held.values() if o.type == "MESH"]
        for layer, objs in layers.items():
            for o in objs:
                lay = LAYER_OF[self.keys[o.name]] if layer != "bones" else "bone"
                ps = bd.parts(o)
                decay.add_masks(o, lay, seed=sum(map(ord, o.name)), part_of_vertex=ps.__getitem__)

    def colours(self, ref, palette, delight=0.0, report=None, mix=1.0):
        """Materials: one colour per part, the robust dominant colour of the part in the
        sprite reference near its `palette` colour (`project.guided`; current pose = the
        reference's; mirrored views for the symmetric parts; `delight` > 0 divides the
        sprites by a fitted shading first), blended with the palette colour by `mix`
        (1 = sprite only); without a reference the palette. Each colour becomes a small
        procedural tile (cloth, leather, wood, ...) on the box-projected UV map `detail`.
        Bones and stump caps are flat. Returns {object: sRGB}."""
        objs = self.layers["flesh"] + self.layers["clothes"] + self.layers["gear"] + list(self.held.values())
        objs = [o for o in objs if o.type == "MESH"]
        info = {}
        if ref is not None:
            _image, info = project.project_uvs(objs, ref, mirror=self.symmetric, delight=delight, report=report,
                                               name=f"{self.name}.sprite")
            for o in objs:
                if "sprite" in o.data.uv_layers:
                    o.data.uv_layers.remove(o.data.uv_layers["sprite"])
        mats = {}

        def mat(key, rgb, kind=None):
            kind = kind or LAYER_OF[key]
            k = (key, tuple(round(c, 3) for c in rgb))
            if k not in mats:
                n = f"{self.name}.{key}.{len(mats)}"
                img = detail.tile(n, rgb, kind, seed=len(mats) + 1)
                mats[k] = decay.material(n, LAYER_OF[key], rgb, image=img, uv_map="detail",
                                         metallic=0.6 if LAYER_OF[key] == "metal" else 0.0)
                mats[k]["acr_colour_from"] = "sprite" if ref is not None else "palette"
            return mats[k]

        # Parts with the same key share one colour, taken from all their samples.
        pooled = {}
        for o in objs:
            k = (o.get("acr_material_keys") or [self.keys[o.name]])[0]
            pooled.setdefault(k, []).extend(info.get(o.name, (None, None, []))[2])
        chosen = {}
        for o in objs:
            keys = list(o.get("acr_material_keys") or [self.keys[o.name]])
            samples = pooled.get(keys[0], [])
            o.data.materials.clear()
            for i, k in enumerate(keys):
                rgb = palette[k]
                if len(samples) and i == 0:
                    seen = project.guided(samples, rgb)
                    rgb = tuple(mix * a + (1 - mix) * b for a, b in zip(seen, rgb))
                o.data.materials.append(mat(k, rgb))
                if i == 0:
                    chosen[o.name] = rgb
            detail.box_uv(o)
        for o in self.layers["bones"]:
            bd.set_material(o, mat("bone", palette.get("bone", (0.85, 0.81, 0.69))))
            detail.box_uv(o)
        if getattr(self, "caps_obj", None):
            c = self.caps_obj
            c.data.materials.clear()
            c.data.materials.append(decay.material(f"{self.name}.stump_meat", "flesh", decay.GORE["stump_meat"]))
            c.data.materials.append(decay.material(f"{self.name}.stump_bone", "bone", decay.GORE["stump_bone"]))
        return chosen

    def all_meshes(self):
        out = [o for objs in self.layers.values() for o in objs]
        if getattr(self, "caps_obj", None):
            out.append(self.caps_obj)
        return out + [o for o in self.held.values() if o.type == "MESH"]

    def triangles(self):
        n = {}
        for o in self.all_meshes():
            o.data.calc_loop_triangles()
            n[o.name] = len(o.data.loop_triangles)
        return n


def prepare_export(objects, extras_root=None, **props):
    """Swaps decay materials for their glTF twins, drops build-time attributes and sets
    root extras."""
    for o in objects:
        if o.type != "MESH":
            continue
        for i, m in enumerate(o.data.materials):
            if m is not None and "acr_decay" in m and not m.name.endswith(".gltf"):
                twin = bpy.data.materials.get(m.name + ".gltf") or decay.export_material(m)
                o.data.materials[i] = twin
        bd.finish_mesh(o)
        if "part" in o.data.attributes:
            o.data.attributes.remove(o.data.attributes["part"])
        for k in ("decay",):
            if k in o:
                del o[k]
    if extras_root is not None:
        for k, v in props.items():
            extras_root[k] = v


def set_decay(objects, value, morph_owner=None):
    """Preview: the decay parameter on every object and the `deflate` weight."""
    for o in objects:
        o["decay"] = value
        keys = o.data.shape_keys if o.type == "MESH" else None
        if keys and "deflate" in keys.key_blocks:
            keys.key_blocks["deflate"].value = decay.deflate_weight(value)
    bpy.context.view_layer.update()


def drop(obj, world):
    """Detaches gear from its socket and lays it at `world` (what the renderer does when the
    corpse lands)."""
    obj.parent = None
    obj.matrix_world = world
    obj["acr_dropped"] = True
