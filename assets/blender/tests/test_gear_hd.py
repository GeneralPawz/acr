"""gear_hd's pure data: the musket's dimensions and grip geometry, the LOD targets."""

import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "lib"))

from acr_assets import gear_hd as g  # noqa: E402


class Musket(unittest.TestCase):
    def test_dimensions(self):
        self.assertAlmostEqual(g.MUZZLE_Z - g.BUTT_Z, 1.47, places=3)
        self.assertAlmostEqual(g.BARREL_LEN, 1.0668, places=3)
        self.assertAlmostEqual(2 * g.BORE_R, 0.01905, places=4)
        self.assertLess(g.BORE_R, g.BARREL_R[1])

    def test_stations_ordered(self):
        zs = [s[0] for s in g.STOCK]
        self.assertEqual(zs, sorted(zs))
        for z, top, bot, hw, p in g.STOCK:
            self.assertGreater(top, bot)
            self.assertGreater(hw, 0.01)

    def test_grip(self):
        gr = g.grip_geometry()
        w, f = gr["wrist"], gr["fore_end"]
        # A hand closes round 30-45 mm (held.GRIPS["musket"]: 36 and 40 mm).
        self.assertTrue(0.030 < w["diameter"] < 0.042, w)
        self.assertTrue(0.030 < f["diameter"] < 0.048, f)
        self.assertAlmostEqual(w["line"][0][0], 0.0)
        self.assertLess(abs(w["line"][0][1]), 0.01)  # origin on the wrist's centre line
        self.assertAlmostEqual(gr["muzzle"][2], g.MUZZLE_Z, places=3)
        self.assertGreater(f["height"], f["width"])  # stock plus barrel

    def test_ramrod_under_barrel(self):
        self.assertLess(g.ROD_Y + g.ROD_R, g.BARREL_Y - g.BARREL_R[1])

    def test_lod_targets(self):
        self.assertEqual(g.lod_targets(30000), [9000, 3000, 900])
        self.assertEqual(g.lod_targets(1000)[-1], g.MIN_LOD_TRIS)


if __name__ == "__main__":
    unittest.main()
