"""Generated gear (Pixal3D image-to-3D) made game-ready: headgear, held weapons, worn gear.

A generated model is a local-only source (never committed): `$ACR_GENERATED/<rel>`, else
`$ACR_SOURCES/generated/<rel>` (render VM: `/home/claude/sources/generated/pixal3d/`;
locally `D:\\ACR-models\\generated\\pixal3d\\`, found as the `generated` folder beside
`ACR_SOURCES`). It comes as ~50k triangles of triangle soup with a 2k texture, a few
thousand non-manifold edges and loose crumbs. `fit(rel)` turns it into one item:

1. import, join, turn +Y-facing to -Y (Pixal3D faces the camera at +Y);
2. repair: weld, dissolve degenerate faces, drop loose crumbs (< 5 % of the largest
   island), fill small holes, recalculate normals (`non_manifold` before/after); drop the
   white backdrop sheet Pixal3D extrudes behind an item (`drop_backdrop`), then orient again;
3. orient and scale by category (`ITEMS`): headgear width along X; a held item's flat
   (least-variance) side along X, the long side of its least-area outline on +Z with the
   grip end at the origin, its blade/head width along Y (a pistol: barrel +Z, butt +Y,
   origin at the wrist, `_butt_grip`); props stand on z = 0; worn gear thinnest
   along Y, the body side at +Y;
4. headgear: band plane above the eyes of the MakeHuman scalp, raised until the crown
   clears it; faces inside a scalp-shaped loft (radius rings raycast from the head, plus
   `EASE`) are dropped, so the head opening fits (`band_gap_m`);
5. collapse-decimate a seam-free copy (UVs removed, welded) to the category budget;
6. Smart-UV, bake base colour (EMIT of the source albedo: a DIFFUSE bake multiplies by
   1 - metallic and blackened metal) and tangent normals from the source (Cycles, selected
   to active); missed texels are measured (`tex_unbaked_frac`: transparent texels inside
   the UV islands, margin 0) and then filled by dilation; both images packed and bound to
   the material (no magenta). Images are PNG: the renderer decodes PNG only;
7. origin: headgear at `socket.hat` (world position given), held items at the grip point
   (`grip_fraction`: the guard or the head along the long axis), worn gear at the middle
   of the body side; `acr_grip` / `acr_length` on the object for FABRIK grips.

Numbers go to the returned stats: `source_triangles`, `triangles`, `non_manifold`
(before, after repair, after decimation), `islands`, `deviation` (mean, p99, max of the
two-way vertex-to-surface distance; headgear outside the head cavity), `band_gap_m`.

A unit wears a generated hat with SPEC `hat_source` (`line.make`): the object gets a
decay material on its baked image and `acr_baked`, so `Character.colours` and
`outfit.dress` leave it alone. The pure parts (tables, `grip_fraction`, `orient_plan`,
`summary`, `islands`, `source_path`) are tested in `tests/test_genfit.py`.
"""

import math
import os

# Category: triangle budget, texture size (px), bake cage (fraction of the item's size).
CATEGORIES = {
    "headgear": {"tris": 4000, "tex": 1024},
    "held": {"tris": 1500, "tex": 512},
    "worn": {"tris": 2000, "tex": 512},
    "prop": {"tris": 3000, "tex": 1024},
}

