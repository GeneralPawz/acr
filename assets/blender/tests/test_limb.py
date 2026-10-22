"""Limb levels, radius profiles and crawl clips (pure Python, no Blender)."""

import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "lib"))

from acr_assets import crawl, limb, rig  # noqa: E402


def chain(name):
    bones = {b.name: b for b in rig.skeleton_h()}
    return [getattr(bones[b], end) for b, end in limb.LIMBS[name][1]]


class Levels(unittest.TestCase):
    def test_joints_land_on_their_levels(self):
        for name, levels in (("arm_l", (0, 110, 205, 255)), ("leg_r", (0, 120, 215, 255))):
            pts = chain(name)
            self.assertEqual(limb.LIMBS[name][0], levels)
            for p, lv in zip(pts, levels):
                got, d = limb.level(p, pts, levels)
                self.assertAlmostEqual(got * 255, lv, places=6)
                self.assertAlmostEqual(d, 0.0, places=9)
        self.assertEqual((limb.ELBOW, limb.WRIST, limb.KNEE, limb.ANKLE), (110, 205, 120, 215))

    def test_piecewise_linear(self):
        pts = chain("arm_l")
        lv = limb.ARM_LEVELS
        mid = tuple((a + b) / 2 for a, b in zip(pts[1], pts[2]))  # mid forearm
        got, _ = limb.level(mid, pts, lv)
        self.assertAlmostEqual(got * 255, (110 + 205) / 2, places=6)
        q = tuple(a + 0.25 * (b - a) for a, b in zip(pts[0], pts[1]))
        self.assertAlmostEqual(limb.level(q, pts, lv)[0] * 255, 27.5, places=6)

    def test_offset_point_projects(self):
        pts = chain("leg_l")
        mid = tuple((a + b) / 2 for a, b in zip(pts[0], pts[1]))
        p = (mid[0], mid[1] - 0.07, mid[2])  # on the front of the thigh
        got, d = limb.level(p, pts, limb.LEG_LEVELS, (0,))
        self.assertAlmostEqual(got * 255, 60, delta=1.0)
        self.assertAlmostEqual(d, 0.07, delta=0.002)

    def test_part_restricts_segments(self):
        pts = chain("arm_l")
        chains = {n: chain(n) for n in limb.LIMBS}
        beyond = tuple(e + 0.3 * (w - e) for e, w in zip(pts[1], pts[2]))  # on the forearm
        # An upper-arm vertex there clamps at the elbow; a forearm vertex goes on.
        self.assertAlmostEqual(limb.vertex_level(beyond, 2, chains)[1] * 255, 110, places=6)
        self.assertGreater(limb.vertex_level(beyond, 3, chains)[1] * 255, 110)
        self.assertEqual(limb.vertex_level(beyond, 0, chains), (None, limb.TRUNK, 0.0))
        self.assertEqual(limb.vertex_level(beyond, 48, chains)[1], limb.TRUNK)

    def test_body_part_of_caps(self):
        j = rig.JOINTS
        self.assertEqual(limb.body_part(16 + 2, j), 2)  # elbow_l, parent side: upper arm
        self.assertEqual(limb.body_part(32 + 2, j), 3)  # child side: forearm
        self.assertEqual(limb.body_part(16 + 1, j), 0)  # shoulder, parent side: torso
        self.assertEqual(limb.body_part(48, j), 48)


class Radius(unittest.TestCase):
    def test_profile_bins_and_fills(self):
        samples = [(0.0, 0.06), (0.0, 0.05), (1.0, 0.02)]
        p = limb.radius_profile(samples)
        self.assertEqual(len(p), limb.SAMPLES)
        self.assertAlmostEqual(p[0], 0.06)
        self.assertAlmostEqual(p[-1], 0.02)
        self.assertAlmostEqual(p[8], 0.06 + (0.02 - 0.06) * 8 / 15, places=4)
        self.assertEqual(limb.radius_profile([]), [0.0] * limb.SAMPLES)


class Crawl(unittest.TestCase):
    def test_euler_round_trip(self):
        for r in ((10, 20, 30), (-80, 5, 170), (0, 0, 0), (45, -60, -10)):
            m = crawl.euler_matrix(r)
            back = crawl.euler_matrix(crawl.to_euler(m))
            for i in range(3):
                for k in range(3):
                    self.assertAlmostEqual(m[i][k], back[i][k], places=4)

    def test_aim_chain_points_bones(self):
        spec = {"spine": (0, 10, 0), "spine2": (-10, 0, 5)}
        want = [(0.5, -0.3, 0.8), (-0.3, -0.1, 0.95), (-0.25, -0.3, 0.92)]
        crawl.aim_chain(spec, ["upperarm_l", "forearm_l", "hand_l"], want)
        for bone, d in zip(["upperarm_l", "forearm_l", "hand_l"], want):
            got = crawl.mat_vec(crawl.delta(spec, bone), crawl.rest_dir(bone))
            d = crawl._norm(d)
            for a, b in zip(got, d):
                self.assertAlmostEqual(a, b, places=3)

    def test_clips_loop_and_lie_down(self):
        cl = crawl.clips()
        self.assertEqual(set(cl), {"CRAWL2", "CRAWL1", "CRAWLARM", "WRITHE"})
        for name, keys in cl.items():
            self.assertEqual(keys[0][0], 0)
            self.assertEqual(keys[-1][0], crawl.FRAMES[name])
            self.assertEqual(keys[0][1], keys[-1][1], name)  # loops
            for _f, spec in keys:
                up = crawl.mat_vec(crawl.euler_matrix(spec["root"]), (0, 0, 1))
                self.assertLess(abs(up[2]), 0.3, name)  # lying, not standing
                self.assertLess(up[1], -0.9, name)  # head first (towards -Y)
        prone = crawl.mat_vec(crawl.euler_matrix(cl["CRAWL2"][0][1]["root"]), (0, -1, 0))
        self.assertLess(prone[2], -0.9)  # face down
