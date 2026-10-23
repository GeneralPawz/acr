"""Spanish drummer, 18th century (`SpnB18`, `SPNB18.MD`): in the King's livery of the Spanish
Bourbons (Kronoskaf: the line's drummers wore the royal livery): blue coat faced red, laced
in white on the lapels and cuffs, white-bound tricorn; the drum shell in the red facings.
"""

from acr_assets import line

TYPE = "SpnB18"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

MD = {
    "REST": "@REST 9 0 0 69",
    "STAND": "#STAND 9 1 0 0",
    "MOTION_L": "@MOTION_L 9 3 0 9",
    "MOTION_R": "@MOTION_R 9 3 10 19",
    "MOTION_LB": "@MOTION_LB 9 3 19 10",
    "MOTION_RB": "@MOTION_RB 9 3 9 0",
    "MOTION_L0": "@MOTION_L0 9 1 0 9",
    "MOTION_R0": "@MOTION_R0 9 1 10 19",
    "MOTION_LB0": "@MOTION_LB0 9 1 19 10",
    "MOTION_RB0": "@MOTION_RB0 9 1 9 0",
    "DEATH": "@DEATH 9 4 0 39",
    "DEATHLIE1": "#DEATHLIE1 9 1 4 39",
    "PATTACK": "#PATTACK 9 1 0 0",
    "ATTACK": "@ATTACK 9 2 0 19",
    "UATTACK": "#UATTACK 9 1 0 0",
    "PSTAND": "#PSTAND 9 1 2 0",
}

PALETTE = {
    "coat": (0.14, 0.20, 0.45), "facings": (0.64, 0.09, 0.07), "lining": (0.64, 0.09, 0.07),
    "waistcoat": (0.64, 0.09, 0.07), "breeches": (0.86, 0.85, 0.80), "belts": (0.82, 0.74, 0.50),
    "lace": (0.90, 0.89, 0.84),
}


def spec():
    return line.spec(MD, palette=PALETTE, weapon="drum", gear=("belt_r",), lace={"hat": 0.012, "buttonholes": True}, fixed=("lace",))


def make(root, name, build=None):
    return line.make(root, name, dict(spec(), build=build or line.BUILD))


def build(ctx):
    line.build(ctx, spec())


def pose_for(ch, clip):
    line.pose_for(ch, spec(), clip)
