"""Sewing patterns of the draped uniform (pure Python, no Blender; docs/clothing.md)."""

import os
import sys
import math
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "lib"))

from acr_assets import garments as gm  # noqa: E402


class Patterns(unittest.TestCase):
    def setUp(self):
        self.uni = gm.uniform()

    def test_every_garment_is_drafted(self):
        self.assertEqual(set(self.uni), set(gm.ORDER))
        coat, _ = self.uni["coat"]
        for name in ("coat_front", "lapel", "coat_back", "skirt_front", "skirt_back", "turnback",
                     "sleeve_top", "sleeve_under", "cuff", "collar"):
            self.assertIn(name, coat)
        self.assertEqual({p for p, x in self.uni["breeches"][0].items()},
                         {"br_front", "br_back", "waistband", "knee_band"})
        self.assertEqual(coat["lapel"].tack, "coat_front")

    def test_outlines_counter_clockwise_and_edges_valid(self):
        for g, (panels, seams) in self.uni.items():
            for p in panels.values():
                self.assertGreater(p.area(), 0.0, f"{g}/{p.name} is clockwise")
                for e, (i, j) in p.edges.items():
                    self.assertTrue(0 <= i < len(p.outline) and 0 <= j < len(p.outline), (p.name, e))
                    self.assertGreaterEqual(len(p.edge_points(e)), 2)
                self.assertIn(p.place["region"], ("torso", "arm", "leg", "neck"))
            for s in seams:
                for name, edge, _rev, side in s.a + s.b:
                    self.assertIn(edge, panels[name].edges, (g, name, edge))
                    self.assertIn(side, ("L", "R"))

    def test_seam_lengths_match_within_ease(self):
        for g, (panels, seams) in self.uni.items():
            for s in seams:
                a, b = gm.seam_lengths(panels, s)
                lo, hi = sorted((a, b))
                self.assertLessEqual(hi, lo * (1.0 + s.ease) + 0.5, (g, s.a, s.b, a, b))

    def test_period_cut(self):
        coat, _ = self.uni["coat"]
        m = gm.MEASURE
        # The skirts fall from the waist to the knee; the cuffs are deep.
        self.assertGreaterEqual(coat["skirt_front"].bounds()[3], m["waist_to_knee"])
        self.assertTrue(coat["skirt_front"].skirt and coat["skirt_back"].skirt and coat["turnback"].skirt)
        self.assertFalse(coat["coat_front"].skirt)
        self.assertGreaterEqual(coat["cuff"].bounds()[3], 10.0)
        # Cutaway: the skirt's front edge slopes back from the waist to the hem.
        fs = coat["skirt_front"].edge_points("front")
        self.assertGreater(fs[-1][0], fs[0][0] - 3.0)
        # Facings and lining are their own materials.
        self.assertEqual({coat[n].key for n in ("lapel", "cuff", "collar")}, {"facings"})
        self.assertEqual(coat["turnback"].key, "lining")

    def test_resample_keeps_ends_and_spacing(self):
        pts = [(0.0, 0.0), (3.0, 0.0), (3.0, 4.0)]
        r = gm.resample(pts, 7)
        self.assertEqual(len(r), 8)
        self.assertAlmostEqual(r[0][0], 0.0)
        self.assertAlmostEqual(r[-1][1], 4.0)
        self.assertAlmostEqual(gm.polyline_length(r), 7.0, places=6)

    def test_half_score_prefers_the_hanging_half(self):
        # `drape.mirror_half`: a skirt drawn up 14 cm loses against a few more folds.
        self.assertLess(gm.half_score(30, 40, 0.0), gm.half_score(10, 5, 14.0))
        self.assertLess(gm.half_score(3, 2, 0.0), gm.half_score(30, 40, 0.0))
        self.assertEqual(gm.half_score(1, 2, -3.0), 3)
        self.assertTrue(gm.CLOTH["coat"]["mirror"])

    def test_coat_body_long_enough_for_the_shoulder(self):
        # The body is `COAT_RISE` longer than the back length + 2 below the armhole line
        # and starts half of that below the waist line; the armhole keeps its depth.
        p = gm.coat()[0]
        top = gm.MEASURE["nape_to_waist"] + 2.0 + gm.COAT_RISE
        self.assertAlmostEqual(p["coat_back"].bounds()[3], top, places=6)
        ah = gm.MEASURE["nape_to_waist"] * 0.5 + gm.SCYE_DROP
        self.assertAlmostEqual(p["coat_front"].outline[2][1], top - ah, places=6)
        for n in ("coat_front", "coat_back"):
            self.assertAlmostEqual(p[n].place["lift"], -gm.COAT_RISE / 2)

    def test_drafted_to_measure(self):
        big = dict(gm.MEASURE, chest=gm.MEASURE["chest"] + 12.0)
        a = gm.coat()[0]["coat_front"].bounds()[2]
        b = gm.coat(big)[0]["coat_front"].bounds()[2]
        self.assertAlmostEqual(b - a, 3.0, places=6)



