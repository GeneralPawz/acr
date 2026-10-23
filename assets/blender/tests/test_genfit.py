"""genfit: the sprite fit of generated models recovers a known turn, scale and shift."""

import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "lib"))

try:
    import numpy as np

    from acr_assets import genfit
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
            tx, ty = genfit.ground_shift(12.0, -7.0, d)
            uv = genfit.project(p + [tx, ty, 0.0], d)
            self.assertAlmostEqual(uv[0, 0], 12.0, places=6)
            self.assertAlmostEqual(uv[0, 1], -7.0, places=6)

    def test_recovers_pose(self):
        pts = _house()
        # The "sprite": the house turned 40 degrees, 1.5 times larger, shifted on the ground.
        truth = (pts @ genfit.rot(40.0).T) * 1.5 + [0.8, -0.5, 0.0]
        anchor = (250.0, 450.0)
        mask = genfit.raster(genfit.project(truth, 64.0) + anchor, (600, 500))
        # The input: the house turned by -70 and shrunk, as a generator might deliver it.
        src = (pts @ genfit.rot(-70.0).T) * 0.3
        src = src - (src.min(axis=0) + src.max(axis=0)) / 2
        f = genfit.fit(src, mask, anchor, 64.0)
        self.assertGreater(f["iou"], 0.9)
        self.assertAlmostEqual((f["yaw"] - 110.0) % 360.0, 0.0, delta=10.0)
        self.assertAlmostEqual(f["scale"], 5.0, delta=0.5)


if __name__ == "__main__":
    unittest.main()
