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
        self.assertEqual(keys["MOTION_L5"][-1][1], run[2])
        self.assertEqual(len(keys["MOTION_L"]), 8)  # a key a frame
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
        self.assertGreater(poses.gait_measure(poses.RUN)["stride_m"], poses.gait_measure(poses.WALK)["stride_m"])
        knee = lambda g: max(poses.gait_pose(poses.STAND_MUSKET, g, i / 48)["shin_r"][0] for i in range(48))  # noqa: E731
        self.assertGreater(knee(poses.RUN), knee(poses.WALK))  # the heel kicked up

    def test_strides_match_the_sim(self):
        # docs/body-sim.md "Cadence and stride": walk 1.67 m/s over 0.75 s a stride wants
        # 1.3 m; the run (3.33 m/s) a flight phase and a longer stride than the walk.
        walk, run = poses.gait_measure(poses.WALK), poses.gait_measure(poses.RUN)
        self.assertAlmostEqual(walk["stride_m"], 1.3, delta=0.05)
        self.assertEqual(walk["flight"], 0.0)
        self.assertGreater(run["stride_m"], 1.7)
        self.assertGreater(run["flight"], 0.15)
        for m in (walk, run):
            self.assertGreater(m["clear_m"], 0.015)  # the swing foot clears the ground
            self.assertGreater(m["swing_pitch_deg"], 0.0)  # toes up in mid-swing

    def test_halves_meet(self):
        # MOTION_L ends where MOTION_R starts and R ends where L starts (one loop).
        for gait in ("walk", "run"):
            l = poses.cycle_half(poses.STAND_MUSKET, gait, "MOTION_L", 10)
            r = poses.cycle_half(poses.STAND_MUSKET, gait, "MOTION_R", 10)
            for a, b in ((l[-1][1], r[0][1]), (r[-1][1], l[0][1])):
                for bone in ("thigh_l", "shin_r", "foot_l", "root_loc"):
                    for x, y in zip(a[bone], b[bone]):
                        self.assertAlmostEqual(x, y, places=2)

    def test_joint_ranges(self):
        # Within `limits.py` (armature X: the thigh 30 deg back, the knee 0-140, the ankle).
        for g in (poses.WALK, poses.RUN):
            for i in range(48):
                p = poses.gait_pose(poses.STAND_MUSKET, g, i / 48)
                for sfx in ("_l", "_r"):
                    self.assertLessEqual(p["thigh" + sfx][0], 30.0)
                    self.assertGreaterEqual(p["shin" + sfx][0], 0.0)
                    self.assertLessEqual(p["shin" + sfx][0], 140.0)


if __name__ == "__main__":
    unittest.main()
