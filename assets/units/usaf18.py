"""Continental standard bearer, 18th century (`UsaF18`, `USAF18.MD`): UsaM18's uniform without
knapsack and cartridge box, the colour belt over the left shoulder, carrying a Continental
colour of 1775-77: thirteen red and white stripes with a plain blue canton.
"""

from acr_assets import line

TYPE = "UsaF18"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

MD = {
    "REST": "@REST 9 0 0 119",
    "STAND": "#STAND 9 1 0 119",
    "MOTION_L": "@MOTION_L 9 1 0 9",
    "MOTION_R": "@MOTION_R 9 1 10 19",
    "DEATH": "@DEATH 9 4 0 39",
    "DEATHLIE1": "#DEATHLIE1 9 1 4 39",
}

PALETTE = {
    "coat": (0.12, 0.16, 0.34), "facings": (0.62, 0.09, 0.07), "lining": (0.86, 0.85, 0.80),
    "waistcoat": (0.86, 0.85, 0.80), "breeches": (0.86, 0.85, 0.80), "belts": (0.84, 0.82, 0.74),
    "flag": (0.70, 0.08, 0.08), "flag_a": (0.92, 0.90, 0.84), "flag_c": (0.10, 0.16, 0.42),
    "lace": (0.90, 0.89, 0.84),
}


def spec():
    return line.spec(MD, palette=PALETTE, weapon="colour", gear=("belt_l",), flag="stripes", lace={"hat": 0.012}, fixed=("waistcoat", "lining", "flag"))


def make(root, name, build=None):
    return line.make(root, name, dict(spec(), build=build or line.BUILD))


def build(ctx):
    line.build(ctx, spec())


def pose_for(ch, clip):
    line.pose_for(ch, spec(), clip)
