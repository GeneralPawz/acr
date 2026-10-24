"""European civilians (family 13, `acr_assets/civil.py`): tools, clips and drafts (pure
Python, no Blender)."""

import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "lib"))

from acr_assets import civil  # noqa: E402
from acr_assets import clips as cl  # noqa: E402
from acr_assets import garments as gm  # noqa: E402
from acr_assets import registry  # noqa: E402

PRIEST_MD = {
    "STAND": "#STAND 9 1 2 0", "REST": "@REST 9 2 0 79", "MOTION_L": "@MOTION_L 9 1 0 9",
    "MOTION_R": "@MOTION_R 9 1 10 19", "MOTION_LB": "@MOTION_LB 9 1 9 0", "MOTION_RB": "@MOTION_RB 9 1 19 10",
    "ATTACK": "@ATTACK 9 3 0 199", "DEATH": "@DEATH 9 4 0 66", "DEATHLIE1": "#DEATHLIE1 9 1 4 66",
}


class Tools(unittest.TestCase):
    def test_budgets_and_faces(self):
        self.assertEqual(set(civil.ITEMS), set(civil.BUDGET))
        for n, f in civil.ITEMS.items():
            it = f()
            self.assertGreater(it.triangles(), 0, n)
            self.assertLessEqual(it.triangles(), civil.BUDGET[n], n)
            self.assertEqual(len(it.faces), len(it.parts), n)
            for k in it.parts:
                self.assertIn(it.keys[k], registry.LAYER_OF, (n, k))

    def test_hand_tools_in_gear_budget(self):
        # Hand tools are seen close up in the work clips: 0.5-1.5k triangles each (#205).
        for n in civil.HAND_TOOLS:
            self.assertTrue(500 <= civil.ITEMS[n]().triangles() <= 1500, n)
        self.assertIn("dagger", civil.ITEMS)

    def test_tips(self):
        for n in civil.HAND_TOOLS:
            tip = civil.ITEMS[n]().sockets["socket.tip"]
            self.assertAlmostEqual(sum(c * c for c in tip) ** 0.5, civil.TIP[n], delta=0.05)

    def test_skeleton_has_tool_sockets(self):
        names = {b.name for b in civil.skeleton()}
        for s in civil.TOOL_SOCKETS:
            self.assertIn(s, names)


class PeasantClips(unittest.TestCase):
    def setUp(self):
        self.md = civil.peasant_md(52, 45)
        self.clips = civil.peasant_clips(self.md)

    def test_every_md_clip(self):
        for k in self.md:
            self.assertIn(k, self.clips)
            n = cl.md_frames(self.md[k])
            self.assertLessEqual(self.clips[k][-1][0], n - 1, k)

    def test_impacts_on_sound_frames(self):
        self.assertEqual(self.clips["ATTACK1"].event, 15)
        self.assertEqual(self.clips["WORK"].event, 5)
        frames = [f for f, _p in self.clips["ATTACK1"]]
        self.assertIn(15, frames)

    def test_axe_edge_on_the_tree(self):
        f, pose = next(k for k in self.clips["ATTACK1"] if k[0] == 15)
        grip = pose["reach"]["r"][0]
        r = pose["world"]["socket.hand_r"]
        tip = cl.add(grip, cl.axis(r), civil.AXE_TIP)
        for a, b in zip(tip, civil.TREE):
            self.assertAlmostEqual(a, b, delta=0.01)

    def test_tools_shown_per_clip(self):
        def shown(c):
            return {s.split(".", 1)[1] for s, v in c[0][1]["show"].items() if v}

        self.assertEqual(shown(self.clips["ATTACK1"]), {"axe"})
        self.assertEqual(shown(self.clips["ATTACK3"]), {"sickle"})
        self.assertEqual(shown(self.clips["WORK"]), {"hammer"})
        self.assertEqual(shown(self.clips["MOTION_L2"]), {"sack"})
        self.assertEqual(shown(self.clips["MOTION_L4"]), {"bundle"})
        self.assertEqual(shown(self.clips["MOTION_L"]), set())
        for k, c in self.clips.items():
            self.assertEqual(set(c[0][1]["show"]), set(civil.TOOL_SOCKETS), k)


class PriestClips(unittest.TestCase):
    def test_heal(self):
        c = civil.priest_clips(PRIEST_MD)
        self.assertEqual(set(c), set(PRIEST_MD))
        self.assertEqual(c["ATTACK"].event, 2)
        self.assertEqual(c["ATTACK"][-1][0], 199)


class Drafts(unittest.TestCase):
    def test_work_coat(self):
        for skirts in (False, True):
            panels, seams = civil.peasant_coat("tunic", "shirt", skirts)(gm.MEASURE)
            for d in civil.DROP_ALWAYS:
                self.assertNotIn(d, panels)
            self.assertEqual("skirt_front" in panels, skirts)
            for s in seams:
                for x in s.a + s.b:
                    self.assertIn(x[0], panels)
            self.assertEqual(panels["sleeve_top"].key, "shirt")
            self.assertEqual(panels["coat_front"].key, "tunic")

    def test_stockings(self):
        panels, _seams = civil.stockings(gm.MEASURE)
        self.assertEqual(set(panels), {"gaiter"})
        self.assertEqual(panels["gaiter"].key, "hose")


if __name__ == "__main__":
    unittest.main()
