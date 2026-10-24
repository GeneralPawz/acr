"""spritefit: the sprite fit of generated models recovers a known turn, scale and shift."""

import math
import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "lib"))

try:
    import numpy as np

    from acr_assets import spritefit
except ImportError:  # numpy missing
    np = None


def _house(n=200000, seed=0):
    """Points on an L-shaped block with a tall tower at one end (asymmetric in plan)."""
    rng = np.random.default_rng(seed)
    a = rng.random((n, 3)) * [6.0, 3.0, 3.0] - [3.0, 1.5, 0.0]
    b = rng.random((n // 2, 3)) * [2.0, 3.0, 7.0] + [1.0, 1.5, 0.0]
    return np.concatenate([a, b])


@unittest.skipIf(np is None, "numpy missing")
class Fit(unittest.TestCase):
    def test_ground_shift_round_trip(self):
        for d in (64.0, 104.0):
            p = np.array([[0.0, 0.0, 0.0]])
            tx, ty = spritefit.ground_shift(12.0, -7.0, d)
            uv = spritefit.project(p + [tx, ty, 0.0], d)
            self.assertAlmostEqual(uv[0, 0], 12.0, places=6)
            self.assertAlmostEqual(uv[0, 1], -7.0, places=6)

    def test_recovers_pose(self):
        pts = _house()
        # The "sprite": the house turned 40 degrees, 1.5 times larger, shifted on the ground.
        truth = (pts @ spritefit.rot(40.0).T) * 1.5 + [0.8, -0.5, 0.0]
        anchor = (250.0, 450.0)
        mask = spritefit.raster(spritefit.project(truth, 64.0) + anchor, (600, 500))
        # The input: the house turned by -70 and shrunk, as a generator might deliver it.
        src = (pts @ spritefit.rot(-70.0).T) * 0.3
        src = src - (src.min(axis=0) + src.max(axis=0)) / 2
        f = spritefit.fit(src, mask, anchor, 64.0)
        self.assertGreater(f["iou"], 0.9)
        self.assertAlmostEqual((f["yaw"] - 110.0) % 360.0, 0.0, delta=10.0)
        self.assertAlmostEqual(f["scale"], 5.0, delta=0.5)


@unittest.skipIf(np is None, "numpy missing")
class Upright(unittest.TestCase):
    def test_camera_upright_stands_the_view_vertical(self):
        # in the camera frame of the 30-degree view the world's up is (0, -sin e, cos e)
        up = np.array([0.0, -0.5, math.sqrt(3) / 2])
        np.testing.assert_allclose(spritefit.camera_upright() @ up, [0.0, 0.0, 1.0], atol=1e-9)

    def test_level_flattens_a_tilted_base(self):
        rng = np.random.default_rng(3)
        slab = rng.random((20000, 3)) * [4.0, 2.0, 0.4] - [2.0, 1.0, 0.0]
        stump = rng.random((8000, 3)) * [1.0, 1.0, 2.0] + [0.5, -0.5, 0.4]
        pts = np.concatenate([slab, stump]) @ spritefit.rot(0.0, 6.0, -4.0).T
        r, how, tilt = spritefit.level(pts)
        self.assertEqual(how, "base")
        self.assertAlmostEqual(tilt, math.hypot(6.0, 4.0), delta=1.5)
        out = (pts - pts.mean(0)) @ r.T
        self.assertGreater(spritefit.base_support(out), 0.08 * len(slab))  # the slab's bottom band

    def test_level_stands_a_pillar_on_its_axis(self):
        rng = np.random.default_rng(4)
        pts = rng.random((20000, 3)) * [0.6, 0.6, 4.0] @ spritefit.rot(30.0, 9.0).T
        r, how, tilt = spritefit.level(pts)
        self.assertEqual(how, "axis")
        self.assertAlmostEqual(tilt, 9.0, delta=1.0)
        out = pts @ r.T
        self.assertGreater(np.ptp(out[:, 2]), 3.95)


if __name__ == "__main__":
    unittest.main()
