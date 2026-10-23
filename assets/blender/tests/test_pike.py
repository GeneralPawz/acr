"""Family 6, pike and shot (`acr_assets.pike`; the module needs Blender): the unit scripts'
data, read without importing them (pure Python)."""

import ast
import os
import sys
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "lib"))

from acr_assets import rig  # noqa: E402

UNITS = os.path.join(HERE, "..", "..", "units")
FAMILY = ("EngPik", "FrnPik", "EngMus", "SpnMus", "FrnS17", "EngB17", "FrnB17", "SpnB17", "EngF17", "FrnF17", "SpnF17")


def constants(ty):
    tree = ast.parse(open(os.path.join(UNITS, ty.lower() + ".py"), encoding="utf-8").read())
    out = {}
    for node in tree.body:
        if isinstance(node, ast.Assign) and isinstance(node.targets[0], ast.Name):
            try:
                out[node.targets[0].id] = ast.literal_eval(node.value)
            except ValueError:
                pass
    return out


class PikeScripts(unittest.TestCase):
    def test_every_unit_has_a_script_with_its_md(self):
        for ty in FAMILY:
            c = constants(ty)
            self.assertEqual(c["TYPE"], ty)
            md = c["MD"]
            for key in ("STAND", "MOTION_L", "MOTION_R", "DEATH", "DEATHLIE1"):
                self.assertIn(key, md, f"{ty} {key}")
            for key, line in md.items():
                n, _ = rig.md_clip_frames(line)
                self.assertGreaterEqual(n, 1, f"{ty} {key}")
                self.assertIn(key, line)

    def test_fighters_have_their_attacks(self):
        for ty in ("EngPik", "FrnPik"):
            self.assertIn("PSTAND", constants(ty)["MD"])
        for ty in ("EngMus", "SpnMus", "FrnS17"):
            md = constants(ty)["MD"]
            for key in ("ATTACK", "ATTACK1", "PATTACK1"):
                self.assertIn(key, md, f"{ty} {key}")


if __name__ == "__main__":
    unittest.main()
