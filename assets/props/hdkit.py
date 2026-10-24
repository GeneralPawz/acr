"""High-detail kit of the 18th-century British line soldier (m2-hd-musket), laid out on the
ground for inspection: the Short Land Pattern musket, its socket bayonet and scabbard, the
cartridge box, the two buff crossbelts, the hanger and its scabbard, the canteen and the
knapsack. Each a baked master with three levels (`acr_assets.gear_hd`); stats under
`gear_hd` (triangles per item and level, texture sizes, the musket's grip geometry)."""

from acr_assets import gear_hd

# Item -> (x, y, z, rotation about X, rotation about Z) on the ground (metres, degrees).
LAYOUT = {
    "musket": (0.0, 0.30, 0.04, 90, 90),
    "bayonet": (-0.40, -0.05, 0.015, 90, 90),
    "bayonet_scabbard": (0.15, -0.05, 0.01, 90, 90),
    "cartridge_box": (-0.55, -0.45, 0.065, 0, 0),
    "crossbelts": (0.95, -0.55, 0.0, 0, 90),
    "hanger": (-0.20, -0.25, 0.012, 90, 90),
    "hanger_scabbard": (0.55, -0.25, 0.01, 90, 90),
    "canteen": (-0.10, -0.55, 0.035, 90, 0),
    "knapsack": (0.30, -0.70, 0.05, 90, 0),
}


def build(ctx):
    import math

    for key, (x, y, z, rx, rz) in LAYOUT.items():
        ob, lods = gear_hd.build_item(ctx, key)
        for o in [ob, *lods]:
            o.parent = ctx.root
            o.rotation_euler = (math.radians(rx), 0.0, math.radians(rz))
            o.location = (x, y, z)
    st = ctx.stats["gear_hd"]
    ctx.stats["triangles"] = {k: v["tris"] for k, v in st.items() if isinstance(v, dict) and "tris" in v}
