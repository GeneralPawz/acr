"""The checked-in MakeHuman head data (#283 pass 4): consistent, small, every face morph."""
import json
import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "lib"))
from acr_assets import faces  # noqa: E402

DATA = os.path.join(os.path.dirname(__file__), "..", "data", "makehuman", "head.json")


class MakeHumanHead(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        with open(DATA) as fh:
            cls.d = json.load(fh)

    def test_small(self):
        self.assertLess(os.path.getsize(DATA), 400_000)

    def test_licence_recorded(self):
        self.assertEqual(self.d["licence"], "CC0 1.0")
        lic = open(os.path.join(os.path.dirname(__file__), "..", "..", "LICENSES.md")).read()
        self.assertIn(self.d["commit"], lic)

    def test_mesh(self):
        n = len(self.d["verts"]) // 3
        self.assertEqual(len(self.d["verts"]), 3 * n)
        self.assertEqual(len(self.d["eye"]), n)
        self.assertTrue(all(0 <= i < n for f in self.d["faces"] for i in f))
        zs = self.d["verts"][2::3]
        self.assertTrue(0.22 < (max(zs) - min(zs)) * 1e-4 < 0.32)  # head and upper neck, metres

    def test_every_face_morph(self):
        n = len(self.d["verts"]) // 3
        self.assertEqual(set(self.d["targets"]), set(faces.FACE_MORPHS))
        for t in self.d["targets"].values():
            self.assertEqual(len(t["d"]), 3 * len(t["i"]))
            self.assertTrue(t["i"] and all(0 <= i < n for i in t["i"]))


if __name__ == "__main__":
    unittest.main()


BODY = os.path.join(os.path.dirname(__file__), "..", "data", "makehuman", "body.json")


class MakeHumanBody(unittest.TestCase):
    """The checked-in MakeHuman body data (#287): the mesh below the chin and the joints
    `mbody.repose` maps onto skeleton H."""

    @classmethod
    def setUpClass(cls):
        with open(BODY) as fh:
            cls.d = json.load(fh)

    def test_small(self):
        self.assertLess(os.path.getsize(BODY), 400_000)

    def test_licence_recorded(self):
        self.assertEqual(self.d["licence"], "CC0 1.0")
        lic = open(os.path.join(os.path.dirname(__file__), "..", "..", "LICENSES.md")).read()
        self.assertIn("body.json", lic)
        self.assertIn(self.d["commit"], lic)

    def test_mesh_and_joints(self):
        n = len(self.d["verts"]) // 3
        self.assertTrue(all(0 <= i < n for f in self.d["faces"] for i in f))
        zs = self.d["verts"][2::3]
        self.assertTrue(1.4 < (max(zs) - min(zs)) * 1e-4 < 1.7)  # soles to the chin, metres
        for side in "lr":
            for j in ("shoulder", "elbow", "hand", "finger-3-1", "finger-3-4", "upper-leg", "knee", "ankle",
                      "foot-1", "foot-2"):
                self.assertIn(f"{side}-{j}", self.d["joints"])
        for j in ("pelvis", "neck", "head"):
            self.assertIn(j, self.d["joints"])
        # Mirror-symmetric (the decimation keeps the symmetry).
        xs = sorted(round(x / 10) for x in self.d["verts"][0::3])
        self.assertEqual(xs, sorted(-x for x in xs))
