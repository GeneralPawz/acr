"""High-detail gear masters (m2-hd-musket): the 18th-century British line soldier's kit,
modelled from reference dimensions, masters first, LODs derived.

- **Musket:** Short Land Pattern ("Brown Bess"): 42" (1.067 m) barrel, 1.47 m overall,
  0.75 calibre bore, walnut stock lofted from `STOCK` stations (butt, comb, wrist, lock
  panel, fore-end over the ramrod), brass butt plate with its tang, trigger guard with its
  finials, side plate, thumb escutcheon, three ramrod pipes, the tail pipe and the nose cap;
  steel barrel with its breech mouldings and an open bore, barrel tang, bayonet stud, lock
  (plate, cock with its jaws, top-jaw screw and flint, frizzen with its pan cover, pan,
  frizzen spring, tumbler screw), trigger, ramrod with its button head, two sling swivels.
- **Bayonet** (socket bayonet, 0.43 m) and its **scabbard**; the **kit**: cartridge box with
  flap and badge, two buff crossbelts with buckles and the belt plate, hanger with its
  scabbard, wooden "cheese-box" canteen, painted-canvas knapsack with flap and straps.

Every item is one mesh in its own frame. The musket keeps `gear.musket`'s frame: +Z along
the barrel, origin on the centre line of the wrist of the stock (the right hand's grip),
the top (barrel, comb) towards +Y, the lock on -X (the gun's right with +Y up and +Z
forward); `socket.muzzle` an empty child at the muzzle. `grip_geometry()` gives the wrist,
the fore-end and the muzzle as data for the grip solver (no `bpy` needed).

Materials are procedural (object coordinates: walnut grain along the stock, brass and
steel with light wear, leathers, canvas), baked per item into its own atlas (smart UV
project): base colour with baked ambient occlusion, one metallic-roughness image (glTF:
G roughness, B metallic) and a tangent normal map from the procedural relief. LODs:
collapse decimation of the master to `LOD_RATIOS`, the master's atlas UVs kept, the
albedo and metallic-roughness scaled down per level and the normal baked from the master
into each level (selected to active), as `extras.acr_lod` = 1..3 / `acr_lod_of` (the
authored-level format of `hdbody`). The crowd level of the kit may later merge into the
body's crowd level (one draw); not done here.

stats (`ctx.stats["gear_hd"]`): per item `tris`, `lod_tris`, `tex_px`, `size_m`; `grip`.
"""

import json
import math

# ----------------------------------------------------------------------------- data (no bpy)

INCH = 0.0254
LENGTH = 1.47  # overall, butt plate to muzzle
BUTT_Z = -0.30  # butt plate heel, item frame (origin on the wrist)
MUZZLE_Z = BUTT_Z + LENGTH
BARREL_LEN = 42 * INCH
BREECH_Z = MUZZLE_Z - BARREL_LEN
BARREL_Y = 0.024  # bore axis above the wrist's centre line
BARREL_R = (0.0162, 0.0114)  # outer radius at the breech and the muzzle
BORE_R = 0.75 * INCH / 2  # 0.75 calibre
ROD_Y = BARREL_Y - 0.0245  # ramrod axis under the barrel
ROD_R = 0.0048
STOCK_END_Z = MUZZLE_Z - 0.10  # the stock stops short of the muzzle for the bayonet
LOCK_SIDE = -1.0  # the lock on -X
BAYONET = {"socket": 0.075, "blade": 0.36, "socket_r": 0.0128}

# Stock stations: z, top y, bottom y, half width, squareness (superellipse exponent).
STOCK = (
    (-0.300, 0.004, -0.122, 0.0205, 4.0),
    (-0.296, 0.006, -0.121, 0.0222, 3.6),
    (-0.270, 0.007, -0.113, 0.0230, 3.2),
    (-0.220, 0.009, -0.099, 0.0228, 3.0),
    (-0.160, 0.012, -0.081, 0.0222, 2.8),
    (-0.100, 0.0148, -0.058, 0.0205, 2.6),
    (-0.060, 0.0160, -0.038, 0.0190, 2.4),
    (-0.030, 0.0166, -0.024, 0.0178, 2.2),
    (0.000, 0.0170, -0.0190, 0.0172, 2.1),
    (0.030, 0.0190, -0.0192, 0.0178, 2.2),
    (0.060, 0.0260, -0.0230, 0.0196, 2.6),
    (0.090, 0.0350, -0.0280, 0.0216, 3.0),
    (0.120, 0.0370, -0.0300, 0.0222, 3.2),
    (0.200, 0.0370, -0.0285, 0.0222, 3.2),
    (0.250, 0.0350, -0.0220, 0.0206, 3.0),
    (0.300, 0.0320, -0.0110, 0.0182, 2.6),
    (0.360, 0.0305, -0.0030, 0.0170, 2.4),
    (0.600, 0.0300, -0.0025, 0.0162, 2.4),
    (0.900, 0.0292, -0.0022, 0.0152, 2.4),
    (STOCK_END_Z, 0.0285, -0.0020, 0.0144, 2.4),
)
PIPES = (0.62, 0.84, 1.00)  # ramrod pipe centres (the tail pipe at 0.36..0.45)
LOD_RATIOS = (0.30, 0.10, 0.03)
LOD_TEX = (1024, 512, 256)
MIN_LOD_TRIS = 60


def stock_at(z):
    """(top y, bottom y, half width, squareness) of the stock at `z`, linear between the
    `STOCK` stations (clamped at the ends)."""
    if z <= STOCK[0][0]:
        return STOCK[0][1:]
    for a, b in zip(STOCK, STOCK[1:]):
        if z <= b[0]:
            t = (z - a[0]) / (b[0] - a[0])
            return tuple(a[i] + (b[i] - a[i]) * t for i in range(1, 5))
    return STOCK[-1][1:]


def barrel_r(z):
    t = min(1.0, max(0.0, (z - BREECH_Z) / BARREL_LEN))
    return BARREL_R[0] + (BARREL_R[1] - BARREL_R[0]) * t


def _section(z, with_barrel=False):
    top, bot, hw, _p = stock_at(z)
    if with_barrel and z >= BREECH_Z:
        top = max(top, BARREL_Y + barrel_r(z))
    return {"z": round(z, 4), "centre": [0.0, round((top + bot) / 2, 4), round(z, 4)],
            "width": round(2 * hw, 4), "height": round(top - bot, 4)}


def grip_geometry():
    """The musket's grips in its item frame (metres; +Z the barrel, origin on the wrist):
    `wrist` and `fore_end` each with the centre line (two points), the cross-section there
    (width across X, height along Y) and an equivalent diameter (the mean of the two; the
    fore-end's measured over stock and barrel, where the left hand closes); `muzzle`, `butt`
    and the barrel axis. `held.GRIPS["musket"]` holds (0.0, 0.036) and (0.34, 0.040)."""
    def span(z0, z1, barrel):
        a, b, m = _section(z0, barrel), _section(z1, barrel), _section((z0 + z1) / 2, barrel)
        return {"line": [a["centre"], b["centre"]], "width": m["width"], "height": m["height"],
                "diameter": round((m["width"] + m["height"]) / 2, 4), "z": [z0, z1]}

    return {
        "frame": "+Z barrel, +Y top, lock on -X, origin on the wrist centre line",
        "wrist": span(-0.045, 0.040, False),
        "fore_end": span(0.30, 0.42, True),
        "barrel_axis": [[0.0, BARREL_Y, round(BREECH_Z, 4)], [0.0, BARREL_Y, round(MUZZLE_Z, 4)]],
        "muzzle": [0.0, BARREL_Y, round(MUZZLE_Z, 4)],
        "butt": [0.0, round(sum(stock_at(BUTT_Z)[:2]) / 2, 4), BUTT_Z],
        "length": LENGTH,
        "bore_d": round(2 * BORE_R, 4),
    }


def lod_targets(tris, ratios=LOD_RATIOS):
    """Triangle targets of the levels of a master of `tris` triangles."""
    return [max(MIN_LOD_TRIS, int(round(tris * r))) for r in ratios]


