"""Continental light dragoon, 18th century (`UsaD18`, `USAD18.MD`) on the WildMesh horse (#287): the dragoon family
(`acr_assets.cavalry`, docs/families/cavalry.md): a blue coat with white facings and waistcoat, the light dragoons' black leather helmet with a brass comb and a white horsehair crest, carbine and sabre; a bay. Colours from the sprites (the
palette guides them)."""

from acr_assets import cavalry

TYPE = "UsaD18"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

MD = {
    "REST": "@REST 9 0 0 85",
    "STAND": "#STAND 9 1 0 0",
    "MOTION_L": "@MOTION_L 9 1 0 6",
    "MOTION_R": "@MOTION_R 9 1 7 13",
    "MOTION_LB": "@MOTION_LB 9 1 13 7",
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

PALETTE = dict(cavalry.PALETTE, **{"coat": (0.12, 0.18, 0.38), "facings": (0.86, 0.85, 0.80), "waistcoat": (0.86, 0.85, 0.80), "breeches": (0.86, 0.85, 0.80), "cap": (0.08, 0.07, 0.06), "crest": (0.86, 0.85, 0.80), "saddlecloth": (0.62, 0.52, 0.38)})

SPEC = {"md": MD, "palette": PALETTE, "dress": "coat", "tack": "military", "held": None, "hat": 'helmet', "gun": 'carbine', "drawn": True, "facings": True,
        "coat": cavalry.HORSE_COATS["bay"]}


def build(ctx):
    cavalry.build(ctx, SPEC)
