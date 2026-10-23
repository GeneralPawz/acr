"""French arquebusier, 16th century (`FrnS15`, `FRNS15.MD`, `MECH`, `VISTREL`): no armour, a
dark red doublet, yellow slops and hose (the sprite's striped Landsknecht slops), a red flat
cap, the sword on the left hip and the arquebus.

Family 12 (`acr_assets.conquista`, docs/families/conquista.md).
"""

from acr_assets import conquista

TYPE = "FrnS15"
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
    "PATTACK": "#PATTACK 9 1 6 0",
    "UATTACK": "#UATTACK 9 1 6 0",
    "ATTACK": "@ATTACK 9 6 0 51",
    "PATTACK1": "#PATTACK1 9 1 3 0",
    "UATTACK1": "#UATTACK1 9 1 3 0",
    "ATTACK1": "@ATTACK1 9 3 0 81",
    "DEATH": "@DEATH 9 4 0 33",
    "PSTAND": "#PSTAND 9 1 6 0",
    "PSTAND1": "#PSTAND1 9 1 3 0",
    "DEATHLIE1": "#DEATHLIE1 9 1 4 33",
}


def spec():
    return conquista.spec(MD, palette={"doublet": (0.40, 0.14, 0.08), "dsleeve": (0.45, 0.16, 0.08), "fhose": (0.70, 0.56, 0.16),
                                       "slops": (0.84, 0.68, 0.14), "breeches": (0.20, 0.16, 0.10), "hat": (0.62, 0.10, 0.06)},
                              layers=("blouse", "slops"), hat="flat_cap", hat_at=(0.0, 0.0, -0.01, -4))


def build(ctx):
    conquista.build(ctx, spec())


def pose_for(ch, clip):
    conquista.pose_for(ch, spec(), clip)