# Material presets: base colours (linear; light, dark), metallic, roughness range, texture
# kind, object-space scale, bump strength.
MATS = {
    "walnut": ((0.30, 0.15, 0.065), (0.085, 0.038, 0.017), 0.0, (0.42, 0.62), "grain", (90.0, 90.0, 5.0), 0.25),
    "brass": ((0.80, 0.56, 0.22), (0.42, 0.29, 0.11), 1.0, (0.22, 0.48), "wear", (60.0, 60.0, 60.0), 0.04),
    "steel": ((0.60, 0.61, 0.62), (0.30, 0.30, 0.31), 1.0, (0.26, 0.55), "wear", (70.0, 70.0, 70.0), 0.05),
    "iron": ((0.20, 0.19, 0.18), (0.30, 0.17, 0.09), 0.85, (0.55, 0.85), "wear", (80.0, 80.0, 80.0), 0.10),
    "flint": ((0.20, 0.19, 0.17), (0.05, 0.05, 0.055), 0.0, (0.30, 0.45), "wear", (200.0, 200.0, 200.0), 0.2),
    "leather": ((0.050, 0.044, 0.040), (0.018, 0.016, 0.015), 0.0, (0.40, 0.65), "pores", (260.0, 260.0, 260.0), 0.18),
    "buff": ((0.74, 0.66, 0.48), (0.54, 0.46, 0.32), 0.0, (0.78, 0.95), "pores", (180.0, 180.0, 180.0), 0.12),
    "wood": ((0.42, 0.27, 0.13), (0.20, 0.12, 0.055), 0.0, (0.60, 0.82), "grain_y", (70.0, 6.0, 70.0), 0.2),
    "canvas": ((0.47, 0.38, 0.23), (0.33, 0.26, 0.15), 0.0, (0.82, 0.96), "weave", (420.0, 420.0, 420.0), 0.3),
    "cord": ((0.50, 0.42, 0.28), (0.30, 0.24, 0.15), 0.0, (0.85, 0.95), "pores", (400.0, 400.0, 400.0), 0.3),
    "felt": ((0.040, 0.038, 0.037), (0.012, 0.011, 0.011), 0.0, (0.88, 0.98), "pores", (600.0, 600.0, 600.0), 0.15),
    "tape": ((0.86, 0.85, 0.80), (0.62, 0.60, 0.55), 0.0, (0.80, 0.92), "weave", (900.0, 900.0, 900.0), 0.2),
    "cockade": ((0.030, 0.028, 0.028), (0.008, 0.008, 0.008), 0.0, (0.55, 0.75), "weave", (1400.0, 1400.0, 1400.0), 0.3),
}


# ----------------------------------------------------------------------------- mesh builder

class Builder:
    """A bmesh with a material index per face (`mats`: material keys in slot order)."""

    def __init__(self):
        import bmesh

        self.bm = bmesh.new()
        self.mats = []

    def _m(self, key):
        if key not in self.mats:
            self.mats.append(key)
        return self.mats.index(key)

    def loft(self, rings, mat, caps=(True, True)):
        """Bridges closed rings of 3D points (same count) in order; optional end caps."""
        from mathutils import Vector

        mi = self._m(mat)
        vs = [[self.bm.verts.new(Vector(p)) for p in r] for r in rings]
        n = len(vs[0])
        for a, b in zip(vs, vs[1:]):
            for i in range(n):
                j = (i + 1) % n
                f = self.bm.faces.new((a[i], a[j], b[j], b[i]))
                f.material_index = mi
        if caps[0]:
            self.bm.faces.new(list(reversed(vs[0]))).material_index = mi
        if caps[1]:
            self.bm.faces.new(vs[-1]).material_index = mi
        return vs

    def prism(self, outline, origin, u, v, w, d0, d1, mat):
        """An outline [(a, b)] in the plane origin + a u + b v, extruded from d0 to d1 along w."""
        o, u, v, w = (_v(x) for x in (origin, u, v, w))
        rings = [[o + u * a + v * b + w * d for a, b in outline] for d in (d0, d1)]
        return self.loft(rings, mat)

    def sweep(self, path, section, mat, side=(1.0, 0.0, 0.0), scales=None, caps=(True, True)):
        """Section [(s, n)] (s along `side`, n along the path's normal) swept along `path`."""
        p = [_v(x) for x in path]
        side = _v(side).normalized()
        rings = []
        for i, c in enumerate(p):
            t = (p[min(i + 1, len(p) - 1)] - p[max(i - 1, 0)]).normalized()
            nrm = t.cross(side).normalized()
            b = nrm.cross(t).normalized()
            k = scales[i] if scales else 1.0
            rings.append([c + b * (s * k) + nrm * (n * k) for s, n in section])
        return self.loft(rings, mat, caps)

    def tube(self, z0, z1, radii, mat, centre=(0.0, 0.0), n=24, axis="z", caps=(True, True)):
        """A solid of revolution: `radii` [(t 0..1, r)] between z0 and z1 along `axis`."""
        rings = []
        for t, r in radii:
            z = z0 + (z1 - z0) * t
            ring = []
            for i in range(n):
                a = 2 * math.pi * i / n
                x, y = centre[0] + r * math.cos(a), centre[1] + r * math.sin(a)
                ring.append({"z": (x, y, z), "y": (x, z, y), "x": (z, x, y)}[axis])
            rings.append(ring)
        if axis == "y":
            rings = [list(reversed(r)) for r in rings]
        return self.loft(rings, mat, caps)

    def box(self, centre, size, mat, r=0.0, n=4):
        """A box (rounded in X-Y by `r` with n segments per corner when r > 0)."""
        cx, cy, cz = centre
        sx, sy, sz = (s / 2 for s in size)
        out = rounded_rect(sx, sy, r, n) if r else [(sx, -sy), (sx, sy), (-sx, sy), (-sx, -sy)]
        return self.prism(out, (cx, cy, cz), (1, 0, 0), (0, 1, 0), (0, 0, 1), -sz, sz, mat)

    def to_object(self, name):
        import bmesh
        import bpy

        bmesh.ops.remove_doubles(self.bm, verts=self.bm.verts[:], dist=1e-6)
        bmesh.ops.recalc_face_normals(self.bm, faces=self.bm.faces[:])
        me = bpy.data.meshes.new(name)
        self.bm.to_mesh(me)
        self.bm.free()
        ob = bpy.data.objects.new(name, me)
        bpy.context.scene.collection.objects.link(ob)
        for key in self.mats:
            me.materials.append(material(key))
        for p in me.polygons:
            p.use_smooth = True
        return ob


def _v(x):
    from mathutils import Vector

    return Vector(x)


def superellipse(cx, cy, hw, hh, p, n):
    pts = []
    for i in range(n):
        a = 2 * math.pi * i / n
        c, s = math.cos(a), math.sin(a)
        pts.append((cx + hw * math.copysign(abs(c) ** (2 / p), c), cy + hh * math.copysign(abs(s) ** (2 / p), s)))
    return pts


def rounded_rect(hx, hy, r, n=4):
    pts = []
    for cx, cy, a0 in ((hx - r, -hy + r, -90), (hx - r, hy - r, 0), (-hx + r, hy - r, 90), (-hx + r, -hy + r, 180)):
        for k in range(n + 1):
            a = math.radians(a0 + 90 * k / n)
            pts.append((cx + r * math.cos(a), cy + r * math.sin(a)))
    return pts


def ellipse(rx, ry, n=24):
    return [(rx * math.cos(2 * math.pi * i / n), ry * math.sin(2 * math.pi * i / n)) for i in range(n)]


# ----------------------------------------------------------------------------- materials

_MAT_CACHE = {}


