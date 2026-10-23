"""Inca chief (`IncVog`, `INCVOG.MD`, INCAS): a lord in an unku of tocapu all over and the
yacolla, gold ear spools, the llautu with a tall fan of feathers, sandals, the spear and the
shield on his back. Melee (PIKA): the thrust.
"""

from acr_assets import inca

TYPE = "IncVog"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

MD = {
    "STAND": "#STAND 9 1 4 0",
    "MOTION_L": "@MOTION_L 9 1 0 7",
    "MOTION_R": "@MOTION_R 9 1 8 15",
    "MOTION_LB": "@MOTION_LB 9 1 15 8",
    "MOTION_RB": "@MOTION_RB 9 1 7 0",
    "ATTACK": "@ATTACK 9 3 0 47",
    "PATTACK": "#PATTACK 9 1 3 0",
    "UATTACK": "#UATTACK 9 1 3 0",
    "DEATH": "@DEATH 9 4 0 68",
    "DEATHLIE1": "#DEATHLIE1 9 1 4 68",
}
WEAPONS = {0: 'PIKA'}
ACTIVE = {}

SPEC = inca.spec(MD, weapons=WEAPONS, active=ACTIVE, unku="royal", yacolla=True, head="llautu", plumes=11, plume_len=0.32, spools=True, hand_r="spear",
                 shield=True, melee="spear", palette={"unku_royal": (0.75, 0.10, 0.10), "yacolla": (0.18, 0.12, 0.30),
                                                    "inca_plumes": (0.85, 0.12, 0.06)})


def build(ctx):
    inca.build(ctx, SPEC)
