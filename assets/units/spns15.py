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
    # The reload stance of a SLOWRECHARGE musketeer (#255, `poses.STANCE4_CLIPS`).
    "PSTAND3": "#PSTAND3 9 1 3 82",
    "PATTACK3": "#PATTACK3 9 10 3 119 3 118 3 117 3 116 3 115 3 114 3 113 3 112 3 111 3 110",
    "UATTACK3": "#UATTACK3 9 9 3 111 3 112 3 113 3 114 3 115 3 116 3 117 3 118 3 119",
    "ATTACK3": "#ATTACK3 9 132 3 109 3 108 3 107 3 106 3 105 3 104 3 103 3 102 3 101 3 100 3 99 3 98 3 97 3 96 3 95 3 94 3 93 3 92 3 91 3 90 3 89 3 88 3 87 3 86 3 85 3 84 3 83 3 82 3 81 3 80 3 79 3 78 3 77 3 76 3 75 3 74 3 73 3 72 3 71 3 70 3 69 3 68 3 67 3 66 3 65 3 64 3 63 3 62 3 61 3 60 3 59 3 58 3 57 3 56 3 55 3 54 3 53 3 52 3 51 3 50 3 49 3 48 3 47 3 46 3 45 3 44 3 45 3 46 3 47 3 48 3 49 3 50 3 51 3 52 3 53 3 54 3 55 3 56 3 57 3 58 3 59 3 60 3 61 3 62 3 63 3 64 3 65 3 66 3 67 3 68 3 69 3 70 3 71 3 72 3 73 3 74 3 75 3 76 3 77 3 78 3 79 3 80 3 81 3 82 3 83 3 84 3 85 3 86 3 87 3 88 3 89 3 90 3 91 3 92 3 93 3 94 3 95 3 96 3 97 3 98 3 99 3 100 3 101 3 102 3 103 3 104 3 105 3 106 3 107 3 108 3 109 3 110",
    "TRANSX3": "#TRANSX3 9 10 3 119 3 118 3 117 3 116 3 115 3 114 3 113 3 112 3 111 3 110",
    "TRANS3X": "#TRANS3X 9 9 3 111 3 112 3 113 3 114 3 115 3 116 3 117 3 118 3 119",
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
