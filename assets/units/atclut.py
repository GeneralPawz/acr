"""Aztec archer (`AtcLut`, `ATCLUT.MD`, ACTEC): maxtlatl and a short quilted ichcahuipilli,
sandals, a feather headband; bow in the left hand, the arrow in the right, a quiver on the
back (OSTRELA and STRELA: ATTACK1/ATTACK2, release on 44); melee (MECH) a hack.
"""

from acr_assets import meso

TYPE = "AtcLut"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

MD = {
    "STAND": "#STAND 9 1 1 0",
    "PSTAND": "#PSTAND 9 1 1 0",
    "PSTAND1": "#PSTAND1 9 1 1 0",
    "PSTAND2": "#PSTAND2 9 1 1 0",
    "MOTION_L": "@MOTION_L 9 0 0 8",
    "MOTION_R": "@MOTION_R 9 0 9 15",
    "MOTION_RB": "@MOTION_RB 9 0 15 9",
    "MOTION_LB": "@MOTION_LB 9 0 8 0",
    "MOTION_L0": "@MOTION_L0 9 0 0 7",
    "MOTION_R0": "@MOTION_R0 9 0 8 15",
    "MOTION_LB0": "@MOTION_LB0 9 0 15 8",
    "MOTION_RB0": "@MOTION_RB0 9 0 7 0",
    "ATTACK2": "#ATTACK2 9 79",
    "ATTACK1": "#ATTACK1 9 79",
    "ATTACK": "@ATTACK 9 2 0 60",
    "DEATH": "@DEATH 9 3 0 48",
    "DEATHLIE1": "#DEATHLIE1 9 1 3 48",
    "DEATHLIE2": "#DEATHLIE2 9 1 3 48",
    "DEATHLIE3": "#DEATHLIE3 9 1 3 48",
}
WEAPONS = {0: 'MECH', 1: 'OSTRELA', 2: 'STRELA'}
ACTIVE = {'ATTACK1': 44, 'ATTACK2': 44}

SPEC = meso.spec(MD, weapons=WEAPONS, active=ACTIVE, dress=("maxtlatl", "ichcahuipilli", "sandals"), head="band", hand_r="arrow", bow=True, shield=None,
                 melee="club", paint="black_stripes")


def build(ctx):
    meso.build(ctx, SPEC)