# Item: category, real size in m (headgear: width along X; held: length; worn: largest
# extent), decay layer, optional `band` (headgear: band plane above the eye line, m),
# `kind` (held: "guard" for swords, the grip between guard and the nearer end; "head" for
# clubs and axes, the grip at the end away from the wider quarter), `grip` (held: grip
# fraction along the length from the grip end, overrides the profile) and `grip_at` ("peak": the grip is the widest stretch, e.g. a pistol's butt).
ITEMS = {
    # headgear (hats/)
    "eq_aztec_eagle_helmet": {"category": "headgear", "size": 0.30, "layer": "hide", "band": 0.030},
    "eq_cabasset": {"category": "headgear", "size": 0.30, "layer": "metal"},
    "eq_dragoon_cap": {"category": "headgear", "size": 0.23, "layer": "wool"},
    "eq_grenadier_cap": {"category": "headgear", "size": 0.23, "layer": "wool"},
    "eq_lobster_pot_helmet": {"category": "headgear", "size": 0.28, "layer": "metal", "band": 0.025},
    "eq_morion": {"category": "headgear", "size": 0.40, "layer": "metal"},
    "eq_pikeman_pot": {"category": "headgear", "size": 0.36, "layer": "metal", "band": 0.030},
    "eq_scotch_bonnet": {"category": "headgear", "size": 0.29, "layer": "wool"},
    "eq_tricorn_hat": {"category": "headgear", "size": 0.40, "layer": "wool", "band": 0.045},
    "eq_tricorn_officer": {"category": "headgear", "size": 0.42, "layer": "wool", "band": 0.045},
    # headgear (gear/)
    "eq_gustoweh": {"category": "headgear", "size": 0.24, "layer": "hide"},
    "eq_war_bonnet": {"category": "headgear", "size": 0.45, "layer": "hide"},
    "eq_quetzal_headdress": {"category": "headgear", "size": 0.60, "layer": "hide"},
    # held
    "eq_bow_native": {"category": "held", "size": 1.40, "layer": "wood", "grip": 0.5},
    "eq_pike": {"category": "held", "size": 4.50, "layer": "wood", "grip": 0.35},
    "eq_spontoon": {"category": "held", "size": 2.20, "layer": "wood", "grip": 0.40},
    "eq_smallsword": {"category": "held", "kind": "guard", "size": 0.95, "layer": "metal"},
    "eq_champi_mace": {"category": "held", "kind": "head", "size": 0.75, "layer": "wood"},
    "eq_cutlass": {"category": "held", "kind": "guard", "size": 0.80, "layer": "metal"},
    "eq_hanger_infantry": {"category": "held", "kind": "guard", "size": 0.80, "layer": "metal"},
    "eq_macuahuitl": {"category": "held", "kind": "guard", "size": 0.95, "layer": "wood"},
    "eq_pistol_flintlock": {"category": "held", "size": 0.40, "layer": "wood", "grip_at": "peak"},
    "eq_rapier_conquistador": {"category": "held", "kind": "guard", "size": 1.10, "layer": "metal"},
    "eq_sabre_cavalry": {"category": "held", "kind": "guard", "size": 1.00, "layer": "metal"},
    "eq_tomahawk": {"category": "held", "kind": "head", "size": 0.50, "layer": "wood"},
    "eq_warclub_ball": {"category": "held", "kind": "head", "size": 0.65, "layer": "wood"},
    "art_rammer_sponge": {"category": "held", "size": 2.80, "layer": "wood", "grip": 0.25},
    # worn
    "eq_baldric_bandolier": {"category": "worn", "size": 0.90, "layer": "leather"},
    "eq_breastplate": {"category": "worn", "size": 0.42, "layer": "metal"},
    "eq_canteen": {"category": "worn", "size": 0.22, "layer": "wood"},
    "eq_cartridge_box": {"category": "worn", "size": 0.20, "layer": "leather"},
    "eq_corselet_17c": {"category": "worn", "size": 0.55, "layer": "metal"},
    "eq_crossbelts": {"category": "worn", "size": 0.90, "layer": "leather"},
    "eq_drum_18c": {"category": "worn", "size": 0.40, "layer": "wood"},
    "eq_gorget": {"category": "worn", "size": 0.12, "layer": "metal"},
    "eq_haversack": {"category": "worn", "size": 0.35, "layer": "linen"},
    "eq_knapsack": {"category": "worn", "size": 0.42, "layer": "hide"},
    "eq_quilted_armour": {"category": "worn", "size": 0.70, "layer": "linen"},
    "eq_shield_chimalli": {"category": "worn", "size": 0.60, "layer": "hide"},
    "eq_shield_inca": {"category": "worn", "size": 0.50, "layer": "hide"},
    "eq_shield_round_hide": {"category": "worn", "size": 0.55, "layer": "hide"},
    "eq_sling_inca": {"category": "held", "size": 1.00, "layer": "linen", "grip": 0.05},
    # props (standing on the ground, size = largest extent)
    "art_ammo_chest": {"category": "prop", "size": 0.90, "layer": "wood"},
    "art_canister_rounds": {"category": "prop", "size": 0.30, "layer": "metal"},
    "art_cannonballs": {"category": "prop", "size": 0.50, "layer": "metal"},
    "art_field_gun": {"category": "prop", "size": 2.60, "layer": "wood"},
    "art_limber": {"category": "prop", "size": 2.80, "layer": "wood"},
    "mortar": {"category": "prop", "size": 1.00, "layer": "metal"},
    "saddle": {"category": "prop", "size": 0.70, "layer": "leather"},
    # map props (assets/nature/prop_*.py, nat_*.py via `nature.build_generated`): own
    # triangle budget and texture per row (`tris`, `tex` override the category). Rows
    # without a script failed (docs/assets.md "Generated map props"); kept to retry.
    "prop_well_stone": {"category": "prop", "size": 1.8, "layer": "wood", "tris": 2000, "tex": 512},
    "prop_well_colonial": {"category": "prop", "size": 2.4, "layer": "wood", "tris": 2500, "tex": 512},
    "prop_ruin_wall": {"category": "prop", "size": 5.0, "layer": "wood", "tris": 4000, "tex": 1024},
    "prop_ruin_chapel": {"category": "prop", "size": 8.0, "layer": "wood", "tris": 6000, "tex": 1024},
    "prop_inca_wall": {"category": "prop", "size": 5.0, "layer": "wood", "tris": 4000, "tex": 1024},
    "prop_wigwam": {"category": "prop", "size": 4.0, "layer": "hide", "tris": 3500, "tex": 1024},
    "prop_burial_scaffold": {"category": "prop", "size": 3.0, "layer": "wood", "tris": 2500, "tex": 512},
    "prop_grave_cross": {"category": "prop", "size": 1.1, "layer": "wood", "tris": 600, "tex": 512},
    "prop_firewood": {"category": "prop", "size": 1.4, "layer": "wood", "tris": 1500, "tex": 512},
    "prop_palisade_gate": {"category": "prop", "size": 4.0, "layer": "wood", "tris": 3000, "tex": 1024},
    "prop_maya_stela": {"category": "prop", "size": 3.0, "layer": "wood", "tris": 2000, "tex": 512},
    "prop_aztec_altar": {"category": "prop", "size": 2.0, "layer": "wood", "tris": 2000, "tex": 512},
    "prop_market_stall": {"category": "prop", "size": 3.0, "layer": "wood", "tris": 3000, "tex": 1024},
    "prop_cannon_rack": {"category": "prop", "size": 2.0, "layer": "wood", "tris": 2000, "tex": 512},
    "prop_bones": {"category": "prop", "size": 1.2, "layer": "wood", "tris": 1200, "tex": 512},
    "nat_stone_pile": {"category": "prop", "size": 1.5, "layer": "wood", "tris": 1500, "tex": 512},
    "nat_fallen_log": {"category": "prop", "size": 5.0, "layer": "wood", "tris": 2000, "tex": 512},
    "nat_dry_bush": {"category": "prop", "size": 1.5, "layer": "wood", "tris": 3000, "tex": 512},
    "nat_pumpkins": {"category": "prop", "size": 1.2, "layer": "wood", "tris": 1500, "tex": 512},
    "nat_maize": {"category": "prop", "size": 2.2, "layer": "wood", "tris": 3000, "tex": 512},
}

BAND = 0.035     # default band plane above the eye line (m)
EASE = 0.002     # inner band clearance over the scalp (m)
WELD = 0.0004    # weld distance of the seam-free copy, at real size (m)
CRUMB = 0.05     # islands smaller than this fraction of the largest are dropped
HEAD_END = 0.12  # a widest stretch within this fraction of an end is a head (club, axe)
GRIP_IN = 0.10   # grip centre from the butt of a headed item, at most (m)


# -- pure ---------------------------------------------------------------------------------

def item(rel):
    """The `ITEMS` row of a source path (`pixal3d/eq_cutlass.glb` → `eq_cutlass`)."""
    stem = os.path.splitext(os.path.basename(rel))[0]
    if stem not in ITEMS:
        raise KeyError(f"generated item {stem!r} has no ITEMS row (have {sorted(ITEMS)})")
    return dict(ITEMS[stem], name=stem)


def source_path(rel, env=None):
    """Absolute path of generated source `rel`: `$ACR_GENERATED/rel`, else
    `$ACR_SOURCES/generated/rel`, else `<parent of ACR_SOURCES>/generated/rel`."""
    env = os.environ if env is None else env
    cands = []
    if env.get("ACR_GENERATED"):
        cands.append(os.path.join(env["ACR_GENERATED"], rel))
    if env.get("ACR_SOURCES"):
        root = env["ACR_SOURCES"].rstrip("/\\")
        cands.append(os.path.join(root, "generated", rel))
        cands.append(os.path.join(os.path.dirname(root), "generated", rel))
    for c in cands:
        if os.path.exists(c):
            return c
    raise FileNotFoundError(f"generated source {rel!r} not found (tried {cands}; set ACR_GENERATED or ACR_SOURCES)")


def summary(dists):
    """{mean_m, p99_m, max_m} of a list of distances."""
    d = sorted(dists)
    if not d:
        return {"mean_m": None, "p99_m": None, "max_m": None}
    p99 = d[min(len(d) - 1, int(math.ceil(0.99 * len(d))) - 1)]
    return {"mean_m": round(sum(d) / len(d), 5), "p99_m": round(p99, 4), "max_m": round(d[-1], 4)}


