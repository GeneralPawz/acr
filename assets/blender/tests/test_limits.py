"""Joint limits of skeleton H (`acr_assets.limits`) and the contact grip solver; and, when the
models are built, every frame of every clip within the limits (`stats.json` "limits")."""

import json
import math
import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "lib"))

from acr_assets import limits, rig  # noqa: E402

ROOT = os.path.join(os.path.dirname(__file__), "..", "..", "..")


def qx(deg):
    t = math.radians(deg) / 2
    return (math.cos(t), math.sin(t), 0.0, 0.0)


def qy(deg):
    t = math.radians(deg) / 2
    return (math.cos(t), 0.0, math.sin(t), 0.0)


def qz(deg):
    t = math.radians(deg) / 2
    return (math.cos(t), 0.0, 0.0, math.sin(t))


class Table(unittest.TestCase):
    def test_every_deforming_bone_has_a_limit(self):
        for b in rig.skeleton_h():
            if b.deform:
                self.assertIsNotNone(limits.limit(b.name), b.name)

    def test_ranges_hold_rest(self):
        for key, lim in limits.table()["bones"].items():
            for ax in ("x", "y", "z", "twist"):
                if ax in lim:
                    lo, hi = lim[ax]
                    self.assertLessEqual(lo, 0, key)
                    self.assertGreaterEqual(hi, 0, key)

    def test_fingers_are_hinges(self):
        # PIP about 100, DIP about 80 degrees of flexion, a few degrees of extension.
        self.assertEqual(limits.limit("index_02_l")["x"], [-5, 100])
        self.assertEqual(limits.limit("pinky_03_r")["x"], [-5, 80])
        self.assertEqual(limits.limit("middle_01_l")["x"], [-20, 90])
        self.assertEqual(limits.limit("forearm_r")["kind"], "hinge")

    def test_right_side_mirrors(self):
        lf, rt = limits.limit("hand_l"), limits.limit("hand_r")
        self.assertEqual(lf["x"], rt["x"])
        self.assertEqual(rt["z"], [-lf["z"][1], -lf["z"][0]])
        self.assertEqual(limits.key_of("rider.thigh_r"), ("thigh", "r"))
        self.assertEqual(limits.key_of("socket.hand_r"), (None, "r"))

    def test_excess(self):
        self.assertEqual(limits.excess("index_02_l", qx(60)), 0.0)
        self.assertAlmostEqual(limits.excess("index_02_l", qx(-20)), 15.0, places=3)
        self.assertAlmostEqual(limits.excess("forearm_l", qx(160)), 10.0, places=3)
        self.assertAlmostEqual(limits.excess("shin_l", qx(20)), 20.0, places=3)
        self.assertAlmostEqual(limits.excess("upperarm_l", qx(170)), 15.0, places=3)
        self.assertAlmostEqual(limits.excess("upperarm_l", qy(100)), 10.0, places=3)
        self.assertIsNone(limits.excess("hips", qz(90)))

    def test_euler_matches_order(self):
        e = limits.euler_xyz(qz(30))
        self.assertAlmostEqual(e[2], 30.0, places=4)
        e = limits.euler_xyz(qx(-45))
        self.assertAlmostEqual(e[0], -45.0, places=4)


class Wrap(unittest.TestCase):
    def finger(self):
        # A finger along -Z from the knuckle at the origin, palm towards -Y (+Z local = palmar),
        # flexing about +X... local X = Y x Z = (0,0,-1) x (0,-1,0) = (-1, 0, 0).
        x, y, z = (-1.0, 0.0, 0.0), (0.0, 0.0, -1.0), (0.0, -1.0, 0.0)
        lens = (0.045, 0.028, 0.022)
        chain, p = [], (0.0, 0.0, 0.0)
        for i, ln in enumerate(lens):
            name = f"index_0{i + 1}_l"
            chain.append((p, x, y, z, ln, 0.008, tuple(limits.limit(name)["x"])))
            p = (p[0], p[1], p[2] - ln)
        return chain

    def test_pads_touch_without_penetration(self):
        r = 0.019
        # The stock across the palm, its axis along X, r + palm pad before the knuckle.
        o, a = (0.0, -(r + 0.010), -0.005), (1.0, 0.0, 0.0)
        ang, gaps = limits.wrap_chain(self.finger(), o, a, r)
        for g in gaps:
            self.assertGreater(g, -0.0005)
            self.assertLess(g, 0.003)
        for d, (_h, _x, _y, _z, _l, _p, (lo, hi)) in zip(ang, self.finger()):
            self.assertTrue(lo <= d <= hi)
        self.assertGreater(sum(ang), 100)  # wrapped, not flat

    def test_out_of_reach_stops_at_range(self):
        o, a = (0.0, -0.5, 0.0), (1.0, 0.0, 0.0)
        ang, _g = limits.wrap_chain(self.finger(), o, a, 0.019)
        self.assertEqual(ang, [90, 100, 80])


class Clips(unittest.TestCase):
    """Built models: every frame of every clip within the limits (the build's audit,
    `armature.audit_limits`). Skips models not built."""

    UNITS = ("hdman", "engm18", "delkri", "engd18")

    def test_hands_within_limits(self):
        seen = 0
        for u in self.UNITS:
            p = os.environ.get(f"ACR_{u.upper()}_STATS") or os.path.join(ROOT, "target", "assets", "units", f"{u}.stats.json")
            if not os.path.isfile(p):
                continue
            with open(p, encoding="utf-8") as f:
                lim = json.load(f).get("limits")
            if lim is None:
                continue
            seen += 1
            hands = {b: v for b, v in lim["by_bone"].items()
                     if limits.key_of(b)[0] and limits.key_of(b)[0].startswith(("finger", "thumb"))}
            self.assertEqual(hands, {}, u)
        if not seen:
            self.skipTest("no built model with a limits audit")
