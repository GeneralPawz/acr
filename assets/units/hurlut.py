"""Huron archer (`HurLut`, `HURLUT.MD`; a SPEC over IroLut): Woodlands (`acr_assets.native`,
docs/families/native.md): the roach in black deer hair
with three feathers, black and white face paint, a green breechcloth, blue leggings, moccasins; a self
bow and quiver, a war club for the melee (index 0), a knife.
"""

from acr_assets import native

TYPE = "HurLut"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

# `#` lists keep only their frame count (the full lists in HURLUT.MD).
MD = {
    "STAND": "#STAND 9 1 1 0",
    "PSTAND": "#PSTAND 9 1 1 0",
    "PSTAND1": "#PSTAND1 9 1 1 0",
    "PSTAND2": "#PSTAND2 9 1 1 0",
    "MOTION_L": "@MOTION_L 9 0 0 7",
    "MOTION_R": "@MOTION_R 9 0 8 15",
    "MOTION_RB": "@MOTION_RB 9 0 15 8",
    "MOTION_LB": "@MOTION_LB 9 0 7 0",
    "MOTION_L0": "@MOTION_L0 9 0 0 7",
    "MOTION_R0": "@MOTION_R0 9 0 8 15",
    "MOTION_RB0": "@MOTION_RB0 9 0 15 8",
    "MOTION_LB0": "@MOTION_LB0 9 0 7 0",
    "ATTACK2": "#ATTACK2 9 79",
    "ATTACK1": "#ATTACK1 9 79",
    "ATTACK": "@ATTACK 9 2 0 60",
    "DEATH": "@DEATH 9 3 0 48",
    "DEATHLIE1": "#DEATHLIE1 9 1 3 48",
    "DEATHLIE2": "#DEATHLIE2 9 1 3 48",
    "DEATHLIE3": "#DEATHLIE3 9 1 3 48",
}
WEAPONS = {0: "MECH", 1: "OSTRELA", 2: "STRELA"}
ACTIVE = {"ATTACK1": 44, "ATTACK2": 44}
PALETTE = {"breechcloth": (0.16, 0.34, 0.2), "leggings": (0.14, 0.18, 0.36), "flaps": (0.14, 0.18, 0.36), "skin": (0.78, 0.53, 0.36), "roach": (0.1, 0.08, 0.07)}


def spec():
    return native.spec(MD, WEAPONS, palette=PALETTE, active=ACTIVE, melee="club", hair="roach", feathers=3, paint="black_face_half", kit=("quiver", "knife"), weapon="bow")


def build(ctx):
    native.build(ctx, spec())


def make(root, name, build=None):
    s = spec()
    return native.make(root, name, dict(s, build=build or s["build"]))


def pose_for(ch, clip):
    native.pose_for(ch, spec(), clip)
