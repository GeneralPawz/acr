"""Iroquois rifleman (`IroStr`, `IROSTR.MD`, "Schütze"): Woodlands warrior
(`acr_assets.native`) with a trade gun: roach and one feather, black stripes painted on
chest and arms, breechcloth, red leggings, moccasins; the strap over the left shoulder with
powder horn and shot pouch at the right hip. The gun's clips are the musketeer's (present,
fire on the active frame, reload; `poses.musketeer_clips`), the melee (index 0) the
musket's thrust and butt.
"""

from acr_assets import native

TYPE = "IroStr"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

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
    "MOTION_L1": "@MOTION_L1 9 1 0 8",
    "MOTION_R1": "@MOTION_R1 9 1 9 15",
    "MOTION_LB1": "@MOTION_LB1 9 1 15 9",
    "MOTION_RB1": "@MOTION_RB1 9 1 8 0",
    "PATTACK": "#PATTACK 9 1 6 0",
    "UATTACK": "#UATTACK 9 1 6 0",
    "ATTACK": "@ATTACK 9 6 0 57",
    "PATTACK1": "#PATTACK1 9 1 3 0",
    "UATTACK1": "#UATTACK1 9 1 3 0",
    "ATTACK1": "@ATTACK1 9 3 0 87",
    "DEATH": "@DEATH 9 4 0 69",
    "PSTAND": "#PSTAND 9 1 6 0",
    "PSTAND1": "#PSTAND1 9 1 3 0",
    "DEATHLIE1": "#DEATHLIE1 9 1 4 69",
}
WEAPONS = {0: "MECH", 1: "VISTREL"}
ACTIVE = {"ATTACK1": 15}


def spec():
    return native.spec(MD, WEAPONS, active=ACTIVE, weapon="gun", melee=None, hair="roach", feathers=1,
                       paint="black_stripes", kit=("belt_l", "horn", "pouch", "knife"))


def build(ctx):
    native.build(ctx, spec())


def make(root, name, build=None):
    s = spec()
    return native.make(root, name, dict(s, build=build or s["build"]))


def pose_for(ch, clip):
    native.pose_for(ch, spec(), clip)
