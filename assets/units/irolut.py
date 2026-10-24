"""Iroquois archer (`IroLut`, `IROLUT.MD`, "Bogenschütze"): Woodlands warrior
(`acr_assets.native`): a roach on the shaved head with two feathers, red and black face
paint, a dark stroud breechcloth, red leggings with wide side flaps and garters, moccasins;
a self bow, a quiver on the back, a knife at the belt. Fire arrow (index 1) and arrow
(index 2) share the bow clip (the arrow leaves on frame 44); the melee (index 0) hacks
with a war club (`native.club_socket`: the arrow hidden, the club shown).
"""

from acr_assets import native

TYPE = "IroLut"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

# `#ATTACK1`/`#ATTACK2` list 79 frames (the full lists in IROLUT.MD; only the count matters).
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
    "ATTACK": "@ATTACK 9 2 0 55",
    "DEATH": "@DEATH 9 3 0 37",
    "DEATHLIE1": "#DEATHLIE1 9 1 3 37",
    "DEATHLIE2": "#DEATHLIE2 9 1 3 37",
    "DEATHLIE3": "#DEATHLIE3 9 1 3 37",
}
WEAPONS = {0: "MECH", 1: "OSTRELA", 2: "STRELA"}
ACTIVE = {"ATTACK1": 44, "ATTACK2": 44}


def spec():
    return native.spec(MD, WEAPONS, drape=("leggings", "breechcloth"), active=ACTIVE, weapon="bow", melee="club", hair="roach", feathers=2,
                       paint="red_black", kit=("quiver", "knife"))


def build(ctx):
    native.build(ctx, spec())


def make(root, name, build=None):
    s = spec()
    return native.make(root, name, dict(s, build=build or s["build"]))


def pose_for(ch, clip):
    native.pose_for(ch, spec(), clip)
