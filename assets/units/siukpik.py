"""Sioux mounted lancer (`SiuKPik`, `SIUKPIK.MD`, "Berittener Lanzenkämpfer"): Plains
(docs/families/plains.md) on the WildMesh horse (`acr_assets.cavalry`, the head from
`acr_assets.plains`): bareback on a painted blanket with a rope bridle, a buckskin shirt,
breechcloth and leggings, two wrapped braids and one eagle feather, a round hide shield on
the left arm; the lance couched in the right fist, thrust in ATTACK (index 0, PIKA). He
gallops (`cavalry.NATIVE_MOTIONS`). Colours from the sprites.
"""

from acr_assets import cavalry, plains

TYPE = "SiuKPik"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

MD = {
    "REST": "@REST 9 2 0 57",
    "STAND": "#STAND 9 1 2 0",
    "MOTION_L": "@MOTION_L 9 1 0 4",
    "MOTION_R": "@MOTION_R 9 1 5 9",
    "MOTION_RB": "@MOTION_RB 9 1 9 5",
    "MOTION_LB": "@MOTION_LB 9 1 4 0",
    "PSTAND": "#PSTAND 9 1 2 0",
    "MOTION_L0": "@MOTION_L0 9 1 0 4",
    "MOTION_R0": "@MOTION_R0 9 1 5 9",
    "MOTION_LB0": "@MOTION_LB0 9 1 9 5",
    "MOTION_RB0": "@MOTION_RB0 9 1 4 0",
    "PATTACK": "#PATTACK 9 1 2 0",
    "UATTACK": "#UATTACK 9 1 2 0",
    "ATTACK": "@ATTACK 9 3 0 36",
    "DEATH": "@DEATH 9 4 0 37",
    "DEATHLIE1": "#DEATHLIE1 9 1 4 37",
}

PALETTE = dict(cavalry.PALETTE, **{"skin": (0.70, 0.46, 0.32), "tunic": (0.70, 0.56, 0.38), "breeches": (0.14, 0.18, 0.34),
                                   "boots": (0.62, 0.48, 0.31), "saddlecloth": (0.62, 0.14, 0.10), "knapsack": (0.86, 0.80, 0.66),
                                   "cap": (0.92, 0.90, 0.85), "hair": (0.05, 0.04, 0.04), "wood": (0.45, 0.30, 0.16)})

SPEC = {"md": MD, "palette": PALETTE, "dress": "native", "shirt": True, "tack": "blanket", "hat": "feathers", "held": "lance",
        "shield": True, "sabre": False, "seat_dz": -0.05, "motions": cavalry.NATIVE_MOTIONS,
        "coat": {"body": (0.55, 0.38, 0.24), "hair": (0.12, 0.09, 0.07)}}


def build(ctx):
    plains.mounted(ctx, SPEC, head="eagle")
