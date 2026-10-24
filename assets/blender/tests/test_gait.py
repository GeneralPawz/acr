"""Run or walk per `MOTION_*` line (#295): `poses.motion_gait`, `poses.step_keys` (no Blender)."""

import ast
import os
import sys
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "lib"))

from acr_assets import poses  # noqa: E402

UNITS = os.path.join(HERE, "..", "..", "units")


def md(name):
    """The unit script's `MD` dict, read without importing it (its family needs Blender)."""
    with open(os.path.join(UNITS, f"{name}.py"), encoding="utf-8") as f:
        tree = ast.parse(f.read())
    for node in tree.body:
        if isinstance(node, ast.Assign) and any(getattr(t, "id", "") == "MD" for t in node.targets):
            return ast.literal_eval(node.value)
    raise KeyError(name)


class Gait(unittest.TestCase):
    def test_native_foot_runs_european_line_walks(self):
        self.assertEqual(poses.motion_gait("@MOTION_L 9 0 0 7"), "run")
        self.assertEqual(poses.motion_gait("@MOTION_L 9 0 0 8"), "run")
        self.assertEqual(poses.motion_gait("@MOTION_L 9 1 0 9"), "walk")
        self.assertEqual(poses.motion_gait("@MOTION_RB4 9 1 9 0"), "walk")

    def test_step_keys_pick_per_line(self):
        keys = poses.step_keys(md("siukri"), poses.STAND_MUSKET)
        run, walk = poses.run_cycle(poses.STAND_MUSKET), poses.walk_cycle(poses.STAND_MUSKET)
        self.assertEqual(keys["MOTION_L"][0][1], run[0])
        self.assertEqual(keys["MOTION_L5"][1][1], run[1])
        # Stance 4 (`RATE 4 8`, 10 frames a step) walks at the European pace.
        self.assertEqual(keys["MOTION_L4"][0][1], walk[0])
        self.assertEqual(set(keys), {k for k in md("siukri") if k.startswith("MOTION_")})

    def test_every_native_family_runs(self):
        for name in ("irolut", "siulut", "puelut", "atclut", "inclut", "maypik", "incpik", "pues15"):
            for k, line in md(name).items():
                # Stance 4 (`RATE 4 8`) walks at half speed; every other state runs.
                if k.rstrip("0123456789") == "MOTION_L" and not k.endswith("4"):
                    self.assertEqual(poses.motion_gait(line), "run", (name, k))
        self.assertEqual(poses.motion_gait(md("engm18")["MOTION_L"]), "walk")

    def test_run_reaches_further_than_walk(self):
        run, walk = poses.run_cycle(poses.STAND_MUSKET), poses.walk_cycle(poses.STAND_MUSKET)
        spread = lambda p: p["thigh_r"][0] - p["thigh_l"][0]  # noqa: E731
        self.assertGreater(spread(run[0]), spread(walk[0]))
        self.assertGreater(run[1]["shin_r"][0], walk[1]["shin_r"][0])


if __name__ == "__main__":
    unittest.main()
