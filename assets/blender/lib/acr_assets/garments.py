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
EASE = {"waistcoat": 8.0, "coat": 20.0, "breeches": 0.5, "gaiters": 1.0, "shirt": 2.0,
        "doublet": 12.0, "buffcoat": 16.0, "hose": 1.0, "slops": 26.0}

# Wool broadcloth, linen and canvas: thickness (m) of the solidified cloth, sim settings.
CLOTH = {
    "coat": {"thickness": 0.0030, "mass": 0.45, "tension": 18.0, "bending": 2.0, "shrink": 0.0, "margin": 0.006},
    "waistcoat": {"thickness": 0.0020, "mass": 0.30, "tension": 15.0, "bending": 0.8, "shrink": 0.05, "margin": 0.004},
    "breeches": {"thickness": 0.0020, "mass": 0.30, "tension": 15.0, "bending": 0.6, "shrink": 0.08, "margin": 0.003},
    "gaiters": {"thickness": 0.0020, "mass": 0.30, "tension": 20.0, "bending": 1.0, "shrink": 0.04, "margin": 0.003},
    "shirt": {"thickness": 0.0012, "mass": 0.15, "tension": 10.0, "bending": 0.2, "shrink": 0.0, "margin": 0.002},
    # 17th century: the doublet's wool, the buff coat's thick oiled leather (stiff and heavy:
    # it stands off the body in few broad folds), knitted hose, full slops.
    "doublet": {"thickness": 0.0025, "mass": 0.35, "tension": 18.0, "bending": 1.5, "shrink": 0.02, "margin": 0.005},
    "buffcoat": {"thickness": 0.0045, "mass": 0.90, "tension": 40.0, "bending": 14.0, "shrink": 0.0, "margin": 0.007},
    "hose": {"thickness": 0.0015, "mass": 0.20, "tension": 15.0, "bending": 0.4, "shrink": 0.06, "margin": 0.003},
    "slops": {"thickness": 0.0025, "mass": 0.30, "tension": 12.0, "bending": 1.2, "shrink": 0.0, "margin": 0.004},
    "shirt17": {"thickness": 0.0012, "mass": 0.15, "tension": 10.0, "bending": 0.2, "shrink": 0.0, "margin": 0.002},
}
# Drafted instead of shells (#287 drape pilot): linen and cotton shirts and tunics, wool and
# cotton cloaks, a wool sash that shrinks onto the shirt.
CLOTH["frn_blouse"] = dict(CLOTH["shirt"], margin=0.003, bending=0.3)
CLOTH["trade_shirt"] = dict(CLOTH["shirt"], margin=0.003, bending=0.8)
CLOTH["unku"] = {"thickness": 0.0020, "mass": 0.30, "tension": 15.0, "bending": 1.0, "shrink": 0.0, "margin": 0.004}
# Cloaks: a wider collision margin (they lie on draped layers), self-collision for the folds.
CLOTH["frn_cape"] = CLOTH["yacolla"] = dict(CLOTH["coat"], margin=0.009, bending=1.5)
CLOTH["tilmatli"] = dict(CLOTH["coat"], margin=0.009, bending=1.5, self=True, self_gap=0.004)
CLOTH["frn_sash"] = {"thickness": 0.0025, "mass": 0.30, "tension": 15.0, "bending": 0.6, "shrink": 0.10,
                     "margin": 0.004}
CLOTH["doublet_slashed"] = CLOTH["doublet"]
CLOTH["buffcoat_bare"] = CLOTH["buffcoat"]

# Order in which the garments are draped (each collides with the body and those before).
ORDER = ("breeches", "gaiters", "shirt", "waistcoat", "coat")


