"""French peasant (`FrnKri`, `FRNKRI.MD`, "Bauer"): family 13 (`acr_assets.civil`). The
sprite's white shirt sleeves under a dark slate waistcoat (one draped garment: body panels
in the waistcoat's cloth, sleeves in the shirt's), blue-grey breeches, pale stockings, brown
shoes, a slouch hat; no sash. Tools and work clips as EngKri.
"""

from acr_assets import civil

TYPE = "FrnKri"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

MD = civil.peasant_md(rest=195, death=68)
PALETTE = {"shirt": (0.86, 0.85, 0.80), "vest": (0.20, 0.25, 0.31), "breeches": (0.28, 0.33, 0.42),
           "hose": (0.80, 0.78, 0.72), "hat": (0.45, 0.33, 0.20)}


def spec():
    return civil.spec(MD, palette=PALETTE, body="vest", sleeves="shirt", sash=False, fixed=("shirt",))


def make(root, name, build=None):
    s = spec()
    if build:
        s["build"] = build
    return civil.make(root, name, s)


def build(ctx):
    civil.build(ctx, spec())


def pose_for(ch, clip):
    civil.pose_for(ch, spec(), clip)
