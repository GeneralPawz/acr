"""Delaware archer (`DelLut`, `DELLUT.MD`; a SPEC over IroLut): Woodlands (`acr_assets.native`,
docs/families/native.md): the scalp lock with two
feathers, red and black face paint, a tan deerskin breechcloth, buckskin leggings, moccasins; a self bow and quiver,
a tomahawk for the melee (index 0), a knife.
"""

from acr_assets import native

TYPE = "DelLut"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

# `#` lists keep only their frame count (the full lists in DELLUT.MD).
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
    "DEATH": "@DEATH 9 3 0 60",
    "DEATHLIE1": "#DEATHLIE1 9 1 3 60",
    "DEATHLIE2": "#DEATHLIE2 9 1 3 60",
    "DEATHLIE3": "#DEATHLIE3 9 1 3 60",
}
WEAPONS = {0: "MECH", 1: "OSTRELA", 2: "STRELA"}
ACTIVE = {"ATTACK1": 44, "ATTACK2": 44}
# Colours from the sprite (palette check, ACR-analysis/genai/palette-check): a tan deerskin
# breechcloth, smoke-browned buckskin leggings with lighter flaps; no blue in the sprite;
# dark garters (#307: the gold read 16.8 off); the sprite's tan wrap over one shoulder
# waits for a draped matchcoat.
PALETTE = {"breechcloth": (0.66, 0.58, 0.4), "leggings": (0.42, 0.32, 0.2), "flaps": (0.55, 0.4, 0.26), "skin": (0.82, 0.57, 0.4),
           "garter": (0.36, 0.29, 0.17)}


def spec():
    return native.spec(MD, WEAPONS, palette=PALETTE, active=ACTIVE, melee="tomahawk", hair="lock", feathers=2, paint="red_black", kit=("quiver", "knife"), weapon="bow",
                       drape=("leggings", "breechcloth"))


def build(ctx):
    native.build(ctx, spec())


def make(root, name, build=None):
    s = spec()
    return native.make(root, name, dict(s, build=build or s["build"]))


def pose_for(ch, clip):
    native.pose_for(ch, spec(), clip)
