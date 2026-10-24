"""hdbody: physically scaled skin micro-normal, T-zone roughness, UV seam measures, finger
chain weights (numpy only)."""

import os
import sys
import unittest

import numpy as np

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "lib"))

from acr_assets import hdbody  # noqa: E402


class Skin(unittest.TestCase):
    def test_micro_normal_is_subtle(self):
        tex = hdbody.texel_mm(4096)
        self.assertTrue(0.2 < tex < 0.6, tex)  # a 4k body atlas: about a third of a mm
        n = hdbody.height_to_normal(hdbody.micro_height(256, tex), tex)
        mean, p99 = hdbody.tilt_deg(n)
        self.assertGreater(mean, 0.3)
        self.assertLess(mean, 4.0)  # the prototype's pores tilted tens of degrees
        self.assertLess(p99, 12.0)

    def test_micro_height_tiles(self):
        h = hdbody.micro_height(128, 0.35)
        self.assertLess(abs(h[0] - h[-1]).mean(), 3 * abs(h[0] - h[1]).mean())

    def test_t_zone_oilier(self):
        eye = np.array([0.0, -0.08, 1.62])
        pts = np.array([[0.0, -0.10, 1.60 - 0.03],  # nose
                        [0.0, -0.09, 1.665],        # forehead centre
                        [0.06, -0.06, 1.56],        # cheek
                        [0.2, 0.0, 1.2]])           # body
        r = hdbody.zone_roughness(pts, eye, 0.064)
        self.assertLess(r[0], r[2])
        self.assertLess(r[1], r[2])
        self.assertAlmostEqual(r[3], 0.58)
        self.assertTrue((hdbody.roughness(r, np.zeros(4)) == r).all())


def _two_islands():
    # two quads side by side in 3D sharing the edge (1, 2); split in UV
    tri_v = np.array([[0, 1, 2], [0, 2, 3], [1, 4, 5], [1, 5, 2]])
    tri_uv = np.array([[[0.05, 0.05], [0.4, 0.05], [0.4, 0.4]], [[0.05, 0.05], [0.4, 0.4], [0.05, 0.4]],
                       [[0.6, 0.05], [0.95, 0.05], [0.95, 0.4]], [[0.6, 0.05], [0.95, 0.4], [0.6, 0.4]]])
    return tri_uv, tri_v


class Seams(unittest.TestCase):
    def test_islands_and_seam_vertices(self):
        tri_uv, tri_v = _two_islands()
        lab, k = hdbody.island_labels(tri_uv, tri_v)
        self.assertEqual(k, 2)
        self.assertEqual(lab[0], lab[1])
        self.assertNotEqual(lab[0], lab[2])
        self.assertIn(1, hdbody.seam_vertices(tri_uv, tri_v))
        self.assertIn(2, hdbody.seam_vertices(tri_uv, tri_v))

    def test_integrity_catches_a_smear(self):
        tri_uv, tri_v = _two_islands()
        lab, _k = hdbody.island_labels(tri_uv, tri_v)
        r = hdbody.island_raster(tri_uv, lab, 128)
        self.assertEqual(hdbody.seam_integrity(r, tri_uv), 1.0)
        smear = np.array([[[0.1, 0.1], [0.9, 0.1], [0.5, 0.35]]])  # spans both islands
        self.assertEqual(hdbody.seam_integrity(r, smear), 0.0)
        self.assertEqual(hdbody.uv_flipped(tri_uv), 0)
        self.assertEqual(hdbody.uv_flipped(np.concatenate([tri_uv, tri_uv[:1, ::-1]])), 1)


class Fingers(unittest.TestCase):
    def test_chain_weights_crisp_at_joints(self):
        joints, band = [0.0, 0.045, 0.073], [0.004, 0.004, 0.003]
        w = hdbody.chain_weights([-0.02, 0.02, 0.045, 0.06, 0.09], joints, band)
        self.assertTrue(np.allclose(w.sum(1), 1))
        self.assertAlmostEqual(w[0, 0], 1.0)  # palm: hand bone
        self.assertAlmostEqual(w[1, 1], 1.0)  # mid proximal phalanx: no blend at all
        self.assertAlmostEqual(w[2, 1], 0.5)  # on the PIP joint: half and half
        self.assertAlmostEqual(w[4, 3], 1.0)  # tip


class Lods(unittest.TestCase):
    def test_limit_influences(self):
        out = hdbody.limit_influences([(3, 0.1), (1, 0.4), (7, 0.05), (2, 0.3), (5, 0.2), (9, 0.0)])
        self.assertEqual([k for k, _w in out], [1, 2, 5, 3])
        self.assertAlmostEqual(sum(w for _k, w in out), 1.0)
        self.assertEqual(hdbody.limit_influences([]), [])

    def test_bary_uv_flags_a_seam(self):
        tri = np.array([[0, 0, 0], [1, 0, 0], [0, 1, 0]], float)
        uv_a = np.array([[0.1, 0.1], [0.2, 0.1], [0.1, 0.2]])
        uv_b = uv_a + [0.5, 0.0]  # the same triangle in another island
        m = np.array([0.5, 0.5, 0.01])  # projected onto the plane
        self.assertTrue(np.allclose(hdbody.bary_uv(m, tri, uv_a), [0.15, 0.15]))
        jump = np.linalg.norm(hdbody.bary_uv(m, tri, uv_a) - hdbody.bary_uv(m, tri, uv_b))
        self.assertGreater(jump, hdbody.SEAM_JUMP)


class Eyes(unittest.TestCase):
    def test_eye_albedo_has_pupil_iris_and_sclera(self):
        a = hdbody.eye_albedo(128)
        self.assertEqual(a.shape, (128, 128, 3))
        c = a[64, 64]
        self.assertLess(c.max(), 0.05)  # pupil
        iris = a[64, 64 + int(0.18 * 128)]
        self.assertGreater(iris[0], iris[2])  # brown: more red than blue
        self.assertLess(iris.max(), 0.6)
        self.assertGreater(a[64, 64 + int(0.40 * 128)].min(), 0.6)  # sclera


if __name__ == "__main__":
    unittest.main()