def material(key):
    """The procedural source material of preset `key` (object coordinates; nodes named
    `base`, `rough`, `metal` for the bakes)."""
    import bpy

    if key in _MAT_CACHE and _MAT_CACHE[key].name in bpy.data.materials:
        return _MAT_CACHE[key]
    light, dark, metal, (r0, r1), kind, scale, bump = MATS[key]
    m = bpy.data.materials.new(f"hd.src.{key}")
    m.use_nodes = True
    nt = m.node_tree
    bs = nt.nodes["Principled BSDF"]
    tc = nt.nodes.new("ShaderNodeTexCoord")
    mp = nt.nodes.new("ShaderNodeMapping")
    mp.inputs["Scale"].default_value = scale
    nt.links.new(tc.outputs["Object"], mp.inputs["Vector"])
    noise = nt.nodes.new("ShaderNodeTexNoise")
    noise.inputs["Scale"].default_value = 1.0
    noise.inputs["Detail"].default_value = 8.0
    noise.inputs["Roughness"].default_value = 0.6
    nt.links.new(mp.outputs[0], noise.inputs["Vector"])
    fac = noise.outputs["Fac"]
    if kind in ("grain", "grain_y"):
        wave = nt.nodes.new("ShaderNodeTexWave")
        wave.bands_direction = "X"
        wave.inputs["Scale"].default_value = 2.0
        wave.inputs["Distortion"].default_value = 9.0
        wave.inputs["Detail"].default_value = 6.0
        wave.inputs["Detail Scale"].default_value = 1.5
        nt.links.new(mp.outputs[0], wave.inputs["Vector"])
        mix = nt.nodes.new("ShaderNodeMix")
        mix.data_type = "FLOAT"
        mix.inputs[0].default_value = 0.35
        nt.links.new(wave.outputs["Fac"], mix.inputs[2])
        nt.links.new(noise.outputs["Fac"], mix.inputs[3])
        fac = mix.outputs[0]
    elif kind == "weave":
        brick = nt.nodes.new("ShaderNodeTexChecker")
        brick.inputs["Scale"].default_value = 1.0
        nt.links.new(mp.outputs[0], brick.inputs["Vector"])
        mix = nt.nodes.new("ShaderNodeMix")
        mix.data_type = "FLOAT"
        mix.inputs[0].default_value = 0.5
        nt.links.new(brick.outputs["Fac"], mix.inputs[2])
        nt.links.new(noise.outputs["Fac"], mix.inputs[3])
        fac = mix.outputs[0]
    ramp = nt.nodes.new("ShaderNodeMix")
    ramp.name = "base"
    ramp.data_type = "RGBA"
    ramp.inputs[6].default_value = (*dark, 1.0)
    ramp.inputs[7].default_value = (*light, 1.0)
    sm = nt.nodes.new("ShaderNodeMapRange")
    sm.inputs["From Min"].default_value = 0.3
    sm.inputs["From Max"].default_value = 0.7
    nt.links.new(fac, sm.inputs["Value"])
    nt.links.new(sm.outputs[0], ramp.inputs[0])
    nt.links.new(ramp.outputs[2], bs.inputs["Base Color"])
    rm = nt.nodes.new("ShaderNodeMapRange")
    rm.name = "rough"
    rm.inputs["From Min"].default_value = 0.3
    rm.inputs["From Max"].default_value = 0.7
    rm.inputs["To Min"].default_value = r1
    rm.inputs["To Max"].default_value = r0
    nt.links.new(fac, rm.inputs["Value"])
    nt.links.new(rm.outputs[0], bs.inputs["Roughness"])
    mv = nt.nodes.new("ShaderNodeValue")
    mv.name = "metal"
    mv.outputs[0].default_value = metal
    nt.links.new(mv.outputs[0], bs.inputs["Metallic"])
    bp = nt.nodes.new("ShaderNodeBump")
    bp.inputs["Strength"].default_value = bump
    bp.inputs["Distance"].default_value = 0.002
    nt.links.new(fac, bp.inputs["Height"])
    nt.links.new(bp.outputs[0], bs.inputs["Normal"])
    _MAT_CACHE[key] = m
    return m


# ----------------------------------------------------------------------------- the musket

def _stock_ring(z, n=48, scale=1.0, grow=0.0):
    top, bot, hw, p = stock_at(z)
    cy, hh = (top + bot) / 2, (top - bot) / 2
    return [(x, y, z) for x, y in superellipse(0.0, cy, hw * scale + grow, hh * scale + grow, p, n)]


def _stock_zs():
    zs = [BUTT_Z + k * 0.004 for k in range(4)]
    z = zs[-1]
    while z < STOCK_END_Z - 0.012:
        z += 0.012 if z < 0.40 else 0.03
        zs.append(min(z, STOCK_END_Z))
    if zs[-1] < STOCK_END_Z:
        zs.append(STOCK_END_Z)
    return zs


