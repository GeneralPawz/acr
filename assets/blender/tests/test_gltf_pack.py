"""gltf_pack: quantised attributes, reduced clips, lossless images (#199)."""

import math
import os
import struct
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "lib"))

from acr_assets import gltf_pack as g  # noqa: E402


def _glb():
    """One triangle with float weights, `_GORE`, `_PART`, UVs and a clip with a sampled
    rotation (a 90° turn over 5 frames), a constant translation equal to the node's and a
    constant scale that differs from it."""
    views, blob = [], b""

    def add(fmt, values):
        nonlocal blob
        data = struct.pack("<" + fmt * len(values), *[x for e in values for x in e])
        views.append({"buffer": 0, "byteOffset": len(blob), "byteLength": len(data)})
        blob += data + b"\0" * (-len(data) % 4)
        return len(views) - 1

    pos = add("fff", [(0, 0, 0), (1, 0, 0), (0, 1, 0)])
    wts = add("ffff", [(0.5, 0.3, 0.2, 0.0), (1, 0, 0, 0), (0.333, 0.333, 0.334, 0)])
    gore = add("ffff", [(0.1, 0.2, 0.3, 1.0)] * 3)
    part = add("f", [(3.0,), (5.0,), (7.0,)])
    uv = add("ff", [(0, 0), (1, 0), (0.25, 1)])
    times = [(i / 4,) for i in range(5)]
    tin = add("f", times)
    rot = add("ffff", [(0, 0, math.sin(a / 2), math.cos(a / 2)) for a in [math.pi / 8 * i for i in range(5)]])
    tra = add("fff", [(0, 1, 0)] * 5)
    sca = add("fff", [(2, 2, 2)] * 5)
    acc = [
        {"bufferView": pos, "componentType": g.FLOAT, "count": 3, "type": "VEC3", "min": [0, 0, 0], "max": [1, 1, 0]},
        {"bufferView": wts, "componentType": g.FLOAT, "count": 3, "type": "VEC4"},
        {"bufferView": gore, "componentType": g.FLOAT, "count": 3, "type": "VEC4"},
        {"bufferView": part, "componentType": g.FLOAT, "count": 3, "type": "SCALAR"},
        {"bufferView": uv, "componentType": g.FLOAT, "count": 3, "type": "VEC2"},
        {"bufferView": tin, "componentType": g.FLOAT, "count": 5, "type": "SCALAR", "min": [0], "max": [1]},
        {"bufferView": rot, "componentType": g.FLOAT, "count": 5, "type": "VEC4"},
        {"bufferView": tra, "componentType": g.FLOAT, "count": 5, "type": "VEC3"},
        {"bufferView": sca, "componentType": g.FLOAT, "count": 5, "type": "VEC3"},
    ]
    doc = {
        "asset": {"version": "2.0"},
        "nodes": [{"name": "arm", "translation": [0, 1, 0]}],
        "meshes": [{"primitives": [{"attributes": {"POSITION": 0, "WEIGHTS_0": 1, "_GORE": 2, "_PART": 3,
                                                   "TEXCOORD_0": 4}}]}],
        "animations": [{"name": "WAVE", "samplers": [{"input": 5, "output": o} for o in (6, 7, 8)],
                        "channels": [{"sampler": i, "target": {"node": 0, "path": p}}
                                     for i, p in enumerate(("rotation", "translation", "scale"))]}],
        "accessors": acc, "bufferViews": views, "buffers": [{"byteLength": len(blob)}],
    }
    return g.write_glb(doc, blob)


class Pack(unittest.TestCase):
    def setUp(self):
        self.out, self.stats = g.pack_bytes(_glb())
        self.doc, blob = g.read_glb(self.out)
        self.views = [blob[v["byteOffset"]:v["byteOffset"] + v["byteLength"]] for v in self.doc["bufferViews"]]
        self.attr = self.doc["meshes"][0]["primitives"][0]["attributes"]

    def read(self, ai):
        return g.read_accessor(self.doc, self.views, self.doc["accessors"][ai])

    def test_weights_are_bytes_summing_to_one(self):
        a = self.doc["accessors"][self.attr["WEIGHTS_0"]]
        self.assertEqual((a["componentType"], a.get("normalized")), (g.UBYTE, True))
        for w in self.read(self.attr["WEIGHTS_0"]):
            self.assertAlmostEqual(sum(w), 1.0, places=6)
        self.assertEqual(g.weights_u8([(0.333, 0.333, 0.334, 0)]), [(85, 85, 85, 0)])

    def test_gore_bytes_part_float(self):
        a = self.doc["accessors"][self.attr["_GORE"]]
        self.assertEqual((a["componentType"], a.get("normalized")), (g.UBYTE, True))
        self.assertTrue(all(abs(x - y) <= 0.5 / 255 for x, y in zip(self.read(self.attr["_GORE"])[0], (0.1, 0.2, 0.3, 1))))
        self.assertEqual(self.doc["accessors"][self.attr["_PART"]]["componentType"], g.FLOAT)
        self.assertEqual(self.read(self.attr["_PART"]), [(3.0,), (5.0,), (7.0,)])
        self.assertEqual(self.doc["accessors"][self.attr["TEXCOORD_0"]]["componentType"], g.USHORT)

    def test_clip_keeps_ends_and_drops_the_rest_pose(self):
        anim = self.doc["animations"][0]
        paths = [c["target"]["path"] for c in anim["channels"]]
        self.assertEqual(paths, ["rotation", "scale"])   # translation = the node's own: gone
        rot = anim["samplers"][anim["channels"][0]["sampler"]]
        times = [t[0] for t in self.read(rot["input"])]
        self.assertEqual(times, [0.0, 1.0])    # a uniform turn needs its two ends only
        self.assertEqual(self.doc["accessors"][rot["output"]]["componentType"], g.SHORT)
        q = self.read(rot["output"])[-1]
        self.assertLess(g.quat_angle(q, (0, 0, math.sin(math.pi / 4), math.cos(math.pi / 4))), 1e-4)
        sca = anim["samplers"][anim["channels"][1]["sampler"]]
        self.assertEqual(self.read(sca["output"]), [(2.0, 2.0, 2.0)] * 2)

    def test_smaller(self):
        self.assertLess(len(self.out), len(_glb()))


