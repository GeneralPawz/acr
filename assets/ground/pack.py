r"""Packs the ground's texture sets for the game (docs/ground.md), run by `cargo xtask ground`.

    python -I assets/ground/pack.py SRC OUT [SIZE]

SRC holds <id>\{diff,nor_gl,rough,disp}.jpg per Poly Haven set (fetch_polyhaven.py);
OUT\ground\<class>_a.png gets the albedo (RGB) and height (A), OUT\ground\<class>_n.png the
OpenGL normal (RG), roughness (B) and 255 (A), SIZE x SIZE (default 1024), in the class order of
materials.py. The game builds the mipmaps.
"""
import os
import sys

from PIL import Image

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from materials import SETS  # noqa: E402

src, out = sys.argv[1], sys.argv[2]
size = int(sys.argv[3]) if len(sys.argv) > 3 else 1024
os.makedirs(os.path.join(out, "ground"), exist_ok=True)


def load(aid, name, mode):
    return Image.open(os.path.join(src, aid, name + ".jpg")).convert(mode).resize((size, size), Image.LANCZOS)


for cls, aid in SETS.items():
    diff = load(aid, "diff", "RGB")
    disp = load(aid, "disp", "L")
    a = Image.merge("RGBA", (*diff.split(), disp))
    a.save(os.path.join(out, "ground", f"{cls}_a.png"), optimize=True)
    nor = load(aid, "nor_gl", "RGB")
    rough = load(aid, "rough", "L")
    r, g, _ = nor.split()
    n = Image.merge("RGBA", (r, g, rough, Image.new("L", (size, size), 255)))
    n.save(os.path.join(out, "ground", f"{cls}_n.png"), optimize=True)
    print(f"packed {cls} ({aid})", flush=True)