class NativeLegwear(unittest.TestCase):
    """The authored leggings' and breechcloth's pure shapes (#523), on made-up numbers."""

    def test_legging_top_is_cut_on_the_slant(self):
        self.assertAlmostEqual(gm.legging_top(0.0, 0.76, 0.90), 0.90)
        self.assertAlmostEqual(gm.legging_top(math.pi, 0.76, 0.90), 0.76)
        tops = [gm.legging_top(math.pi * i / 12, 0.76, 0.90) for i in range(13)]
        self.assertEqual(tops, sorted(tops, reverse=True))
        self.assertAlmostEqual(gm.legging_top(0.5, 0.76, 0.90), gm.legging_top(-0.5, 0.76, 0.90))

    def test_loosen_fills_hollows_only(self):
        r = [5.0, 5.0, 3.0, 5.0, 6.0, 4.0]
        out = gm.loosen(r, passes=1)
        self.assertEqual(out[2], 5.0)
        self.assertTrue(all(b >= a for a, b in zip(r, out)))
        self.assertEqual(gm.loosen([4.0, 4.0, 4.0]), [4.0, 4.0, 4.0])
        self.assertEqual(gm.loosen([1.0, 5.0, 5.0, 5.0], passes=1, closed=True)[0], 5.0)

    def test_plumb_hangs_from_the_furthest_point(self):
        self.assertEqual(gm.plumb([1.0, 3.0, 2.0, None, 4.0, 1.0]), [1.0, 3.0, 3.0, 3.0, 4.0, 4.0])
        self.assertEqual(gm.plumb([-1.0, -3.0, -2.0], out=-1.0), [-1.0, -3.0, -3.0])
        self.assertEqual(gm.plumb([None, 2.0]), [2.0, 2.0])

    def test_flap_bends_as_one_cloth(self):
        top = gm.flap_follow(0.05, 1.0, 0.93)
        self.assertEqual(top, {"hips": 1.0, "thigh_l": 0.0, "thigh_r": 0.0})
        left, right, mid = (gm.flap_follow(x, 0.70, 0.93) for x in (0.12, -0.12, 0.0))
        self.assertAlmostEqual(left["thigh_r"], 0.0)
        self.assertAlmostEqual(right["thigh_l"], 0.0)
        self.assertAlmostEqual(mid["thigh_l"], mid["thigh_r"])
        # A modest follow, less at the hem than above it (#523: the hem swings less).
        hem = gm.flap_follow(0.12, 0.57, 0.93)
        self.assertLess(hem["thigh_l"], left["thigh_l"])
        self.assertLess(left["thigh_l"], 0.8)
        self.assertGreater(hem["thigh_l"], 0.4)
        # One smooth blend across the width: no step between neighbouring columns.
        xs = [-0.12 + 0.24 * i / 16 for i in range(17)]
        ws = [gm.flap_follow(x, 0.70, 0.93)["thigh_l"] for x in xs]
        self.assertEqual(ws, sorted(ws))
        self.assertLess(max(b - a for a, b in zip(ws, ws[1:])), 0.12)
        for w in (left, right, mid, hem, gm.flap_follow(0.01, 0.85, 0.93)):
            self.assertAlmostEqual(sum(w.values()), 1.0)

    def test_fill_ring_bridges_gaps(self):
        self.assertEqual(gm.fill_ring([1.0, None, None, 4.0]), [1.0, 2.0, 3.0, 4.0])
        self.assertEqual(gm.fill_ring([None, 2.0, None, 2.0]), [2.0, 2.0, 2.0, 2.0])
        self.assertEqual(gm.fill_ring([None, None], default=0.3), [0.3, 0.3])

    def test_hang_keeps_the_skirt_as_wide_as_the_hips(self):
        self.assertEqual(gm.hang([1.0, 2.0, 3.0, 2.5, 1.0], 2), [3.0, 3.0, 3.0, 2.5, 1.0])
        self.assertEqual(gm.hang([3.0, 2.0], 5), [3.0, 2.0])


if __name__ == "__main__":
    unittest.main()


def _inside(pt, poly):
    x, y = pt
    c = False
    for (x0, y0), (x1, y1) in zip(poly, poly[1:] + poly[:1]):
        if (y0 > y) != (y1 > y) and x < x0 + (y - y0) * (x1 - x0) / (y1 - y0):
            c = not c
    return c


