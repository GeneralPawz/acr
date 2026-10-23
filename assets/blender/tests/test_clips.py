"""Clips by weapon (`acr_assets.clips`) and the lace and paint masks' registry side (pure
Python, no Blender)."""

import math
import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "lib"))

from acr_assets import clips, held, masks, registry  # noqa: E402

# `.MD` lines of the units the clips serve (game files, read with `acr extract`).
IROLUT = {"ATTACK1": "#ATTACK1 9 79 " + " ".join(f"1 {i}" for i in range(79)),
          "ATTACK2": "#ATTACK2 9 79 " + " ".join(f"1 {i}" for i in range(79)),
          "ATTACK": "@ATTACK    9 2 0 55", "PSTAND1": "#PSTAND1 9 1 1 0", "PSTAND2": "#PSTAND2 9 1 1 0"}
IROLUT_ACTIVE = {"ATTACK1": 44, "ATTACK2": 44}
IROTAM = {"ATTACK": "#ATTACK 9 60 " + " ".join(f"6 {i}" for i in range(60)),
          "ATTACK1": "#ATTACK1 9 50 " + " ".join(f"3 {i}" for i in range(50))}
INCPRA = {"ATTACK1": "@ATTACK1   9 1 0 67", "ATTACK": "@ATTACK    9 2 0 60"}
INCTRB = {"ATTACK": "@ATTACK   9 6 0 60", "ATTACK1": "@ATTACK1  9 3 0 60"}
ATCMET = {"ATTACK1": "@ATTACK1   9 1 0 54", "ATTACK": "@ATTACK    9 2 0 53"}
SPNSHA = {"ATTACK": "@ATTACK    9 3 0 199"}
ENGB18 = {"REST": "@REST 9 0 0 69", "STAND": "#STAND     9 1   0 0", "MOTION_L": "@MOTION_L  9 3 0 9",
          "MOTION_R": "@MOTION_R  9 3 10 19", "MOTION_L0": "@MOTION_L0 9 1 0 9", "MOTION_R0": "@MOTION_R0 9 1 10 19"}
ENGF18 = {"REST": "@REST 9 0 0 119", "STAND": "#STAND     9 1 0 119", "MOTION_L": "@MOTION_L  9 1 0 9",
          "MOTION_R": "@MOTION_R  9 1 10 19"}
ENGOF18 = {"ATTACK": "@ATTACK 9 3 0 51"}
DELPIK = {"ATTACK": "@ATTACK    9 3 0 68"}


def dist(a, b):
    return math.sqrt(sum((p - q) ** 2 for p, q in zip(a, b)))


def tip_at(pose, length):
    grip, _pole = pose["reach"]["r"]
    return clips.add(grip, clips.axis(pose["world"]["socket.hand_r"]), length)


