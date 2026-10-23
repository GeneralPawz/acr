"""Finishing blows (docs/melee.md, `finish.py`): every melee builder adds them, the blow
lands at frame 2 on the man 0.9 m ahead, and the clip ends standing (no lunge held)."""

import math
import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "lib"))

from acr_assets import clips as cl  # noqa: E402
from acr_assets import finish  # noqa: E402


def point(pose, tip):
    """The weapon's point of a pose: its right-hand goal plus the point (`grip_for`'s inverse)."""
    grip = pose["reach"]["r"][0]
    r = pose["world"]["socket.hand_r"]
    if isinstance(tip, (tuple, list)):
        return cl.add(grip, cl.rot(r, tuple(tip)))
    return cl.add(grip, cl.axis(r), tip)


class FinishTest(unittest.TestCase):
    def test_builders_add_finishing_clips(self):
        for name, build, kinds in (("sword", cl.sword_strikes, ("THRUST", "CUT")),
                                   ("club", cl.club_strikes, ("CUT", "SMASH")),
                                   ("spear", cl.spear_strikes, ("THRUST",))):
            out = build(30)
            self.assertIn("ATTACK_FINISH", out, name)
            for k in kinds:
                self.assertIn(f"ATTACK_{k}_FINISH", out, name)
            self.assertIn("ATTACK_FINISH", cl.MELEE[name](30), name)

    def test_blow_lands_on_the_man_at_frame_2(self):
        for kind, tip in (("sword", cl.SWORD_TIP), ("club", cl.MELEE_TIP["club"]),
                          ("tomahawk", cl.MELEE_TIP["tomahawk"]), ("spear", cl.SPEAR_TIP),
                          ("knife", finish.KNIFE_TIP)):
            clip = finish.finish_clips(30, kind, tip)["ATTACK_FINISH"]
            self.assertEqual(clip.event, 2)
            hit = dict(clip)[2]
            p = point(hit, tip)
            self.assertLess(math.dist(p, finish.TARGET_FINISH), 0.002, kind)
            # Over him: the hand no further out than the man, no lunge.
            self.assertGreater(hit["reach"]["r"][0][1], -0.85, kind)

    def test_ends_standing(self):
        clip = finish.finish_clips(30, "sword", cl.SWORD_TIP)["ATTACK_FINISH"]
        frames = dict(clip)
        self.assertIs(frames[29], frames[9])  # back to the stand by 30 % and kept
        self.assertNotIn("root_loc", frames[29])


if __name__ == "__main__":
    unittest.main()