def musket_mesh(name="musket"):
    """The Short Land Pattern master (one mesh, materials walnut, brass, steel, flint)."""
    b = Builder()
    s = LOCK_SIDE
    # Stock: lofted stations, the butt end rounded off.
    zs = _stock_zs()
    rings = [_stock_ring(z, scale=0.97 if i == 0 else 1.0) for i, z in enumerate(zs)]
    b.loft(rings, "walnut")
    # Butt plate (brass): the cap over the butt end and its tang up the comb.
    b.loft([_stock_ring(BUTT_Z + 0.004, grow=0.0012), _stock_ring(BUTT_Z, grow=0.0012),
            _stock_ring(BUTT_Z - 0.004, scale=0.985, grow=0.0012)], "brass")
    tang = [(0.0, stock_at(z)[0] + 0.0008, z) for z in [BUTT_Z + 0.002 + k * 0.01 for k in range(11)]]
    b.sweep(tang, rounded_rect(0.009, 0.0012, 0.0008, 2), "brass", side=(1, 0, 0),
            scales=[1.0 - 0.5 * k / 10 for k in range(11)])
    # Barrel (steel): breech mouldings, the taper, an open bore at the muzzle.
    br = []
    for z in [BREECH_Z, BREECH_Z + 0.004, BREECH_Z + 0.006, BREECH_Z + 0.030, BREECH_Z + 0.032,
              BREECH_Z + 0.034, BREECH_Z + 0.20, BREECH_Z + 0.203, BREECH_Z + 0.206]:
        rr = barrel_r(z) + (0.0009 if BREECH_Z + 0.003 < z < BREECH_Z + 0.033 or BREECH_Z + 0.2 < z < BREECH_Z + 0.205 else 0.0)
        br.append((z, rr))
    z = BREECH_Z + 0.22
    while z < MUZZLE_Z - 0.01:
        br.append((z, barrel_r(z)))
        z += 0.025
    br += [(MUZZLE_Z - 0.003, barrel_r(MUZZLE_Z)), (MUZZLE_Z, barrel_r(MUZZLE_Z) - 0.0008)]
    n = 40
    rings = [[(r * math.cos(2 * math.pi * i / n), BARREL_Y + r * math.sin(2 * math.pi * i / n), zz) for i in range(n)] for zz, r in br]
    bore = [[(BORE_R * math.cos(2 * math.pi * i / n), BARREL_Y + BORE_R * math.sin(2 * math.pi * i / n), zz) for i in range(n)]
            for zz in (MUZZLE_Z, MUZZLE_Z - 0.06)]
    b.loft(rings + bore, "steel", caps=(True, True))
    # Barrel tang on the wrist, with its screw.
    tang = [(0.0, max(stock_at(z)[0], BARREL_Y - 0.004) + 0.0006, z) for z in [BREECH_Z - k * 0.012 for k in range(8)]]
    b.sweep(tang, rounded_rect(0.007, 0.001, 0.0007, 2), "steel", scales=[1.0 - 0.45 * k / 7 for k in range(8)])
    b.tube(0.0, 1.0, [(0, 0.0032), (0.7, 0.0032), (1.0, 0.0022)], "steel", centre=(0.0, BREECH_Z - 0.06), n=16, axis="y")
    _shift_last(b, 16 * 3, (0.0, stock_at(BREECH_Z - 0.06)[0] + 0.0004, 0.0), scale_y=0.0016)
    # Bayonet stud (doubles as the front sight).
    b.box((0.0, BARREL_Y + barrel_r(MUZZLE_Z - 0.06) + 0.002, MUZZLE_Z - 0.06), (0.004, 0.005, 0.009), "steel", r=0.0015, n=2)
    # Ramrod (steel) with its button head, in the channel under the barrel.
    b.tube(0.15, MUZZLE_Z - 0.006, [(0, ROD_R * 0.8), (0.004, ROD_R), (0.97, ROD_R), (0.975, ROD_R * 1.35),
                                    (0.995, ROD_R * 1.42), (1.0, ROD_R * 1.1)], "steel", centre=(0.0, ROD_Y), n=20)
    # Ramrod pipes (brass), the tail pipe with its finial, the nose cap.
    for zc in PIPES:
        b.tube(zc - 0.018, zc + 0.018, [(0, ROD_R + 0.0034), (0.12, ROD_R + 0.0022), (0.88, ROD_R + 0.0022), (1, ROD_R + 0.0034)],
               "brass", centre=(0.0, ROD_Y), n=24)
    b.tube(0.36, 0.45, [(0, ROD_R + 0.0028), (0.1, ROD_R + 0.0022), (0.9, ROD_R + 0.0022), (1, ROD_R + 0.0034)],
           "brass", centre=(0.0, ROD_Y), n=24)
    fin = [(0.0, stock_at(z)[1] - 0.0004, z) for z in [0.36 - k * 0.012 for k in range(6)]]
    b.sweep(fin, rounded_rect(0.006, 0.001, 0.0007, 2), "brass", side=(-1, 0, 0), scales=[1.0 - 0.6 * k / 5 for k in range(6)])
    b.loft([_stock_ring(STOCK_END_Z - 0.016, grow=0.0006), _stock_ring(STOCK_END_Z - 0.014, grow=0.0012),
            _stock_ring(STOCK_END_Z, grow=0.0012), _stock_ring(STOCK_END_Z + 0.002, scale=0.92, grow=0.0012)], "brass")
    # Thumb escutcheon on the wrist.
    b.prism(ellipse(0.0055, 0.009, 20), (0.0, stock_at(-0.02)[0] - 0.0004, -0.02), (1, 0, 0), (0, 0, 1), (0, 1, 0), 0.0, 0.0012, "brass")
    # Lock (steel) on -X.
    xs = lambda z: s * stock_at(z)[2]  # noqa: E731
    xp = s * 0.0222
    plate = [(0.060, 0.016), (0.075, 0.009), (0.110, 0.0065), (0.170, 0.0070), (0.215, 0.0095), (0.240, 0.0140),
             (0.236, 0.0200), (0.205, 0.0235), (0.170, 0.0255), (0.120, 0.0300), (0.090, 0.0310), (0.068, 0.0270), (0.058, 0.0215)]
    b.prism([(y, z) for z, y in plate], (xp, 0.0, 0.0), (0, 1, 0), (0, 0, 1), (s, 0, 0), -0.001, 0.0040, "steel")
    po = xp + s * 0.004  # outside of the plate
    cock = [(0.124, 0.012), (0.134, 0.018), (0.131, 0.027), (0.120, 0.034), (0.110, 0.038), (0.104, 0.044),
            (0.112, 0.047), (0.114, 0.0505), (0.112, 0.054), (0.094, 0.054), (0.092, 0.050), (0.097, 0.046),
            (0.098, 0.039), (0.106, 0.031), (0.114, 0.022), (0.115, 0.014)]
    b.prism([(y, z) for z, y in cock], (po, 0.0, 0.0), (0, 1, 0), (0, 0, 1), (s, 0, 0), -0.0005, 0.0075, "steel")
    jaw = [(0.093, 0.0555), (0.113, 0.0555), (0.113, 0.0585), (0.093, 0.0585)]
    b.prism([(y, z) for z, y in jaw], (po, 0.0, 0.0), (0, 1, 0), (0, 0, 1), (s, 0, 0), 0.0, 0.0075, "steel")
    b.tube(0.0, 1.0, [(0, 0.0022), (0.62, 0.0022), (0.65, 0.0042), (1.0, 0.0040)], "steel",
           centre=(po + s * 0.0037, 0.103), n=16, axis="y")
    _shift_last(b, 16 * 4, (0.0, 0.0545, 0.0), scale_y=0.014)
    flint = [(0.112, 0.0505), (0.128, 0.0515), (0.131, 0.0535), (0.112, 0.0545)]
    b.prism([(y, z) for z, y in flint], (po + s * 0.0005, 0.0, 0.0), (0, 1, 0), (0, 0, 1), (s, 0, 0), 0.0, 0.0065, "flint")
    # Frizzen (face and pan cover), the pan, the frizzen spring, the tumbler screw.
    friz = [(0.139, 0.0275), (0.168, 0.0275), (0.168, 0.0305), (0.146, 0.0310), (0.149, 0.0600), (0.143, 0.0612),
            (0.139, 0.0330)]
    b.prism([(y, z) for z, y in friz], (po - s * 0.0008, 0.0, 0.0), (0, 1, 0), (0, 0, 1), (s, 0, 0), 0.0, 0.0125, "steel")
    b.prism([(y, z) for z, y in [(0.137, 0.0215), (0.166, 0.0215), (0.166, 0.0273), (0.137, 0.0273)]],
            (po - s * 0.0008, 0.0, 0.0), (0, 1, 0), (0, 0, 1), (s, 0, 0), 0.0, 0.0115, "steel")
    spring = [(0.172, 0.0185), (0.226, 0.0145), (0.230, 0.0118), (0.226, 0.0105), (0.180, 0.0120), (0.172, 0.0140)]
    b.prism([(y, z) for z, y in spring], (po, 0.0, 0.0), (0, 1, 0), (0, 0, 1), (s, 0, 0), -0.0005, 0.0035, "steel")
    b.tube(0.0, 1.0, [(0, 0.0040), (0.6, 0.0040), (1.0, 0.0028)], "steel", centre=(0.0175, 0.121), n=20, axis="x")
    _shift_last(b, 20 * 3, (po, 0.0, 0.0), scale_x=s * 0.0035)
    # Side plate (brass) and the two lock screws on the other side.
    side = [(0.055, 0.010), (0.080, 0.006), (0.120, 0.0075), (0.170, 0.012), (0.200, 0.016), (0.195, 0.021),
            (0.160, 0.0185), (0.118, 0.021), (0.090, 0.028), (0.065, 0.024)]
    xo = -s * 0.0222
    b.prism([(y, z) for z, y in side], (xo, 0.0, 0.0), (0, 1, 0), (0, 0, 1), (-s, 0, 0), -0.001, 0.0018, "brass")
    for zc, yc in ((0.085, 0.017), (0.180, 0.0155)):
        b.tube(0.0, 1.0, [(0, 0.0042), (0.5, 0.0040), (1.0, 0.0020)], "steel", centre=(yc, zc), n=16, axis="x")
        _shift_last(b, 16 * 3, (xo - s * 0.0016, 0.0, 0.0), scale_x=-s * 0.0018)
    # Trigger guard (brass): rear finial, bow, front finial with the sling swivel; trigger.
    rear = [(0.0, stock_at(z)[1] - 0.0006, z) for z in [0.072 - k * 0.012 for k in range(11)]]
    b.sweep(rear, rounded_rect(0.0065, 0.0012, 0.0008, 2), "brass", side=(-1, 0, 0), scales=[1.0 - 0.55 * k / 10 for k in range(11)])
    bow = []
    for k in range(17):
        a = math.pi * k / 16
        bow.append((0.0, -0.028 - 0.024 * math.sin(a), 0.112 - 0.040 * math.cos(a)))
    b.sweep(bow, rounded_rect(0.0055, 0.0022, 0.0012, 2), "brass", side=(-1, 0, 0))
    front = [(0.0, stock_at(z)[1] - 0.0006, z) for z in [0.150 + k * 0.014 for k in range(11)]]
    b.sweep(front, rounded_rect(0.0065, 0.0012, 0.0008, 2), "brass", side=(-1, 0, 0), scales=[1.0 - 0.55 * k / 10 for k in range(11)])
    trig = [(0.0, -0.028 - 0.004 * k, 0.108 - 0.0035 * k * k / 3.0) for k in range(7)]
    b.sweep(trig, rounded_rect(0.0022, 0.0035, 0.0012, 2), "steel", side=(-1, 0, 0), scales=[1.0 - 0.3 * k / 6 for k in range(7)])
    _swivel(b, (0.0, stock_at(0.200)[1] - 0.012, 0.200))
    _swivel(b, (0.0, ROD_Y - ROD_R - 0.012, PIPES[0]))
    _ = xs
    ob = b.to_object(name)
    _bevel(ob, 0.00045)
    ob["acr_grip"] = json.dumps(grip_geometry())
    return ob


