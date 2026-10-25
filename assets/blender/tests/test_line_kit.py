"""line_kit: the worn kit's tables and what it replaces (no bpy)."""

import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "lib"))

from acr_assets import line_kit  # noqa: E402


class Kit(unittest.TestCase):
    def test_every_nation_kit_is_known_and_built(self):
        for nation, kit in [*line_kit.KITS.items(), ("17th", line_kit.KIT17)]:
            self.assertEqual(line_kit.check(kit), kit, nation)
            for piece in kit:
                if piece not in line_kit.STRAP_PIECES:
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

    def test_every_line_unit_wears_one_canteen_on_its_own_strap(self):
        for nation, kit in line_kit.KITS.items():
            self.assertEqual(len([p for p in kit if p in line_kit.CANTEENS]), 1, nation)
        self.assertTrue(all(b in line_kit.BONES.values() or b.startswith("thigh") for b in line_kit.CANTEEN_BONES))

    def test_musketeer_kit_replaces_ribbons_charges_and_scabbard(self):
        gear = ("belt_l", "belt_r", "falling_band", "pike_apostles", "pike_match", "off_scabbard")
        self.assertEqual(line_kit.covered(gear, line_kit.KIT17), ("falling_band", "pike_match"))

    def test_twelve_apostles_do_not_touch(self):
        # Hung on a strap spanning 0.29 m in X: neighbours on one run must be more than a
        # bottle's width apart in X.
        self.assertEqual(len(line_kit.APOSTLES), 12)
        for run in ("front", "back"):
            ts = sorted(t for r, t in line_kit.APOSTLES if r == run)
            self.assertGreater(min(b - a for a, b in zip(ts, ts[1:])) * 0.29, line_kit.APOSTLE_W, run)
            self.assertLess(max(ts), 0.97 if run == "back" else 0.85, run)  # clear of the bullet bag

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
