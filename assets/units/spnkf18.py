"""Spanish cavalry standard bearer, 18th century (`SpnKF18`, `SPNKF18.MD`) on the WildMesh horse (#287): the dragoon's
rider and horse (`acr_assets.cavalry`) with this type's colours, carrying the standard.
Colours from the sprites (the palette guides them)."""

from acr_assets import cavalry

TYPE = "SpnKF18"
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

PALETTE = dict(cavalry.PALETTE, **{"coat": (0.15, 0.35, 0.15), "cap": (0.10, 0.09, 0.08), "saddlecloth": (0.70, 0.10, 0.06), "sash": (0.90, 0.88, 0.82)})

SPEC = {"md": MD, "palette": PALETTE, "dress": "coat", "tack": "military", "hat": "tricorne", "held": "flag"}


def build(ctx):
    cavalry.build(ctx, SPEC)
