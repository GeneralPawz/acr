"""American officer, Continental Army (`UsaOf18`, `USAOF18.MD`, `MECH`): dark blue coat with
buff facings, buff waistcoat and breeches, a gold-laced tricorn, crimson sash, gilt gorget and
a smallsword.

Family 4 (`acr_assets.officers`, docs/families/officers.md): the line uniform in finer cloth,
lace, gorget, kit and the sword clips with the stab down at a man on the ground.
"""

from acr_assets import officers

TYPE = "UsaOf18"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

MD = {
    "STAND": "#STAND 9 1 5 0",
    "REST": "@REST 9 5 0 120",
    "MOTION_L": "@MOTION_L 9 1 0 9",
    "MOTION_R": "@MOTION_R 9 1 10 19",
    "MOTION_RB": "@MOTION_RB 9 1 9 0",
    "MOTION_LB": "@MOTION_LB 9 1 19 10",
    "ATTACK": "#ATTACK 9 52 3 0 3 1 3 2 3 3 3 4 3 5 3 6 3 7 3 8 3 9 3 10 3 11 3 12 3 13 3 14 3 15 3 16 3 17 3 18 3 19 3 20 3 21 3 22 3 23 3 24 3 25 3 26 3 27 3 28 3 29 3 30 3 31 3 32 3 33 3 34 3 35 3 36 3 37 3 38 3 39 3 40 3 41 3 42 3 43 3 44 3 45 3 46 3 47 3 48 3 49 3 50 3 0",
    "DEATH": "@DEATH 9 4 0 45",
    "DEATHLIE1": "#DEATHLIE1 9 1 4 45",
}


def spec():
    return officers.spec(MD, weapon="smallsword",
                         palette={"coat": (0.10, 0.13, 0.30), "facings": (0.80, 0.70, 0.50), "lining": (0.80, 0.70, 0.50),
                                  "waistcoat": (0.80, 0.70, 0.50), "breeches": (0.80, 0.70, 0.50)})


def make(root, name, build=None):
    s = spec()
    return officers.make(root, name, dict(s, build=build or s["build"]))


def build(ctx):
    officers.build(ctx, spec())


def pose_for(ch, clip):
    officers.pose_for(ch, spec(), clip)
