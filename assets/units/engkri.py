"""English peasant (`EngKri`, `ENGKRI.MD`, "Bauer"): family 13 (`acr_assets.civil`). A dark
red-brown smock-frock to mid-thigh over the shirt (one draped garment, sleeves and body in
the tunic cloth), a red sash, light linen breeches, grey stockings, brown shoes, a slouch
hat; felling axe, sickle, hammer, sack and firewood with the work clips of the sprite packs.
"""

from acr_assets import civil

TYPE = "EngKri"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

MD = civil.peasant_md(rest=52, death=45)
PALETTE = {"smock": (0.36, 0.11, 0.09), "band": (0.62, 0.07, 0.06), "breeches": (0.78, 0.74, 0.60),
           "hose": (0.70, 0.68, 0.62)}


def spec():
    return civil.spec(MD, palette=PALETTE, body="smock", sleeves="smock", skirts=True, drape=True)


def make(root, name, build=None):
    s = spec()
    if build:
        s["build"] = build
    return civil.make(root, name, s)


def build(ctx):
    civil.build(ctx, spec())


def pose_for(ch, clip):
    civil.pose_for(ch, spec(), clip)
