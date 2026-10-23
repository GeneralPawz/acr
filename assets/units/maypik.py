"""Maya spearman (`MayPik`, `MAYPIK.MD`, MAY): quilted cotton jacket and hip cloth, sandals,
a turban with plumes; a long spear with a broad stone head in both hands (PIKA: the
thrust, `ATTACK_THRUST_MID`, and down at a man on the ground).
"""

from acr_assets import meso

TYPE = "MayPik"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

MD = {
    "STAND": "#STAND 9 1 3 0",
    "MOTION_L": "@MOTION_L 9 1 0 7",
    "MOTION_R": "@MOTION_R 9 1 8 15",
    "MOTION_LB": "@MOTION_LB 9 1 15 8",
    "MOTION_RB": "@MOTION_RB 9 1 7 0",
    "MOTION_L0": "@MOTION_L0 9 1 0 7",
    "MOTION_R0": "@MOTION_R0 9 1 8 15",
    "MOTION_LB0": "@MOTION_LB0 9 1 15 8",
    "MOTION_RB0": "@MOTION_RB0 9 1 7 0",
    "ATTACK": "@ATTACK 9 3 0 68",
    "DEATH": "@DEATH 9 4 0 68",
    "DEATHLIE1": "#DEATHLIE1 9 1 4 68",
}
WEAPONS = {0: 'PIKA'}
ACTIVE = {}

SPEC = meso.spec(MD, weapons=WEAPONS, active=ACTIVE, dress=("hipcloth", "maya_jacket", "sandals"), head="maya", hand_r="spear", shield=None, melee="spear",
                 palette={"maya_jacket": (0.85, 0.80, 0.66), "plumes": (0.70, 0.20, 0.12)}, plume=("macaw", "quetzal"))


def build(ctx):
    meso.build(ctx, SPEC)
