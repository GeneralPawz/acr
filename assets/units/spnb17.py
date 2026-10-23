"""Spanish drummer, 17th century (`SpnB17`, `SPNB17.MD`): yellow livery faced red, broad hat, the side drum.

Family 6, pike and shot (`acr_assets.pike`, docs/families/pike.md).
"""

from acr_assets import pike

TYPE = "SpnB17"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

MD = {
    "STAND": "#STAND 9 1 0 0",
    "PSTAND": "@PSTAND 9 0 0 61",
    "MOTION_L": "@MOTION_L 9 1 0 9",
    "MOTION_R": "@MOTION_R 9 1 10 19",
    "MOTION_LB": "@MOTION_LB 9 1 19 10",
    "MOTION_RB": "@MOTION_RB 9 1 9 0",
    "DEATH": "@DEATH 9 3 0 47",
    "DEATHLIE1": "#DEATHLIE1 9 1 3 47",
}


def spec():
    return pike.support_spec(MD, "drum", palette={"coat": (0.75, 0.60, 0.20), "facings": (0.62, 0.10, 0.07), "breeches": (0.62, 0.10, 0.07)})


def make(root, name, build=None):
    s = spec()
    return pike.make(root, name, dict(s, build=build or s["build"]))


def build(ctx):
    pike.build(ctx, spec())


def pose_for(ch, clip):
    pike.pose_for(ch, spec(), clip)
