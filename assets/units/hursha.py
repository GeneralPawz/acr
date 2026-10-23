"""Huron shaman (`HurSha`, `HURSHA.MD`): Woodlands (`acr_assets.native`,
docs/families/native.md): long loose hair with two feathers,
white paint, a wolf-skin cloak, a green breechcloth, blue leggings, moccasins; a turtle-shell rattle and
a war club. Heals (MEDIK, ATTACK1, `clips.heal`) and strikes with the club (ATTACK).
"""

from acr_assets import native

TYPE = "HurSha"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

# `#` lists keep only their frame count (the full lists in HURSHA.MD).
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
    "PATTACK0": "#PATTACK0 9 1 6 0",
    "UATTACK0": "#UATTACK0 9 1 6 0",
    "ATTACK": "@ATTACK 9 6 0 50",
    "PATTACK1": "#PATTACK1 9 1 3 0",
    "UATTACK1": "#UATTACK1 9 1 3 0",
    "ATTACK1": "@ATTACK1 9 3 0 74",
    "DEATH": "@DEATH 9 4 0 68",
    "PSTAND": "#PSTAND 9 1 6 0",
    "PSTAND1": "#PSTAND1 9 1 3 0",
    "DEATHLIE1": "#DEATHLIE1 9 1 4 68",
}
WEAPONS = {0: "MECH", 1: "MEDIK"}
ACTIVE = {"ATTACK1": 1}
PALETTE = {"breechcloth": (0.16, 0.34, 0.2), "leggings": (0.14, 0.18, 0.36), "flaps": (0.14, 0.18, 0.36), "skin": (0.78, 0.53, 0.36), "pelt": (0.42, 0.38, 0.32)}


def spec():
    return native.spec(MD, WEAPONS, palette=PALETTE, active=ACTIVE, hair="long", feathers=2, paint="white_band", rattle=True, dress=("breechcloth", "leggings", "moccasins", "cloak"), kit=("knife",), weapon="club", melee="club")


def build(ctx):
    native.build(ctx, spec())


def make(root, name, build=None):
    s = spec()
    return native.make(root, name, dict(s, build=build or s["build"]))


def pose_for(ch, clip):
    native.pose_for(ch, spec(), clip)
