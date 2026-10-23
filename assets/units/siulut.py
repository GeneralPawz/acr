"""Sioux archer (`SiuLut`, `SIULUT.MD`, "Bogenschütze (Jäger)"): Plains (`acr_assets.plains`
over `acr_assets.native`, docs/families/plains.md). Bare-chested with a hair-pipe breastplate,
hair parted in two wrapped braids, one eagle feather at the back, red and black face paint, a
dark trade-cloth breechcloth, buckskin leggings with wide flaps and strand fringe along the
outer seam, moccasins; a self bow, a quiver on the back, a knife. Fire arrow (index 1) and
arrow (index 2) share the bow clip (the arrow leaves on frame 44); the melee (index 0)
strikes with a stone-headed war club (`socket.club_r`, the arrow hidden).
"""

from acr_assets import plains

TYPE = "SiuLut"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

# `#ATTACK1`/`#ATTACK2` list 79 frames (the full lists in SIULUT.MD; only the count matters).
MD = {
    "STAND": "#STAND 9 1 1 0",
    "PSTAND": "#PSTAND 9 1 1 0",
    "PSTAND2": "#PSTAND2 9 1 1 0",
    "PSTAND3": "#PSTAND3 9 1 1 0",
    "PSTAND4": "#PSTAND4 9 1 1 0",
    "MOTION_L": "@MOTION_L 9 0 0 7",
    "MOTION_R": "@MOTION_R 9 0 8 15",
    "MOTION_RB": "@MOTION_RB 9 0 15 8",
    "MOTION_LB": "@MOTION_LB 9 0 7 0",
    "MOTION_L4": "@MOTION_L4 9 4 0 9",
    "MOTION_R4": "@MOTION_R4 9 4 10 19",
    "MOTION_RB4": "@MOTION_RB4 9 4 19 10",
    "MOTION_LB4": "@MOTION_LB4 9 4 0 9",
    "ATTACK2": "#ATTACK2 9 79",
    "ATTACK1": "#ATTACK1 9 79",
    "ATTACK": "@ATTACK 9 2 0 54",
    "DEATH": "@DEATH 9 3 0 48",
    "DEATHLIE1": "#DEATHLIE1 9 1 3 48",
    "DEATHLIE2": "#DEATHLIE2 9 1 3 48",
    "DEATHLIE3": "#DEATHLIE3 9 1 3 48",
}
WEAPONS = {0: "MECH", 1: "OSTRELA", 2: "STRELA"}
ACTIVE = {"ATTACK1": 44, "ATTACK2": 44}


def spec():
    return plains.spec(MD, WEAPONS, active=ACTIVE, weapon="bow", melee="club", paint="red_black", eagle=1,
                       breastplate=True, kit=("quiver", "knife"))


def build(ctx):
    plains.build(ctx, spec())


def make(root, name, build=None):
    s = spec()
    return plains.make(root, name, dict(s, build=build or s["build"]))


def pose_for(ch, clip):
    plains.pose_for(ch, spec(), clip)
