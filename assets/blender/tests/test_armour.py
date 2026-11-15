"""Plate armour (`acr_assets.armour`, #522): the pure shape functions of the corslet."""

import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "lib"))

from acr_assets import armour, registry  # noqa: E402


class Corslet(unittest.TestCase):
    def test_edges(self):
        c = armour.CORSLET
        # The neck is scooped below the strap points; the arm openings end under the armpit.
        self.assertLess(armour.breast_top(0), armour.breast_top(36) - 0.04)
        self.assertLess(armour.breast_top(80), 1.30)
        self.assertLess(armour.back_top(80), 1.30)
        for a in range(-93, 94):
            self.assertAlmostEqual(armour.breast_top(a), armour.breast_top(-a))
            self.assertGreater(armour.breast_top(a) - armour.breast_bottom(a), 0.20)
            # No steps along the edge (1 degree is about 2.5 mm of arc).
            self.assertLess(abs(armour.breast_top(a + 1) - armour.breast_top(a)), 0.012)
        self.assertAlmostEqual(armour.breast_bottom(0), c["waist"] - c["point"])
        self.assertAlmostEqual(armour.breast_bottom(60), c["waist"])

    def test_ridge(self):
        # A crease at the centre, strongest at the waist, gone at the sides.
        self.assertGreater(armour.ridge(0, 1.10), armour.ridge(0, 1.36))
        self.assertGreater(armour.ridge(0, 1.10) - armour.ridge(6, 1.10), 0.002)
        self.assertLess(armour.ridge(80, 1.20), 0.001)
        self.assertGreaterEqual(min(armour.ridge(a, 1.0 + 0.01 * k) for a in range(0, 94) for k in range(45)), 0.0)

    def test_lames(self):
        c = armour.CORSLET
        w = armour.lame_weights(c["lames"])
        self.assertEqual(len(w), c["lames"])
        self.assertTrue(all(0.0 < a < b < 1.0 for a, b in zip(w, w[1:])))
        self.assertLess(w[0], 0.15)   # the top lame stays at the flange
        self.assertGreater(w[-1], 0.80)  # the lowest goes with the thigh
        # Neighbours differ little: at a 35 degree thigh swing two lames 5 cm apart, 0.3 m
        # below the hip, part by less than their overlap.
        self.assertLess(max(b - a for a, b in zip(w, w[1:])) * 0.3 * 0.57 * 0.25, c["lame"][1])
        rows = armour.lame_rows(c["lames"], 1.05, *c["lame"])
        for (t0, b0), (t1, b1) in zip(rows, rows[1:]):
            self.assertAlmostEqual(t1 - b0, c["lame"][1])
            self.assertLess(b1, b0)
        self.assertGreater(rows[-1][1], 0.66)  # ends above the knee (thigh 0.93 to 0.50)

    def test_helmets(self):
        self.assertEqual(sorted(armour.HELMETS), ["cabasset", "morion", "pot", "zischagge"])
        for kind, k in armour.HELMETS.items():
            # The rim clears a head 0.155 m wide and 0.19 m long; the apex is on the axis.
            x, _y, z = armour.skull_point(k, 0.0, 0.0)
            self.assertGreater(x, 0.105, kind)
            self.assertAlmostEqual(z, k["rim"])
            self.assertGreater(-armour.skull_point(k, -1.5707963, 0.0)[1], 0.12, kind)
            x, y, z = armour.skull_point(k, 1.0, 1.0)
            self.assertAlmostEqual(x, 0.0)
            self.assertAlmostEqual(y, 0.0)
            self.assertAlmostEqual(z, k["rim"] + k["h"])
            if k.get("brim"):
                for sn in (-1.0, 0.0, 1.0):
                    w, drop, rise = k["brim"](sn, (1 - sn * sn) ** 0.5)
                    self.assertGreater(w, 0.02, kind)
                    self.assertGreaterEqual(drop, 0.0)
                    self.assertGreaterEqual(rise, 0.0)
        # The morion's brim sweeps up to points front and back, the cabasset is the tallest.
        self.assertGreater(armour.HELMETS["morion"]["brim"](-1.0, 0.0)[2], 0.08)
        self.assertEqual(armour.HELMETS["morion"]["brim"](0.0, 1.0)[2], 0.0)
        self.assertEqual(max(armour.HELMETS, key=lambda n: armour.HELMETS[n]["h"]), "cabasset")

    def test_registered(self):
        self.assertEqual(registry.LAYER_OF["plate"], "metal")
        self.assertEqual(registry.PAINTER["plate"], "plate")
        self.assertEqual(registry.METALLIC["plate"], 1.0)
        self.assertIs(registry.GEAR["corslet"], armour.corslet)


if __name__ == "__main__":
    unittest.main()
