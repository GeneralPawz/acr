"""Draped garments (needs `bpy`; docs/clothing.md): the sewing patterns of `garments.py`
cut into triangle meshes (lapels and turnbacks cut into their panel as inlays), placed
around the rest-pose body (torso panels over the shoulders), sewn with Blender's cloth
sewing springs in a gravity-free phase, then settled with gravity and shrink, colliding
with the body and the garments draped before (margins growing outwards); baked, welded
along the seams, single-sided (or solidified), skinned and split into the export layers.

- Measurements and placement frames from the full-resolution MakeHuman body.
- Weights: interpolated from the nearest face of the skin on the vertex's own side; the
  coat skirts and turnbacks get the skirt bones (`rig.SKIRT_COLUMNS`) plus a corrective
  share of the thigh behind them, and the FLOAT attribute `_cloth` (0 at the waist seam, 1
  at the hem: how freely the runtime cloth may move each vertex, docs/clothing.md).
- `part` from the dominant bone, the mesh split at the part borders like the skin, so
  cuts, wounds and decay work as for the shells (docs/assets-humans.md).
- One object per material key and skirt flag (`coat`, `facings`, `coat_skirts`,
  `turnbacks`, ...): the atlas painter paints one key per object.
"""

import math
import time

import bmesh
import bpy
import numpy as np
from mathutils import Vector
from mathutils.bvhtree import BVHTree
from mathutils.geometry import delaunay_2d_cdt

from . import armature as am
from . import body as bd
from . import garments as gm
from . import registry, rig

CAPE_ARM = 0.10  # m below the shoulder joint: the upper arm in a cloak's hull down to here
CAPE_OVER = 0.012  # m: a cloak's hull lies this much further off the layers under it
# m: a layer's neckline counts in the hull this much above the skin's (the unku's neck stood
# above the yacolla's yoke ring and showed white over the shoulders; Drape 5)
CAPE_NECK_OVER = 0.04
# Pose-space drape (docs/clothing.md "Drape 4", "Drape 5"): cloaks are simulated on the body
# with the arms of the unit's own STAND clip (`ch.stand`, set by the family before `dress`;
# `clips.STAND_FREE` without one), then brought back to the A-pose rest by inverse skinning
# with their own weights. Garments listed here (or with `CLOTH[g]["arms_down"]`) are draped so.
ARM_BONES = ("clavicle", "upperarm", "forearm", "hand")
ARMS_DOWN_GARMENTS = {"tilmatli", "frn_cape"}


def arms_down(stand=None):
    """The arm rotations of the stand pose `stand` (clavicles, upper arms, forearms, hands;
    `clips.STAND_FREE` when None): the pose a cloak is draped in."""
    from . import clips

    pose = clips.STAND_FREE if stand is None else stand
    return {k: v for k, v in pose.items() if k != "world" and k.rsplit("_", 1)[0] in ARM_BONES}


ARMS_DOWN = arms_down()  # the default (STAND_FREE's arms)
CAPE_ARM_DOWN = 0.32  # m below the shoulder joint: with the arms down the cloak hangs over the upper arm
POSED_SMOOTH = 8  # Taubin passes over a posed cloak after its sim: one-triangle pleats flattened
ARM_HOMES = ("upperarm", "forearm", "hand", "index", "middle", "ring", "pinky", "thumb")
SPACING = registry.SPACING  # cm (the waistcoat lies under the coat); families add with `registry.garment`
SEW_FRAMES = 45  # sewing phase (no gravity), then the cloth settles
FRAMES = 75
SEW_FORCE = 40.0
WELD = 0.045  # sewn pairs closer than this are merged after the sim (about one triangle)
# Single-sided cloth: the renderer shades back faces (`model.wgsl`, front_facing), and 2-3 mm
# of thickness is far below a pixel at game scale. True adds `solidify` with rims.
SOLIDIFY = False
FINGERS = ("index", "middle", "ring", "pinky", "thumb")
TORSO = ("hips", "spine", "spine1", "spine2", "clavicle_l", "clavicle_r")


# --------------------------------------------------------------------------- measuring

def _nearest_bone(V, S, bones):
    """Name of the nearest bone segment for each point (numpy)."""
    best = np.full(len(V), np.inf)
    out = np.empty(len(V), dtype=object)
    for b in bones:
        a, c = S[b]
        d = c - a
        t = np.clip(((V - a) @ d) / max(float(d @ d), 1e-12), 0.0, 1.0)
        dist = np.linalg.norm(V - (a + t[:, None] * d), axis=1)
        m = dist < best
        best[m] = dist[m]
        out[m] = b
    return out.astype(str)


def _hull2(pts):
    pts = sorted(set(pts))
    if len(pts) < 3:
        return pts

    def cross(o, a, b):
        return (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])

    lo, hi = [], []
    for p in pts:
        while len(lo) >= 2 and cross(lo[-2], lo[-1], p) <= 0:
            lo.pop()
        lo.append(p)
    for p in reversed(pts):
        while len(hi) >= 2 and cross(hi[-2], hi[-1], p) <= 0:
            hi.pop()
        hi.append(p)
    return lo[:-1] + hi[:-1]


def _girth(pts):
    h = _hull2([(round(p[0], 5), round(p[1], 5)) for p in pts])
    return sum(math.dist(a, b) for a, b in zip(h, h[1:] + h[:1])) * 100.0


class Body:
    """Measurements of the rest-pose skin (cm, `garments.MEASURE` keys), landmarks (m) and
    the placement frames of the regions."""

    def __init__(self, ch):
        P = ch.prefix
        segs = am.segments(ch.arm)
        self.seg = {k[len(P):]: (np.array(a[:]), np.array(b[:])) for k, (a, b) in segs.items() if k.startswith(P)}
        S = self.seg
        # The full-resolution MakeHuman body (not the decimated hull): smooth slices.
        from . import mbody

        Q, _faces = mbody.repose(segs, P, ch.build)
        self.V = np.asarray(Q)
        self.home = _nearest_bone(self.V, S, [b for b in rig.BODY_BONES if b in S and not b.startswith(FINGERS)])
        torso = np.isin(self.home, TORSO)
        legs = np.isin(self.home, ("thigh_l", "thigh_r"))
        self.torso_pts = self.V[torso | legs]
        z = self.V[:, 2]

        def slice_girth(mask, z0, w=0.015):
            sel = self.V[mask & (np.abs(z - z0) < w)]
            return _girth([tuple(p[:2]) for p in sel]) if len(sel) > 3 else 0.0

        self.waist_z = 1.04
        self.knee_z = float(S["shin_l"][0][2])
        self.ankle_z = float(S["foot_l"][0][2]) + 0.03
        self.neck_z = float(S["neck"][0][2])
        sh, wr = S["upperarm_l"][0], S["forearm_l"][1]
        m = dict(gm.MEASURE)
        m["chest"] = slice_girth(torso, 1.30)
        m["waist"] = min(slice_girth(torso, zz) for zz in (1.06, 1.08, 1.10))
        m["seat"] = slice_girth(torso | legs, 0.90)
        m["neck"] = slice_girth(self.home == "neck", self.neck_z + 0.03) or m["neck"]
        m["nape_to_waist"] = (self.neck_z + 0.02 - self.waist_z) * 100.0
        m["arm"] = float(np.linalg.norm(wr - sh)) * 100.0 + 4.0
        m["waist_to_knee"] = (self.waist_z - self.knee_z) * 100.0
        m["knee_to_ankle"] = (self.knee_z - self.ankle_z) * 100.0

        def ring(bone, t):
            a, b = S[bone]
            sel = self.V[self.home == bone]
            d = b - a
            tt = ((sel - a) @ d) / float(d @ d)
            near = sel[np.abs(tt - t) < 0.12]
            if len(near) < 4:
                return 0.0
            q = a + np.outer(((near - a) @ d) / float(d @ d), d)
            return float(np.mean(np.linalg.norm(near - q, axis=1))) * 2 * math.pi * 100.0 * 1.05

        m["upper_arm"] = ring("upperarm_l", 0.3) or m["upper_arm"]
        m["wrist"] = ring("forearm_l", 0.92) or m["wrist"]
        m["thigh"] = ring("thigh_l", 0.15) or m["thigh"]
        m["knee"] = ring("shin_l", 0.02) or m["knee"]
        m["calf"] = ring("shin_l", 0.3) or m["calf"]
        m["ankle"] = ring("shin_l", 0.92) or m["ankle"]
        # The decimated hull gives noisy slices: keep each measure within 0.85..1.2 of the
        # default figure.
        for k, d in gm.MEASURE.items():
            m[k] = min(max(m[k], d * 0.8), d * 1.25)
        self.m = m
        # Shoulder fold (torso panels go over the shoulders, `place`): from the fold line
        # the panels run up and over the top of the shoulder; the skin's top per |x|.
        self.fold_z = float(S["upperarm_l"][0][2]) - 0.04
        top = self.V[np.isin(self.home, TORSO + ("neck",)) & (self.V[:, 2] > self.fold_z - 0.05)]
        self.top_x = np.linspace(0.0, 0.30, 31)
        self.top_z = np.array([float(top[np.abs(np.abs(top[:, 0]) - x) < 0.015][:, 2].max())
                               if np.any(np.abs(np.abs(top[:, 0]) - x) < 0.015) else self.fold_z for x in self.top_x])
        self.top_z = np.maximum(self.top_z, self.fold_z)
        self._capes, self._cpts, self._cy, self._over = {}, None, 0.0, None
        self.land = {"waist": self.waist_z, "knee": self.knee_z, "ankle": self.ankle_z, "wrist": None,
                     "collar": self.neck_z + 0.035, "stock": self.neck_z + 0.03}
        # Torso ellipses per height (2 cm steps): centre y, half widths.
        self.ell = {}
        for k in range(30, 80):
            zz = k * 0.02
            sel = self.torso_pts[np.abs(self.torso_pts[:, 2] - max(zz, 0.88)) < 0.02]
            if zz > self.neck_z - 0.02:
                sel = self.V[(self.home == "neck") & (np.abs(z - zz) < 0.02)]
            if len(sel) < 4:
                continue
            # Robust half widths, scaled so that the ellipse's perimeter is the slice's
            # convex girth (armpit outliers would widen it and the panels would not meet).
            lo, hi = np.percentile(sel[:, 1], 2), np.percentile(sel[:, 1], 98)
            cy = 0.5 * (lo + hi)
            a0 = max(float(np.percentile(np.abs(sel[:, 0]), 97)), 0.04)
            b0 = max(0.5 * (hi - lo), 0.04)
            per = math.pi * (3 * (a0 + b0) - math.sqrt((3 * a0 + b0) * (a0 + 3 * b0)))
            f = _girth([tuple(q[:2]) for q in sel]) / 100.0 / per
            self.ell[k] = (cy, a0 * f, b0 * f)

    def cape(self, th, gap):
        """The cloak's hanging line at drape theta `th` (degrees), `gap` m off the skin: the
        upper convex hull of the body (torso, neck below the ring, shoulder caps, thighs)
        seen along the ray from the neck axis, from the neck ring over the shoulder and down,
        then straight down from its outermost point. Returns (s, h, z): arc length from the
        neck ring, distance from the neck axis, height (m)."""
        key = (int(round(th)) % 360, round(gap, 4))
        if key in self._capes:
            return self._capes[key]
        if self._cpts is None:
            zc = self.neck_z + 0.02
            arms = ("upperarm_l", "upperarm_r")
            keep = np.isin(self.home, TORSO + ("neck", "thigh_l", "thigh_r") + arms) & (self.V[:, 2] < zc)
            arm_z = float(self.seg["upperarm_l"][0][2]) - (CAPE_ARM_DOWN if getattr(self, "_rest", None) else CAPE_ARM)
            keep &= ~(np.isin(self.home, arms) & (self.V[:, 2] < arm_z))
            pts = [self.V[keep]]
            if self._over is not None:
                # The layers the cloak lies on (a blouse, a tunic, a hip cloth): their
                # outer surface, `CAPE_OVER` m further out, by the same rule.
                Q, home = self._over
                ok = np.isin(home, TORSO + ("neck", "thigh_l", "thigh_r") + arms) & (Q[:, 2] < zc + CAPE_NECK_OVER)
                ok &= ~(np.isin(home, arms) & (Q[:, 2] < arm_z))
                d = Q[ok, :2] - np.array((0.0, self.ellipse(self.neck_z)[0]))
                r = np.maximum(np.linalg.norm(d, axis=1), 1e-6)[:, None]
                pts.append(np.hstack([Q[ok, :2] + d / r * CAPE_OVER, Q[ok, 2:]]))
            self._cpts = np.vstack(pts)
            self._cy = self.ellipse(self.neck_z)[0]
        t = math.radians(key[0])
        d = np.array((math.sin(t), -math.cos(t)))
        rel = self._cpts[:, :2] - np.array((0.0, self._cy))
        h = rel @ d
        lat = np.abs(rel @ np.array((-d[1], d[0])))
        sel = (h > 0.0) & (lat < np.maximum(0.015, h * 0.12))
        pts = sorted(zip(h[sel].tolist(), self._cpts[sel, 2].tolist()))
        hull = []
        for p in pts:  # upper hull, left to right
            while len(hull) >= 2 and ((hull[-1][0] - hull[-2][0]) * (p[1] - hull[-2][1])
                                      - (hull[-1][1] - hull[-2][1]) * (p[0] - hull[-2][0])) >= 0:
                hull.pop()
            hull.append(p)
        top = max(range(len(hull)), key=lambda i: hull[i][1])
        hull = hull[top:]
        hull.append((hull[-1][0], 0.05))
        H = np.array(hull)
        seg = np.diff(H, axis=0)
        seg /= np.maximum(np.linalg.norm(seg, axis=1)[:, None], 1e-9)
        nrm = np.stack([-seg[:, 1], seg[:, 0]], axis=1)  # up and out
        vn = np.vstack([nrm[:1], nrm[:-1] + nrm[1:], nrm[-1:]])
        vn /= np.maximum(np.linalg.norm(vn, axis=1)[:, None], 1e-9)
        H = H + vn * gap
        sl = np.concatenate([[0.0], np.cumsum(np.linalg.norm(np.diff(H, axis=0), axis=1))])
        self._capes[key] = (sl, H[:, 0], H[:, 1])
        return self._capes[key]

    def pose_arms(self, M):
        """Moves the body's arm points (and the arm bone segments) by the posed bone
        matrices `M` ({unprefixed bone: 4x4}, armature space); `None` restores the rest.
        Clears the cached hanging lines."""
        if M is None:
            if getattr(self, "_rest", None) is not None:
                self.V, self.seg = self._rest
                self._rest = None
        else:
            self._rest = (self.V.copy(), dict(self.seg))
            V = self.V.copy()
            for b, m in M.items():
                if not b.startswith(ARM_HOMES) or b not in self.seg:
                    continue
                sel = self.home == b
                R, t = np.asarray(m)[:3, :3], np.asarray(m)[:3, 3]
                V[sel] = V[sel] @ R.T + t
                a, c = self.seg[b]
                self.seg[b] = (R @ a + t, R @ c + t)
            self.V = V
        self._capes, self._cpts = {}, None

    def cape_over(self, objs):
        """The cloak's hull also covers these rest-pose layers (`dress`: everything made before
        it). Clears the cached hanging lines."""
        qs = [np.array([tuple(o.matrix_world @ v.co) for v in o.data.vertices])
              for o in objs if o.type == "MESH" and len(o.data.vertices)]
        if qs:
            Q = np.vstack(qs)
            self._over = (Q, _nearest_bone(Q, self.seg, [b for b in rig.BODY_BONES
                                                          if b in self.seg and not b.startswith(FINGERS)]))
        else:
            self._over = None
        self._capes, self._cpts = {}, None

    def cape_at(self, th, s, gap, step=3.0):
        """Point (x, y, z) of the cloak's hanging surface at theta `th`, `s` m from the neck
        ring (profiles every `step` degrees, blended)."""
        k0 = math.floor(th / step) * step
        f = (th - k0) / step
        out = np.zeros(3)
        for k, w in ((k0, 1.0 - f), (k0 + step, f)):
            if w <= 0.0:
                continue
            sl, h, z = self.cape(k, gap)
            hh = float(np.interp(s, sl, h))
            zz = float(np.interp(s, sl, z)) - max(0.0, s - sl[-1])
            t = math.radians(k)
            out += w * np.array((hh * math.sin(t), self._cy - hh * math.cos(t), zz))
        return out

    def ellipse(self, z):
        k = min(max(int(round(z / 0.02)), min(self.ell)), max(self.ell))
        while k not in self.ell:
            k += 1
        return self.ell[k]

    def limb_radius(self, bones, p, axis_a, axis_b):
        """Mean skin distance to the axis near the axis point p."""
        sel = self.V[np.isin(self.home, bones)]
        d = axis_b - axis_a
        L = float(np.linalg.norm(d))
        u = d / L
        t = (sel - axis_a) @ u
        tp = float((p - axis_a) @ u)
        near = sel[np.abs(t - tp) < 0.03]
        if len(near) < 3:
            near = sel[np.argsort(np.abs(t - tp))[:8]]
        q = axis_a + np.outer((near - axis_a) @ u, u)
        return float(np.percentile(np.linalg.norm(near - q, axis=1), 80))


