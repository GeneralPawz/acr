"""Spanish dragoon, 17th century (`SpnD17`, `SPND17.MD`) on the WildMesh horse (#287): the dragoon family
(`acr_assets.cavalry`, docs/families/cavalry.md): a buff coat with red cuffs, a grey broad-brimmed hat with a white plume, carbine and sabre; a bay. Colours from the sprites (the
palette guides them).

17th-century kit (`dress` "buff17"): a buff coat with cloth sleeves, falling band, bucket
boots, a wheellock carbine and a drawn sword.
"""

from acr_assets import cavalry

TYPE = "SpnD17"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

MD = {
    "REST": "@REST 9 0 0 57",
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

PALETTE = dict(cavalry.PALETTE, **cavalry.PALETTE17, **{"dsleeve": (0.70, 0.12, 0.08), "coat": (0.72, 0.62, 0.45), "waistcoat": (0.72, 0.62, 0.45), "hat": (0.50, 0.50, 0.48), "crest": (0.86, 0.85, 0.80), "saddlecloth": (0.60, 0.15, 0.10)})

SPEC = {"kit": cavalry.KITS["d17"], "md": MD, "palette": PALETTE, "paint": True, "fixed": cavalry.FIXED17, "dress": "buff17", "tack": "military", "held": None, "hat": 'broad', "gun": 'wheellock', "blade": 'sword', "drawn": True,
        "coat": cavalry.HORSE_COATS["bay"]}


def build(ctx):
    cavalry.build(ctx, SPEC)
