"""Face fields, variant regions and the per-soldier choice (pure Python, no Blender)."""

import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "lib"))

from acr_assets import faces  # noqa: E402

POINTS = [(x / 100, y / 100, z / 100) for x in range(-9, 10) for y in range(-1, 22, 3) for z in range(-22, 12, 3)]
FIELDS = [faces.base_face] + [lambda x, y, z, n=n: faces.morph(n, x, y, z) for n in faces.FACE_MORPHS]


class FaceSource(unittest.TestCase):
    def test_lookup_order_and_fallback(self):
        have = {os.path.join("/f", "engmus.png"), os.path.join("/s", "faces", "engmus.png")}
        ok = have.__contains__
        self.assertEqual(faces.face_source("EngMus", {"ACR_FACES": "/f", "ACR_SOURCES": "/s"}, ok),
                         os.path.join("/f", "engmus.png"))
        self.assertEqual(faces.face_source("engmus", {"ACR_SOURCES": "/s"}, ok), os.path.join("/s", "faces", "engmus.png"))
        self.assertIsNone(faces.face_source("engm18", {"ACR_SOURCES": "/s"}, ok))
        self.assertIsNone(faces.face_source("engmus", {}, ok))

    def test_size(self):
        self.assertEqual(faces.face_size(), faces.FACE_SIZE)
        self.assertEqual(faces.face_size(512), faces.FACE_SIZE)
        self.assertEqual(faces.face_size(1536), 1024)
        self.assertEqual(faces.face_size(2048), 2048)
        self.assertEqual(faces.face_size(4096), faces.FACE_SIZE_MAX)

    def test_portrait(self):
        ok = {os.path.join("/f", "engm18.portrait.png")}.__contains__
        self.assertEqual(faces.portrait_sources("EngM18", {"ACR_FACES": "/f"}, ok), [os.path.join("/f", "engm18.portrait.png")])
        self.assertEqual(faces.portrait_sources("engm18", {"ACR_SOURCES": "/f"}, ok), [])
        # The default set: European units only, from $ACR_SOURCES, off by ACR_PORTRAITS=0.
        lib = [os.path.join("/s", faces.PORTRAIT_DIR, f"{k}.png") for k in faces.PORTRAIT_SET]
        have = set(lib).__contains__
        self.assertEqual(faces.portrait_sources("EngM18", {"ACR_SOURCES": "/s"}, have), lib)
        self.assertEqual(faces.portrait_sources("EngM18", {"ACR_SOURCES": "/s"}, have, tiles=1), lib[:1])
        self.assertEqual(faces.portrait_sources("DelLut", {"ACR_SOURCES": "/s", "ACR_PORTRAIT_GROUPS": "eu"}, have), [])
        self.assertEqual(faces.portrait_sources("EngM18", {"ACR_SOURCES": "/s", "ACR_PORTRAITS": "0"}, have), [])
        self.assertEqual(faces.portrait_sources("EngM18", {}, have), [])
        # Numbered portraits in $ACR_FACES come first.
        mine = {os.path.join("/f", f"engm18.portrait{k}.png") for k in (1, 2)} | set(lib)
        self.assertEqual(faces.portrait_sources("EngM18", {"ACR_FACES": "/f", "ACR_SOURCES": "/s"}, mine.__contains__),
                         [os.path.join("/f", "engm18.portrait1.png"), os.path.join("/f", "engm18.portrait2.png")])
        self.assertEqual([faces.tile_grid(n) for n in (1, 2, 4, 5)], [1, 2, 2, 3])

    def test_portrait_groups(self):
        self.assertEqual([faces.portrait_group(n) for n in ("EngM18", "DelLut", "IroVog", "SiuKVog", "PueLut", "AtcVog",
                                                            "MayKri", "IncTrb", "Artpus")],
                         ["eu", "wd", "wd", "pl", "sw", "ms", "ms", "an", None])
        lib = {os.path.join("/s", faces.PORTRAIT_DIR, f"{g}{k}_s1.png") for g in faces.PORTRAIT_GROUPS for k in range(1, 5)}
        env = {"ACR_SOURCES": "/s", "ACR_PORTRAIT_GROUPS": "eu,pl"}
        self.assertEqual(faces.portrait_sources("SiuKVog", env, lib.__contains__, tiles=2),
                         [os.path.join("/s", faces.PORTRAIT_DIR, f"pl{k}_s1.png") for k in (1, 2)])
        self.assertEqual(faces.portrait_sources("IncTrb", env, lib.__contains__), [])
        self.assertEqual(faces.portrait_sources("IncTrb", dict(env, ACR_PORTRAIT_GROUPS=""), lib.__contains__), [])
        self.assertTrue(set(faces.PORTRAIT_ON) <= set(faces.PORTRAIT_GROUPS))

    def test_skin_match(self):
        # A matching photo stays as it is.
        for a, b in zip(faces.skin_match((0.3, 0.2, 0.15), (0.3, 0.2, 0.15)), (1.0, 1.0, 1.0)):
            self.assertAlmostEqual(a, b)
        # eu2: a dark, red rim asked for (1.15, 1.71, 1.64); now one gain within SKIN_GAIN and
        # each channel at most SKIN_TINT off it.
        painted = (0.30, 0.22, 0.17)
        photo = (painted[0] / 1.147, painted[1] / 1.708, painted[2] / 1.64)
        r = faces.skin_match(painted, photo)
        self.assertLessEqual(max(r), faces.SKIN_GAIN[1] * (1 + faces.SKIN_TINT) + 1e-6)
        self.assertGreaterEqual(min(r), faces.SKIN_GAIN[0] * (1 - faces.SKIN_TINT) - 1e-6)
        self.assertLess(max(r) / min(r), (1 + faces.SKIN_TINT) / (1 - faces.SKIN_TINT) + 1e-6)
        self.assertLess(r[0], r[1])  # still pulls the red back a little
        x0, x1, z0, z1 = faces.PORTRAIT_BOX
        self.assertEqual(faces.portrait_px(x0, z1, 768, 768), (0.0, 0.0))
        c, r = faces.portrait_px(0.0, 0.0, 768, 768)
        self.assertAlmostEqual(c, 384.0)
        self.assertLess(r, 384.0)  # the eyes above the frame's middle
        self.assertAlmostEqual(faces.portrait_radius(faces.PORTRAIT_AXES[0], faces.PORTRAIT_CENTRE_Z), 1.0)


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
        for v in list(faces.PATCHES) + list(faces.OTHER_VARIANTS):
            self.assertTrue(any(v in p for t in (faces.HAIR_VARIANTS, faces.BEARD_VARIANTS) for p in t.values()))

    def test_sideburns_before_the_ear(self):
        # A ray that reaches the ear lifts the patch onto it: a card standing off the cheek.
        for row in faces.patch_grid("beard_sideburns"):
            for th, ph, _tp in row:
                d = faces.ray_dir(th, ph)
                at = [c + 0.07 * k for c, k in zip(faces.CENTRE, d)]
                self.assertEqual(faces.ear(*at), 0.0, (th, ph))

    def test_patches(self):
        for name in faces.PATCHES:
            rows = faces.patch_grid(name)
            for row in rows:
                for th, ph, tp in row:
                    self.assertTrue(-90 <= ph <= 90 and 0 <= tp <= 1, name)
                    self.assertGreater(faces.thickness(name, th, 0.5, tp), 0.0)
            # Open edges fade into the skin, the inside does not.
            self.assertLess(rows[0][len(rows[0]) // 2][2], 0.01, name)
            self.assertGreater(max(tp for row in rows for _t, _p, tp in row), 0.9, name)
        # The hairline is high on the forehead and low at the nape.
        self.assertGreater(faces.curve(faces.HAIRLINE, 0), 30)
        self.assertLess(faces.curve(faces.HAIRLINE, 180), -30)
        self.assertAlmostEqual(sum(c * c for c in faces.ray_dir(33, -12)), 1.0)
        # Ragged edge: locks reach below the line.
        row = faces.patch_grid("hair_tied")[0]
        self.assertLess(min(p - faces.curve(faces.HAIRLINE, t) for t, p, _tp in row), -1.0)
        self.assertTrue(all(0.35 <= faces.lock(i) <= 1.0 for i in range(50)))

    def test_asym_small_and_local(self):
        self.assertGreater(max(map(abs, faces.asym(0.0, 0.0, -0.025))), 0.001)
        self.assertEqual(faces.asym(0.0, 0.05, -0.6), (0.0, 0.0, 0.0))


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
