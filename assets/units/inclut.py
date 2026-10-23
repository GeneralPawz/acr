"""Inca archer (`IncLut`, `INCLUT.MD`, INCAS, TESCOCO): a forest bowman in a plain yellow
unku with a zigzag hem, the llautu, sandals, the bow in the left hand and the quiver on his
back (OSTRELA, STRELA: the arrow leaves on frame 24 of SETACTIVEPOINT); melee (MECH).
"""

from acr_assets import inca

TYPE = "IncLut"
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
    "ATTACK2": "@ATTACK2 9 1 0 58",
    "ATTACK1": "@ATTACK1 9 1 0 58",
    "ATTACK": "@ATTACK 9 2 0 60",
    "DEATH": "@DEATH 9 3 0 40",
    "DEATHLIE1": "#DEATHLIE1 9 1 3 40",
    "DEATHLIE2": "#DEATHLIE2 9 1 3 40",
    "DEATHLIE3": "#DEATHLIE3 9 1 3 40",
}
WEAPONS = {0: 'MECH', 1: 'OSTRELA', 2: 'STRELA'}
ACTIVE = {'ATTACK1': 24, 'ATTACK2': 24}

SPEC = inca.spec(MD, weapons=WEAPONS, active=ACTIVE, unku="plain", head="llautu", plumes=2, plume_len=0.16, hand_r=None, bow=True, melee="club",
                 palette={"unku_plain": (0.90, 0.68, 0.12), "llautu": (0.80, 0.10, 0.06)})


def build(ctx):
    inca.build(ctx, SPEC)
