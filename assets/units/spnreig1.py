"""Spanish armoured horseman (reiter) (`SpnReiG1`, `SPNREIG1.MD`) on the WildMesh horse (#287): the dragoon's
rider and horse (`acr_assets.cavalry`) with this type's colours.
Colours from the sprites (the palette guides them)."""

from acr_assets import cavalry

TYPE = "SpnReiG1"
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
    "ATTACK1": "@ATTACK1 9 4 0 113",
    "DEATH": "@DEATH 9 2 0 37",
    "DEATHLIE1": "#DEATHLIE1 9 1 2 37",
    "PSTAND": "#PSTAND 9 1 3 0",
    "PSTAND1": "#PSTAND1 9 1 4 0",
}

PALETTE = dict(cavalry.PALETTE, **{"coat": (0.72, 0.72, 0.74), "waistcoat": (0.72, 0.72, 0.74), "cap": (0.72, 0.72, 0.74), "saddlecloth": (0.70, 0.10, 0.06), "breeches": (0.60, 0.15, 0.10)})

SPEC = {"md": MD, "palette": PALETTE, "dress": "coat", "tack": "military", "hat": "cap", "held": None}


def build(ctx):
    cavalry.build(ctx, SPEC)
