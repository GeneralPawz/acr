"""French musketeer, 17th century (`FrnS17`, `FRNS17.MD`, `0 MECH`, `1 VISTREL`): the grey-white coat, blue breeches, red stockings, broad felt hat, the bandolier, a sword on its baldric, matchlock and match.

Family 6, pike and shot (`acr_assets.pike`, docs/families/pike.md).
"""

from acr_assets import pike

TYPE = "FrnS17"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

MD = {
    "STAND": "#STAND 9 1 2 0",
    "PSTAND1": "#PSTAND1 9 1 4 0",
    "REST": "@REST 9 2 0 66",
    "PSTAND": "#PSTAND 9 1 4 0",
    "MOTION_L": "@MOTION_L 9 0 0 9",
    "MOTION_R": "@MOTION_R 9 0 10 19",
    "MOTION_RB": "@MOTION_RB 9 0 19 10",
    "MOTION_LB": "@MOTION_LB 9 0 9 0",
    "MOTION_L0": "@MOTION_L0 9 0 0 9",
    "MOTION_R0": "@MOTION_R0 9 0 10 19",
    "MOTION_RB0": "@MOTION_RB0 9 0 19 10",
    "MOTION_LB0": "@MOTION_LB0 9 0 9 0",
    "ATTACK1": "@ATTACK1 9 1 0 86",
    "ATTACK": "@ATTACK 9 4 0 52",
    "PATTACK": "#PATTACK 9 1 4 0",
    "UATTACK": "#UATTACK 9 1 4 0",
    "PATTACK1": "#PATTACK1 9 1 1 0",
    "UATTACK1": "#UATTACK1 9 1 1 0",
    "PSTAND3": "#PSTAND3 9 1 1 87",
    "PATTACK3": "#PATTACK3 9 20 1 148 1 147 1 146 1 145 1 144 1 143 1 142 1 141 1 140 1 139 1 138 1 137 1 136 1 135 1 134 1 133 1 132 1 131 1 130 1 129",
    "UATTACK3": "#UATTACK3 9 20 1 129 1 130 1 131 1 132 1 133 1 134 1 135 1 136 1 137 1 138 1 139 1 140 1 141 1 142 1 143 1 144 1 145 1 146 1 147 1 148",
    "ATTACK3": "#ATTACK3 9 82 1 87 1 88 1 89 1 90 1 91 1 92 1 93 1 94 1 95 1 96 1 97 1 98 1 99 1 100 1 101 1 102 1 103 1 104 1 105 1 106 1 107 1 108 1 109 1 110 1 111 1 112 1 113 1 114 1 115 1 116 1 117 1 118 1 119 1 120 1 121 1 122 1 123 1 124 1 125 1 126 1 127 1 128 1 127 1 126 1 125 1 124 1 123 1 122 1 121 1 120 1 119 1 118 1 117 1 116 1 115 1 114 1 113 1 112 1 111 1 110 1 109 1 108 1 107 1 106 1 105 1 104 1 103 1 102 1 101 1 100 1 99 1 98 1 97 1 96 1 95 1 94 1 93 1 92 1 91 1 90 1 89 1 88",
    "TRANSX3": "#TRANSX3 9 20 1 148 1 147 1 146 1 145 1 144 1 143 1 142 1 141 1 140 1 139 1 138 1 137 1 136 1 135 1 134 1 133 1 132 1 131 1 130 1 129",
    "TRANS3X": "#TRANS3X 9 20 1 129 1 130 1 131 1 132 1 133 1 134 1 135 1 136 1 137 1 138 1 139 1 140 1 141 1 142 1 143 1 144 1 145 1 146 1 147 1 148",
    "DEATH": "@DEATH 9 3 0 33",
    "DEATHLIE1": "#DEATHLIE1 9 1 3 33",
    "DEATHLIE2": "#DEATHLIE2 9 1 3 33",
    "DEATHLIE3": "#DEATHLIE3 9 1 3 33",
}


def spec():
    return pike.musketeer_spec(MD, palette={"coat": (0.74, 0.72, 0.66), "breeches": (0.20, 0.24, 0.42), "gaiters": (0.55, 0.12, 0.10)}, sidearm="sword")


def make(root, name, build=None):
    s = spec()
    return pike.make(root, name, dict(s, build=build or s["build"]))


def build(ctx):
    pike.build(ctx, spec())


def pose_for(ch, clip):
    pike.pose_for(ch, spec(), clip)
