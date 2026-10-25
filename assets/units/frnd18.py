"""French dragoon, 18th century (`FrnD18`, `FRND18.MD`) on the WildMesh horse (#287): the dragoon family
(`acr_assets.cavalry`, docs/families/cavalry.md): a red coat with blue cuffs and lapels, the grey dragoon bonnet (a stocking cap falling back from a black fur turban), carbine and sabre; a black horse with a blue saddlecloth. Colours from the sprites (the
palette guides them)."""

from acr_assets import cavalry

TYPE = "FrnD18"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

MD = {
    "REST": "@REST 9 0 0 57",
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
    "ATTACK1": "@ATTACK1 9 4 1 155",
    "DEATH": "@DEATH 9 2 0 37",
    "DEATHLIE1": "#DEATHLIE1 9 1 2 37",
    "PSTAND": "#PSTAND 9 1 3 0",
    "PSTAND1": "#PSTAND1 9 1 4 0",
}

PALETTE = dict(cavalry.PALETTE, **{"coat": (0.70, 0.10, 0.07), "facings": (0.15, 0.20, 0.50), "cap": (0.70, 0.70, 0.68), "hat": (0.10, 0.09, 0.08), "saddlecloth": (0.15, 0.20, 0.50), "waistcoat": (0.85, 0.80, 0.62)})

SPEC = {"kit": cavalry.KITS["d18"], "md": MD, "palette": PALETTE, "dress": "coat", "tack": "military", "held": None, "hat": 'bonnet', "gun": 'carbine', "drawn": True, "facings": True,
        "coat": cavalry.HORSE_COATS["black"]}


def build(ctx):
    cavalry.build(ctx, SPEC)
