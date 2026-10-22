"""Sewing patterns of draped garments (pure Python, no Blender; docs/clothing.md).

A garment is a set of flat panels cut from cloth: 2D polygons in centimetres, the grain
along +v, the outline counter-clockwise. Named edges are runs of outline vertices; a seam
joins one run (or a chain of runs on several panels) with another, and `drape.py` sews
them with Blender's sewing springs after placing every panel around the body.

Panels are drafted for the wearer's left side and centre; `mirror` panels are repeated on
the right by the drape (x mirrored). Measurements (`MEASURE`, cm) come from the body: the
drape measures the MakeHuman hull, so the same draft fits every build.

The cut follows the reconstructions of the 1768 Royal Warrant uniform (the pattern
diagrams of the Hunter and Rogers reconstructions): a fitted, cutaway coat with lapels
buttoned back to the waist, the skirts falling to the knee with their front corners turned
back, two-piece sleeves with round cuffs and a small stand collar; a sleeveless waistcoat
reaching below the waist; breeches with a fall front and knee bands; knee-high gaiters
buttoned on the outside over the shoes. Only the shirt's wristbands and the black stock
show, so only those are cut.

Placement (`place`): where the panel's anchor (`u0`, v = 0) goes before the sim.
- region "torso": wrapped on an elliptical cylinder around the body's vertical axis;
  theta 0 = centre front, 90 = the wearer's left side, 180 = centre back; `dir` +1 when u
  runs from the front towards the left side; v rises from the landmark `at` (metres).
- region "arm": wrapped round the straight A-pose arm (shoulder to wrist); theta 0 = the
  top of the arm, +90 = the front; v runs from the wrist (`at` = "wrist") to the shoulder.
- region "leg": wrapped round the thigh/shin axis; theta 0 = the front of the leg,
  +90 = the outside; v rises from `at`.
`gap` (cm) is the clearance before the sim; `layer` orders overlapping panels.
"""

import math

# Default measurements (cm) of the reposed MakeHuman body (build 1.0); `drape.measure`
# replaces them with the hull's own.
MEASURE = {
    "chest": 98.0, "waist": 84.0, "seat": 98.0, "neck": 39.0,
    "nape_to_waist": 45.0, "shoulder": 15.0, "arm": 58.0, "upper_arm": 31.0, "wrist": 18.0,
    "waist_to_knee": 50.0, "thigh": 56.0, "knee": 38.0, "calf": 36.0, "ankle": 24.0,
    "knee_to_ankle": 42.0,
}

# Ease (cm of girth) per garment over the body.
EASE = {"waistcoat": 6.0, "coat": 12.0, "breeches": 6.0, "gaiters": 3.0, "shirt": 2.0}

# Wool broadcloth, linen and canvas: thickness (m) of the solidified cloth, sim settings.
CLOTH = {
    "coat": {"thickness": 0.0030, "mass": 0.45, "tension": 18.0, "bending": 2.0, "shrink": 0.03},
    "waistcoat": {"thickness": 0.0020, "mass": 0.30, "tension": 15.0, "bending": 0.8, "shrink": 0.05},
    "breeches": {"thickness": 0.0020, "mass": 0.30, "tension": 15.0, "bending": 0.6, "shrink": 0.06},
    "gaiters": {"thickness": 0.0020, "mass": 0.30, "tension": 20.0, "bending": 1.0, "shrink": 0.04},
    "shirt": {"thickness": 0.0012, "mass": 0.15, "tension": 10.0, "bending": 0.2, "shrink": 0.0},
}

# Order in which the garments are draped (each collides with the body and those before).
ORDER = ("breeches", "gaiters", "shirt", "waistcoat", "coat")


