"""Inca spearman (`IncPik`, `INCPIK.MD`, INCAS, TESCOCO): quilted cotton armour over a red
unku with a tocapu band, a cane helmet with a crest of feathers, sandals, the long spear
with a bronze point and the small shield on his back. Melee (PIKA): the thrust.
"""

from acr_assets import inca

TYPE = "IncPik"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

MD = {
    "STAND": "#STAND 9 1 3 41",
    "MOTION_L": "@MOTION_L 9 1 0 7",
    "MOTION_R": "@MOTION_R 9 1 8 15",
    "MOTION_LB": "@MOTION_LB 9 1 15 8",
    "MOTION_RB": "@MOTION_RB 9 1 7 0",
    "MOTION_L0": "@MOTION_L0 9 1 0 7",
    "MOTION_R0": "@MOTION_R0 9 1 8 15",
    "MOTION_LB0": "@MOTION_LB0 9 1 15 8",
    "MOTION_RB0": "@MOTION_RB0 9 1 7 0",
    "ATTACK": "@ATTACK 9 3 0 45",
    "DEATH": "@DEATH 9 4 0 71",
    "DEATHLIE1": "#DEATHLIE1 9 1 4 71",
}
WEAPONS = {0: 'PIKA'}
ACTIVE = {}

SPEC = inca.spec(MD, weapons=WEAPONS, active=ACTIVE, unku="tocapu", quilt=True, head="helmet", plumes=7, plume_len=0.20, hand_r="spear", shield=True,
                 melee="spear", palette={"unku_tocapu": (0.78, 0.12, 0.08), "inca_plumes": (0.90, 0.20, 0.06)})


def build(ctx):
    inca.build(ctx, SPEC)
