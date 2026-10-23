"""Human characters on skeleton H (needs `bpy`): the body graph, a `Character` that collects
the layers, and the finishing steps every unit shares (cuts, caps, masks, morph targets,
materials from the sprites, export preparation). Unit scripts (`assets/units/*.py`) describe
clothes, gear, poses and clips. See docs/assets-humans.md.
"""

import math
import os

import bpy
from mathutils import Matrix, Vector

from . import armature as am
from . import body as bd
from . import decay, detail, heads, held, limb, mbody, project, registry, rig

# Layer order (#178) and the decay layer of every material key a unit may use; colours of
# keys a unit's palette may leave out (sRGB). Families add keys with `registry.material`.
LAYER_OF = registry.LAYER_OF
FALLBACK = registry.FALLBACK
# `drape.settle` before the morphs (docs/clothing.md, "Layers"): every inner layer pushed
# along the outer layer's normal to a few mm under it, cloth polygons turned outwards. Off:
# on the #287 trisoup2 sample it cut the counted pokes (2034 -> 1307, mostly the turned
# winding) but not the patches on the renders, and it opened new slivers on EngOf17.
# `ACR_SETTLE=1` in a local Blender switches it on (the VM gets no env).
SETTLE = bool(os.environ.get("ACR_SETTLE"))


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
        self.face_frame = None  # heads.Frame of the refined head (#283)
        self.variants = {}  # hair/beard variant suffix -> object (heads.variants)

    # -- layers ---------------------------------------------------------------------------

    def add(self, layer, obj, key, symmetric=False):
        self.layers[layer].append(obj)
        self.keys[obj.name] = key
        if symmetric:
            self.symmetric.append(obj)
        return obj

    def body(self, key="skin", bones=rig.BODY_BONES, faces=True, shoulders=True):
        """The skin hull; with `faces` a finer head with a modelled face (#283). `shoulders`:
        re-mesh the shoulder line (`bd.SHOULDER_REMESH`); False under drafted garments
        (`drape.dress`), whose coats come out torn over the re-meshed body (#287 trisoup8)."""
        P = self.prefix
        g = graph(self.build)
        cx = next(p for n, _par, p, _r in g if n == "skull")[0]

        def mh_body(bm):
            seg = am.segments(self.arm)
            mbody.build(bm, seg, P, self.build)
            if bd.SHOULDER_REMESH and shoulders:
                print(f"[{self.name}] shoulder line re-meshed: {bd.remesh_shoulders(bm, seg, P)}")

        def head(bm):
            self.face_frame = heads.refine(bm, cx, mh_body if self.build.get("mh_body", True) else None)

        self.flesh = bd.hull(self.arm, f"{self.name}.flesh", g, bones=[P + b for b in bones], head=head if faces else None)
        if self.build.get("mh_body", True):
            mbody.belly_weights(self.flesh, self.arm, P)
        if faces:
            heads.head_weights(self)
        bd.split_parts(self.flesh, lambda b: rig.part_of_bone(b[len(P):]))
        self.add("flesh", self.flesh, key, symmetric=True)
        return self.flesh

    def shell(self, name, key, keep_face, offset, symmetric=True, neck=None):
        """A clothing shell of the flesh (`bd.derive`). A shell that covers the top of the
        trunk gets its neck opening cut by distance from the neck (`bd.neck_opening`, an
        ellipse `neck` metres outside the skin of the neck, default `bd.NECK_EASE`) instead of
        by part and height, so the trapezius stays whole in it (#287 trisoup7). `neck=False`:
        no opening (a hood or collar standing round the neck, trisoup8)."""
        P = self.prefix
        shoe = key in mbody.FOOTWEAR and self.build.get("mh_body", True)
        feet = {"foot_l", "foot_r", "toe_l", "toe_r"}

        def keep(c, n, h, p):
            h = h[len(P):]
            return keep_face(c, n, h, p) and not (shoe and h in feet and c.z < self.arm.data.bones[P + "foot_l"].head_local.z + 0.045)

        field = None
        if bd.NECK_CUT and neck is not False and self.build.get("mh_body", True):
            opening = bd.neck_opening(self.flesh, self.arm, keep, P, neck)
            if opening:
                keep, field = opening
                print(f"[{self.name}] {name}: neck opening by distance, radius {field.radius:.3f}")
        obj = bd.derive(self.flesh, f"{self.name}.{name}", keep, offset, border=self.build.get("mh_body", True), field=field)
        obj = self.add("clothes", obj, key, symmetric)
        if shoe and not getattr(self, "shoes", None):
            # The MakeHuman feet stay bare skin; footwear gets a shoe over a last (#287).
            self.shoes = self.add("clothes", mbody.shoes(self.arm, self.flesh, f"{self.name}.footwear", P, key=key), key, symmetric)
        return obj

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
        clothes (a third: loose clothes hang). With `SETTLE` every layer is first settled under
        the layers on it (`drape.settle`, docs/clothing.md "Layers"), so the morphs keep the order."""
        from . import drape

        if SETTLE:
            self.settle_report = drape.settle(self)
            print(f"[{self.name}] settled under the layers above: {self.settle_report}")
        # The unit's own layer order (`acr_layer` on the materials) and its hidden garment
        # faces deleted (docs/clothing.md, "Layers").
        order = None
        if drape.ORDER or drape.CULL:
            order = drape.layer_order(self)
            print(f"[{self.name}] layer order: {order} from {self.layer_votes}")
        if drape.RELAX:
            print(f"[{self.name}] free edges relaxed: {drape.relax_edges(self)}")
        if drape.CULL:
            before = sum(self.triangles().values())
            self.cull_report = drape.cull_hidden(self, order)
            print(f"[{self.name}] hidden faces culled: {self.cull_report}, triangles {before} -> {sum(self.triangles().values())}")
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
        self.limb_levels(layers)

    def limb_levels(self, layers):
        """Writes `_limb` (float: limb level 0..1, trunk -1; `acr_assets.limb`) on every mesh
        and sets `self.limb_radius` = {limb: [limb.SAMPLES outer radii of skin and clothes]}. Skin, bones and
        stump caps project their rest positions onto the limb chain of their part; clothes
        and strapped gear take the level of the nearest skin vertex of the same limb; held
        gear and the horse are -1."""
        from mathutils import kdtree

        P = self.prefix
        bones = self.arm.data.bones
        chains = {n: [tuple(getattr(bones[P + b], end + "_local")) for b, end in spec[1]]
                  for n, spec in limb.LIMBS.items()}
        joints = rig.JOINTS

        def write(o, values):
            a = o.data.attributes.get("_limb") or o.data.attributes.new("_limb", "FLOAT", "POINT")
            for d, x in zip(a.data, values):
                d.value = x

        skin = {n: [] for n in limb.LIMBS}  # limb -> [(co, level)]
        radius = {n: [] for n in limb.LIMBS}
        projected = layers["flesh"] + layers["bones"] + ([self.caps_obj] if getattr(self, "caps_obj", None) else [])
        for o in projected:
            ps = bd.parts(o)
            vals = []
            for v, pt in zip(o.data.vertices, ps):
                name, lv, dist = limb.vertex_level(tuple(v.co), limb.body_part(pt, joints), chains)
                vals.append(lv)
                if name and o in layers["flesh"]:
                    skin[name].append((v.co.copy(), lv))
                    radius[name].append((lv, dist))
            write(o, vals)
        trees = {}
        for n, pts in skin.items():
            t = kdtree.KDTree(len(pts))
            for i, (co, _lv) in enumerate(pts):
                t.insert(co, i)
            t.balance()
            trees[n] = t
        for o in layers["clothes"] + layers["gear"]:
            ps = bd.parts(o)
            vals = []
            for v, pt in zip(o.data.vertices, ps):
                name, _lv, dist = limb.vertex_level(tuple(v.co), limb.body_part(pt, joints), chains)
                if name is None or not skin[name]:
                    vals.append(limb.TRUNK)
                    continue
                _co, i, _d = trees[name].find(v.co)
                lv = skin[name][i][1]
                vals.append(lv)
                # The outer radius is skin plus clothes: rigid strapped gear (cartridge box,
                # knapsack, scabbard) and crossbelts (they run off the body at the shoulder
                # and the hip) would inflate it.
                if o in layers["clothes"] and self.keys.get(o.name) not in ("belts",):
                    radius[name].append((lv, dist))
            write(o, vals)
        for o in layers.get("held", []):
            write(o, [limb.TRUNK] * len(o.data.vertices))
        self.limb_radius = {n: limb.radius_profile(radius[n]) for n in limb.LIMB_ORDER}

    def colours(self, ref, palette, delight=0.0, report=None, mix=1.0, fixed=()):
        """Materials: one colour per part, the robust dominant colour of the part in the
        sprite reference near its `palette` colour (`project.guided`; current pose = the
        reference's; mirrored views for the symmetric parts; `delight` > 0 divides the
        sprites by a fitted shading first), blended with the palette colour by `mix`
        (1 = sprite only); without a reference the palette. Keys in `fixed` keep their
        palette colour whatever the sprite shows (a white waistcoat the projection would
        tint with the coat's red). Each colour becomes a small
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
                if key in getattr(self, "layer_rank", {}):
                    mats[k]["acr_layer"] = float(self.layer_rank[key])
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
            slots = []
            for i, k in enumerate(keys):
                rgb = palette[k] if k in palette else FALLBACK[k]
                if len(samples) and i == 0 and k not in fixed:
                    seen = project.guided(samples, rgb)
                    rgb = tuple(mix * a + (1 - mix) * b for a, b in zip(seen, rgb))
                slots.append(mat(k, rgb))
                if i == 0:
                    chosen[o.name] = rgb
            held.set_materials(o.data, slots)  # keeps the faces' slots (drum, musket, saddle)
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
        return out + [o for o in self.held.values() if o.type == "MESH"] + list(self.variants.values())

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