class Panel:
    """One cloth panel: `outline` [(u, v)] cm, counter-clockwise; `edges` {name: (i, j)}
    = the outline from vertex i to vertex j (forwards, wrapping); `key` the material key;
    `skirt` marks free-hanging cloth (own weights, runtime cloth later)."""

    def __init__(self, name, garment, key, outline, edges, place, mirror=True, skirt=False, grain=(0.0, 1.0)):
        self.name = name
        self.garment = garment
        self.key = key
        self.outline = [(float(u), float(v)) for u, v in outline]
        self.edges = dict(edges)
        self.place = dict(place)
        self.mirror = mirror
        self.skirt = skirt
        self.grain = grain

    def edge_points(self, edge):
        """The outline points of `edge` in order (inclusive)."""
        i, j = self.edges[edge]
        n = len(self.outline)
        out = [self.outline[i]]
        k = i
        while k != j:
            k = (k + 1) % n
            out.append(self.outline[k])
        return out

    def area(self):
        """Signed area (cm², positive counter-clockwise)."""
        a = 0.0
        pts = self.outline
        for (u0, v0), (u1, v1) in zip(pts, pts[1:] + pts[:1]):
            a += u0 * v1 - u1 * v0
        return a / 2.0

    def bounds(self):
        us = [p[0] for p in self.outline]
        vs = [p[1] for p in self.outline]
        return min(us), min(vs), max(us), max(vs)


def _link(x):
    """(panel, edge[, reversed[, side]]) -> (panel, edge, reversed, side); side "L" (the
    drafted panel, or each side's own copy for seams within a side) or "R" (the mirrored
    copy: centre seams)."""
    x = tuple(x)
    return x + ((False, "L")[len(x) - 2:] if len(x) < 4 else ())


class Seam:
    """Sews side `a` to side `b`; each side is a chain [(panel, edge, reversed, side)] whose
    runs are concatenated. Both chains run in the same direction on the body. A seam whose
    links are all "L" is sewn on both sides (once per mirrored copy); one with an "R" link
    joins the drafted panel with its mirror (centre front/back) and is sewn once."""

    def __init__(self, a, b, garment, ease=0.15):
        self.a = [_link(x) for x in a]
        self.b = [_link(x) for x in b]
        self.garment = garment
        self.ease = ease  # the longer side may exceed the shorter by this fraction (gathered)


def polyline_length(pts):
    return sum(math.dist(p, q) for p, q in zip(pts, pts[1:]))


def resample(pts, n):
    """`n` + 1 points evenly spaced along the polyline `pts` (arc length)."""
    seg = [math.dist(p, q) for p, q in zip(pts, pts[1:])]
    total = sum(seg)
    out = []
    for k in range(n + 1):
        t = total * k / n
        acc = 0.0
        for (p, q), s in zip(zip(pts, pts[1:]), seg):
            if acc + s >= t - 1e-9 or (p, q) == (pts[-2], pts[-1]):
                f = 0.0 if s < 1e-12 else min(max((t - acc) / s, 0.0), 1.0)
                out.append((p[0] + (q[0] - p[0]) * f, p[1] + (q[1] - p[1]) * f))
                break
            acc += s
    return out


def chain_points(panels, chain):
    """[(panel name, (u, v))] along a seam side."""
    out = []
    for name, edge, rev, _side in chain:
        pts = panels[name].edge_points(edge)
        if rev:
            pts = pts[::-1]
        if out and math.dist(out[-1][1], pts[0]) < 1e-9 and out[-1][0] == name:
            pts = pts[1:]
        out += [(name, p) for p in pts]
    return out


def seam_lengths(panels, seam):
    def length(chain):
        return sum(polyline_length(panels[n].edge_points(e)) for n, e, _r, _s in chain)

    return length(seam.a), length(seam.b)


# --------------------------------------------------------------------------- drafting

def _circle(cx, cy, r, a0, a1, n):
    """Points on a circular arc (degrees), a0 to a1 inclusive."""
    return [(cx + r * math.cos(math.radians(a0 + (a1 - a0) * k / n)),
             cy + r * math.sin(math.radians(a0 + (a1 - a0) * k / n))) for k in range(n + 1)]


