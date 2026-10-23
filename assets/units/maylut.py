"""Maya archer (`MayLut`, `MAYLUT.MD`, MAY): white hip cloth, bare chest with red paint,
sandals, hair bound with a band and feathers; bow, arrow and quiver as the Aztec archer.
"""

from acr_assets import meso

TYPE = "MayLut"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

MD = {
    "STAND": "#STAND 9 1 1 0",
    "PSTAND": "#PSTAND 9 1 1 0",
    "PSTAND1": "#PSTAND1 9 1 1 0",
    "PSTAND2": "#PSTAND2 9 1 1 0",
    "MOTION_L": "@MOTION_L 9 0 0 7",
    "MOTION_R": "@MOTION_R 9 0 8 15",
    "MOTION_RB": "@MOTION_RB 9 0 15 8",
    "MOTION_LB": "@MOTION_LB 9 0 7 0",
    "MOTION_L0": "@MOTION_L0 9 0 0 7",
    "MOTION_R0": "@MOTION_R0 9 0 8 15",
    "MOTION_RB0": "@MOTION_RB0 9 0 15 8",
    "MOTION_LB0": "@MOTION_LB0 9 0 7 0",
    "ATTACK2": "#ATTACK2 9 79",
    "ATTACK1": "#ATTACK1 9 79",
    "ATTACK": "@ATTACK 9 2 0 50",
    "DEATH": "@DEATH 9 3 0 50",
    "DEATHLIE1": "#DEATHLIE1 9 1 3 50",
    "DEATHLIE2": "#DEATHLIE2 9 1 3 50",
    "DEATHLIE3": "#DEATHLIE3 9 1 3 50",
}
WEAPONS = {0: 'MECH', 1: 'OSTRELA', 2: 'STRELA'}
ACTIVE = {'ATTACK1': 44, 'ATTACK2': 44}

SPEC = meso.spec(MD, weapons=WEAPONS, active=ACTIVE, dress=("hipcloth", "sandals"), head="band", hand_r="arrow", bow=True, shield=None, melee="club",
                 paint="red_face_half")


def build(ctx):
    meso.build(ctx, SPEC)
