"""Animal coats (needs `bpy`, `numpy`): one baked texture atlas per animal.

The renderer lights with Lambert only and reads one base-colour PNG per material, so the
look has to live in the texture. Each part (hide, mane/tail, tack, cloth) gets its own
rectangle of one atlas through a Smart-UV map `atlas`. Cycles bakes, in the rest pose,
the world position, the normal and ambient occlusion of every texel; the colours are then
painted per texel in numpy from those (coat zones, hair-direction streaks, dapples, points,
hooves, eyes; leather, brass, wool) and multiplied by the occlusion so the form reads under
the game's flat light. The functions are deterministic (seeded noise).
"""

import math

import bpy
import numpy as np

SIZE = 1024
# Atlas rectangles (u0, v0, u1, v1). Baked textures do not mind non-square texels.
REGIONS = {
    "hide": (0.0, 0.0, 1.0, 0.70),
    "hair": (0.0, 0.70, 0.34, 1.0),
    "tack": (0.34, 0.70, 0.67, 0.90),
    "bridle": (0.34, 0.90, 0.67, 1.0),
    "cloth": (0.67, 0.70, 1.0, 1.0),
}
AO_DISTANCE = 0.35
AO_SAMPLES = 48


# ----------------------------------------------------------------------------- noise

def _hash(ix, iy, iz, seed):
    h = (ix * 73856093) ^ (iy * 19349663) ^ (iz * 83492791) ^ (seed * 2654435761)
    h = (h ^ (h >> 13)) * 1274126177
    return ((h ^ (h >> 16)) & 0xFFFF).astype(np.float32) / 65535.0


def vnoise(p, seed=0):
    """Smooth value noise 0..1 at points p (N, 3), lattice spacing 1."""
    f = np.floor(p)
    t = p - f
    t = t * t * (3 - 2 * t)
    i = f.astype(np.int64)
    out = np.zeros(len(p), np.float32)
    for dx in (0, 1):
        wx = t[:, 0] if dx else 1 - t[:, 0]
        for dy in (0, 1):
            wy = t[:, 1] if dy else 1 - t[:, 1]
            for dz in (0, 1):
                wz = t[:, 2] if dz else 1 - t[:, 2]
                out += wx * wy * wz * _hash(i[:, 0] + dx, i[:, 1] + dy, i[:, 2] + dz, seed)
    return out


def fbm(p, freq, seed=0, octaves=3):
    """Fractal noise around 0.5; `freq` scalar or per axis (anisotropic: streaks)."""
    freq = np.asarray(freq, np.float32)
    total, amp, norm = np.zeros(len(p), np.float32), 1.0, 0.0
    for o in range(octaves):
        total += amp * vnoise(p * freq * (2.0 ** o), seed + 17 * o)
        norm += amp
        amp *= 0.5
    return total / norm


def smooth(e0, e1, x):
    t = np.clip((x - e0) / (e1 - e0), 0.0, 1.0)
    return t * t * (3 - 2 * t)


def lin(rgb):
    c = np.asarray(rgb, np.float32)
    return np.where(c <= 0.04045, c / 12.92, ((c + 0.055) / 1.055) ** 2.4)


def to_srgb(c):
    c = np.clip(c, 0.0, 1.0)
    return np.where(c <= 0.0031308, c * 12.92, 1.055 * np.power(c, 1 / 2.4) - 0.055)


def mix(a, b, t):
    t = np.asarray(t, np.float32)[..., None] if np.ndim(t) else t
    return a * (1 - t) + b * t


# ----------------------------------------------------------------------------- UVs

def atlas_uv(obj, region, name="atlas", angle=66.0):
    """Replaces the object's UV maps by one Smart-UV map `name` fitted into `region`."""
    me = obj.data
    while len(me.uv_layers):
        me.uv_layers.remove(me.uv_layers[0])
    uv = me.uv_layers.new(name=name)
    me.uv_layers.active = uv
    for o in bpy.context.view_layer.objects:
        o.select_set(False)
    bpy.context.view_layer.objects.active = obj
    obj.select_set(True)
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.uv.smart_project(angle_limit=math.radians(angle), island_margin=0.006, scale_to_bounds=True)
    bpy.ops.object.mode_set(mode="OBJECT")
    u0, v0, u1, v1 = region
    pad = 2.0 / SIZE
    for d in me.uv_layers[name].data:
        u, v = d.uv
        d.uv = (u0 + pad + u * (u1 - u0 - 2 * pad), v0 + pad + v * (v1 - v0 - 2 * pad))
    obj.select_set(False)
    return uv


