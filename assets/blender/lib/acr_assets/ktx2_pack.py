"""GPU-compressed textures for a packed `.glb` (pure Python, no `bpy`; #199, #297).

A step after `gltf_pack`: every PNG or JPEG texture becomes a KTX2 file in Basis Universal
(`KHR_texture_basisu`, listed as required), which `acr-render` (`model.rs`, `ktx2.rs`)
transcodes at load to what the GPU samples directly: BC7 on desktop, ETC2 or ASTC where BC
is missing, RGBA8 as the last resort. The mips are encoded with it.

Modes per texture use (`MODES`):

- base colours of opaque materials: ETC1S (`ETC1S_Q`), the smallest files;
- base colours of alpha-masked or blended materials (leaf and hair cards, decay layers):
  UASTC with light RDO, so the alpha edge that `discard` cuts stays sharp;
- normal maps: UASTC with `-normal_map` (linear, no RDO), renormalised mips;
- metallic-roughness, occlusion and emissive maps: UASTC, linear.

Images below `MIN_SIZE` texels or whose sides are not multiples of 4 (block textures need
whole blocks at level 0) stay PNG or JPEG. The encoder is the upstream `basisu` tool
(`ACR_BASISU`, else `basisu` on the PATH; the render VM has it in `~/tools/basis_universal`).

Run over finished glbs: `python3 ktx2_pack.py a.glb b.glb ...` (in place). Blender's glTF
importer cannot read KTX2, so `posecheck` and the asset tests keep the PNG glbs; this step
runs only at install time.
"""

import json
import os
import shutil
import subprocess
import sys
import tempfile

try:
    from . import gltf_pack
except ImportError:  # run as a script
    sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
    import gltf_pack

EXT = "KHR_texture_basisu"
MIN_SIZE = 16
ETC1S_Q = 255           # ETC1S quality 1..255 (255 best)
UASTC_RDO = 1.0         # UASTC RDO lambda for alpha cards and data maps (0: off)
MODES = {
    "colour": ["-q", str(ETC1S_Q), "-comp_level", "2", "-mip_srgb"],
    "masked": ["-uastc", "-uastc_level", "2", "-uastc_rdo_l", "0.5", "-mip_srgb"],
    "normal": ["-uastc", "-uastc_level", "2", "-normal_map", "-mip_renorm", "-mip_linear"],
    "data": ["-uastc", "-uastc_level", "2", "-uastc_rdo_l", str(UASTC_RDO), "-linear", "-mip_linear"],
}


def encoder():
    tool = os.environ.get("ACR_BASISU") or shutil.which("basisu")
    if not tool:
        home = os.path.expanduser("~/tools/basis_universal/basisu")
        tool = home if os.path.exists(home) else None
    return tool


def image_roles(doc):
    """Image index -> use ("colour", "masked", "normal" or "data"); an image used several
    ways takes the most careful one (normal > data > masked > colour)."""
    tex = doc.get("textures", [])
    rank = {"colour": 0, "masked": 1, "data": 2, "normal": 3}
    roles = {}

    def mark(ref, role):
        if not ref or ref.get("index", -1) >= len(tex):
            return
        t = tex[ref["index"]]
        src = t.get("extensions", {}).get(EXT, {}).get("source", t.get("source"))
        if src is not None and rank[role] >= rank.get(roles.get(src), -1):
            roles[src] = role

    for m in doc.get("materials", []):
        pbr = m.get("pbrMetallicRoughness", {})
        mark(pbr.get("baseColorTexture"), "colour" if m.get("alphaMode", "OPAQUE") == "OPAQUE" else "masked")
        mark(pbr.get("metallicRoughnessTexture"), "data")
        mark(m.get("normalTexture"), "normal")
        mark(m.get("occlusionTexture"), "data")
        mark(m.get("emissiveTexture"), "data")
    return roles


def _size(data, mime):
    if mime == "image/png":
        return _png_size(data)
    if mime == "image/jpeg":
        return _jpeg_size(data)
    return None