# --------------------------------------------------------------------------- panels

def _mesh2d(panel, spacing, inlays=()):
    """Triangulates the outline: boundary samples every `spacing` cm (corners kept),
    interior lattice points; returns (uv[], tris[], segs, owner[]) with segs[k] = the sample
    indices of outline segment k (outline[k] -> outline[k+1], both ends included) and
    owner[t] = the inlay panel a triangle lies in (or None). The outlines of `inlays`
    (lapels, turnbacks: panels in the same drafting frame) are cut into the triangulation as
    constraint edges, so they are part of the same cloth with a crisp outline."""
    pts = panel.outline
    n = len(pts)
    uv, segs = [], []
    for k in range(n):
        a, b = pts[k], pts[(k + 1) % n]
        cnt = max(1, int(round(math.dist(a, b) / spacing)))
        idx = []
        for s in range(cnt):
            f = s / cnt
            uv.append((a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f))
            idx.append(len(uv) - 1)
        segs.append(idx)
    for k in range(n):
        segs[k].append(segs[(k + 1) % n][0])
    nb = len(uv)
    cons = []
    for ip in inlays:
        q = ip.outline
        first = len(uv)
        for k in range(len(q)):
            a, b = q[k], q[(k + 1) % len(q)]
            cnt = max(1, int(round(math.dist(a, b) / spacing)))
            for s_ in range(cnt):
                f = s_ / cnt
                uv.append((a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f))
        last = len(uv)
        cons += [(i, i + 1 if i + 1 < last else first) for i in range(first, last)]
    csegs = [(uv[a], uv[b]) for a, b in cons]
    u0, v0, u1, v1 = panel.bounds()
    h = spacing * math.sqrt(3) / 2
    row = 0
    v = v0 + h * 0.5
    while v < v1:
        u = u0 + (spacing * 0.5 if row % 2 else 0.0) + spacing * 0.5
        while u < u1:
            if _inside((u, v), pts) and min(_seg_dist((u, v), pts[k], pts[(k + 1) % n]) for k in range(n)) > spacing * 0.45                     and all(_seg_dist((u, v), a, b) > spacing * 0.4 for a, b in csegs):
                uv.append((u, v))
            u += spacing
        v += h
        row += 1
    out = delaunay_2d_cdt([Vector(p) for p in uv], cons, [list(range(nb))], 1, 1e-3, True)
    verts, _edges, faces, orig = out[0], out[1], out[2], out[3]
    remap = {}
    for i, o in enumerate(orig):
        for j in o:
            remap.setdefault(j, i)
    segs = [[remap[i] for i in s] for s in segs]
    vs = [(p.x, p.y) for p in verts]
    owner = []
    for f in faces:
        c = (sum(vs[i][0] for i in f) / len(f), sum(vs[i][1] for i in f) / len(f))
        owner.append(next((ip.name for ip in inlays if _inside(c, ip.outline)), None))
    return vs, [tuple(f) for f in faces], segs, owner


def _inside(p, poly):
    x, y = p
    c = False
    for (x0, y0), (x1, y1) in zip(poly, poly[1:] + poly[:1]):
        if (y0 > y) != (y1 > y) and x < x0 + (y - y0) * (x1 - x0) / (y1 - y0):
            c = not c
    return c


def _seg_dist(p, a, b):
    ax, ay = b[0] - a[0], b[1] - a[1]
    L = ax * ax + ay * ay
    t = 0.0 if L < 1e-12 else max(0.0, min(1.0, ((p[0] - a[0]) * ax + (p[1] - a[1]) * ay) / L))
    return math.dist(p, (a[0] + ax * t, a[1] + ay * t))


def _edge_samples(panel, segs, edge):
    i, j = panel.edges[edge]
    n = len(panel.outline)
    out = []
    k = i
    while k != j:
        s = segs[k]
        out += s if not out else s[1:]
        k = (k + 1) % n
    return out


