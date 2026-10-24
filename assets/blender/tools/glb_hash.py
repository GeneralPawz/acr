"""Per-mesh hashes of glbs' vertex data (POSITION, JOINTS_0, WEIGHTS_0 bytes; meshopt or plain),
one column per file, `*` where they differ: the repeatability check of docs/assets.md
"Repeatable builds". Plain Python, no Blender.

    python3 assets/blender/tools/glb_hash.py a.glb b.glb [...]
"""
import hashlib
import json
import struct
import sys


def load(path):
    data = open(path, "rb").read()
    off, js, binc = 12, None, b""
    while off < len(data):
        ln, ty = struct.unpack_from("<II", data, off)
        chunk = data[off + 8 : off + 8 + ln]
        if ty == 0x4E4F534A:
            js = json.loads(chunk)
        else:
            binc = chunk
        off += 8 + ln
    return js, binc


def view_bytes(js, binc, acc_i):
    acc = js["accessors"][acc_i]
    bv = js["bufferViews"][acc["bufferView"]]
    ext = bv.get("extensions", {}).get("EXT_meshopt_compression")
    if ext:
        bv = ext
    o = bv.get("byteOffset", 0)
    return binc[o : o + bv["byteLength"]] + json.dumps(acc, sort_keys=True).encode()


def hashes(path):
    js, binc = load(path)
    out = {}
    for m in js["meshes"]:
        h = hashlib.sha1()
        for p in m["primitives"]:
            for k in ("POSITION", "JOINTS_0", "WEIGHTS_0"):
                if k in p["attributes"]:
                    h.update(view_bytes(js, binc, p["attributes"][k]))
        out[m.get("name", "?")] = h.hexdigest()[:12]
    return out


if __name__ == "__main__":
    all_h = [hashes(p) for p in sys.argv[1:]]
    names = sorted(set().union(*all_h))
    same = 0
    for n in names:
        row = [h.get(n, "-") for h in all_h]
        ok = len(set(row)) == 1
        same += ok
        print(("  " if ok else "* ") + n.ljust(28), " ".join(row))
    print(f"meshes {len(names)} identical {same}")
