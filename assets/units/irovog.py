"""Iroquois chief (`IroVog`, `IROVOG.MD`): Woodlands (`acr_assets.native`,
docs/families/native.md): a gustoweh (a fitted cap on a splint band,
short feathers over the crown and an eagle feather standing up), silver gorget and arm bands, a fine
blue matchcoat, breechcloth, red leggings, moccasins; a trade gun with the strap, powder horn and shot
pouch.
"""

from acr_assets import native

TYPE = "IroVog"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

# `#` lists keep only their frame count (the full lists in IROVOG.MD).
MD = {
    "STAND": "#STAND 9 1 3 0",
    "MOTION_L": "@MOTION_L 9 1 0 8",
    "MOTION_R": "@MOTION_R 9 1 9 15",
    "MOTION_LB": "@MOTION_LB 9 1 15 9",
    "MOTION_RB": "@MOTION_RB 9 1 8 0",
    "PATTACK": "#PATTACK 9 1 6 0",
    "UATTACK": "#UATTACK 9 1 6 0",
    "ATTACK": "@ATTACK 9 6 0 57",
    "ATTACK1": "@ATTACK1 9 3 0 87",
    # The reload stance of a SLOWRECHARGE musketeer (#255, `poses.STANCE4_CLIPS`).
    "PSTAND3": "#PSTAND3 9 1 3 135",
    "PATTACK3": "#PATTACK3 9 20 3 149 3 148 3 147 3 146 3 145 3 144 3 143 3 142 3 141 3 140 3 139 3 138 3 137 3 136 3 135 3 134 3 133 3 132 3 131 3 130",
    "UATTACK3": "#UATTACK3 9 20 3 130 3 131 3 132 3 133 3 134 3 135 3 136 3 137 3 138 3 139 3 140 3 141 3 142 3 143 3 144 3 145 3 146 3 147 3 148 3 149",
    "ATTACK3": "#ATTACK3 9 88 3 87 3 88 3 89 3 90 3 91 3 92 3 93 3 94 3 95 3 96 3 97 3 98 3 99 3 100 3 101 3 102 3 103 3 104 3 105 3 106 3 107 3 108 3 109 3 110 3 111 3 112 3 113 3 114 3 115 3 116 3 117 3 118 3 119 3 120 3 121 3 122 3 123 3 124 3 125 3 126 3 127 3 128 3 129 3 130 3 129 3 128 3 127 3 126 3 125 3 124 3 123 3 122 3 121 3 120 3 119 3 118 3 117 3 116 3 115 3 114 3 113 3 112 3 111 3 110 3 109 3 108 3 107 3 106 3 105 3 104 3 103 3 102 3 101 3 100 3 99 3 98 3 97 3 96 3 95 3 94 3 93 3 92 3 91 3 90 3 89 3 88 3 87 3 86",
    "TRANSX3": "#TRANSX3 9 20 3 149 3 148 3 147 3 146 3 145 3 144 3 143 3 142 3 141 3 140 3 139 3 138 3 137 3 136 3 135 3 134 3 133 3 132 3 131 3 130",
    "TRANS3X": "#TRANS3X 9 20 3 130 3 131 3 132 3 133 3 134 3 135 3 136 3 137 3 138 3 139 3 140 3 141 3 142 3 143 3 144 3 145 3 146 3 147 3 148 3 149",
    "PATTACK1": "#PATTACK1 9 1 3 0",
    "UATTACK1": "#UATTACK1 9 1 3 0",
    "DEATH": "@DEATH 9 4 0 69",
    "PSTAND": "#PSTAND 9 1 6 0",
    "PSTAND1": "#PSTAND1 9 1 3 0",
    "DEATHLIE1": "#DEATHLIE1 9 1 4 69",
}
WEAPONS = {0: "MECH", 1: "VISTREL"}
ACTIVE = {"ATTACK1": 15}
PALETTE = {"matchcoat": (0.14, 0.2, 0.42)}


def spec():
    return native.spec(MD, WEAPONS, drape=("leggings", "breechcloth"), palette=PALETTE, active=ACTIVE, hair="lock", feathers=0, headdress="gustoweh", silver=True, paint="red_face_half", dress=("breechcloth", "leggings", "moccasins", "matchcoat"), kit=("belt_l", "horn", "pouch", "knife"), weapon="gun", melee=None)


def build(ctx):
    native.build(ctx, spec())


def make(root, name, build=None):
    s = spec()
    return native.make(root, name, dict(s, build=build or s["build"]))


def pose_for(ch, clip):
    native.pose_for(ch, spec(), clip)
