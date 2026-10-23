"""English dragoon, 17th century (`EngD17`, `ENGD17.MD`) on the WildMesh horse (#287): the dragoon family
(`acr_assets.cavalry`, docs/families/cavalry.md): a red coat, a red cap on a black turban, carbine and sabre; a dark dappled grey. Colours from the sprites (the
palette guides them)."""

from acr_assets import cavalry

TYPE = "EngD17"
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

PALETTE = dict(cavalry.PALETTE, **{"coat": (0.70, 0.10, 0.07), "facings": (0.70, 0.10, 0.07), "cap": (0.70, 0.10, 0.07), "hat": (0.10, 0.09, 0.08), "saddlecloth": (0.16, 0.12, 0.10), "waistcoat": (0.85, 0.80, 0.62)})

SPEC = {"md": MD, "palette": PALETTE, "dress": "coat", "tack": "military", "held": None, "hat": 'bonnet', "gun": 'carbine', "drawn": True,
        "coat": cavalry.HORSE_COATS["dapple"]}


def build(ctx):
    cavalry.build(ctx, SPEC)
