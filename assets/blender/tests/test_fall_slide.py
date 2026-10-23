"""The dead horse's slide to the sprites' corpse spot (`cavalry.centre_fall`; pure Python,
no Blender): `python -m unittest discover -s assets/blender/tests`."""

import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "lib"))

from acr_assets import rig  # noqa: E402


class FallSlide(unittest.TestCase):
    def test_done_while_the_horse_goes_down(self):
        self.assertEqual(rig.fall_slide(0.0), 0.0)
        self.assertAlmostEqual(rig.fall_slide(rig.FALL_SLIDE_END), 1.0)
        # Once down (from 0.3 of the clip) the body no longer moves: no slide on the ground.
        for i in range(31, 101):
            self.assertEqual(rig.fall_slide(i / 100), 1.0)

    def test_eased_and_monotonic(self):
        vals = [rig.fall_slide(i / 100) for i in range(101)]
        self.assertTrue(all(b >= a for a, b in zip(vals, vals[1:])))
        self.assertLess(vals[3], 0.1)


if __name__ == "__main__":
    unittest.main()
