"""Inca family (`acr_assets.inca`): unit clips, items and headgear as data (no Blender)."""

import importlib.util
import os
import sys
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "lib"))

from acr_assets import clips, inca, registry  # noqa: E402

UNITS = os.path.join(HERE, "..", "..", "units")
NAMES = ("incpra", "incpik", "inctrb", "incvog", "inclut", "incsha")


def unit(name):
    spec = importlib.util.spec_from_file_location(name, os.path.join(UNITS, f"{name}.py"))
    m = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(m)
    return m


class IncaTest(unittest.TestCase):
    def test_every_md_clip_is_keyed(self):
        for name in NAMES:
            m = unit(name)
            made, extra = inca.unit_clips(m.SPEC)
            self.assertEqual([k for k in m.MD if k not in made], [], name)
            self.assertIn("ATTACK", made, name)
            self.assertIn("ATTACK_DOWN", made, name)
            for k in extra:
                self.assertTrue(k.startswith("ATTACK_"), k)

    def test_release_frames_follow_setactivepoint(self):
        made, _ = inca.unit_clips(unit("incpra").SPEC)
        self.assertEqual(made["ATTACK1"].event, clips.release_frame(56))
        made, _ = inca.unit_clips(unit("inctrb").SPEC)
        self.assertEqual(made["ATTACK1"].event, clips.release_frame(17))

    def test_down_strikes_lean_forward(self):
        for name in ("incpra", "incpik"):
            made, _ = inca.unit_clips(unit(name).SPEC)
            hit = dict(made["ATTACK_DOWN"])[clips.STRIKE_HIT_FRAME]
            self.assertGreater(hit["spine"][0], 0, name)  # positive X bends forward on skeleton H

    def test_items(self):
        for it in (inca.band(), inca.helmet(), inca.plumes(7, 0.25), inca.back_shield()):
            keys, _ = it.material_keys()
            self.assertEqual(len(keys), 1, it.name)  # one key: painted in one atlas cell
            self.assertIn(keys[0], registry.LAYER_OF)
            self.assertLess(it.triangles(), 600, it.name)
        self.assertAlmostEqual(inca.champi().sockets["socket.tip"][2], inca.CHAMPI)

    def test_registered(self):
        for key in ("unku_check", "unku_plain", "unku_tocapu", "unku_royal", "yacolla", "llautu", "inca_plumes",
                    "inca_quilt", "inca_helmet", "inca_shield", "inca_hair", "spool"):
            self.assertIn(key, registry.LAYER_OF)
            self.assertIn(registry.PAINTER[key], registry.PAINT_FN.keys() | registry.KIND.keys())

    def test_painters(self):
        try:
            import numpy as np

            from acr_assets import coat  # noqa: F401
        except ImportError:
            self.skipTest("numpy (Blender's Python) not here")

        p = np.random.default_rng(1).uniform((-0.2, -0.2, 0.5), (0.2, 0.2, 1.5), (500, 3)).astype(np.float32)
        n = np.tile(np.array([[0, -1, 0]], np.float32), (500, 1))
        for name in ("unku_check", "unku_plain", "unku_tocapu", "unku_royal", "llautu", "inca_plumes", "inca_helmet"):
            out = registry.PAINT_FN[name](p, n, (0.7, 0.1, 0.1), {"head_top": 1.75}, 3)
            self.assertEqual(out.shape, (500, 3), name)
            self.assertTrue(np.isfinite(out).all(), name)


if __name__ == "__main__":
    unittest.main()
