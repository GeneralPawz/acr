"""cavalry_kit: the saddle kit laid down slice by slice, the strap's hip end (no bpy)."""

import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "lib"))

from acr_assets import cavalry_kit  # noqa: E402


class Sag(unittest.TestCase):
    def test_each_slice_rests_clear_above_its_surface(self):
        cols = [[(1.0, 0.5), (1.2, 0.5)], [(1.0, 0.6), (1.1, 0.62)]]
        self.assertEqual([round(x, 3) for x in cavalry_kit.sag_shifts(cols, 0.005)], [-0.495, -0.395])

    def test_slice_without_surface_takes_neighbours(self):
        cols = [[(1.0, 0.5)], [(1.0, None)], [(1.0, 0.7)]]
        self.assertAlmostEqual(cavalry_kit.sag_shifts(cols, 0.0)[1], -0.4)


    def test_bend_capped_below_the_highest_support(self):
        cols = [[(1.0, 0.5)], [(1.0, 0.6)], [(1.0, 0.5)]]
        self.assertEqual([round(x, 3) for x in cavalry_kit.sag_shifts(cols, 0.0, 0.02)], [-0.42, -0.4, -0.42])


class Kit(unittest.TestCase):
    def test_strap_end_is_lowest_on_its_side(self):
        pts = [(0.1, 0, 1.4), (-0.17, 0, 0.95), (-0.15, 0.1, 0.97), (0.2, 0, 0.9)]
        self.assertEqual(cavalry_kit.strap_end(pts, -1), (-0.17, 0, 0.95))

    def test_kits_known(self):
        for kit in cavalry_kit.KITS.values():
            self.assertEqual(cavalry_kit.check(kit), kit)
        with self.assertRaises(KeyError):
            cavalry_kit.check(("lance_bucket",))


if __name__ == "__main__":
    unittest.main()
