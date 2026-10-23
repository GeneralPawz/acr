"""French pikeman (`FrnPik`, `FRNPIK.MD`, `0 PIKA`): the grey-white coat of the French foot, blue breeches, red stockings, a combed morion, back and breast with tassets, a sword at the hip and a 16 ft pike.

Family 6, pike and shot (`acr_assets.pike`, docs/families/pike.md).
"""

from acr_assets import pike

TYPE = "FrnPik"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

MD = {
    "STAND": "#STAND 9 1 5 0",
    "PSTAND": "#PSTAND 9 1 5 35",
    "MOTION_L": "@MOTION_L 9 1 0 9",
    "MOTION_R": "@MOTION_R 9 1 10 19",
    "MOTION_LB": "@MOTION_LB 9 1 19 10",
    "MOTION_RB": "@MOTION_RB 9 1 9 0",
    "MOTION_L0": "@MOTION_L0 9 2 0 9",
    "MOTION_R0": "@MOTION_R0 9 2 10 19",
    "MOTION_LB0": "@MOTION_LB0 9 2 19 10",
    "MOTION_RB0": "@MOTION_RB0 9 2 9 0",
    "ATTACK": "@ATTACK 9 3 0 20",
    "PATTACK": "@PATTACK 9 5 0 35",
    "UATTACK": "@UATTACK 9 5 35 0",
    "DEATH": "@DEATH 9 4 0 70",
    "DEATHLIE1": "#DEATHLIE1 9 1 4 70",
}


def spec():
    return pike.pikeman_spec(MD, palette={"coat": (0.74, 0.72, 0.66), "breeches": (0.20, 0.24, 0.42), "gaiters": (0.55, 0.12, 0.10)}, hat="pike_morion")


def make(root, name, build=None):
    s = spec()
    return pike.make(root, name, dict(s, build=build or s["build"]))


def build(ctx):
    pike.build(ctx, spec())


def pose_for(ch, clip):
    pike.pose_for(ch, spec(), clip)