class TailoredTrim(unittest.TestCase):
    """`garments.TRIM_PANELS`: dropped after the drape and rebuilt as bands (`drape.tailored`)."""

    def test_trim_panels_can_be_dropped(self):
        panels, seams = gm.coat()
        for n in gm.TRIM_PANELS:
            self.assertIn(n, panels)
            self.assertFalse(panels[n].skirt)
            if panels[n].tack:
                # An inlay stays as cloth of its base panel, which is kept (`drape.dress`).
                self.assertNotIn(panels[n].tack, gm.TRIM_PANELS)
                continue
            # Each is sewn to a kept panel: that seam is what the band is fitted to.
            self.assertTrue(any({l[0] for l in s.a} == {n} and not {l[0] for l in s.b} & set(gm.TRIM_PANELS)
                                for s in seams), n)
        self.assertFalse([p.name for p in panels.values() if p.tack in gm.TRIM_PANELS])


class TailoredPanels(unittest.TestCase):
    """Synthetic data: the authored lapel's edges and the flat strap (#523)."""

    def test_lapel_edges_are_straight_through_a_ragged_trace(self):
        pts = []
        for j in range(41):
            z = 1.0 + 0.4 * j / 40
            jag = 0.004 * (-1) ** j
            pts += [(0.02 + 0.05 * (z - 1.0) + max(jag, 0.0), -0.1, z), (0.09 + 0.10 * (z - 1.0) - max(jag, 0.0), -0.1, z)]
        z0, z1, inner, outer = gm.lapel_edges(pts)
        self.assertAlmostEqual(z0, 1.0)
        self.assertAlmostEqual(z1, 1.4)
        self.assertAlmostEqual(inner[0], 0.02, delta=0.004)
        self.assertAlmostEqual(inner[1], 0.04, delta=0.004)
        self.assertAlmostEqual(outer[0], 0.09, delta=0.004)
        # 9 cm wide at the top: held to the widest lapel
        self.assertAlmostEqual(outer[1] - inner[1], 0.085)

    def test_ease_closed_shuts_a_seam_and_stretches_the_cloth_round_it(self):
        # Two strips of 6 vertices, 9 cm apart at their sewn ends.
        pts = [(float(i), 0.0, 0.0) for i in range(6)] + [(14.0 + i, 0.0, 0.0) for i in range(6)]
        edges = [(i, i + 1) for i in range(5)] + [(6 + i, 7 + i) for i in range(5)]
        out = gm.ease_closed(pts, edges, [[5, 6]], passes=3)
        self.assertEqual(out[5], out[6])
        self.assertAlmostEqual(out[5][0], 9.5)
        xs = [p[0] for p in out]
        self.assertEqual(xs[:6], sorted(xs[:6]))  # no fold
        self.assertGreater(xs[4], 4.0)
        self.assertGreater(xs[3], 3.0)
        self.assertEqual(out[1], pts[1])  # past the rings: untouched
        held = gm.ease_closed(pts, edges, [[5, 6]], passes=3, fixed=(4,))
        self.assertEqual(held[4], pts[4])

    def test_trim_line_follows_the_lapel_edge(self):
        self.assertAlmostEqual(gm.trim_line(1.3, 1.1, 1.5, (0.10, 0.0)), 0.05)
        self.assertAlmostEqual(gm.trim_line(1.6, 1.1, 1.5, (0.10, 0.0), floor=0.006), 0.006)

    def test_strap_rows_bridge_folds_and_cover_every_hit(self):
        hits = [[-0.10, -0.10, -0.10, -0.10, -0.10] for _ in range(9)]
        hits[4] = [-0.10, -0.10, -0.13, -0.10, -0.10]  # a fold standing out in front
        hits[7] = [None, None, None, None, -0.10]
        rows = gm.strap_rows(hits, 0.004, -1)
        self.assertIsNone(rows[7])
        for row, r in zip(hits, rows):
            if r is not None:
                for k, y in enumerate(row):
                    self.assertLessEqual(r[0] + (r[1] - r[0]) * k / 4, y - 0.004 + 1e-9)
        self.assertAlmostEqual(rows[4][0], -0.134)
        self.assertLess(rows[3][0], -0.104)  # eased in along the run
        back = gm.strap_rows([[0.10, 0.12, 0.14]], 0.004, 1)
        self.assertAlmostEqual(back[0][0], 0.104)
        self.assertAlmostEqual(back[0][1], 0.144)

    def test_taut_strap_bridges_the_waist(self):
        # chest out, waist in, hip out: the taut belt runs straight from chest to hip
        depth = [-0.10, -0.14, -0.10, -0.08, -0.10, -0.13]
        rows = gm.strap_rows([[y] * 3 for y in depth], 0.0, -1, passes=0, taut=True)
        self.assertAlmostEqual(rows[1][0], -0.14)
        self.assertAlmostEqual(rows[5][0], -0.13)
        self.assertAlmostEqual(rows[3][0], -0.135)
        for r, y in zip(rows, depth):
            self.assertLessEqual(r[0], y + 1e-9)
        back = gm.strap_rows([[y] * 3 for y in (0.10, 0.06, 0.10)], 0.0, 1, passes=0, taut=True)
        self.assertAlmostEqual(back[1][1], 0.10)