def _swivel(b, centre):
    """A sling swivel: a flattened ring hanging under its stud (ring in the Y-Z plane)."""
    cx, cy, cz = centre
    ring = [(cx, cy + 0.010 * math.sin(2 * math.pi * k / 24), cz + 0.016 * math.cos(2 * math.pi * k / 24)) for k in range(25)]
    b.sweep(ring, ellipse(0.0016, 0.0016, 10), "steel", side=(1, 0, 0), caps=(False, False))
    b.box((cx, cy + 0.011, cz), (0.004, 0.006, 0.006), "steel", r=0.0012, n=2)


def _shift_last(b, count, offset, scale_x=None, scale_y=None):
    """Moves the last `count` vertices (a tube just built round the origin in its own axis)
    by `offset`, first scaling its length (built 0..1) to `scale_x` / `scale_y` metres."""
    b.bm.verts.ensure_lookup_table()
    for v in b.bm.verts[-count:]:
        if scale_x is not None:
            v.co.x *= scale_x
        if scale_y is not None:
            v.co.y *= scale_y
        v.co.x += offset[0]
        v.co.y += offset[1]
        v.co.z += offset[2]


def _bevel(ob, width, angle=35):
    import bpy

    m = ob.modifiers.new("bevel", "BEVEL")
    m.width = width
    m.segments = 2
    m.limit_method = "ANGLE"
    m.angle_limit = math.radians(angle)
    m.use_clamp_overlap = True
    m.harden_normals = False
    bpy.context.view_layer.objects.active = ob
    with bpy.context.temp_override(object=ob, active_object=ob):
        bpy.ops.object.modifier_apply(modifier=m.name)


def bayonet_mesh(name="bayonet"):
    """Socket bayonet in its own frame: +Z along the barrel, origin on the socket's rear
    edge on the bore axis; the blade offset to -X (beside the barrel), flat face out."""
    b = Builder()
    sl, bl, r = BAYONET["socket"], BAYONET["blade"], BAYONET["socket_r"]
    b.tube(0.0, sl, [(0, r + 0.0012), (0.05, r), (0.95, r), (1.0, r + 0.0008)], "steel", n=32, caps=(False, False))
    b.tube(0.0, sl, [(0, r - 0.0014), (1.0, r - 0.0014)], "steel", n=32, caps=(False, False))
    neck = [(-0.004 - 0.026 * math.sin(math.pi / 2 * k / 6), 0.0, sl - 0.004 + 0.024 * (1 - math.cos(math.pi / 2 * k / 6))) for k in range(7)]
    b.sweep(neck, ellipse(0.0042, 0.0042, 12), "steel", side=(0, 1, 0))
    x0, z0 = neck[-1][0], neck[-1][2]
    sec = [(0.0, -0.011), (0.0012, -0.004), (0.0060, 0.0), (0.0012, 0.004), (0.0, 0.011), (-0.0020, 0.0)]
    path = [(x0, 0.0, z0 + bl * k / 30) for k in range(31)]
    b.sweep(path, [(n_, s_) for s_, n_ in sec], "steel", side=(0, 1, 0),
            scales=[max(0.02, 1.0 - (k / 30) ** 1.1) for k in range(31)])
    return _finish_mesh(b, name)


def _finish_mesh(b, name, bevel=0.0004):
    ob = b.to_object(name)
    if bevel:
        _bevel(ob, bevel)
    return ob


def scabbard_mesh(name="bayonet_scabbard", length=0.40, width=0.030, brass_tip=True):
    """Black leather scabbard with a brass locket and chape, along -Z from its mouth."""
    b = Builder()
    rings = []
    for k in range(25):
        t = k / 24
        w = width * (1 - 0.85 * t ** 1.3) / 2 + 0.002
        rings.append([(x, y * 0.35, -length * t) for x, y in superellipse(0, 0, w, w, 2.3, 20)])
    b.loft(rings, "leather")
    b.loft([[(x * 1.06, y * 1.15, z + 0.002) for x, y, z in rings[0]], [(x * 1.06, y * 1.15, z - 0.04) for x, y, z in rings[0]]], "brass")
    if brass_tip:
        tip = [[(x * 1.08, y * 1.15, -length + 0.06 - 0.06 * t) for x, y, _z in rings[int(24 * (1 - 0.15 * (1 - t)))]] for t in (0.0, 0.5, 1.0)]
        b.loft(tip, "brass")
    b.box((0.0, 0.0, 0.03), (width * 0.8, 0.008, 0.07), "leather", r=0.003, n=2)
    return _finish_mesh(b, name)


# ----------------------------------------------------------------------------- the kit

def cartridge_box_mesh(name="cartridge_box"):
    """Black leather box (0.20 x 0.075 x 0.13 m) with its flap over the top and down the
    front, an oval brass badge, the loops for the pouch belt on the back."""
    b = Builder()
    w, d, h = 0.20, 0.075, 0.13
    b.prism(rounded_rect(w / 2, h / 2, 0.012, 4), (0.0, -d / 2, 0.0), (1, 0, 0), (0, 0, 1), (0, 1, 0), 0.0, d, "leather")
    path = [(0.0, -d / 2 - 0.002, h / 2 + 0.003)]
    for k in range(7):
        a = math.pi / 2 * k / 6
        path.append((0.0, d / 2 - 0.010 + 0.0135 * math.sin(a), h / 2 - 0.010 + 0.0135 * math.cos(a)))
    path += [(0.0, d / 2 + 0.0035, h / 2 - 0.03 - 0.03 * k) for k in range(4)]
    path.append((0.0, d / 2 + 0.0035, -h / 2 + 0.018))
    b.sweep(path, [(-w / 2 - 0.004, 0.0), (w / 2 + 0.004, 0.0), (w / 2 + 0.004, 0.003), (-w / 2 - 0.004, 0.003)], "leather",
            side=(1, 0, 0), scales=None)
    b.prism(ellipse(0.036, 0.027, 32), (0.0, d / 2 + 0.0065, -0.005), (1, 0, 0), (0, 0, 1), (0, 1, 0), 0.0, 0.0018, "brass")
    b.prism(ellipse(0.028, 0.020, 32), (0.0, d / 2 + 0.0083, -0.005), (1, 0, 0), (0, 0, 1), (0, 1, 0), 0.0, 0.0012, "brass")
    for x in (-0.06, 0.06):
        b.box((x, -d / 2 - 0.003, 0.0), (0.03, 0.006, 0.09), "leather", r=0.002, n=2)
    return _finish_mesh(b, name)


def crossbelts_mesh(name="crossbelts"):
    """The two buff crossbelts laid flat (each 1.40 m x 60 mm, 4 mm thick): the pouch belt
    with its buckle, the waist (bayonet) belt with the oval brass belt plate."""
    b = Builder()
    for i, x in enumerate((-0.12, 0.12)):
        path = [(x + 0.08 * math.sin(math.pi * k / 20) * (1 if i else -1), -0.70 + 1.40 * k / 20, 0.002) for k in range(21)]
        b.sweep(path, rounded_rect(0.002, 0.030, 0.0015, 2), "buff", side=(0, 0, 1))
        y_end = path[-1][1]
        xe = path[-1][0]
        frame = rounded_rect(0.036, 0.028, 0.004, 3)
        inner = rounded_rect(0.030, 0.022, 0.002, 3)
        b.prism(frame, (xe, y_end - 0.03, 0.004), (1, 0, 0), (0, 1, 0), (0, 0, 1), 0.0, 0.004, "brass")
        b.prism(inner, (xe, y_end - 0.03, 0.0081), (1, 0, 0), (0, 1, 0), (0, 0, 1), 0.0, 0.0001, "buff")
        b.box((xe, y_end - 0.03, 0.009), (0.004, 0.05, 0.003), "steel", r=0.0012, n=2)
    plate = ellipse(0.038, 0.030, 40)
    b.prism(plate, (-0.12, -0.10, 0.004), (1, 0, 0), (0, 1, 0), (0, 0, 1), 0.0, 0.0026, "brass")
    b.prism(ellipse(0.032, 0.024, 40), (-0.12, -0.10, 0.0066), (1, 0, 0), (0, 1, 0), (0, 0, 1), 0.0, 0.0010, "brass")
    return _finish_mesh(b, name)


