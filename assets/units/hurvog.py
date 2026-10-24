"""Huron chief (`HurVog`, `HURVOG.MD`): Woodlands (`acr_assets.native`,
docs/families/native.md): a crown of upright feathers on a band,
silver gorget and arm bands, a fine scarlet matchcoat, a green breechcloth, blue leggings, moccasins;
a trade gun with the strap, powder horn and shot pouch.
"""

from acr_assets import native

TYPE = "HurVog"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

# `#` lists keep only their frame count (the full lists in HURVOG.MD).
MD = {
    "STAND": "#STAND 9 1 3 0",
    "MOTION_L": "@MOTION_L 9 1 0 8",
    "MOTION_R": "@MOTION_R 9 1 9 15",
    "MOTION_LB": "@MOTION_LB 9 1 15 9",
    "MOTION_RB": "@MOTION_RB 9 1 8 0",
    "PATTACK": "#PATTACK 9 1 6 0",
    "UATTACK": "#UATTACK 9 1 6 0",
    "ATTACK": "@ATTACK 9 6 0 60",
    "PATTACK1": "#PATTACK1 9 1 3 0",
    "UATTACK1": "#UATTACK1 9 1 3 0",
    "ATTACK1": "@ATTACK1 9 3 0 49",
    # The reload stance of a SLOWRECHARGE musketeer (#255, `poses.STANCE4_CLIPS`).
    "PSTAND3": "#PSTAND3 9 1 3 103",
    "PATTACK3": "#PATTACK3 9 22 3 149 3 148 3 147 3 146 3 145 3 144 3 143 3 142 3 141 3 140 3 139 3 138 3 137 3 136 3 135 3 134 3 133 3 132 3 131 3 130 3 129 3 128",
    "UATTACK3": "#UATTACK3 9 22 3 128 3 129 3 130 3 131 3 132 3 133 3 134 3 135 3 136 3 137 3 138 3 139 3 140 3 141 3 142 3 143 3 144 3 145 3 146 3 147 3 148 3 149",
    "ATTACK3": "#ATTACK3 9 160 3 50 3 51 3 52 3 53 3 54 3 55 3 56 3 57 3 58 3 59 3 60 3 61 3 62 3 63 3 64 3 65 3 66 3 67 3 68 3 69 3 70 3 71 3 72 3 73 3 74 3 75 3 76 3 77 3 78 3 79 3 80 3 81 3 82 3 83 3 84 3 85 3 86 3 87 3 88 3 89 3 90 3 91 3 92 3 93 3 94 3 95 3 96 3 97 3 98 3 99 3 100 3 101 3 102 3 103 3 104 3 105 3 106 3 107 3 108 3 109 3 110 3 111 3 112 3 113 3 114 3 115 3 116 3 117 3 118 3 119 3 120 3 121 3 122 3 123 3 124 3 125 3 126 3 127 3 128 3 129 3 128 3 127 3 126 3 125 3 124 3 123 3 122 3 121 3 120 3 119 3 118 3 117 3 116 3 115 3 114 3 113 3 112 3 111 3 110 3 109 3 108 3 107 3 106 3 105 3 104 3 103 3 102 3 101 3 100 3 99 3 98 3 97 3 96 3 95 3 94 3 93 3 92 3 91 3 90 3 89 3 88 3 87 3 86 3 85 3 84 3 83 3 82 3 81 3 80 3 79 3 78 3 77 3 76 3 75 3 74 3 73 3 72 3 71 3 70 3 69 3 68 3 67 3 66 3 65 3 64 3 63 3 62 3 61 3 60 3 59 3 58 3 57 3 56 3 55 3 54 3 53 3 52 3 51 3 50 3 49",
    "TRANSX3": "#TRANSX3 9 22 3 149 3 148 3 147 3 146 3 145 3 144 3 143 3 142 3 141 3 140 3 139 3 138 3 137 3 136 3 135 3 134 3 133 3 132 3 131 3 130 3 129 3 128",
    "TRANS3X": "#TRANS3X 9 22 3 128 3 129 3 130 3 131 3 132 3 133 3 134 3 135 3 136 3 137 3 138 3 139 3 140 3 141 3 142 3 143 3 144 3 145 3 146 3 147 3 148 3 149",
    "DEATH": "@DEATH 9 4 0 39",
    "PSTAND": "#PSTAND 9 1 6 0",
    "PSTAND1": "#PSTAND1 9 1 3 0",
    "DEATHLIE1": "#DEATHLIE1 9 1 4 39",
}
WEAPONS = {0: "MECH", 1: "VISTREL"}
ACTIVE = {"ATTACK1": 15}
PALETTE = {"breechcloth": (0.16, 0.34, 0.2), "leggings": (0.14, 0.18, 0.36), "flaps": (0.14, 0.18, 0.36), "skin": (0.78, 0.53, 0.36), "matchcoat": (0.62, 0.1, 0.08)}


def spec():
    return native.spec(MD, WEAPONS, palette=PALETTE, active=ACTIVE, hair="lock", feathers=0, headdress="crown", silver=True, paint="black_red_chin", dress=("breechcloth", "leggings", "moccasins", "matchcoat"), kit=("belt_l", "horn", "pouch", "knife"), weapon="gun", melee=None)


def build(ctx):
    native.build(ctx, spec())


def make(root, name, build=None):
    s = spec()
    return native.make(root, name, dict(s, build=build or s["build"]))


def pose_for(ch, clip):
    native.pose_for(ch, spec(), clip)