class Timing(unittest.TestCase):
    def test_release_frame_rounds_up_to_even(self):
        self.assertEqual([clips.release_frame(f) for f in (0, 1, 2, 15, 17, 44, 56)], [2, 2, 2, 16, 18, 44, 56])

    def check(self, clip, n, event):
        self.assertEqual(clip[0][0], 0)
        self.assertEqual(clip[-1][0], n - 1)
        self.assertEqual(clip.event, event)
        fs = [f for f, _ in clip]
        self.assertEqual(fs, sorted(set(fs)))
        if event is not None:
            self.assertIn(event, fs)

    def test_bow(self):
        c = clips.unit_clips(IROLUT, {0: "MECH", 1: "OSTRELA", 2: "STRELA"}, melee="club", active=IROLUT_ACTIVE)
        for k in ("ATTACK1", "ATTACK2"):
            self.check(c[k], 79, 44)
            at = dict(c[k])
            # The arrow is in the hand up to the frame before, gone on the release.
            self.assertEqual(at[43]["show"]["socket.hand_r"], 1)
            self.assertEqual(at[44]["show"]["socket.hand_r"], 0)
        self.assertEqual(c["PSTAND1"].event, None)
        for k in ("ATTACK", "ATTACK_CUT_MID", "ATTACK_CUT_DOWN", "ATTACK_DOWN"):
            self.check(c[k], 56, 2)

    def test_arrow_on_the_nock_and_the_bow(self):
        for pose in (clips.bow_ready(), clips.bow_full()):
            l_grip, r_grip = pose["reach"]["l"][0], pose["reach"]["r"][0]
            arrow_tip = clips.add(r_grip, clips.axis(pose["world"]["socket.hand_r"]), clips.ARROW)
            # The arrow runs from the drawing hand through the bow's grip.
            self.assertLess(dist(clips.add(r_grip, clips.axis(pose["world"]["socket.hand_r"]), dist(r_grip, l_grip)),
                                 l_grip), 0.01)
            self.assertGreater(dist(arrow_tip, r_grip), dist(l_grip, r_grip))
        ready = clips.bow_ready()
        nock = clips.nock(ready["reach"]["l"][0], ready["world"]["socket.hand_l"])
        self.assertLess(dist(nock, ready["reach"]["r"][0]), 0.01)
        self.assertAlmostEqual(held.build("longbow").sockets["socket.nock"][1], clips.BOW_NOCK, places=3)

    def test_thrown(self):
        c = clips.unit_clips(IROTAM, {1: "TAMAGAVK", 0: "MECH"}, melee="tomahawk", active={"ATTACK1": 15})
        self.check(c["ATTACK1"], 50, 16)
        at = dict(c["ATTACK1"])
        self.assertEqual(at[15]["show"]["socket.hand_r"], 1)
        self.assertEqual(at[16]["show"]["socket.hand_r"], 0)
        self.check(c["ATTACK_CUT_MID"], 60, 2)
        c = clips.unit_clips(ATCMET, {0: "PIKA", 1: "KOPIE"}, melee="spear", active={"ATTACK1": 28})
        self.check(c["ATTACK1"], 55, 28)
        self.check(c["ATTACK_THRUST_MID"], 54, 2)

    def test_sling_blowpipe_heal(self):
        c = clips.unit_clips(INCPRA, {0: "MECH", 1: "KAMEN"}, melee="club", active={"ATTACK1": 56})
        self.check(c["ATTACK1"], 68, 56)
        c = clips.unit_clips(INCTRB, {0: "MECH", 1: "VAFLIA"}, melee="club", active={"ATTACK1": 17})
        self.check(c["ATTACK1"], 61, 18)
        c = clips.unit_clips(SPNSHA, {0: "MEDIK"}, active={"ATTACK": 1})
        self.check(c["ATTACK"], 200, 2)

    def test_sling_whirls_overhead(self):
        c = clips.sling(68, 56)
        overhead = [p for f, p in c if 10 < f < 50 and p["reach"]["r"][0][2] > 1.8]
        self.assertGreaterEqual(len(overhead), 8)

    def test_drummer_and_colour(self):
        c = clips.drummer_clips(ENGB18)
        self.assertEqual(set(c), {"STAND", "REST", "MOTION_L", "MOTION_R", "MOTION_L0", "MOTION_R0"})
        self.check(c["REST"], 70, None)
        self.check(c["MOTION_L"], 10, None)
        c = clips.colour_clips(ENGF18)
        self.assertEqual(set(c), {"STAND", "REST", "MOTION_L", "MOTION_R"})
        self.check(c["REST"], 120, None)

    def test_sticks_on_the_drum_head(self):
        for f, pose in clips.drum_roll(70):
            for side, strike in (("r", clips.STRIKE_R), ("l", clips.STRIKE_L)):
                tip = clips.stick_tip(pose["reach"][side][0], pose["world"][f"socket.hand_{side}"])
                d = dist(tip, strike)
                # Down: on the head; up: raised clear of it.
                self.assertTrue(d < 0.002 or 0.09 < d < 0.11, (f, side, d))
        self.assertAlmostEqual(clips.STICK, 0.42 * 2 / 3)
        self.assertAlmostEqual(held.build("drumstick").sockets["socket.tip"][2], clips.STICK)