class Period17(unittest.TestCase):
    """The 17th century's garments (docs/clothing.md, "17th century")."""

    NAMES = ("doublet", "doublet_slashed", "buffcoat", "buffcoat_bare", "hose", "slops", "shirt17")

    def setUp(self):
        self.uni = gm.uniform(garments=self.NAMES)

    def test_registered_with_cloth_and_spacing(self):
        from acr_assets import registry

        for g in self.NAMES:
            self.assertIn(g, gm.CLOTH)
            self.assertIn(g, registry.SPACING)

    def test_outlines_seams_and_inlays(self):
        for g, (panels, seams) in self.uni.items():
            for p in panels.values():
                self.assertGreater(p.area(), 0.0, (g, p.name))
                if p.tack:
                    for q in p.outline:
                        self.assertTrue(_inside(q, panels[p.tack].outline), (g, p.name, q))
            for s in seams:
                for name, edge, _rev, _side in s.a + s.b:
                    self.assertIn(edge, panels[name].edges, (g, name, edge))
                a, b = gm.seam_lengths(panels, s)
                lo, hi = sorted((a, b))
                self.assertLessEqual(hi, lo * (1.0 + s.ease) + 0.5, (g, s.a, s.b, a, b))

    def test_period_cut(self):
        m = gm.MEASURE
        d, _ = self.uni["doublet"]
        # Tabbed skirts: short, free, slit into tabs; closed down the front; no lapels.
        self.assertTrue(d["tabs_front"].skirt and d["tabs_back"].skirt)
        self.assertLess(d["tabs_front"].bounds()[3], 16.0)
        self.assertGreaterEqual(len(d["tabs_front"].edge_points("hem")), 7)
        self.assertNotIn("lapel", d)
        ds, _ = self.uni["doublet_slashed"]
        self.assertEqual({p.key for p in ds.values() if p.tack == "sleeve_top"}, {"slashes"})
        # The buff coat: buff leather, skirts to above the knee, stiffer and heavier cloth.
        b, _ = self.uni["buffcoat"]
        self.assertEqual(b["coat_front"].key, "buff")
        self.assertGreater(b["buff_skirt_front"].bounds()[3], m["waist_to_knee"] * 0.7)
        self.assertGreater(gm.CLOTH["buffcoat"]["bending"], 4 * gm.CLOTH["doublet"]["bending"])
        bare, _ = self.uni["buffcoat_bare"]
        self.assertFalse(any(k.startswith("sleeve") for k in bare))
        # Slops: full (gathered into the thigh band), paned.
        sl, seams = self.uni["slops"]
        band = next(s for s in seams if s.a[0][:2] == ("knee_band", "bottom"))
        a, b2 = gm.seam_lengths(sl, band)
        self.assertGreater(b2, a * 1.2)
        self.assertTrue(any(p.key == "slashes" for p in sl.values()))
        # Long hose reach above the knee.
        h, _ = self.uni["hose"]
        self.assertGreater(h["hose"].bounds()[3], m["knee_to_ankle"] + 20.0)


