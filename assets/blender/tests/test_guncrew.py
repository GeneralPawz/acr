"""The gun crew's drill (`acr_assets.guncrew`, pure Python, no Blender)."""

import json
import math
import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "lib"))

from acr_assets import guncrew, poses  # noqa: E402


SLIDE_MAX = 0.08  # m: a foot planted through the turn moves less than this over the ground (four keys a cycle: 0.15; built, bezier: 0.24)


def _at(keys, f):
    """The leg angles, root and yaw of keys [(frame, spec)] at frame `f`, linear between keys."""
    (f0, a), (f1, b) = next(((p, q) for p, q in zip(keys, keys[1:]) if p[0] <= f <= q[0]))
    t = (f - f0) / (f1 - f0)
    mix = lambda u, v: tuple(x + (y - x) * t for x, y in zip(u, v))
    out = {k: mix(a[k], b[k]) for k in ("thigh_l", "thigh_r", "shin_l", "shin_r", "root_loc", "root")}
    return out


def foot_slide(keys, side):
    """Metres a foot (`"_l"`/`"_r"`) of a man keyed through the gun's turn moves over the ground
    while planted (within 3 cm of its lowest height in two keys running), summed over the keys:
    the ankle from the leg angles, set in the gun's frame at his station, which turns
    clockwise by `guncrew.TURN_STEP` over the clip."""
    n = keys[-1][0]
    pts = []
    for f in range(n + 1):
        spec = _at(keys, f)
        a = math.radians(-spec["thigh" + side][0])
        b = math.radians(spec["shin" + side][0]) + poses.LEG_SHIN_BACK
        fwd = poses.LEG_THIGH * math.sin(a) + poses.LEG_SHIN * math.sin(a - b)
        up = (poses.LEG_HIP + spec["root_loc"][2] - poses.LEG_THIGH * math.cos(a)
              - poses.LEG_SHIN * math.cos(a - b))
        yaw = math.radians(spec["root"][2])
        x0, y0 = spec["root_loc"][:2]
        # The man looks along (sin yaw, -cos yaw).
        gx, gy = x0 + fwd * math.sin(yaw), y0 - fwd * math.cos(yaw)
        th = -math.radians(guncrew.TURN_STEP) * f / n
        pts.append((gx * math.cos(th) - gy * math.sin(th), gx * math.sin(th) + gy * math.cos(th), up))
    low = min(p[2] for p in pts)
    return sum(math.hypot(b[0] - a[0], b[1] - a[1]) for a, b in zip(pts, pts[1:])
               if a[2] < low + 0.03 and b[2] < low + 0.03)


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

    def test_the_firer_does_not_slide_through_the_turn(self):
        """Foot slide of c3 in TRAIL_WALK (#341), as `measure` of the studio sums it: the gun
        turns clockwise by TURN_STEP over the clip, and a foot within 3 cm of its lowest
        height in two keys running moves over the ground (fixed in the world) by its slide."""
        keys = guncrew.clips()["TRAIL_WALK"]["c3."]
        for side in ("_l", "_r"):
            self.assertLess(foot_slide(keys, side), SLIDE_MAX, side)

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
        # The carriers' fists are on the rope loops within a few cm (#342).
        for what in ("chest_l", "chest_r"):
            self.assertLess(grips[what]["max_m"], 0.03, (what, grips[what]))
        # The handspike (the turn's and the laying's lever) is held exactly.
        for what in ("spike_l", "spike_seat"):
            self.assertLess(grips[what]["max_m"], 0.01, (what, grips[what]))
        # Its seat is on the trail's top (0.25 m over the ground, the trail bone's local y the
        # height over the axle at 0.72), not the carriage's top 0.7 m over it (m2-cannon-crew).
        self.assertLess(st["hands"]["marks_rest"]["intact"][1], -0.3, st["hands"]["marks_rest"])


def _poses():
    """The last build's `artpus.poses.json` (bone heads and turns per clip frame), or None."""
    here = os.path.dirname(os.path.abspath(__file__))
    path = os.environ.get("ACR_ARTPUS_POSES") or os.path.join(here, "..", "..", "..", "target", "assets", "artillery",
                                                               "artpus.poses.json")
    if not os.path.exists(path):
        return None
    with open(path, encoding="utf-8") as f:
        return json.load(f)["clips"]


# A man as capsules round his bones (m): the coat's breadth is the torso's.
BODY = (((("hips", "spine"), ("spine", "spine1"), ("spine1", "spine2"), ("spine2", "neck")), 0.17),
        ((("neck", "head"),), 0.11),
        ((("upperarm_l", "forearm_l"), ("upperarm_r", "forearm_r")), 0.06),
        ((("thigh_l", "shin_l"), ("thigh_r", "shin_r"), ("shin_l", "foot_l"), ("shin_r", "foot_r")), 0.08))
