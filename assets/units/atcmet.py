"""Aztec spear thrower (`AtcMet`, `ATCMET.MD`, ACTEC): a commoner warrior in the quilted cotton
ichcahuipilli over the maxtlatl, sandals, hair tied up in the temillotl; an atlatl in the
right hand launches the darts (KOPIE, ATTACK1: the throw, the dart leaves on frame 28 of
SETACTIVEPOINT); in melee (PIKA) he thrusts with a dart.
"""

from acr_assets import meso

TYPE = "AtcMet"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

MD = {
    "STAND": "#STAND 9 1 1 0",
    "PSTAND": "#PSTAND 9 1 1 0",
    "PSTAND1": "#PSTAND1 9 1 1 0",
    "MOTION_L": "@MOTION_L 9 0 0 7",
    "MOTION_R": "@MOTION_R 9 0 8 15",
    "MOTION_RB": "@MOTION_RB 9 0 15 8",
    "MOTION_LB": "@MOTION_LB 9 0 7 0",
    "MOTION_L0": "@MOTION_L0 9 0 0 7",
    "MOTION_R0": "@MOTION_R0 9 0 8 15",
    "MOTION_LB0": "@MOTION_LB0 9 0 15 8",
    "MOTION_RB0": "@MOTION_RB0 9 0 7 0",
    "ATTACK1": "@ATTACK1 9 1 0 54",
    "ATTACK": "@ATTACK 9 2 0 53",
    "DEATH": "@DEATH 9 3 0 47",
    "DEATHLIE1": "#DEATHLIE1 9 1 3 47",
    "DEATHLIE2": "#DEATHLIE2 9 1 3 47",
    "DEATHLIE3": "#DEATHLIE3 9 1 3 47",
}
WEAPONS = {0: 'PIKA', 1: 'KOPIE'}
ACTIVE = {'ATTACK1': 28}

SPEC = meso.spec(MD, weapons=WEAPONS, active=ACTIVE, dress=("maxtlatl", "ichcahuipilli", "sandals"), head="topknot", hand_r="dart", atlatl=True, shield=None,
                 melee="spear")


def build(ctx):
    meso.build(ctx, SPEC)
