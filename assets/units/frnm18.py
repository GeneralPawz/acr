"""French fusilier, 18th century (`FrnM18`, `FRNM18.MD`, "Füsilier"): line infantry
(`acr_assets.line`) as the sprite shows him: a dark blue coat with red cuffs, collar and
turnbacks (the colours of the Gardes françaises; the line's white-grey coat would lose the
sprite's look), white waistcoat and breeches, black gaiters, buff belts, tricorn,
Charleville musket. Colours mixed with the `#STAND` sprites like EngM18's.
"""

from acr_assets import line

TYPE = "FrnM18"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

MD = {
    "STAND": "#STAND 9 1 5 0",
    "MOTION_L": "@MOTION_L 9 1 0 9",
    "MOTION_R": "@MOTION_R 9 1 10 19",
    "MOTION_LB": "@MOTION_LB 9 1 19 10",
    "MOTION_RB": "@MOTION_RB 9 1 9 0",
    "PATTACK": "@PATTACK 9 5 0 35",
    "UATTACK": "@UATTACK 9 5 35 0",
    "PSTAND": "#PSTAND 9 1 5 35",
    "PATTACK1": "@PATTACK1 9 5 0 35",
    "UATTACK1": "@UATTACK1 9 5 35 0",
    "PSTAND1": "#PSTAND1 9 1 5 35",
    "ATTACK": "@ATTACK 9 6 0 70",
    "ATTACK1": "@ATTACK1 9 3 0 87",
    "DEATH": "@DEATH 9 4 0 70",
    "DEATHLIE1": "#DEATHLIE1 9 1 4 70",
}

PALETTE = {
    "coat": (0.12, 0.17, 0.38), "facings": (0.62, 0.08, 0.07), "lining": (0.62, 0.08, 0.07),
    "waistcoat": (0.86, 0.85, 0.80), "breeches": (0.86, 0.85, 0.80), "belts": (0.80, 0.70, 0.45),
}


def spec():
    return line.spec(MD, palette=PALETTE)


def make(root, name, build=None):
    return line.make(root, name, dict(spec(), build=build or line.BUILD))


def build(ctx):
    line.build(ctx, spec())


def pose_for(ch, clip):
    line.pose_for(ch, spec(), clip)
