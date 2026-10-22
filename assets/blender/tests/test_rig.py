"""Rig tables, clip timing and decay parameters (pure Python, no Blender):
`python -m unittest discover -s assets/blender/tests`."""

import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "lib"))

from acr_assets import decay, rig  # noqa: E402


class SkeletonH(unittest.TestCase):
    def test_counts_and_names(self):
        b = rig.skeleton_h()
        names = [x.name for x in b]
        self.assertEqual(len(b), 75)
        self.assertEqual(sum(x.deform for x in b), 60)
        self.assertEqual(len(set(names)), len(names))
        for x in b:
            self.assertTrue(x.parent is None or x.parent in names, x)
            # Sockets never deform.
            if x.name.startswith("socket."):
                self.assertFalse(x.deform, x.name)

    def test_prefix_and_offset(self):
        b = rig.skeleton_h("rider.", (0, 0, 1))
        hips = next(x for x in b if x.name == "rider.hips")
        self.assertEqual(hips.parent, "rider.root")
        self.assertAlmostEqual(hips.head[2], 1.95)

    def test_parts(self):
        self.assertEqual(rig.part_of_bone("forearm_l"), 3)
        self.assertEqual(rig.part_of_bone("index_02_l"), 3)
        self.assertEqual(rig.part_of_bone("skirt_fl_01"), 0)
        names = {x.name for x in rig.skeleton_h()}
        for _j, parent, child, bone in rig.JOINTS:
            self.assertIn(bone, names)
            self.assertEqual(rig.part_of_bone(bone), child)
            self.assertNotEqual(parent, child)


class RigR(unittest.TestCase):
    def test_tree(self):
        b = rig.rig_r()
        names = [x.name for x in b]
        self.assertEqual(len(set(names)), len(names))
        for x in b:
            self.assertTrue(x.parent is None or x.parent in names, x)
        self.assertIn("socket.saddle", names)
        # Hooves reach the ground.
        for x in b:
            if x.name.startswith(("front_hoof", "back_hoof")):
                self.assertAlmostEqual(x.tail[2], 0.0)


class Clips(unittest.TestCase):
    def test_md_lines(self):
        self.assertEqual(rig.md_clip_frames("@DEATH 9 4 0 70"), (71, False))
        self.assertEqual(rig.md_clip_frames("@UATTACK 9 5 35 0"), (36, True))
        self.assertEqual(rig.md_clip_frames("#STAND 9 1 5 0"), (1, False))

    def test_keys_on_sprite_frames(self):
        keys = rig.clip_keys(71, [(0.0, "a"), (0.5, "b"), (1.0, "c")])
        self.assertEqual([f for f, _ in keys], [0, 35, 70])
        # Keys that round to the same frame collapse into the later one.
        self.assertEqual(rig.clip_keys(1, [(0.0, "a"), (1.0, "b")]), [(0, "b")])


class Decay(unittest.TestCase):
    def test_counter_mapping(self):
        self.assertEqual(decay.decay_from_counter(100), 0.0)
        self.assertEqual(decay.decay_from_counter(1200), 1.0)
        self.assertAlmostEqual(decay.decay_from_counter(1100, remove=2100), 0.5)
        self.assertEqual(decay.deflate_weight(0.1), 0.0)
        self.assertAlmostEqual(decay.sink_offset(1.0), -decay.SINK)

    def test_layer_order(self):
        # Flesh goes first, bones last; wood and metal never dissolve.
        s = {k: v["s"] for k, v in decay.LAYERS.items()}
        self.assertLess(s["flesh"][0], s["linen"][0])
        self.assertLess(s["linen"][0], s["wool"][0])
        self.assertLess(s["wool"][0], s["leather"][0])
        self.assertLess(s["leather"][0], s["bone"][0])
        self.assertIsNone(s["wood"])
        self.assertEqual(sorted(v["code"] for v in decay.LAYERS.values()), list(range(8)))


if __name__ == "__main__":
    unittest.main()