class Panel:
    """One cloth panel: `outline` [(u, v)] cm, counter-clockwise; `edges` {name: (i, j)}
    = the outline from vertex i to vertex j (forwards, wrapping); `key` the material key;
    `skirt` marks free-hanging cloth (own weights, runtime cloth later)."""

    def __init__(self, name, garment, key, outline, edges, place, mirror=True, skirt=False, grain=(0.0, 1.0),
                 tack=None):
        self.name = name
        self.garment = garment
        self.key = key
        self.outline = [(float(u), float(v)) for u, v in outline]
        self.edges = dict(edges)
        self.place = dict(place)
        self.mirror = mirror
        self.skirt = skirt
        self.grain = grain
        self.tack = tack  # base panel an overlay (lapel, turnback) is tacked onto

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
    joins the drafted panel with its mirror (centre front/back) and is sewn once. `close`
    (m): a closure that must end shut (buttoned): its pairs are welded up to this distance
    instead of `drape.WELD`."""

    def __init__(self, a, b, garment, ease=0.15, close=None):
        self.a = [_link(x) for x in a]
        self.b = [_link(x) for x in b]
        self.garment = garment
        self.ease = ease  # the longer side may exceed the shorter by this fraction (gathered)
        self.close = close


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
             (neck_r, top), (neck_r * 0.6, top - neck_r * 0.8), (0.0, top - neck_r * 1.3),
             (0.0, top - neck_r * 1.3 - 5.0)]
    # The fronts were hooked together at the top of the chest (`hook`), the lapels
    # buttoned back on them, the skirts falling open below.
    P["coat_front"] = Panel("coat_front", "coat", "coat", front,
                            {"waist": (0, 1), "side": (1, 2), "armhole": (2, 5), "shoulder": (5, 6),
                             "neck": (6, 8), "cf": (8, 0), "hook": (8, 9)},
                            {"region": "torso", "theta": 0.0, "dir": 1, "u0": 0.0, "at": "waist", "gap": 2.5})
    # Lapel (facing colour): sewn along the CF edge, lying back on the chest, wider at the
    # top (the 1768 lapel ran to the waist, 7.5 cm wide at the top).
    lp = [(0.0, 0.0), (6.0, 0.0), (8.0, top - neck_r * 1.3 - 4.0), (7.0, top - neck_r * 1.3),
          (0.0, top - neck_r * 1.3)]
    P["lapel"] = Panel("lapel", "coat", "facings", lp, {"cf": (4, 0), "outer": (1, 2)},
                       {"region": "torso", "theta": 0.0, "dir": 1, "u0": 0.0, "at": "waist", "gap": 3.2, "layer": 1},
                       tack="coat_front")
    # Back (left): CB seam, side seam, armhole, shoulder seam.
    back = [(0.0, 0.0), (bw_w, 0.0), (bw_c, top - ah), (bw_c + 1.0, top - ah + 9.0),
            (neck_r + sh, top - 1.5), (neck_r, top), (0.0, top - 1.5)]
    P["coat_back"] = Panel("coat_back", "coat", "coat", back,
                           {"waist": (0, 1), "side": (1, 2), "armhole": (2, 4), "shoulder": (4, 5),
                            "neck": (5, 6), "cb": (6, 0)},
                           {"region": "torso", "theta": 180.0, "dir": -1, "u0": 0.0, "at": "waist", "gap": 2.5})
    # Skirts: waist seam to the knee, flared (the pleats at the side are let out as flare).
    drop = m["waist_to_knee"] + 2.0
    # The front edge curves away below the waist (cutaway); v rises from the hem.
    tb_top = drop * 0.45
    fs = [(12.0, 0.0), (25.0, 0.0), (fw_w + 3.0, 0.0), (fw_w - 1.0, drop), (2.0, drop - 1.0), (0.0, drop - 8.0),
          (5.0, tb_top)]
    P["skirt_front"] = Panel("skirt_front", "coat", "coat", fs,
                             {"hem_tb": (0, 1), "hem": (0, 2), "side": (2, 3), "waist": (3, 4), "front": (4, 0),
                              "front_low": (6, 0)},
                             {"region": "torso", "theta": 4.0, "dir": 1, "u0": 0.0, "at": "waist", "lift": -drop,
                              "gap": 4.0}, skirt=True)
    bs = [(0.0, 0.0), (bw_w + 4.0, 0.0), (bw_w, drop), (0.0, drop)]
    P["skirt_back"] = Panel("skirt_back", "coat", "coat", bs,
                            {"hem": (0, 1), "side": (1, 2), "waist": (2, 3), "vent": (3, 0)},
                            {"region": "torso", "theta": 180.0, "dir": -1, "u0": 0.0, "at": "waist",
                             "lift": -drop, "gap": 4.0}, skirt=True)
    # Turnback: the skirt's front corner hooked back, its white lining outwards: a triangle
    # lying on the skirt front, sewn along the front edge and the hem.
    tb = [(12.0, 0.0), (25.0, 0.0), (5.0, tb_top)]
    P["turnback"] = Panel("turnback", "coat", "lining", tb, {"hem": (0, 1), "fold": (1, 2), "front": (2, 0)},
                          {"region": "torso", "theta": 4.0, "dir": 1, "u0": 0.0, "at": "waist", "lift": -drop,
                           "gap": 4.8, "layer": 1}, skirt=True, tack="skirt_front")
    # Two-piece sleeve: the top sleeve over the outside of the arm with the cap, the under
    # sleeve in the armpit; seams at the front and back of the arm.
    arm_len = m["arm"]
    up_g = m["upper_arm"] + 5.0
    wr_g = m["wrist"] + 12.0
    tw_top, tw_wr = up_g * 0.62, wr_g * 0.60
    uw_top, uw_wr = up_g * 0.38, wr_g * 0.40
    cap = 7.0  # a low cap: the 18th-century sleeve was set in without puff
    top_sl = [(-tw_wr / 2, 0.0), (tw_wr / 2, 0.0), (tw_top / 2, arm_len - cap),
              (tw_top / 4, arm_len - cap * 0.25), (0.0, arm_len), (-tw_top / 4, arm_len - cap * 0.25),
              (-tw_top / 2, arm_len - cap)]
    P["sleeve_top"] = Panel("sleeve_top", "coat", "coat", top_sl,
                            {"wrist": (0, 1), "fwd": (1, 2), "cap_front": (2, 4), "cap_back": (4, 6), "back": (6, 0)},
                            {"region": "arm", "theta": 0.0, "dir": 1, "u0": 0.0, "at": "wrist", "gap": 2.5})
    under = [(-uw_wr / 2, 0.0), (uw_wr / 2, 0.0), (uw_top / 2, arm_len - cap), (0.0, arm_len - cap - 3.0),
             (-uw_top / 2, arm_len - cap)]
    P["sleeve_under"] = Panel("sleeve_under", "coat", "coat", under,
                              {"wrist": (0, 1), "back": (1, 2), "pit_back": (2, 3), "pit_front": (3, 4), "fwd": (4, 0)},
                              {"region": "arm", "theta": 180.0, "dir": 1, "u0": 0.0, "at": "wrist", "gap": 2.5})
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
        Seam([("coat_front", "hook")], [("coat_front", "hook", False, "R")], "coat"),
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
                          {"region": "torso", "theta": 0.0, "dir": 1, "u0": 0.0, "at": "waist", "gap": 1.2})
    bk = [(0.0, -below + 3.0), (q_waist - 2.0, -below + 2.0), (q_chest - 1.5, top - ah),
          (q_chest - 3.0, top - ah + 10.0), (neck_r + sh, top - 1.5), (neck_r, top), (0.0, top - 1.5)]
    P["wc_back"] = Panel("wc_back", "waistcoat", "waistcoat", bk,
                         {"side": (1, 2), "armhole": (2, 4), "shoulder": (4, 5), "cb": (6, 0)},
                         {"region": "torso", "theta": 180.0, "dir": -1, "u0": 0.0, "at": "waist", "gap": 1.2})
    seams = [
        Seam([("wc_front", "side")], [("wc_back", "side")], "waistcoat"),
        Seam([("wc_front", "shoulder")], [("wc_back", "shoulder")], "waistcoat", ease=0.25),
        Seam([("wc_back", "cb")], [("wc_back", "cb", False, "R")], "waistcoat"),
        Seam([("wc_front", "cf")], [("wc_front", "cf", False, "R")], "waistcoat"),  # buttoned closed
    ]
    return P, seams


def breeches(m=MEASURE):
    """Breeches: front and back per leg (the back with the seat), a waistband and buttoned
    knee bands."""
    e = EASE["breeches"]
    q_seat = (m["seat"] + e) / 4.0
    q_waist = (m["waist"] + e) / 4.0
    half_thigh = (m["thigh"] + e) / 2.0
    half_knee = (m["knee"] + 3.0) / 2.0
    rise = 27.0  # crotch to waist
    leg = m["waist_to_knee"] - rise + 3.0  # crotch to the knee band
    P = {}
    fwid_t, fwid_k = half_thigh * 0.45, half_knee * 0.5
    fr = [(0.0, 0.0), (fwid_k * 2, 0.0), (fwid_t * 2, leg), (q_seat, leg + rise * 0.55),
          (q_waist, leg + rise), (3.0, leg + rise), (0.0, leg + rise - 4.0), (-4.0, leg + 3.0), (-6.0, leg)]
    # u: 0 at the inseam-front at the knee, round the front of the leg to the outside.
    P["br_front"] = Panel("br_front", "breeches", "breeches", fr,
                          {"knee": (0, 1), "outseam": (1, 4), "waist": (4, 5), "cf": (5, 7), "fork": (7, 8),
                           "inseam": (8, 0)},
                          {"region": "leg", "theta": -90.0, "dir": 1, "u0": 0.0, "at": "knee", "gap": 1.0})
    bwid_t, bwid_k = half_thigh * 0.50, half_knee * 0.5
    bk = [(0.0, 0.0), (-bwid_k * 2, 0.0), (-bwid_t * 2, leg), (-q_seat - 1.0, leg + rise * 0.55),
          (-q_waist + 1.0, leg + rise + 3.0), (-2.0, leg + rise + 5.0), (2.0, leg + rise * 0.3), (9.0, leg - 1.0)]
    bk = [(u, v) for u, v in bk][::-1]  # counter-clockwise
    # After the reversal: 0 fork tip, 1 seat curve, 2 CB top, 3 waist side, 4 hip, 5 thigh, 6 knee out, 7 knee in.
    P["br_back"] = Panel("br_back", "breeches", "breeches", bk,
                         {"fork": (0, 1), "cb": (1, 2), "waist": (2, 3), "outseam": (3, 6), "knee": (6, 7),
                          "inseam": (7, 0)},
                         {"region": "leg", "theta": -90.0, "dir": 1, "u0": 0.0, "at": "knee", "gap": 1.0})
    # The fall front (a 14 cm flap buttoned to the waistband) lies under the waistcoat and is
    # not cut yet: as a free overlay it fought the breeches in the sim.
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

    ]
    return P, seams


def seam_fractions(pts):
    """Where each sample of a seam edge lies along it, 0..1 by the pattern's length (cm), so
    the two sides pair up point for point: counting samples paired the corners of edges cut
    into different numbers of samples (the gaiter's straight back edge against its slanted
    outer edge) a sample or two apart, and those pairs pulled the cloth open (skin holes at
    the shin of the stockings and leggings cut from the gaiter pattern)."""
    d = [0.0]
    for a, b in zip(pts, pts[1:]):
        d.append(d[-1] + math.dist(a, b))
    return [x / max(d[-1], 1e-9) for x in d]


GAITER_CLOSE = 0.09  # m: the gaiter's buttoned seam is welded shut up to this gap


def gaiters(m=MEASURE):
    """Knee-high gaiters of black canvas, buttoned up the outside, the front dropping over
    the instep of the shoe."""
    calf = m["calf"] + EASE["gaiters"]
    ank = m["ankle"] + 6.0
    h = m["knee_to_ankle"] - 3.0
    top = m["knee"] + 1.5  # buttoned snug below the knee
    g = [(0.0, 0.0), (ank * 0.75, -4.0), (ank, 0.0), (calf, h * 0.55), (top, h), (0.0, h), (0.0, h * 0.55)]
    gplace = {"region": "leg", "theta": 90.0, "dir": 1, "u0": 0.0, "at": "ankle", "lift": 1.0, "gap": 1.0}
    P = {"gaiter": Panel("gaiter", "gaiters", "gaiters", g,
                         {"bottom": (0, 2), "outer": (2, 4), "top": (4, 5), "start": (5, 0)},
                         dict(gplace, pin="top"))}

    # The garter: a black leather strap buckled round the top below the knee (an inlay: one
    # surface with the gaiter, no fold to fight it).
    def edge_u(v):
        return calf + (top - calf) * (v - h * 0.55) / (h * 0.45)

    v0, v1 = h - 6.0, h - 3.5
    P["garter"] = Panel("garter", "gaiters", "garter", [(0.0, v0), (edge_u(v0), v0), (edge_u(v1), v1), (0.0, v1)],
                        {"bottom": (0, 1), "top": (2, 3)}, dict(gplace, layer=1), tack="gaiter")
    # The outside opening is buttoned: start and outer edges are sewn shut. Without the
    # garter (the stockings and leggings cut from this pattern) the top pairs end 5..6 cm
    # apart after the sim, over `drape.WELD`, and the open seam showed the shin: `close`.
    seams = [Seam([("gaiter", "start", True)], [("gaiter", "outer")], "gaiters", close=GAITER_CLOSE)]
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


# --------------------------------------------------------------------------- 17th century
# (docs/clothing.md, "17th century"): the doublet, the buff coat, long hose and slops. The
# bodies are the coat's draft fitted closer (less ease), sewn shut down the centre front.
# Material keys (registered by `frontier.py`, `civil.py` and `outfit.py`): `doublet`,
# `dsleeve` (cloth sleeves), `slashes` (the lining in the slashes), `buff`, `hose`, `slops`.

SLASH = 1.0  # cm: width of a slash (an inlay strip of the lining's cloth)


def _fitted(m, chest, waist):
    """`m` with the girths taken in so that `coat`'s draft (ease `EASE["coat"]`) comes out
    with `chest` and `waist` cm of ease."""
    e = EASE["coat"]
    return dict(m, chest=m["chest"] + chest - e, waist=m["waist"] + waist - e)


def _slashes(base, garment, u_list, v0, v1, place, key="slashes", width=SLASH):
    """Inlay strips (`tack` = `base`) from v0 to v1 at each u of `u_list`."""
    out = {}
    for k, u in enumerate(u_list):
        nm = f"{base}_slash{k}"
        out[nm] = Panel(nm, garment, key, [(u - width / 2, v0), (u + width / 2, v0), (u + width / 2, v1), (u - width / 2, v1)],
                        {"bottom": (0, 1)}, dict(place, layer=1), tack=base)
    return out


def _tabbed(width, drop, tab=8.0, slit=None, flare=3.0):
    """A skirt panel cut into tabs: v rises from the hem (0) to the waist (`drop`), u from 0
    (the centre front or back) to `width` + `flare` at the hem; slits `slit` cm deep every
    `tab` cm. Returns (outline, edges)."""
    slit = drop * 0.75 if slit is None else slit
    w0 = width + flare
    n = max(1, int(round(w0 / tab)))
    step = w0 / n
    hem = [(0.0, 0.0)]
    for k in range(1, n):
        x = k * step
        hem += [(x - 0.5, 0.0), (x, slit), (x + 0.5, 0.0)]
    hem.append((w0, 0.0))
    pts = hem + [(width, drop), (0.0, drop)]
    k = len(hem) - 1
    return pts, {"hem": (0, k), "side": (k, k + 1), "waist": (k + 1, k + 2), "front": (k + 2, 0)}


def _body17(m, garment, key, sleeve_key, chest, waist, collar):
    """Front, back and two-piece sleeves of `coat`'s draft fitted to `chest`/`waist` cm of
    ease, without lapels, cuffs, turnbacks or skirts; the centre front sewn shut (hooks)."""
    panels, seams = coat(_fitted(m, chest, waist))
    keep = {"coat_front", "coat_back", "sleeve_top", "sleeve_under"} | ({"collar"} if collar else set())
    panels = {k: p for k, p in panels.items() if k in keep}
    for k, p in panels.items():
        p.garment = garment
        p.key = sleeve_key if k.startswith("sleeve") else key
        p.place = dict(p.place, gap=min(p.place.get("gap", 2.0), 2.0))
    seams = [sm for sm in seams if {x[0] for x in sm.a + sm.b} <= set(panels) and sm.a[0][1] != "hook"]
    seams.append(Seam([("coat_front", "cf")], [("coat_front", "cf", False, "R")], garment, close=0.05))
    for sm in seams:
        sm.garment = garment
    return panels, seams


def doublet(m=MEASURE, tabs=True, slashed=False, key="doublet", sleeve_key="dsleeve", slash_key="slashes",
            collar=True):
    """The doublet (1620s-1650s): a fitted body closed down the front, a low stand collar,
    two-piece sleeves, below the waist short skirts cut into tabs (`tabs`); `slashed`
    sleeves show the lining through slashes down the top sleeve (paned)."""
    panels, seams = _body17(m, "doublet", key, sleeve_key, chest=12.0, waist=10.0, collar=collar)
    if tabs:
        drop = 13.0
        for nm, base, theta, d in (("tabs_front", "coat_front", 0.0, 1), ("tabs_back", "coat_back", 180.0, -1)):
            pts, ed = _tabbed(panels[base].outline[1][0], drop)
            panels[nm] = Panel(nm, "doublet", key, pts, ed,
                               {"region": "torso", "theta": theta, "dir": d, "u0": 0.0, "at": "waist", "lift": -drop,
                                "gap": 3.0}, skirt=True)
            seams.append(Seam([(base, "waist")], [(nm, "waist", True)], "doublet", ease=0.3))
    if slashed:
        st = panels["sleeve_top"]
        half = st.outline[1][0]
        top = st.outline[2][1]
        panels.update(_slashes("sleeve_top", "doublet", (-half * 0.5, 0.0, half * 0.5), 7.0, top - 4.0, st.place,
                               slash_key))
    return panels, seams


def buffcoat(m=MEASURE, sleeves="dsleeve", key="buff", drop=None):
    """The buff coat (1640s): thick oiled leather, fitted, hooked shut to the waist, long
    skirts to above the knee (open at the back vent), sleeves of cloth (`sleeves` = its
    key), of buff (`sleeves` = `key`) or none (`None`: under a pikeman's armour)."""
    panels, seams = _body17(m, "buffcoat", key, sleeves or key, chest=16.0, waist=12.0, collar=False)
    if sleeves is None:
        panels = {k: p for k, p in panels.items() if not k.startswith("sleeve")}
        seams = [sm for sm in seams if not any(x[0].startswith("sleeve") for x in sm.a + sm.b)]
    fw = panels["coat_front"].outline[1][0]
    bw = panels["coat_back"].outline[1][0]
    drop = m["waist_to_knee"] - 10.0 if drop is None else drop
    for nm, w, theta, d, last in (("buff_skirt_front", fw, 0.0, 1, "front"), ("buff_skirt_back", bw, 180.0, -1, "vent")):
        panels[nm] = Panel(nm, "buffcoat", key, [(0.0, 0.0), (w + 7.0, 0.0), (w, drop), (0.0, drop)],
                           {"hem": (0, 1), "side": (1, 2), "waist": (2, 3), last: (3, 0)},
                           {"region": "torso", "theta": theta, "dir": d, "u0": 0.0, "at": "waist", "lift": -drop,
                            "gap": 3.5}, skirt=True)
    seams += [Seam([("coat_front", "waist")], [("buff_skirt_front", "waist", True)], "buffcoat", ease=0.3),
              Seam([("coat_back", "waist")], [("buff_skirt_back", "waist", True)], "buffcoat", ease=0.3),
              Seam([("buff_skirt_back", "side")], [("buff_skirt_front", "side")], "buffcoat", ease=0.2)]
    return panels, seams


def hose(m=MEASURE, key="hose", top=0.6):
    """Long hose (stockings) from the instep to `top` x the knee-to-waist length above the
    knee, close to the leg, seamed up the back; the top pinned (gartered under the slops)."""
    calf = m["calf"] + 1.0
    ank = m["ankle"] + 5.0
    knee = m["knee"] + 1.0
    h = m["knee_to_ankle"] - 1.0
    up = m["waist_to_knee"] * top
    thigh = m["thigh"] * 0.88 + 1.0
    g = [(0.0, 0.0), (ank * 0.75, -4.0), (ank, 0.0), (calf, h * 0.55), (knee, h), (thigh, h + up), (0.0, h + up),
         (0.0, h * 0.55)]
    P = {"hose": Panel("hose", "hose", key, g, {"bottom": (0, 2), "outer": (2, 5), "top": (5, 6), "start": (6, 0)},
                       {"region": "leg", "theta": 180.0, "dir": 1, "u0": 0.0, "at": "ankle", "lift": 1.0, "gap": 0.8,
                        "pin": "top"})}
    return P, [Seam([("hose", "start", True)], [("hose", "outer")], "hose", close=GAITER_CLOSE)]


def slops(m=MEASURE, key="slops", slash_key="slashes", full=26.0, down=0.45, paned=True):
    """Slops / trunk hose (16th century): the breeches' draft cut short (`down` x the
    crotch-to-knee length below the crotch), `full` cm of girth over the thigh gathered into
    a band round the thigh; `paned`: strips of the lining between the panes."""
    rise = 27.0
    leg0 = m["waist_to_knee"] - rise + 3.0
    cut = leg0 * (1.0 - down)
    mm = dict(m, waist_to_knee=m["waist_to_knee"] - cut, seat=m["seat"] + full * 0.6, thigh=m["thigh"] + full,
              knee=m["thigh"] * 0.8 + full * 0.8)
    panels, seams = breeches(mm)
    band_g = m["thigh"] * 0.85 + 3.0
    for p in panels.values():
        p.garment = "slops"
        p.key = key
        if p.place["region"] == "leg":
            p.place = dict(p.place, lift=p.place.get("lift", 0.0) + cut, gap=2.5)
    panels["knee_band"].outline = [(0.0, 0.0), (band_g, 0.0), (band_g, 4.0), (0.0, 4.0)]
    for sm in seams:
        sm.garment = "slops"
        if sm.a[0][:2] == ("knee_band", "bottom"):
            sm.ease = 1.3
    if paned:
        fr, bk = panels["br_front"], panels["br_back"]
        wf, leg_f = fr.outline[1][0], fr.outline[2][1]
        wb = -bk.outline[6][0]
        leg_b = bk.outline[5][1]
        panels.update(_slashes("br_front", "slops", [wf * f for f in (0.3, 0.65)], 1.5, leg_f + 6.0, fr.place,
                               slash_key, width=1.6))
        panels.update(_slashes("br_back", "slops", [-wb * f for f in (0.3, 0.65)], 1.5, leg_b + 6.0, bk.place,
                               slash_key, width=1.6))
    return panels, seams


def shirt17(m=MEASURE):
    """The shirt's linen at the wrists only, deep turned-back cuffs (the falling band at the
    neck is modelled on the doublet: `drape.falling_band`)."""
    P, seams = shirt(m)
    w = P["wristband"]
    w.garment = "shirt17"
    w.outline = [(0.0, 0.0), (m["wrist"] + 6.0, 0.0), (m["wrist"] + 6.0, 6.0), (0.0, 6.0)]
    seams[0].garment = "shirt17"
    return {"wristband": w}, seams


# --------------------------------------------------------------------------- drafted shells
# (#287 drape pilot, docs/clothing.md "Drape pilot"): garments the frontier, native, Inca and
# meso families cut out of the body hull, drafted and draped instead. The families set the
# material keys (`keyed`), the cut is shared.

def keyed(draft, key, sleeve_key=None):
    """`draft` with every panel in material `key` (sleeves in `sleeve_key`)."""

    def out(m=MEASURE):
        panels, seams = draft(m)
        for k, p in panels.items():
            p.key = sleeve_key if sleeve_key and k.startswith("sleeve") else key
        return panels, seams

    return out


def tunic(m=MEASURE, garment="tunic", key="tunic", sleeves=True, chest=24.0, waist=26.0, below=20.0, flare=4.0):
    """A loose body garment pulled over the head: the coat's body and two-piece sleeves (or
    open armholes without `sleeves`) let out to `chest`/`waist` cm of ease, closed down the
    front and back, a plain neck opening; below the waist skirts (free cloth on the skirt
    bones, so a striding leg does not drag the hem) `below` cm long, sewn shut at the sides,
    the centre front and back. The trade shirt, the buccaneer's bloused shirt, the unku."""
    panels, seams = coat(_fitted(m, chest, waist))
    keep = {"coat_front", "coat_back"} | ({"sleeve_top", "sleeve_under"} if sleeves else set())
    panels = {k: p for k, p in panels.items() if k in keep}
    for p in panels.values():
        p.garment = garment
        p.key = key
        p.place = dict(p.place, gap=min(p.place.get("gap", 2.0), 2.0))
    seams = [sm for sm in seams if {x[0] for x in sm.a + sm.b} <= set(panels) and sm.a[0][1] != "hook"]
    seams.append(Seam([("coat_front", "cf")], [("coat_front", "cf", False, "R")], garment, close=0.05))
    q_seat = (m["seat"] + waist) / 4.0
    for nm, base, theta, d, mid, off in (("skirt_front", "coat_front", 0.0, 1, "cf", 2.0),
                                         ("skirt_back", "coat_back", 180.0, -1, "cb", -2.0)):
        top = panels[base].outline[1][0]
        w = max(top, q_seat + off) + flare
        panels[nm] = Panel(nm, garment, key, [(0.0, 0.0), (w, 0.0), (top, below), (0.0, below)],
                           {"hem": (0, 1), "side": (1, 2), "waist": (2, 3), mid: (3, 0)},
                           {"region": "torso", "theta": theta, "dir": d, "u0": 0.0, "at": "waist", "lift": -below,
                            "gap": 2.5}, skirt=True)
        # Welded shut wherever it closed (the skirts hang off the waist seam).
        seams.append(Seam([(base, "waist")], [(nm, "waist", True)], garment, ease=0.3, close=0.08))
        seams.append(Seam([(nm, mid)], [(nm, mid, False, "R")], garment, close=0.05))
    seams.append(Seam([("skirt_back", "side")], [("skirt_front", "side")], garment, ease=0.2))
    for sm in seams:
        sm.garment = garment
    return panels, seams


def cloak(m=MEASURE, garment="cloak", key="cloak", drop=20.0, front=12.0, spread=10.0, knot=3.0, gap=2.5,
          yoke=7.0, knot_at=None):
    """A cloak or mantle hung from a yoke on the shoulders (drape region `cape`: u runs
    round the neck, v = cm from the neck ring down over the shoulder hull and on down,
    docs/clothing.md "Drape 2"). The yoke (the first `yoke` cm round the neck) is placed on
    the shoulders and pinned while the rest drapes. Symmetric (`knot_at` None): one panel per
    side, sewn down the centre back, `drop` cm below the waist at the back, `spread` cm longer
    over the shoulders, `front` cm down the chest; the fronts knotted at the throat (`knot`
    cm sewn shut), open below. `knot_at` (degrees, drape theta: -45 = the wearer's right
    front shoulder): one panel round the neck from the knot back to it, knotted there (the
    tilmatli). The buccaneer's cape, the yacolla, the tilmatli."""
    back = m["nape_to_waist"] + drop + 2.0  # neck ring to the hem at the centre back
    fr = 10.0 + front  # neck ring to the hem at the centre front
    side = back + spread * 0.6
    hr = m["neck"] / (2 * math.pi) + gap  # the neck ring's radius (cm)
    one = knot_at is None
    turn = 180.0 if one else 360.0
    # u spans the neck ring 2.2 times over (the cloth's rest shape is its placed shape, so
    # this only sets the mesh density round the shoulders and the hem).
    U = math.pi * hr * turn / 180.0 * 2.2

    def length(th):  # neck ring to the hem at drape theta `th`
        c = math.cos(math.radians(th))  # 1 centre front, -1 centre back
        return side + (fr - side) * max(c, 0.0) + (back - side) * max(-c, 0.0)

    th0 = 180.0 if one else float(knot_at)
    n = 16 if one else 32
    u1 = 0.0 if one else 3.0  # the hem ends at the centre back seam, or at the knot's other front
    hem = [(u, length(th0 - turn * u / U)) for u in (U - 3.0 - (U - 3.0 - u1) * k / n for k in range(n + 1))]
    if one:
        pts = [(0.0, 0.0), (U, 0.0), (U, knot)] + hem
        edges = {"neck": (0, 1), "knot": (1, 2), "front": (2, 3), "hem": (3, len(pts) - 1), "cb": (len(pts) - 1, 0)}
        seams = [Seam([("cloak", "cb")], [("cloak", "cb", False, "R")], garment),
                 Seam([("cloak", "knot")], [("cloak", "knot", False, "R")], garment, close=0.05)]
    else:
        pts = [(0.0, 0.0), (U, 0.0), (U, knot)] + hem + [(0.0, knot)]
        e = len(pts) - 1
        edges = {"neck": (0, 1), "knot_b": (1, 2), "front_b": (2, 3), "hem": (3, e - 1), "front_a": (e - 1, e),
                 "knot_a": (e, 0)}
        seams = [Seam([("cloak", "knot_a", True)], [("cloak", "knot_b")], garment, close=0.05)]
    pl = {"region": "cape", "theta": th0, "turn": turn, "span": U, "gap": gap, "pin_v": yoke}
    P = {"cloak": Panel("cloak", garment, key, pts, edges, pl, mirror=one)}
    return P, seams


def band(m=MEASURE, garment="band", key="sash", width=10.0, ease=6.0, lift=-7.0):
    """A sash or girdle wound round the waist: one strip of the waist girth plus `ease`,
    `width` cm wide, from the centre back round both sides, the ends sewn at the front;
    `lift` cm from the waist to its lower edge. Shrinks onto what is under it."""
    g = m["waist"] + ease
    P = {"band": Panel("band", garment, key, [(-g / 2, 0.0), (g / 2, 0.0), (g / 2, width), (-g / 2, width)],
                       {"bottom": (0, 1), "end": (1, 2), "top": (2, 3), "start": (3, 0)},
                       {"region": "torso", "theta": 180.0, "dir": -1, "u0": 0.0, "at": "waist", "lift": lift,
                        "gap": 7.0}, mirror=False)}
    return P, [Seam([("band", "start", True)], [("band", "end")], garment, close=0.05)]


GARMENTS = {"coat": coat, "waistcoat": waistcoat, "breeches": breeches, "gaiters": gaiters, "shirt": shirt,
            "doublet": doublet, "doublet_slashed": lambda m=MEASURE: doublet(m, slashed=True),
            "buffcoat": buffcoat, "buffcoat_bare": lambda m=MEASURE: buffcoat(m, sleeves=None),
            "hose": hose, "slops": slops, "shirt17": shirt17,
            # drafted instead of shells (#287 drape pilot); the families key them (`keyed`)
            "frn_blouse": lambda m=MEASURE: tunic(m, "frn_blouse", "fshirt", chest=30.0, waist=34.0, below=14.0),
            "frn_sash": lambda m=MEASURE: band(m, "frn_sash", "fsash"),
            "frn_cape": lambda m=MEASURE: cloak(m, "frn_cape", "fcloak", drop=18.0, front=10.0, spread=9.0),
            "trade_shirt": lambda m=MEASURE: tunic(m, "trade_shirt", "shirt", chest=12.0, waist=12.0, below=28.0, flare=2.0),
            "unku": lambda m=MEASURE: tunic(m, "unku", "unku_plain", sleeves=False, chest=26.0, waist=30.0,
                                            below=m["waist_to_knee"] - 4.0, flare=2.0),
            "yacolla": lambda m=MEASURE: cloak(m, "yacolla", "yacolla", drop=m["waist_to_knee"] - 18.0, front=14.0,
                                               spread=12.0),
            "tilmatli": lambda m=MEASURE: cloak(m, "tilmatli", "tilmatli", drop=m["waist_to_knee"] - 22.0, front=16.0,
                                                spread=10.0, knot_at=-45.0)}


def uniform(m=MEASURE, garments=ORDER, patterns=None):
    """{garment: (panels, seams)} drafted to `m`: the British infantry uniform by default;
    `patterns` {name: draft} overrides or adds drafts (families register theirs with
    `registry.garment`, which adds to `GARMENTS`, `CLOTH` and `EASE`)."""
    drafts = dict(GARMENTS, **(patterns or {}))
    missing = [g for g in garments if g not in drafts]
    if missing:
        raise KeyError(f"no sewing pattern for {missing} (register it with registry.garment)")
    return {g: drafts[g](m) for g in garments}


# -- layer order ---------------------------------------------------------------------------

# {material key: layer rank}: the game's table (`acr_render::layers::layer_rank`; the test
# `test_layer_ranks_match_the_game` keeps them equal). Skin 0, legwear 1, gaiters and boots 2,
# shirts 3, waistcoats 4, coats and cloaks 5, facings and sashes 6, belts 7, boxes and pouches 8.
# `drape.settle` pushes every ranked layer under the ranked layers above it.
LAYER_RANK = {
    "skin": 0, "paint_red": 0, "paint_black": 0, "paint_white": 0,
    "breeches": 1, "stockings": 1, "hose": 1, "fhose": 1, "leggings": 1,
    "gaiters": 2, "garter": 2, "boots": 2, "boots_high": 2, "jackboots": 2, "slops": 2, "shoes": 2,
    "moccasins": 2, "sandals": 2, "wraps": 2, "pue_wraps": 2, "breechcloth": 2, "hipcloth": 2,
    "maxtlatl": 2, "pue_kilt": 2,
    "shirt": 3, "stock": 3, "fshirt": 3, "war_shirt": 3, "flaps": 3,
    "waistcoat": 4, "doublet": 4, "dsleeve": 4, "slashes": 4, "vest": 4, "fvest": 4,
    "pue_jerkin": 4, "ichcahuipilli": 4, "maya_jacket": 4, "inca_quilt": 4,
    "coat": 5, "tunic": 5, "cloak": 5, "buff": 5, "officer_coat": 5, "matchcoat": 5, "smock": 5,
    "habit": 5, "hunting": 5, "cappa": 5, "fcloak": 5, "armour": 5, "unku_plain": 5,
    "unku_tocapu": 5, "unku_check": 5, "unku_royal": 5, "pue_tunic": 5,
    "facings": 6, "lining": 6, "sash": 6, "fallband": 6, "lapels": 6, "fsash": 6, "pue_sash": 6,
    "band": 6, "lace": 6, "fringe": 6, "meso_trim": 6, "tilmatli": 6, "yacolla": 6,
    "pue_blanket": 6, "pelt": 6, "jaguar": 6,
    "belts": 7, "brass": 7, "belt_hide": 7, "cord": 7, "pue_band": 7,
    "cartridge": 8, "knapsack": 8, "pouch": 8, "sack": 8, "scabbard": 8, "horn": 8, "match": 8,
}



# -- traced patterns -------------------------------------------------------------------------
# (docs/clothing.md "Traced patterns"): historical cutting diagrams digitised into
# `acr-pattern/1` JSON under `ACR_SOURCES/patterns/<source>/<garment>.json` (copyrighted:
# never in the repo), graded to the body and handed to the drape as `Panel`s and `Seam`s.

PATTERN_SCHEMA = "acr-pattern/1"
FIXED = "fixed"  # a grading band that keeps the traced size (no measure for it, or full cloth)
# Length measure a panel's v scales by when its JSON gives no `grade`.
DEFAULT_V = {"torso": "nape_to_waist", "neck": "neck", "arm": "arm", "cape": "nape_to_waist"}


def _bands(spec, fallback):
    """`grade.u` / `grade.v` -> [(v_from, measure)] sorted; a plain string is one band."""
    if spec is None:
        spec = fallback
    if isinstance(spec, str):
        return [(-1e9, spec)]
    return sorted((float(a), str(k)) for a, k in spec)


def load_pattern(path):
    """Reads and checks an `acr-pattern/1` JSON file: counter-clockwise outlines, edge and
    seam references in range, the measures it grades by present in `base`. Returns the dict."""
    import json

    with open(path, encoding="utf-8") as f:
        doc = json.load(f)
    return check_pattern(doc, path)


def check_pattern(doc, where="pattern"):
    if doc.get("schema") != PATTERN_SCHEMA:
        raise ValueError(f"{where}: schema {doc.get('schema')!r}, want {PATTERN_SCHEMA!r}")
    names = {}
    for p in doc["panels"]:
        pl = Panel(p["name"], doc["garment"], p.get("key", doc["garment"]), p["outline"], p["edges"], p["place"])
        if pl.area() <= 0.0:
            raise ValueError(f"{where}: panel {p['name']!r} is not counter-clockwise")
        n = len(pl.outline)
        for e, (i, j) in pl.edges.items():
            if not (0 <= i < n and 0 <= j < n):
                raise ValueError(f"{where}: edge {p['name']}.{e} {i, j} outside 0..{n - 1}")
        g = p.get("grade") or {}
        region = p["place"]["region"]
        for _v, k in _bands(g.get("u"), "chest") + _bands(g.get("v"), DEFAULT_V.get(region, "waist_to_knee")):
            if k != FIXED and k not in doc["base"]:
                raise ValueError(f"{where}: panel {p['name']!r} grades by {k!r}, missing in base")
        names[p["name"]] = pl
    for s in doc["seams"]:
        for link in s["a"] + s["b"]:
            if link[0] not in names or link[1] not in names[link[0]].edges:
                raise ValueError(f"{where}: seam link {link} names no panel edge")
    return doc


def _ratio(m, base, k, ease):
    """Girth ratio with the ease kept constant; lengths (no ease) scale plainly."""
    if k == FIXED:
        return 1.0
    return (m[k] + ease) / (base[k] + ease)


def _grade_v(v, bands, m, base):
    """v' = integral from 0 to v of the band ratio m/base (piecewise constant in v): the
    waist line (v = 0) stays put, above and below scale by their own lengths."""
    def r(t):
        k = bands[0][1]
        for a, kk in bands:
            if t >= a:
                k = kk
        return 1.0 if k == FIXED else m[k] / base[k]

    cuts = sorted({0.0, v} | {a for a, _ in bands if min(0.0, v) < a < max(0.0, v)})
    out = 0.0
    for a, b in zip(cuts, cuts[1:]):
        out += (b - a) * r((a + b) / 2.0)
    return out if v >= 0.0 else -out


GRADE_BLEND = 8.0  # cm: a girth band blends in linearly over this height below its `v_from`


def _grade_u(u, v, bands, m, base, ease):
    """u x the girth ratio of the band at v; a band's ratio blends in over `GRADE_BLEND` cm
    below its `v_from` (no jump in width where one band hands over to the next)."""
    r = _ratio(m, base, bands[0][1], ease.get(bands[0][1], 0.0))
    for a, k in bands[1:]:
        rk = _ratio(m, base, k, ease.get(k, 0.0))
        f = min(max((v - (a - GRADE_BLEND)) / GRADE_BLEND, 0.0), 1.0)
        r = r + (rk - r) * f
    return u * r


def grade_pattern(doc, m=MEASURE):
    """(panels, seams) of the checked pattern `doc` graded from `doc["base"]` to `m`."""
    base = doc["base"]
    e = doc.get("ease", 0.0)
    ease = dict(e) if isinstance(e, dict) else {"chest": float(e), "waist": float(e), "seat": float(e)}
    gname = doc["garment"]
    panels = {}
    for p in doc["panels"]:
        region = p["place"]["region"]
        g = p.get("grade") or {}
        ub = _bands(g.get("u"), "chest")
        vb = _bands(g.get("v"), DEFAULT_V.get(region, "waist_to_knee"))
        out = [(_grade_u(u, v, ub, m, base, ease), _grade_v(v, vb, m, base)) for u, v in p["outline"]]
        place = dict(p["place"])
        if "lift" in place:
            place["lift"] = _grade_v(float(place["lift"]), vb, m, base)
        if "u0" in place:
            place["u0"] = _grade_u(float(place["u0"]), 0.0, ub, m, base, ease)
        panels[p["name"]] = Panel(p["name"], gname, p.get("key", gname), out, {k: tuple(v) for k, v in p["edges"].items()},
                                  place, mirror=p.get("mirror", True), skirt=p.get("skirt", False),
                                  grain=tuple(p.get("grain", (0.0, 1.0))), tack=p.get("tack"))
    seams = [Seam([tuple(x) for x in s["a"]], [tuple(x) for x in s["b"]], gname, ease=s.get("ease", 0.15),
                  close=s.get("close")) for s in doc["seams"]]
    return panels, seams


def traced(path):
    """A draft `draft(m) -> (panels, seams)` reading the traced pattern at `path` (checked
    once, graded per call)."""
    doc = load_pattern(path)

    def draft(m=MEASURE):
        return grade_pattern(doc, m)

    draft.pattern = doc
    return draft


def pattern_path(rel):
    """`ACR_SOURCES/patterns/<rel>.json` (`rel` e.g. "waugh/doublet_1635")."""
    import os

    root = os.environ.get("ACR_SOURCES")
    if not root:
        raise RuntimeError(f"traced pattern {rel!r}: ACR_SOURCES is not set (the patterns are local sources)")
    return os.path.join(root, "patterns", rel + ".json")
