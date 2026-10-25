"""The held musket against the man (`acr_assets.penetration`, m2-aim-hd): the summary and
the tolerance check, and the built units' "musket_penetration" stats when they exist."""

import json
import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "lib"))

from acr_assets import penetration as pen  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
UNITS = os.path.join(HERE, "..", "..", "..", "target", "assets", "units")
# The clips of the shouldered and the hip shot, checked frame by frame on every unit that
# measured them (the reload, the charge and the blows are reported, not gated yet).
CHECKED = ("FIRE_AIMED", "FIRE_HIP", "PSTAND1")


def _stats(name):
    path = os.path.join(os.environ.get("ACR_UNITS_DIR", UNITS), f"{name}.stats.json")
    if not os.path.exists(path):
        return None
    with open(path, encoding="utf-8") as f:
        return json.load(f).get("musket_penetration")


class Summary(unittest.TestCase):
    def test_contact_uses_the_nearest_surface(self):
        self.assertEqual(pen.nearest_surface_gap((0.003, -0.19, 0.04)), 0.003)
        self.assertEqual(pen.nearest_surface_gap((0.02, -0.004, None)), -0.004)
        self.assertIsNone(pen.nearest_surface_gap((None,)))

    def test_summary_takes_the_deepest_either_way(self):
        frames = [(0, (0, 0.0), (0, 0.0)), (1, (12, 0.03), (2, 0.004)), (2, (3, 0.01), (5, 0.02))]
        s = pen.summarise(frames)
        self.assertEqual(s["frames"], 3)
        self.assertEqual(s["gun_in_body"], {"max_n": 12, "depth_m": 0.03})
        self.assertEqual(s["body_in_gun"], {"max_n": 5, "depth_m": 0.02})
        self.assertEqual(s["worst_frame"], 1)

    def test_parity_inside(self):
        # Inside a closed body every ray crosses once; outside a garment tube twice or not at
        # all; behind a single flap or past a hole only some rays cross an odd count.
        self.assertTrue(pen.parity_inside([1, 1, 3, 1, 1, 1]))
        self.assertTrue(pen.parity_inside([1, 1, 1, 1, 1, 0]))
        self.assertFalse(pen.parity_inside([0, 2, 0, 2, 0, 0]))
        self.assertFalse(pen.parity_inside([1, 0, 1, 0, 0, 0]))

    def test_over_tolerance(self):
        ok = pen.summarise([(0, (0, 0.002), (0, 0.0))])
        bad = pen.summarise([(0, (40, 0.08), (0, 0.0))])
        st = {"clips": {"FIRE_AIMED": bad, "STAND": ok}}
        self.assertEqual(pen.over(st), [("FIRE_AIMED", 0.08)])
        self.assertEqual(pen.over(st, clips=("STAND",)), [])
        self.assertEqual(pen.total(st["clips"])["worst"], ["FIRE_AIMED", 0])


class Built(unittest.TestCase):
    """The fire clips of every built musketeer keep the musket out of the man. Opt-in
    (`ACR_PENETRATION_GATE=1`) until the present is re-solved by contact: on 908a721 every
    musketeer's FIRE_AIMED has the butt in the shoulder (m2-aim-hd)."""

    def test_fire_clips_within_tolerance(self):
        if not os.environ.get("ACR_PENETRATION_GATE"):
            self.skipTest("the aimed present still penetrates (m2-aim-hd); set ACR_PENETRATION_GATE=1 to gate")
        seen = 0
        for name in ("hdman", "hdengm18", "engm18", "engmus"):
            st = _stats(name)
            if not st:
                continue
            seen += 1
            self.assertEqual(pen.over(st, clips=CHECKED), [], name)
        if not seen:
            self.skipTest("no built musketeer with musket_penetration stats (cargo xtask assets hdman)")


if __name__ == "__main__":
    unittest.main()
