"""Generated gear fitting (`genfit`): the pure parts (no Blender)."""

import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "lib"))

from acr_assets import decay, genfit  # noqa: E402


class Tables(unittest.TestCase):
    def test_items(self):
        for n, it in genfit.ITEMS.items():
            self.assertIn(it["category"], genfit.CATEGORIES, n)
            self.assertIn(it["layer"], decay.LAYERS, n)
            self.assertTrue(0.05 < it["size"] < 5.0, n)
            if "grip" in it:
                self.assertTrue(0.0 < it["grip"] < 1.0, n)

    def test_budgets(self):
        for c, b in genfit.CATEGORIES.items():
            self.assertLessEqual(b["tris"], 6000, c)
            self.assertIn(b["tex"], (512, 1024), c)

    def test_item_lookup(self):
        self.assertEqual(genfit.item("pixal3d/eq_tricorn_officer.glb")["category"], "headgear")
        with self.assertRaises(KeyError):
            genfit.item("pixal3d/unknown.glb")


class Sources(unittest.TestCase):
    def test_paths(self):
        with tempfile.TemporaryDirectory() as d:
            src = os.path.join(d, "extracted")
            gen = os.path.join(d, "generated", "pixal3d")
            os.makedirs(src)
            os.makedirs(gen)
            open(os.path.join(gen, "eq_cutlass.glb"), "w").close()
            # beside ACR_SOURCES (local layout)
            self.assertEqual(genfit.source_path("pixal3d/eq_cutlass.glb", {"ACR_SOURCES": src}),
                             os.path.join(d, "generated", "pixal3d/eq_cutlass.glb"))
            # below ACR_GENERATED
            self.assertEqual(genfit.source_path("eq_cutlass.glb", {"ACR_GENERATED": gen}),
                             os.path.join(gen, "eq_cutlass.glb"))
            with self.assertRaises(FileNotFoundError):
                genfit.source_path("pixal3d/none.glb", {"ACR_SOURCES": src})


class Numbers(unittest.TestCase):
    def test_summary(self):
        s = genfit.summary([i / 1000 for i in range(1, 101)])
        self.assertAlmostEqual(s["mean_m"], 0.0505, places=4)
        self.assertEqual(s["p99_m"], 0.099)
        self.assertEqual(s["max_m"], 0.1)
        self.assertIsNone(genfit.summary([])["max_m"])

    def test_islands(self):
        self.assertEqual(genfit.islands(6, [(0, 1), (1, 2), (3, 4)]), 2)
        self.assertEqual(genfit.islands(3, [(0, 1), (1, 2), (2, 0)]), 1)


class Orient(unittest.TestCase):
    def test_held_long_axis_up(self):
        # lying along X, wide along Z: long X → Z, wide Z → Y
        self.assertEqual(genfit.orient_plan((0.9, 0.02, 0.1), "held"), (1, 2, 0))

    def test_worn_thin_along_y(self):
        self.assertEqual(genfit.orient_plan((0.05, 0.4, 0.5), "worn")[1], 0)
        self.assertEqual(genfit.orient_plan((0.4, 0.05, 0.5), "worn"), (0, 1, 2))

    def test_headgear_unchanged(self):
        self.assertEqual(genfit.orient_plan((0.4, 0.3, 0.1), "headgear"), (0, 1, 2))


class Grip(unittest.TestCase):
    def sword(self, flip=False):
        # pommel, grip, guard at 20 %, then a long blade (40 bins)
        w = [0.04, 0.03, 0.03, 0.03, 0.03, 0.03, 0.03, 0.12] + [0.035] * 32
        return w[::-1] if flip else w

    def test_sword_grip_between_guard_and_pommel(self):
        end, f = genfit.grip_fraction(self.sword(), 0.9)
        self.assertEqual(end, 0)
        self.assertTrue(0.05 < f < 0.15, f)
        end, f2 = genfit.grip_fraction(self.sword(flip=True), 0.9)
        self.assertEqual(end, 1)
        self.assertAlmostEqual(f, f2)

    def test_club_grip_at_the_butt(self):
        w = [0.03] * 36 + [0.10, 0.12, 0.12, 0.10]
        end, f = genfit.grip_fraction(w, 0.65)
        self.assertEqual(end, 0)
        self.assertAlmostEqual(f * 0.65, min(genfit.GRIP_IN, 0.15 * 0.65))
        end, _f = genfit.grip_fraction(w[::-1], 0.65)
        self.assertEqual(end, 1)

    def test_kinds(self):
        # a cutlass: wide knuckle-bow hilt in the first fifth, long blade
        w = [0.05, 0.09, 0.10, 0.12, 0.12, 0.12, 0.13, 0.14] + [0.05] * 32
        self.assertEqual(genfit.grip_fraction(w, 0.8, {"kind": "guard"})[0], 0)
        # a tomahawk whose head is the widest quarter
        t = [0.04] * 30 + [0.12] * 10
        self.assertEqual(genfit.grip_fraction(t[::-1], 0.5, {"kind": "head"})[0], 1)

    def test_fixed_and_peak(self):
        w = [0.03] * 36 + [0.10, 0.12, 0.12, 0.10]
        self.assertEqual(genfit.grip_fraction(w, 2.2, {"grip": 0.4}), (0, 0.4))
        pistol = [0.10, 0.12, 0.08] + [0.03] * 17
        end, f = genfit.grip_fraction(pistol, 0.4, {"grip_at": "peak"})
        self.assertEqual(end, 0)
        self.assertLess(f, 0.1)


if __name__ == "__main__":
    unittest.main()
