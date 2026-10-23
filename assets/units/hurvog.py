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