# ----------------------------------------------------------------------------- bake

def _bake_material(img, emit=None):
    mat = bpy.data.materials.new("acr_bake")
    nodes, links = mat.node_tree.nodes, mat.node_tree.links
    nodes.clear()
    out = nodes.new("ShaderNodeOutputMaterial")
    if emit is None:
        sh = nodes.new("ShaderNodeBsdfDiffuse")
        sh.inputs["Color"].default_value = (0.8, 0.8, 0.8, 1)
    else:
        # Emission stays positive: position / 8 + 0.5 (within +-4 m), normal / 2 + 0.5.
        geo = nodes.new("ShaderNodeNewGeometry")
        ma = nodes.new("ShaderNodeVectorMath")
        ma.operation = "MULTIPLY_ADD"
        k = 0.125 if emit == "Position" else 0.5
        ma.inputs[1].default_value = (k, k, k)
        ma.inputs[2].default_value = (0.5, 0.5, 0.5)
        links.new(geo.outputs[emit], ma.inputs[0])
        sh = nodes.new("ShaderNodeEmission")
        links.new(ma.outputs[0], sh.inputs["Color"])
    links.new(sh.outputs[0], out.inputs["Surface"])
    tex = nodes.new("ShaderNodeTexImage")
    tex.image = img
    nodes.active = tex
    return mat


def bake(objs, size=SIZE):
    """Bakes world position, normal and ambient occlusion of `objs` (their rest pose:
    armature modifiers off, every other object hidden) into (size, size, 3) arrays, plus a
    coverage mask. Returns (pos, nrm, ao, covered)."""
    s = bpy.context.scene
    keep = {"engine": s.render.engine}
    hidden = []
    for o in s.objects:
        if o not in objs and not o.hide_render:
            o.hide_render = True
            hidden.append(o)
    mods = []
    for o in objs:
        for m in o.modifiers:
            if m.type == "ARMATURE" and m.show_render:
                m.show_render = m.show_viewport = False
                mods.append(m)
    saved = {o: list(o.data.materials) for o in objs}
    s.render.engine = "CYCLES"
    s.cycles.device = "CPU"
    s.cycles.samples = 1
    if s.world is None:
        s.world = bpy.data.worlds.new("acr_bake")
    s.world.light_settings.distance = AO_DISTANCE
    bpy.context.view_layer.update()

    out = {}
    for key, emit, samples in (("pos", "Position", 1), ("nrm", "Normal", 1), ("ao", None, AO_SAMPLES)):
        img = bpy.data.images.new(f"acr_bake_{key}", size, size, float_buffer=True, alpha=True)
        img.generated_color = (0, 0, 0, 0)
        mat = _bake_material(img, emit)
        for o in objs:
            o.data.materials.clear()
            o.data.materials.append(mat)
        for o in s.objects:
            o.select_set(o in objs)
        bpy.context.view_layer.objects.active = objs[0]
        s.cycles.samples = samples
        s.render.bake.margin = 6
        s.render.bake.use_clear = True
        bpy.ops.object.bake(type="EMIT" if emit else "AO", margin=6, use_clear=True)
        px = np.empty(size * size * 4, np.float32)
        img.pixels.foreach_get(px)
        out[key] = px.reshape(size, size, 4)
        bpy.data.images.remove(img)
        bpy.data.materials.remove(mat)
    for o, ms in saved.items():
        o.data.materials.clear()
        for m in ms:
            o.data.materials.append(m)
    for m in mods:
        m.show_render = m.show_viewport = True
    for o in hidden:
        o.hide_render = False
    s.render.engine = keep["engine"]
    # Covered texels (margins included): the cleared image is black.
    covered = out["pos"][..., :3].sum(-1) > 0.05
    pos = (out["pos"][..., :3] - 0.5) * 8.0
    nrm = (out["nrm"][..., :3] - 0.5) * 2.0
    return pos, nrm, out["ao"][..., 0], covered


# ----------------------------------------------------------------------------- painting

def region_mask(name, size=SIZE):
    u0, v0, u1, v1 = REGIONS[name]
    v, u = np.mgrid[0:size, 0:size]
    u = (u + 0.5) / size
    v = (v + 0.5) / size  # image rows run bottom-up in Blender
    return (u >= u0) & (u < u1) & (v >= v0) & (v < v1)


