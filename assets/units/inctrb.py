"""Inca blowpipe warrior (`IncTrb`, `INCTRB.MD`, INCAS): a man of the forest edge in a plain
unku with a zigzag hem, the llautu with a few feathers, sandals and the long cane blowpipe.
The blowpipe (VAFLIA, ATTACK1: the dart leaves on frame 17 of SETACTIVEPOINT); in melee
(MECH) he swings the pipe.
"""

from acr_assets import inca

TYPE = "IncTrb"
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
    "PATTACK": "#PATTACK 9 1 6 0",
    "UATTACK": "#UATTACK 9 1 6 0",
    "ATTACK": "@ATTACK 9 6 0 60",
    "PATTACK1": "#PATTACK1 9 1 3 0",
    "UATTACK1": "#UATTACK1 9 1 3 0",
    "ATTACK1": "@ATTACK1 9 3 0 60",
    "DEATH": "@DEATH 9 4 0 34",
    "PSTAND": "#PSTAND 9 1 6 0",
    "PSTAND1": "#PSTAND1 9 1 3 0",
    "DEATHLIE1": "#DEATHLIE1 9 1 4 34",
}
WEAPONS = {0: 'MECH', 1: 'VAFLIA'}
ACTIVE = {'ATTACK1': 17}

SPEC = inca.spec(MD, weapons=WEAPONS, active=ACTIVE, unku="plain", head="llautu", plumes=4, plume_len=0.18, hand_r="blowpipe", melee="club",
                 palette={"unku_plain": (0.15, 0.48, 0.30), "llautu": (0.90, 0.65, 0.10), "inca_plumes": (0.85, 0.15, 0.08)})


def build(ctx):
    inca.build(ctx, SPEC)