def hanger_mesh(name="hanger", blade=True):
    """Infantry hanger (short sword): slightly curved 0.64 m blade, brass hilt with a
    ribbed grip, knuckle bow and pommel, along +Z from the grip (origin). `blade=False`: the
    hilt only (worn in its scabbard, `kit_hd`)."""
    b = Builder()
    if blade:
        length = 0.64
        path = [(0.0, -0.025 * (k / 30) ** 2, -0.01 - length * k / 30) for k in range(31)]
        sec = [(0.0, 0.003), (0.014, 0.0012), (0.016, 0.0), (0.014, -0.0012), (0.0, -0.003), (-0.012, -0.0022), (-0.013, 0.0), (-0.012, 0.0022)]
        b.sweep(path, [(n_, s_) for s_, n_ in sec], "steel", side=(1, 0, 0),
                scales=[1.0 - 0.75 * (k / 30) ** 3 for k in range(31)])
    grip = [(t, 0.0135 + 0.0012 * math.sin(2 * math.pi * 7 * t) - 0.002 * abs(t - 0.5)) for t in [k / 28 for k in range(29)]]
    b.tube(0.0, 0.10, grip, "brass", n=20)
    b.tube(0.10, 0.125, [(0, 0.012), (0.3, 0.016), (0.7, 0.014), (1.0, 0.004)], "brass", n=20)
    b.box((0.0, 0.0, -0.008), (0.022, 0.07, 0.008), "brass", r=0.004, n=3)
    bow = [(0.0, -0.034 * (1 - k / 12) - 0.012 * math.sin(math.pi * k / 12), -0.004 + 0.12 * k / 12) for k in range(13)]
    b.sweep(bow, ellipse(0.0035, 0.0035, 10), "brass", side=(1, 0, 0))
    return _finish_mesh(b, name)


def canteen_mesh(name="canteen"):
    """Wooden "cheese-box" canteen: drum 0.21 m across, 0.07 m deep, axis along Y, two iron
    hoops, the spout with its stopper and the cord loops."""
    b = Builder()
    R, D = 0.105, 0.070
    prof = [(0.0, R - 0.03), (0.02, R - 0.006), (0.06, R - 0.001), (0.5, R), (0.94, R - 0.001), (0.98, R - 0.006), (1.0, R - 0.03)]
    b.tube(-D / 2, D / 2, prof, "wood", n=48, axis="y")
    for yh in (-0.022, 0.022):
        b.tube(yh - 0.005, yh + 0.005, [(0, R + 0.0005), (0.2, R + 0.002), (0.8, R + 0.002), (1, R + 0.0005)], "iron", n=48, axis="y",
               caps=(False, False))
    b.tube(R - 0.01, R + 0.022, [(0, 0.014), (0.6, 0.013), (0.75, 0.011), (1.0, 0.010)], "wood", n=20)
    _rot_last_to_top(b, 20 * 4)
    b.tube(R + 0.018, R + 0.040, [(0, 0.0085), (0.7, 0.0095), (1.0, 0.007)], "cord", n=16)
    _rot_last_to_top(b, 16 * 3)
    for a in (math.radians(55), math.radians(-55)):
        c = (R + 0.004) * math.sin(a), (R + 0.004) * math.cos(a)
        ring = [(c[0] + 0.009 * math.cos(2 * math.pi * k / 16) * math.cos(a), 0.012 * math.sin(2 * math.pi * k / 16),
                 c[1] - 0.009 * math.cos(2 * math.pi * k / 16) * math.sin(a)) for k in range(17)]
        b.sweep(ring, ellipse(0.0018, 0.0018, 8), "iron", side=(0, 1, 0), caps=(False, False))
    return _finish_mesh(b, name)


def _rot_last_to_top(b, count):
    """Tubes built along Z: stand them up out of the canteen's rim (already along Z: no-op;
    kept for clarity of intent)."""
    return count


def knapsack_mesh(name="knapsack"):
    """Painted canvas knapsack, soft (an envelope that bulges, not a box): 0.40 x 0.38 m,
    0.10 m deep, the flap over the top and two thirds down the back, two buff straps with
    brass buckles."""
    b = Builder()
    W, H, D = 0.20, 0.19, 0.05
    rings = []
    for k in range(13):
        t = -1 + 2 * k / 12
        bulge = math.sqrt(max(0.0, 1 - t * t)) ** 0.6
        sx, sz = W * (0.86 + 0.14 * bulge), H * (0.88 + 0.12 * bulge)
        y = D * t
        ring = [(x, y, z) for x, z in superellipse(0.0, 0.0, sx, sz, 5.0 if abs(t) < 0.99 else 4.0, 64)]
        rings.append([(x, y * (0.6 + 0.4 * (1 - abs(z) / H)) if abs(t) > 0.5 else y, z) for x, y, z in ring])
    for r_ in rings:  # the pack bulges in the middle of each face
        for i, (x, y, z) in enumerate(r_):
            f = (1 - (x / W) ** 2) * (1 - (z / H) ** 2)
            r_[i] = (x, y * (0.75 + 0.25 * f), z)
    b.loft([[(x, z * 0 + y, z) for x, y, z in r_] for r_ in rings], "canvas")
    path = [(0.0, -D * 0.85 - 0.002, H * 0.15)]
    path += [(0.0, -D * 0.85 + (D * 1.7) * k / 6, H + 0.004 + 0.006 * math.sin(math.pi * k / 6)) for k in range(7)]
    path += [(0.0, D * 0.95 + 0.003, H - 0.02 - 0.08 * k) for k in range(4)]
    b.sweep([(p[0], p[1], p[2]) for p in path], [(-W - 0.006, 0.0), (W + 0.006, 0.0), (W + 0.006, 0.003), (-W - 0.006, 0.003)],
            "canvas", side=(1, 0, 0))
    for x in (-0.11, 0.11):
        strap = [(x, -D - 0.006, -H * 0.9)] + [(x, -D - 0.006 - 0.05 * math.sin(math.pi * k / 10), -H * 0.9 + 1.8 * H * k / 10) for k in range(1, 11)]
        b.sweep(strap, rounded_rect(0.018, 0.0018, 0.001, 2), "buff", side=(1, 0, 0))
        b.prism(rounded_rect(0.024, 0.018, 0.003, 3), (x, D * 0.95 + 0.007, -H * 0.4), (1, 0, 0), (0, 0, 1), (0, 1, 0), 0.0, 0.0035, "brass")
        b.box((x, D * 0.95 + 0.004, -H * 0.15), (0.034, 0.003, 0.30), "buff", r=0.001, n=1)
    return _finish_mesh(b, name)


def belt_plate_mesh(name="belt_plate"):
    """Oval brass belt plate (76 x 60 mm) facing -Y, origin on its back."""
    b = Builder()
    b.prism(ellipse(0.038, 0.030, 40), (0.0, 0.0, 0.0), (1, 0, 0), (0, 0, 1), (0, -1, 0), 0.0, 0.0026, "brass")
    b.prism(ellipse(0.032, 0.024, 40), (0.0, 0.0, 0.0), (1, 0, 0), (0, 0, 1), (0, -1, 0), 0.0026, 0.0036, "brass")
    return _finish_mesh(b, name)


def buckle_mesh(name="buckle"):
    """Brass frame buckle with its steel tongue facing -Y (72 x 56 mm), origin on its back."""
    b = Builder()
    b.prism(rounded_rect(0.036, 0.028, 0.004, 3), (0.0, 0.0, 0.0), (1, 0, 0), (0, 0, 1), (0, -1, 0), 0.0, 0.004, "brass")
    b.prism(rounded_rect(0.030, 0.022, 0.002, 3), (0.0, 0.0, 0.0), (1, 0, 0), (0, 0, 1), (0, -1, 0), 0.004, 0.0041, "buff")
    b.box((0.0, -0.005, 0.0), (0.004, 0.003, 0.05), "steel", r=0.0012, n=2)
    return _finish_mesh(b, name)


def _cyclic_tube(b, centres, normals, ups, r, mat, n=6):
    """A closed tube of radius `r` through the closed loop `centres` (section in the plane of
    `normals` and `ups`)."""
    from mathutils import Vector

    mi = b._m(mat)
    rings = []
    for c, nn, uu in zip(centres, normals, ups):
        c, nn, uu = Vector(c), Vector(nn), Vector(uu)
        rings.append([b.bm.verts.new(c + nn * (r * math.cos(2 * math.pi * k / n)) + uu * (r * math.sin(2 * math.pi * k / n)))
                      for k in range(n)])
    for i in range(len(rings)):
        a, c = rings[i], rings[(i + 1) % len(rings)]
        for k in range(n):
            j = (k + 1) % n
            b.bm.faces.new((a[k], a[j], c[j], c[k])).material_index = mi