CHEST_BOX = ((-0.30, -0.20, 0.0), (0.30, 0.20, 0.40))  # the chest about its bottom's centre (the lid included)
GAP_MEN = 0.10  # m: between the two carriers' bodies
GAP_CHEST = 0.04  # m: between a carrier's body and the chest
GAP_TOOL = 0.15  # m: the tool's axis from the chest
HANDLE_NEAR = 0.15  # m: the wrist from the rope loop it holds (the fist is round it)


def _capsules(rig, man):
    pt = lambda b: rig[man + b][:3]
    return [(pt(a), pt(b), r) for pairs, r in BODY for a, b in pairs]


def _along(a, b, n=9):
    return [tuple(x + (y - x) * i / (n - 1) for x, y in zip(a, b)) for i in range(n)]


def _to_box(p, chest):
    out = [max(lo - (v - c), (v - c) - hi, 0.0) for v, c, lo, hi in zip(p, chest, CHEST_BOX[0], CHEST_BOX[1])]
    return math.sqrt(sum(o * o for o in out))


def _to_chest(rig, man):
    """The nearest a man's body (not his hands) comes to the chest's box, minus his breadth."""
    chest = rig["chest"][:3]
    return min(_to_box(p, chest) - r for a, b, r in _capsules(rig, man) for p in _along(a, b))


def _between(rig):
    return min(math.dist(p, q) - r1 - r2
               for a, b, r1 in _capsules(rig, "c2.") for c, e, r2 in _capsules(rig, "c3.")
               for p in _along(a, b) for q in _along(c, e))


class Carriers(unittest.TestCase):
    """The two men who carry the ammunition chest (#342), as built: `artpus.poses.json`."""

    def setUp(self):
        self.clips = _poses()
        if self.clips is None:
            self.skipTest("no artpus build (cargo xtask assets artpus)")

    def frames(self, carried=False):
        """(clip, frame, rig): the push and the carrying in (`carried`: only once the chest is
        up in the men's hands, from frame 28)."""
        for clip in ("GUN_PUSH", "CHEST_CARRY"):
            for f, pose in self.clips[clip].items():
                if not carried or clip == "GUN_PUSH" or int(f) >= 28:
                    yield clip, f, pose["artpus.rig"]

    def test_the_men_and_the_chest_do_not_overlap(self):
        for clip, f, rig in self.frames():
            self.assertGreater(_between(rig), GAP_MEN, (clip, f))
            for man in ("c2.", "c3."):
                # (Stooping to the chest on the ground they stand at its handles, close.)
                gap = GAP_CHEST if clip == "GUN_PUSH" or int(f) >= 28 else 0.0
                self.assertGreater(_to_chest(rig, man), gap, (clip, f, man))

    def test_the_hands_hold_the_rope_loops(self):
        """Each carrier's hand at his end of the chest is on the loop (CHEST_GRIP over the
        bottom, x from its middle), the left man's left, the right man's right."""
        hx, hz = guncrew.CHEST_GRIP
        for f, pose in self.clips["GUN_PUSH"].items():
            rig = pose["artpus.rig"]
            cx, cy, cz = rig["chest"][:3]
            for man, hand, sd in (("c2.", "hand_l", -1), ("c3.", "hand_r", 1)):
                loop = (cx + sd * hx, cy, cz + hz)
                self.assertLess(math.dist(rig[man + hand][:3], loop), HANDLE_NEAR, (f, man))

    def test_the_linstock_is_not_in_the_chests_hand(self):
        """The firer's linstock stands in his free (outer) hand, clear of the chest."""
        for clip, f, rig in self.frames(carried=True):
            tool = rig["c3.socket.hand_r"][:3]
            hand = rig["c3.socket.hand_l"][:3]
            self.assertLess(math.dist(tool, rig["c3.hand_l"][:3]), 0.2, (clip, f))
            self.assertGreater(abs(tool[0] - rig["chest"][0]), 0.5, (clip, f))
            axis = [(tool[0], tool[1], tool[2] + z) for z in (guncrew.LINSTOCK[0], guncrew.LINSTOCK[1])]
            self.assertGreater(min(_to_box(p, rig["chest"][:3]) for p in _along(*axis)), GAP_TOOL, (clip, f))
            self.assertLess(math.dist(tool[:2], hand[:2]), 0.2, (clip, f))


if __name__ == "__main__":
    unittest.main()
