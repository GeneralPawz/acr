"""French dragoon, 17th century (`FrnD17`, `FRND17.MD`) on the WildMesh horse (#287): the dragoon family
(`acr_assets.cavalry`, docs/families/cavalry.md): a blue coat with red cuffs, a blue dragoon bonnet, carbine and sabre; a bay with a blue saddlecloth. Colours from the sprites (the
palette guides them).

17th-century kit (`dress` "buff17"): a buff coat with blue cloth sleeves, falling band,
bucket boots, a wheellock carbine and a drawn sword.
"""

from acr_assets import cavalry

TYPE = "FrnD17"
CHECKS = ["--anim STAND", "--anim ATTACK", "--anim DEATHLIE1"]

MD = {
    "REST": "@REST 9 0 0 85",
    "STAND": "#STAND 9 1 0 0",
    "MOTION_L": "@MOTION_L 9 1 0 6",
    "MOTION_R": "@MOTION_R 9 1 7 15",
    "MOTION_LB": "@MOTION_LB 9 1 15 7",
    "MOTION_RB": "@MOTION_RB 9 1 6 0",
    "PATTACK": "#PATTACK 9 1 3 0",
    "UATTACK": "#UATTACK 9 1 3 0",
    "ATTACK": "@ATTACK 9 3 0 61",
    "PATTACK1": "#PATTACK1 9 1 4 0",
    "UATTACK1": "#UATTACK1 9 1 4 0",
    "ATTACK1": "@ATTACK1 9 4 0 155",
    "DEATH": "@DEATH 9 2 0 37",
    "DEATHLIE1": "#DEATHLIE1 9 1 2 37",
    "PSTAND": "#PSTAND 9 1 3 0",
    "PSTAND1": "#PSTAND1 9 1 4 0",
}

PALETTE = dict(cavalry.PALETTE, **cavalry.PALETTE17, **{"dsleeve": (0.22, 0.30, 0.62), "coat": (0.22, 0.30, 0.62), "cap": (0.22, 0.30, 0.62), "hat": (0.10, 0.09, 0.08), "saddlecloth": (0.15, 0.20, 0.50)})

SPEC = {"kit": cavalry.KITS["d17"], "md": MD, "palette": PALETTE, "paint": True, "fixed": cavalry.FIXED17, "dress": "buff17", "tack": "military", "held": None, "hat": 'bonnet', "gun": 'wheellock', "blade": 'sword', "drawn": True,
        "coat": cavalry.HORSE_COATS["bay"]}


def build(ctx):
    cavalry.build(ctx, SPEC)
