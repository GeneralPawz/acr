"""line_kit: the worn kit's tables and what it replaces (no bpy)."""

import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "lib"))

from acr_assets import line_kit  # noqa: E402


class Kit(unittest.TestCase):
    def test_every_nation_kit_is_known_and_built(self):
        for nation, kit in line_kit.KITS.items():
            self.assertEqual(line_kit.check(kit), kit, nation)
            for piece in kit:
                if piece != "crossbelts":
                    self.assertIn(piece, line_kit.TRIS, piece)
                    self.assertIn(piece, line_kit.BONES, piece)

    def test_unknown_piece_raises(self):
        with self.assertRaises(KeyError):
            line_kit.check(("crossbelts", "sporran"))

    def test_covered_drops_ribbons_and_boxes(self):
        gear = ("crossbelts", "cartridge", "knapsack")
        self.assertEqual(line_kit.covered(gear, line_kit.KITS["eng"]), ())
        self.assertEqual(line_kit.covered(gear, ("cartridge_box",)), ("crossbelts", "knapsack"))
        self.assertEqual(line_kit.covered(gear, None), gear)

    def test_material_keys_map_hd_sources(self):
        self.assertEqual(line_kit.material_keys(["hd.src.leather", "hd.src.brass", "hd.src.buff"]),
                         ["cartridge", "brass", "belts"])

    def test_rigid_items_ride_trunk_bones(self):
        from acr_assets import kit_hd

        for piece, bone in line_kit.BONES.items():
            self.assertFalse(kit_hd.is_arm(bone), piece)

    def test_inner_faces_are_the_strap_bottom(self):
        from acr_assets import kit_hd

        across, rows = 3, 4
        faces = kit_hd.strap_faces(rows, across)
        inner = line_kit.inner_faces(faces, across)
        self.assertEqual(len(inner), (rows - 1) * (across - 1))
        for i in inner:
            self.assertTrue(all(v % (2 * across) < across for v in faces[i]))


if __name__ == "__main__":
    unittest.main()
