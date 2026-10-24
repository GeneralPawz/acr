"""Generic building ruins (`acr_assets/ruin.py`): the broken top and the rubble layout (pure
Python, no Blender)."""

import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "lib"))

from acr_assets import ruin  # noqa: E402


class Ruin(unittest.TestCase):
    def test_height_field_stays_in_band_and_varies(self):
        h = ruin.height_field(ruin.seed_of("engskl"), 6.0)
        vals = [h(x * 0.37, y * 0.41) for x in range(-30, 30) for y in range(-30, 30)]
        self.assertGreaterEqual(min(vals), ruin.LOW * 6.0 - 1e-9)
        self.assertLessEqual(max(vals), ruin.HIGH * 6.0 + 1e-9)
        self.assertGreater(max(vals) - min(vals), 0.3 * (ruin.HIGH - ruin.LOW) * 6.0)
        self.assertEqual(h(1.3, -2.2), ruin.height_field(ruin.seed_of("engskl"), 6.0)(1.3, -2.2))

    def test_rubble_heap_on_the_footprint(self):
        parts = ruin.rubble_layout(5, (4.0, 3.0), 6.0, 60, 12)
        self.assertEqual(sum(1 for p in parts if p[0] == "chunk"), 60)
        self.assertEqual(sum(1 for p in parts if p[0] == "beam"), 12)
        for kind, c, size, _rot in parts:
            self.assertLessEqual(abs(c[0]), 4.0 * 1.31 + 1e-6)
            self.assertLessEqual(abs(c[1]), 3.0 * 1.31 + 1e-6)
            self.assertGreaterEqual(c[2], 0.0)
            self.assertLess(c[2], 6.0)
        self.assertEqual(parts, ruin.rubble_layout(5, (4.0, 3.0), 6.0, 60, 12))


if __name__ == "__main__":
    unittest.main()
