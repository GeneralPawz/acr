"""Maya macuahuitl warrior (`MayVoi`, `MAYVOI.MD`, MAY): the Bonampak warrior in a sleeveless
quilted cotton jacket over a white hip cloth, sandals, a turban with a tall feather frame,
the macuahuitl (obsidian-edged club) in the right hand and a chimalli with feather down
balls on the left arm; red and black body paint. Melee (MECH): the macuahuitl's hack.
"""

from acr_assets import meso

TYPE = "MayVoi"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

MD = {
    "STAND": "#STAND 9 1 3 0",
    "MOTION_L": "@MOTION_L 9 1 0 7",
    "MOTION_R": "@MOTION_R 9 1 8 15",
    "MOTION_LB": "@MOTION_LB 9 1 15 8",
    "MOTION_RB": "@MOTION_RB 9 1 7 0",
    "MOTION_L0": "@MOTION_L0 9 1 0 7",
    "MOTION_R0": "@MOTION_R0 9 1 8 15",
    "MOTION_LB0": "@MOTION_LB0 9 1 15 8",
    "MOTION_RB0": "@MOTION_RB0 9 1 7 0",
    "ATTACK": "@ATTACK 9 3 0 50",
    "PATTACK": "#PATTACK 9 1 3 0",
    "UATTACK": "#UATTACK 9 1 3 0",
    "DEATH": "@DEATH 9 4 0 60",
    "DEATHLIE1": "#DEATHLIE1 9 1 4 60",
}
WEAPONS = {0: 'MECH'}
ACTIVE = {}

SPEC = meso.spec(MD, weapons=WEAPONS, active=ACTIVE, dress=("hipcloth", "maya_jacket", "sandals"), head="maya", hand_r="macuahuitl", shield="rings", melee="club",
                 paint="red_black", palette={"maya_jacket": (0.80, 0.62, 0.40), "plumes": (0.12, 0.55, 0.40)})


def build(ctx):
    meso.build(ctx, SPEC)
