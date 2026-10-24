"""kit_hd / gear_hd: belt weights without the arms, the solid straps, the tricorn's cocked
brim (no bpy)."""

import math
import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "lib"))

from acr_assets import gear_hd, kit_hd  # noqa: E402


class TrunkWeights(unittest.TestCase):
    def test_arm_bones_removed_and_normalised(self):
        w = kit_hd.trunk_weights({"upperarm_l": 0.5, "spine2": 0.3, "clavicle_l": 0.2})
        self.assertEqual(set(w), {"spine2", "clavicle_l"})
        self.assertAlmostEqual(sum(w.values()), 1.0)

    def test_all_arm_falls_back_to_trunk(self):
        self.assertEqual(kit_hd.trunk_weights({"forearm_r": 1.0, "index_01_r": 0.2}), {"spine1": 1.0})

    def test_at_most_four(self):
        w = kit_hd.trunk_weights({b: 0.2 for b in ("hips", "spine", "spine1", "spine2", "neck")})
        self.assertEqual(len(w), 4)


class Straps(unittest.TestCase):
    def test_resample_even_steps_keeps_ends(self):
        pts, src = kit_hd.resample([(0, 0, 0), (0.1, 0, 0), (0.1, 0.3, 0)], 0.01)
        self.assertEqual(len(pts), 41)
        self.assertEqual(pts[0], (0, 0, 0))
        self.assertAlmostEqual(pts[-1][1], 0.3)
        gaps = [math.dist(pts[i], pts[i + 1]) for i in range(len(pts) - 1)]
        self.assertTrue(all(abs(g - 0.01) < 0.002 for g in gaps))
        self.assertEqual((src[0], src[-1]), (0, 1))

    def test_solid_strap_is_closed(self):
        for rows, across in ((2, 2), (7, 5)):
            faces = kit_hd.strap_faces(rows, across)
            edges = {}
            for f in faces:
                for i in range(len(f)):
                    e = (f[i], f[(i + 1) % len(f)])
                    edges[e] = edges.get(e, 0) + 1
            for (a, b), c in edges.items():  # every edge once each way: closed, wound alike
                self.assertEqual(c, 1)
                self.assertIn((b, a), edges)
            self.assertEqual(max(max(f) for f in faces), rows * 2 * across - 1)

    def test_pits_bridged_bumps_kept(self):
        nb = [[1], [0, 2], [1]]
        self.assertEqual(kit_hd.fill_pits([0.0, -0.05, 0.0], nb, drop=0.004, passes=1)[1], -0.004)
        self.assertEqual(kit_hd.fill_pits([0.0, 0.02, 0.0], nb, drop=0.004, passes=1)[1], 0.02)


class Tricorn(unittest.TestCase):
    def test_three_points_reach_out_low_sides_cock_up(self):
        rx, ry = 0.09, 0.105
        for k in gear_hd.TRICORN_CORNERS:
            _e, _d, reach, rise = gear_hd.tricorn_shape(math.radians(k), rx, ry)
            self.assertAlmostEqual(reach, 0.075, places=3)
            self.assertAlmostEqual(rise, 0.030, places=3)
        for k in (-30.0, 90.0, 210.0):  # the middle of each cock
            _e, d, reach, rise = gear_hd.tricorn_shape(math.radians(k), rx, ry)
            self.assertLess(reach, 0.035)
            self.assertGreater(rise, 0.08)
            self.assertAlmostEqual(math.hypot(*d), 1.0)

    def test_front_point_faces_minus_y(self):
        (ex, ey), (dx, dy), _w, _h = gear_hd.tricorn_shape(math.radians(-90.0), 0.09, 0.105)
        self.assertAlmostEqual(ey, -0.105)
        self.assertAlmostEqual(dy, -1.0)


if __name__ == "__main__":
    unittest.main()
