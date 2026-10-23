"""Frontiersmen (family 11, `frontier.py`) and conquistadors (family 12, `conquista.py`):
specs, registrations and the halberd clips (pure Python, no Blender)."""

import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "lib"))

from acr_assets import clips as cl  # noqa: E402
from acr_assets import conquista, frontier, poses, registry  # noqa: E402

ALB_MD = {
    "STAND": "#STAND 9 1 5 0", "MOTION_L": "@MOTION_L 9 1 0 9", "MOTION_R": "@MOTION_R 9 1 10 19",
    "MOTION_L0": "@MOTION_L0 9 2 0 9", "ATTACK": "@ATTACK 9 3 0 47", "PATTACK": "@PATTACK 9 5 0 37",
    "UATTACK": "@UATTACK 9 5 37 0", "DEATH": "@DEATH 9 4 0 41", "DEATHLIE1": "#DEATHLIE1 9 1 4 41",
}


class Specs(unittest.TestCase):
    def test_keys_registered(self):
        for s in (frontier.spec({}), conquista.spec({})):
            for k in (s["body"], s["sleeves"], s["legs"]):
                self.assertIn(k, registry.LAYER_OF)
                self.assertIn(k, s["palette"])
            self.assertIn(s["hat"], registry.HATS)
            for g in s["kit"]:
                self.assertTrue(g in registry.GEAR or g in ("belt_l", "belt_r"), g)

    def test_own_painters(self):
        # One colour per painter: the family's cloth keys each have their own.
        keys = ("hunting", "leggings", "fsash", "fcloak", "fshirt", "fvest", "fhose", "doublet", "dsleeve", "slops")
        painters = [registry.PAINTER[k] for k in keys]
        self.assertEqual(len(set(painters)), len(painters))

    def test_unknown_key(self):
        with self.assertRaises(KeyError):
            frontier.spec({}, coat="red")


class Halberd(unittest.TestCase):
    def test_clips(self):
        c = conquista.halberd_clips(ALB_MD)
        for k in ("STAND", "MOTION_L", "MOTION_L0", "ATTACK", "ATTACK_THRUST_MID", "ATTACK_THRUST_DOWN", "ATTACK_DOWN",
                  "PATTACK", "UATTACK", "DEATH", "DEATHLIE1"):
            self.assertIn(k, c)
        hit = dict(c["ATTACK_THRUST_DOWN"])[poses.STRIKE_HIT_FRAME]
        # The stab down leans forward (positive spine X) and steps in.
        self.assertGreater(hit["spine"][0], 0)
        self.assertLess(hit["root_loc"][1], -0.2)

    def test_tip_on_target(self):
        tip = conquista.halberd_tip()
        for clip, target in conquista.TARGETS.items():
            hit = dict(conquista.halberd_clips(ALB_MD)[clip])[poses.STRIKE_HIT_FRAME]
            r = hit["world"]["socket.hand_r"]
            grip = hit["reach"]["r"][0]
            end = cl.add(grip, cl.axis(r), tip)
            for a, b in zip(end, target):
                self.assertAlmostEqual(a, b, places=3)


if __name__ == "__main__":
    unittest.main()