def _png_size(data):
    if data[:8] != b"\x89PNG\r\n\x1a\n" or data[12:16] != b"IHDR":
        return None
    return int.from_bytes(data[16:20], "big"), int.from_bytes(data[20:24], "big")


def _jpeg_size(data):
    if data[:2] != b"\xff\xd8":
        return None
    at = 2
    while at + 9 < len(data):
        if data[at] != 0xFF:
            return None
        marker = data[at + 1]
        n = int.from_bytes(data[at + 2:at + 4], "big")
        if 0xC0 <= marker <= 0xCF and marker not in (0xC4, 0xC8, 0xCC):
            return int.from_bytes(data[at + 7:at + 9], "big"), int.from_bytes(data[at + 5:at + 7], "big")
        at += 2 + n
    return None


def encode(tool, data, mime, role, tmp):
    src = os.path.join(tmp, "in.png" if mime == "image/png" else "in.jpg")
    out = os.path.join(tmp, "out.ktx2")
    with open(src, "wb") as f:
        f.write(data)
    if os.path.exists(out):
        os.remove(out)
    cmd = [tool, "-ktx2", "-mipmap", *MODES[role], "-file", src, "-output_file", out]
    r = subprocess.run(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, check=False)
    if r.returncode != 0 or not os.path.exists(out):
        raise RuntimeError(f"basisu failed ({role}): {r.stderr.decode(errors='replace')[-400:]}")
    with open(out, "rb") as f:
        return f.read()


def pack_bytes(data, tool=None):
    """The glb with its textures as KTX2, and per use [count, bytes before, bytes after]."""
    tool = tool or encoder()
    if not tool:
        raise RuntimeError("no basisu encoder (set ACR_BASISU)")
    doc, blob = gltf_pack.read_glb(data)
    images = doc.get("images", [])
    if not images or not doc.get("bufferViews"):
        return data, {}
    p = gltf_pack._Packer(doc, blob)
    roles = image_roles(doc)
    stats, done = {}, set()
    with tempfile.TemporaryDirectory(prefix="acr-ktx2-") as tmp:
        for i, img in enumerate(images):
            mime = img.get("mimeType")
            if "bufferView" not in img or mime not in ("image/png", "image/jpeg") or i not in roles:
                continue
            raw = p.views[img["bufferView"]]
            size = _size(raw, mime)
            if not size or min(size) < MIN_SIZE or size[0] % 4 or size[1] % 4:
                continue
            out = encode(tool, raw, mime, roles[i], tmp)
            p.views[img["bufferView"]] = out
            img["mimeType"] = "image/ktx2"
            done.add(i)
            n, a, b = stats.get(roles[i], [0, 0, 0])
            stats[roles[i]] = [n + 1, a + len(raw), b + len(out)]
    if not done:
        return data, stats
    for t in doc.get("textures", []):
        if t.get("source") in done:
            t.setdefault("extensions", {})[EXT] = {"source": t.pop("source")}
    for key in ("extensionsUsed", "extensionsRequired"):
        names = doc.setdefault(key, [])
        if EXT not in names:
            names.append(EXT)
    blob = p.finish()
    return gltf_pack.write_glb(doc, blob), stats


def pack_file(path, tool=None):
    with open(path, "rb") as f:
        data = f.read()
    out, stats = pack_bytes(data, tool)
    with open(path, "wb") as f:
        f.write(out)
    stats["bytes"] = [len(data), len(out)]
    return stats


def main(paths):
    tool = encoder()
    total = [0, 0]
    for path in paths:
        s = pack_file(path, tool)
        a, b = s.pop("bytes")
        total = [total[0] + a, total[1] + b]
        print(f"{os.path.basename(path)}: {a / 1e6:.2f} -> {b / 1e6:.2f} MB {json.dumps(s)}", flush=True)
    print(f"total: {total[0] / 1e6:.1f} -> {total[1] / 1e6:.1f} MB")


if __name__ == "__main__":
    main(sys.argv[1:])
