"""Map object table (`assets/nature/map_objects.json`) against the rock models: no Blender."""

import json
import os
import unittest

NATURE = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "nature")


def indices(key):
    lo, _, hi = key.partition("-")
    return range(int(lo), int(hi or lo) + 1)


class Outcrops(unittest.TestCase):
    def setUp(self):
        with open(os.path.join(NATURE, "map_objects.json")) as f:
            self.stones = json.load(f)["STONLIST"]

    def entry(self, i):
        for k, v in self.stones.items():
            if i in indices(k):
                return v
        return None

    def test_outcrops_are_scaled_boulders(self):
        # 128-130 are 200-230 px wide rock fields: a field stone fitted by height ends at the
        # 4x clamp as a blob (#345); they take a big boulder at a table scale instead.
        for i in (128, 129, 130):
            e = self.entry(i)
            self.assertIsInstance(e, dict, i)
            self.assertEqual(e["model"], "boulder_c")
            self.assertTrue(0.5 <= e["scale"] <= 1.5, i)

    def test_ore_scatter_stays_sprite(self):
        self.assertIsNone(self.entry(131))

    def test_models_exist(self):
        for k, v in self.stones.items():
            name = v["model"] if isinstance(v, dict) else v
            self.assertTrue(os.path.exists(os.path.join(NATURE, name + ".py")), k)


if __name__ == "__main__":
    unittest.main()
