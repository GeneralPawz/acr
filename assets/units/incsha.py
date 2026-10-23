"""Inca priest (`IncSha`, `INCSHA.MD`, INCAS): a white unku with a tocapu band under a black
yacolla, gold ear spools, the llautu with a fan of feathers, sandals and the champi (the
star-headed mace). Heals (MEDIK, ATTACK1); melee (MECH): the champi's smash.
"""

from acr_assets import inca

TYPE = "IncSha"
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
    "ATTACK": "@ATTACK 9 6 0 47",
    "PATTACK1": "#PATTACK1 9 1 3 0",
    "UATTACK1": "#UATTACK1 9 1 3 0",
    "ATTACK1": "@ATTACK1 9 3 0 80",
    "DEATH": "@DEATH 9 4 0 50",
    "PSTAND": "#PSTAND 9 1 6 0",
    "PSTAND1": "#PSTAND1 9 1 3 0",
    "DEATHLIE1": "#DEATHLIE1 9 1 4 50",
}
WEAPONS = {0: 'MECH', 1: 'MEDIK'}
ACTIVE = {'ATTACK1': 1}

SPEC = inca.spec(MD, weapons=WEAPONS, active=ACTIVE, unku="tocapu", yacolla=True, drape=True, head="llautu", plumes=7, plume_len=0.24, spools=True, hand_r="champi",
                 melee="club", palette={"unku_tocapu": (0.90, 0.88, 0.80), "yacolla": (0.07, 0.06, 0.06),
                                         "inca_plumes": (0.95, 0.75, 0.10)})


def build(ctx):
    inca.build(ctx, SPEC)