# Bay coat (sRGB): body, darker top line, lighter belly and flank undersides, mealy muzzle,
# black points, hooves.
BAY = {
    "body": (0.40, 0.205, 0.09), "top": (0.30, 0.15, 0.065), "belly": (0.60, 0.40, 0.23),
    "muzzle": (0.55, 0.40, 0.29), "nose": (0.10, 0.08, 0.07), "points": (0.065, 0.05, 0.045),
    "hoof": (0.20, 0.18, 0.16), "eye": (0.025, 0.02, 0.018), "mane": (0.07, 0.05, 0.04),
    "mane_tips": (0.20, 0.12, 0.07),
}


def paint_hide(p, n, L, seed=1, coat=BAY):
    """Coat colour (linear RGB, (N, 3)) at rest positions p and normals n. `L` holds the
    landmarks: knee (z of the black stockings), hoof (z), eyes [(x, y, z)], muzzle (y of the
    muzzle tip), poll (y), ears [(x, y, z)]."""
    x, y, z = p[:, 0], p[:, 1], p[:, 2]
    c = {k: lin(v) for k, v in coat.items()}
    col = np.broadcast_to(c["body"], p.shape).copy()
    # Large-scale tone: darker along the top line and rump, lighter towards the belly.
    up = n[:, 2]
    col = mix(col, c["top"], 0.55 * smooth(0.3, 0.95, up) * smooth(1.0, 1.3, z))
    under = smooth(-0.15, -0.75, up) * smooth(L["knee"] + 0.25, L["knee"] + 0.45, z)
    flank_in = smooth(0.06, 0.0, np.abs(x)) * smooth(L["knee"] + 0.1, L["knee"] + 0.4, z)  # inner thighs
    col = mix(col, c["belly"], np.clip(0.75 * under + 0.5 * flank_in, 0, 1))
    # Hair direction: on the body hair runs backwards (+Y) and down, on the legs down;
    # streaks are noise stretched along the hair.
    legness = smooth(L["knee"] + 0.45, L["knee"] + 0.15, z)
    body_streak = fbm(p, (110.0, 9.0, 55.0), seed)
    leg_streak = fbm(p, (110.0, 110.0, 9.0), seed + 5)
    streak = mix(body_streak[:, None], leg_streak[:, None], legness)[:, 0]
    col *= (0.80 + 0.40 * streak)[:, None]
    # Broad mottling and faint dapples on the barrel and quarters.
    mott = fbm(p, 3.0, seed + 11)
    col *= (0.90 + 0.20 * mott)[:, None]
    dap = fbm(p, 9.0, seed + 23, octaves=2)
    dapple = smooth(0.60, 0.70, dap) * smooth(0.95, 1.25, z) * (1 - legness)
    col = mix(col, col * 1.22, 0.6 * dapple)
    # Black points: lower legs (wavy boundary), ear rims, muzzle edge.
    wav = 0.06 * (fbm(p, 6.0, seed + 31) - 0.5)
    points = smooth(L["knee"] + 0.03, L["knee"] - 0.09, z + wav)
    for ex, ey, ez in L["ears"]:
        d = np.sqrt((x - ex) ** 2 + (y - ey) ** 2 + (z - ez) ** 2)
        points = np.maximum(points, smooth(0.07, 0.03, d) * smooth(ez - 0.06, ez + 0.02, z))
    col = mix(col, c["points"] * (0.85 + 0.3 * streak)[:, None], points)
    # Muzzle: lighter "mealy" ring, dark nose tip and nostrils.
    mz = L["muzzle"]
    muzzle = smooth(mz + 0.13, mz + 0.06, y)
    col = mix(col, c["muzzle"], 0.65 * muzzle * (1 - points))
    nose = smooth(mz + 0.04, mz + 0.012, y)
    col = mix(col, c["nose"], 0.85 * nose)
    # Hooves: dark horn with vertical growth lines, a lighter coronet band above.
    hoofs = smooth(L["hoof"] + 0.01, L["hoof"] - 0.01, z)
    lines = fbm(p, (90.0, 90.0, 3.0), seed + 41)
    col = mix(col, c["hoof"] * (0.75 + 0.5 * lines)[:, None], hoofs)
    # Eyes: dark almond with a rim and a small catchlight.
    for ex, ey, ez in L["eyes"]:
        d = np.sqrt((x - ex) ** 2 + ((y - ey) * 0.75) ** 2 + ((z - ez) * 1.4) ** 2)
        col = mix(col, c["points"], smooth(0.040, 0.030, d))
        col = mix(col, c["eye"], smooth(0.026, 0.020, d))
        hl = np.sqrt((x - ex) ** 2 + (y - ey + 0.006) ** 2 + (z - ez - 0.007) ** 2)
        col = mix(col, lin((0.75, 0.75, 0.72)), 0.8 * smooth(0.006, 0.003, hl))
    return col


