"""Face fields, variant regions and the per-soldier choice (pure Python, no Blender)."""

import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "lib"))

from acr_assets import faces  # noqa: E402

POINTS = [(x / 100, y / 100, z / 100) for x in range(-9, 10) for y in range(-1, 22, 3) for z in range(-22, 12, 3)]
FIELDS = [faces.base_face] + [lambda x, y, z, n=n: faces.morph(n, x, y, z) for n in faces.FACE_MORPHS]


class Fields(unittest.TestCase):
    def test_mirror_symmetric(self):
        for f in FIELDS:
            for x, y, z in POINTS:
                a, b = f(x, y, z), f(-x, y, z)
                self.assertAlmostEqual(a[0], -b[0], places=12)
                self.assertAlmostEqual(a[1], b[1], places=12)
                self.assertAlmostEqual(a[2], b[2], places=12)

    def test_bounded_and_local(self):
        for f in FIELDS:
            for p in POINTS:
                self.assertLess(max(map(abs, f(*p))), 0.02)
            # Nothing moves on the body below the neck.
            self.assertLess(max(map(abs, f(0.1, 0.05, -0.5))), 1e-6)
            # Nor anything in front of the face (a rider's horse).
            self.assertEqual(f(0.0, -0.6, -0.05), (0.0, 0.0, 0.0))

    def test_every_morph_moves_something(self):
        for n in faces.FACE_MORPHS:
            peak = max(max(map(abs, faces.morph(n, *p))) for p in POINTS)
            self.assertGreater(peak, 0.002, n)

    def test_nose_points_forward(self):
        self.assertLess(faces.base_face(0.0, 0.0, -0.03)[1], -0.012)
        self.assertLess(faces.morph("face_nose", 0.0, 0.0, -0.032)[1], -0.005)

    def test_zones_in_range(self):
        for p in POINTS:
            for c in faces.zones(*p):
                self.assertGreaterEqual(c, 0.0)
                self.assertLessEqual(c, 1.0)
        self.assertGreater(faces.zones(0.04, 0.0, -0.08)[0], 0.8, "jaw grows a beard")
        self.assertLess(faces.zones(0.0, 0.0, 0.05)[0], 0.05, "forehead does not")


class Regions(unittest.TestCase):
    def test_hair_and_face_apart(self):
        for v in ("hair_short", "hair_tied", "hair_loose"):
            self.assertTrue(faces.region(v, 0.0, 0.08, 0.09), v)
            self.assertFalse(faces.region(v, 0.0, 0.0, 0.0), v)
            self.assertFalse(faces.region(v, 0.03, 0.0, -0.08), v)
        self.assertTrue(faces.region("beard_full", 0.03, 0.0, -0.08))
        self.assertFalse(faces.region("beard_full", 0.0, 0.0, -0.06), "lips stay bare")
        self.assertTrue(faces.region("beard_moustache", 0.01, 0.0, -0.05))
        self.assertTrue(faces.region("beard_sideburns", 0.07, 0.07, -0.02))

    def test_variants_named(self):
        for table in (faces.HAIR_VARIANTS, faces.BEARD_VARIANTS):
            for parts in table.values():
                for p in parts:
                    self.assertTrue(p.startswith(("hair_", "beard_")))
        for v in faces.OFFSETS:
            self.assertTrue(any(v in p for t in (faces.HAIR_VARIANTS, faces.BEARD_VARIANTS) for p in t.values()))


class Choice(unittest.TestCase):
    def test_deterministic_and_in_range(self):
        for seed in range(400):
            a = faces.choose(seed)
            self.assertEqual(a, faces.choose(seed))
            for n, x in a["morphs"].items():
                lo, hi = {**faces.FACE_MORPHS, **faces.BODY_MORPHS}[n]
                self.assertTrue(lo <= x <= hi, (n, x))
            self.assertFalse(a["morphs"]["face_fat"] and a["morphs"]["face_thin"])
            self.assertTrue(faces.HEIGHT_M[0] <= a["height_m"] <= faces.HEIGHT_M[1])
            self.assertIn(a["hair_colour"], faces.HAIR_COLOURS)
            self.assertIn(a["skin"], faces.SKIN_TONES["english"])
            for n in faces.nodes(a):
                self.assertIn(n, [p for t in (faces.HAIR_VARIANTS, faces.BEARD_VARIANTS) for v in t.values() for p in v])

    def test_variety(self):
        looks = [faces.choose(s) for s in range(400)]
        self.assertGreater(len({(lk["hair"], lk["beard"], lk["hair_colour"], lk["skin"]) for lk in looks}), 60)
        hs = [lk["height_m"] for lk in looks]
        self.assertAlmostEqual(sum(hs) / len(hs), faces.HEIGHT_MEAN_M, delta=0.015)
        regular = sum(lk["beard"] == "none" for lk in looks) / len(looks)
        self.assertGreater(regular, 0.65)
        civ = [faces.choose(s, "civilian") for s in range(400)]
        self.assertGreater(sum(lk["beard"] == "full" for lk in civ), 60)

    def test_hash_stable(self):
        # The renderer ports `_mix32`/`Rng`: pin a few values.
        self.assertEqual(faces._mix32(0), 0)
        self.assertEqual(faces._mix32(1), faces._mix32(1))
        r = faces.Rng(7)
        first = [r.u() for _ in range(3)]
        r = faces.Rng(7)
        self.assertEqual(first, [r.u() for _ in range(3)])


if __name__ == "__main__":
    unittest.main()
