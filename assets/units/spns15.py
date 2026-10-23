"""Spanish arquebusier, 16th century (`SpnS15`, `SPNS15.MD`, `MECH`, `VISTREL`): a steel
breastplate over an orange doublet with red sleeves, ochre slops, red hose, a cabasset, the
sword on the left hip and the arquebus.

Family 12 (`acr_assets.conquista`, docs/families/conquista.md).
"""

from acr_assets import conquista, frontier, line

TYPE = "SpnS15"
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
    "ATTACK": "@ATTACK 9 6 0 81",
    "PATTACK1": "#PATTACK1 9 1 3 0",
    "UATTACK1": "#UATTACK1 9 1 3 0",
    "ATTACK1": "@ATTACK1 9 3 0 109",
    "DEATH": "@DEATH 9 4 0 33",
    "PSTAND": "#PSTAND 9 1 6 0",
    "PSTAND1": "#PSTAND1 9 1 3 0",
    "DEATHLIE1": "#DEATHLIE1 9 1 4 33",
}


def spec():
    return conquista.spec(MD, palette={"doublet": (0.75, 0.40, 0.18), "dsleeve": (0.66, 0.16, 0.08), "fhose": (0.62, 0.10, 0.07),
                                       "slops": (0.78, 0.60, 0.20), "breeches": (0.60, 0.45, 0.16)}, hat="cabasset")


def build(ctx):
    frontier.build(ctx, spec(), make_fn=line.with_sidearm(frontier.make, "sword"))


def pose_for(ch, clip):
    conquista.pose_for(ch, spec(), clip)
