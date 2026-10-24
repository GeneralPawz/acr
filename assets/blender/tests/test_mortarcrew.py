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


def gc_stand_spine():
    return guncrew.STAND["spine"][0]


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


    def test_carried_by_its_handles(self):
        """Moving: every man holds a handle on every frame, the bed 10-20 cm off the ground,
        lifted in CHEST_CARRY and set down in DEPLOY."""
        c = mortarcrew.clips()
        for p in mortarcrew.PREFIXES:
            self.assertEqual(guncrew.ik_spans(c["GUN_PUSH"][p]), [(0, 36, mortarcrew.CARRY_IK[p])], p)
            self.assertEqual(guncrew.ik_spans(c["DEPLOY"][p])[0][:2], (0, mortarcrew.SET_DOWN[1]), p)
            self.assertEqual(guncrew.ik_spans(c["CHEST_CARRY"][p])[-1][1:], (41, mortarcrew.CARRY_IK[p]), p)
        lift = mortarcrew.gun_lift()
        self.assertTrue(0.10 <= mortarcrew.LIFT <= 0.20)
        self.assertEqual(lift["GUN_PUSH"][0][1], mortarcrew.LIFT)
        self.assertEqual((lift["CHEST_CARRY"][0][1], lift["CHEST_CARRY"][-1][1]), (0.0, mortarcrew.LIFT))
        self.assertEqual((lift["DEPLOY"][0][1], lift["DEPLOY"][-1][1]), (mortarcrew.LIFT, 0.0))

    def test_no_backward_lean_while_carrying(self):
        """The trunk leans forward under the load (positive spine X), never back."""
        c = mortarcrew.clips()
        for clip in ("GUN_PUSH", "CHEST_CARRY", "DEPLOY"):
            for p, ks in c[clip].items():
                for f, s in ks:
                    if s.get("ik"):
                        self.assertGreater(s.get("spine", (0, 0, 0))[0], 0, (clip, p, f))
                    self.assertGreaterEqual(s.get("spine", (0, 0, 0))[0], gc_stand_spine(), (clip, p, f))


class Grips(unittest.TestCase):
    def test_every_hand_on_its_grip(self):
        st = _stats()
        if st is None or "grips" not in st.get("hands", {}):
            self.skipTest("no artmrt build (cargo xtask assets artmrt)")
        grips = st["hands"]["grips"]
        for what in ("linstock", "muzzle_r", "handle_l", "handle_r", "rear_l"):
            self.assertIn(what, grips)
            self.assertLess(grips[what]["mean_m"], GRIP_MEAN, (what, grips[what]))
            self.assertLess(grips[what]["max_m"], GRIP_MAX, (what, grips[what]))
        self.assertGreater(st["elevation_deg"]["built"], 55)


if __name__ == "__main__":
    unittest.main()
