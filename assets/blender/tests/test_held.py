"""Held gear library and the family registry (pure Python, no Blender)."""

import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "lib"))

from acr_assets import garments as gm  # noqa: E402
from acr_assets import held, registry  # noqa: E402


class Items(unittest.TestCase):
    def setUp(self):
        self.items = {n: held.build(n) for n in held.ITEMS}

    def test_every_item_has_a_budget(self):
        self.assertEqual(set(held.ITEMS), set(held.BUDGET))

    def test_triangle_budget(self):
        for n, it in self.items.items():
            self.assertGreater(it.triangles(), 0, n)
            self.assertLessEqual(it.triangles(), held.BUDGET[n], n)

    def test_faces_are_valid(self):
        for n, it in self.items.items():
            self.assertEqual(len(it.faces), len(it.parts), n)
            for f in it.faces:
                self.assertGreaterEqual(len(f), 3, n)
                self.assertEqual(len(set(f)), len(f), (n, f))
                self.assertTrue(all(0 <= i < len(it.verts) for i in f), n)
            for p in set(it.parts):
                self.assertIn(it.keys[p], registry.LAYER_OF, (n, p))

    def test_tip_present_where_it_applies(self):
        for n, it in self.items.items():
            if n in held.TIP:
                self.assertIn("socket.tip", it.sockets, n)
                # The tip is the farthest point along the item's axis (+Z) or, for the
                # club and the tomahawk, the striking face forward (-Y).
                lo, hi = it.bounds()
                tip = it.sockets["socket.tip"]
                if n in ("war_club", "tomahawk"):
                    self.assertLess(tip[1], -0.1, n)
                else:
                    self.assertGreater(tip[2], hi[2] - 0.02, n)
            else:
                self.assertNotIn("socket.tip", it.sockets, n)
            if n in held.MUZZLE:
                self.assertIn("socket.muzzle", it.sockets, n)

    def test_named_sockets(self):
        for n, names in held.SOCKETS.items():
            self.assertTrue(names <= set(self.items[n].sockets), n)

    def test_sizes_are_period_plausible(self):
        def length(n):
            lo, hi = self.items[n].bounds()
            return hi[2] - lo[2]

        self.assertGreater(length("colour"), 2.8)  # 9 ft 10 in pike with its finial
        self.assertTrue(1.9 < length("halberd") < 2.4)
        self.assertTrue(0.85 < length("smallsword") < 1.05)
        self.assertTrue(0.7 < length("hanger") < 0.85)
        self.assertTrue(1.3 < length("longbow") < 1.6)
        self.assertTrue(0.8 < length("macuahuitl") < 1.0)
        lo, hi = self.items["drum"].bounds()
        self.assertTrue(0.35 < hi[2] - lo[2] < 0.42 and 0.40 < hi[0] - lo[0] < 0.48)

    def test_colour_cloth_groups(self):
        it = self.items["colour"]
        cloth, pin = set(it.groups["cloth"]), set(it.groups["pin"])
        self.assertTrue(pin and pin < cloth)
        # The hoist hangs on the staff.
        self.assertTrue(all(abs(it.verts[i][0]) < 0.05 for i in pin))

    def test_material_slots(self):
        keys, slots = self.items["pistol"].material_keys()
        self.assertEqual(set(keys), {"wood", "metal", "brass"})
        self.assertEqual(len(slots), len(self.items["pistol"].faces))


class _Poly:
    def __init__(self, s):
        self.material_index = s


class _Slots(list):
    """`Mesh.materials` as Blender has it: `clear()` also drops the faces' `material_index`."""

    def __init__(self, mesh):
        super().__init__()
        self.mesh = mesh

    def clear(self):
        super().clear()
        for p in self.mesh.polygons:
            p.material_index = 0


class _Mesh:
    def __init__(self, face_slot):
        self.polygons = [_Poly(s) for s in face_slot]
        self.materials = _Slots(self)


class Materials(unittest.TestCase):
    def test_multi_material_items_keep_their_slots(self):
        # The drum's hoops, vellum heads and cords must not all take the shell's facings
        # colour when the materials are (re)assigned (#287).
        multi = 0
        for n in held.ITEMS:
            it = held.build(n)
            keys, face_slot = it.material_keys()
            if len(keys) < 2:
                continue
            multi += 1
            me = _Mesh(face_slot)
            mats = [f"{n}.{k}" for k in keys]
            held.set_materials(me, mats)
            self.assertEqual(list(me.materials), mats, n)
            self.assertEqual([p.material_index for p in me.polygons], face_slot, n)
            self.assertEqual(len({p.material_index for p in me.polygons}), len(keys), n)
            # Re-assigning (the outfit pass, export twins) keeps them too.
            held.set_materials(me, [m + ".gltf" for m in mats])
            self.assertEqual([p.material_index for p in me.polygons], face_slot, n)
        self.assertIn("vellum", held.build("drum").material_keys()[0])
        self.assertGreater(multi, 1)


class Registry(unittest.TestCase):
    def test_shared_tables_are_the_registry(self):
        self.assertIs(registry.SPACING, registry.SPACING)
        self.assertIn("vellum", registry.LAYER_OF)  # registered by held.py

    def test_material_and_painter(self):
        registry.painter("test_paint", like="wool")
        registry.material("test_key", "wool", painter="test_paint", fallback=(0.1, 0.2, 0.3))
        self.assertEqual(registry.PAINTER["test_key"], "test_paint")
        self.assertEqual(registry.KIND["test_paint"], "wool")
        registry.material("test_key", "wool")  # the same again is fine
        with self.assertRaises(ValueError):
            registry.material("test_key", "linen")
        with self.assertRaises(ValueError):
            registry.material("test_other", "plush")
        with self.assertRaises(ValueError):
            registry.material("test_other", "wool", painter="nobody")
        with self.assertRaises(ValueError):
            registry.painter("wool", fn=lambda *a: None)

    def test_garment(self):
        def apron(m):
            return gm.shirt(m)

        registry.garment("test_apron", apron, dict(gm.CLOTH["shirt"]), spacing=2.0)
        uni = gm.uniform(gm.MEASURE, ("breeches", "test_apron"))
        self.assertEqual(set(uni), {"breeches", "test_apron"})
        self.assertEqual(registry.SPACING["test_apron"], 2.0)
        with self.assertRaises(KeyError):
            gm.uniform(gm.MEASURE, ("nothing",))
        with self.assertRaises(KeyError):
            registry.garment("test_bad", apron, {"mass": 1.0})
        # A one-off pattern overrides the registered draft.
        one = gm.uniform(gm.MEASURE, ("coat",), patterns={"coat": apron})
        self.assertEqual(set(one["coat"][0]), set(gm.shirt()[0]))

    def test_hats_and_gear(self):
        registry.hat("test_cap", lambda name: ("cap", name))
        self.assertEqual(registry.build_hat("test_cap", "x.cap"), ("cap", "x.cap"))
        registry.gear("test_pouch", lambda ch, name, s: f"{name}.pouch")
        self.assertEqual(registry.build_gear(["test_pouch"], None, "u", {}), {"test_pouch": "u.pouch"})
        with self.assertRaises(KeyError):
            registry.build_gear(["nothing"], None, "u", {})
        with self.assertRaises(ValueError):
            registry.hat("test_cap", lambda name: None)


if __name__ == "__main__":
    unittest.main()
