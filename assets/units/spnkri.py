"""Spanish peasant (`SpnKri`, `SPNKRI.MD`, "Bauer"): family 13 (`acr_assets.civil`). An
off-white linen shirt bloused at the waist, a red sash, red-brown breeches, pale stockings,
brown shoes, a slouch hat. Tools and work clips as EngKri (stance 1 walks on its own sprites,
`MOTION_L1 9 2`).
"""

from acr_assets import civil

TYPE = "SpnKri"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

MD = civil.peasant_md(rest=195, death=47, motion1_gp=2)
PALETTE = {"shirt": (0.85, 0.82, 0.74), "band": (0.66, 0.08, 0.06), "breeches": (0.52, 0.20, 0.12),
           "hose": (0.80, 0.78, 0.72), "hat": (0.42, 0.30, 0.18)}


def spec():
    return civil.spec(MD, palette=PALETTE, body="shirt", sleeves="shirt", fixed=("shirt",))


def make(root, name, build=None):
    s = spec()
    if build:
        s["build"] = build
    return civil.make(root, name, s)


def build(ctx):
    civil.build(ctx, spec())


def pose_for(ch, clip):
    civil.pose_for(ch, spec(), clip)
