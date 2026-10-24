"""texfill: blank (Pixal3D white) texels found and in-painted from their neighbours."""

import os
import sys
import unittest

import numpy as np

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "lib"))

from acr_assets import texfill  # noqa: E402


class Blank(unittest.TestCase):
    def test_plateau_white_only(self):
        rgb = np.full((8, 8, 3), 0.4)
        rgb[2:4, 2:4] = 1.0          # blank patch
        rgb[2, 4] = 0.9              # WebP ringing next to it
        rgb[6, 6] = (0.97, 0.96, 0.9)  # a real light texel, not blank
        hole = texfill.blank(rgb)
        self.assertTrue(hole[2:4, 2:4].all())
        self.assertTrue(hole[2, 4])
        self.assertFalse(hole[6, 6])
        self.assertEqual(int(hole.sum()), 5)


class Fill(unittest.TestCase):
    def test_hole_takes_surrounding_colour(self):
        rgb = np.zeros((16, 16, 3))
        rgb[:, :8] = (0.6, 0.3, 0.1)
        rgb[:, 8:] = (0.2, 0.4, 0.5)
        hole = np.zeros((16, 16), bool)
        hole[4:12, 2:6] = True
        rgb[hole] = 1.0
        out = texfill.fill(rgb, hole)
        self.assertTrue(np.allclose(out[~hole], rgb[~hole]))
        self.assertLess(np.abs(out[6, 3] - (0.6, 0.3, 0.1)).max(), 0.05)
        self.assertLess(out[hole].max(), 0.7)

    def test_no_hole_or_all_hole_unchanged(self):
        rgb = np.random.default_rng(0).random((5, 7, 4))
        self.assertTrue(np.array_equal(texfill.fill(rgb, np.zeros((5, 7), bool)), rgb))
        self.assertTrue(np.array_equal(texfill.fill(rgb, np.ones((5, 7), bool)), rgb))

    def test_large_hole_odd_size(self):
        rgb = np.full((33, 17, 3), 0.25)
        hole = np.ones((33, 17), bool)
        hole[0, 0] = False
        out = texfill.fill(rgb, hole)
        self.assertTrue(np.allclose(out, 0.25))


if __name__ == "__main__":
    unittest.main()
