"""Delaware spearman (`DelPik`, `DELPIK.MD`, "Lanzenkämpfer"): Woodlands warrior
(`acr_assets.native`) in a linen trade shirt over breechcloth and blue stroud leggings,
moccasins; the scalp lock with a braid and three feathers, a red half face; a spear held
in both hands for the thrust (`clips.spear_strikes`, PIKA index 0).
"""

from acr_assets import native

TYPE = "DelPik"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

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
    "ATTACK": "@ATTACK 9 3 0 68",
    "DEATH": "@DEATH 9 4 0 34",
    "DEATHLIE1": "#DEATHLIE1 9 1 4 34",
}
WEAPONS = {0: "PIKA"}
PALETTE = {"leggings": (0.14, 0.20, 0.42), "flaps": (0.14, 0.20, 0.42), "breechcloth": (0.45, 0.10, 0.08),
           "skin": (0.80, 0.56, 0.39)}


def spec():
    return native.spec(MD, WEAPONS, palette=PALETTE, weapon="spear", melee="spear", hair="lock", feathers=3,
                       paint="red_face_half", dress=("breechcloth", "leggings", "moccasins", "shirt"), kit=("knife",))


def build(ctx):
    native.build(ctx, spec())


def make(root, name, build=None):
    s = spec()
    return native.make(root, name, dict(s, build=build or s["build"]))


def pose_for(ch, clip):
    native.pose_for(ch, spec(), clip)
