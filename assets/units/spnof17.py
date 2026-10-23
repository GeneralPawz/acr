"""Spanish officer, late 17th century (`SpnOf17`, `SPNOF17.MD`, `MECH`): dark coat with red
facings, a broad cocked hat laced in gold, the red sash of Spain, gilt gorget and an
officer's sword.

Family 4 (`acr_assets.officers`, docs/families/officers.md): the line uniform in finer cloth,
lace, gorget, kit and the sword clips with the stab down at a man on the ground.
"""

from acr_assets import officers

TYPE = "SpnOf17"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

MD = {
    "STAND": "#STAND 9 1 5 0",
    "REST": "@REST 9 5 0 39",
    "MOTION_L": "@MOTION_L 9 1 0 9",
    "MOTION_R": "@MOTION_R 9 1 10 19",
    "MOTION_RB": "@MOTION_RB 9 1 9 0",
    "MOTION_LB": "@MOTION_LB 9 1 19 10",
    "ATTACK": "@ATTACK 9 3 0 59",
    "DEATH": "@DEATH 9 4 0 45",
    "DEATHLIE1": "#DEATHLIE1 9 1 4 45",
}


def spec():
    return officers.spec(MD, hat="cocked",
                         palette={"coat": (0.20, 0.18, 0.16), "facings": (0.62, 0.08, 0.07), "lining": (0.62, 0.08, 0.07),
                                  "sash": (0.70, 0.07, 0.07)})


def make(root, name, build=None):
    s = spec()
    return officers.make(root, name, dict(s, build=build or s["build"]))


def build(ctx):
    officers.build(ctx, spec())


def pose_for(ch, clip):
    officers.pose_for(ch, spec(), clip)
