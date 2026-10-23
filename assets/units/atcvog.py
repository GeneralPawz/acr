"""Aztec chief (`AtcVog`, `ATCVOG.MD`, ACTEC): a jaguar knight (ocelotl) in the full
tlahuiztli suit of jaguar pelt, the jaguar-head helmet with his face between the jaws,
maxtlatl, sandals, a chimalli with the stepped fret and the macuahuitl (MECH: the hack).
"""

from acr_assets import meso

TYPE = "AtcVog"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

MD = {
    "STAND": "#STAND 9 1 3 0",
    "MOTION_L": "@MOTION_L 9 1 0 9",
    "MOTION_R": "@MOTION_R 9 1 10 19",
    "MOTION_LB": "@MOTION_LB 9 1 19 10",
    "MOTION_RB": "@MOTION_RB 9 1 9 0",
    "ATTACK": "@ATTACK 9 3 0 53",
    "PATTACK": "#PATTACK 9 1 3 0",
    "UATTACK": "#UATTACK 9 1 3 0",
    "DEATH": "@DEATH 9 4 0 58",
    "DEATHLIE1": "#DEATHLIE1 9 1 4 58",
}
WEAPONS = {0: 'MECH'}
ACTIVE = {}

SPEC = meso.spec(MD, weapons=WEAPONS, active=ACTIVE, dress=("tlahuiztli", "maxtlatl", "sandals"), suit="jaguar", head="jaguar", hand_r="macuahuitl", shield="fret",
                 melee="club")


def build(ctx):
    meso.build(ctx, SPEC)
