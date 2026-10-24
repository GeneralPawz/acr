"""Delaware villager (`DelKri`, `DELKRI.MD`, the peasant): Woodlands (`acr_assets.native`,
docs/families/native.md): the scalp lock, a linen trade
shirt, a red breechcloth, blue leggings, moccasins, no paint; a bow and quiver (ATTACK1/2, the arrow
leaves on frame 25), a war club for the melee; WORK (a two-handed swing, as EngKri's) and REST (a look
round).
"""

from acr_assets import native

TYPE = "DelKri"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

# `#` lists keep only their frame count (the full lists in DELKRI.MD).
MD = {
    "REST": "@REST 9 13 0 70",
    "STAND": "#STAND 9 1 13 0",
    "MOTION_L5": "@MOTION_L5 9 0 0 7",
    "MOTION_RB5": "@MOTION_RB5 9 0 15 8",
    "MOTION_R5": "@MOTION_R5 9 0 8 15",
    "WORK": "@WORK 9 10 0 22",
    "MOTION_L6": "@MOTION_L6 9 0 0 7",
    "MOTION_RB6": "@MOTION_RB6 9 0 15 8",
    "MOTION_R6": "@MOTION_R6 9 0 8 15",
    "PSTAND0": "#PSTAND0 9 1 8 0",
    "MOTION_L8": "@MOTION_L8 9 3 0 9",
    "MOTION_RB8": "@MOTION_RB8 9 3 9 0",
    "MOTION_R8": "@MOTION_R8 9 3 10 19",
    "MOTION_LB8": "@MOTION_LB8 9 3 19 10",
    "MOTION_L7": "@MOTION_L7 9 3 0 9",
    "MOTION_RB7": "@MOTION_RB7 9 3 9 0",
    "MOTION_R7": "@MOTION_R7 9 3 10 19",
    "MOTION_LB7": "@MOTION_LB7 9 3 19 10",
    "MOTION_L4": "@MOTION_L4 9 1 0 9",
    "MOTION_RB4": "@MOTION_RB4 9 1 9 0",
    "MOTION_R4": "@MOTION_R4 9 1 10 19",
    "MOTION_LB4": "@MOTION_LB4 9 1 19 10",
    "PATTACK": "#PATTACK 9 1 8 0",
    "UATTACK": "#UATTACK 9 1 8 0",
    "ATTACK": "@ATTACK 9 8 0 60",
    "MOTION_L": "@MOTION_L 9 2 0 7",
    "MOTION_RB": "@MOTION_RB 9 2 7 0",
    "MOTION_R": "@MOTION_R 9 2 8 15",
    "MOTION_LB": "@MOTION_LB 9 2 15 8",
    "PATTACK2": "#PATTACK2 9 1 14 0",
    "UATTACK2": "#UATTACK2 9 1 14 0",
    "ATTACK2": "@ATTACK2 9 14 0 58",
    "MOTION_L2": "@MOTION_L2 9 2 0 7",
    "MOTION_RB2": "@MOTION_RB2 9 2 7 0",
    "MOTION_R2": "@MOTION_R2 9 2 8 15",
    "MOTION_LB2": "@MOTION_LB2 9 2 15 8",
    "PATTACK1": "#PATTACK1 9 1 14 0",
    "UATTACK1": "#UATTACK1 9 1 14 0",
    "ATTACK1": "@ATTACK1 9 14 0 58",
    "MOTION_L1": "@MOTION_L1 9 2 0 7",
    "MOTION_RB1": "@MOTION_RB1 9 2 7 0",
    "MOTION_R1": "@MOTION_R1 9 2 8 15",
    "MOTION_LB1": "@MOTION_LB1 9 2 15 8",
    "MOTION_L3": "@MOTION_L3 9 2 0 7",
    "MOTION_RB3": "@MOTION_RB3 9 2 7 0",
    "MOTION_R3": "@MOTION_R3 9 2 8 15",
    "MOTION_LB3": "@MOTION_LB3 9 2 15 8",
    "MOTION_L9": "@MOTION_L9 9 4 0 9",
    "MOTION_RB9": "@MOTION_RB9 9 4 9 0",
    "MOTION_R9": "@MOTION_R9 9 4 10 19",
    "MOTION_LB9": "@MOTION_LB9 9 4 19 10",
    "DEATH": "@DEATH 9 12 0 60",
    "DEATHLIE1": "#DEATHLIE1 9 1 12 60",
}
WEAPONS = {0: "MECH", 1: "OSTRELA", 2: "STRELA"}
ACTIVE = {"ATTACK1": 25, "ATTACK2": 25}
# The DELKRI sprites draw the feet about 4 px (zoom 1) higher against the anchor than the
# other units' sprites, which the renderer meets with `view3d::UNIT_SOUTH` (#196): the model
# stands 0.15 m (4 px / 30 px per m / cos 30) up so its feet meet the sprite's again.
LIFT = 0.15
PALETTE = {"breechcloth": (0.58, 0.12, 0.08), "leggings": (0.16, 0.22, 0.42), "flaps": (0.16, 0.22, 0.42), "skin": (0.82, 0.57, 0.4)}


def spec():
    return native.spec(MD, WEAPONS, palette=PALETTE, active=ACTIVE, melee="club", hair="lock", feathers=0, paint=None, work=True, dress=("breechcloth", "leggings", "moccasins", "shirt"), kit=("quiver", "knife"), weapon="bow", lift=LIFT)


def build(ctx):
    native.build(ctx, spec())


def make(root, name, build=None):
    s = spec()
    return native.make(root, name, dict(s, build=build or s["build"]))


def pose_for(ch, clip):
    native.pose_for(ch, spec(), clip)