class Pikeman1640(unittest.TestCase):
    """The pikeman's legs and collar of about 1640 (#523): full breeches gathered into a
    knee band, stockings into the shoe, the falling band laid flat."""

    def test_breeches_are_full_and_gathered_into_a_band_below_the_knee(self):
        m = gm.MEASURE
        plain, _ = gm.breeches(m)
        full, seams = gm.breeches17(m)
        self.assertTrue(all(p.garment == "breeches17" and p.area() > 0 for p in full.values()))
        # Fuller round the thigh by the draft's `full`, longer than the knee.
        self.assertGreater(full["br_front"].bounds()[2], plain["br_front"].bounds()[2] + 5.0)
        self.assertGreater(full["br_front"].bounds()[3], plain["br_front"].bounds()[3] + 5.0)
        # The gather: the legs' bottom edge is much longer than the band it is sewn to, and
        # the band hangs below the seam (its top edge is sewn), once round the leg, pinned.
        g = next(s for s in seams if s.a[0][0] == "knee_band" and s.b[0][1] == "knee")
        self.assertEqual(g.a[0][:3], ("knee_band", "top", True))
        a, b = gm.seam_lengths(full, g)
        self.assertGreater(b, a * 1.3)
        band = full["knee_band"]
        self.assertLess(band.place["lift"], 0.0)
        self.assertEqual(band.place["pin"], "all")
        self.assertAlmostEqual(band.place["wrap"], band.bounds()[2])
        self.assertLess(band.bounds()[2], m["knee"] + 6.0)  # closed on the leg
        from acr_assets import registry

        self.assertLess(registry.SPACING["breeches17"], 2.0)  # fine enough for folds

    def test_pleats_are_regular_and_only_stand_outwards(self):
        n = gm.CLOTH["breeches17"]["pleats"]["n"]
        ys = [gm.pleat_wave(k / 600.0, n) for k in range(601)]
        self.assertTrue(all(0.0 <= y <= 1.0 for y in ys))
        peaks = [k for k in range(1, 600) if ys[k - 1] < ys[k] >= ys[k + 1]]
        self.assertEqual(len(peaks), n)
        self.assertEqual({b - a for a, b in zip(peaks, peaks[1:])}, {600 // n})  # evenly spaced
        self.assertAlmostEqual(gm.pleat_wave(0.0, n), 0.0)  # a valley on each seam
        self.assertAlmostEqual(gm.pleat_wave(1.0, n), 0.0)
        # The buff coat's cloth sleeves are shaped (`drape.sleeve_shape`), its skirts flare
        # over the full breeches.
        self.assertIn("sleeve_shape", gm.CLOTH["buffcoat"])
        sl = gm.CLOTH["buffcoat"]["sleeve_shape"]
        self.assertGreaterEqual(sl["full"], 0.015)  # a fuller upper arm
        self.assertTrue(0.01 <= sl["show"] <= 0.02)  # the shirt cuff shows 1-2 cm beyond the hem
        self.assertIn("wing", sl)
        # The breeches: calm legs, the top kept under the coat, no folds there, the band closed.
        pl = gm.CLOTH["breeches17"]["pleats"]
        self.assertGreater(pl["calm"]["round"], pl["calm"]["along"])  # even round the leg first
        self.assertLessEqual(pl["cover"]["off"], 0.01)
        self.assertGreater(pl["under"], 15.0)
        self.assertEqual(pl["close"]["panel"], "knee_band")
        self.assertIn("knee_band", gm.breeches17(gm.MEASURE)[0])
        b, _ = gm.buffcoat(gm.MEASURE)
        sk = b["buff_skirt_front"]
        self.assertGreater(sk.outline[1][0] - sk.outline[2][0], 10.0)

    def test_stockings_end_level_in_the_shoe_and_under_the_band(self):
        m = gm.MEASURE
        st, seams = gm.GARMENTS["stockings17"](m)
        h = st["hose"]
        bottom = h.edge_points("bottom")
        self.assertEqual(len({v for _u, v in bottom}), 1)  # level: no tongue over the instep
        self.assertLess(bottom[0][1], 0.0)
        top = h.bounds()[3] - (m["knee_to_ankle"] - 1.0)
        self.assertTrue(2.0 < top < 10.0)  # above the knee band, well under the breeches
        self.assertTrue(gm.CLOTH["stockings17"]["in_shoes"])
        self.assertEqual(seams[0].garment, "stockings17")
        # The long hose keep their cut.
        self.assertEqual(len(gm.hose(m)[0]["hose"].outline), 8)

    def test_falling_band_lies_on_the_high_points_at_its_own_width(self):
        # Deeper in front than behind, symmetric about the centre front.
        self.assertGreater(gm.band_width(-math.pi / 2), gm.band_width(math.pi / 2) + 3.0)
        self.assertAlmostEqual(gm.band_width(-math.pi / 2 + 0.4), gm.band_width(-math.pi / 2 - 0.4))
        # A hollow between the collar and the corslet's edge is bridged, lumps are not followed.
        prof = [(0.0, 1.0), (1.0, 0.2), (2.0, 0.9), (3.0, 0.1), (4.0, 0.0)]
        self.assertEqual(gm.upper_hull(prof), [(0.0, 1.0), (2.0, 0.9), (4.0, 0.0)])
        pts = gm.lay_over(prof, [0.0, 1.0, 2.0, 3.0, 6.0])
        self.assertEqual(pts[0], (0.0, 1.0))
        for (r0, z0), (r1, z1), d in zip(pts, pts[1:], (1.0, 1.0, 1.0, 3.0)):
            self.assertGreater(r1, r0)  # never folds back over itself
            self.assertLess(abs(math.hypot(r1 - r0, z1 - z0) - d), 0.05)  # keeps its width
        # Narrow on the shoulders: a collar, not a cape.
        self.assertLess(gm.band_width(0.0, 11.0, 5.5), 6.0)
        # `lift` moves it off the surface along the normal.
        up = gm.lay_over([(0.0, 0.0), (1.0, 0.0)], [0.5], lift=0.1)
        self.assertAlmostEqual(up[0][1], 0.1)


class DrapePilot(Period17):
    """Shell garments drafted and draped instead (#287 drape pilot): shirts and tunics,
    cloaks, the sash."""

    NAMES = ("frn_blouse", "frn_sash", "frn_cape", "trade_shirt", "unku", "yacolla", "tilmatli", "nat_leggings",
             "nat_breechcloth")

    def test_period_cut(self):
        m = gm.MEASURE
        unku, _ = self.uni["unku"]
        self.assertFalse(any(k.startswith("sleeve") for k in unku))  # open armholes
        self.assertTrue(unku["skirt_front"].skirt)
        self.assertGreater(unku["skirt_front"].bounds()[3], m["waist_to_knee"] * 0.8)
        shirt, _ = self.uni["trade_shirt"]
        self.assertIn("sleeve_top", shirt)
        for g in ("frn_cape", "yacolla", "tilmatli"):
            cl, seams = self.uni[g]
            # Hung from a pinned yoke round the neck, down past the waist at the back, knotted.
            self.assertGreater(cl["cloak"].bounds()[3], m["nape_to_waist"] + 15.0)
            self.assertEqual(cl["cloak"].place["region"], "cape")
            self.assertGreater(cl["cloak"].place["pin_v"], 0.0)
            self.assertTrue(any(s.close for s in seams))
        # Symmetric cloaks: sewn down the centre back; the tilmatli: one piece knotted on the
        # right shoulder.
        self.assertEqual({s.b[0][3] for s in self.uni["yacolla"][1]}, {"R"})
        self.assertFalse(self.uni["tilmatli"][0]["cloak"].mirror)
        sash, _ = self.uni["frn_sash"]
        self.assertGreater(sash["band"].bounds()[2] - sash["band"].bounds()[0], m["waist"])
        self.assertEqual(gm.keyed(gm.GARMENTS["unku"], "unku_tocapu")()[0]["coat_front"].key, "unku_tocapu")
        # Side-seam leggings (#307): high on the thigh, looser than the leg, sewn up the outer
        # side and closed, the top pinned, a garter inlay below the knee.
        leg, seams = self.uni["nat_leggings"]
        lo = leg["legging"]
        self.assertGreater(lo.bounds()[3], m["knee_to_ankle"] + 15.0)
        self.assertGreater(lo.outline[1][0], m["ankle"])
        self.assertEqual((lo.place["region"], lo.place["theta"], lo.place["pin"]), ("leg", 90.0, "top"))
        self.assertTrue(seams[0].close)
        self.assertEqual((leg["garter"].tack, leg["garter"].key), ("legging", "garter"))
        self.assertLess(leg["garter"].bounds()[3], m["knee_to_ankle"])
        # The breechcloth (#307): one strip through the crotch and over the belt, flaps wider
        # than the gathered band, the folds over the belt pinned, the marks in order.
        bc, seams = self.uni["nat_breechcloth"]
        c = bc["cloth"]
        vf, vb, L = c.place["marks"]
        self.assertEqual((c.place["region"], c.mirror, seams), ("sagittal", False, []))
        self.assertTrue(0.0 < vf < vb < L and abs(c.bounds()[3] - L) < 1e-6)
        self.assertGreater(vb - vf, m["seat"] * 0.5)
        self.assertGreater(c.bounds()[2] - c.bounds()[0], 20.0)
        self.assertTrue(all(vf - 2.0 <= lo < hi <= vb + 2.0 for lo, hi in c.place["pin_vr"]))


class LayerRanks(unittest.TestCase):
    """The build's layer order (`garments.LAYER_RANK`, `drape.settle`) is the game's
    (`acr_render::layers::layer_rank`, the shader's depth bias), and every cloth key of the
    base registry has a rank or is listed as no layer."""

    def _game(self):
        import re

        path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "..",
                            "crates", "acr-render", "src", "layers.rs")
        if not os.path.exists(path):
            self.skipTest("no game sources")
        with open(path, encoding="utf-8") as f:
            src = f.read()
        body = src[src.index("pub fn layer_rank"):src.index("_ => return None")]
        ranks = {}
        for arm in re.finditer(r"((?:\"\w+\"\s*\|?\s*)+)=>\s*(\d+)", body):
            for k in re.findall(r"\"(\w+)\"", arm.group(1)):
                ranks[k] = int(arm.group(2))
        nl = src[src.index("pub const NOT_LAYERS"):]
        return ranks, set(re.findall(r"\"(\w+)\"", nl[:nl.index("];")]))

    def test_layer_ranks_match_the_game(self):
        ranks, _ = self._game()
        self.assertEqual(gm.LAYER_RANK, ranks)

    def test_every_cloth_key_has_a_rank(self):
        from acr_assets import registry

        ranks, not_layers = self._game()
        self.assertFalse(set(ranks) & not_layers)
        cloth = {k for k, layer in registry.LAYER_OF.items() if layer in ("wool", "linen", "leather", "hide", "flesh")}
        missing = sorted(k for k in cloth if k not in ranks and k not in not_layers)
        self.assertEqual(missing, [], "give these keys a rank in layers.rs and garments.LAYER_RANK")


def synthetic_pattern():
    """A made-up pattern (never traced data): a rectangular torso front and back with a
    skirt below the waist, one tube sleeve."""
    torso = {"u": [[-200, "waist"], [10, "chest"]], "v": [[-200, "waist_to_knee"], [0, "nape_to_waist"]]}
    return {
        "schema": "acr-pattern/1", "garment": "test_body",
        "base": {"chest": 90.0, "waist": 80.0, "nape_to_waist": 40.0, "waist_to_knee": 50.0, "arm": 60.0,
                 "upper_arm": 30.0},
        "ease": 10.0,
        "panels": [
            {"name": "front", "key": "coat", "outline": [[0, -20], [25, -20], [25, 40], [0, 40]],
             "edges": {"hem": [0, 1], "side": [1, 2], "top": [2, 3], "cf": [3, 0]},
             "place": {"region": "torso", "theta": 0.0, "dir": 1, "u0": 0.0, "at": "waist", "lift": 4.0},
             "grade": torso},
            {"name": "back", "key": "coat", "outline": [[0, -20], [25, -20], [25, 40], [0, 40]],
             "edges": {"hem": [0, 1], "side": [1, 2], "top": [2, 3], "cb": [3, 0]},
             "place": {"region": "torso", "theta": 180.0, "dir": -1, "u0": 0.0, "at": "waist"},
             "grade": torso},
            {"name": "sleeve", "key": "coat", "outline": [[-20, 0], [20, 0], [20, 60], [-20, 60]],
             "edges": {"wrist": [0, 1], "seam_f": [1, 2], "cap": [2, 3], "seam_b": [3, 0]},
             "place": {"region": "arm", "theta": 0.0, "dir": 1, "u0": 0.0, "at": "wrist"},
             "grade": {"u": [[-200, "upper_arm"]], "v": "arm"}},
        ],
        "seams": [{"a": [["front", "side"]], "b": [["back", "side"]]},
                  {"a": [["back", "cb"]], "b": [["back", "cb", False, "R"]]},
                  {"a": [["sleeve", "seam_f"]], "b": [["sleeve", "seam_b", True]], "close": 0.05}],
    }


class SeatWeights(unittest.TestCase):
    """`seat_weights`: a rider's skirt on the chains of its own side (synthetic weights)."""

    W = {"rider.hips": 0.2, "rider.skirt_fl_01": 0.3, "rider.skirt_bl_01": 0.2, "rider.skirt_br_02": 0.1, "rider.thigh_l": 0.2}

    def test_front_stays_on_the_front_chain_of_its_side(self):
        w = gm.seat_weights(self.W, 0.1, -0.10, "rider.")
        self.assertAlmostEqual(sum(w.values()), 1.0)
        self.assertEqual({b for b in w if "skirt" in b}, {"rider.skirt_fl_01", "rider.skirt_fl_02"})
        self.assertAlmostEqual(w["rider.thigh_l"], 0.2)

    def test_back_splits_at_the_vent_and_drops_the_thigh(self):
        for x, side in ((0.01, "l"), (-0.01, "r")):
            w = gm.seat_weights(self.W, x, 0.12, "rider.")
            self.assertAlmostEqual(sum(w.values()), 1.0)
            self.assertEqual(set(w), {"rider.hips", f"rider.skirt_b{side}_01", f"rider.skirt_b{side}_02"})
            self.assertAlmostEqual(w[f"rider.skirt_b{side}_01"], 0.7)

    def test_side_seam_blends_and_keeps_four(self):
        w = gm.seat_weights(self.W, 0.2, 0.01, "rider.")
        self.assertLessEqual(len(w), 4)
        self.assertAlmostEqual(sum(w.values()), 1.0)
        self.assertIn("rider.skirt_fl_01", w)
        self.assertIn("rider.skirt_bl_01", w)

    def test_waist_seam_vertices_are_left_alone(self):
        w = {"rider.hips": 0.6, "rider.spine": 0.4}
        self.assertEqual(gm.seat_weights(w, 0.1, 0.1, "rider."), w)


class Traced(unittest.TestCase):
    """`garments.traced`: acr-pattern/1 JSON graded to the body (docs/clothing.md "Traced
    patterns"), on a synthetic pattern."""

    def setUp(self):
        import json
        import tempfile

        self.doc = synthetic_pattern()
        fd, self.path = tempfile.mkstemp(suffix=".json")
        with os.fdopen(fd, "w") as f:
            json.dump(self.doc, f)

    def tearDown(self):
        os.remove(self.path)

    def test_base_measures_give_the_pattern_back(self):
        draft = gm.traced(self.path)
        panels, seams = draft(dict(gm.MEASURE, **self.doc["base"]))
        self.assertEqual(panels["front"].outline, [(0, -20), (25, -20), (25, 40), (0, 40)])
        self.assertEqual(len(seams), 3)
        self.assertEqual(seams[2].close, 0.05)
        self.assertEqual(seams[1].b[0], ("back", "cb", False, "R"))
        self.assertTrue(all(p.garment == "test_body" for p in panels.values()))

    def test_girths_keep_their_ease(self):
        m = dict(gm.MEASURE, chest=110.0, waist=90.0, nape_to_waist=40.0, waist_to_knee=50.0)
        panels, _ = gm.traced(self.path)(m)
        top = panels["front"].outline[2]  # v = 40: the chest band
        hem = panels["front"].outline[1]  # v = -20: the waist band
        self.assertAlmostEqual(top[0], 25 * (110 + 10) / (90 + 10))
        self.assertAlmostEqual(hem[0], 25 * (90 + 10) / (80 + 10))

    def test_lengths_scale_from_the_waist_line_per_band(self):
        m = dict(gm.MEASURE, chest=90.0, waist=80.0, nape_to_waist=44.0, waist_to_knee=40.0)
        panels, _ = gm.traced(self.path)(m)
        f = panels["front"]
        self.assertAlmostEqual(f.outline[2][1], 40 * 44 / 40)
        self.assertAlmostEqual(f.outline[1][1], -20 * 40 / 50)
        self.assertAlmostEqual(f.place["lift"], 4 * 44 / 40)
        self.assertAlmostEqual(panels["sleeve"].outline[2][1], 60 * gm.MEASURE["arm"] / 60)

    def test_widths_blend_between_bands(self):
        m = dict(gm.MEASURE, chest=110.0, waist=80.0, nape_to_waist=40.0, waist_to_knee=50.0)
        doc = dict(self.doc)
        doc["panels"] = [dict(doc["panels"][0], outline=[[0, 0], [25, 0], [25, 6], [25, 10], [0, 10]],
                              edges={"hem": [0, 1], "side": [1, 3], "top": [3, 4], "cf": [4, 0]})] + doc["panels"][1:]
        panels, _ = gm.grade_pattern(gm.check_pattern(doc), m)
        # chest band from v = 10, blended in from v = 2: half way at v = 6
        self.assertAlmostEqual(panels["front"].outline[2][0], 25 * (1.0 + (120 / 100 - 1.0) * 0.5))
        self.assertAlmostEqual(panels["front"].outline[1][0], 25.0)

    def test_u_on_an_edge_follows_its_slant(self):
        p = gm.Panel("leg", "t", "t", [(0, -50), (20, -50), (20, 10), (-20, 10), (-20, 0)],
                     {"hem": (0, 1), "inseam": (4, 0)}, {"region": "leg", "u0_edge": "inseam"})
        self.assertAlmostEqual(p.u_on("inseam", -50), 0.0)
        self.assertAlmostEqual(p.u_on("inseam", -25), -10.0)
        self.assertAlmostEqual(p.u_on("inseam", 5), -20.0)  # held past the crotch
        self.assertAlmostEqual(p.u_on("inseam", -60), 0.0)

    def test_bad_patterns_are_refused(self):
        clockwise = synthetic_pattern()
        clockwise["panels"][0]["outline"].reverse()
        with self.assertRaisesRegex(ValueError, "counter-clockwise"):
            gm.check_pattern(clockwise)
        no_edge = synthetic_pattern()
        no_edge["seams"].append({"a": [["front", "armhole"]], "b": [["back", "side"]]})
        with self.assertRaisesRegex(ValueError, "names no panel edge"):
            gm.check_pattern(no_edge)
        no_base = synthetic_pattern()
        del no_base["base"]["arm"]
        with self.assertRaisesRegex(ValueError, "missing in base"):
            gm.check_pattern(no_base)

    def test_fixed_bands_keep_the_traced_size(self):
        doc = synthetic_pattern()
        doc["panels"][2]["grade"] = {"u": [[-200, "fixed"]], "v": "fixed"}
        m = dict(gm.MEASURE, arm=80.0, upper_arm=40.0)
        panels, _ = gm.grade_pattern(gm.check_pattern(doc), m)
        self.assertEqual(panels["sleeve"].outline[2], (20.0, 60.0))

    def test_the_drape_pairs_every_seam(self):
        panels, seams = gm.traced(self.path)()
        for sm in seams:
            a, b = gm.seam_lengths(panels, sm)
            self.assertGreater(min(a, b), 0.0)

    def test_local_traced_patterns_load(self):
        root = os.environ.get("ACR_SOURCES")
        pats = os.path.join(root or "", "patterns")
        if not root or not os.path.isdir(pats):
            self.skipTest("ACR_SOURCES/patterns not available")
        found = 0
        for dirpath, _dirs, files in os.walk(pats):
            for fn in sorted(files):
                if fn.endswith(".json") and not fn.endswith(".trace.json"):
                    doc = gm.load_pattern(os.path.join(dirpath, fn))
                    panels, seams = gm.grade_pattern(doc)
                    for sm in seams:  # the graded seam pairs lie within their ease (+ 10 %)
                        a, b = gm.seam_lengths(panels, sm)
                        self.assertLessEqual(max(a, b) / min(a, b), 1.0 + sm.ease + 0.10, (fn, sm.a, sm.b, a, b))
                    found += 1
        if not found:
            self.skipTest("no traced patterns")
