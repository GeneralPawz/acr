"""English standard bearer, 18th century (`EngF18`, `ENGF18.MD`): EngM18's uniform without
knapsack and cartridge box, the colour belt over the left shoulder, carrying the King's
colour (the Union of 1707: St George's cross fimbriated white over St Andrew's saltire on
blue) on its pike; carried and waved by `clips.colour_clips`.
"""

from acr_assets import line

TYPE = "EngF18"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

MD = {
    "REST": "@REST 9 0 0 119",
    "STAND": "#STAND 9 1 0 119",
    "MOTION_L": "@MOTION_L 9 1 0 9",
    "MOTION_R": "@MOTION_R 9 1 10 19",
    "DEATH": "@DEATH 9 4 0 39",
    "DEATHLIE1": "#DEATHLIE1 9 1 4 39",
}

PALETTE = {"flag": (0.10, 0.16, 0.42), "flag_a": (0.92, 0.90, 0.84), "flag_b": (0.70, 0.08, 0.08)}


def spec():
    return line.spec(MD, palette=PALETTE, weapon="colour", gear=("belt_l",), flag="union", fixed=("flag",))


def make(root, name, build=None):
    return line.make(root, name, dict(spec(), build=build or line.BUILD))


def build(ctx):
    line.build(ctx, spec())


def pose_for(ch, clip):
    line.pose_for(ch, spec(), clip)