TRICORN_CORNERS = (-90.0, 30.0, 150.0)  # plan angles of the three points (front at -Y)


def tricorn_shape(a, rx, ry, side=(0.030, 0.085), corner=(0.075, 0.030), sharp=10):
    """(crown edge point (x, y), outward plan direction, reach, rise) of the brim at plan
    angle `a` (radians): a mid-side brim reaches `side[0]` out and is cocked `side[1]` up,
    a point reaches `corner[0]` and rises `corner[1]`."""
    c = max(max(0.0, math.cos(a - math.radians(k))) ** sharp for k in TRICORN_CORNERS)
    ex, ey = rx * math.cos(a), ry * math.sin(a)
    nx, ny = ex / rx ** 2, ey / ry ** 2
    ln = math.hypot(nx, ny)
    return (ex, ey), (nx / ln, ny / ln), side[0] + (corner[0] - side[0]) * c, side[1] + (corner[1] - side[1]) * c


def tricorn_mesh(name="tricorn", rx=0.088, ry=0.102, crown=0.10, n=96, m=10):
    """Black felt tricorn: an oval crown (`rx`, `ry` inside at the band, `crown` m high), the
    brim cocked up on three sides into three points (front and two at the back,
    `TRICORN_CORNERS`), a white tape edge, the black silk cockade on the left front cock with
    its tape loop and brass button. Origin at the crown's opening (the hat band), +Z up, the
    front point towards -Y. Felt 3 mm (solidified)."""
    import bpy

    b = Builder()
    crown_rings = []
    for t, s in ((0.0, 1.0), (0.35, 0.995), (0.7, 0.975), (0.88, 0.95), (0.96, 0.90), (1.0, 0.80)):
        crown_rings.append([(rx * s * math.cos(2 * math.pi * k / n), ry * s * math.sin(2 * math.pi * k / n), crown * t) for k in range(n)])
    b.loft(crown_rings, "felt", caps=(False, True))
    rings, edge, enorm, eup = [], [], [], []
    for j in range(m + 1):
        th = (math.pi / 2) * j / m
        ring = []
        for k in range(n):
            a = 2 * math.pi * k / n
            (ex, ey), (dx, dy), w, h = tricorn_shape(a, rx, ry)
            out = w * math.sin(th) + 0.002
            ring.append((ex + dx * out, ey + dy * out, h * (1 - math.cos(th)) - 0.002))
            if j == m:
                edge.append(ring[-1])
                enorm.append((dx, dy, 0.0))
                eup.append((0.0, 0.0, 1.0))
        rings.append(ring)
    b.loft(rings, "felt", caps=(False, False))
    ob = b.to_object(name + ".tmp")
    sol = ob.modifiers.new("felt", "SOLIDIFY")
    sol.thickness = 0.003
    sol.offset = 0.0
    _select_only(ob)
    bpy.ops.object.modifier_apply(modifier="felt")
    # Tape edge, cockade, loop and button: a second builder joined on.
    t = Builder()
    _cyclic_tube(t, edge, enorm, eup, 0.0035, "tape", n=6)
    a = math.radians(-30.0)  # the left front cock (wearer's left is +X)
    (ex, ey), (dx, dy), w, h = tricorn_shape(a, rx, ry)
    th = 0.80 * math.pi / 2
    out = w * math.sin(th) + 0.002 + 0.003
    c = (ex + dx * out, ey + dy * out, h * (1 - math.cos(th)))
    u = (-dy, dx, 0.0)
    lobes = [(0.034 + 0.003 * math.cos(24 * 2 * math.pi * k / 96)) for k in range(96)]
    rosette = [(r * math.cos(2 * math.pi * k / 96), r * math.sin(2 * math.pi * k / 96)) for k, r in enumerate(lobes)]
    t.prism(rosette, c, u, (0, 0, 1), (dx, dy, 0.0), 0.0, 0.004, "cockade")
    t.prism(rounded_rect(0.006, 0.045, 0.002, 2), (c[0], c[1], c[2] - 0.005), u, (0, 0, 1), (dx, dy, 0.0), 0.004, 0.0055, "tape")
    t.prism(ellipse(0.008, 0.008, 20), (c[0], c[1], c[2] - 0.030), u, (0, 0, 1), (dx, dy, 0.0), 0.0055, 0.0085, "brass")
    tape = t.to_object(name + ".trim")
    _select_only(ob, tape)
    bpy.ops.object.join()
    ob.name = ob.data.name = name
    return ob


# ----------------------------------------------------------------------------- bakes and LODs

def _tris(ob):
    ob.data.calc_loop_triangles()
    return len(ob.data.loop_triangles)


def _select_only(ob, *others):
    import bpy

    bpy.ops.object.select_all(action="DESELECT")
    for o in others:
        o.select_set(True)
    ob.select_set(True)
    bpy.context.view_layer.objects.active = ob


def _atlas(ob, px):
    import bpy

    _select_only(ob)
    me = ob.data
    while me.uv_layers:
        me.uv_layers.remove(me.uv_layers[0])
    me.uv_layers.new(name="atlas")
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.uv.smart_project(angle_limit=math.radians(60), island_margin=4.0 / px, area_weight=1.0)
    bpy.ops.object.mode_set(mode="OBJECT")


def _bake_emit(ob, img, source):
    """Bakes each material's `source` ("base" | "rough" | "metal") through emission into
    `img` (the object's own materials, the target node active in each)."""
    import bpy

    saved = []
    for m in ob.data.materials:
        nt = m.node_tree
        bs = nt.nodes["Principled BSDF"]
        node = nt.nodes[source]
        out = node.outputs[2] if source == "base" else node.outputs[0]
        em = nt.nodes.new("ShaderNodeEmission")
        nt.links.new(out, em.inputs["Color"])
        mo = nt.nodes["Material Output"]
        old = mo.inputs["Surface"].links[0].from_socket
        nt.links.new(em.outputs[0], mo.inputs["Surface"])
        tgt = nt.nodes.new("ShaderNodeTexImage")
        tgt.image = img
        nt.nodes.active = tgt
        saved.append((nt, em, tgt, old, bs))
    _select_only(ob)
    bpy.context.scene.render.bake.use_selected_to_active = False
    bpy.ops.object.bake(type="EMIT")
    for nt, em, tgt, old, _bs in saved:
        nt.links.new(old, nt.nodes["Material Output"].inputs["Surface"])
        nt.nodes.remove(em)
        nt.nodes.remove(tgt)


def _bake_type(ob, img, kind, samples=1, selected=None):
    import bpy

    sc = bpy.context.scene
    tg = []
    for m in (ob.data.materials):
        nt = m.node_tree
        t = nt.nodes.new("ShaderNodeTexImage")
        t.image = img
        nt.nodes.active = t
        tg.append((nt, t))
    sc.cycles.samples = samples
    if selected is not None:
        _select_only(ob, selected)
        sc.render.bake.use_selected_to_active = True
        sc.render.bake.cage_extrusion = 0.003
        sc.render.bake.max_ray_distance = 0.012
    else:
        _select_only(ob)
        sc.render.bake.use_selected_to_active = False
    bpy.ops.object.bake(type=kind)
    sc.cycles.samples = 1
    sc.render.bake.use_selected_to_active = False
    for nt, t in tg:
        nt.nodes.remove(t)


def _pixels(img):
    import numpy as np

    n = img.size[0]
    a = np.empty(n * n * 4, np.float32)
    img.pixels.foreach_get(a)
    return a.reshape(n, n, 4)


def _set_pixels(img, arr):
    img.pixels.foreach_set(arr.ravel())
    img.pack()


