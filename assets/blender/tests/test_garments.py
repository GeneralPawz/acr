"""Sewing patterns of the draped uniform (pure Python, no Blender; docs/clothing.md)."""

import os
import sys
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
                         {"br_front", "br_back", "fall", "waistband", "knee_band"})

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

    def test_drafted_to_measure(self):
        big = dict(gm.MEASURE, chest=gm.MEASURE["chest"] + 12.0)
        a = gm.coat()[0]["coat_front"].bounds()[2]
        b = gm.coat(big)[0]["coat_front"].bounds()[2]
        self.assertAlmostEqual(b - a, 3.0, places=6)


if __name__ == "__main__":
    unittest.main()
