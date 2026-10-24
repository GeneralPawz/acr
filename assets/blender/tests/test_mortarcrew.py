"""The mortar crew's drill (`acr_assets.mortarcrew`, pure Python, no Blender) and its hands
as measured by the build (`target/assets/artillery/artmrt.stats.json`)."""

import json
import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "lib"))

from acr_assets import guncrew, mortarcrew  # noqa: E402

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "..")
GRIP_MEAN, GRIP_MAX = 0.01, 0.10


def _stats():
    p = os.environ.get("ACR_ARTMRT_STATS") or os.path.join(ROOT, "target", "assets", "artillery", "artmrt.stats.json")
    if not os.path.exists(p):
        return None
    with open(p, encoding="utf-8") as f:
        return json.load(f)


class Drill(unittest.TestCase):
    def test_three_men_in_every_clip_of_the_gun(self):
        c = mortarcrew.clips()
        self.assertEqual(set(c), set(guncrew.clips()))
        for clip, n in (("ATTACK", 419), ("GUN_PUSH", 36), ("CHEST_CARRY", 41), ("TRAIL_LIFT", 70),
                        ("TRAIL_WALK", 40), ("DEPLOY", 170), ("DEATH", 72)):
            self.assertEqual(set(c[clip]), set(mortarcrew.PREFIXES), clip)
            for p, ks in c[clip].items():
                frames = [f for f, _ in ks]
                self.assertEqual(frames, sorted(frames), (clip, p))
                self.assertEqual(frames[-1], n, (clip, p))

    def test_two_load_one_fires(self):
        ks = mortarcrew.attack_keys()
        fire = dict(ks["c2."])
        self.assertIn(mortarcrew.FIRE, fire)
        self.assertEqual(fire[mortarcrew.FIRE].get("ik"), "linstock")
        for p in ("c0.", "c1."):
            self.assertTrue(any(k == "muzzle" for _f, _f1, k in guncrew.ik_spans(ks[p])), p)
        self.assertEqual(mortarcrew.KITS["c2."], "linstock")


class Grips(unittest.TestCase):
    def test_every_hand_on_its_grip(self):
        st = _stats()
        if st is None or "grips" not in st.get("hands", {}):
            self.skipTest("no artmrt build (cargo xtask assets artmrt)")
        grips = st["hands"]["grips"]
        for what in ("linstock", "muzzle_r"):
            self.assertIn(what, grips)
            self.assertLess(grips[what]["mean_m"], GRIP_MEAN, (what, grips[what]))
            self.assertLess(grips[what]["max_m"], GRIP_MAX, (what, grips[what]))
        self.assertGreater(st["elevation_deg"]["built"], 55)


if __name__ == "__main__":
    unittest.main()