def prop_wind(z, top, horiz, reach, mode, rgb=None, phase=0.0):
    """The `_WIND` channels (x trunk bend, y branch, z phase, w leaf flutter) of one vertex of
    a generated prop: `mode` None (hard props: zeros), "stalk" (the whole plant sways with
    height, leaves flutter from a fifth of the height up: maize, dry bushes) or "green" (only
    green texels flutter, nothing bends: pumpkin leaves). `rgb` is the vertex's baked colour."""
    if not mode or top <= 0:
        return (0.0, 0.0, 0.0, 0.0)
    h = min(max(z / top, 0.0), 1.0)
    if mode == "stalk":
        bend = min(1.0, top / 6.0)
        w = min(1.0, max(0.0, (h - 0.2) / 0.3))
        y = min(horiz / max(reach, 1e-6), 1.0) * min(1.0, reach / 2.5)
        return (bend * h * h, y, phase if w else 0.0, w)
    if mode == "green":
        r, g, b = rgb if rgb is not None else (0.0, 0.0, 0.0)
        w = 1.0 if g > r + 0.02 and g > b + 0.04 else 0.0
        return (0.0, 0.0, phase if w else 0.0, w)
    raise ValueError(f"unknown wind mode {mode!r}")


def islands(n, edges):
    """Number of connected components of `n` vertices joined by `edges` (pairs), vertices
    without edges not counted."""
    parent = list(range(n))

    def find(a):
        while parent[a] != a:
            parent[a] = parent[parent[a]]
            a = parent[a]
        return a

    used = set()
    for a, b in edges:
        used.add(a)
        used.add(b)
        ra, rb = find(a), find(b)
        if ra != rb:
            parent[ra] = rb
    return len({find(v) for v in used})


def orient_plan(extents, category):
    """Axis permutation for an item with bounding-box `extents` (x, y, z): returns
    (new_x, new_y, new_z) as source axis indices. Held: the longest axis becomes Z, the
    wider of the other two Y (blade edge / head width facing -Y). Worn: the thinnest axis
    becomes Y, the longest of the others Z unless Z is not thin. Headgear: unchanged."""
    order = sorted(range(3), key=lambda i: extents[i])  # thin .. long
    if category == "held":
        z = order[2]
        y = order[1]
        x = order[0]
    elif category == "worn":
        y = order[0]
        rest = [i for i in (0, 1, 2) if i != y]
        z = 2 if 2 in rest else max(rest, key=lambda i: extents[i])
        x = next(i for i in rest if i != z)
    else:
        return (0, 1, 2)
    return (x, y, z)


def grip_fraction(widths, length, spec=None):
    """Where a held item is gripped, from its width profile along the long axis.

    `widths`: max cross extent per bin from one end (bin 0) to the other. Returns
    (grip end, fraction): grip end 0 or 1 (which end becomes the butt at z = 0) and the
    grip centre as a fraction of the length from that end.
    - the widest stretch within `HEAD_END` of an end is a head (club, axe, mace, spear):
      the grip is at the other end, `GRIP_IN` (or 15 % of the length) in;
    - otherwise it is a guard (sword): the grip lies between the guard and the nearer end;
    - `spec["grip"]` fixes the fraction (bow riser, pike), `spec["grip_at"] == "peak"`
      grips the widest stretch itself (a pistol's butt)."""
    spec = spec or {}
    n = len(widths)
    peak = max(range(n), key=lambda i: (widths[i], -abs(i - (n - 1) / 2)))
    lo, hi = peak, n - 1 - peak
    if "grip" in spec:
        end = 0 if lo >= hi else 1  # the head (widest) away from the grip end
        return end, float(spec["grip"])
    if spec.get("grip_at") == "peak":
        end = 0 if lo <= hi else 1
        return end, (min(lo, hi) + 0.5) / n
    kind = spec.get("kind") or ("head" if min(lo, hi) < HEAD_END * n else "guard")
    if kind == "head":
        q = max(1, n // 4)
        end = 0 if sum(widths[-q:]) >= sum(widths[:q]) else 1
        return end, min(GRIP_IN, 0.15 * length) / length
    end = 0 if lo < hi else 1
    return end, (min(lo, hi) / 2 + 0.25) / n


# -- Blender --------------------------------------------------------------------------------

def _tris(me):
    return sum(len(p.vertices) - 2 for p in me.polygons)


def _non_manifold(me):
    import bmesh

    bm = bmesh.new()
    bm.from_mesh(me)
    n = sum(1 for e in bm.edges if not e.is_manifold)
    bm.free()
    return n


def _nm_kinds(me):
    """Non-manifold edges by kind: open (one face), multi (three or more faces), wire."""
    import bmesh

    bm = bmesh.new()
    bm.from_mesh(me)
    k = {"open": 0, "multi": 0, "wire": 0}
    for e in bm.edges:
        n = len(e.link_faces)
        if n == 1:
            k["open"] += 1
        elif n > 2:
            k["multi"] += 1
        elif n == 0:
            k["wire"] += 1
    bm.free()
    return k


def _islands(me):
    return islands(len(me.vertices), [tuple(e.vertices) for e in me.edges])


def _bvh(ob):
    import bpy
    from mathutils.bvhtree import BVHTree

    return BVHTree.FromObject(ob, bpy.context.evaluated_depsgraph_get())


def _base_image(ob):
    for m in ob.data.materials:
        if not (m and m.node_tree):
            continue
        bsdf = next((n for n in m.node_tree.nodes if n.type == "BSDF_PRINCIPLED"), None)
        if bsdf and bsdf.inputs["Base Color"].is_linked:
            n = bsdf.inputs["Base Color"].links[0].from_node
            while n.type != "TEX_IMAGE" and n.inputs and any(i.is_linked for i in n.inputs):
                n = next(i for i in n.inputs if i.is_linked).links[0].from_node
            if n.type == "TEX_IMAGE":
                return n.image
    return None


def load(path, name):
    """Imports a generated glb as one mesh object at the world origin, turned to face -Y."""
    import bpy
    from mathutils import Matrix

    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=path)
    new = [o for o in bpy.data.objects if o not in before]
    meshes = [o for o in new if o.type == "MESH"]
    bpy.ops.object.select_all(action="DESELECT")
    for o in meshes:
        o.select_set(True)
    bpy.context.view_layer.objects.active = meshes[0]
    if len(meshes) > 1:
        bpy.ops.object.join()
    ob = bpy.context.view_layer.objects.active
    mw = ob.matrix_world.copy()
    ob.parent = None
    ob.data.transform(Matrix.Rotation(math.pi, 4, "Z") @ mw)
    ob.matrix_world = Matrix.Identity(4)
    for o in new:
        if o != ob and o.name in bpy.data.objects:
            bpy.data.objects.remove(o, do_unlink=True)
    ob.name = name
    return ob


