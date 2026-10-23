"""French officer, 18th century (`FrnOf18`, `FRNOF18.MD`, `MECH`): the fusilier's dark blue coat
(`frnm18.py`) with red facings and white lapels in finer cloth, gold lace, gilt gorget, no
sash (French officers wore none before the Revolution), and a smallsword.

Family 4 (`acr_assets.officers`, docs/families/officers.md): the line uniform in finer cloth,
lace, gorget, kit and the sword clips with the stab down at a man on the ground.
"""

from acr_assets import officers

TYPE = "FrnOf18"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

MD = {
    "STAND": "#STAND 9 1 5 0",
    "REST": "@REST 9 5 0 52",
    "MOTION_L": "@MOTION_L 9 1 0 9",
    "MOTION_R": "@MOTION_R 9 1 10 19",
    "MOTION_RB": "@MOTION_RB 9 1 9 0",
    "MOTION_LB": "@MOTION_LB 9 1 19 10",
    "ATTACK": "@ATTACK 9 3 0 51",
    "DEATH": "@DEATH 9 4 0 47",
    "DEATHLIE1": "#DEATHLIE1 9 1 4 47",
}


def spec():
    return officers.spec(MD, weapon="smallsword", kit=("off_gorget", "off_scabbard"), lapels="lapels", fixed=("waistcoat", "lapels"),
                         palette={"coat": (0.12, 0.17, 0.38), "facings": (0.62, 0.08, 0.07), "lining": (0.62, 0.08, 0.07),
                                  "waistcoat": (0.86, 0.85, 0.80), "lapels": (0.86, 0.85, 0.80)})


def make(root, name, build=None):
    s = spec()
    return officers.make(root, name, dict(s, build=build or s["build"]))


def build(ctx):
    officers.build(ctx, spec())


def pose_for(ch, clip):
    officers.pose_for(ch, spec(), clip)
