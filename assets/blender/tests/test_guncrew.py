"""The gun crew's drill (`acr_assets.guncrew`, pure Python, no Blender)."""

import json
import math
import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "lib"))

from acr_assets import guncrew  # noqa: E402


class Drill(unittest.TestCase):
    def test_clips_cover_the_gun_frames(self):
        c = guncrew.clips()
        self.assertEqual(set(c), {"STAND", "GUN_PUSH", "CHEST_CARRY", "TRAIL_LIFT", "TRAIL_WALK", "DEPLOY", "ATTACK",
                                  "DEATH", "DEATHLIE1"})
        for clip, n in (("ATTACK", 419), ("GUN_PUSH", 36), ("CHEST_CARRY", 41), ("TRAIL_LIFT", 70),
                        ("TRAIL_WALK", 40), ("DEPLOY", 170), ("DEATH", 72)):
            for p, ks in c[clip].items():
                frames = [f for f, _ in ks]
                self.assertEqual(frames, sorted(frames), (clip, p))
                self.assertEqual(frames[-1], n, (clip, p))

    def test_linstock_on_the_vent_at_the_shot(self):
        ks = dict(guncrew.attack_keys()["c3."])
        self.assertIn(guncrew.FIRE, ks)
        self.assertEqual(ks[guncrew.FIRE]["world"]["socket.hand_r"][0], guncrew.VENT[0])

    def test_attack_loops_to_the_stations(self):
        for p, ks in guncrew.attack_keys().items():
            first, last = ks[0][1]["root_loc"], ks[-1][1]["root_loc"]
            self.assertAlmostEqual(math.dist(first[:2], last[:2]), 0.0, places=6, msg=p)

    def test_place_turns_offsets(self):
        d = guncrew.place({"root_loc": (0, -1, 0)}, (2, 3), 90)
        self.assertAlmostEqual(d["root_loc"][0], 3.0)
        self.assertAlmostEqual(d["root_loc"][1], 3.0)
        self.assertEqual(d["root"], (0, 0, 90))

    def test_moves_between_stations_take_steps(self):
        ks = guncrew.attack_keys()["c0."]
        walk = [f for f, s in ks if 246 < f < 290]
        self.assertGreaterEqual(len(walk), 3)
        thighs = {ks[[f for f, _ in ks].index(f)][1]["thigh_l"] for f in walk}
        self.assertGreater(len(thighs), 1)

    def test_hands_are_put_on_the_tools(self):
        ks = dict(guncrew.attack_keys()["c3."])
        self.assertEqual(ks[guncrew.FIRE].get("ik"), "linstock")
        kinds = {s.get("ik") for f, s in guncrew.attack_keys()["c0."]}
        self.assertTrue({"staff", "wheel"} <= kinds)
        self.assertIn("spike", {s.get("ik") for f, s in guncrew.attack_keys()["c2."]})
        self.assertEqual({s.get("ik") for f, s in guncrew.death_keys()[0]["c1."]}, {None})

    def test_a_turn_is_one_continuous_action(self):
        """TRAIL_LIFT starts at the stations and ends where TRAIL_WALK starts, which ends where
        it starts (`#PSTAND1` between the steps holds that pose): nobody pops back."""
        c = guncrew.clips()
        for p in guncrew.PREFIXES:
            self.assertEqual(c["TRAIL_LIFT"][p][0][1], c["STAND"][p][0][1], p)
            self.assertEqual(c["TRAIL_LIFT"][p][-1][1], c["TRAIL_WALK"][p][0][1], p)
            self.assertEqual(c["TRAIL_WALK"][p][-1][1], c["TRAIL_WALK"][p][0][1], p)
            self.assertEqual(c["CHEST_CARRY"][p][0][1], c["STAND"][p][0][1], p)
            self.assertEqual(c["CHEST_CARRY"][p][-1][1], c["GUN_PUSH"][p][0][1], p)
        self.assertIn("spike", {s.get("ik") for _, s in c["TRAIL_WALK"]["c2."]})
        for p in ("c0.", "c1."):
            self.assertEqual({s.get("ik") for _, s in c["TRAIL_WALK"][p]}, {"wheel"}, p)

    def test_the_firer_steps_through_the_turn(self):
        """c3 walks on the spot through TRAIL_WALK (one stride per cycle), facing along the
        ground that runs past his station while the gun turns."""
        c3 = guncrew.clips()["TRAIL_WALK"]["c3."]
        self.assertGreater(len({s["thigh_l"] for _, s in c3}), 1)
        x, y = c3[0][1]["root_loc"][:2]
        self.assertAlmostEqual(c3[0][1]["root"][2], guncrew.turn_heading(x, y))
        # The ground runs past him clockwise: the facing is the clockwise tangent.
        yaw = math.radians(guncrew.turn_heading(1.0, 0.0))
        self.assertAlmostEqual(math.sin(yaw), 0.0)
        self.assertAlmostEqual(-math.cos(yaw), -1.0)
        self.assertAlmostEqual(guncrew.turn_slip(x, y), math.hypot(x, y) * math.radians(22.5))

    def test_carriers_stand_clear_of_the_chest(self):
        """The carriers' hips stand clear of the chest's ends and rope loops (0.37 m out)."""
        self.assertGreater(guncrew.CHEST_SPAN - 0.37, 0.2)

    def test_two_push_and_two_carry_the_chest(self):
        c = guncrew.clips()["GUN_PUSH"]
        self.assertEqual({s.get("ik") for _, s in c["c0."]} | {s.get("ik") for _, s in c["c1."]}, {"wheel"})
        xs = []
        for p in ("c2.", "c3."):
            self.assertEqual({s.get("ik") for _, s in c[p]}, {"chest"}, p)
            xs.append(c[p][0][1]["root_loc"][0])
        self.assertLess(xs[0], 0.0)
        self.assertGreater(xs[1], 0.0)


    def test_deploy_starts_and_ends_at_the_stations(self):
        """DEPLOY (`#PATTACK4/5`, backwards `#UATTACK4/5`) leaves and returns to STAND, so
        neither the deploy nor the limbering pops; the trail man works the handspike, the
        wheel men heave, the firer stoops to the chest."""
        c = guncrew.clips()
        for p in guncrew.PREFIXES:
            self.assertEqual(c["DEPLOY"][p][0][1], c["STAND"][p][0][1], p)
            self.assertEqual(c["DEPLOY"][p][-1][1], c["STAND"][p][0][1], p)
        kinds = {p: {k for _a, _b, k in guncrew.ik_spans(ks)} for p, ks in c["DEPLOY"].items()}
        self.assertEqual(kinds, {"c0.": {"wheel"}, "c1.": {"wheel"}, "c2.": {"spike"}, "c3.": {"chest"}})

    def test_grip_spans_cover_the_work_not_the_steps(self):
        spans = guncrew.ik_spans(guncrew.attack_keys()["c0."])
        self.assertIn("wheel", {k for _a, _b, k in spans})
        for a, b, k in spans:
            if k == "wheel":
                # The run back into battery: hands on the wheel from 300 to 400, not while
                # he walks back to his station.
                self.assertEqual((a, b), (300, 400))
        self.assertEqual(guncrew.ik_spans(guncrew.motion_keys()["c2."]), [(0, 36, "chest")])