def repair(ob, weld):
    """Weld, dissolve degenerate faces, drop crumbs, fill small holes, normals outward.
    Returns the non-manifold edge count after."""
    import bmesh

    bm = bmesh.new()
    bm.from_mesh(ob.data)
    bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=weld)
    bmesh.ops.dissolve_degenerate(bm, edges=bm.edges, dist=weld * 0.25)
    bm.faces.ensure_lookup_table()
    comps = _components(bm)
    if comps:
        big = len(comps[0])
        bmesh.ops.delete(bm, geom=[f for c in comps[1:] if len(c) < CRUMB * big for f in c], context="FACES")
    bmesh.ops.delete(bm, geom=[v for v in bm.verts if not v.link_faces], context="VERTS")
    bmesh.ops.delete(bm, geom=[e for e in bm.edges if not e.link_faces], context="EDGES")
    # Edges with three or more faces stall the collapse: drop faces that repeat another's
    # vertices, then split the remaining fins off (they become short open borders).
    bm.verts.index_update()
    seen, dup = set(), []
    for f in bm.faces:
        k = frozenset(v.index for v in f.verts)
        if k in seen:
            dup.append(f)
        seen.add(k)
    bmesh.ops.delete(bm, geom=dup, context="FACES")
    multi = [e for e in bm.edges if len(e.link_faces) > 2]
    if multi:
        bmesh.ops.split_edges(bm, edges=multi)
        bm.faces.ensure_lookup_table()
        comps = _components(bm)
        bmesh.ops.delete(bm, geom=[f for c in comps[1:] if len(c) < 0.01 * len(comps[0]) for f in c], context="FACES")
        bmesh.ops.delete(bm, geom=[v for v in bm.verts if not v.link_faces], context="VERTS")
    bmesh.ops.holes_fill(bm, edges=[e for e in bm.edges if e.is_boundary], sides=8)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    bm.to_mesh(ob.data)
    bm.free()
    ob.data.update()
    return _non_manifold(ob.data)


def is_backdrop(n_faces, open_edges, thin_frac):
    """A bright face region is the image backdrop Pixal3D extrudes behind an item (a white
    sheet) when its perimeter is open (at least sqrt(n) open edges: a sheet, not a closed
    part such as a drum head or a pale belt) or most of it is paper-thin (two layers)."""
    return n_faces >= 8 and (open_edges >= math.sqrt(n_faces) or thin_frac > 0.5)


BRIGHT = 0.72     # backdrop texels: every channel at least this (sRGB) ...
GREY = 0.10       # ... and channels within this of each other