def coat(m=MEASURE):
    """The regimental coat (1768 pattern): fronts with lapels, backs, skirts (front and
    back per side) with turnbacks, two-piece sleeves with round cuffs, a stand collar."""
    e = EASE["coat"]
    q_chest = (m["chest"] + e) / 4.0
    q_waist = (m["waist"] + e) / 4.0
    back_len = m["nape_to_waist"]
    ah = back_len * 0.5  # armhole depth below the shoulder line
    fw_c, bw_c = q_chest + 3.5, q_chest - 3.5  # side seams set to the back
    fw_w, bw_w = q_waist + 4.0, q_waist - 6.0  # the backs narrow into the side pleats
    top = back_len + 2.0  # neck point above the waist (over the shoulder)
    sh = m["shoulder"]
    neck_r = m["neck"] / (2 * math.pi) + 0.6
    P = {}
    # Front (left): CF edge straight from the waist to the neck, armhole, shoulder seam.
    u_ah = fw_c
    front = [(0.0, 0.0), (fw_w, 0.0), (u_ah, top - ah), (u_ah - 4.0, top - ah + 5.0),
             (u_ah - 7.0, top - ah + 11.0), (neck_r + sh - 1.0, top - 2.5),
             (neck_r, top), (neck_r * 0.6, top - neck_r * 0.8), (0.0, top - neck_r * 1.3)]
    P["coat_front"] = Panel("coat_front", "coat", "coat", front,
                            {"waist": (0, 1), "side": (1, 2), "armhole": (2, 5), "shoulder": (5, 6),
                             "neck": (6, 8), "cf": (8, 0)},
                            {"region": "torso", "theta": 0.0, "dir": 1, "u0": 0.0, "at": "waist", "gap": 1.5})
    # Lapel (facing colour): sewn along the CF edge, lying back on the chest, wider at the
    # top (the 1768 lapel ran to the waist, 7.5 cm wide at the top).
    lp = [(0.0, 0.0), (6.0, 0.0), (8.0, top - neck_r * 1.3 - 4.0), (7.0, top - neck_r * 1.3),
          (0.0, top - neck_r * 1.3)]
    P["lapel"] = Panel("lapel", "coat", "facings", lp, {"cf": (4, 0), "outer": (1, 2)},
                       {"region": "torso", "theta": 0.0, "dir": 1, "u0": 0.0, "at": "waist", "gap": 2.3, "layer": 1})
    # Back (left): CB seam, side seam, armhole, shoulder seam.
    back = [(0.0, 0.0), (bw_w, 0.0), (bw_c, top - ah), (bw_c + 1.0, top - ah + 9.0),
            (neck_r + sh, top - 1.5), (neck_r, top), (0.0, top - 1.5)]
    P["coat_back"] = Panel("coat_back", "coat", "coat", back,
                           {"waist": (0, 1), "side": (1, 2), "armhole": (2, 4), "shoulder": (4, 5),
                            "neck": (5, 6), "cb": (6, 0)},
                           {"region": "torso", "theta": 180.0, "dir": -1, "u0": 0.0, "at": "waist", "gap": 1.5})
    # Skirts: waist seam to the knee, flared (the pleats at the side are let out as flare).
    drop = m["waist_to_knee"] + 2.0
    # The front edge curves away below the waist (cutaway); v rises from the hem.
    tb_top = drop - 16.0
    fs = [(4.0, 0.0), (17.0, 0.0), (fw_w + 7.0, 0.0), (fw_w - 1.0, drop), (2.0, drop - 1.0), (0.0, drop - 8.0),
          (0.7, tb_top)]
    P["skirt_front"] = Panel("skirt_front", "coat", "coat", fs,
                             {"hem_tb": (0, 1), "hem": (0, 2), "side": (2, 3), "waist": (3, 4), "front": (4, 0),
                              "front_low": (6, 0)},
                             {"region": "torso", "theta": 4.0, "dir": 1, "u0": 0.0, "at": "waist", "lift": -drop,
                              "gap": 2.5}, skirt=True)
    bs = [(0.0, 0.0), (bw_w + 9.0, 0.0), (bw_w, drop), (0.0, drop)]
    P["skirt_back"] = Panel("skirt_back", "coat", "coat", bs,
                            {"hem": (0, 1), "side": (1, 2), "waist": (2, 3), "vent": (3, 0)},
                            {"region": "torso", "theta": 180.0, "dir": -1, "u0": 0.0, "at": "waist",
                             "lift": -drop, "gap": 2.5}, skirt=True)
    # Turnback: the skirt's front corner hooked back, its white lining outwards: a triangle
    # lying on the skirt front, sewn along the front edge and the hem.
    tb = [(4.0, 0.0), (17.0, 0.0), (0.7, tb_top)]
    P["turnback"] = Panel("turnback", "coat", "lining", tb, {"hem": (0, 1), "fold": (1, 2), "front": (2, 0)},
                          {"region": "torso", "theta": 4.0, "dir": 1, "u0": 0.0, "at": "waist", "lift": -drop,
                           "gap": 3.3, "layer": 1}, skirt=True)
    # Two-piece sleeve: the top sleeve over the outside of the arm with the cap, the under
    # sleeve in the armpit; seams at the front and back of the arm.
    arm_len = m["arm"]
    up_g = m["upper_arm"] + 8.0
    wr_g = m["wrist"] + 12.0
    tw_top, tw_wr = up_g * 0.62, wr_g * 0.60
    uw_top, uw_wr = up_g * 0.38, wr_g * 0.40
    cap = 9.0
    top_sl = [(-tw_wr / 2, 0.0), (tw_wr / 2, 0.0), (tw_top / 2, arm_len - cap),
              (tw_top / 4, arm_len - cap * 0.25), (0.0, arm_len), (-tw_top / 4, arm_len - cap * 0.25),
              (-tw_top / 2, arm_len - cap)]
    P["sleeve_top"] = Panel("sleeve_top", "coat", "coat", top_sl,
                            {"wrist": (0, 1), "fwd": (1, 2), "cap_front": (2, 4), "cap_back": (4, 6), "back": (6, 0)},
                            {"region": "arm", "theta": 0.0, "dir": 1, "u0": 0.0, "at": "wrist", "gap": 2.0})
    under = [(-uw_wr / 2, 0.0), (uw_wr / 2, 0.0), (uw_top / 2, arm_len - cap), (0.0, arm_len - cap - 3.0),
             (-uw_top / 2, arm_len - cap)]
    P["sleeve_under"] = Panel("sleeve_under", "coat", "coat", under,
                              {"wrist": (0, 1), "back": (1, 2), "pit_back": (2, 3), "pit_front": (3, 4), "fwd": (4, 0)},
                              {"region": "arm", "theta": 180.0, "dir": 1, "u0": 0.0, "at": "wrist", "gap": 2.0})
    # Round cuff (facing colour), deep, closed at the back of the arm.
    cuff_g = wr_g + 4.0
    P["cuff"] = Panel("cuff", "coat", "facings", [(0.0, 0.0), (cuff_g, 0.0), (cuff_g + 1.0, 11.0), (-1.0, 11.0)],
                      {"bottom": (0, 1), "end": (1, 2), "top": (2, 3), "start": (3, 0)},
                      {"region": "arm", "theta": -180.0 * tw_wr / wr_g, "dir": 1, "u0": 0.0, "at": "wrist",
                       "lift": -1.0, "gap": 3.2, "layer": 1})
    # Stand collar (facing colour), half: CB to CF.
    half_neck = m["neck"] / 2.0 + 2.0
    P["collar"] = Panel("collar", "coat", "facings", [(0.0, 0.0), (half_neck, 0.0), (half_neck - 1.0, 3.5), (0.0, 4.0)],
                        {"base": (0, 1), "top": (2, 3)},
                        {"region": "neck", "theta": 180.0, "dir": -1, "u0": 0.0, "at": "collar", "gap": 1.2})
    seams = [
        Seam([("coat_front", "side")], [("coat_back", "side")], "coat"),
        Seam([("coat_front", "shoulder")], [("coat_back", "shoulder")], "coat", ease=0.25),
        Seam([("coat_back", "cb")], [("coat_back", "cb", False, "R")], "coat"),  # centre back
        Seam([("coat_front", "waist")], [("skirt_front", "waist", True)], "coat", ease=0.3),
        Seam([("coat_back", "waist")], [("skirt_back", "waist", True)], "coat", ease=0.5),
        Seam([("skirt_back", "side")], [("skirt_front", "side")], "coat", ease=0.2),
        Seam([("lapel", "cf")], [("coat_front", "cf")], "coat"),
        Seam([("turnback", "front")], [("skirt_front", "front_low")], "coat"),
        Seam([("turnback", "hem")], [("skirt_front", "hem_tb")], "coat"),
        Seam([("sleeve_top", "fwd")], [("sleeve_under", "fwd", True)], "coat"),
        Seam([("sleeve_top", "back", True)], [("sleeve_under", "back")], "coat"),
        Seam([("cuff", "start")], [("cuff", "end", True)], "coat"),
        Seam([("cuff", "bottom")], [("sleeve_top", "wrist"), ("sleeve_under", "wrist")], "coat", ease=0.3),
        Seam([("sleeve_under", "pit_front"), ("sleeve_top", "cap_front")],
             [("coat_front", "armhole")], "coat", ease=0.35),
        Seam([("sleeve_top", "cap_back"), ("sleeve_under", "pit_back", True)],
             [("coat_back", "armhole", True)], "coat", ease=0.35),
        Seam([("collar", "base")], [("coat_back", "neck", True), ("coat_front", "neck")], "coat", ease=0.5),
    ]
    return P, seams


