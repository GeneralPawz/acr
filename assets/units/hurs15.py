"""Huron rifleman (`HurS15`, `HURS15.MD`; a SPEC over IroStr): Woodlands (`acr_assets.native`,
docs/families/native.md): the roach in black deer
hair with one feather, white dots over the chest and red bars on the arms, a green breechcloth, blue
leggings, moccasins; strap, powder horn, shot pouch, knife.
"""

from acr_assets import native

TYPE = "HurS15"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

# `#` lists keep only their frame count (the full lists in HURS15.MD).
MD = {
    "STAND": "#STAND 9 1 3 0",
    "MOTION_L": "@MOTION_L 9 1 0 8",
    "MOTION_R": "@MOTION_R 9 1 9 15",
    "MOTION_LB": "@MOTION_LB 9 1 15 9",
    "MOTION_RB": "@MOTION_RB 9 1 8 0",
    "MOTION_L0": "@MOTION_L0 9 1 0 8",
    "MOTION_R0": "@MOTION_R0 9 1 9 15",
    "MOTION_LB0": "@MOTION_LB0 9 1 15 9",
    "MOTION_RB0": "@MOTION_RB0 9 1 8 0",
    "PATTACK": "#PATTACK 9 1 6 0",
    "UATTACK": "#UATTACK 9 1 6 0",
    "ATTACK": "@ATTACK 9 6 0 60",
    "PATTACK1": "#PATTACK1 9 1 3 0",
    "UATTACK1": "#UATTACK1 9 1 3 0",
    "ATTACK1": "@ATTACK1 9 3 0 87",
    "DEATH": "@DEATH 9 4 0 68",
    "PSTAND": "#PSTAND 9 1 6 0",
    "PSTAND1": "#PSTAND1 9 1 3 0",
    "DEATHLIE1": "#DEATHLIE1 9 1 4 68",
}
WEAPONS = {0: "MECH", 1: "VISTREL"}
ACTIVE = {"ATTACK1": 15}
PALETTE = {"breechcloth": (0.16, 0.34, 0.2), "leggings": (0.14, 0.18, 0.36), "flaps": (0.14, 0.18, 0.36), "skin": (0.78, 0.53, 0.36), "roach": (0.1, 0.08, 0.07)}


def spec():
    return native.spec(MD, WEAPONS, palette=PALETTE, active=ACTIVE, hair="roach", feathers=1, paint="white_dots", kit=("belt_l", "horn", "pouch", "knife"), weapon="gun", melee=None)


def build(ctx):
    native.build(ctx, spec())


def make(root, name, build=None):
    s = spec()
    return native.make(root, name, dict(s, build=build or s["build"]))


def pose_for(ch, clip):
    native.pose_for(ch, spec(), clip)