def drop_backdrop(ob):
    """Deletes the white backdrop sheet (`is_backdrop`) from a repaired source; returns the
    number of faces removed. Faces are bright by the source texture at their UV centre."""
    import bmesh
    import numpy as np
    from mathutils.bvhtree import BVHTree

    img = _base_image(ob)
    if img is None or not img.size[0]:
        return 0
    w, h = img.size
    px = np.empty(w * h * 4, np.float32)
    img.pixels.foreach_get(px)
    px = px.reshape(h, w, 4)
    bm = bmesh.new()
    bm.from_mesh(ob.data)
    uvl = bm.loops.layers.uv.active
    if uvl is None:
        bm.free()
        return 0
    bm.faces.ensure_lookup_table()
    size = max(_ext(ob))
    bright = set()
    for f in bm.faces:
        u = sum(lp[uvl].uv.x for lp in f.loops) / len(f.loops)
        v = sum(lp[uvl].uv.y for lp in f.loops) / len(f.loops)
        c = px[min(h - 1, max(0, int((v % 1.0) * h))), min(w - 1, max(0, int((u % 1.0) * w))), :3]
        if c.min() >= BRIGHT and c.max() - c.min() <= GREY:
            bright.add(f.index)
    if not bright:
        bm.free()
        return 0
    tree = BVHTree.FromBMesh(bm)
    gone, seen = [], set()
    for i in sorted(bright):
        if i in seen:
            continue
        stack, reg = [bm.faces[i]], []
        seen.add(i)
        while stack:
            f = stack.pop()
            reg.append(f)
            for e in f.edges:
                for g in e.link_faces:
                    if g.index in bright and g.index not in seen:
                        seen.add(g.index)
                        stack.append(g)
        open_edges = len({e.index for f in reg for e in f.edges if e.is_boundary})
        thin = 0
        for f in reg[:: max(1, len(reg) // 200)]:
            c, n = f.calc_center_median(), f.normal
            hit = tree.ray_cast(c - n * 1e-5 * size, -n, 0.004 * size)
            thin += hit[0] is not None
        if is_backdrop(len(reg), open_edges, thin / max(1, len(reg[:: max(1, len(reg) // 200)]))):
            gone += reg
    bmesh.ops.delete(bm, geom=gone, context="FACES")
    bm.faces.ensure_lookup_table()
    comps = _components(bm)
    if comps:
        bmesh.ops.delete(bm, geom=[f for c in comps[1:] if len(c) < CRUMB * len(comps[0]) for f in c], context="FACES")
    bmesh.ops.delete(bm, geom=[v for v in bm.verts if not v.link_faces], context="VERTS")
    bm.to_mesh(ob.data)
    bm.free()
    ob.data.update()
    return len(gone)


def _components(bm):
    """Connected face islands, largest first."""
    seen, out = set(), []
    for f in bm.faces:
        if f.index in seen:
            continue
        stack, isl = [f], []
        seen.add(f.index)
        while stack:
            g = stack.pop()
            isl.append(g)
            for e in g.edges:
                for h in e.link_faces:
                    if h.index not in seen:
                        seen.add(h.index)
                        stack.append(h)
        out.append(isl)
    return sorted(out, key=len, reverse=True)


def _coords(ob):
    return [v.co.copy() for v in ob.data.vertices]


def _extent(cs):
    lo = [min(c[i] for c in cs) for i in range(3)]
    hi = [max(c[i] for c in cs) for i in range(3)]
    return lo, hi


def _ext(ob):
    lo, hi = _extent(_coords(ob))
    return [h - l for l, h in zip(lo, hi)]


def min_area_angle(pts2, step_deg=1.0):
    """Angle (radians) that turns 2D points `pts2` (n x 2 array) so their axis-aligned
    bounding rectangle has the least area (rotating calipers by search)."""
    import numpy as np

    a = np.radians(np.arange(0.0, 90.0, step_deg))
    c, s = np.cos(a), np.sin(a)
    x = pts2[:, :1] * c - pts2[:, 1:] * s
    y = pts2[:, :1] * s + pts2[:, 1:] * c
    area = (x.max(0) - x.min(0)) * (y.max(0) - y.min(0))
    return float(a[int(area.argmin())])


def _align_principal(ob):
    """Turns `ob` so its thinnest direction (least variance: the flat of a blade, the side
    of a pistol) is X, and in the plane across it the least-area bounding rectangle's long
    side is Z, the short side Y. The PCA long axis alone tilts an L-shaped item (a pistol's
    butt pulls it off the barrel)."""
    import numpy as np
    from mathutils import Matrix

    v = np.array([c[:] for c in _coords(ob)])
    v = v - v.mean(0)
    w, e = np.linalg.eigh(np.cov(v.T))
    x, u, t = e[:, 0], e[:, 1], e[:, 2]
    p2 = np.stack([v @ t, v @ u], 1)[:: max(1, len(v) // 8000)]
    a = min_area_angle(p2)
    z = math.cos(a) * t - math.sin(a) * u
    y = math.sin(a) * t + math.cos(a) * u
    ez = np.ptp(v @ z)
    ey = np.ptp(v @ y)
    if ey > ez:
        z, y = y, -z
    x = np.cross(y, z)
    ob.data.transform(Matrix([list(x), list(y), list(z)]).to_4x4())
    ob.data.update()


def place(ob, it):
    """Orients and scales `ob` by its category; returns the matrix applied and extras."""
    from mathutils import Matrix, Vector

    cat = it["category"]
    cs = _coords(ob)
    lo, hi = _extent(cs)
    ext = [h - l for l, h in zip(lo, hi)]
    px, py, pz = orient_plan(ext, cat)
    rot = Matrix.Identity(4)
    if cat == "held":
        # the principal axis, not the box: generated weapons often lie diagonally
        _align_principal(ob)
        px, py, pz = orient_plan(_ext(ob), cat)
    if (px, py, pz) != (0, 1, 2):
        rows = [[1.0 if j == a else 0.0 for j in range(3)] for a in (px, py, pz)]
        r3 = Matrix(rows)
        if r3.determinant() < 0:
            rows[0] = [-v for v in rows[0]]
            r3 = Matrix(rows)
        rot = r3.to_4x4()
    ob.data.transform(rot)
    cs = _coords(ob)
    lo, hi = _extent(cs)
    ext = [h - l for l, h in zip(lo, hi)]
    size = ext[0] if cat == "headgear" else max(ext)
    s = it["size"] / size
    c = Vector([(a + b) / 2 for a, b in zip(lo, hi)])
    if cat in ("headgear", "prop"):
        t = Matrix.Translation((-c.x, -c.y, -lo[2]))
    else:
        t = Matrix.Translation(-c)
    m = Matrix.Scale(s, 4) @ t
    ob.data.transform(m)
    ob.data.update()
    return {"scale": round(s, 5), "extent_src": [round(e, 4) for e in ext]}


def held_grip(ob, it, bins=40):
    """Puts a held item's grip end at z = 0 (turning it over if needed) and returns the grip
    point (on the long axis) and the profile verdict."""
    from mathutils import Matrix

    cs = _coords(ob)
    lo, hi = _extent(cs)
    L = hi[2] - lo[2]
    xs = [[] for _ in range(bins)]
    for c in cs:
        k = min(bins - 1, int((c.z - lo[2]) / L * bins))
        xs[k].append(c)
    widths = []
    for b in xs:
        if not b:
            widths.append(0.0)
            continue
        widths.append(max(max(c.x for c in b) - min(c.x for c in b), max(c.y for c in b) - min(c.y for c in b)))
    end, frac = grip_fraction(widths, L, it)
    if end == 1:
        ob.data.transform(Matrix.Rotation(math.pi, 4, "Y"))
        cs = _coords(ob)
        lo, hi = _extent(cs)
        widths = widths[::-1]
    ob.data.transform(Matrix.Translation((0, 0, -lo[2])))
    z = frac * L
    # the grip point: middle of the cross-section at z
    band = [c for c in _coords(ob) if abs(c.z - z) < L / bins] or _coords(ob)
    gx = (max(c.x for c in band) + min(c.x for c in band)) / 2
    gy = (max(c.y for c in band) + min(c.y for c in band)) / 2
    extra = {}
    if it.get("grip_at") == "peak":
        z, gy, extra = _butt_grip(ob, widths, L, bins)
        band = [c for c in _coords(ob) if abs(c.z - z) < L / bins] or _coords(ob)
        gx = (max(c.x for c in band) + min(c.x for c in band)) / 2
    ob.data.transform(Matrix.Translation((-gx, -gy, -z)))
    ob.data.update()
    return dict({"grip_end": end, "grip_fraction": round(frac, 3), "length_m": round(L, 3),
                 "widths_cm": [round(w * 100, 1) for w in widths]}, **extra)


def _butt_grip(ob, widths, L, bins):
    """A pistol (`grip_at: peak`), butt end at z = 0, barrel up +Z: turns the butt to +Y
    (`held.pistol`: the butt down and back from the grip, the barrel in front of it) and
    returns the wrist (z, y) and stats. The barrel axis is the middle of the muzzle
    quarter's cross-section; the wrist is the middle of the wide stretch (width at least
    3/4 of the peak), a quarter of the way from the barrel axis to the butt's far side."""
    from mathutils import Matrix

    cs = _coords(ob)
    muz = [c for c in cs if c.z > 0.8 * L] or cs
    yb = (max(c.y for c in muz) + min(c.y for c in muz)) / 2
    wide = [k for k, w in enumerate(widths) if w >= 0.75 * max(widths)]
    z = (min(wide) + max(wide) + 1) / 2 / bins * L
    band = [c for c in cs if abs(c.z - z) < 0.15 * L] or cs
    lo_y, hi_y = min(c.y for c in band), max(c.y for c in band)
    if (yb - lo_y) > (hi_y - yb):  # the butt hangs to -Y: turn it over to +Y
        ob.data.transform(Matrix.Rotation(math.pi, 4, "Z"))
        yb, lo_y, hi_y = -yb, -hi_y, -lo_y
    far = hi_y
    gy = yb + 0.25 * (far - yb)
    return z, gy, {"butt_drop_m": round(far - yb, 3), "wrist_z_m": round(z, 3)}


def head_of(flesh_mesh_obj, head_center=None):
    """Nose, crown and the scalp axis of a world-space body or head mesh: nose = the most
    forward vertex of the head region (within 0.13 m of `head_center` in x/y, above its z
    minus 0.17 m), crown = the highest."""
    from mathutils import Vector

    vs = [v.co for v in flesh_mesh_obj.data.vertices]
    if head_center is not None:
        hc = head_center
        vs = [v for v in vs if abs(v.x - hc.x) < 0.13 and abs(v.y - hc.y) < 0.16 and v.z > hc.z - 0.17]
    nose = min(vs, key=lambda v: v.y)
    crown = max(vs, key=lambda v: v.z)
    return {"nose": Vector(nose), "crown": Vector(crown), "axis": (nose.x, nose.y + 0.095)}


def _rings(scalp, axis, z_lo, z_hi, n=48, dz=0.004):
    from mathutils import Vector

    t = _bvh(scalp)
    out = []
    z = z_lo
    while z <= z_hi + 1e-9:
        r = []
        for k in range(n):
            a = 2 * math.pi * k / n
            d = Vector((math.cos(a), math.sin(a), 0))
            h = t.ray_cast(Vector((axis[0], axis[1], z)) + d * 0.5, -d, 0.5)
            r.append(0.5 - h[3] if h[0] else None)
        if all(x is not None for x in r) and min(r) > 0.012:
            out.append((z, r))
        z += dz
    return out


def _cutter(scalp, axis, band_z, top_z, n=48):
    """Closed loft around the head (scalp radius + EASE from the band up, a straight tube
    below so the opening goes through, a cap over the crown)."""
    import bmesh
    import bpy

    rings = _rings(scalp, axis, band_z, top_z, n)
    if not rings:
        raise RuntimeError("no scalp rings: the head is not under the band plane")
    band_r = rings[0][1]
    bm = bmesh.new()
    loops = []
    for z, r in [(band_z - 0.25, band_r)] + rings:
        loops.append([bm.verts.new((axis[0] + (r[k] + EASE) * math.cos(2 * math.pi * k / n),
                                    axis[1] + (r[k] + EASE) * math.sin(2 * math.pi * k / n), z)) for k in range(n)])
    for a, b in zip(loops, loops[1:]):
        for k in range(n):
            bm.faces.new((a[k], a[(k + 1) % n], b[(k + 1) % n], b[k]))
    top = bm.verts.new((axis[0], axis[1], rings[-1][0] + 0.003 + EASE))
    bot = bm.verts.new((axis[0], axis[1], band_z - 0.25))
    for k in range(n):
        bm.faces.new((loops[-1][k], loops[-1][(k + 1) % n], top))
        bm.faces.new((loops[0][(k + 1) % n], loops[0][k], bot))
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    me = bpy.data.meshes.new("genfit.cutter")
    bm.to_mesh(me)
    bm.free()
    ob = bpy.data.objects.new("genfit.cutter", me)
    bpy.context.scene.collection.objects.link(ob)
    ob.hide_render = True
    circ = sum(math.dist(((band_r[k] + EASE) * math.cos(2 * math.pi * k / n), (band_r[k] + EASE) * math.sin(2 * math.pi * k / n)),
                         ((band_r[k - 1] + EASE) * math.cos(2 * math.pi * (k - 1) / n),
                          (band_r[k - 1] + EASE) * math.sin(2 * math.pi * (k - 1) / n))) for k in range(n))
    return ob, circ


def seat_on_head(src, it, scalp, head):
    """Moves headgear `src` (opening at z = 0, centred) onto the head: band plane `band`
    above the eye line, raised until the crown clears the scalp. Returns stats."""
    from mathutils import Matrix, Vector

    eye_z = head["nose"].z + 0.035
    band_z = eye_z + it.get("band", BAND)
    cs = _coords(src)
    core = [c for c in cs if math.hypot(c.x, c.y) < 0.07]
    open_z = min(c.z for c in core) if core else 0.0
    top = max(c.z for c in cs)
    dz = band_z - open_z
    lift = max(0.0, (head["crown"].z + 0.006 + EASE) - (dz + top - 0.004))
    dz += lift
    src.data.transform(Matrix.Translation(Vector((head["axis"][0], head["axis"][1], dz))))
    src.data.update()
    return {"band_z": round(band_z, 3), "lift_m": round(lift, 4), "top_z": round(dz + top, 3)}


def _outward(ob):
    """Turns faces outward on a decimated generated prop: after the collapse stretches of
    Pixal3D's soup faced inward (dark under Lambert). Normals are made consistent per island,
    then each face is tested by a ray along its normal and one against it: a face whose front
    is enclosed and whose back is open faces inward. An island where most faces do is flipped
    whole, then single faces. Returns {inward_before, inward_after} as shares of the faces."""
    import bmesh
    from mathutils.bvhtree import BVHTree

    bm = bmesh.new()
    bm.from_mesh(ob.data)
    bm.faces.ensure_lookup_table()
    size = max(_ext(ob)) or 1.0

    def inward():
        tree = BVHTree.FromBMesh(bm)
        eps, far = 1e-4 * size, 4.0 * size
        out = set()
        for f in bm.faces:
            c, n = f.calc_center_median(), f.normal
            if n.length < 0.5:
                continue
            front = tree.ray_cast(c + n * eps, n, far)[0] is not None
            back = tree.ray_cast(c - n * eps, -n, far)[0] is not None
            if front and not back:
                out.add(f.index)
        return out

    before = inward()
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    bm.faces.ensure_lookup_table()
    bad = inward()
    for isl in _components(bm):
        if sum(f.index in bad for f in isl) > len(isl) / 2:
            bmesh.ops.reverse_faces(bm, faces=isl)
    bm.faces.ensure_lookup_table()
    bad = inward()
    bmesh.ops.reverse_faces(bm, faces=[bm.faces[i] for i in sorted(bad)])
    bm.faces.ensure_lookup_table()
    after = inward()
    n = max(1, len(bm.faces))
    bm.to_mesh(ob.data)
    bm.free()
    ob.data.update()
    return {"inward_before": round(len(before) / n, 3), "inward_after": round(len(after) / n, 3)}


def _apply_decimate(ob, tris):
    import bpy

    bpy.context.view_layer.objects.active = ob
    if _tris(ob.data) > 2 * tris:
        # flat stretches first (a blade's flats, a box's sides): merging coplanar faces costs
        # no shape, so the collapse spends its budget on the outline and thin edges
        m = ob.modifiers.new("flat", "DECIMATE")
        m.decimate_type = "DISSOLVE"
        m.angle_limit = math.radians(1)
        bpy.ops.object.modifier_apply(modifier=m.name)
        m = ob.modifiers.new("tri", "TRIANGULATE")
        bpy.ops.object.modifier_apply(modifier=m.name)
    last = None
    stalls = 0
    steps = []
    for _ in range(24):
        n = _tris(ob.data)
        steps.append(n)
        if n <= tris * 1.05:
            break
        m = ob.modifiers.new("dec", "DECIMATE")
        if last is not None and n > 0.97 * last:
            # the collapse stalled (open borders, fins): dissolve near-planar regions, then go
            # on; each further stall dissolves a wider angle (3, 6, 10, 15 degrees: stone
            # rubble stalls at twice its budget at 3)
            m.decimate_type = "DISSOLVE"
            m.angle_limit = math.radians((3, 6, 10, 15)[min(stalls, 3)])
            stalls += 1
            bpy.ops.object.modifier_apply(modifier=m.name)
            if stalls > 1:
                # still stuck: open seams of the soup block the collapse; weld them shut
                # (0.5, 1, 1.5 % of the size)
                import bmesh

                bm = bmesh.new()
                bm.from_mesh(ob.data)
                bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=0.005 * (stalls - 1) * max(_ext(ob)))
                bm.to_mesh(ob.data)
                bm.free()
            m = ob.modifiers.new("tri", "TRIANGULATE")
            bpy.ops.object.modifier_apply(modifier=m.name)
            last = None
            continue
        m.ratio = max(tris / n, 0.25)
        m.use_collapse_triangulate = True
        bpy.ops.object.modifier_apply(modifier=m.name)
        last = n
    return steps


def _bake(src, low, img, kind, reach, margin=4):
    import bpy

    sc = bpy.context.scene
    sc.render.engine = "CYCLES"
    sc.cycles.device = "CPU"
    sc.cycles.samples = 1
    nt = low.data.materials[0].node_tree
    n = nt.nodes["genfit.bake"]
    n.image = img
    nt.nodes.active = n
    src.hide_render = low.hide_render = False
    bpy.ops.object.select_all(action="DESELECT")
    src.select_set(True)
    low.select_set(True)
    bpy.context.view_layer.objects.active = low
    kw = dict(use_selected_to_active=True, cage_extrusion=reach, max_ray_distance=3 * reach, margin=margin)
    if kind == "EMIT":
        bpy.ops.object.bake(type="EMIT", **kw)
    else:
        bpy.ops.object.bake(type="NORMAL", normal_space="TANGENT", **kw)


def _emissive(src):
    """Turns the source's materials into pure emission of their base-colour chain. A
    DIFFUSE colour bake multiplies by (1 - metallic): Pixal3D's metal-roughness texture made
    helmets almost black. Emission returns the albedo as stored (sRGB in, sRGB out)."""
    for m in src.data.materials:
        if not (m and m.node_tree):
            continue
        nt = m.node_tree
        bsdf = next((x for x in nt.nodes if x.type == "BSDF_PRINCIPLED"), None)
        out = next((x for x in nt.nodes if x.type == "OUTPUT_MATERIAL"), None)
        if bsdf is None or out is None:
            continue
        em = nt.nodes.new("ShaderNodeEmission")
        em.inputs["Strength"].default_value = 1.0
        bc = bsdf.inputs["Base Color"]
        if bc.is_linked:
            nt.links.new(bc.links[0].from_socket, em.inputs["Color"])
        else:
            em.inputs["Color"].default_value = bc.default_value
        nt.links.new(em.outputs["Emission"], out.inputs["Surface"])


def uv_mask(tris_uv, res):
    """Boolean res x res mask (row = v) of the texels whose centres lie in any UV triangle
    (`tris_uv`: list of three (u, v) pairs)."""
    import numpy as np

    m = np.zeros((res, res), bool)
    for (a, b, c) in tris_uv:
        xs = [a[0] * res - 0.5, b[0] * res - 0.5, c[0] * res - 0.5]
        ys = [a[1] * res - 0.5, b[1] * res - 0.5, c[1] * res - 0.5]
        x0, x1 = max(0, int(math.floor(min(xs)))), min(res - 1, int(math.ceil(max(xs))))
        y0, y1 = max(0, int(math.floor(min(ys)))), min(res - 1, int(math.ceil(max(ys))))
        if x1 < x0 or y1 < y0:
            continue
        gx, gy = np.meshgrid(np.arange(x0, x1 + 1), np.arange(y0, y1 + 1))
        d = (ys[1] - ys[2]) * (xs[0] - xs[2]) + (xs[2] - xs[1]) * (ys[0] - ys[2])
        if abs(d) < 1e-12:
            continue
        l1 = ((ys[1] - ys[2]) * (gx - xs[2]) + (xs[2] - xs[1]) * (gy - ys[2])) / d
        l2 = ((ys[2] - ys[0]) * (gx - xs[2]) + (xs[0] - xs[2]) * (gy - ys[2])) / d
        e = -1e-6
        m[y0:y1 + 1, x0:x1 + 1] |= (l1 >= e) & (l2 >= e) & (1 - l1 - l2 >= e)
    return m


def dilate_fill(rgb, known, steps=64):
    """Fills texels not `known` (bool h x w) of `rgb` (h x w x 3) with the mean of their
    known 8-neighbours, ring by ring, until all are known or `steps` rings are done.
    Returns the filled array and the known mask after."""
    import numpy as np

    rgb = rgb.copy()
    known = known.copy()
    for _ in range(steps):
        if known.all():
            break
        acc = np.zeros_like(rgb)
        cnt = np.zeros(known.shape)
        for dy in (-1, 0, 1):
            for dx in (-1, 0, 1):
                if dy == 0 and dx == 0:
                    continue
                k = np.roll(np.roll(known, dy, 0), dx, 1)
                acc += np.roll(np.roll(rgb, dy, 0), dx, 1) * k[..., None]
                cnt += k
        new = (~known) & (cnt > 0)
        if not new.any():
            break
        rgb[new] = acc[new] / cnt[new][:, None]
        known = known | new
    return rgb, known


def bake(src, low, name, tex, reach):
    """Smart-UV on `low`, bakes base colour (emission of the source albedo) and normals from
    `src`, and binds both images (packed) in one Principled material: colour → Base Color,
    normal → Normal Map. The colour bake has no margin into a transparent image, so texels
    the rays missed stay transparent: `unbaked` is their share of the texels inside the UV
    islands. Then misses and the margins are filled from baked neighbours (dilation)."""
    import bpy
    import numpy as np

    bpy.ops.object.select_all(action="DESELECT")
    low.select_set(True)
    bpy.context.view_layer.objects.active = low
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.uv.smart_project(angle_limit=math.radians(60), island_margin=0.004)
    bpy.ops.object.mode_set(mode="OBJECT")
    for p in low.data.polygons:
        p.use_smooth = True
    mat = bpy.data.materials.new(name + ".baked")
    nt = mat.node_tree
    bake_node = nt.nodes.new("ShaderNodeTexImage")
    bake_node.name = "genfit.bake"
    low.data.materials.clear()
    low.data.materials.append(mat)
    _emissive(src)
    col = bpy.data.images.new(name + ".col", tex, tex, alpha=True)
    col.generated_color = (0.0, 0.0, 0.0, 0.0)
    nrm = bpy.data.images.new(name + ".nrm", tex, tex)
    nrm.generated_color = (0.5, 0.5, 1.0, 1.0)
    nrm.colorspace_settings.name = "Non-Color"
    _bake(src, low, col, "EMIT", reach, margin=0)
    _bake(src, low, nrm, "NORMAL", reach)
    px = np.empty(tex * tex * 4, np.float32)
    col.pixels.foreach_get(px)
    px = px.reshape(tex, tex, 4)
    known = px[..., 3] > 0.5
    uvl = low.data.uv_layers.active.data
    tris = [tuple(tuple(uvl[li].uv) for li in p.loop_indices[:3]) for p in low.data.polygons if p.loop_total == 3]
    tris += [(tuple(uvl[p.loop_indices[0]].uv), tuple(uvl[p.loop_indices[k]].uv), tuple(uvl[p.loop_indices[k + 1]].uv))
             for p in low.data.polygons if p.loop_total > 3 for k in range(1, p.loop_total - 1)]
    mask = uv_mask(tris, tex)
    unbaked = float((mask & ~known).sum()) / max(1, int(mask.sum()))
    fill = tuple(float(np.median(px[..., i][known])) if known.any() else 0.5 for i in range(3)) + (1.0,)
    rgb, k2 = dilate_fill(px[..., :3], known)
    rgb[~k2] = fill[:3]
    px[..., :3] = rgb
    px[..., 3] = 1.0
    col.pixels.foreach_set(px.ravel())
    col.update()
    bsdf = nt.nodes["Principled BSDF"]
    bsdf.inputs["Roughness"].default_value = 0.85
    bake_node.image = col
    nt.links.new(bake_node.outputs["Color"], bsdf.inputs["Base Color"])
    nt_n = nt.nodes.new("ShaderNodeTexImage")
    nt_n.image = nrm
    nm = nt.nodes.new("ShaderNodeNormalMap")
    nt.links.new(nt_n.outputs["Color"], nm.inputs["Color"])
    nt.links.new(nm.outputs["Normal"], bsdf.inputs["Normal"])
    for im in (col, nrm):
        im.pack()
    return mat, col, nrm, round(unbaked, 4), fill


def _deviation(src, low, cutter=None, n=6000):
    """Two-way vertex-to-surface distances (headgear: outside the cutter loft only)."""
    ct = _bvh(cutter) if cutter is not None else None

    def outside(p):
        if ct is None:
            return True
        h = ct.find_nearest(p)
        return h[0] is None or (h[3] > 0.004 and (p - h[0]).dot(h[1]) > 0)

    d = []
    for a, b in ((src, low), (low, src)):
        t = _bvh(b)
        vs = a.data.vertices
        step = max(1, len(vs) // n)
        d += [t.find_nearest(vs[i].co)[3] for i in range(0, len(vs), step) if outside(vs[i].co)]
    return summary(d)


def fit(rel, head=None, socket=None, scalp=None, name=None, keep_source=False, row=None):
    """Makes generated item `rel` (path below the generated sources) game-ready; returns
    (object, stats). Headgear with `scalp` (a world-space mesh object of the head or body),
    `head` (`head_of`) and `socket` (world position of `socket.hat`) is fitted to that head,
    its origin at the socket; without them its origin is the middle of the opening. `row`
    stands in for the `ITEMS` row (map objects made from their sprite: `nature`)."""
    import bmesh
    import bpy
    from mathutils import Matrix

    it = dict(row, name=os.path.splitext(os.path.basename(rel))[0]) if row else item(rel)
    cat = it["category"]
    budget = dict(CATEGORIES[cat], **{k: it[k] for k in ("tris", "tex") if k in it})
    name = name or it["name"]
    st = {"item": it["name"], "category": cat}
    src = load(source_path(rel), name + ".source")
    st["source_triangles"] = _tris(src.data)
    nm0 = _non_manifold(src.data)
    st.update(place(src, it))
    weld = WELD
    nm1 = repair(src, weld)
    st["backdrop_faces_removed"] = drop_backdrop(src)
    if st["backdrop_faces_removed"]:
        # the backdrop sheet skewed the box: orient and scale again without it
        st.update(place(src, it))
    st["source_islands"] = _islands(src.data)
    if cat == "held":
        st.update(held_grip(src, it))
    cutter = None
    if cat == "headgear" and scalp is not None:
        st.update(seat_on_head(src, it, scalp, head))
        cutter, circ = _cutter(scalp, head["axis"], st["band_z"], head["crown"].z - 0.002)
        st["band_circumference_m"] = round(circ, 3)
    # seam-free low copy
    low = src.copy()
    low.data = src.data.copy()
    low.name = name
    bpy.context.scene.collection.objects.link(low)
    low.data.materials.clear()
    for uv in list(low.data.uv_layers):
        low.data.uv_layers.remove(uv)
    bm = bmesh.new()
    bm.from_mesh(low.data)  # already welded by `repair` (welding again would rejoin split fins)
    if cutter is not None:
        ct = _bvh(cutter)
        bm.faces.ensure_lookup_table()
        inside = []
        for f in bm.faces:
            c = f.calc_center_median()
            h = ct.find_nearest(c)
            if h[0] is not None and (c - h[0]).dot(h[1]) < 0 and h[3] > 0.003:
                inside.append(f)
        st["faces_inside_head_removed"] = len(inside)
        bmesh.ops.delete(bm, geom=inside, context="FACES")
        bm.faces.ensure_lookup_table()
        comps = _components(bm)
        if comps:
            bmesh.ops.delete(bm, geom=[f for c in comps[1:] if len(c) < 0.02 * len(comps[0]) for f in c], context="FACES")
        bmesh.ops.delete(bm, geom=[v for v in bm.verts if not v.link_faces], context="VERTS")
    bm.to_mesh(low.data)
    bm.free()
    st["decimate_steps"] = _apply_decimate(low, budget["tris"])
    if cat == "prop":
        st["normals"] = _outward(low)
    st["triangles"] = _tris(low.data)
    st["islands"] = _islands(low.data)
    st["non_manifold"] = {"source": nm0, "repaired": nm1, "final": _non_manifold(low.data), "kinds": _nm_kinds(src.data)}
    st["deviation"] = _deviation(src, low, cutter)
    if cutter is not None:
        ht, ct = _bvh(scalp), _bvh(cutter)

        def inside(p):
            h = ct.find_nearest(p)
            return h[0] is not None and (p - h[0]).dot(h[1]) < 0.002

        inner = [v.co for v in low.data.vertices if v.co.z > st["band_z"] - 0.005 and inside(v.co)]
        gaps = sorted(ht.find_nearest(p)[3] for p in inner)
        st["band_gap_m"] = {"min": round(gaps[0], 4), "median": round(gaps[len(gaps) // 2], 4)} if gaps else None
    lo, hi = _extent(_coords(low))
    size = max(h - l for l, h in zip(lo, hi))
    _mat, col, _nrm, unbaked, fill = bake(src, low, name, budget["tex"], max(0.004, 0.02 * size))
    st["tex_px"] = budget["tex"]
    st["tex_unbaked_frac"] = unbaked
    low["acr_fill_rgb"] = list(fill[:3])
    # origin
    if cat == "headgear" and socket is not None:
        low.data.transform(Matrix.Translation(-socket))
        st["socket_hat"] = [round(c, 3) for c in socket]
    elif cat == "worn":
        lo, hi = _extent(_coords(low))
        low.data.transform(Matrix.Translation((-(lo[0] + hi[0]) / 2, -hi[1], -(lo[2] + hi[2]) / 2)))
    if cat == "held":
        low["acr_grip"] = [0.0, 0.0, 0.0]
        low["acr_length"] = st["length_m"]
    lo, hi = _extent(_coords(low))
    st["size_m"] = [round(h - l, 3) for l, h in zip(lo, hi)]
    low.data.update()
    if not keep_source:
        bpy.data.objects.remove(src, do_unlink=True)
    if cutter is not None:
        bpy.data.objects.remove(cutter, do_unlink=True)
    return low, st


def game_material(ob, layer):
    """Swaps the baked Principled material for a decay material on the baked colour image
    (the renderer reads base colour; the normal map stays out of the unit) and marks the
    object `acr_baked` so the unit's colour and outfit passes keep it."""
    from . import decay

    col = next(n.image for n in ob.data.materials[0].node_tree.nodes
               if n.type == "TEX_IMAGE" and n.image and n.image.name.endswith(".col"))
    rgb = tuple(ob.get("acr_fill_rgb", (0.5, 0.5, 0.5)))
    m = decay.material(f"{ob.name}.genfit", layer, rgb,
                       image=col, uv_map=ob.data.uv_layers[0].name, interpolation="Linear")
    m["acr_colour_from"] = "generated"
    ob.data.materials.clear()
    ob.data.materials.append(m)
    ob["acr_baked"] = True
    return m


def unit_head(ch):
    """World-space copy of character `ch`'s flesh (current pose) and its head (`head_of`)."""
    import bpy

    dg = bpy.context.evaluated_depsgraph_get()
    me = bpy.data.meshes.new_from_object(ch.flesh.evaluated_get(dg))
    me.transform(ch.flesh.matrix_world)
    ob = bpy.data.objects.new(f"{ch.name}.genfit_scalp", me)
    bpy.context.scene.collection.objects.link(ob)
    ob.hide_render = True
    hc = ch.arm.matrix_world @ ch.arm.pose.bones["head"].tail
    return ob, head_of(ob, hc)


def hat_on(ch, rel, obj_name, socket_world):
    """A generated hat fitted to `ch`'s head, origin at `socket_world`, game material on;
    returns (object, stats). The scalp copy is removed again."""
    import bpy

    scalp, head = unit_head(ch)
    try:
        hat, st = fit(rel, head=head, socket=socket_world, scalp=scalp, name=obj_name)
    finally:
        bpy.data.objects.remove(scalp, do_unlink=True)
    game_material(hat, item(rel)["layer"])
    return hat, st
