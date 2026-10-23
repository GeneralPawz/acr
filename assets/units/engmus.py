"""English musketeer (`EngMus`, `ENGMUS.MD`, `0 MECH`, `1 VISTREL`): red coat, grey breeches, broad felt hat, the bandolier of twelve apostles over the left shoulder, a sword on a buff baldric, a matchlock and the lit match in the left hand.

Family 6, pike and shot (`acr_assets.pike`, docs/families/pike.md).
"""

from acr_assets import pike

TYPE = "EngMus"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

MD = {
    "STAND": "#STAND 9 1 2 0",
    "PSTAND": "#PSTAND 9 1 4 0",
    "PSTAND1": "#PSTAND1 9 1 2 62",
    "MOTION_L": "@MOTION_L 9 0 0 9",
    "MOTION_R": "@MOTION_R 9 0 10 19",
    "MOTION_RB": "@MOTION_RB 9 0 19 10",
    "MOTION_LB": "@MOTION_LB 9 0 9 0",
    "PATTACK": "#PATTACK 9 1 4 0",
    "UATTACK": "#UATTACK 9 1 4 0",
    "ATTACK": "@ATTACK 9 4 0 52",
    "PATTACK1": "@PATTACK1 9 2 0 62",
    "UATTACK1": "@UATTACK1 9 2 62 0",
    "ATTACK1": "@ATTACK1 9 1 0 67",
    "DEATH": "@DEATH 9 3 0 39",
    "DEATHLIE1": "#DEATHLIE1 9 1 3 39",
    "DEATHLIE2": "#DEATHLIE2 9 1 3 39",
    "DEATHLIE3": "#DEATHLIE3 9 1 3 39",
    "PSTAND3": "#PSTAND3 9 1 1 68",
    "PATTACK3": "#PATTACK3 9 99 2 0 2 1 2 2 2 3 2 4 2 5 2 6 2 7 2 8 2 9 2 10 2 11 2 12 2 13 2 14 2 15 2 16 2 17 2 18 2 19 2 20 2 21 2 22 2 23 2 24 2 25 2 26 2 27 2 28 2 29 2 30 2 31 2 32 2 33 2 34 2 35 2 36 2 37 2 38 2 39 2 40 2 41 2 42 2 43 2 44 2 45 2 46 2 47 2 48 2 49 2 50 2 51 2 52 2 53 2 54 2 55 2 56 2 57 2 58 2 59 2 60 2 61 2 62 1 119 1 118 1 117 1 116 1 115 1 114 1 113 1 112 1 111 1 110 1 109 1 108 1 107 1 106 1 105 1 104 1 103 1 102 1 101 1 100 1 99 1 98 1 97 1 96 1 95 1 94 1 93 1 92 1 91 1 90 1 89 1 88 1 87 1 86 1 85 1 84",
    "UATTACK3": "#UATTACK3 9 99 1 84 1 85 1 86 1 87 1 88 1 89 1 90 1 91 1 92 1 93 1 94 1 95 1 96 1 97 1 98 1 99 1 100 1 101 1 102 1 103 1 104 1 105 1 106 1 107 1 108 1 109 1 110 1 111 1 112 1 113 1 114 1 115 1 116 1 117 1 118 1 119 2 62 2 61 2 60 2 59 2 58 2 57 2 56 2 55 2 54 2 53 2 52 2 51 2 50 2 49 2 48 2 47 2 46 2 45 2 44 2 43 2 42 2 41 2 40 2 39 2 38 2 37 2 36 2 35 2 34 2 33 2 32 2 31 2 30 2 29 2 28 2 27 2 26 2 25 2 24 2 23 2 22 2 21 2 20 2 19 2 18 2 17 2 16 2 15 2 14 2 13 2 12 2 11 2 10 2 9 2 8 2 7 2 6 2 5 2 4 2 3 2 2 2 1 2 0",
    "ATTACK3": "#ATTACK3 9 34 1 68 1 69 1 70 1 71 1 72 1 73 1 74 1 75 1 76 1 77 1 78 1 79 1 80 1 81 1 82 1 83 1 84 1 83 1 82 1 81 1 80 1 79 1 78 1 77 1 76 1 75 1 74 1 73 1 72 1 71 1 70 1 69 1 68 1 67",
    "TRANSX3": "#TRANSX3 9 36 1 119 1 118 1 117 1 116 1 115 1 114 1 113 1 112 1 111 1 110 1 109 1 108 1 107 1 106 1 105 1 104 1 103 1 102 1 101 1 100 1 99 1 98 1 97 1 96 1 95 1 94 1 93 1 92 1 91 1 90 1 89 1 88 1 87 1 86 1 85 1 84",
    "TRANS3X": "#TRANS3X 9 36 1 84 1 85 1 86 1 87 1 88 1 89 1 90 1 91 1 92 1 93 1 94 1 95 1 96 1 97 1 98 1 99 1 100 1 101 1 102 1 103 1 104 1 105 1 106 1 107 1 108 1 109 1 110 1 111 1 112 1 113 1 114 1 115 1 116 1 117 1 118 1 119",
}


def spec():
    return pike.musketeer_spec(MD, palette={"coat": (0.62, 0.10, 0.07), "breeches": (0.45, 0.44, 0.42), "gaiters": (0.50, 0.48, 0.45)})


def make(root, name, build=None):
    s = spec()
    return pike.make(root, name, dict(s, build=build or s["build"]))


def build(ctx):
    pike.build(ctx, spec())


def pose_for(ch, clip):
    pike.pose_for(ch, spec(), clip)
