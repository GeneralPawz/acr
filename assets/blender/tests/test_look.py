"""Shared look fixes (#287): clean-shaven faces, the gaiter seam pairing, the face mask's
extent and the outfit atlas' per-object labels and colours. The numpy parts run where numpy
is installed (Blender's Python: `blender/5.2/python/bin/python.exe -m unittest ...`) and are
skipped elsewhere."""

import os
import sys
import types
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "lib"))

from acr_assets import faces  # noqa: E402
from acr_assets import garments as gm  # noqa: E402

try:
    import numpy as np
except ImportError:  # pragma: no cover
    np = None


class CleanShaven(unittest.TestCase):
    def test_no_beard_channel(self):
        for x, y, z in ((0.03, -0.05, -0.08), (0.0, -0.06, -0.10), (0.05, -0.02, -0.05), (0.0, -0.07, 0.04)):
            full = faces.zones(x, y, z)
            bare = faces.zones(x, y, z, beard=False)
            self.assertEqual(bare[0], 0.0)
            self.assertEqual(bare[1:], full[1:])
        self.assertGreater(faces.zones(0.03, -0.05, -0.08)[0], 0.3)

    def test_hairline_lifted_without_scalp(self):
        self.assertEqual(faces.hairline(), faces.HAIRLINE)
        lifted = faces.hairline(scalp=False)
        self.assertEqual([a for a, _ in lifted], [a for a, _ in faces.HAIRLINE])
        self.assertTrue(all(p > 90.0 for _, p in lifted))


class GaiterSeam(unittest.TestCase):
    def test_fractions_follow_length(self):
        f = gm.seam_fractions([(0.0, 0.0), (0.0, 1.0), (0.0, 4.0)])
        self.assertEqual(f, [0.0, 0.25, 1.0])

    def test_corners_pair_up(self):
        """The gaiter's back edge (straight) and outer edge (slanted) meet their corners at
        the same fraction of their length, so the sewn pairs do not skew."""
        panels, seams = gm.gaiters()
        g = panels["gaiter"]
        pts = g.outline

        def edge(name, rev=False):
            i, j = g.edges[name]
            out, k = [pts[i]], i
            while k != j:
                k = (k + 1) % len(pts)
                out.append(pts[k])
            return out[::-1] if rev else out

        start, outer = edge("start", True), edge("outer")
        fs, fo = gm.seam_fractions(start), gm.seam_fractions(outer)
        # The knee-high corner (the calf, 55 % up) of each side.
        self.assertAlmostEqual(fs[1], fo[1], delta=0.05)
        # A drop-in for the stockings (the pattern without its garter) keeps the same seam.
        no_garter = {k: p for k, p in panels.items() if k != "garter"}
        self.assertEqual(set(no_garter), {"gaiter"})
        self.assertEqual(len(seams), 1)

    def test_buttoned_seam_closes(self):
        """The gaiter's buttoned seam is welded shut beyond the drape's usual limit (the
        garterless stockings left its top pairs 5..6 cm apart: skin showed at the shin)."""
        _panels, seams = gm.gaiters()
        self.assertGreaterEqual(seams[0].close, 0.065)
        breeches = gm.breeches()[1]
        self.assertTrue(all(s.close is None for s in breeches))


@unittest.skipIf(np is None, "numpy")
class FaceMask(unittest.TestCase):
    def setUp(self):
        from acr_assets import masks

        self.masks = masks
        self.L = {"head_top": 1.75}

    def test_face_half_stays_on_the_head(self):
        top = self.L["head_top"]
        p = np.array([
            [0.03, -0.1, top - 0.10],   # the cheek
            [0.03, -0.1, top - 0.26],   # the throat / top of the chest
            [0.18, -0.1, top - 0.20],   # the shoulder at chin height
            [0.25, -0.1, top - 0.30],   # the upper arm
        ], np.float32)
        n = np.tile(np.array([[0.0, -1.0, 0.0]], np.float32), (len(p), 1))
        w = self.masks.paint_face_half(p, n, self.L, 1, side=1.0)
        self.assertGreater(w[0], 0.9)
        self.assertTrue(np.all(w[1:] < 1e-3), w)


@unittest.skipIf(np is None, "numpy")
class AtlasLabels(unittest.TestCase):
    def setUp(self):
        for m in ("bpy", "bmesh", "mathutils"):
            sys.modules.setdefault(m, types.ModuleType(m))
        from acr_assets import outfit

        self.outfit = outfit

    def test_raster_and_grow(self):
        lab = self.outfit.raster_labels([(0, [(0.0, 0.0), (0.5, 0.0), (0.0, 0.5)]),
                                         (1, [(1.0, 1.0), (0.6, 1.0), (1.0, 0.6)])], 32)
        self.assertEqual(lab[2, 2], 0)
        self.assertEqual(lab[30, 30], 1)
        self.assertEqual(lab[16, 16], -1)
        want = np.zeros((32, 32), bool)
        want[0:20, 0:20] = True
        grown = self.outfit.grow_labels(lab, want)
        self.assertTrue((grown[want] >= 0).all())
        self.assertEqual(grown[2, 14], 0)

    def test_colour_per_key(self):
        """Two keys on one painter keep their own colours (the cell is split by owner)."""
        o1, o2 = types.SimpleNamespace(name="a.tunic"), types.SimpleNamespace(name="a.sash")
        colours = {"a.tunic": (0.2, 0.3, 0.6), "a.sash": (0.7, 0.1, 0.1)}
        owner = np.array([0, 0, 1, 1, -1])
        m = np.array([True, True, True, False, True])
        parts = dict((rgb, sel) for sel, rgb in self.outfit.split_by_colour([o1, o2], colours, owner, [o1, o2], m))
        self.assertEqual(set(parts), set(colours.values()))
        self.assertEqual(parts[(0.7, 0.1, 0.1)].tolist(), [False, False, True, False, False])
        self.assertEqual(parts[(0.2, 0.3, 0.6)].tolist(), [True, True, False, False, True])
        one = self.outfit.split_by_colour([o1], colours, None, [o1], m)
        self.assertEqual(len(one), 1)


if __name__ == "__main__":
    unittest.main()