def paint_hair(p, n, seed=2, coat=BAY):
    """Mane and tail: near-black strands (noise stretched along the fall), sun-bleached tips."""
    c = {k: lin(v) for k, v in coat.items()}
    strands = fbm(p, (160.0, 40.0, 6.0), seed, octaves=2)
    fine = fbm(p, (420.0, 120.0, 12.0), seed + 3, octaves=1)
    col = mix(np.broadcast_to(c["mane"], p.shape), c["mane_tips"], 0.35 * smooth(0.55, 0.85, strands))
    return col * (0.65 + 0.7 * (0.6 * strands + 0.4 * fine))[:, None]


def paint_tack(p, n, L, seed=3):
    """Leather (saddle, bridle) with grain, brass on the bit, buckles and stirrups."""
    leather = lin((0.27, 0.15, 0.07))
    grain = fbm(p, 60.0, seed, octaves=2)
    wear = fbm(p, 6.0, seed + 7)
    col = leather * (0.75 + 0.4 * grain)[:, None]
    col = mix(col, lin((0.40, 0.25, 0.12)), 0.5 * smooth(0.6, 0.85, wear))
    brass = np.zeros(len(p), bool)
    for bx, by, bz, r in L.get("brass", []):
        brass |= np.sqrt((p[:, 0] - bx) ** 2 + (p[:, 1] - by) ** 2 + (p[:, 2] - bz) ** 2) < r
    tone = fbm(p, 25.0, seed + 13)
    col[brass] = (lin((0.72, 0.56, 0.26)) * (0.8 + 0.4 * tone[brass, None]))
    return col


def paint_cloth(p, n, L, rgb, lace=(0.86, 0.72, 0.32), seed=4):
    """Wool saddlecloth in the nation colour `rgb` with a lace border along its edges
    (`L["cloth_edges"]`: z of the lower edge, y of the front and back edges)."""
    base = lin(rgb)
    weave = fbm(p, (250.0, 250.0, 250.0), seed, octaves=1)
    felt = fbm(p, 12.0, seed + 3)
    col = base * (0.85 + 0.15 * weave + 0.15 * felt)[:, None]
    z0, yf, yb = L["cloth_edges"]
    d = np.minimum(np.minimum(p[:, 2] - z0, p[:, 1] - yf), yb - p[:, 1])
    band = smooth(0.025, 0.018, d) * smooth(0.0, 0.006, d)
    return mix(col, lin(lace) * (0.85 + 0.2 * weave)[:, None], band)


def atlas(name, parts, L, cloth_rgb, size=SIZE):
    """Bakes and paints the atlas for `parts` = {region: [objects]} (UVs already in their
    regions). Returns the packed image (sRGB PNG)."""
    objs = [o for os_ in parts.values() for o in os_]
    pos, nrm, ao, covered = bake(objs, size)
    flat_p, flat_n = pos.reshape(-1, 3), nrm.reshape(-1, 3)
    out = np.zeros((size * size, 3), np.float32)
    out[:] = lin(BAY["body"])
    for region in parts:
        m = (region_mask(region, size) & covered).reshape(-1)
        p, n = flat_p[m], flat_n[m]
        if region == "hide":
            out[m] = paint_hide(p, n, L)
        elif region == "hair":
            out[m] = paint_hair(p, n)
        elif region in ("tack", "bridle"):
            out[m] = paint_tack(p, n, L)
        else:
            out[m] = paint_cloth(p, n, L, cloth_rgb)
    occ = np.clip(ao.reshape(-1), 0.0, 1.0)
    out *= (0.42 + 0.58 * occ ** 0.8)[:, None]
    px = np.ones((size * size, 4), np.float32)
    px[:, :3] = to_srgb(out)
    img = bpy.data.images.new(name, size, size, alpha=False)
    img.pixels.foreach_set(px.ravel())
    img.file_format = "PNG"
    img.pack()
    return img