GRIP_MEAN = 0.01  # m: a fist's centre from its grip, mean over every frame of every work
GRIP_MAX = 0.10  # m: the worst frame (the stoop to the chest's lift, the first heave of DEPLOY)
WORK = ("chest_l", "chest_r", "wheel_l", "wheel_r", "spike_l", "spike_seat", "linstock")


def _stats():
    """The last build's `artpus.stats.json` (`cargo xtask assets artpus`), or None."""
    here = os.path.dirname(os.path.abspath(__file__))
    path = os.environ.get("ACR_ARTPUS_STATS") or os.path.join(here, "..", "..", "..", "target", "assets", "artillery",
                                                               "artpus.stats.json")
    if not os.path.exists(path):
        return None
    with open(path, encoding="utf-8") as f:
        return json.load(f)


class Grips(unittest.TestCase):
    """Hands on the grips, as measured in Blender per frame by the build (`guncrew.measure`):
    the carriers' fists on the chest's rope loops, the wheel men's on the felloes, the
    trail man's on the handspike (its head seated under the trail's ring), the left hand on
    the rammer staff, the match on the vent."""

    def test_every_hand_on_its_grip(self):
        st = _stats()
        if st is None or "grips" not in st.get("hands", {}):
            self.skipTest("no artpus build with per-frame grip stats (cargo xtask assets artpus)")
        grips = st["hands"]["grips"]
        for what in WORK:
            self.assertIn(what, grips)
            self.assertLess(grips[what]["mean_m"], GRIP_MEAN, (what, grips[what]))
            self.assertLess(grips[what]["max_m"], GRIP_MAX, (what, grips[what]))
        # The handspike (the turn's and the laying's lever) is held exactly.
        for what in ("spike_l", "spike_seat"):
            self.assertLess(grips[what]["max_m"], 0.01, (what, grips[what]))
        # Its seat is on the trail's top (0.25 m over the ground, the trail bone's local y the
        # height over the axle at 0.72), not the carriage's top 0.7 m over it (m2-cannon-crew).
        self.assertLess(st["hands"]["marks_rest"]["intact"][1], -0.3, st["hands"]["marks_rest"])


if __name__ == "__main__":
    unittest.main()
