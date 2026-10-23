"""Spanish ensign, 17th century (`SpnF17`, `SPNF17.MD`): ochre coat, broad hat, the red ragged Cross of Burgundy on white.

Family 6, pike and shot (`acr_assets.pike`, docs/families/pike.md).
"""

from acr_assets import pike

TYPE = "SpnF17"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

MD = {
    "REST": "@REST 9 0 0 59",
    "STAND": "#STAND 9 1 0 0",
    "MOTION_L": "@MOTION_L 9 1 0 9",
    "MOTION_R": "@MOTION_R 9 1 10 19",
    "DEATH": "@DEATH 9 4 0 59",
    "DEATHLIE1": "#DEATHLIE1 9 1 4 59",
}


def spec():
    return pike.support_spec(MD, "colour", palette={"coat": (0.60, 0.46, 0.28), "breeches": (0.52, 0.10, 0.08), "flag": (0.92, 0.90, 0.84), "flag_b": (0.70, 0.08, 0.08)}, flag="burgundy", fixed=("flag",))


def make(root, name, build=None):
    s = spec()
    return pike.make(root, name, dict(s, build=build or s["build"]))


def build(ctx):
    pike.build(ctx, spec())


def pose_for(ch, clip):
    pike.pose_for(ch, spec(), clip)
