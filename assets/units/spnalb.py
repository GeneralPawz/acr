"""Spanish halberdier, 16th century (`SpnAlb`, `SPNALB.MD`, `PIKA`): a steel cuirass with
tassets over a doublet with red sleeves, olive hose, a combed morion with a red plume, the
sword on the left hip and the halberd (thrust and stab down, `conquista.halberd_clips`).

Family 12 (`acr_assets.conquista`, docs/families/conquista.md).
"""

from acr_assets import conquista

TYPE = "SpnAlb"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

MD = {
    "STAND": "#STAND 9 1 5 0",
    "PSTAND": "#PSTAND 9 1 3 0",
    "PSTAND1": "#PSTAND1 9 1 3 20",
    "MOTION_L": "@MOTION_L 9 1 0 9",
    "MOTION_R": "@MOTION_R 9 1 10 19",
    "MOTION_LB": "@MOTION_LB 9 1 19 10",
    "MOTION_RB": "@MOTION_RB 9 1 9 0",
    "MOTION_L0": "@MOTION_L0 9 2 0 9",
    "MOTION_R0": "@MOTION_R0 9 2 10 19",
    "MOTION_LB0": "@MOTION_LB0 9 2 19 10",
    "MOTION_RB0": "@MOTION_RB0 9 2 9 0",
    "ATTACK": "@ATTACK 9 3 0 47",
    "PATTACK": "@PATTACK 9 5 0 37",
    "UATTACK": "@UATTACK 9 5 37 0",
    "DEATH": "@DEATH 9 4 0 41",
    "DEATHLIE1": "#DEATHLIE1 9 1 4 41",
}


def spec():
    return conquista.spec(MD, palette={"doublet": (0.55, 0.18, 0.10), "dsleeve": (0.68, 0.10, 0.07), "fhose": (0.38, 0.34, 0.20),
                                       "slops": (0.55, 0.16, 0.10), "breeches": (0.35, 0.30, 0.18)},
                              layers=("blouse", "slops", "cuirass", "tassets"), kit=("belt_r", "c_scabbard", "plume"),
                              gun=None)


def build(ctx):
    conquista.build_pole(ctx, spec())


def pose_for(ch, clip):
    conquista.pose_for(ch, spec(), clip)