def waistcoat(m=MEASURE):
    """Sleeveless waistcoat (1768: to just below the waist, buttoned down the front)."""
    e = EASE["waistcoat"]
    q_chest = (m["chest"] + e) / 4.0
    q_waist = (m["waist"] + e) / 4.0
    back_len = m["nape_to_waist"]
    top = back_len + 1.0
    below = 14.0  # below the waist
    ah = back_len * 0.48
    neck_r = m["neck"] / (2 * math.pi) + 0.3
    sh = m["shoulder"] - 2.0
    P = {}
    fr = [(0.0, -below + 4.0), (6.0, -below), (q_waist + 2.0, -below + 2.0), (q_chest + 1.5, top - ah),
          (q_chest - 4.0, top - ah + 9.0), (neck_r + sh, top - 2.0), (neck_r, top), (0.0, top - 16.0)]
    P["wc_front"] = Panel("wc_front", "waistcoat", "waistcoat", fr,
                          {"side": (2, 3), "armhole": (3, 5), "shoulder": (5, 6), "cf": (7, 0)},
                          {"region": "torso", "theta": 0.0, "dir": 1, "u0": 0.0, "at": "waist", "gap": 0.8})
    bk = [(0.0, -below + 3.0), (q_waist - 2.0, -below + 2.0), (q_chest - 1.5, top - ah),
          (q_chest - 3.0, top - ah + 10.0), (neck_r + sh, top - 1.5), (neck_r, top), (0.0, top - 1.5)]
    P["wc_back"] = Panel("wc_back", "waistcoat", "waistcoat", bk,
                         {"side": (1, 2), "armhole": (2, 4), "shoulder": (4, 5), "cb": (6, 0)},
                         {"region": "torso", "theta": 180.0, "dir": -1, "u0": 0.0, "at": "waist", "gap": 0.8})
    seams = [
        Seam([("wc_front", "side")], [("wc_back", "side")], "waistcoat"),
        Seam([("wc_front", "shoulder")], [("wc_back", "shoulder")], "waistcoat", ease=0.25),
        Seam([("wc_back", "cb")], [("wc_back", "cb", False, "R")], "waistcoat"),
        Seam([("wc_front", "cf")], [("wc_front", "cf", False, "R")], "waistcoat"),  # buttoned closed
    ]
    return P, seams


