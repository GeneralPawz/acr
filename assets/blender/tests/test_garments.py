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

    def test_drafted_to_measure(self):
        big = dict(gm.MEASURE, chest=gm.MEASURE["chest"] + 12.0)
        a = gm.coat()[0]["coat_front"].bounds()[2]
        b = gm.coat(big)[0]["coat_front"].bounds()[2]
        self.assertAlmostEqual(b - a, 3.0, places=6)


if __name__ == "__main__":
    unittest.main()


def _inside(pt, poly):
    x, y = pt
    c = False
    for (x0, y0), (x1, y1) in zip(poly, poly[1:] + poly[:1]):
        if (y0 > y) != (y1 > y) and x < x0 + (y - y0) * (x1 - x0) / (y1 - y0):
            c = not c
    return c


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