class Strikes(unittest.TestCase):
    def test_names_and_hit_frame(self):
        c = clips.unit_clips(ENGOF18, {0: "MECH"}, melee="sword")
        self.assertEqual(set(c), {"ATTACK", "ATTACK_CUT_MID", "ATTACK_CUT_HIGH", "ATTACK_THRUST_MID",
                                  "ATTACK_THRUST_DOWN", "ATTACK_DOWN"})
        for k, v in c.items():
            self.assertEqual(v[-1][0], 51, k)
            self.assertEqual(v.event, clips.STRIKE_HIT_FRAME, k)
            self.assertIn(2, [f for f, _ in v], k)
        c = clips.unit_clips(DELPIK, {0: "PIKA"}, melee="spear")
        self.assertEqual(set(c), {"ATTACK", "ATTACK_THRUST_MID", "ATTACK_THRUST_DOWN", "ATTACK_DOWN"})

    def test_points_on_the_targets(self):
        """At the hit frame the weapon's point (grip goal + axis x length) is on the target:
        the clip's data; the IK's reach is checked by the probe on a built model."""
        for strikes, length in ((clips.sword_strikes(52), clips.SWORD_TIP), (clips.club_strikes(56), clips.CLUB_TIP),
                                (clips.spear_strikes(69), clips.SPEAR_TIP)):
            for name, clip in strikes.items():
                hit = dict(clip)[2]
                tip = tip_at(hit, length)
                target = {"MID": clips.TARGET_MID, "HIGH": clips.TARGET_HIGH, "DOWN": clips.TARGET_DOWN}[name.split("_")[-1]]
                if name == "ATTACK_CUT_MID" and length == clips.CLUB_TIP:
                    target = clips.add(target, (0, 0, 0.12))
                self.assertLess(dist(tip, target), 0.01, name)

    def test_down_blows_lean_forward(self):
        """Positive spine X bends forward on skeleton H (m2-shared-moves): the blows down
        lean over the man, at 2.0 m."""
        for base in (clips.STOOP, clips.CLUB_STOOP):
            self.assertGreater(base["spine"][0], 20)
            self.assertLess(base["root_loc"][1], -0.7)
        self.assertGreater(clips.LEAN["spine"][0], 0)
        self.assertAlmostEqual(clips.TARGET_DOWN[1], -2.0)

    def test_point_off_the_axis(self):
        """`grip_for` with the point as a vector in the item frame (an axe's edge): the
        point lands on the target; the club's and tomahawk's points are `held`'s."""
        r = (130, 0, 0)
        for tip in (clips.MELEE_TIP["club"], clips.MELEE_TIP["tomahawk"]):
            g = clips.grip_for(clips.TARGET_DOWN, r, tip)
            self.assertLess(dist(clips.add(g, clips.rot(r, tip)), clips.TARGET_DOWN), 1e-9)
        self.assertEqual(tuple(held.build("war_club").sockets["socket.tip"]), clips.MELEE_TIP["club"])
        self.assertEqual(tuple(held.build("tomahawk").sockets["socket.tip"]), clips.MELEE_TIP["tomahawk"])

    def test_drummer_stands_ready(self):
        """STAND: both stick tips just over the drum head (not raised)."""
        p = clips.STAND_DRUM
        for side in ("r", "l"):
            grip = p["reach"][side][0]
            tip = clips.stick_tip(grip, p["world"][f"socket.hand_{side}"])
            self.assertLess(dist(tip, clips.DRUM_HEAD), 0.09, side)

    def test_held_lengths(self):
        self.assertAlmostEqual(held.build("spear").sockets["socket.tip"][2], clips.SPEAR_TIP, delta=0.03)
        self.assertAlmostEqual(held.build("arrow").sockets["socket.tip"][2], clips.ARROW, delta=0.01)
        self.assertAlmostEqual(held.build("smallsword").sockets["socket.tip"][2], clips.SWORD_TIP, delta=0.08)


class Masks(unittest.TestCase):
    def test_registered(self):
        for m in ("hat_lace", "buttonhole_lace", "paint_face_half", "paint_face_band", "paint_stripes",
                  "paint_hands", "paint_dots"):
            self.assertIn(m, registry.MASKS)
        for k in ("lace", "paint_red", "paint_black", "paint_white"):
            self.assertIn(k, registry.FALLBACK)
            self.assertIn(k, registry.LAYER_OF)

    def test_paint_sets(self):
        self.assertEqual(masks.paint_set(7), masks.paint_set(7))
        picked = {id(masks.paint_set(s)) for s in range(40)}
        self.assertGreater(len(picked), 3)
        for name in masks.PAINT_SETS:
            for e in masks.body_paint(name):
                self.assertEqual(e["keys"], ("skin",))
                self.assertIn(e["mask"], registry.MASKS)

    def test_lace_entries(self):
        es = masks.lace({"lace": (0.9, 0.9, 0.9)}, hat="X.tricorn", width=0.01)
        self.assertEqual([e["mask"] for e in es], ["hat_lace", "buttonhole_lace"])
        self.assertEqual(es[0]["frame"], "X.tricorn")
        self.assertEqual(es[0]["rgb"], (0.9, 0.9, 0.9))
        self.assertEqual(masks.lace(None, hat=None)[0]["rgb"], registry.FALLBACK["lace"])

    def test_registry_rejects_a_second_mask(self):
        with self.assertRaises(ValueError):
            registry.mask("hat_lace", lambda *a, **k: None)


if __name__ == "__main__":
    unittest.main()
