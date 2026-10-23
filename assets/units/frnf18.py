"""French standard bearer, 18th century (`FrnF18`, `FRNF18.MD`): FrnM18's uniform without
knapsack and cartridge box, the colour belt over the left shoulder, carrying a French
regimental colour: the white cross through blue quarters (the Gardes francaises' colour,
without its fleurs-de-lis).
"""

from acr_assets import line

TYPE = "FrnF18"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

MD = {
    "REST": "@REST 9 0 0 119",
    "STAND": "#STAND 9 1 0 0",
    "MOTION_L": "@MOTION_L 9 1 0 9",
    "MOTION_R": "@MOTION_R 9 1 10 19",
    "DEATH": "@DEATH 9 4 0 39",
    "DEATHLIE1": "#DEATHLIE1 9 1 4 39",
}

PALETTE = {
    "coat": (0.12, 0.17, 0.38), "facings": (0.62, 0.08, 0.07), "lining": (0.62, 0.08, 0.07),
    "waistcoat": (0.86, 0.85, 0.80), "lapels": (0.86, 0.85, 0.80), "breeches": (0.86, 0.85, 0.80),
    "belts": (0.80, 0.70, 0.45),
    "flag": (0.12, 0.18, 0.46), "flag_a": (0.93, 0.92, 0.88),
}


def spec():
    return line.spec(MD, palette=PALETTE, weapon="colour", gear=("belt_l",), flag="france", lapels="lapels", fixed=("waistcoat", "lapels", "flag"))


def make(root, name, build=None):
    return line.make(root, name, dict(spec(), build=build or line.BUILD))


def build(ctx):
    line.build(ctx, spec())


def pose_for(ch, clip):
    line.pose_for(ch, spec(), clip)
