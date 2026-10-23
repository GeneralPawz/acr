"""Iroquois tomahawk thrower (`IroTam`, `IROTAM.MD`, "Tomahawkwerfer"; the Huron field him
too): Woodlands warrior (`acr_assets.native`): roach with two feathers, red hand prints
and a red band across the eyes, silver gorget and arm bands, a blue matchcoat over the
left shoulder, breechcloth, red leggings, moccasins; tomahawk in the right hand. ATTACK1
throws it (gone on frame 16, the next drawn from the belt), ATTACK and the strike clips
hack (`clips.club_strikes`).
"""

from acr_assets import native

TYPE = "IroTam"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

# `#ATTACK` lists 60 frames, `#ATTACK1` 50 (IROTAM.MD; only the counts matter).
MD = {
    "STAND": "#STAND 9 1 6 0",
    "MOTION_L": "@MOTION_L 9 1 0 8",
    "MOTION_R": "@MOTION_R 9 1 9 15",
    "MOTION_LB": "@MOTION_LB 9 1 15 9",
    "MOTION_RB": "@MOTION_RB 9 1 8 0",
    "MOTION_L0": "@MOTION_L0 9 1 0 8",
    "MOTION_R0": "@MOTION_R0 9 1 9 15",
    "MOTION_LB0": "@MOTION_LB0 9 1 15 9",
    "MOTION_RB0": "@MOTION_RB0 9 1 8 0",
    "ATTACK": "#ATTACK 9 60",
    "ATTACK1": "#ATTACK1 9 50",
    "DEATH": "@DEATH 9 4 0 57",
    "DEATHLIE1": "#DEATHLIE1 9 1 4 57",
}
WEAPONS = {0: "MECH", 1: "TAMAGAVK"}
ACTIVE = {"ATTACK1": 15}


def spec():
    return native.spec(MD, WEAPONS, active=ACTIVE, weapon="tomahawk", melee="tomahawk", hair="roach", feathers=2,
                       paint="red_hands", silver=True, dress=("breechcloth", "leggings", "moccasins", "matchcoat"),
                       kit=("knife",))


def build(ctx):
    native.build(ctx, spec())


def make(root, name, build=None):
    s = spec()
    return native.make(root, name, dict(s, build=build or s["build"]))


def pose_for(ch, clip):
    native.pose_for(ch, spec(), clip)
