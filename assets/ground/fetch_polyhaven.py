r"""Download the ground's CC0 texture sets from Poly Haven (docs/ground.md).

    python assets/ground/fetch_polyhaven.py [OUT]

Writes OUT\<id>\{diff,nor_gl,rough,disp}.jpg (2k) and SOURCE.json per set; OUT defaults to
D:\ACR-models\polyhaven-textures. `cargo xtask ground` packs them for the game.
"""
import json
import os
import sys
import urllib.request

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from materials import SETS  # noqa: E402

OUT = sys.argv[1] if len(sys.argv) > 1 else r"D:\ACR-models\polyhaven-textures"
UA = {"User-Agent": "ACR-asset-fetch/1.0"}
MAPS = {"Diffuse": "diff", "nor_gl": "nor_gl", "Rough": "rough", "Displacement": "disp"}


def get(url):
    with urllib.request.urlopen(urllib.request.Request(url, headers=UA), timeout=120) as r:
        return r.read()


for aid in SETS.values():
    d = os.path.join(OUT, aid)
    try:
        files = json.loads(get(f"https://api.polyhaven.com/files/{aid}"))
        os.makedirs(d, exist_ok=True)
        for key, name in MAPS.items():
            entry = files[key]["2k"]
            url = (entry.get("jpg") or entry["png"])["url"]
            p = os.path.join(d, name + os.path.splitext(url)[1])
            if not os.path.exists(p):
                open(p, "wb").write(get(url))
        info = json.loads(get(f"https://api.polyhaven.com/info/{aid}"))
        json.dump({"id": aid, "name": info.get("name"), "authors": info.get("authors"), "license": "CC0",
                   "source": f"https://polyhaven.com/a/{aid}", "res": "2k"},
                  open(os.path.join(d, "SOURCE.json"), "w"), indent=1)
        print(f"ok {aid}", flush=True)
    except Exception as e:  # keep going
        print(f"FAIL {aid}: {e}", flush=True)
print("DONE", flush=True)
