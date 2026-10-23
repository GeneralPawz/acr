"""Aztec priest (`AtcSha`, `ATCSHA.MD`, ACTEC): black body paint, a dark tilmatli, maxtlatl
and sandals, hair long and bound; he heals (MEDIK: ATTACK1) and in melee (MECH) hacks
with a macuahuitl.
"""

from acr_assets import meso

TYPE = "AtcSha"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

MD = {
    "STAND": "#STAND 9 1 3 0",
    "MOTION_L": "@MOTION_L 9 1 0 9",
    "MOTION_R": "@MOTION_R 9 1 10 19",
    "MOTION_LB": "@MOTION_LB 9 1 19 10",
    "MOTION_RB": "@MOTION_RB 9 1 9 0",
    "MOTION_L0": "@MOTION_L0 9 1 0 9",
    "MOTION_R0": "@MOTION_R0 9 1 10 19",
    "MOTION_LB0": "@MOTION_LB0 9 1 19 10",
    "MOTION_RB0": "@MOTION_RB0 9 1 9 0",
    "PATTACK0": "#PATTACK0 9 1 6 0",
    "UATTACK0": "#UATTACK0 9 1 6 0",
    "ATTACK": "@ATTACK 9 6 0 53",
    "PATTACK1": "#PATTACK1 9 1 3 0",
    "UATTACK1": "#UATTACK1 9 1 3 0",
    "ATTACK1": "@ATTACK1 9 3 0 85",
    "DEATH": "@DEATH 9 4 0 40",
    "PSTAND": "#PSTAND 9 1 6 0",
    "PSTAND1": "#PSTAND1 9 1 3 0",
    "DEATHLIE1": "#DEATHLIE1 9 1 4 40",
}
WEAPONS = {0: 'MECH', 1: 'MEDIK'}
ACTIVE = {'ATTACK1': 1}

SPEC = meso.spec(MD, weapons=WEAPONS, active=ACTIVE, dress=("maxtlatl", "tilmatli", "sandals"), head="band", hand_r="macuahuitl", shield=None, melee="club",
                 paint="black_face_half", palette={"tilmatli": (0.15, 0.12, 0.10)})


def build(ctx):
    meso.build(ctx, SPEC)
