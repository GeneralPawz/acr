"""acr_assets.breaks: the felled tree's break, lying pose and ground contact (no bpy)."""

import math
import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "lib"))

from acr_assets import breaks  # noqa: E402


class Breaks(unittest.TestCase):
    def test_profile_is_seeded_ragged_and_spiked(self):
        a = breaks.profile(16, 3, 0.1)
        self.assertEqual(a, breaks.profile(16, 3, 0.1))
        self.assertNotEqual(a, breaks.profile(16, 4, 0.1))
        self.assertTrue(all(h >= 0 for h in a))
        self.assertGreaterEqual(sum(h >= 0.2 for h in a), 3)
        self.assertGreater(max(a) - min(a), 0.15)

    def test_at_angle_interpolates_and_wraps(self):
        h = [0.0, 1.0, 0.0, 1.0]
        self.assertAlmostEqual(breaks.at_angle(h, 0.0), 0.0)
        self.assertAlmostEqual(breaks.at_angle(h, math.pi / 4), 0.5)
        self.assertAlmostEqual(breaks.at_angle(h, 2 * math.pi + math.pi / 2), 1.0)

    def test_lay_tips_the_top_east_and_near_the_ground(self):
        x, y, z = breaks.lay((0.0, 0.0, 7.0))
        self.assertGreater(x, 6.9)
        self.assertLess(abs(z), 0.7)
        self.assertEqual(y, 0.0)
        # The renderer's FALL_ANGLE (map_models.rs) in degrees.
        self.assertAlmostEqual(breaks.FELL_DEG, 84.8, places=1)

    def test_ground_keeps_everything_on_or_just_under_the_floor(self):
        self.assertEqual(breaks.ground(1.0), 1.0)
        self.assertAlmostEqual(breaks.ground(breaks.FLOOR), breaks.FLOOR)
        self.assertGreaterEqual(breaks.ground(-3.0), breaks.FLOOR - 0.1)
        self.assertLess(breaks.ground(-0.1), breaks.FLOOR)


if __name__ == "__main__":
    unittest.main()
