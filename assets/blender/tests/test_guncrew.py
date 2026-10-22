"""The gun crew's drill (`acr_assets.guncrew`, pure Python, no Blender)."""

import math
import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "lib"))

from acr_assets import guncrew  # noqa: E402


class Drill(unittest.TestCase):
    def test_clips_cover_the_gun_frames(self):
        c = guncrew.clips()
        self.assertEqual(set(c), {"STAND", "MOTION_L", "MOTION_R", "ATTACK", "DEATH", "DEATHLIE1"})
        for clip, n in (("ATTACK", 419), ("MOTION_L", 36), ("DEATH", 72)):
            for p, ks in c[clip].items():
                frames = [f for f, _ in ks]
                self.assertEqual(frames, sorted(frames), (clip, p))
                self.assertEqual(frames[-1], n, (clip, p))

    def test_linstock_on_the_vent_at_the_shot(self):
        ks = dict(guncrew.attack_keys()["c3."])
        self.assertIn(guncrew.FIRE, ks)
        self.assertEqual(ks[guncrew.FIRE]["world"]["socket.hand_r"][0], guncrew.VENT[0])

    def test_attack_loops_to_the_stations(self):
        for p, ks in guncrew.attack_keys().items():
            first, last = ks[0][1]["root_loc"], ks[-1][1]["root_loc"]
            self.assertAlmostEqual(math.dist(first[:2], last[:2]), 0.0, places=6, msg=p)

    def test_place_turns_offsets(self):
        d = guncrew.place({"root_loc": (0, -1, 0)}, (2, 3), 90)
        self.assertAlmostEqual(d["root_loc"][0], 3.0)
        self.assertAlmostEqual(d["root_loc"][1], 3.0)
        self.assertEqual(d["root"], (0, 0, 90))


if __name__ == "__main__":
    unittest.main()
