"""Inca slinger (`IncPra`, `INCPRA.MD`, INCAS): Guaman Poma's soldier in the army's
checkerboard unku with the red stepped yoke, the llautu round his cropped hair, sandals,
the sling (huaraca) in the right hand and a small round shield on his back. The sling
(KAMEN, ATTACK1: whirled over the head, the stone leaves on frame 56 of SETACTIVEPOINT);
in melee (MECH) he smashes with the loaded sling.
"""

from acr_assets import inca

TYPE = "IncPra"
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
    "ATTACK1": "@ATTACK1 9 1 0 67",
    "ATTACK": "@ATTACK 9 2 0 60",
    "DEATH": "@DEATH 9 3 0 48",
    "DEATHLIE1": "#DEATHLIE1 9 1 3 48",
    "DEATHLIE2": "#DEATHLIE2 9 1 3 48",
    "DEATHLIE3": "#DEATHLIE3 9 1 3 48",
}
WEAPONS = {0: 'MECH', 1: 'KAMEN'}
ACTIVE = {'ATTACK1': 56}

SPEC = inca.spec(MD, weapons=WEAPONS, active=ACTIVE, unku="check", head="llautu", hand_r="sling", shield=True, melee="club",
                 palette={"unku_check": (0.80, 0.08, 0.06), "llautu": (0.85, 0.10, 0.06)})


def build(ctx):
    inca.build(ctx, SPEC)
