r"""Packs the ground's texture sets for the game (docs/ground.md), run by `cargo xtask ground`.

    python -I assets/ground/pack.py SRC OUT [SIZE]

SRC is the sources root (D:\ACR-models) with the sets of materials.py: Poly Haven sets
(<id>\{diff,nor_gl,rough,disp}.jpg, fetch_polyhaven.py) or ambientCG sets
(<id>\<id>_2K-JPG_{Color,NormalGL,Roughness,Displacement}.jpg). OUT\ground\<layer>_a.png gets
the albedo (RGB) and height (A), OUT\ground\<layer>_n.png the OpenGL normal (RG), roughness
(B) and 255 (A), SIZE x SIZE (default 1024), in the layer order of materials.py. The game
builds the mipmaps.
"""
import os
import sys

from PIL import Image

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from materials import SETS  # noqa: E402

src, out = sys.argv[1], sys.argv[2]
size = int(sys.argv[3]) if len(sys.argv) > 3 else 1024
os.makedirs(os.path.join(out, "ground"), exist_ok=True)

# Poly Haven map name -> ambientCG map name.
AMBIENTCG = {"diff": "Color", "nor_gl": "NormalGL", "rough": "Roughness", "disp": "Displacement"}


def load(path, name, mode):
    file = os.path.join(src, path, name + ".jpg")
    if not os.path.exists(file):
        aid = os.path.basename(path)
        file = os.path.join(src, path, f"{aid}_2K-JPG_{AMBIENTCG[name]}.jpg")
    return Image.open(file).convert(mode).resize((size, size), Image.LANCZOS)


for layer, path in SETS.items():
    diff = load(path, "diff", "RGB")
    disp = load(path, "disp", "L")
    a = Image.merge("RGBA", (*diff.split(), disp))
    a.save(os.path.join(out, "ground", f"{layer}_a.png"), optimize=True)
    nor = load(path, "nor_gl", "RGB")
    rough = load(path, "rough", "L")
    r, g, _ = nor.split()
    n = Image.merge("RGBA", (r, g, rough, Image.new("L", (size, size), 255)))
    n.save(os.path.join(out, "ground", f"{layer}_n.png"), optimize=True)
    print(f"packed {layer} ({path})", flush=True)
