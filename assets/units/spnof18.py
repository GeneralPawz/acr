"""Spanish officer, 18th century (`SpnOf18`, `SPNOF18.MD`, `MECH`): white coat with blue
facings in finer cloth, silver lace on hat and buttonholes, the red sash of Spain, silver
gorget, an officer's sword.

Family 4 (`acr_assets.officers`, docs/families/officers.md): the line uniform in finer cloth,
lace, gorget, kit and the sword clips with the stab down at a man on the ground.
"""

from acr_assets import officers

TYPE = "SpnOf18"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

MD = {
    "STAND": "#STAND 9 1 5 0",
    "REST": "@REST 9 5 0 52",
    "MOTION_L": "@MOTION_L 9 1 0 9",
    "MOTION_R": "@MOTION_R 9 1 10 19",
    "MOTION_RB": "@MOTION_RB 9 1 9 0",
    "MOTION_LB": "@MOTION_LB 9 1 19 10",
    "ATTACK": "@ATTACK 9 3 0 51",
    "DEATH": "@DEATH 9 4 0 48",
    "DEATHLIE1": "#DEATHLIE1 9 1 4 48",
}


def spec():
    return officers.spec(MD, palette={"coat": (0.86, 0.85, 0.80), "facings": (0.13, 0.17, 0.40), "lining": (0.86, 0.85, 0.80),
                                  "sash": (0.70, 0.07, 0.07), "lace": officers.SILVER, "brass": officers.SILVER})


def make(root, name, build=None):
    s = spec()
    return officers.make(root, name, dict(s, build=build or s["build"]))


def build(ctx):
    officers.build(ctx, spec())


def pose_for(ch, clip):
    officers.pose_for(ch, spec(), clip)
