"""The clothing audit's pure parts (`acr_assets.clothaudit`, docs/clothing.md "Clothing audit")."""

import importlib.util
import math
import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "lib"))

HAVE_NUMPY = importlib.util.find_spec("numpy") is not None
if HAVE_NUMPY:
    import numpy as np

    from acr_assets import clothaudit as ca


def grid(nx, ny, step=0.02, diag=True):
    """A flat grid of quads split into right triangles: (positions, triangles)."""
    pos = np.array([[x * step, y * step, 0.0] for y in range(ny + 1) for x in range(nx + 1)])
    tri = []
    for y in range(ny):
        for x in range(nx):
            a = y * (nx + 1) + x
            b, c, d = a + 1, a + nx + 2, a + nx + 1
            tri += [[a, b, c], [a, c, d]]
    return pos, np.array(tri)


def ring(n, r=0.08, wobble=0.0):
    return np.array([[(r + (wobble if i % 2 else -wobble)) * math.cos(2 * math.pi * i / n),
                      (r + (wobble if i % 2 else -wobble)) * math.sin(2 * math.pi * i / n), 0.0] for i in range(n)])


@unittest.skipUnless(HAVE_NUMPY, "numpy missing")
class Names(unittest.TestCase):
    def test_keys(self):
        self.assertEqual(ca.material_key("engpik.breeches.outfit.gltf"), "breeches")
        self.assertEqual(ca.mesh_key("engpik.fallband"), "fallband")

    def test_classify(self):
        self.assertEqual(ca.classify("flesh", "flesh", ["skin", "face"], True), ("skin", 0))
        self.assertEqual(ca.classify("breeches", "linen", ["breeches"], True), ("garment", 1))
        self.assertEqual(ca.classify("fallband", "linen", ["fallband"], True), ("garment", 6))
        self.assertEqual(ca.classify("footwear", "leather", ["shoes", "brass"], True), ("garment", 2))
        self.assertEqual(ca.classify("breast", "metal", ["plate"], True), ("cover", 5))
        self.assertEqual(ca.classify("scabbard", "leather", ["scabbard"], True), ("cover", 8))
        for key in ("caps", "hair_loose", "beard_full", "bones", "fringe_shin_l"):
            self.assertEqual(ca.classify(key, "hair", [], True)[0], None)
        self.assertEqual(ca.classify("breeches", "linen", ["breeches"], False)[0], None)


