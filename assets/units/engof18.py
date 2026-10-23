"""English officer, 18th century (`EngOf18`, `ENGOF18.MD`, `MECH`): the red coat of the line
in finer cloth with blue facings and gold lace on the buttonholes, a gold-laced tricorn,
crimson sash round the waist, gilt gorget, and a drawn officer's sword (spadroon).

Family 4 (`acr_assets.officers`, docs/families/officers.md): the line uniform in finer cloth,
lace, gorget, kit and the sword clips with the stab down at a man on the ground.
"""

from acr_assets import officers

TYPE = "EngOf18"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

MD = {
    "STAND": "#STAND 9 1 5 0",
    "REST": "#REST 9 43 5 0 5 1 5 2 5 3 5 4 5 5 5 6 5 7 5 8 5 9 5 10 5 11 5 12 5 13 5 14 5 15 5 16 5 17 5 18 5 19 5 20 5 21 5 20 5 19 5 18 5 17 5 16 5 15 5 14 5 13 5 12 5 11 5 10 5 9 5 8 5 7 5 6 5 5 5 4 5 3 5 2 5 1 5 0",
    "MOTION_L": "@MOTION_L 9 1 0 9",
    "MOTION_R": "@MOTION_R 9 1 10 19",
    "MOTION_RB": "@MOTION_RB 9 1 9 0",
    "MOTION_LB": "@MOTION_LB 9 1 19 10",
    "ATTACK": "@ATTACK 9 3 0 51",
    "DEATH": "@DEATH 9 4 0 47",
    "DEATHLIE1": "#DEATHLIE1 9 1 4 47",
}


def spec():
    return officers.spec(MD, palette={"facings": (0.13, 0.17, 0.36)})


def make(root, name, build=None):
    s = spec()
    return officers.make(root, name, dict(s, build=build or s["build"]))


def build(ctx):
    officers.build(ctx, spec())


def pose_for(ch, clip):
    officers.pose_for(ch, spec(), clip)