def breeches(m=MEASURE):
    """Breeches: front and back per leg (the back with the seat), a fall front over the
    front opening, a waistband and buttoned knee bands."""
    e = EASE["breeches"]
    q_seat = (m["seat"] + e) / 4.0
    q_waist = (m["waist"] + e) / 4.0
    half_thigh = (m["thigh"] + e) / 2.0
    half_knee = (m["knee"] + 4.0) / 2.0
    rise = 27.0  # crotch to waist
    leg = m["waist_to_knee"] - rise + 3.0  # crotch to the knee band
    P = {}
    fwid_t, fwid_k = half_thigh * 0.48, half_knee * 0.5
    fr = [(0.0, 0.0), (fwid_k * 2, 0.0), (fwid_t * 2, leg), (q_seat, leg + rise * 0.55),
          (q_waist, leg + rise), (3.0, leg + rise), (0.0, leg + rise - 4.0), (-4.0, leg + 3.0), (-6.0, leg)]
    # u: 0 at the inseam-front at the knee, round the front of the leg to the outside.
    P["br_front"] = Panel("br_front", "breeches", "breeches", fr,
                          {"knee": (0, 1), "outseam": (1, 4), "waist": (4, 5), "cf": (5, 7), "fork": (7, 8),
                           "inseam": (8, 0)},
                          {"region": "leg", "theta": -90.0, "dir": 1, "u0": 0.0, "at": "knee", "gap": 1.0})
    bwid_t, bwid_k = half_thigh * 0.52, half_knee * 0.5
    bk = [(0.0, 0.0), (-bwid_k * 2, 0.0), (-bwid_t * 2, leg), (-q_seat - 1.0, leg + rise * 0.55),
          (-q_waist + 1.0, leg + rise + 3.0), (-2.0, leg + rise + 5.0), (2.0, leg + rise * 0.3), (9.0, leg - 1.0)]
    bk = [(u, v) for u, v in bk][::-1]  # counter-clockwise
    # After the reversal: 0 fork tip, 1 seat curve, 2 CB top, 3 waist side, 4 hip, 5 thigh, 6 knee out, 7 knee in.
    P["br_back"] = Panel("br_back", "breeches", "breeches", bk,
                         {"fork": (0, 1), "cb": (1, 2), "waist": (2, 3), "outseam": (3, 6), "knee": (6, 7),
                          "inseam": (7, 0)},
                         {"region": "leg", "theta": -90.0, "dir": 1, "u0": 0.0, "at": "knee", "gap": 1.0})
    P["fall"] = Panel("fall", "breeches", "breeches", [(0.0, 0.0), (14.0, 0.0), (14.0, 14.0), (0.0, 14.0)],
                      {"bottom": (0, 1), "top": (2, 3)},
                      {"region": "torso", "theta": 0.0, "dir": 1, "u0": 0.0, "at": "waist", "lift": -14.0,
                       "gap": 1.8, "layer": 1})
    wb = (m["waist"] + e) / 2.0
    P["waistband"] = Panel("waistband", "breeches", "breeches", [(0.0, 0.0), (14.0, 0.0), (wb, 0.0), (wb, 4.0), (0.0, 4.0)],
                           {"bottom_fall": (0, 1), "bottom": (0, 2), "top": (3, 4)},
                           {"region": "torso", "theta": 0.0, "dir": 1, "u0": 0.0, "at": "waist", "lift": -2.0,
                            "gap": 1.6, "layer": 1})
    P["knee_band"] = Panel("knee_band", "breeches", "breeches",
                           [(0.0, 0.0), (half_knee * 2 + 2.0, 0.0), (half_knee * 2 + 2.0, 4.0), (0.0, 4.0)],
                           {"bottom": (0, 1), "end": (1, 2), "top": (2, 3), "start": (3, 0)},
                           {"region": "leg", "theta": 90.0, "dir": 1, "u0": 0.0, "at": "knee", "lift": -1.5,
                            "gap": 1.5, "layer": 1})
    seams = [
        Seam([("br_front", "outseam")], [("br_back", "outseam", True)], "breeches"),
        Seam([("br_front", "inseam", True)], [("br_back", "inseam")], "breeches"),
        Seam([("br_back", "cb")], [("br_back", "cb", False, "R")], "breeches"),
        Seam([("br_front", "cf")], [("br_front", "cf", False, "R")], "breeches"),
        Seam([("br_front", "fork")], [("br_front", "fork", False, "R")], "breeches"),
        Seam([("br_back", "fork")], [("br_back", "fork", False, "R")], "breeches"),
        Seam([("knee_band", "start")], [("knee_band", "end", True)], "breeches"),
        Seam([("knee_band", "bottom")], [("br_back", "knee"), ("br_front", "knee")], "breeches", ease=0.3),
        Seam([("waistband", "bottom")], [("br_front", "waist", True), ("br_back", "waist", True)], "breeches",
             ease=0.3),
        Seam([("fall", "top", True)], [("waistband", "bottom_fall")], "breeches"),
    ]
    return P, seams


