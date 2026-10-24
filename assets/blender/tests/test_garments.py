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


class DrapePilot(Period17):
    """Shell garments drafted and draped instead (#287 drape pilot): shirts and tunics,
    cloaks, the sash."""

    NAMES = ("frn_blouse", "frn_sash", "frn_cape", "trade_shirt", "unku", "yacolla", "tilmatli")

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
