"""outfit_hd: weave relief, box UVs, coat-skirt swing (numpy only)."""

import os
import sys
import unittest

import numpy as np

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "lib"))

from acr_assets import outfit_hd  # noqa: E402


class Weave(unittest.TestCase):
    def test_tileable_and_normalised(self):
        for kind in ("twill", "plain"):
            h = outfit_hd.weave_height(128, 8, kind)
            self.assertAlmostEqual(float(h.min()), 0.0, places=6)
            self.assertAlmostEqual(float(h.max()), 1.0, places=6)
            # wraps: the jump across the tile border is no larger than inside it
            inner = np.abs(np.diff(h, axis=1)).max()
            border = np.abs(h[:, 0] - h[:, -1]).max()
            self.assertLessEqual(border, inner + 1e-6, kind)

    def test_twill_runs_diagonally(self):
        h = outfit_hd.weave_height(128, 8, "twill") - 0.5
        diag = np.mean(h * np.roll(np.roll(h, 4, 0), -4, 1))  # along the ribs
        across = np.mean(h * np.roll(np.roll(h, 4, 0), 4, 1))
        self.assertGreater(diag, across)


class BoxUV(unittest.TestCase):
    def test_dominant_axis_plane_in_tiles(self):
        u, v = outfit_hd.box_uv((0.1, 0.2, 0.3), (0.0, -1.0, 0.1), tile=0.1)
        self.assertAlmostEqual(u, 1.0)
        self.assertAlmostEqual(v, 3.0)
        u, v = outfit_hd.box_uv((0.1, 0.2, 0.3), (0.9, 0.1, 0.1), tile=0.05)
        self.assertAlmostEqual(u, 4.0)
        self.assertAlmostEqual(v, 6.0)


class SkirtSwing(unittest.TestCase):
    def test_follows_thigh_one_key_late(self):
        keys = [(0, {"thigh_l": (20, 0, 0), "thigh_r": (-20, 0, 0)}),
                (4, {"thigh_l": (-20, 0, 0), "thigh_r": (20, 0, 0)})]
        out = outfit_hd.skirt_swing(keys, gain=0.5)
        self.assertEqual(out[0][1]["skirt_fl_01"], (10.0, 0.0, 0.0))
        self.assertEqual(out[1][1]["skirt_fl_01"], (10.0, 0.0, 0.0))  # lags the swing back
        self.assertEqual(out[1][1]["skirt_fr_01"], (-10.0, 0.0, 0.0))
        self.assertLess(abs(out[1][1]["skirt_bl_01"][0]), abs(out[1][1]["skirt_fl_01"][0]))

    def test_keeps_authored_skirt_keys(self):
        keys = [(0, {"thigh_l": (30, 0, 0), "skirt_fl_01": (-80, 0, 0)})]
        self.assertEqual(outfit_hd.skirt_swing(keys)[0][1]["skirt_fl_01"], (-80, 0, 0))


if __name__ == "__main__":
    unittest.main()
