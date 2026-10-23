"""Sioux villager (`SiuKri`, `SIUKRI.MD`, the peasant): Plains (`acr_assets.plains` over
`acr_assets.native`, docs/families/plains.md). A buckskin shirt with quilled strips over the
shoulders and down the sleeves and strand fringe under the arms, a breechcloth, fringed
buckskin leggings, moccasins, two wrapped braids, no paint; a stone-headed war club. WORK and
ATTACK1 (the harvest) are the two-handed swing, REST a look round; the melee (index 0) is
ATTACK5, the club blow (the `.MD` has no ATTACK: the strikes are made on ATTACK5's frames).
"""

from acr_assets import plains

TYPE = "SiuKri"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

MD = {
    "REST": "@REST 9 12 0 70",
    "STAND": "#STAND 9 1 12 0",
    "MOTION_L": "@MOTION_L 9 0 0 7",
    "MOTION_RB": "@MOTION_RB 9 0 7 0",
    "MOTION_R": "@MOTION_R 9 0 8 15",
    "MOTION_LB": "@MOTION_LB 9 0 15 8",
    "WORK": "@WORK 9 9 0 22",
    "ATTACK1": "@ATTACK1 9 8 0 43",
    "PATTACK1": "#PATTACK1 9 1 4 85",
    "UATTACK1": "#UATTACK1 9 1 4 85",
    "MOTION_L1": "@MOTION_L1 9 2 0 7",
    "MOTION_RB1": "@MOTION_RB1 9 2 7 0",
    "MOTION_R1": "@MOTION_R1 9 2 8 15",
    "MOTION_LB1": "@MOTION_LB1 9 2 15 8",
    "PSTAND1": "#PSTAND1 9 1 8 0",
    "PSTAND4": "#PSTAND4 9 1 4 85",
    "PSTAND5": "#PSTAND5 9 1 12 0",
    "PATTACK4": "@PATTACK4 9 4 0 85",
    "UATTACK4": "@UATTACK4 9 4 85 0",
    "ATTACK4": "#ATTACK4 9 1 8 0",
    "MOTION_L4": "@MOTION_L4 9 1 0 9",
    "MOTION_RB4": "@MOTION_RB4 9 1 9 0",
    "MOTION_R4": "@MOTION_R4 9 1 10 19",
    "MOTION_LB4": "@MOTION_LB4 9 1 19 10",
    "PATTACK5": "#PATTACK5 9 1 8 0",
    "UATTACK5": "#UATTACK5 9 1 8 0",
    "ATTACK5": "@ATTACK5 9 8 0 43",
    "MOTION_L5": "@MOTION_L5 9 0 0 7",
    "MOTION_RB5": "@MOTION_RB5 9 0 7 0",
    "MOTION_R5": "@MOTION_R5 9 0 8 15",
    "MOTION_LB5": "@MOTION_LB5 9 0 15 8",
    "DEATH": "@DEATH 9 11 0 48",
    "DEATHLIE1": "#DEATHLIE1 9 1 11 48",
}
WEAPONS = {0: "MECH"}
PALETTE = {"breechcloth": (0.50, 0.12, 0.08), "skin": (0.80, 0.55, 0.38)}


def spec():
    return plains.spec(MD, WEAPONS, palette=PALETTE, weapon="club", melee="club", paint=None, work=True, eagle=0,
                       warshirt=True, kit=("knife",))


def build(ctx):
    plains.build(ctx, spec())


def make(root, name, build=None):
    s = spec()
    return plains.make(root, name, dict(s, build=build or s["build"]))


def pose_for(ch, clip):
    plains.pose_for(ch, spec(), clip)
