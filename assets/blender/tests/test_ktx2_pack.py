"""ktx2_pack: texture uses and the KHR_texture_basisu rewrite (#297)."""

import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "lib"))

from acr_assets import gltf_pack, ktx2_pack as k  # noqa: E402

KTX2 = b"\xabKTX 20\xbb\r\n\x1a\n" + b"\0" * 20


def _png(w, h):
    return b"\x89PNG\r\n\x1a\n" + b"\0\0\0\x0dIHDR" + w.to_bytes(4, "big") + h.to_bytes(4, "big") + b"\0" * 9


def _doc():
    images = [_png(64, 64), _png(64, 64), _png(32, 32), _png(8, 8), _png(30, 64)]
    blob, views = b"", []
    for data in images:
        views.append({"buffer": 0, "byteOffset": len(blob), "byteLength": len(data)})
        blob += data + b"\0" * (-len(data) % 4)
    doc = {
        "asset": {"version": "2.0"},
        "buffers": [{"byteLength": len(blob)}],
        "bufferViews": views,
        "images": [{"bufferView": i, "mimeType": "image/png"} for i in range(len(images))],
        "textures": [{"source": i} for i in range(len(images))],
        "materials": [
            {"pbrMetallicRoughness": {"baseColorTexture": {"index": 0}, "metallicRoughnessTexture": {"index": 2}},
             "normalTexture": {"index": 1}},
            {"alphaMode": "MASK", "pbrMetallicRoughness": {"baseColorTexture": {"index": 3}}},
            {"pbrMetallicRoughness": {"baseColorTexture": {"index": 4}}},
        ],
    }
    return gltf_pack.write_glb(doc, blob)


class Ktx2Pack(unittest.TestCase):
    def test_roles(self):
        doc, _ = gltf_pack.read_glb(_doc())
        self.assertEqual(k.image_roles(doc), {0: "colour", 1: "normal", 2: "data", 3: "masked", 4: "colour"})

    def test_rewrite(self):
        calls = []

        def fake(_tool, data, mime, role, _tmp):
            calls.append(role)
            return KTX2

        real, k.encode = k.encode, fake
        try:
            out, stats = k.pack_bytes(_doc(), tool="basisu")
        finally:
            k.encode = real
        doc, _ = gltf_pack.read_glb(out)
        # Too small (8 x 8) and not whole blocks (30 wide) stay PNG.
        self.assertEqual(sorted(calls), ["colour", "data", "normal"])
        self.assertEqual([i["mimeType"] for i in doc["images"]][:3], ["image/ktx2"] * 3)
        self.assertEqual(doc["images"][3]["mimeType"], "image/png")
        self.assertEqual(doc["textures"][0], {"extensions": {k.EXT: {"source": 0}}})
        self.assertEqual(doc["textures"][3], {"source": 3})
        self.assertIn(k.EXT, doc["extensionsRequired"])
        self.assertEqual(stats["normal"][0], 1)


if __name__ == "__main__":
    unittest.main()