def place(body, panel, uv):
    """3D rest positions (armature space, metres) of the panel's points and their axis
    points (to orient the faces outwards)."""
    pl = panel.place
    reg = pl["region"]
    gap = pl.get("gap", 1.0) / 100.0
    lift = pl.get("lift", 0.0) / 100.0
    th0 = math.radians(pl.get("theta", 0.0))
    sgn = pl.get("dir", 1)
    u0 = pl.get("u0", 0.0)
    out, axis = [], []
    if reg == "cape":
        # A cloak from its yoke (docs/clothing.md "Drape 2"): u round the neck ring from
        # `theta` turning `turn` degrees over `span` cm (towards the wearer's left), v = cm
        # from the neck ring down the hanging line (`Body.cape`).
        th0, turn, span = pl.get("theta", 180.0), pl.get("turn", 180.0), pl["span"]
        for u, v in uv:
            th = th0 - turn * min(max(u / span, 0.0), 1.0)
            p = body.cape_at(th, v / 100.0, gap)
            out.append(tuple(p))
            axis.append((0.0, body._cy, float(p[2]) - 0.05))
        return out, axis
    if reg in ("torso", "neck"):
        z0 = body.land[pl["at"]]
        for u, v in uv:
            z = z0 + lift + v / 100.0
            cy, a, b = body.ellipse(z) if reg == "torso" else body.ellipse(max(z, body.neck_z + 0.02))
            a, b = a + gap, b + gap
            arc = (u - u0) / 100.0 * sgn
            # Arc length on the ellipse from theta0 (numerically).
            ts = th0 + np.linspace(0.0, math.copysign(2 * math.pi, arc if arc else 1.0), 721)
            xs, ys = np.sin(ts) * a, cy - np.cos(ts) * b
            cum = np.concatenate([[0.0], np.cumsum(np.hypot(np.diff(xs), np.diff(ys)))])
            t = float(np.interp(abs(arc), cum, ts))
            p = np.array((math.sin(t) * a, cy - math.cos(t) * b, z))
            ax = np.array((0.0, cy, z))
            if reg == "torso" and z > body.fold_z - 0.08 and pl.get("theta", 0.0) in (0.0, 180.0):
                # Over the shoulder: flat across (x = u), up to the top of the shoulder,
                # then on towards the other side; blended in over 8 cm below the fold.
                side = -1.0 if pl.get("theta", 0.0) == 0.0 else 1.0  # front panels start at -y
                cyf, _af, bf = body.ellipse(body.fold_z)
                x = abs(u) / 100.0
                ztop = float(np.interp(x, body.top_x, body.top_z)) + gap
                y0, z0f = cyf + side * (bf + gap), body.fold_z
                rise = math.hypot(cyf - y0, ztop - z0f)
                e = max(0.0, z - body.fold_z)
                if e <= rise:
                    f = e / max(rise, 1e-6)
                    q = np.array((x, y0 + (cyf - y0) * f, z0f + (ztop - z0f) * f))
                else:
                    q = np.array((x, cyf - side * (e - rise), ztop))
                w = min(max((z - (body.fold_z - 0.08)) / 0.08, 0.0), 1.0)
                w = w * w * (3 - 2 * w)
                p = p * (1 - w) + q * w
                ax = np.array((x * (1 - w), cyf, min(z, body.fold_z) - 0.1 * w))
            out.append(tuple(p))
            axis.append(tuple(ax))
        return out, axis
    if reg == "arm":
        sh, wr = body.seg["upperarm_l"][0], body.seg["forearm_l"][1]
        ea = (wr - sh) / np.linalg.norm(wr - sh)
        up = np.array((0.0, 0.0, 1.0))
        tvec = up - (up @ ea) * ea
        tvec /= np.linalg.norm(tvec)
        fvec = np.array((0.0, -1.0, 0.0))
        for u, v in uv:
            p = wr - ea * (lift + v / 100.0)
            r = body.limb_radius(("upperarm_l", "forearm_l"), p, sh, wr) + gap
            t = th0 + sgn * (u - u0) / 100.0 / r
            q = p + r * (math.cos(t) * tvec + math.sin(t) * fvec)
            out.append(tuple(q))
            axis.append(tuple(p))
        return out, axis
    # leg: thigh above the knee, shin below.
    th_a, th_b = body.seg["thigh_l"]
    sh_a, sh_b = body.seg["shin_l"]
    z0 = body.land[pl["at"]]
    for u, v in uv:
        z = z0 + lift + v / 100.0
        if z >= th_b[2]:
            a, b, bones = th_a, th_b, ("thigh_l", "hips")
        else:
            a, b, bones = sh_a, sh_b, ("shin_l", "foot_l")
        f = (z - a[2]) / (b[2] - a[2])
        p = a + (b - a) * f
        p[2] = z
        if z > th_a[2] - 0.12:  # round the hip: the leg tube widens to the seat
            sel = body.torso_pts[(np.abs(body.torso_pts[:, 2] - z) < 0.02) & (body.torso_pts[:, 0] > -0.01)]
            r = float(np.percentile(np.hypot(sel[:, 0] - p[0], sel[:, 1] - p[1]), 85)) if len(sel) > 4 else 0.12
        else:
            r = body.limb_radius(bones, p, a, b)
        r += gap
        t = th0 + sgn * (u - u0) / 100.0 / r
        out.append((p[0] + math.sin(t) * r, p[1] - math.cos(t) * r, z))
        axis.append((p[0], p[1], z))
    return out, axis


# --------------------------------------------------------------------------- building

class Piece:
    def __init__(self, panel, side, base, n, segs, faces):
        self.panel, self.side, self.base, self.n, self.segs, self.faces = panel, side, base, n, segs, faces

    def edge(self, edge):
        return [self.base + i for i in _edge_samples(self.panel, self.segs, edge)]


def build_garment(body, garment, panels, seams, spacing):
    """One mesh with every panel copy of `garment` placed, its sewing edges loose. Returns
    (verts, faces, face_info [(panel, side)], sew [(i, j)], pieces)."""
    verts, faces, finfo, pieces = [], [], [], {}
    for name, pnl in panels.items():
        if pnl.tack:
            continue  # cut into its base panel
        inl = [q for q in panels.values() if q.tack == name]
        uv, tris, segs, owner = _mesh2d(pnl, spacing, inl)
        pos, axis = place(body, pnl, uv)
        # Faces outward: compare the mean normal with the direction away from the axis.
        P = np.array(pos)
        A = np.array(axis)
        s = 0.0
        for f in tris:
            a, b, c = P[f[0]], P[f[1]], P[f[2]]
            nrm = np.cross(b - a, c - a)
            s += float(nrm @ ((a + b + c) / 3 - (A[f[0]] + A[f[1]] + A[f[2]]) / 3))
        if s < 0:
            tris = [(f[0], f[2], f[1]) for f in tris]
        sides = ("L", "R") if pnl.mirror else ("L",)
        for side in sides:
            base = len(verts)
            if side == "L":
                verts += [tuple(p) for p in pos]
                faces += [tuple(base + i for i in f) for f in tris]
            else:
                verts += [(-p[0], p[1], p[2]) for p in pos]
                faces += [(base + f[0], base + f[2], base + f[1]) for f in tris]
            finfo += [(o or name, side) for o in owner]
            pieces[(name, side)] = Piece(pnl, side, base, len(pos), segs, tris)
            pieces[(name, side)].uv = uv
    sew, sew_id = [], []
    for k_seam, sm in enumerate(seams):
        if any(panels[lk[0]].tack for lk in sm.a + sm.b):
            continue  # inlays are cut into their base panel
        sides = ("L", "R") if all(lk[3] == "L" for lk in sm.a + sm.b) and all(panels[lk[0]].mirror for lk in sm.a + sm.b) else ("L",)
        for side in sides:
            def chain(ch_):
                out, at = [], []
                for name, edge, rev, s in ch_:
                    sd = side if s == "L" else "R"
                    pc = pieces[(name, sd)]
                    idx = pc.edge(edge)
                    if rev:
                        idx = idx[::-1]
                    pts = [pc.uv[i - pc.base] for i in idx]
                    if out and out[-1] == idx[0]:
                        idx, pts = idx[1:], pts[1:]
                    out += idx
                    at += pts
                return out, at
            (A, at_a), (B, at_b) = chain(sm.a), chain(sm.b)
            fa, fb = gm.seam_fractions(at_a), gm.seam_fractions(at_b)
            pairs = set()
            for i, f in zip(A, fa):
                k = min(range(len(B)), key=lambda q: abs(fb[q] - f))
                pairs.add((i, B[k]))
            for j, f in zip(B, fb):
                k = min(range(len(A)), key=lambda q: abs(fa[q] - f))
                pairs.add((A[k], j))
            new = [p for p in pairs if p[0] != p[1]]
            sew += new
            sew_id += [k_seam] * len(new)
    pieces["_sew_id"] = sew_id
    return verts, faces, finfo, sew, pieces


def _collision(obj, on=True, thickness=0.004):
    if on:
        if not any(m.type == "COLLISION" for m in obj.modifiers):
            obj.modifiers.new("acr_collision", "COLLISION")
            obj.collision.thickness_outer = thickness
            obj.collision.thickness_inner = 0.004
            obj.collision.cloth_friction = 5.0
    else:
        for m in [m for m in obj.modifiers if m.type == "COLLISION"]:
            obj.modifiers.remove(m)


def _key(obj, path, pairs):
    for f, x in pairs:
        obj.path_resolve(path.rsplit(".", 1)[0]).__setattr__(path.rsplit(".", 1)[1], x)
        obj.keyframe_insert(data_path=path, frame=f)


