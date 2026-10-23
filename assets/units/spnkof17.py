"""Spanish mounted officer, 17th century (`SpnKOf17`, `SPNKOF17.MD`) on the WildMesh horse (#287): the dragoon family
(`acr_assets.cavalry`, docs/families/cavalry.md): a buff coat laced gold, a red sash, a black broad-brimmed hat with a white plume, pistol and sword; a grey with a green saddlecloth. Colours from the sprites (the
palette guides them).

17th-century kit (`dress` "buff17"): a buff coat with buff sleeves, falling band, sash,
bucket boots, pistol and sword.
"""

from acr_assets import cavalry

TYPE = "SpnKOf17"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

MD = {
    "REST": "@REST 9 0 0 103",
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
    "ATTACK1": "@ATTACK1 9 4 0 113",
    "DEATH": "@DEATH 9 2 0 37",
    "DEATHLIE1": "#DEATHLIE1 9 1 2 37",
    "PSTAND": "#PSTAND 9 1 3 0",
    "PSTAND1": "#PSTAND1 9 1 4 0",
}

PALETTE = dict(cavalry.PALETTE, **cavalry.PALETTE17, **{"coat": (0.72, 0.62, 0.45), "waistcoat": (0.72, 0.62, 0.45), "lace": (0.80, 0.64, 0.28), "sash": (0.70, 0.10, 0.07), "hat": (0.10, 0.09, 0.08), "crest": (0.86, 0.85, 0.80), "saddlecloth": (0.15, 0.42, 0.18)})

SPEC = {"md": MD, "palette": PALETTE, "paint": True, "fixed": cavalry.FIXED17, "dress": "buff17", "sleeves": "buff", "tack": "military", "held": None, "hat": 'broad', "gun": 'pistol', "drawn": True, "blade": 'sword', "sash": True,
        "coat": cavalry.HORSE_COATS["grey"]}


def build(ctx):
    cavalry.build(ctx, SPEC)
