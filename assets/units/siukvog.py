"""Sioux mounted chief (`SiuKVog`, `SIUKVOG.MD`, "Berittener Häuptling"): Plains
(docs/families/plains.md) on the WildMesh horse (`acr_assets.cavalry`, the head from
`acr_assets.plains`): the eagle-feather war bonnet (a beaded brow band, a fan of
black-tipped feathers standing up and back, a short trailer), two wrapped braids, a white
buckskin shirt, a red breechcloth, buckskin leggings, a painted hide shield on the left arm;
bareback on a red blanket with a rope bridle, a spotted horse. ATTACK (index 0, MECH) is the
cavalry cut with a war club (`blade` "club", drawn on `socket.blade_r`). He gallops (`cavalry.NATIVE_MOTIONS`).
"""

from acr_assets import cavalry, plains

TYPE = "SiuKVog"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

MD = {
    "REST": "@REST 9 2 0 57",
    "STAND": "#STAND 9 1 2 0",
    "MOTION_L": "@MOTION_L 9 1 0 4",
    "MOTION_R": "@MOTION_R 9 1 5 9",
    "MOTION_LB": "@MOTION_LB 9 1 9 5",
    "MOTION_RB": "@MOTION_RB 9 1 4 0",
    "MOTION_L0": "@MOTION_L0 9 1 0 4",
    "MOTION_R0": "@MOTION_R0 9 1 5 9",
    "MOTION_LB0": "@MOTION_LB0 9 1 9 5",
    "MOTION_RB0": "@MOTION_RB0 9 1 4 0",
    "ATTACK": "@ATTACK 9 3 0 61",
    "PATTACK": "#PATTACK 9 1 3 0",
    "UATTACK": "#UATTACK 9 1 3 0",
    "DEATH": "@DEATH 9 4 0 37",
    "DEATHLIE1": "#DEATHLIE1 9 1 4 37",
}

PALETTE = dict(cavalry.PALETTE, **{"skin": (0.72, 0.48, 0.33), "tunic": (0.84, 0.76, 0.60), "breeches": (0.58, 0.10, 0.07),
                                   "boots": (0.66, 0.52, 0.34), "saddlecloth": (0.62, 0.10, 0.08), "knapsack": (0.88, 0.84, 0.72),
                                   "cap": (0.94, 0.92, 0.88), "hair": (0.05, 0.04, 0.04)})

SPEC = {"md": MD, "palette": PALETTE, "dress": "native", "shirt": True, "tack": "blanket", "hat": "crown", "held": None,
        "shield": True, "sabre": False, "drawn": True, "blade": "club", "seat_dz": -0.05, "motions": cavalry.NATIVE_MOTIONS,
        "coat": {"body": (0.92, 0.90, 0.86), "hair": (0.25, 0.22, 0.20)}}


def build(ctx):
    plains.mounted(ctx, SPEC, head="bonnet")