class RestClip(unittest.TestCase):
    def test_rest_clip_holds_the_node_transforms(self):
        doc, blob = g.read_glb(g.with_rest_clip(g.pack_bytes(_glb())[0]))
        rest = doc["animations"][-1]
        self.assertEqual(rest["name"], g.REST_CLIP)
        self.assertEqual(sorted(c["target"]["path"] for c in rest["channels"]), ["rotation", "scale"])
        views = [blob[v["byteOffset"]:v["byteOffset"] + v["byteLength"]] for v in doc["bufferViews"]]
        out = [g.read_accessor(doc, views, doc["accessors"][s["output"]]) for s in rest["samplers"]]
        self.assertIn([(0.0, 0.0, 0.0, 1.0)], out)


class Keys(unittest.TestCase):
    def test_reduce_keeps_a_corner(self):
        t = [0, 1, 2, 3, 4]
        v = [(0,), (1,), (2,), (1,), (0,)]
        self.assertEqual(g.reduce_keys(t, v, "translation"), [0, 2, 4])


@unittest.skipUnless(__import__("importlib").util.find_spec("numpy"), "numpy missing")
class Png(unittest.TestCase):
    def test_lossless_round_trip(self):
        import numpy as np

        rng = np.random.default_rng(1)
        rgb = rng.integers(0, 256, (16, 24, 3), dtype=np.uint8)
        rgb[4:8] = rgb[3]                       # some repeated rows for the filters
        opaque = np.concatenate([rgb, np.full((16, 24, 1), 255, np.uint8)], axis=2)
        src = g.png_encode(opaque)
        self.assertTrue((g.png_decode(src) == opaque).all())
        small = g.png_shrink(src)
        self.assertIsNotNone(small)
        self.assertTrue((g.png_decode(small) == rgb).all())   # alpha dropped, pixels kept
        few = (rng.integers(0, 256, (64, 64, 3), dtype=np.uint8) // 64) * 64   # 64 colours: indexed
        idx = g.png_shrink(g.png_encode(few))
        self.assertEqual(idx[25], 3)            # colour type 3 (palette)
        self.assertTrue((g.png_decode(g.png_encode(few)) == few).all())
        palette = next(b for k, b in g._chunks(idx) if k == b"PLTE")
        self.assertLessEqual(len(palette) // 3, 256)


@unittest.skipUnless(__import__("importlib").util.find_spec("numpy"), "numpy missing")
class Jpeg(unittest.TestCase):
    def test_only_opaque_base_colours_become_jpeg(self):
        """Images: 0 opaque base colour, 1 normal map, 2 base colour of a masked material,
        3 base colour with alpha < 255, 4 base colour also used as an occlusion map."""
        import numpy as np

        rng = np.random.default_rng(2)
        rgb = rng.integers(0, 256, (32, 32, 3), dtype=np.uint8)
        holey = np.concatenate([rgb, np.full((32, 32, 1), 255, np.uint8)], axis=2)
        holey[0, 0, 3] = 0
        pngs = [g.png_encode(rgb), g.png_encode(rgb[::-1]), g.png_encode(rgb[:, ::-1]), g.png_encode(holey),
                g.png_encode(rgb.transpose(1, 0, 2))]
        views, blob = [], b""
        for data in pngs:
            views.append({"buffer": 0, "byteOffset": len(blob), "byteLength": len(data)})
            blob += data + b"\0" * (-len(data) % 4)
        doc = {
            "asset": {"version": "2.0"},
            "images": [{"bufferView": i, "mimeType": "image/png"} for i in range(5)],
            "textures": [{"source": i} for i in range(5)],
            "materials": [
                {"pbrMetallicRoughness": {"baseColorTexture": {"index": 0}}, "normalTexture": {"index": 1}},
                {"pbrMetallicRoughness": {"baseColorTexture": {"index": 2}}, "alphaMode": "MASK"},
                {"pbrMetallicRoughness": {"baseColorTexture": {"index": 3}}},
                {"pbrMetallicRoughness": {"baseColorTexture": {"index": 4}}, "occlusionTexture": {"index": 4}},
            ],
            "accessors": [], "bufferViews": views, "buffers": [{"byteLength": len(blob)}],
        }
        calls = []

        def jpeg(px, quality):
            calls.append((px.shape, quality))
            return b"\xff\xd8\xff\xd9"

        out, stats = g.pack_bytes(g.write_glb(doc, blob), jpeg)
        mimes = [i["mimeType"] for i in g.read_glb(out)[0]["images"]]
        self.assertEqual(mimes, ["image/jpeg"] + ["image/png"] * 4)
        self.assertEqual(calls, [((32, 32, 3), g.JPEG_QUALITY)])
        self.assertEqual(stats["jpeg"][:1], [1])
        self.assertLess(stats["jpeg"][2], stats["jpeg"][1])
        _, plain = g.pack_bytes(g.write_glb(doc, blob))
        self.assertNotIn("jpeg", plain)


if __name__ == "__main__":
    unittest.main()
