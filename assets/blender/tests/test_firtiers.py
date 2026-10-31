"""Fir tiers (`acr_assets/firtiers.py`): the layout is deterministic, tiers narrow towards the
top and leave gaps between them (pure Python, no Blender)."""

import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "lib"))

from acr_assets import firtiers  # noqa: E402


class FirTiers(unittest.TestCase):
    def test_deterministic(self):
        self.assertEqual(firtiers.branches(), firtiers.branches())

    def test_tiers_narrow_and_rise(self):
        ys = [sy for sy, _r in firtiers.TIERS]
        rs = [r for _sy, r in firtiers.TIERS]
        self.assertEqual(ys, sorted(ys, reverse=True))
        self.assertEqual(rs, sorted(rs, reverse=True))
        self.assertGreater(len(firtiers.TIERS), 6)
        self.assertLess(firtiers.TIERS[-1][0], firtiers.TOP + 12)

    def test_every_tier_is_a_ring_of_branches(self):
        by_tier = {}
        for t, *_ in firtiers.branches():
            by_tier[t] = by_tier.get(t, 0) + 1
        self.assertEqual(sorted(by_tier), list(range(len(firtiers.TIERS))))
        self.assertTrue(all(n >= 4 for n in by_tier.values()))

    def test_branch_reach_stays_near_the_sprite_width(self):
        for t, _a, reach, _sy, droop in firtiers.branches():
            base = firtiers.TIERS[t][1]
            self.assertTrue(0.85 * base <= reach <= 1.1 * base)
            self.assertTrue(0.25 <= droop <= 0.5)

    def test_silhouette_has_gaps_above_the_lowest_tier(self):
        self.assertEqual(firtiers.silhouette_halfwidth(-8), 0.0)
        self.assertGreater(firtiers.silhouette_halfwidth(-28), 25)
        self.assertEqual(firtiers.silhouette_halfwidth(-155), 0.0)


if __name__ == "__main__":
    unittest.main()
