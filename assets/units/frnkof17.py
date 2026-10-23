"""French mounted officer, 17th century (`FrnKOf17`, `FRNKOF17.MD`) on the WildMesh horse (#287): the dragoon family
(`acr_assets.cavalry`, docs/families/cavalry.md): a light blue coat laced gold, a white sash, a dark blue broad-brimmed hat with a blue plume, pistol and sword; a grey with a blue saddlecloth. Colours from the sprites (the
palette guides them)."""

from acr_assets import cavalry

TYPE = "FrnKOf17"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

MD = {
    "REST": "@REST 9 0 0 104",
    "STAND": "#STAND 9 1 0 0",
    "MOTION_L": "@MOTION_L 9 1 0 6",
    "MOTION_R": "@MOTION_R 9 1 7 15",
    "MOTION_LB": "@MOTION_LB 9 1 15 7",
    "MOTION_RB": "@MOTION_RB 9 1 6 0",
    "PATTACK": "#PATTACK 9 1 3 0",
    "UATTACK": "#UATTACK 9 1 3 0",
    "ATTACK": "@ATTACK 9 0 0 111",
    "PATTACK1": "#PATTACK1 9 1 4 0",
    "UATTACK1": "#UATTACK1 9 1 4 0",
    "ATTACK1": "@ATTACK1 9 4 0 113",
    "DEATH": "@DEATH 9 2 0 37",
    "DEATHLIE1": "#DEATHLIE1 9 1 2 37",
    "PSTAND": "#PSTAND 9 1 3 0",
    "PSTAND1": "#PSTAND1 9 1 4 0",
}

PALETTE = dict(cavalry.PALETTE, **{"coat": (0.50, 0.62, 0.80), "waistcoat": (0.50, 0.62, 0.80), "lace": (0.80, 0.64, 0.28), "sash": (0.86, 0.85, 0.80), "hat": (0.15, 0.20, 0.45), "crest": (0.30, 0.40, 0.75), "saddlecloth": (0.15, 0.20, 0.50)})

SPEC = {"md": MD, "palette": PALETTE, "dress": "coat", "tack": "military", "held": None, "hat": 'broad', "gun": 'pistol', "drawn": True, "blade": 'sword', "lace": True, "sash": True,
        "coat": cavalry.HORSE_COATS["grey"]}


def build(ctx):
    cavalry.build(ctx, SPEC)
