"""Strike clips (docs/melee.md, Strike clips; pure Python, no Blender):
`python -m unittest discover -s assets/blender/tests`."""

import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "lib"))

from acr_assets import poses, rig  # noqa: E402

MD = {"STAND": "#STAND 9 1 5 0", "ATTACK": "@ATTACK 9 6 0 70", "ATTACK1": "@ATTACK1 9 3 0 86"}


class StrikeClips(unittest.TestCase):
    def test_names_frames_and_hit(self):
        clips = poses.musketeer_clips(MD)
        names = [k for k in clips if k.startswith("ATTACK_")]
        self.assertEqual(sorted(names), sorted(["ATTACK_THRUST_MID", "ATTACK_THRUST_DOWN", "ATTACK_DOWN",
                                                "ATTACK_SMASH_MID", "ATTACK_SHOT_DOWN"]))
        n = rig.md_clip_frames(MD["ATTACK"])[0]
        for name in names:
            keys = clips[name]
            frames = [f for f, _ in keys]
            # Same frames as ATTACK, the blow on the sim's strike frame.
            self.assertEqual(frames[0], 0, name)
            self.assertEqual(frames[-1], n - 1, name)
            self.assertIn(poses.STRIKE_HIT_FRAME, frames, name)

    def test_thrust_down_points_down_and_steps_in(self):
        keys = dict(poses.musketeer_clips(MD)["ATTACK_THRUST_DOWN"])
        hit = keys[poses.STRIKE_HIT_FRAME]
        # The muzzle well below level (90°), the man stepped in and down.
        self.assertGreater(hit["world"]["socket.hand_r"][0], 120)
        self.assertLess(hit["root_loc"][1], -0.2)
        self.assertLess(hit["root_loc"][2], -0.15)

    def test_without_attack_line_no_strikes(self):
        self.assertFalse(any(k.startswith("ATTACK_") for k in poses.musketeer_clips({"STAND": MD["STAND"]})))


if __name__ == "__main__":
    unittest.main()
