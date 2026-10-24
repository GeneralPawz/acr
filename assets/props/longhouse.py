"""Iroquois longhouse (scripted; the generated ones came out as short huts).

24 m long, 6 m wide, 6 m high, the long axis along Y, the front end wall with its hide
door facing -Y. Cross-section: straight side walls 2.6 m high under a barrel vault (half
ellipse). Cladding: overlapping rectangular elm-bark sheets in courses along the profile,
each course standing a little proud of the one below (shingled, sheds rain), joints
staggered; a cap course over the ridge with four smoke holes (one per pair of hearths),
each with a bark lid propped open beside it. Outside: arched sapling ribs every 2 m and
horizontal poles lashed across them that hold the bark down; end walls of vertical bark
sheets between posts, a hide over the door at both ends.

Materials (`acr_assets.scripted`): bark, wood and hide, each base colour + normal map +
roughness. Stats: `triangles`, `size_m`, `panels`, `smoke_holes`, `ribs`, `poles`.
"""

import math
import random

import numpy as np
from mathutils import Vector

from acr_assets import scripted as sc

L, W, HW, H = 24.0, 6.0, 2.6, 6.0
A, B = W / 2, H - HW  # half width, vault rise
COURSE = 1.05  # bark course height along the profile (m)
SMOKE = (-9.0, -3.0, 3.0, 9.0)  # smoke holes along the ridge (y)
HOLE = 1.0  # smoke hole length along y
RIBS = 13


def _profile():
    """Dense cross-section polyline from the left foot over the ridge to the right foot:
    points (x, z), outward normals and arc length."""
    pts = [(-A, z) for z in np.linspace(0, HW, 8)[:-1]]
    for phi in np.linspace(math.pi, 0, 61):
        pts.append((A * math.cos(phi), HW + B * math.sin(phi)))
    pts += [(A, z) for z in np.linspace(HW, 0, 8)[1:]]
    p = np.array(pts)
    seg = np.linalg.norm(np.diff(p, axis=0), axis=1)
    s = np.concatenate([[0], np.cumsum(seg)])
    return p, s


P, S = _profile()
T = float(S[-1])


def at(t, off=0.0):
    """Profile point (x, z) at arc length t, pushed `off` outward."""
    t = min(max(t, 0.0), T)
    x = float(np.interp(t, S, P[:, 0]))
    z = float(np.interp(t, S, P[:, 1]))
    if z <= HW + 1e-6:
        n = (-1.0 if t < T / 2 else 1.0, 0.0)
    else:
        nx, nz = x / A ** 2, (z - HW) / B ** 2
        k = math.hypot(nx, nz)
        n = (nx / k, nz / k)
    return x + n[0] * off, z + n[1] * off


def out_dir(c):
    """Outward direction of a point on the shell (for face winding)."""
    return Vector((c.x, 0.0, c.z - 1.5))


def panel(bm, t0, t1, y0, y1, off, rng, segs=1):
    """One bark sheet between arc lengths t0..t1 and y0..y1, `off` outward; its lower edge
    curls out a little."""
    curl = rng.uniform(0.0, 0.04)
    jit = rng.uniform(-0.01, 0.01)
    u0, v0 = rng.random(), rng.random()

    def pt(u, v):
        t = t0 + (t1 - t0) * v
        x, z = at(t, off + jit + curl * (1 - v) ** 2)
        return (x, y0 + (y1 - y0) * u, z)

    sc.grid(bm, pt, 1, segs, outward=out_dir, uv_scale=((y1 - y0) / 2.0, (t1 - t0) / 2.0), uv_offset=(u0, v0))


