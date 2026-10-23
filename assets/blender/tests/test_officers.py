"""Family 4, officers on foot (`acr_assets.officers`; the module needs Blender): the unit
scripts' data, read without importing them (pure Python)."""

import ast
import os
import sys
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "lib"))

from acr_assets import poses, rig  # noqa: E402

UNITS = os.path.join(HERE, "..", "..", "units")
OFFICERS = ("EngOf17", "EngOf18", "FrnOf17", "FrnOf18", "SpnOf17", "SpnOf18", "UsaOf18")


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


class OfficerScripts(unittest.TestCase):
    def test_every_officer_has_a_script_with_its_md(self):
        for ty in OFFICERS:
            c = constants(ty)
            self.assertEqual(c["TYPE"], ty)
            md = c["MD"]
            for key in ("STAND", "REST", "MOTION_L", "MOTION_R", "ATTACK", "DEATH", "DEATHLIE1"):
                self.assertIn(key, md, f"{ty} {key}")
                n, _ = rig.md_clip_frames(md[key])
                self.assertGreaterEqual(n, 1)
            for key, line in md.items():
                self.assertEqual(line.split()[0][1:], key, ty)

    def test_attack_holds_the_blow(self):
        # The blow lands on frame 2 and is held two frames: ATTACK must be longer.
        for ty in OFFICERS:
            n, _ = rig.md_clip_frames(constants(ty)["MD"]["ATTACK"])
            self.assertGreater(n, poses.STRIKE_HIT_FRAME + 4, ty)

    def test_scripts_use_the_family(self):
        for ty in OFFICERS:
            src = open(os.path.join(UNITS, ty.lower() + ".py"), encoding="utf-8").read()
            self.assertIn("officers.spec(MD", src)
            self.assertIn("officers.build(ctx, spec())", src)


if __name__ == "__main__":
    unittest.main()
