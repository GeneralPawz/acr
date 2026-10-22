"""English cavalry standard bearer, 18th century (`EngKF18`, `ENGKF18.MD`) on the WildMesh horse (#287): the dragoon's
rider and horse (`acr_assets.cavalry`) with this type's colours, carrying the standard.
Colours from the sprites (the palette guides them)."""

from acr_assets import cavalry

TYPE = "EngKF18"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

MD = {
    "REST": "@REST 9 0 0 57",
    "STAND": "#STAND 9 1 0 0",
    "MOTION_L": "@MOTION_L 9 1 0 5",
    "MOTION_R": "@MOTION_R 9 1 6 13",
    "MOTION_LB": "@MOTION_LB 9 1 13 6",
    "MOTION_RB": "@MOTION_RB 9 1 5 0",
    "DEATH": "@DEATH 9 2 0 41",
    "DEATHLIE1": "#DEATHLIE1 9 1 2 41",
}

PALETTE = dict(cavalry.PALETTE, **{"coat": (0.70, 0.08, 0.05), "cap": (0.70, 0.08, 0.05), "saddlecloth": (0.72, 0.10, 0.06), "sash": (0.75, 0.10, 0.10)})

SPEC = {"md": MD, "palette": PALETTE, "dress": "coat", "tack": "military", "hat": "cap", "held": "flag",
        "coat": {"body": (0.18, 0.15, 0.14), "hair": (0.08, 0.07, 0.06)}}  # a black horse (sprites)


def build(ctx):
    cavalry.build(ctx, SPEC)