def cladding(bm, rng):
    """The bark courses on both sides and the cap over the ridge (with the smoke holes)."""
    half = T / 2 - 0.5
    n = max(1, round(half / COURSE))
    ch = half / n
    count = 0
    for side in (0, 1):
        for k in range(n):
            a, b = k * ch - (0.15 if k else 0.0), (k + 1) * ch
            t0, t1 = (a, b) if side == 0 else (T - a, T - b)
            off = 0.03 + 0.03 * k
            segs = 1 if b <= HW else max(1, round((b - a) / 0.45))
            y = -L / 2 - rng.uniform(0.3, 1.5)
            while y < L / 2:
                w = rng.uniform(1.4, 2.0)
                y0, y1 = max(y, -L / 2 - 0.05), min(y + w + 0.08, L / 2 + 0.05)
                if y1 - y0 > 0.2:
                    panel(bm, t0, t1, y0, y1, off, rng, segs)
                    count += 1
                y += w
    # Cap course: over the ridge, open over the smoke holes.
    off = 0.03 + 0.03 * n
    cuts = [(-L / 2 - 0.05, L / 2 + 0.05)]
    for h in SMOKE:
        nxt = []
        for a, b in cuts:
            if a < h - HOLE / 2 < b:
                nxt += [(a, h - HOLE / 2), (h + HOLE / 2, b)]
            else:
                nxt.append((a, b))
        cuts = nxt
    for a, b in cuts:
        y = a
        while y < b - 0.1:
            w = rng.uniform(1.4, 2.0)
            y1 = min(y + w + 0.08, b)
            panel(bm, T / 2 - 0.65, T / 2 + 0.65, y, y1, off, rng, segs=3)
            count += 1
            y += w
    # Lids propped open beside each hole.
    lids = []
    for h in SMOKE:
        panel(bm, T / 2 - 0.7, T / 2 + 0.7, h + 0.1, h + HOLE + 0.25, off + 0.35, rng, segs=3)
        lids.append(h)
        count += 1
    return count, off


def end_wall(bm, hide_bm, y, sign, rng):
    """End wall at `y` facing `sign` (-1: -Y): vertical bark sheets, a door with a hide."""
    edges = [-A, -2.1, -1.2, -0.6, 0.6, 1.2, 2.1, A]
    door_top = 2.1

    def top(x):
        return HW + B * math.sqrt(max(0.0, 1 - (x / A) ** 2)) + 0.02

    for i in range(len(edges) - 1):
        x0, x1 = edges[i], edges[i + 1]
        yy = y + sign * (0.03 + 0.03 * (i % 2))
        z0 = door_top if (x0, x1) == (-0.6, 0.6) else 0.0
        u0, v0 = rng.random(), rng.random()
        segs = 2

        def pt(u, v, x0=x0, x1=x1, z0=z0, yy=yy):
            x = x0 + (x1 - x0) * u
            return (x, yy, z0 + (top(x) - z0) * v)

        sc.grid(bm, pt, segs, 1, outward=(0, sign, 0), uv_scale=((x1 - x0) / 2.0, (top(x0) - z0) / 2.0),
                uv_offset=(u0, v0))

    # The hide over the door, bulging out, its lower edge hanging loose.
    def hp(u, v):
        x = -0.68 + 1.36 * u
        z = 0.05 + (door_top + 0.08 - 0.05) * v
        bulge = 0.07 * math.sin(math.pi * u) * math.sin(math.pi * min(1.0, v * 1.2))
        x += 0.05 * math.sin(3 * math.pi * u) * (1 - v) ** 2
        return (x, y + sign * (0.08 + bulge), z)

    sc.grid(hide_bm, hp, 3, 4, outward=(0, sign, 0), uv_scale=(0.7, 1.1))


