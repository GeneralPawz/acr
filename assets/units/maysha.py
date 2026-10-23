"""Maya priest (`MaySha`, `MAYSHA.MD`, MAY): a white tilmatli over the hip cloth, sandals, a
feather headdress, white dots painted; he heals (MEDIK: ATTACK1), hacks in melee (MECH).
"""

from acr_assets import meso

TYPE = "MaySha"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

MD = {
    "STAND": "#STAND 9 1 3 0",
    "MOTION_L": "@MOTION_L 9 1 0 9",
    "MOTION_R": "@MOTION_R 9 1 10 19",
    "MOTION_LB": "@MOTION_LB 9 1 19 10",
    "MOTION_RB": "@MOTION_RB 9 1 9 0",
    "PATTACK0": "#PATTACK0 9 1 6 0",
    "UATTACK0": "#UATTACK0 9 1 6 0",
    "ATTACK": "@ATTACK 9 6 0 50",
    "PATTACK1": "#PATTACK1 9 1 3 0",
    "UATTACK1": "#UATTACK1 9 1 3 0",
    "ATTACK1": "@ATTACK1 9 3 0 108",
    "DEATH": "@DEATH 9 4 0 53",
    "PSTAND": "#PSTAND 9 1 6 0",
    "PSTAND1": "#PSTAND1 9 1 3 0",
    "DEATHLIE1": "#DEATHLIE1 9 1 4 53",
}
WEAPONS = {0: 'MECH', 1: 'MEDIK'}
ACTIVE = {'ATTACK1': 1}

SPEC = meso.spec(MD, weapons=WEAPONS, active=ACTIVE, dress=("hipcloth", "tilmatli", "sandals"), head="maya", hand_r="macuahuitl", shield=None, melee="club",
                 paint="white_dots", palette={"tilmatli": (0.88, 0.85, 0.78)})


def build(ctx):
    meso.build(ctx, SPEC)
