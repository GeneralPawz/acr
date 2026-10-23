"""Continental mounted officer, 18th century (`UsaKOf18`, `USAKOF18.MD`) on the WildMesh horse (#287): the dragoon family
(`acr_assets.cavalry`, docs/families/cavalry.md): a blue coat with red facings and gold lace, buff waistcoat and breeches, a crimson sash, a laced tricorne, pistol and sword; a black horse. Colours from the sprites (the
palette guides them)."""

from acr_assets import cavalry

TYPE = "UsaKOf18"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

MD = {
    "REST": "@REST 9 0 0 61",
    "STAND": "#STAND 9 1 0 0",
    "MOTION_L": "@MOTION_L 9 1 0 5",
    "MOTION_R": "@MOTION_R 9 1 6 13",
    "MOTION_LB": "@MOTION_LB 9 1 13 6",
    "MOTION_RB": "@MOTION_RB 9 1 5 0",
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

PALETTE = dict(cavalry.PALETTE, **{"coat": (0.12, 0.18, 0.38), "facings": (0.70, 0.10, 0.07), "waistcoat": (0.85, 0.80, 0.62), "breeches": (0.85, 0.80, 0.62), "lace": (0.80, 0.64, 0.28), "sash": (0.62, 0.08, 0.14), "saddlecloth": (0.12, 0.18, 0.38), "hat": (0.10, 0.09, 0.08)})

SPEC = {"md": MD, "palette": PALETTE, "dress": "coat", "tack": "military", "held": None, "hat": 'tricorne', "gun": 'pistol', "drawn": True, "blade": 'sword', "facings": True, "lace": True, "sash": True,
        "coat": cavalry.HORSE_COATS["black"]}


def build(ctx):
    cavalry.build(ctx, SPEC)
