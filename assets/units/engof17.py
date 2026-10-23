"""English officer, late 17th century (`EngOf17`, `ENGOF17.MD`, `PIKA`): red coat with
blue facings, a broad cocked hat laced in gold, crimson sash, gilt gorget and a spontoon
(two-handed thrust, `PATTACK`/`PSTAND`: the spontoon levelled).

Family 4 (`acr_assets.officers`, docs/families/officers.md): the line uniform in finer cloth,
lace, gorget, kit and the sword clips with the stab down at a man on the ground.
"""

from acr_assets import officers

TYPE = "EngOf17"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

MD = {
    "STAND": "#STAND 9 1 5 0",
    "REST": "@REST 9 5 0 119",
    "MOTION_L": "@MOTION_L 9 1 0 9",
    "MOTION_R": "@MOTION_R 9 1 10 19",
    "MOTION_RB": "@MOTION_RB 9 1 9 0",
    "MOTION_LB": "@MOTION_LB 9 1 19 10",
    "PATTACK": "@PATTACK 9 2 0 32",
    "ATTACK": "@ATTACK 9 3 0 69",
    "UATTACK": "@UATTACK 9 2 32 0",
    "DEATH": "@DEATH 9 4 0 70",
    "PSTAND": "#PSTAND 9 1 2 32",
    "DEATHLIE1": "#DEATHLIE1 9 1 4 70",
}


def spec():
    return officers.spec(MD, weapon="spontoon", hat="cocked",
                         palette={"facings": (0.13, 0.17, 0.36), "lining": (0.13, 0.17, 0.36)})


def make(root, name, build=None):
    s = spec()
    return officers.make(root, name, dict(s, build=build or s["build"]))


def build(ctx):
    officers.build(ctx, spec())


def pose_for(ch, clip):
    officers.pose_for(ch, spec(), clip)