def gaiters(m=MEASURE):
    """Knee-high gaiters of black canvas, buttoned up the outside, the front dropping over
    the instep of the shoe."""
    calf = m["calf"] + EASE["gaiters"]
    ank = m["ankle"] + 8.0
    h = m["knee_to_ankle"] - 3.0
    g = [(0.0, 0.0), (ank * 0.75, -7.0), (ank, 0.0), (calf, h * 0.55), (calf - 1.0, h), (1.0, h), (0.0, h * 0.55)]
    P = {"gaiter": Panel("gaiter", "gaiters", "gaiters", g,
                         {"bottom": (0, 2), "outer": (2, 4), "top": (4, 5), "start": (5, 0)},
                         {"region": "leg", "theta": 90.0, "dir": 1, "u0": 0.0, "at": "ankle", "lift": 1.0, "gap": 1.0})}
    # The outside opening is buttoned: start and outer edges are sewn shut.
    seams = [Seam([("gaiter", "start", True)], [("gaiter", "outer")], "gaiters")]
    return P, seams


def shirt(m=MEASURE):
    """Only what shows: the wristbands below the cuffs and the black stock at the neck."""
    wr = m["wrist"] + 4.0
    P = {
        "wristband": Panel("wristband", "shirt", "shirt", [(0.0, 0.0), (wr, 0.0), (wr, 4.0), (0.0, 4.0)],
                           {"start": (3, 0), "end": (1, 2)},
                           {"region": "arm", "theta": 180.0, "dir": 1, "u0": 0.0, "at": "wrist", "lift": -3.0,
                            "gap": 0.6}),
        "stock": Panel("stock", "shirt", "stock", [(0.0, 0.0), (m["neck"] / 2.0 + 1.0, 0.0),
                                                     (m["neck"] / 2.0 + 1.0, 5.0), (0.0, 5.0)],
                       {"start": (3, 0), "end": (1, 2)},
                       {"region": "neck", "theta": 180.0, "dir": -1, "u0": 0.0, "at": "stock", "gap": 0.4}),
    }
    seams = [Seam([("wristband", "start")], [("wristband", "end", True)], "shirt")]
    return P, seams


GARMENTS = {"coat": coat, "waistcoat": waistcoat, "breeches": breeches, "gaiters": gaiters, "shirt": shirt}


def uniform(m=MEASURE, garments=ORDER):
    """{garment: (panels, seams)} for the British infantry uniform, drafted to `m`."""
    return {g: GARMENTS[g](m) for g in garments}