@unittest.skipUnless(HAVE_NUMPY, "numpy missing")
class Geometry(unittest.TestCase):
    def test_weld_and_loops(self):
        pos, tri = grid(4, 3)
        # Split every triangle's vertices, as the exporter does at seams.
        split = pos[tri.ravel()]
        inv, n = ca.weld(split)
        self.assertEqual(n, len(pos))
        wt = ca.welded_triangles(np.arange(len(split)).reshape(-1, 3), inv)
        loops = ca.boundary_loops(wt)
        self.assertEqual(len(loops), 1)
        chain, closed = loops[0]
        self.assertTrue(closed)
        self.assertEqual(len(chain), 2 * (4 + 3))

    def test_hole_gives_second_loop(self):
        pos, tri = grid(6, 6)
        keep = [t for t in tri if not (2 <= t.min() % 7 <= 3 and 2 <= t.min() // 7 <= 3)]
        self.assertEqual(len(ca.boundary_loops(np.array(keep))), 2)

    def test_clean_ring_has_no_teeth(self):
        rag, teeth, length = ca.loop_raggedness(ring(14), True)
        self.assertEqual((rag, teeth), (0.0, 0.0))
        self.assertAlmostEqual(length, 2 * math.pi * 0.08, delta=0.01)

    def test_sawtooth_ring(self):
        rag, teeth, length = ca.loop_raggedness(ring(24, wobble=0.008), True)
        self.assertGreater(rag, ca.LIMITS["rag_mm"])
        self.assertAlmostEqual(teeth * length, 12.0, delta=1.0)

    def test_hem_smoothing_removes_the_teeth(self):
        from acr_assets import garments

        saw = ring(24, wobble=0.008)
        self.assertGreater(ca.loop_raggedness(saw, True)[1], ca.LIMITS["teeth_per_m"])
        out = np.array(garments.smooth_outline([tuple(p) for p in saw], True))
        rag, teeth, length = ca.loop_raggedness(out, True)
        self.assertEqual((rag, teeth), (0.0, 0.0))
        self.assertAlmostEqual(np.linalg.norm(out, axis=1).mean(), 0.08, delta=0.004)  # the girth stays
        # Pinned vertices (seams shared with another object) and open ends do not move.
        pinned = garments.smooth_outline([tuple(p) for p in saw], False, pinned=[5])
        for i in (0, 5, 23):
            self.assertEqual(pinned[i], tuple(saw[i]))

    def test_hem_smoothing_keeps_corners(self):
        from acr_assets import garments

        # An L: a skirt's front edge meeting its hem, with small teeth along both runs.
        pts = [(0.0, 0.02 * i, 0.004 * (i % 2)) for i in range(10, 0, -1)] + [(0.0, 0.0, 0.0)] \
            + [(0.02 * i, 0.0, 0.004 * (i % 2)) for i in range(1, 11)]
        out = garments.smooth_outline(pts, False)
        self.assertEqual(out[10], (0.0, 0.0, 0.0))
        z = [p[2] for p in out[3:8]]  # the teeth (0 / 4 mm) settle on their mean line
        self.assertLess(max(z) - min(z), 0.001)

    def test_one_corner_is_not_ragged(self):
        square = np.array([[x, 0, 0] for x in np.linspace(0, 0.2, 6)] + [[0.2, y, 0] for y in np.linspace(0.04, 0.2, 5)]
                          + [[x, 0.2, 0] for x in np.linspace(0.16, 0, 5)] + [[0, y, 0] for y in np.linspace(0.16, 0.04, 4)])
        self.assertEqual(ca.loop_raggedness(square, True)[1], 0.0)

    def test_kind(self):
        pos, tri = grid(8, 8)
        self.assertEqual(ca.kind(pos, tri)[0], "shell")
        # A triangular lattice: rows shifted by half a step.
        h = math.sqrt(3) / 2
        lat = np.array([[(x + 0.5 * (y % 2)) * 0.02, y * 0.02 * h, 0.0] for y in range(6) for x in range(6)])
        tl = []
        for y in range(5):
            for x in range(5):
                a, b, c, d = y * 6 + x, y * 6 + x + 1, (y + 1) * 6 + x, (y + 1) * 6 + x + 1
                tl += [[a, b, c], [b, d, c]] if y % 2 == 0 else [[a, b, d], [a, d, c]]
        k, ang = ca.kind(lat, np.array(tl))
        self.assertEqual(k, "draped")
        self.assertGreater(ang, 55.0)

    def test_soup(self):
        pos, tri = grid(10, 10)
        self.assertEqual(ca.soup(pos, tri), 0.0)
        bent = pos.copy()  # a coarse but smooth quarter cylinder scores nothing
        a = pos[:, 0] / pos[:, 0].max() * math.pi / 2
        bent[:, 0], bent[:, 2] = 0.1 * np.sin(a), 0.1 * np.cos(a)
        self.assertEqual(ca.soup(bent, tri), 0.0)
        rng = np.random.default_rng(1)
        crumpled = pos + np.array([0, 0, 1.0]) * rng.normal(0, 0.012, (len(pos), 1))
        self.assertGreater(ca.soup(crumpled, tri), ca.LIMITS["soup"])

    def test_stretch(self):
        pos, tri = grid(4, 4)
        wide = pos * np.array([2.0, 1.0, 1.0])
        s, col, sl_a, sl_b = ca.stretch(pos, wide, tri)
        self.assertEqual((s, col, sl_a), (1.0, 0.0, 0.0))
        flat = pos * np.array([0.05, 1.0, 1.0])
        s, col, _a, sl_b = ca.stretch(pos, flat, tri)
        self.assertEqual((s, col, sl_b), (1.0, 1.0, 1.0))
        self.assertEqual(ca.stretch(pos, pos, tri)[:2], (0.0, 0.0))

    def test_poke(self):
        pos, tri = grid(10, 10)  # the upper layer at z = 0, facing +z
        pts = np.array([[0.1, 0.1, 0.005], [0.1, 0.1, -0.005], [0.1, 0.1, 0.2], [0.5, 0.5, 0.005]])
        nrm = np.tile([0.0, 0.0, 1.0], (4, 1))
        mask, mm, own, covered = ca.poke(pts, nrm, pos, tri, np.zeros(len(tri), int))
        self.assertEqual(mask.tolist(), [True, False, False, False])
        self.assertAlmostEqual(mm[0], 5.0, places=3)
        self.assertEqual(covered.tolist(), [True, True, False, False])
        # The far side of a tube faces the other way: not a poke.
        self.assertFalse(ca.poke(pts[:1], -nrm[:1], pos, tri, np.zeros(len(tri), int))[0][0])

    def test_inner_wall_of_a_thick_garment_is_no_poke(self):
        # A tube with a thickness round a bone along z: the inner wall faces the bone.
        ang = np.linspace(0.0, 2 * np.pi, 12, endpoint=False)
        d = np.stack([np.cos(ang), np.sin(ang), np.zeros(12)], axis=1)
        z = np.array([0.0, 0.0, 0.2])
        outer, inner = d * 0.06 + z, d * 0.057 + z
        a, b = np.array([[0.0, 0.0, 0.0]]), np.array([[0.0, 0.0, 0.4]])
        self.assertFalse(ca.inner_wall(outer, d, a, b).any())
        self.assertTrue(ca.inner_wall(inner, -d, a, b).all())
        # A panel standing off the limb (a seam flap) faces neither way.
        side = np.tile([0.0, 1.0, 0.0], (1, 1))
        self.assertFalse(ca.inner_wall(np.array([[0.08, 0.0, 0.2]]), side, a, b).any())
        self.assertEqual(ca.inner_wall(np.zeros((0, 3)), np.zeros((0, 3)), a, b).tolist(), [])

    def test_turned_hem_is_no_cover(self):
        # Two cloths 3 mm apart, each with its +x edge turned under by 4 mm (`drape.hems`);
        # the lower one shows 6 mm past the upper one's edge (a waistcoat in the coat's
        # opening). Both strips face down and out: the lower ring lies "outside" the upper
        # strip along that normal, which is no poke.
        pos, tri = grid(4, 4)
        n = len(pos)
        turned = np.array([[0.08 - 0.004, y * 0.02, -0.004] for y in range(5)])
        strip = []
        for y in range(4):
            a, b, c, d = y * 5 + 4, y * 5 + 9, n + y + 1, n + y
            strip += [[a, c, b], [a, d, c]]
        upper, utri = np.concatenate([pos, turned]), np.concatenate([tri, np.array(strip)])
        inv, _n = ca.weld(upper)
        tmask, vmask = ca.hem_rim(upper, utri, inv)
        self.assertEqual(int(tmask.sum()), 8)
        self.assertTrue(tmask[len(tri):].all())
        self.assertEqual(np.nonzero(vmask)[0].tolist(), list(range(n, n + 5)))
        self.assertLess(ca.face_normals(upper, utri)[0][tmask][:, 2].max(), -0.5)
        lower = upper + np.array([0.006, 0.0, -0.003])
        nrm = ca.vertex_normals(lower, utri)
        owner = np.zeros(len(utri), int)
        raw = ca.poke(lower, nrm, upper, utri, owner)[0]
        self.assertGreater(int(raw[vmask].sum()), 0)  # the artefact: ring against strip
        fixed = ca.poke(lower, nrm, upper, utri[~tmask], owner[~tmask])[0] & ~vmask
        self.assertEqual(int(fixed.sum()), 0)
        # A plain grid has no hem, and real poke through the cloth still counts.
        self.assertFalse(ca.hem_rim(pos, tri, ca.weld(pos)[0])[0].any())
        through = np.array([[0.04, 0.04, 0.006]])
        up = np.array([[0.0, 0.0, 1.0]])
        self.assertTrue(ca.poke(through, up, upper, utri[~tmask], owner[~tmask])[0][0])

    def test_skin_holes(self):
        pos, tri = grid(10, 10)
        cover = pos + np.array([0.0, 0.0, 0.01])
        cen = cover[tri].mean(axis=1)
        gap = tri[~((np.abs(cen[:, 0] - 0.1) < 0.015) & (np.abs(cen[:, 1] - 0.1) < 0.015))]
        nrm = np.tile([0.0, 0.0, 1.0], (len(pos), 1))
        holes, who = ca.skin_holes(pos, nrm, cover, gap, np.zeros(len(gap), int))
        self.assertEqual(int(holes.sum()), 1)
        self.assertEqual(who[holes].tolist(), [0])
        self.assertEqual(int(ca.skin_holes(pos, nrm, cover, tri, np.zeros(len(tri), int))[0].sum()), 0)

    def test_open_seam(self):
        a, ta = grid(10, 2)
        b = a + np.array([0.0, 0.05, 0.0])  # a second strip 1 cm from the first
        pos = np.concatenate([a, b])
        tri = np.concatenate([ta, ta + len(a)])
        length, gap = ca.open_seams(pos, ca.boundary_loops(tri), tri)
        self.assertAlmostEqual(length, 0.2, delta=0.04)
        self.assertTrue(0.009 < gap < 0.025, gap)
        # One narrow strip alone: its two sides are joined by edges, no seam.
        self.assertEqual(ca.open_seams(a[:22], ca.boundary_loops(ta[:20]), ta[:20])[0], 0.0)

    def test_band_is_the_inner_ring_under_the_bag(self):
        # Full breeches: a bag (r 10 cm) hanging over a knee band (r 6 cm) that ends 7 cm
        # below the knee, on a stocking (r 5.5 cm) down to the ankle.
        from types import SimpleNamespace

        def ring(r, z, n=48):
            t = np.linspace(0, 2 * np.pi, n, endpoint=False)
            return np.stack([0.1 + r * np.cos(t), np.full(n, z), r * np.sin(t)], axis=1)

        def world(y):
            w = np.eye(4)
            w[:3, 3] = (0.1, y, 0.0)
            return w

        m = SimpleNamespace(nodes=[{"name": "thigh_l"}, {"name": "shin_l"}, {"name": "foot_l"}])
        wm = [world(0.95), world(0.5), world(0.08)]
        br = np.concatenate([ring(0.10, z) for z in np.arange(0.44, 0.95, 0.01)]
                            + [ring(0.06, z) for z in np.arange(0.43, 0.471, 0.01)])
        hose = np.concatenate([ring(0.055, z) for z in np.arange(0.10, 0.55, 0.01)])
        skin = np.concatenate([ring(0.05, z) for z in np.arange(0.08, 0.95, 0.01)])
        parts = [{"role": "garment", "rank": 2, "key": "breeches", "stand": br,
                  "bones": ["thigh_l" if p[1] > 0.5 else "shin_l" for p in br]},
                 {"role": "garment", "rank": 1, "key": "hose", "stand": hose, "bones": ["shin_l"] * len(hose)},
                 {"role": "skin", "rank": 0, "key": "flesh", "stand": skin,
                  "bones": ["thigh_l" if p[1] > 0.5 else "shin_l" for p in skin]}]
        sh = ca.leg_shape(m, parts, wm)
        self.assertAlmostEqual(sh["band_cm"], 100 * 2 * np.pi * 0.06, delta=1.0)  # not the bag's 62.8
        self.assertGreater(sh["fullness"], 1.5)
        self.assertLess(sh["closure"], 1.3)

    def test_girths(self):
        self.assertAlmostEqual(ca.hull_girth([(0, 0), (1, 0), (1, 1), (0, 1), (0.5, 0.5)]), 4.0)
        # A tube that narrows from r = 0.10 to r = 0.05 along z.
        pts = np.array([[r * math.cos(a), r * math.sin(a), z] for z, r in ((k * 0.01, 0.10 - 0.005 * k) for k in range(11))
                        for a in np.linspace(0, 2 * math.pi, 24, endpoint=False)])
        prof = dict(ca.girth_profile(pts, (0, 0, 0), (0, 0, 0.1), step=0.05, slab=0.004))
        self.assertAlmostEqual(prof[0.0], 2 * math.pi * 0.10, delta=0.01)
        self.assertAlmostEqual(prof[0.1], 2 * math.pi * 0.05, delta=0.01)

    def test_sample(self):
        self.assertEqual(ca.sample([0, 1], [[0, 0, 0], [2, 0, 0]], 0.25).tolist(), [0.5, 0, 0])
        self.assertEqual(ca.sample([0, 1], [[0, 0, 0], [2, 0, 0]], 0.25, "STEP").tolist(), [0, 0, 0])
        q = ca.sample([0, 1], [[0, 0, 0, 1], [0, 0, 0, -1]], 0.5)
        self.assertAlmostEqual(abs(q[3]), 1.0)

    def test_flags(self):
        row = dict.fromkeys(ca.COLUMNS, 0)
        row.update(garment="breeches", kind="draped", flags="")
        self.assertEqual(ca.flags(row), ([], 0.0))
        row.update(poke_walk=30, rag_mm=20.0, teeth_per_m=1.0, seam_cm=40.0)
        self.assertEqual(ca.flags(row)[0], ["poke", "seam"])
        row.update(teeth_per_m=5.0)
        self.assertIn("ragged", ca.flags(row)[0])


if __name__ == "__main__":
    unittest.main()