def baked_material(name, alb, orm, nrm):
    """Principled BSDF from baked images: albedo, metallic-roughness (G, B), normal map."""
    import bpy

    m = bpy.data.materials.new(name)
    m.use_nodes = True
    nt = m.node_tree
    bs = nt.nodes["Principled BSDF"]
    ta = nt.nodes.new("ShaderNodeTexImage")
    ta.image = alb
    nt.links.new(ta.outputs[0], bs.inputs["Base Color"])
    to = nt.nodes.new("ShaderNodeTexImage")
    to.image = orm
    sp = nt.nodes.new("ShaderNodeSeparateColor")
    nt.links.new(to.outputs[0], sp.inputs[0])
    nt.links.new(sp.outputs["Green"], bs.inputs["Roughness"])
    nt.links.new(sp.outputs["Blue"], bs.inputs["Metallic"])
    tn = nt.nodes.new("ShaderNodeTexImage")
    tn.image = nrm
    nm = nt.nodes.new("ShaderNodeNormalMap")
    nt.links.new(tn.outputs[0], nm.inputs["Color"])
    nt.links.new(nm.outputs[0], bs.inputs["Normal"])
    return m


def _new_img(name, px, colour):
    import bpy

    img = bpy.data.images.new(name, px, px, alpha=False, float_buffer=False)
    img.colorspace_settings.name = "sRGB" if colour else "Non-Color"
    return img


def finish(ob, px, stats, ratios=LOD_RATIOS, lod_tex=LOD_TEX, layer=None):
    """Bakes the master's procedural materials into its own atlas (albedo with ambient
    occlusion, metallic-roughness, normal), swaps in the baked material, then builds and
    bakes its levels. `layer`: the decay layer whose `acr_decay` extras every baked
    material (master and levels) carries from its creation. Returns the level objects (rest
    at the master's place)."""
    import bpy
    import numpy as np

    from . import decay, hdbody

    sc = hdbody._cycles()
    sc.render.bake.margin = max(4, px // 256)
    tris = _tris(ob)
    _atlas(ob, px)
    nm = ob.name
    alb, rough, metal, ao, nrm = (_new_img(f"{nm}.{k}", px, k == "albedo") for k in ("albedo", "rough", "metal", "ao", "normal"))
    _bake_emit(ob, alb, "base")
    _bake_emit(ob, rough, "rough")
    _bake_emit(ob, metal, "metal")
    _bake_type(ob, ao, "AO", samples=24)
    _bake_type(ob, nrm, "NORMAL", samples=1)
    a, r_, m_, o_ = (_pixels(i) for i in (alb, rough, metal, ao))
    occ = 0.45 + 0.55 * o_[..., :1]
    a[..., :3] *= occ
    _set_pixels(alb, a)
    orm_px = np.ones_like(a)
    orm_px[..., 0] = o_[..., 0]
    orm_px[..., 1] = np.clip(r_[..., 0] + 0.12 * (1 - o_[..., 0]), 0.0, 1.0)
    orm_px[..., 2] = m_[..., 0]
    orm = _new_img(f"{nm}.orm", px, False)
    _set_pixels(orm, orm_px)
    nrm.pack()
    for i in (rough, metal, ao):
        bpy.data.images.remove(i)
    src = list(ob.data.materials)
    mat = baked_material(f"{nm}.hd", alb, orm, nrm)
    if layer:
        mat["acr_decay"] = decay.extras(layer)
    from . import held

    held.set_materials(ob.data, [mat])
    for p in ob.data.polygons:
        p.material_index = 0
    lods, lod_tris = [], []
    for level, (target, lpx) in enumerate(zip(lod_targets(tris, ratios), lod_tex), 1):
        lo = ob.copy()
        lo.data = ob.data.copy()
        # The level ends in the item key (`<unit>.lod1.musket`), so the game drops a held
        # item's levels with it at death (`three_d::held_gear` reads the last name part).
        base, _, item = nm.rpartition(".")
        lo.name = f"{base}.lod{level}.{item}" if base else f"{nm}.lod{level}"
        bpy.context.scene.collection.objects.link(lo)
        d = lo.modifiers.new("dec", "DECIMATE")
        d.ratio = min(1.0, target / tris)
        d.use_collapse_triangulate = True
        _select_only(lo)
        bpy.ops.object.modifier_apply(modifier="dec")
        ln = _new_img(f"{nm}.lod{level}.normal", lpx, False)
        la, lr = alb.copy(), orm.copy()
        la.name, lr.name = f"{nm}.lod{level}.albedo", f"{nm}.lod{level}.orm"
        la.scale(lpx, lpx)
        lr.scale(lpx, lpx)
        la.pack()
        lr.pack()
        lm = baked_material(f"{nm}.lod{level}", la, lr, ln)
        if layer:
            lm["acr_decay"] = decay.extras(layer)
        held.set_materials(lo.data, [lm])
        sc.render.bake.margin = max(2, lpx // 128)
        _bake_type(lo, ln, "NORMAL", selected=ob)
        ln.pack()
        lo["acr_lod"] = level
        lo["acr_lod_of"] = nm
        lo["acr_lod_tris"] = _tris(lo)
        lod_tris.append(_tris(lo))
        lods.append(lo)
    del src  # the procedural sources stay (shared by the next items), unused: not exported
    dims = [round(x, 3) for x in ob.dimensions]
    stats[nm.split(".")[-1]] = {"tris": tris, "lod_tris": lod_tris, "tex_px": px, "lod_tex_px": list(lod_tex), "size_m": dims}
    return lods


ITEMS = {
    "musket": (musket_mesh, 2048),
    "bayonet": (bayonet_mesh, 1024),
    "bayonet_scabbard": (scabbard_mesh, 1024),
    "cartridge_box": (cartridge_box_mesh, 2048),
    "crossbelts": (crossbelts_mesh, 2048),
    "hanger": (hanger_mesh, 2048),
    "hanger_scabbard": (lambda name: scabbard_mesh(name, length=0.66, width=0.036), 1024),
    "canteen": (canteen_mesh, 2048),
    "knapsack": (knapsack_mesh, 2048),
    "hanger_hilt": (lambda name: hanger_mesh(name, blade=False), 1024),
    "belt_plate": (belt_plate_mesh, 512),
    "buckle": (buckle_mesh, 512),
    "tricorn": (tricorn_mesh, 2048),
}


# The decay layer of each item (`decay.LAYERS`; the knapsack is painted canvas), for items
# that carry a decay mask (`COLOR_0`): without `acr_decay` the game would read the mask as
# colour (the noise showed as rainbow on the worn kit).
LAYERS = {"musket": "wood", "bayonet": "metal", "bayonet_scabbard": "leather", "cartridge_box": "leather",
          "crossbelts": "leather", "hanger": "metal", "hanger_scabbard": "leather", "canteen": "wood",
          "knapsack": "linen", "hanger_hilt": "metal", "belt_plate": "metal", "buckle": "metal", "tricorn": "wool"}


def build_item(ctx, key, prefix=None, layer=None, **shape):
    """Builds, bakes and LODs item `key` (`shape`: keyword arguments of its mesh function,
    e.g. the tricorn's crown fitted to the head; `layer`: its decay layer when it will carry a
    decay mask, e.g. `LAYERS[key]`); returns (master, [levels])."""
    fn, px = ITEMS[key]
    name = f"{prefix or ctx.name}.{key}"
    ob = fn(name, **shape)
    st = ctx.stats.setdefault("gear_hd", {})
    lods = finish(ob, px, st, layer=layer)
    if key == "musket":
        st["grip"] = grip_geometry()
    return ob, lods


def muzzle_socket(gun):
    import bpy

    m = bpy.data.objects.new("socket.muzzle", None)
    m.empty_display_size = 0.05
    bpy.context.scene.collection.objects.link(m)
    m.parent = gun
    m.location = (0.0, BARREL_Y, MUZZLE_Z)
    return m


def hold(arm, gun, lods, bayonet=None, bayonet_lods=()):
    """Puts the musket (and its levels) on `socket.hand_r` as `line` puts `gear.musket`
    (`grips.hold_matrix`, the same item frame), the bayonet fixed on the muzzle."""
    from mathutils import Matrix

    from . import armature as am
    from . import grips

    at = grips.hold_matrix(arm, "socket.hand_r", (0.0, 0.0, 0.0))  # origin already on the wrist
    for ob in [gun, *lods]:
        am.attach(ob, arm, "socket.hand_r", at)
    if bayonet is not None:
        fix = at @ Matrix.Translation((0.0, BARREL_Y, MUZZLE_Z - BAYONET["socket"]))
        for ob in [bayonet, *bayonet_lods]:
            am.attach(ob, arm, "socket.hand_r", fix)
    muzzle_socket(gun)
