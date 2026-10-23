"""Continental fusilier, 18th century (`UsaM18`, `USAM18.MD`, "Fusilier"): line infantry
(`acr_assets.line`) in the Continental Army's blue coat faced red of the 1779 regulations
(the Middle States: New York, New Jersey, Pennsylvania, Delaware, Maryland, Virginia), white
turnbacks and buttons, white waistcoat and breeches, black gaiters, white belts, a tricorn
bound with white tape (Mollo & McGregor). Waistcoat and lining keep the palette's white.
"""

from acr_assets import line

TYPE = "UsaM18"
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
    "ATTACK": "@ATTACK 9 6 0 69",
    "ATTACK1": "@ATTACK1 9 3 0 87",
    "DEATH": "@DEATH 9 4 0 70",
    "DEATHLIE1": "#DEATHLIE1 9 1 4 70",
}

PALETTE = {
    "coat": (0.12, 0.16, 0.34), "facings": (0.62, 0.09, 0.07), "lining": (0.86, 0.85, 0.80),
    "waistcoat": (0.86, 0.85, 0.80), "breeches": (0.86, 0.85, 0.80), "belts": (0.84, 0.82, 0.74),
    "lace": (0.90, 0.89, 0.84),
}


def spec():
    return line.spec(MD, palette=PALETTE, fixed=("waistcoat", "lining"), lace={"hat": 0.012})


def make(root, name, build=None):
    return line.make(root, name, dict(spec(), build=build or line.BUILD))


def build(ctx):
    line.build(ctx, spec())


def pose_for(ch, clip):
    line.pose_for(ch, spec(), clip)