def frame(wood_bm, clad_off, rng):
    """Ribs, horizontal poles, ridge pole and end posts."""
    roff = clad_off + 0.08
    ts = sorted(set([0.0, HW * 0.5, HW] + list(np.linspace(HW, T - HW, 15)) + [T - HW, T - HW * 0.5, T]))
    for i in range(RIBS):
        y = -L / 2 + i * L / (RIBS - 1)
        y = y + (0.12 if i == 0 else -0.12 if i == RIBS - 1 else 0.0)
        pts = []
        for t in ts:
            # The ribs bear on the courses beneath: outward by the course stack there.
            x, z = at(t, roff if HW < t < T - HW else 0.03 + 0.03 * min(int(min(t, T - t) / COURSE), 6) + 0.1)
            pts.append((x, y, z))
        sc.tube(wood_bm, pts, 0.055, sides=5, tile=1.5, seed=i)
    poles = 0
    half_arc = T / 2 - HW
    for t in (0.35, 1.25, 2.15, HW + half_arc * 0.3, HW + half_arc * 0.62):
        for tt in (t, T - t):
            off = (roff if t > HW else 0.03 + 0.03 * int(t / COURSE) + 0.1) + 0.1
            x, z = at(tt, off)
            pts = []
            for y in np.linspace(-L / 2 - 0.2, L / 2 + 0.2, 5):
                sag = 0.0 if abs(y) > L / 2 - 0.1 else rng.uniform(-0.03, 0.03)
                pts.append((x + sag, float(y), z + sag))
            sc.tube(wood_bm, pts, 0.045, sides=5, tile=1.5, seed=poles)
            poles += 1
    # The ridge pole, broken at the smoke holes.
    x, z = at(T / 2, roff + 0.08)
    sc.tube(wood_bm, [(x, -L / 2 - 0.2, z), (x, L / 2 + 0.2, z)], 0.05, sides=5, tile=1.5, seed=99)
    poles += 1
    # Lid props: two sticks per lid.
    for h in SMOKE:
        for yy in (h + 0.2, h + HOLE + 0.1):
            for side in (-0.5, 0.5):
                x0, z0 = at(T / 2 + side, clad_off)
                x1, z1 = at(T / 2 + side, clad_off + 0.35)
                sc.tube(wood_bm, [(x0, yy, z0), (x1, yy, z1)], 0.025, sides=4)
    # End-wall posts and cross bars.
    for y, sign in ((-L / 2, -1), (L / 2, 1)):
        yy = y + sign * 0.14
        for x in (-2.1, -1.2, -0.68, 0.68, 1.2, 2.1):
            ztop = HW + B * math.sqrt(max(0.0, 1 - (x / A) ** 2))
            sc.tube(wood_bm, [(x, yy, -0.1), (x, yy, ztop + 0.05)], 0.06, sides=5, tile=1.5)
        sc.tube(wood_bm, [(-0.68, yy, 2.2), (0.68, yy, 2.2)], 0.05, sides=5)
        for z in (0.6, 1.5):
            for xs in ((-A, -0.68), (0.68, A)):
                sc.tube(wood_bm, [(xs[0], yy, z), (xs[1], yy, z)], 0.04, sides=5)
    return poles


def build(ctx):
    rng = random.Random(7)
    bark_rgb, bark_h = sc.bark()
    wood_rgb, wood_h = sc.wood()
    hide_rgb, hide_h = sc.hide()
    m_bark = sc.material("longhouse_bark", bark_rgb, bark_h, roughness=0.92, strength=8.0, double=True)
    m_wood = sc.material("longhouse_wood", wood_rgb, wood_h, roughness=0.85, strength=5.0)
    m_hide = sc.material("longhouse_hide", hide_rgb, hide_h, roughness=0.8, strength=3.0, double=True)

    bark_bm, wood_bm, hide_bm = sc.new_bm(), sc.new_bm(), sc.new_bm()
    panels, clad_off = cladding(bark_bm, rng)
    for y, sign in ((-L / 2, -1), (L / 2, 1)):
        end_wall(bark_bm, hide_bm, y, sign, rng)
    poles = frame(wood_bm, clad_off, rng)
    sc.to_object(bark_bm, "longhouse.bark", m_bark, ctx.root)
    sc.to_object(wood_bm, "longhouse.frame", m_wood, ctx.root)
    sc.to_object(hide_bm, "longhouse.doors", m_hide, ctx.root)

    tris = sc.triangles(ctx.root)
    ctx.root["acr_triangles"] = tris
    ctx.stats = {"triangles": tris, "size_m": sc.size(ctx.root), "panels": panels, "smoke_holes": len(SMOKE),
                 "ribs": RIBS, "poles": poles, "normal_maps": 3}
