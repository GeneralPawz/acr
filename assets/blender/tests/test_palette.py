"""`acr_assets.palette`: colour conversion, CIEDE2000, dominant colour, region names."""

import os
import sys
import unittest

import numpy as np

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "lib"))

from acr_assets import palette  # noqa: E402


class Palette(unittest.TestCase):
    def test_lab_round_trip(self):
        rgb = np.array([[0.58, 0.12, 0.08], [0.16, 0.22, 0.42], [1.0, 1.0, 1.0], [0.0, 0.0, 0.0]])
        lab = palette.srgb_to_lab(rgb)
        self.assertAlmostEqual(lab[2, 0], 100.0, places=2)
        self.assertAlmostEqual(lab[3, 0], 0.0, places=2)
        np.testing.assert_allclose(palette.lab_to_srgb(lab), rgb, atol=1e-4)

    def test_ciede2000_reference_pairs(self):
        # Sharma, Wu, Dalal (2005), pairs 1 and 17 (kL = 1).
        self.assertAlmostEqual(float(palette.ciede2000([50, 2.6772, -79.7751], [50, 0, -82.7485], kl=1)), 2.0425, 3)
        self.assertAlmostEqual(float(palette.ciede2000([22.7233, 20.0904, -46.694], [23.0331, 14.973, -42.5619], kl=1)), 2.0373, 3)
        self.assertEqual(float(palette.ciede2000([40, 10, 10], [40, 10, 10])), 0.0)

    def test_dominant_ignores_a_minority(self):
        red = palette.srgb_to_lab([0.6, 0.1, 0.1])
        black = palette.srgb_to_lab([0.0, 0.0, 0.0])
        samples = np.array([red] * 70 + [black] * 30)
        np.testing.assert_allclose(palette.dominant(samples), red, atol=1e-6)
        self.assertIsNone(palette.dominant(np.zeros((0, 3))))

    def test_region_names(self):
        self.assertEqual(palette.region("engm18.belts.outfit.002.gltf", "engm18"), "belts")
        self.assertEqual(palette.region("dellut.wood.13", "dellut"), "wood")
        self.assertEqual(palette.region("M_Body", "siukvog"), "M_Body")

    def test_erode(self):
        m = np.zeros((5, 5), bool)
        m[1:4, 1:4] = True
        e = palette.erode(m)
        self.assertEqual(int(e.sum()), 1)
        self.assertTrue(e[2, 2])


if __name__ == "__main__":
    unittest.main()
