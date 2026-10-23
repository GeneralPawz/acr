"""Iroquois shaman (`IroSha`, `IROSHA.MD`): Woodlands (`acr_assets.native`,
docs/families/native.md): long loose hair with a feather, a white band
across the eyes and white dots on the chest, a bear-skin cloak over the shoulders, breechcloth, red
leggings, moccasins; a turtle-shell rattle in the left hand, a war club in the right. He heals (MEDIK,
ATTACK1: `clips.heal`, the rattle shaken) and strikes with the club (ATTACK).
"""

from acr_assets import native

TYPE = "IroSha"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

# `#` lists keep only their frame count (the full lists in IROSHA.MD).
MD = {
    "STAND": "#STAND 9 1 3 0",
    "MOTION_L": "@MOTION_L 9 1 0 7",
    "MOTION_R": "@MOTION_R 9 1 8 15",
    "MOTION_LB": "@MOTION_LB 9 1 15 8",
    "MOTION_RB": "@MOTION_RB 9 1 7 0",
    "MOTION_L0": "@MOTION_L0 9 1 0 7",
    "MOTION_R0": "@MOTION_R0 9 1 8 15",
    "MOTION_LB0": "@MOTION_LB0 9 1 15 8",
    "MOTION_RB0": "@MOTION_RB0 9 1 7 0",
    "PATTACK1": "#PATTACK1 9 1 6 0",
    "UATTACK1": "#UATTACK1 9 1 6 0",
    "ATTACK": "@ATTACK 9 6 0 55",
    "PATTACK": "#PATTACK 9 1 3 0",
    "UATTACK": "#UATTACK 9 1 3 0",
    "ATTACK1": "#ATTACK1 9 101",
    "DEATH": "@DEATH 9 4 0 34",
    "PSTAND1": "#PSTAND1 9 1 6 0",
    "PSTAND": "#PSTAND 9 1 3 0",
    "DEATHLIE1": "#DEATHLIE1 9 1 4 34",
}
WEAPONS = {0: "MECH", 1: "MEDIK"}
ACTIVE = {"ATTACK1": 1}
PALETTE = {"pelt": (0.3, 0.22, 0.15)}


def spec():
    return native.spec(MD, WEAPONS, palette=PALETTE, active=ACTIVE, hair="long", feathers=1, paint="white_band", rattle=True, dress=("breechcloth", "leggings", "moccasins", "cloak"), kit=("knife",), weapon="club", melee="club")


def build(ctx):
    native.build(ctx, spec())


def make(root, name, build=None):
    s = spec()
    return native.make(root, name, dict(s, build=build or s["build"]))


def pose_for(ch, clip):
    native.pose_for(ch, spec(), clip)
