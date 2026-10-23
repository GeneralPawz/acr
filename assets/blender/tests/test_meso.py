"""Mesoamerica family (`acr_assets.meso`): unit clips, items and headgear as data (no Blender)."""

import importlib.util
import os
import sys
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "lib"))

from acr_assets import clips, held, meso, registry  # noqa: E402

UNITS = os.path.join(HERE, "..", "..", "units")
NAMES = ("mayvoi", "atcmet", "atclut", "maylut", "maypik", "atcvog", "mayvog", "atcsha", "maysha")


def unit(name):
    spec = importlib.util.spec_from_file_location(name, os.path.join(UNITS, f"{name}.py"))
    m = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(m)
    return m


class MesoTest(unittest.TestCase):
    def test_every_md_clip_is_keyed(self):
        for name in NAMES:
            m = unit(name)
            made, extra = meso.unit_clips(m.SPEC)
            self.assertEqual([k for k in m.MD if k not in made], [], name)
            self.assertIn("ATTACK", made, name)
            for k in extra:
                self.assertTrue(k.startswith("ATTACK_"), k)

    def test_release_frames_follow_setactivepoint(self):
        made, _ = meso.unit_clips(unit("atcmet").SPEC)
        self.assertEqual(made["ATTACK1"].event, clips.release_frame(28))
        made, _ = meso.unit_clips(unit("atclut").SPEC)
        self.assertEqual(made["ATTACK1"].event, clips.release_frame(44))
        self.assertEqual(made["ATTACK2"].event, clips.release_frame(44))

    def test_strikes_hit_on_frame_2_with_the_shield_up(self):
        made, _ = meso.unit_clips(unit("mayvoi").SPEC)
        self.assertEqual(made["ATTACK_CUT_MID"].event, clips.STRIKE_HIT_FRAME)
        for _f, pose in made["ATTACK_CUT_MID"]:
            self.assertIn("l", pose["reach"])  # the chimalli held before the body

    def test_items_and_headgear(self):
        for kind in ("jaguar", "eagle", "crest", "maya", "topknot", "band"):
            it = meso.helmet(kind)
            keys, _ = it.material_keys()
            self.assertEqual(len(keys), 1, kind)  # one key: painted in one atlas cell
            self.assertIn(keys[0], registry.LAYER_OF)
            self.assertLess(it.triangles(), 600, kind)
        sh = meso.chimalli()
        self.assertEqual(sh.material_keys()[0], ["chimalli"])
        self.assertIn("socket.hook", meso.atlatl().sockets)
        self.assertGreater(meso.dart().sockets["socket.tip"][2], 0.9)
        self.assertAlmostEqual(meso.tips({"hand_r": "macuahuitl"}), held.macuahuitl().sockets["socket.tip"][2])

    def test_registered(self):
        for key in ("ichcahuipilli", "maxtlatl", "tilmatli", "jaguar", "eagle", "plumes", "sandals", "chimalli"):
            self.assertIn(key, registry.LAYER_OF)
        for mask in ("chimalli_halves", "chimalli_rings", "chimalli_quarters"):
            self.assertIn(mask, registry.MASKS)


if __name__ == "__main__":
    unittest.main()
