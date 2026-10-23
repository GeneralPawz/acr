"""English ensign, 17th century (`EngF17`, `ENGF17.MD`): red coat, broad hat, the colour belt over the left shoulder and a St George's colour.

Family 6, pike and shot (`acr_assets.pike`, docs/families/pike.md).
"""

from acr_assets import pike

TYPE = "EngF17"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

MD = {
    "REST": "@REST 9 0 0 59",
    "STAND": "#STAND 9 1 0 0",
    "MOTION_L": "@MOTION_L 9 1 0 9",
    "MOTION_R": "@MOTION_R 9 1 10 19",
    "DEATH": "@DEATH 9 4 0 58",
    "DEATHLIE1": "#DEATHLIE1 9 1 4 58",
}


def spec():
    return pike.support_spec(MD, "colour", palette={"coat": (0.62, 0.10, 0.07), "breeches": (0.45, 0.44, 0.42), "flag": (0.92, 0.90, 0.84), "flag_a": (0.70, 0.08, 0.08)}, flag="france", fixed=("flag",))


def make(root, name, build=None):
    s = spec()
    return pike.make(root, name, dict(s, build=build or s["build"]))


def build(ctx):
    pike.build(ctx, spec())


def pose_for(ch, clip):
    pike.pose_for(ch, spec(), clip)