def simulate(obj, settings, sew=(), frames=None):
    """Two phases in one run: sewing (no gravity, strong sewing springs, no shrink) for
    `SEW_FRAMES`, then gravity and shrink fade in and the cloth settles."""
    sc = bpy.context.scene
    frames = frames or FRAMES
    mod = obj.modifiers.new("acr_cloth", "CLOTH")
    cs = mod.settings
    cs.quality = 8
    cs.mass = settings["mass"]
    cs.tension_stiffness = cs.compression_stiffness = settings["tension"]
    cs.shear_stiffness = settings["tension"] * 0.4
    cs.bending_stiffness = settings["bending"]
    cs.use_sewing_springs = True
    if obj.vertex_groups.get("acr_pin"):
        cs.vertex_group_mass = "acr_pin"
        cs.pin_stiffness = 1.0
    cs.air_damping = 5.0
    cc = mod.collision_settings
    cc.distance_min = settings.get("margin", 0.004)
    cc.use_self_collision = bool(settings.get("self"))
    cc.self_distance_min = settings.get("self_gap", 0.002)
    cc.collision_quality = 3
    mod.point_cache.frame_start = 1
    mod.point_cache.frame_end = frames
    base = 'modifiers["acr_cloth"].settings.'
    s1 = SEW_FRAMES
    _key(obj, base + "effector_weights.gravity", [(1, 0.0), (s1, 0.0), (s1 + 8, 1.0)])
    _key(obj, base + "sewing_force_max", [(1, 5.0), (s1 // 3, SEW_FORCE), (s1, SEW_FORCE), (s1 + 8, 15.0)])
    _key(obj, base + "shrink_min", [(1, 0.0), (s1, 0.0), (s1 + 8, settings["shrink"])])
    old = sc.frame_current
    t0 = time.time()
    gaps = []
    for f in range(1, frames + 1):
        sc.frame_set(f)
        if f == s1 and sew:
            dg = bpy.context.evaluated_depsgraph_get()
            ev = obj.evaluated_get(dg).data
            gaps = [(ev.vertices[a].co - ev.vertices[b].co).length for a, b in sew]
    dg = bpy.context.evaluated_depsgraph_get()
    me = bpy.data.meshes.new_from_object(obj.evaluated_get(dg))
    dt = time.time() - t0
    obj.modifiers.remove(mod)
    obj.animation_data_clear()
    old_me = obj.data
    obj.data = me
    bpy.data.meshes.remove(old_me)
    sc.frame_set(old)
    return dt, gaps


def weld(obj, sew, limit=0.02, limits=None):
    """Merges the sewn vertex pairs that met (within `limit`, or `limits[i]` per pair, the
    seam's `close`); drops the sewing edges."""
    parent = {}

    def find(x):
        while parent.get(x, x) != x:
            x = parent[x]
        return x

    V = obj.data.vertices
    for i, (a, b) in enumerate(sew):
        if (V[a].co - V[b].co).length < (limits[i] if limits else limit):
            ra, rb = find(a), find(b)
            if ra != rb:
                parent[max(ra, rb)] = min(ra, rb)
    groups = {}
    for v in range(len(V)):
        groups.setdefault(find(v), []).append(v)
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    bm.verts.ensure_lookup_table()
    tmap = {}
    for r, vs in groups.items():
        if len(vs) > 1:
            c = sum((bm.verts[i].co for i in vs), Vector()) / len(vs)
            bm.verts[r].co = c
            for i in vs:
                if i != r:
                    tmap[bm.verts[i]] = bm.verts[r]
    bmesh.ops.delete(bm, geom=[e for e in bm.edges if not e.link_faces], context="EDGES_FACES")
    bmesh.ops.weld_verts(bm, targetmap=tmap)
    bmesh.ops.delete(bm, geom=[v for v in bm.verts if not v.link_faces], context="VERTS")
    bm.to_mesh(obj.data)
    bm.free()
    obj.data.update()


def solidify(obj, thickness):
    """Cloth thickness with rims along the cut edges (the pattern outline)."""
    mod = obj.modifiers.new("acr_solid", "SOLIDIFY")
    mod.thickness = thickness
    mod.offset = 1.0
    mod.use_rim = True
    mod.use_even_offset = True
    dg = bpy.context.evaluated_depsgraph_get()
    me = bpy.data.meshes.new_from_object(obj.evaluated_get(dg))
    obj.modifiers.remove(mod)
    old = obj.data
    obj.data = me
    bpy.data.meshes.remove(old)


# Edge depth (m) of the hems: a rim along the cut edges of the coat skirts, the turnbacks
# and the facings (cuffs, lapels, collar), turned inwards: the broadcloth's turned hem and
# the thickness of the facings show at the edge instead of a paper-thin outline.
HEM = {"coat_skirts": 0.006, "turnbacks": 0.006, "facings": 0.005, "lapels": 0.005}


def hem(obj, depth):
    """A rim `depth` deep along the boundary of `obj`, inwards (no second shell)."""
    mod = obj.modifiers.new("acr_hem", "SOLIDIFY")
    mod.thickness = depth
    mod.offset = -1.0
    mod.use_rim = True
    mod.use_rim_only = True
    mod.use_even_offset = True
    dg = bpy.context.evaluated_depsgraph_get()
    me = bpy.data.meshes.new_from_object(obj.evaluated_get(dg))
    obj.modifiers.remove(mod)
    old = obj.data
    obj.data = me
    me.name = old.name
    bpy.data.meshes.remove(old)


def transfer_weights(obj, flesh, arm_obj, keep=4, under=()):
    """Weights interpolated from the nearest face of the skin (rest pose), or of the layer
    right beneath: `under` are the inner garments already skinned (docs/clothing.md,
    "Layers"). A vertex takes the weights of the surface it rests on, so the two move
    together under any pose and cannot cross."""
    srcs = []
    for o in [flesh, *under]:
        fm = o.data
        if not fm.polygons:
            continue
        tree = BVHTree.FromPolygons([v.co for v in fm.vertices], [p.vertices for p in fm.polygons])
        names = {g.index: g.name for g in o.vertex_groups}
        vw = [{names[e.group]: e.weight for e in v.groups if e.group in names} for v in fm.vertices]
        srcs.append((fm, tree, vw))
    out = []
    for v in obj.data.vertices:
        best = None
        for fm, tree, vw in srcs:
            loc, _n, fi, d = tree.find_nearest(v.co)
            if fi is not None and (best is None or d < best[3]):
                best = (fm, vw, loc, d, fi)
        w = {}
        if best is not None:
            fm, vw, loc, _d, fi = best
            poly = fm.polygons[fi]
            ds = [max((fm.vertices[i].co - loc).length, 1e-5) for i in poly.vertices]
            inv = [1.0 / d for d in ds]
            s = sum(inv)
            for i, k in zip(poly.vertices, inv):
                for b, x in vw[i].items():
                    w[b] = w.get(b, 0.0) + x * k / s
        # Only bones of the vertex's own side (the skin inside the shoes is gone, so a
        # gaiter's tongue could find the other foot): mirror the names if none is left.
        side = "_l" if v.co.x > 0.02 else "_r" if v.co.x < -0.02 else None
        if side:
            other = "_r" if side == "_l" else "_l"
            own = {b: x for b, x in w.items() if not b.endswith(other)}
            w = own or {(b[:-2] + side if b.endswith(other) else b): x for b, x in w.items()}
        top = sorted(w.items(), key=lambda kv: -kv[1])[:keep]
        tot = sum(x for _b, x in top) or 1.0
        out.append({b: x / tot for b, x in top if x / tot > 0.02})
    return out


def skirt_weights(co, prefix=""):
    """Weights of a coat-skirt vertex on the skirt bones (as `body.tube` with columns)."""
    top, bottom = rig.SKIRT_TOP, rig.SKIRT_BOTTOM
    t = min(max((top - co.z) / (top - bottom), 0.0), 1.2)
    ang = math.atan2(co.y, co.x)
    cs = sorted(rig.SKIRT_COLUMNS, key=lambda c: abs(bd._adiff(ang, math.atan2(c[2], c[1]))))[:2]
    d0, d1 = (abs(bd._adiff(ang, math.atan2(c[2], c[1]))) for c in cs)
    w0 = d1 / (d0 + d1) if d0 + d1 > 1e-6 else 1.0
    h = max(0.0, 1.0 - t / 0.3)
    s2 = min(max((t - 0.45) / 0.35, 0.0), 1.0)
    s1 = max(0.0, 1.0 - h - s2)
    w = {prefix + "hips": h}
    for c, wc in ((cs[0], w0), (cs[1], 1.0 - w0)):
        w[f"{prefix}skirt_{c[0]}_01"] = w.get(f"{prefix}skirt_{c[0]}_01", 0.0) + s1 * wc
        w[f"{prefix}skirt_{c[0]}_02"] = w.get(f"{prefix}skirt_{c[0]}_02", 0.0) + s2 * wc
    return {b: x for b, x in w.items() if x > 0.01}, min(t, 1.0)


def _follow_thigh(w, co, t, P=""):
    """Corrective weights: the skirt's lower part follows the thigh behind it partly (the
    clips only move the skirt bones when the body lies down), so a striding leg does not
    pass through the skirt: up to 55 % in front, 30 % behind, little at the back vent."""
    front = min(max(-co.y / 0.08, 0.0), 1.0)
    side = min(max((abs(co.x) - 0.02) / 0.06, 0.0), 1.0)
    a = t * (0.30 + 0.25 * front) * side
    if a <= 0.0:
        return w
    out = {b: x * (1.0 - a) for b, x in w.items()}
    th = P + ("thigh_l" if co.x > 0 else "thigh_r")
    out[th] = out.get(th, 0.0) + a
    return out


ARM_BONES = ("upperarm", "forearm", "hand", "index", "middle", "ring", "pinky", "thumb", "thigh", "shin", "foot")


def cape_weights(obj, ch, body, under=()):
    """A cloak's weights (docs/clothing.md "Drape 2"): the yoke on the shoulders from the
    skin or layer under it, the free part from the trunk only (the arms swing under it, no
    limb bone drags it), below the waist blended into the skirt bones like a coat's skirts.
    Returns (weights, cloth amounts)."""
    P = ch.prefix
    ws = transfer_weights(obj, ch.flesh, ch.arm, under=under)
    out, cl = [], []
    for w, v in zip(ws, obj.data.vertices):
        z = v.co.z
        a = min(max((z - (body.fold_z - 0.04)) / 0.06, 0.0), 1.0) * 0.5  # arm share on the shoulder top
        w = {b: x * (a if b[len(P):].startswith(ARM_BONES) else 1.0) for b, x in w.items()}
        tot = sum(w.values())
        w = {b: x / tot for b, x in w.items()} if tot > 1e-6 else {P + "spine2": 1.0}
        sk, t = skirt_weights(v.co, P)
        f = min(max((rig.SKIRT_TOP + 0.04 - z) / 0.12, 0.0), 1.0)
        if f > 0.0:
            w = {b: w.get(b, 0.0) * (1.0 - f) + sk.get(b, 0.0) * f for b in set(w) | set(sk)}
        out.append({b: x for b, x in w.items() if x > 0.01})
        cl.append(t * f)
    return out, cl


# -- pose-space drape (docs/clothing.md "Drape 4") --------------------------------------

def pose_matrices(ch, spec):
    """{prefixed bone: 4x4 numpy} skinning matrices (posed @ rest inverse, armature space)
    of `spec` on the character's rig; the rig is back at rest afterwards."""
    P = ch.prefix
    am.pose(ch.arm, spec, prefix=P)
    out = {pb.name: np.array(pb.matrix @ pb.bone.matrix_local.inverted()) for pb in ch.arm.pose.bones
           if pb.name.startswith(P)}
    am.pose(ch.arm, {}, prefix=P)
    return out


def _vertex_weights(obj):
    names = {g.index: g.name for g in obj.vertex_groups}
    return [{names[e.group]: e.weight for e in v.groups if e.group in names} for v in obj.data.vertices]


def _blend(M, w):
    """The blended skinning matrix of weights `w` ({bone: weight}); identity without any."""
    tot = sum(x for b, x in w.items() if b in M)
    if tot < 1e-6:
        return np.eye(4)
    return sum(M[b] * (x / tot) for b, x in w.items() if b in M)


def pose_objects(objs, M, weights=None):
    """Writes the linear-blend-skinned positions of `objs` (their vertex groups) into their
    meshes (and shape keys' basis offsets kept). Returns the rest coordinates to restore."""
    saved = []
    for o in objs:
        if o.type != "MESH" or not len(o.data.vertices):
            continue
        me = o.data
        co = np.empty(len(me.vertices) * 3)
        me.vertices.foreach_get("co", co)
        co = co.reshape(-1, 3)
        saved.append((o, co.copy()))
        ws = weights if weights is not None else _vertex_weights(o)
        out = np.array([(_blend(M, w) @ np.append(c, 1.0))[:3] for c, w in zip(co, ws)])
        me.vertices.foreach_set("co", out.ravel())
        me.update()
    return saved


def restore_objects(saved):
    for o, co in saved:
        o.data.vertices.foreach_set("co", co.ravel())
        o.data.update()


def taubin(obj, passes, lam=0.5, mu=-0.53, keep=()):
    """Taubin smoothing (no shrink) of `obj`'s vertices but `keep`; a vertex on the cut edge
    moves along the edge only (with its edge neighbours), so the hem keeps its length."""
    me = obj.data
    n = len(me.vertices)
    co = np.empty(n * 3)
    me.vertices.foreach_get("co", co)
    co = co.reshape(-1, 3)
    bm = bmesh.new()
    bm.from_mesh(me)
    bm.verts.ensure_lookup_table()
    nb = []
    for v in bm.verts:
        if v.is_boundary:
            nb.append([e.other_vert(v).index for e in v.link_edges if e.is_boundary])
        else:
            nb.append([e.other_vert(v).index for e in v.link_edges if e.link_faces])
    bm.free()
    fixed = np.zeros(n, dtype=bool)
    fixed[list(keep)] = True
    for _ in range(passes):
        for f in (lam, mu):
            d = np.array([co[x].mean(axis=0) - co[i] if x else np.zeros(3) for i, x in enumerate(nb)])
            d[fixed] = 0.0
            co = co + f * d
    me.vertices.foreach_set("co", co.ravel())
    me.update()


def unpose(obj, M, weights):
    """Inverse skinning: each vertex of `obj` (posed) brought to the rest pose by the inverse
    of its blended skinning matrix, so that skinning it with `weights` gives the posed shape
    back. Returns the largest move (m)."""
    me = obj.data
    big = 0.0
    for v, w in zip(me.vertices, weights):
        B = _blend(M, w)
        if np.allclose(B, np.eye(4), atol=1e-7):
            continue
        r = np.linalg.solve(B, np.append(np.array(v.co[:]), 1.0))[:3]
        big = max(big, float(np.linalg.norm(r - np.array(v.co[:]))))
        v.co = Vector(r)
    me.update()
    return big


def finish(obj, ch, skirt, weights=None, under=(), cloth=None):
    """Weights, `_cloth`, homes and parts, split at the part borders, bound to the rig.
    `weights`: given per vertex ({bone: weight}) instead of transferred from the skin;
    `under`: inner layers to take the weights from where they are nearer than the skin."""
    P = ch.prefix
    pin = obj.vertex_groups.get("acr_pin")
    if pin:
        obj.vertex_groups.remove(pin)
    if weights is not None:
        ws, cl = weights, list(cloth) if cloth is not None else [0.0] * len(weights)
    elif skirt:
        ws, cl = zip(*(skirt_weights(v.co, P) for v in obj.data.vertices))
        ws = [_follow_thigh(w, v.co, t, P) for w, v, t in zip(ws, obj.data.vertices, cl)]
    else:
        ws = transfer_weights(obj, ch.flesh, ch.arm, under=under)
        cl = [0.0] * len(ws)
    groups = {}
    bones = [P + b for b in rig.BODY_BONES] + [P + f"skirt_{c[0]}_0{k}" for c in rig.SKIRT_COLUMNS for k in (1, 2)]
    bones = [b for b in bones if b in ch.arm.data.bones]
    homes = []
    for i, w in enumerate(ws):
        for b, x in w.items():
            g = groups.get(b) or obj.vertex_groups.get(b) or obj.vertex_groups.new(name=b)
            groups[b] = g
            g.add([i], x, "REPLACE")
        h = max(w, key=w.get) if w else P + "hips"
        homes.append(h if h in bones else P + "hips")
    a = obj.data.attributes.new("home", "INT", "POINT")
    for i, h in enumerate(homes):
        a.data[i].value = bones.index(h)
    obj["acr_home_bones"] = bones
    c = obj.data.attributes.new("_cloth", "FLOAT", "POINT")
    for i, x in enumerate(cl):
        c.data[i].value = x
    bd.split_parts(obj, lambda b: rig.part_of_bone(b[len(P):]))
    for p in obj.data.polygons:
        p.use_smooth = True
    return am.bind(obj, ch.arm)


def dress(ch, garments=gm.ORDER, report=None, patterns=None, over=()):
    """Drapes the garments `garments` (in order: each rests on those before) on character
    `ch` (rest pose) and adds them to its `clothes` layer. A garment is drafted by
    `patterns[name]` when given (a family's one-off), else by `garments.GARMENTS` (the
    uniform and what families registered with `registry.garment`). `over`: clothes made
    before (shells, modelled flaps) the garments rest on: they collide in the sim and lend
    their weights like the draped layers beneath (docs/clothing.md, "Layers"). Returns
    {object name: material key}."""
    # A unit's own set (`build["drape"]`: garment names in drape order, e.g. the 17th
    # century's hose, slops, doublet) replaces the set its family's `make` asks for.
    plan = (getattr(ch, "build", None) or {}).get("drape")
    if plan:
        garments = tuple(plan)
    body = Body(ch)
    uni = gm.uniform(body.m, garments, patterns)
    colliders = [ch.flesh] + ([ch.shoes] if getattr(ch, "shoes", None) else []) + list(over)
    for o in colliders:
        _collision(o)
    made = {}
    under = list(over)  # skinned inner layers (not the free skirts): the next garment rests on them
    stats = report if report is not None else {}
    stats["measure"] = {k: round(v, 1) for k, v in body.m.items()}
    for g in garments:
        panels, seams = uni[g]
        cape = any(p.place.get("region") == "cape" for p in panels.values())
        # Pose-space drape: the collision body and the layers beneath with the arms down.
        posed = cape and (g in ARMS_DOWN_GARMENTS or gm.CLOTH.get(g, {}).get("arms_down"))
        if posed:
            M = pose_matrices(ch, arms_down(getattr(ch, "stand", None)))
            saved = pose_objects(list(dict.fromkeys(colliders + [o for o in ch.layers["clothes"] if o.name in made])), M)
            body.pose_arms({k[len(ch.prefix):]: v for k, v in M.items()})
        if cape:
            body.cape_over(list(over) + [o for o in ch.layers["clothes"] if o.name in made])
        verts, faces, finfo, sew, pieces = build_garment(body, g, panels, seams, SPACING.get(g, 3.5))
        me = bpy.data.meshes.new(f"{ch.name}.{g}.drape")
        me.from_pydata(verts, [tuple(e) for e in sew], faces)
        obj = bd.link(bpy.data.objects.new(f"{ch.name}.{g}.drape", me))
        # Pinned edges (`place["pin"]`: the gaiters' garters) stay where they were placed.
        seam_v = {x for pr in sew for x in pr}
        # Only the middle half of the edge: its ends must be free to close their seams.
        pins = []
        for key, pc in pieces.items():
            if isinstance(key, tuple) and pc.panel.place.get("pin"):
                e = pc.edge(pc.panel.place["pin"])
                pins += [i for i in e[len(e) // 4:len(e) - len(e) // 4] if i not in seam_v]
            if isinstance(key, tuple) and pc.panel.place.get("pin_v"):
                # A cloak's yoke: everything within `pin_v` cm of the neck ring.
                pins += [pc.base + i for i, (_u, v) in enumerate(pc.uv) if v <= pc.panel.place["pin_v"]]
        if pins:
            obj.vertex_groups.new(name="acr_pin").add(pins, 1.0, "REPLACE")
        fa = me.attributes.new("panel", "INT", "FACE")
        names = list(panels)
        # from_pydata keeps the face order.
        for i, (n, _s) in enumerate(finfo):
            fa.data[i].value = names.index(n)
        dt, gaps_sewn = 0.0, []
        if not g.startswith("shirt"):
            dt, gaps_sewn = simulate(obj, gm.CLOTH.get(g, gm.CLOTH["waistcoat"]), sew)
        ev = obj.data
        longest = max(((ev.vertices[e.vertices[0]].co - ev.vertices[e.vertices[1]].co).length
                       for e in ev.edges if not e.is_loose), default=0.0)
        per_seam = {}
        for (a, b), k in zip(sew, pieces["_sew_id"]):
            lab = "+".join(x[0] + "." + x[1] for x in seams[k].a) + "|" + "+".join(x[0] + "." + x[1] for x in seams[k].b)
            per_seam[lab] = max(per_seam.get(lab, 0.0), (ev.vertices[a].co - ev.vertices[b].co).length)
        gaps = sorted((ev.vertices[a].co - ev.vertices[b].co).length for a, b in sew)
        base = WELD if not g.startswith("shirt") else 0.3
        limits = [getattr(seams[k], "close", None) or base for k in pieces["_sew_id"]]
        welded = sum(1 for (a, b), lim in zip(sew, limits) if (ev.vertices[a].co - ev.vertices[b].co).length < lim)
        weld(obj, sew, limit=base, limits=limits)
        unposed = 0.0
        if posed:
            # Weights from the posed skin and layers beneath, then back to the rest pose by
            # their inverse; kept in the vertex groups (and `_capecl`) for the parts.
            if POSED_SMOOTH:
                pin = obj.vertex_groups.get("acr_pin")
                keep = [v.index for v in obj.data.vertices if pin and any(e.group == pin.index for e in v.groups)]
                taubin(obj, POSED_SMOOTH, keep=keep)
            cw, ccl = cape_weights(obj, ch, body, under=under)
            unposed = unpose(obj, M, cw)
            for i, w in enumerate(cw):
                for b, x in w.items():
                    (obj.vertex_groups.get(b) or obj.vertex_groups.new(name=b)).add([i], x, "REPLACE")
            ca = obj.data.attributes.new("_capecl", "FLOAT", "POINT")
            ca.data.foreach_set("value", list(ccl))
            restore_objects(saved)
            body.pose_arms(None)
            if cape:
                body.cape_over(list(over) + [o for o in ch.layers["clothes"] if o.name in made])
        if SOLIDIFY:
            solidify(obj, gm.CLOTH.get(g, gm.CLOTH["waistcoat"])["thickness"])
        # One object per (key, skirt).
        kinds = {}
        new_under = []  # the parts of one garment rest on the layers before it, not on each other
        for n, pnl in panels.items():
            kinds.setdefault((pnl.key, pnl.skirt), []).append(names.index(n))
        both = {k for k, sk in kinds if sk} & {k for k, sk in kinds if not sk}
        for (key, skirt), idx in kinds.items():
            part = obj.copy()
            part.data = obj.data.copy()
            bd.link(part)
            bm = bmesh.new()
            bm.from_mesh(part.data)
            lay = bm.faces.layers.int.get("panel")
            bmesh.ops.delete(bm, geom=[f for f in bm.faces if f[lay] not in idx], context="FACES")
            bmesh.ops.delete(bm, geom=[v for v in bm.verts if not v.link_faces], context="VERTS")
            bm.to_mesh(part.data)
            bm.free()
            part.data.attributes.remove(part.data.attributes["panel"])
            label = {("coat", True): "coat_skirts", ("lining", True): "turnbacks"}.get(
                (key, skirt), f"{key}_skirts" if skirt and key in both else key)
            part.name = part.data.name = f"{ch.name}.{label}"
            if label in HEM:
                hem(part, HEM[label])
            if posed:
                bones = set(ch.arm.data.bones.keys())
                cw = [{b: x for b, x in w.items() if b in bones} for w in _vertex_weights(part)]
                ccl = [d.value for d in part.data.attributes["_capecl"].data]
                part.data.attributes.remove(part.data.attributes["_capecl"])
                for vg in [vg for vg in part.vertex_groups if vg.name in bones]:
                    part.vertex_groups.remove(vg)
                part["acr_posed"] = 1  # draped with the arms down: the A-pose arm is under it
                finish(part, ch, skirt, weights=cw, under=under, cloth=ccl)
            elif any(panels[names[i]].place["region"] == "cape" for i in idx):
                cw, ccl = cape_weights(part, ch, body, under=under)
                finish(part, ch, skirt, weights=cw, under=under, cloth=ccl)
            else:
                finish(part, ch, skirt, under=under)
            ch.add("clothes", part, key, symmetric=True)
            made[part.name] = key
            if not skirt:
                new_under.append(part)
        bpy.data.objects.remove(obj)
        under += new_under

        def mm(xs, q):
            return round(1000 * xs[min(len(xs) - 1, int(q * len(xs)))], 1) if xs else 0.0

        gs = sorted(gaps_sewn)
        stats[g] = {"unposed_m": round(unposed, 3), "sim_s": round(dt, 2), "sew": len(sew), "welded": welded, "verts_sim": len(verts),
                    "longest_edge_m": round(longest, 3), "gap_mm_p95": mm(gaps, 0.95), "gap_mm_max": mm(gaps, 1.0),
                    "gap_mm_max_after_sewing": mm(gs, 1.0), "within_5mm": sum(1 for x in gaps if x < 0.005),
                    "worst_seams_mm": {k: round(v * 1000) for k, v in sorted(per_seam.items(), key=lambda kv: -kv[1])[:5]}}
        for o in ch.layers["clothes"]:
            if o.name in made:
                _collision(o, thickness=gm.CLOTH.get(g, gm.CLOTH["waistcoat"]).get("margin", 0.004))
    for o in list(ch.layers["clothes"]) + [ch.flesh, *over]:
        _collision(o, False)
    return made


def hide_skin(ch, garments, depth=0.012, reach=0.06):
    """Pushes the skin that lies under the garments `depth` towards the bones (it stays for
    decay and cuts but cannot poke through)."""
    fm = ch.flesh.data

    def tree_of(objs):
        polys, verts = [], []
        for o in objs:
            base = len(verts)
            verts += [v.co.copy() for v in o.data.vertices]
            polys += [[base + i for i in p.vertices] for p in o.data.polygons]
        return BVHTree.FromPolygons(verts, polys) if polys else None

    tree = tree_of(garments)
    if tree is None:
        return 0
    # A cloak draped with the arms down (`acr_posed`) lies through the A-pose arm at rest:
    # the arm's skin is not under it in any clip that shows the arm.
    arm_tree = tree_of([o for o in garments if not o.get("acr_posed")])
    seg = am.segments(ch.arm)
    bones = [ch.prefix + b for b in rig.BODY_BONES if not b.startswith(("hand", "head", "index", "middle", "ring",
                                                                        "pinky", "thumb"))]
    n = 0
    hs = bd.homes(ch.flesh)
    for v, h in zip(fm.vertices, hs):
        if h not in bones or h.endswith(("neck",)):
            continue
        t = arm_tree if h[len(ch.prefix):].startswith(ARM_HOMES) else tree
        hit = t.ray_cast(v.co + v.normal * 0.001, v.normal, reach) if t else (None,)
        if hit[0] is None:
            continue
        q = bd.seg_point(v.co, *seg[h])
        d = v.co - q
        if d.length > depth * 2:
            v.co = q + d * ((d.length - depth) / d.length)
            n += 1
    fm.update()
    return n


def hide_under(ch, inner, outer, depth=0.006, reach=0.05):
    """Pushes the vertices of garment `inner` that lie under the garments `outer` `depth`
    towards their bone, as `hide_skin` does for the skin: a covered layer keeps a margin
    below the one above it under skinning. Returns the number moved."""
    polys, verts = [], []
    for o in outer:
        base = len(verts)
        verts += [v.co.copy() for v in o.data.vertices]
        polys += [[base + i for i in p.vertices] for p in o.data.polygons]
    if not polys:
        return 0
    tree = BVHTree.FromPolygons(verts, polys)
    seg = am.segments(ch.arm)
    n = 0
    me = inner.data
    me.update()
    for v, h in zip(me.vertices, bd.homes(inner)):
        if h not in seg or h.endswith("neck"):
            continue
        hit = tree.ray_cast(v.co + v.normal * 0.001, v.normal, reach)
        if hit[0] is None:
            continue
        # Single-sided cloth: the normal may point inwards; only push towards the bone.
        q = bd.seg_point(v.co, *seg[h])
        d = v.co - q
        if d.length > depth * 3:
            v.co = q + d * ((d.length - depth) / d.length)
            n += 1
    me.update()
    return n


SETTLE_MARGIN = 0.004  # m: an inner layer ends at least this far below the layer on it
SETTLE_POKE = 0.015  # m: inner vertices up to this far out through an outer layer are pulled in
SETTLE_SEAM = 0.0015  # m: an outer surface this close to the vertex is the same cloth (an inlay's border)
SETTLE_SKIP = ("head", "neck", "hand", "index", "middle", "ring", "pinky", "thumb", "foot", "toe")
SETTLE_NOT_OVER = ("brass",)  # outer keys nothing is settled under
SETTLE_ROUNDS = 3  # passes (a moved vertex may come under another outer triangle)
SETTLE_SPREAD = 0.5  # share of the mean move of its moved neighbours an unmoved vertex follows


_OUTWARD = {}


def _outward(o, seg):
    """Per polygon of `o`: +1 if its normal (winding) points away from the nearest body bone,
    -1 if it is turned inwards."""
    if o.name in _OUTWARD and len(_OUTWARD[o.name]) == len(o.data.polygons):
        return _OUTWARD[o.name]
    mw = o.matrix_world
    out = []
    for p in o.data.polygons:
        c = mw @ p.center
        n = (mw.to_3x3() @ p.normal)
        q = min((bd.seg_point(c, *s) for s in seg.values()), key=lambda x: (c - x).length)
        out.append(1.0 if n.dot(c - q) >= 0.0 else -1.0)
    _OUTWARD[o.name] = out
    return out


def orient_outwards(ch, objs, seg):
    """Turns the winding of every cloth polygon whose normal points towards the nearest bone
    outwards (the right panels are mirrored copies of the left, so their winding came out
    inwards, and so did whole capes and flaps). The renderer draws both sides, but the shading
    and the layer check's above/below (`acr_render::layers`) read the winding: a cloth turned
    inwards counted the layer under it as poking through. Returns {name: polygons turned}."""
    turned = {}
    for o in objs:
        side = _outward(o, seg)
        bad = [i for i, x in enumerate(side) if x < 0]
        if not bad:
            continue
        bm = bmesh.new()
        bm.from_mesh(o.data)
        bm.faces.ensure_lookup_table()
        bmesh.ops.reverse_faces(bm, faces=[bm.faces[i] for i in bad], flip_multires=False)
        bm.to_mesh(o.data)
        bm.free()
        o.data.update()
        _OUTWARD[o.name] = [1.0] * len(side)
        turned[o.name] = len(bad)
    return turned


def settle(ch, objs=None, margin=SETTLE_MARGIN, poke=SETTLE_POKE, rounds=SETTLE_ROUNDS):
    """Every ranked layer under the ranked layers above it (`garments.LAYER_RANK`), for all
    families (docs/clothing.md, "Layers"). Top layer first: for each vertex of an inner layer
    the nearest triangle of every outer layer is found; where the vertex lies above it (up to
    `poke`: a shirt through a cloak, a doublet over the falling band) or less than `margin`
    under it, it moves along that triangle's outward normal to `margin` below it, the same
    measure as the layer check (`acr_render::layers`, `POKE`). Unmoved neighbours follow
    half the mean move (no spikes), `rounds` passes. Outer meshes turned inwards are turned
    out first (`orient_outwards`). Openings (nothing above) keep their place; the free
    skirts are not moved (they swing); the head, neck, hands and feet of the skin neither
    (the face seam, the fingers). Returns {inner key: vertices moved}."""
    _OUTWARD.clear()
    objs = objs if objs is not None else ch.layers["flesh"] + ch.layers["clothes"] + ch.layers["gear"]
    body = {ch.prefix + x for x in rig.BODY_BONES}
    every = {b: s for b, s in am.segments(ch.arm).items() if b in body}
    skin_seg = {b for b in every if not b[len(ch.prefix):].startswith(SETTLE_SKIP)}
    ranked = []
    for o in objs:
        if o.type != "MESH" or len(o.get("acr_material_keys") or ()) > 1:
            continue
        rank = gm.LAYER_RANK.get(ch.keys.get(o.name))
        if rank is not None and len(o.data.polygons):
            ranked.append((rank, o))
    # Draped cloth only: gear (boxes, pouches, belts) is closed, its faces at the body face it.
    clothes = {o.name for o in ch.layers["clothes"]}
    cloth = [o for r, o in ranked if r > 0 and o.name in clothes]
    turned = orient_outwards(ch, cloth, every)
    if turned:
        print(f"[{ch.name}] polygons turned outwards: {turned}")
    ranked.sort(key=lambda x: -x[0])
    moved = {}
    for r, o in ranked:
        # Buttons (brass) are small domes on the cloth, not a layer to sink it under.
        outer = [x for rx, x in ranked if rx > r and ch.keys.get(x.name) not in SETTLE_NOT_OVER]
        if not outer or o.name.endswith(("_skirts", "turnbacks")) or "skirt" in o.name:
            continue
        trees = []
        for x in outer:
            mw = x.matrix_world
            trees.append((BVHTree.FromPolygons([mw @ v.co for v in x.data.vertices],
                                               [tuple(p.vertices) for p in x.data.polygons]),
                          _outward(x, every), bool(x.get("acr_posed"))))
        me = o.data
        # Under a cloak draped with the arms down the A-pose arm (skin or sleeve) is not settled.
        armv = set()
        if any(t[2] for t in trees):
            if "home" in me.attributes and o.get("acr_home_bones"):
                armv = {i for i, h in enumerate(bd.homes(o)) if h[len(ch.prefix):].startswith(ARM_HOMES)}
        mw, inv = o.matrix_world, o.matrix_world.inverted()
        keep = set()
        if r == 0:
            # The skin's head, neck, hands and feet stay (the face seam, the fingers).
            for i, v in enumerate(me.vertices):
                co = mw @ v.co
                near = min(every, key=lambda x: (co - bd.seg_point(co, *every[x])).length)
                if near not in skin_seg:
                    keep.add(i)
        nbrs = [[] for _ in me.vertices]
        for e in me.edges:
            a, b = e.vertices
            nbrs[a].append(b)
            nbrs[b].append(a)
        done = set()
        for _ in range(rounds):
            delta = {}
            for i, v in enumerate(me.vertices):
                if i in keep:
                    continue
                co = mw @ v.co
                best = None
                for t, side, posed in trees:
                    if posed and i in armv:
                        continue
                    loc, n, fi, dist = t.find_nearest(co, poke)
                    if loc is None or dist < SETTLE_SEAM:
                        continue
                    n = n * side[fi]
                    h = (co - loc).dot(n)
                    # Over the triangle (not beside an edge), above it or barely under it.
                    if dist - abs(h) > 0.003 or not (-margin < h < poke):
                        continue
                    if best is None or h > best[0]:
                        best = (h, n)
                if best is not None:
                    delta[i] = -best[1] * (best[0] + margin)
            if not delta:
                break
            spread = {}
            for i in delta:
                for j in nbrs[i]:
                    if j not in delta and j not in keep:
                        spread.setdefault(j, []).append(delta[i])
            for j, ds in spread.items():
                delta[j] = sum(ds, Vector()) * (SETTLE_SPREAD / len(ds))
            # With shape keys the export takes each key's own positions, not `v.co`: move
            # every key by the same offset (a relative key keeps its difference to the basis).
            keys = me.shape_keys.key_blocks if me.shape_keys else ()
            rot = inv.to_3x3()
            for i, d in delta.items():
                ld = rot @ d
                me.vertices[i].co += ld
                for kb in keys:
                    kb.data[i].co += ld
            done |= {i for i, d in delta.items() if d.length > 1e-5}
        me.update()
        if done:
            key = ch.keys.get(o.name)
            moved[key] = moved.get(key, 0) + len(done)
    return moved


# -- Per-unit layer order and hidden faces (#287 trisoup3, docs/clothing.md "Layers") -----

# Keys worn over whatever cloth or belt they meet, whatever the drape's votes say: collars,
# falling bands, sashes, capes and mantles (a buff coat poking through the band at the
# collar must not turn the band under it). Gear boxes (rank 8) still go by the votes.
OVER = ("fallband", "band", "lapels", "sash", "fsash", "pue_sash", "fcloak", "cloak", "cappa", "tilmatli",
        "yacolla", "pue_blanket", "lace")
ORDER_SAMPLES = 400  # outward normals per layer sampled for the votes
ORDER_REACH = 0.03  # m: another layer this far under or over a face is the layer it lies on
ORDER_MIN = 6  # votes a pair needs before the drape orders it
ORDER = False  # `layer_order` before the morphs: the unit's own `acr_layer` (off: see docs/clothing.md "Layers")
CULL = False  # `cull_hidden` before the morphs (off: no visible change on the trisoup3 sample)
CULL_DEPTH = 0.025  # m: a covering layer may lie this far *under* the face (it pokes through)
CULL_REACH = 0.05  # m: ... or this far over it
CULL_MARGIN = 0.015  # m: the ring round the face that must be covered too (a margin at openings)
CULL_RING = 8  # ring samples


def _layer_objs(ch):
    """[(key, object)] of the single-material layers with a base rank."""
    out = []
    for o in ch.layers["flesh"] + ch.layers["clothes"] + ch.layers["gear"]:
        if o.type != "MESH" or len(o.get("acr_material_keys") or ()) > 1 or not len(o.data.polygons):
            continue
        k = ch.keys.get(o.name)
        if k in gm.LAYER_RANK:
            out.append((k, o))
    return out


def _tree(objs, seg):
    """One BVH over `objs` (world space) and the outward sign of each of its polygons."""
    vs, ps, side = [], [], []
    for o in objs:
        mw = o.matrix_world
        base = len(vs)
        vs += [mw @ v.co for v in o.data.vertices]
        ps += [tuple(base + i for i in p.vertices) for p in o.data.polygons]
        side += _outward(o, seg)
    return BVHTree.FromPolygons(vs, ps), side


def _outward_normals(o, seg):
    mw = o.matrix_world
    rot = mw.to_3x3()
    return [((rot @ p.normal).normalized() * s, mw @ p.center) for p, s in zip(o.data.polygons, _outward(o, seg))]


def _body_segments(ch):
    body = {ch.prefix + x for x in rig.BODY_BONES}
    return {b: s for b, s in am.segments(ch.arm).items() if b in body}


def layer_order(ch):
    """The unit's own layer order, from its build: for every pair of layers that lie on each
    other, sampled faces of each look `ORDER_REACH` under and over themselves along their
    outward normal for the other; the majority says which is outside. Keys in `OVER` win
    over cloth and belts; the skin is under everything. The pairs go strongest first (one
    closing a cycle is dropped) and each key's rank is the longest chain under it (skin 0);
    keys that touch nothing keep `garments.LAYER_RANK`. Returns {key: rank} and sets
    `ch.layer_rank`; the materials carry it as the `acr_layer` extra (the shader's depth
    bias and `acr_render::layers`)."""
    _OUTWARD.clear()
    seg = _body_segments(ch)
    objs = _layer_objs(ch)
    keys = sorted({k for k, _o in objs}, key=lambda k: (gm.LAYER_RANK[k], k))
    by_key = {k: [o for kk, o in objs if kk == k] for k in keys}
    trees = {k: _tree(by_key[k], seg) for k in keys}
    votes = {}  # (under, over) -> n
    for k in keys:
        samples = []
        for o in by_key[k]:
            samples += _outward_normals(o, seg)
        samples = samples[:: max(1, len(samples) // ORDER_SAMPLES)]
        for k2 in keys:
            if k2 == k:
                continue
            t, _side = trees[k2]
            for n, c in samples:
                loc, _hn, _fi, d = t.ray_cast(c - n * ORDER_REACH, n, 2 * ORDER_REACH)
                if loc is None or abs(d - ORDER_REACH) < 0.0005:
                    continue
                pair = (k2, k) if d < ORDER_REACH else (k, k2)
                votes[pair] = votes.get(pair, 0) + 1
    edges = []
    for i, a in enumerate(keys):
        for b in keys[i + 1:]:
            up, down = votes.get((a, b), 0), votes.get((b, a), 0)
            # Buttons (brass) are domes on the cloth, not a layer: they keep their base rank.
            if not up + down or a in SETTLE_NOT_OVER or b in SETTLE_NOT_OVER:
                continue
            if a == "skin" or b == "skin":
                edges.append((1e9, "skin", b if a == "skin" else a))
                continue
            gear = gm.LAYER_RANK[a] >= 8 or gm.LAYER_RANK[b] >= 8
            if (a in OVER) != (b in OVER) and not gear:
                o = a if a in OVER else b
                edges.append((1e8, b if o == a else a, o))
                continue
            if up + down < ORDER_MIN:
                continue
            u, o = (a, b) if up >= down else (b, a)
            strength = abs(up - down) / (up + down)
            if strength < 0.2:  # split votes: the base order
                u, o = (a, b) if gm.LAYER_RANK[a] <= gm.LAYER_RANK[b] else (b, a)
            edges.append((strength * (up + down), u, o))
    edges.sort(key=lambda e: -e[0])
    above = {k: set() for k in keys}

    def reaches(x, y):
        stack, seen = [x], set()
        while stack:
            z = stack.pop()
            if z == y:
                return True
            if z not in seen:
                seen.add(z)
                stack += list(above[z])
        return False

    kept = []
    for _s, u, o in edges:
        if not reaches(o, u):
            above[u].add(o)
            kept.append((u, o))
    rank = {k: (0 if k == "skin" else None) for k in keys}
    for _ in range(len(keys) + 1):
        for k in keys:
            under = [rank[u] for u, o in kept if o == k]
            if under and all(r is not None for r in under):
                rank[k] = max(under) + 1
    for k in keys:
        if rank[k] is None:
            rank[k] = gm.LAYER_RANK[k]
    ch.layer_rank = rank
    ch.layer_votes = {f"{u}<{o}": votes.get((u, o), 0) for u, o in kept}
    return rank


def cull_hidden(ch, rank=None, depth=CULL_DEPTH, reach=CULL_REACH, margin=CULL_MARGIN):
    """Deletes the garment faces nobody can see (docs/clothing.md "Layers"): a face of a
    cloth layer (not the skin, which wounds and decay need) goes when its corners, its centre
    and a ring `margin` round its centre all find a layer ranked above it (`layer_order`)
    along the face's outward normal, from `depth` under the face (a layer poking through it
    covers it as well) to `reach` over it, turned the same way (not a sleeve's inside seen
    from the torso). The free skirts and turnbacks neither cover (they swing) nor are culled;
    the layers above follow the one under them by their weights, so what is covered at rest
    stays covered. Returns {key: faces deleted}."""
    rank = rank or getattr(ch, "layer_rank", None) or layer_order(ch)
    _OUTWARD.clear()
    seg = _body_segments(ch)
    clothes = {o.name for o in ch.layers["clothes"]}
    objs = [(k, o) for k, o in _layer_objs(ch) if k not in SETTLE_NOT_OVER]

    def free(o):
        return "skirt" in o.name or o.name.endswith("turnbacks")

    out = {}
    for k, o in objs:
        if rank.get(k, 0) <= 0 or o.name not in clothes or free(o):
            continue
        cover = [x for kx, x in objs if rank.get(kx, 0) > rank[k] and not free(x)]
        if not cover:
            continue
        t, side = _tree(cover, seg)
        mw = o.matrix_world
        rot = mw.to_3x3()
        verts = [mw @ v.co for v in o.data.vertices]

        def covered(p, n, t=t, side=side):
            loc, hn, fi, _d = t.ray_cast(p - n * depth, n, depth + reach)
            return loc is not None and (hn * side[fi]).dot(n) > 0.3

        ring = [(math.cos(2 * math.pi * j / CULL_RING), math.sin(2 * math.pi * j / CULL_RING)) for j in range(CULL_RING)]
        dead = []
        for p, s in zip(o.data.polygons, _outward(o, seg)):
            n = (rot @ p.normal).normalized() * s
            c = mw @ p.center
            a = n.orthogonal().normalized()
            b = n.cross(a)
            pts = [c] + [verts[i] for i in p.vertices] + [c + (a * x + b * y) * margin for x, y in ring]
            if all(covered(q, n) for q in pts):
                dead.append(p.index)
        if not dead:
            continue
        bm = bmesh.new()
        bm.from_mesh(o.data)
        bm.faces.ensure_lookup_table()
        bmesh.ops.delete(bm, geom=[bm.faces[i] for i in dead], context="FACES")
        bm.to_mesh(o.data)
        bm.free()
        o.data.update()
        _OUTWARD.pop(o.name, None)
        out[k] = out.get(k, 0) + len(dead)
    return out


RELAX = False  # `relax_edges` before the morphs (off: smoother capes but opened seams on the trisoup3 sample)
RELAX_EAR = 22.0  # degrees: a triangle hanging off the edge (two free edges) this sharp is a spike
RELAX_ROUNDS = 10  # Taubin passes along each free edge loop
RELAX_LAMBDA, RELAX_MU = 0.5, -0.53


def relax_edges(ch, objs=None):
    """Evens the free edges of the draped cloth (docs/clothing.md "Layers"): the drape's 4-5 cm
    triangles end in a zigzag, and where such an edge lies on the layer below, each tooth
    pokes in and out. First the spikes go (a triangle with two free edges and an angle under
    `RELAX_EAR`), then every free-edge vertex is smoothed along its loop (Taubin: no
    shrinking), its inner neighbours following half the move. Shape keys move with it.
    Returns {key: (spikes removed, edge vertices moved)}."""
    clothes = {o.name for o in ch.layers["clothes"]}
    objs = objs if objs is not None else [o for k, o in _layer_objs(ch) if o.name in clothes and k != "skin"
                                          and k not in SETTLE_NOT_OVER]
    out = {}
    for o in objs:
        bm = bmesh.new()
        bm.from_mesh(o.data)
        spikes = 0
        for _ in range(2):
            ears = []
            for f in bm.faces:
                if len(f.verts) != 3 or sum(1 for e in f.edges if e.is_boundary) < 2:
                    continue
                if min(math.degrees(l.calc_angle()) for l in f.loops) < RELAX_EAR:
                    ears.append(f)
            if not ears:
                break
            bmesh.ops.delete(bm, geom=ears, context="FACES")
            spikes += len(ears)
        loose = [v for v in bm.verts if not v.link_faces]
        if loose:
            bmesh.ops.delete(bm, geom=loose, context="VERTS")
        bm.verts.ensure_lookup_table()
        nb = {}
        for e in bm.edges:
            if e.is_boundary:
                a, b = e.verts
                nb.setdefault(a.index, []).append(b.index)
                nb.setdefault(b.index, []).append(a.index)
        edge = [i for i, n in nb.items() if len(n) == 2]
        pos = {i: bm.verts[i].co.copy() for i in nb}
        start = {i: pos[i].copy() for i in edge}
        for r in range(2 * RELAX_ROUNDS):
            f = RELAX_LAMBDA if r % 2 == 0 else RELAX_MU
            nxt = {}
            for i in edge:
                a, b = nb[i]
                nxt[i] = pos[i] + ((pos[a] + pos[b]) * 0.5 - pos[i]) * f
            pos.update(nxt)
        delta = {i: pos[i] - start[i] for i in edge}
        inner = {}
        for i, d in delta.items():
            for e in bm.verts[i].link_edges:
                j = e.other_vert(bm.verts[i]).index
                if j not in nb:
                    inner.setdefault(j, []).append(d)
        for j, ds in inner.items():
            delta[j] = sum(ds, Vector()) * (0.5 / len(ds))
        keys = list(bm.verts.layers.shape.values())
        for i, d in delta.items():
            v = bm.verts[i]
            v.co += d
            for lay in keys:
                v[lay] += d
        bm.to_mesh(o.data)
        bm.free()
        o.data.update()
        k = ch.keys.get(o.name)
        a, b = out.get(k, (0, 0))
        out[k] = (a + spikes, b + len(edge))
    return out



def inherit_weights(obj, ch, under):
    """Replaces the weights of gear laid over the garments (the crossbelts) by those of the
    garment surface under each vertex (`transfer_weights` with `under`)."""
    ws = transfer_weights(obj, ch.flesh, ch.arm, under=under)
    for g in list(obj.vertex_groups):
        if g.name in ch.arm.data.bones:
            obj.vertex_groups.remove(g)
    for i, w in enumerate(ws):
        for b, x in w.items():
            g = obj.vertex_groups.get(b) or obj.vertex_groups.new(name=b)
            g.add([i], x, "REPLACE")
    return obj


BUTTON = {"lapel": 0.0095, "cuff": 0.0085, "gaiter": 0.0065}  # radius (m)


def _weights_at(obj, face):
    """{bone: weight} of the vertex of `face` (polygon index of `obj`) with the most weight."""
    names = {g.index: g.name for g in obj.vertex_groups}
    best = {}
    for i in obj.data.polygons[face].vertices:
        w = {names[g.group]: g.weight for g in obj.data.vertices[i].groups if g.group in names and g.weight > 0}
        if sum(w.values()) > sum(best.values()):
            best = w
    t = sum(best.values()) or 1.0
    return {b: x / t for b, x in best.items()}


def buttons(ch, cloth, name):
    """Brass buttons on the lapels and cuffs, pewter ones down the outside of the gaiters:
    small low domes (6 triangles) on the draped cloth, skinned like the cloth under them.
    Adds the objects to `ch` (keys `brass`, `metal`); returns them."""
    P = ch.prefix
    by_key = {}
    for o in cloth:
        by_key.setdefault(ch.keys.get(o.name), []).append(o)
    seg = am.segments(ch.arm)
    spots = {"brass": [], "metal": []}  # (object, point, normal, radius)

    trees = {}

    def cast(objs, origin, direction, r, key):
        best = None
        for o in objs:
            if o.name not in trees:
                trees[o.name] = BVHTree.FromPolygons([v.co.copy() for v in o.data.vertices],
                                                     [list(p.vertices) for p in o.data.polygons])
            hit = trees[o.name].ray_cast(origin, direction, 1.0)
            if hit[0] is not None and (best is None or hit[3] < best[3]):
                best = (o, hit[0], hit[1], hit[2], hit[3])
        if best is not None:
            o, co, nrm, face, _d = best
            if nrm.dot(direction) > 0:
                nrm = -nrm
            spots[key].append((o, co, nrm, r, face))

    # Lapels in an own cloth (`lapels` key, line.py) carry the lapel buttons too.
    facings = by_key.get("facings", []) + by_key.get("lapels", [])
    for o in facings:
        V = np.array([v.co[:] for v in o.data.vertices])
        if not len(V):
            continue
        for sx in (1.0, -1.0):
            # Lapels: front of the chest, buttons near the outer edge, evenly to the waist.
            lap = V[(V[:, 1] < -0.04) & (V[:, 0] * sx > 0.01) & (V[:, 0] * sx < 0.2) & (V[:, 2] > 1.0) & (V[:, 2] < 1.47)]
            if len(lap) > 8:
                z0, z1 = np.percentile(lap[:, 2], 6) + 0.02, np.percentile(lap[:, 2], 94) - 0.015
                for z in np.linspace(z0, z1, 7):
                    band = lap[np.abs(lap[:, 2] - z) < 0.025]
                    if len(band) < 2:
                        continue
                    x = sx * (np.abs(band[:, 0]).max() - 0.02)
                    cast(facings, Vector((x, -0.6, z)), Vector((0, 1, 0)), BUTTON["lapel"], "brass")
            # Cuffs: three buttons up the back of the round cuff.
            side = "l" if sx > 0 else "r"
            h, t = seg.get(P + f"forearm_{side}", (None, None))
            if h is None:
                continue
            axis = (t - h).normalized()
            rel = V - np.array(h[:])
            along = rel @ np.array(axis[:])
            perp = np.linalg.norm(rel - np.outer(along, axis[:]), axis=1)
            cuff = (along > 0.4 * (t - h).length) & (along < (t - h).length + 0.05) & (perp < 0.09)
            if cuff.sum() < 6:
                continue
            a0, a1 = along[cuff].min(), along[cuff].max()
            back = Vector((0, 1, 0)) - axis * axis.y
            back.normalize()
            for k in range(3):
                q = h + axis * (a0 + (a1 - a0) * (0.25 + 0.25 * k))
                cast(facings, q + back * 0.3, -back, BUTTON["cuff"], "brass")
    for side, sx in (("l", 1.0), ("r", -1.0)):
        h, t = seg.get(P + f"shin_{side}", (None, None))
        gs = by_key.get("gaiters", [])
        if h is None or not gs:
            continue
        axis = (h - t).normalized()  # ankle -> knee
        out = Vector((sx, 0, 0)) - axis * (axis.x * sx)
        out.normalize()
        V = np.concatenate([np.array([v.co[:] for v in o.data.vertices]) for o in gs])
        V = V[V[:, 0] * sx > 0]
        along = (V - np.array(t[:])) @ np.array(axis[:])
        lo, hi = 0.07, min(along.max() - 0.07, (h - t).length - 0.02)
        for a in np.arange(lo, hi, 0.045):
            q = t + axis * float(a)
            cast(gs, q + out * 0.3, -out, BUTTON["gaiter"], "metal")
    made = []
    for key, sp in spots.items():
        if not sp:
            continue
        verts, faces, ws = [], [], []
        for o, co, nrm, r, face in sp:
            nrm = nrm.normalized()
            u = nrm.orthogonal().normalized()
            v = nrm.cross(u)
            w = _weights_at(o, face)
            c = len(verts)
            verts.append(co + nrm * (r * 0.45))
            ws.append(w)
            for k in range(6):
                ang = k * math.pi / 3
                verts.append(co + nrm * 0.0015 + (u * math.cos(ang) + v * math.sin(ang)) * r)
                ws.append(w)
            faces += [(c, c + 1 + k, c + 1 + (k + 1) % 6) for k in range(6)]
        me = bpy.data.meshes.new(f"{name}.buttons_{key}")
        me.from_pydata([x[:] for x in verts], [], faces)
        me.validate()
        # (centre, rim k, rim k+1) with the rim counter-clockwise about the normal: faces out.
        obj = bd.link(bpy.data.objects.new(f"{name}.buttons_{key}", me))
        finish(obj, ch, False, weights=ws)
        ch.add("clothes", obj, key, symmetric=True)
        made.append(obj)
    return made


def surface(objs, name):
    """A mesh joining `objs` (rest pose), for straps laid over the garments."""
    bm = bmesh.new()
    for o in objs:
        bm.from_mesh(o.data)
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    return bpy.data.objects.new(name, me)


# --------------------------------------------------------------------------- modelled pieces
# (docs/clothing.md, "17th century"): stiff pieces that are not draped, cast onto the draped
# clothes and skinned like the layer under them.

def _cloth_tree(objs):
    vs, fs, off = [], [], 0
    for o in objs:
        m = o.matrix_world
        vs += [m @ v.co for v in o.data.vertices]
        fs += [[i + off for i in p.vertices] for p in o.data.polygons]
        off += len(o.data.vertices)
    return BVHTree.FromPolygons(vs, fs) if fs else None


BAND_CLEAN = True  # the falling band's rings limited in drop and smoothed round the neck (#287 trisoup4)
BAND_SLOPE = 0.8  # tan of the steepest drop from one ring to the next
BAND_SMOOTH = 4  # smoothing passes round the neck


def falling_band(ch, name, width=(0.115, 0.085), key="fallband", n=48, rings=7, lift=0.008):
    """The falling band: a wide linen collar lying flat on the doublet's shoulders, deeper in
    front (`width` m front, back), split at the centre front. Each vertex is cast straight
    down onto the clothes (`lift` above them) from a ring round the neck; skinned like the
    cloth under it. Added to `ch`'s clothes with key `key`."""
    cloth = [o for o in ch.layers["clothes"] if o.type == "MESH" and ch.keys.get(o.name) not in ("shoes",)]
    tree = _cloth_tree(cloth)  # not the skin: rays near the neck found the chin and the face
    seg = am.segments(ch.arm)
    head, tail = seg[ch.prefix + "neck"]
    cx, cy = head.x, head.y + 0.008
    r0 = 0.062
    bm = bmesh.new()
    # Per row (angle) and ring: the cloth surface under it (`floor`, z + lift) or None.
    rows_xy, floor = [], []
    for i in range(n + 1):
        t = -math.pi / 2 + 2 * math.pi * (i + 0.5 * (i == 0) - 0.5 * (i == n)) / n  # split at the front (-Y)
        front = max(0.0, -math.sin(t)) ** 2
        w = width[1] + (width[0] - width[1]) * front
        xy, fl = [], []
        for k in range(rings + 1):
            r = r0 + w * k / rings
            o = Vector((cx + r * math.cos(t), cy + r * math.sin(t), head.z + 0.03))
            hit = tree.ray_cast(o, Vector((0, 0, -1)), 0.35)
            xy.append((o.x, o.y, r))
            fl.append(None if hit[0] is None else hit[0].z + lift)
        rows_xy.append(xy)
        floor.append(fl)
    # Stiff linen: a ring drops at most `BAND_SLOPE` x its radial step below the ring inside
    # it (a ray past the shoulder's or the back's edge found the cloth far below: the long
    # spikes of #287's trisoup), then each ring is smoothed round the neck, never under the
    # cloth.
    z = []
    for xy, fl in zip(rows_xy, floor):
        zr = []
        for k in range(rings + 1):
            f = fl[k]
            if k == 0:
                zr.append(f if f is not None else head.z - 0.03)
            elif not BAND_CLEAN:
                zr.append(f if f is not None else head.z - 0.03)
            else:
                lim = zr[-1] - BAND_SLOPE * (xy[k][2] - xy[k - 1][2])
                zr.append(max(f, lim) if f is not None else lim)
        z.append(zr)
    if BAND_CLEAN:
        for _ in range(BAND_SMOOTH):
            z = [[max((z[max(i - 1, 0)][k] + 2 * z[i][k] + z[min(i + 1, n)][k]) / 4,
                      floor[i][k] if floor[i][k] is not None else -1e9) for k in range(rings + 1)]
                 for i in range(n + 1)]
    grid = [[bm.verts.new((x, y, zz)) for (x, y, _r), zz in zip(xy, zr)] for xy, zr in zip(rows_xy, z)]
    for a, b in zip(grid, grid[1:]):
        for k in range(rings):
            bm.faces.new((a[k], b[k], b[k + 1], a[k + 1]))
    # The edge turned down 6 mm, so the band reads as a flat collar with a thickness.
    lip = [bm.verts.new(row[-1].co - Vector((0, 0, 0.006))) for row in grid]
    for i in range(n):
        bm.faces.new((grid[i][-1], grid[i + 1][-1], lip[i + 1], lip[i]))
    me = bpy.data.meshes.new(f"{name}.{key}")
    bm.to_mesh(me)
    bm.free()
    obj = bd.link(bpy.data.objects.new(f"{name}.{key}", me))
    under = [o for o in cloth if not o.name.endswith("_skirts")]
    finish(obj, ch, False, under=under)
    return ch.add("clothes", obj, key, symmetric=True)


def bucket_boots(ch, name, key="jackboots", n=16, offset=0.010):
    """Bucket-top boots (modelled, not draped): per leg a tube from the shoe up the shin,
    fitted per angle to the skin and the hose under it plus `offset`, then the bucket: the
    top flaring wide over the knee and its edge turned down. Skinned like the skin."""
    import numpy as np

    P = ch.prefix
    seg = am.segments(ch.arm)
    pts = [(o.matrix_world @ v.co) for o in [ch.flesh] + [o for o in ch.layers["clothes"] if o.type == "MESH"
                                                         and ch.keys.get(o.name) in ("hose", "stockings", "breeches")]
           for v in o.data.vertices]
    p = np.array([tuple(v) for v in pts], np.float32)
    bm = bmesh.new()
    for sfx in ("_l", "_r"):
        knee, ankle = seg[P + "shin" + sfx]
        hip = seg[P + "thigh" + sfx][0]
        z_a, z_k = ankle.z + 0.05, knee.z

        def centre(z):
            if z <= z_k:
                f = (z - ankle.z) / max(z_k - ankle.z, 1e-6)
                return ankle.lerp(knee, f)
            f = (z - z_k) / max(hip.z - z_k, 1e-6)
            return knee.lerp(hip, f)

        def radii(z):
            c = centre(z)
            sel = p[(np.abs(p[:, 2] - z) < 0.02) & (np.hypot(p[:, 0] - c.x, p[:, 1] - c.y) < 0.11)
                    & (np.sign(p[:, 0] + 1e-6) == np.sign(c.x))]
            a = np.arctan2(sel[:, 1] - c.y, sel[:, 0] - c.x)
            r = np.hypot(sel[:, 0] - c.x, sel[:, 1] - c.y)
            out = []
            for i in range(n):
                t = 2 * math.pi * i / n
                d = np.abs((a - t + math.pi) % (2 * math.pi) - math.pi)
                near = r[d < math.pi / n * 1.6]
                out.append(float(near.max()) if len(near) else 0.06)
            return c, out

        prof = []
        for k in range(6):  # the leg of the boot, ankle to knee
            z = z_a + (z_k - 0.03 - z_a) * k / 5
            c, rs = radii(z)
            prof.append([(c, r + offset, z) for r in rs])
        c, rs = radii(z_k + 0.02)
        base = [max(r, prof[-1][i][1] - offset) + offset for i, r in enumerate(rs)]
        for dz, grow in ((0.03, 0.012), (0.09, 0.040), (0.13, 0.060), (0.10, 0.075)):  # the bucket, then its lip
            z = z_k + dz
            c = centre(z)
            prof.append([(c, b + grow, z) for b in base])
        rings = []
        for ring in prof:
            rings.append([bm.verts.new((c.x + r * math.cos(2 * math.pi * i / n), c.y + r * math.sin(2 * math.pi * i / n), z))
                          for i, (c, r, z) in enumerate(ring)])
        for a, b in zip(rings, rings[1:]):
            for i in range(n):
                j = (i + 1) % n
                bm.faces.new((a[i], a[j], b[j], b[i]))
    me = bpy.data.meshes.new(f"{name}.{key}")
    bm.to_mesh(me)
    bm.free()
    obj = bd.link(bpy.data.objects.new(f"{name}.{key}", me))
    finish(obj, ch, False)
    return ch.add("clothes", obj, key, symmetric=True)


registry.gear("falling_band", lambda ch, name, s: falling_band(ch, name))
registry.gear("bucket_boots", lambda ch, name, s: bucket_boots(ch, name))
