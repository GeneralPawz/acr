"""Pueblo horseman with a lance (`PueVsda`, `PUEVSDA.MD`, `PueVsda(PU)` of PUEBLO.NDS) on
the WildMesh horse (#287): bareback on a blanket, a rope bridle (the pack's bridle and
reins), a dark horse. The rider: leggings and breechcloth, a white shirt, long black hair
under a crown of feathers, a round painted shield on the left arm, a lance couched at the
right hip (PIKA: the thrust, ATTACK; no shot). He gallops: 25 px a tick, two gallop
strides per 10-frame cycle. Colours from the sprites.

`PueVsd` (`PUEVSD.MD`) is the same unit with the same sprites; no nation lists it (PUEBLO.NDS
has only `PueVsda(PU)`), so the game never makes one: this model is PueVsda's.
"""

from acr_assets import cavalry

TYPE = "PueVsda"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

MD = {
    "STAND": "#STAND 9 1 3 0",
    "MOTION_L": "@MOTION_L 9 1 0 4",
    "MOTION_R": "@MOTION_R 9 1 5 9",
    "MOTION_LB": "@MOTION_LB 9 1 9 5",
    "MOTION_RB": "@MOTION_RB 9 1 4 0",
    "PATTACK": "#PATTACK 9 1 3 0",
    "UATTACK": "#UATTACK 9 1 3 0",
    "ATTACK": "@ATTACK 9 3 0 35",
    "DEATH": "@DEATH 9 2 0 37",
    "DEATHLIE1": "#DEATHLIE1 9 1 2 37",
    "PSTAND": "#PSTAND 9 1 3 0",
}

PALETTE = dict(cavalry.PALETTE, **{"skin": (0.62, 0.40, 0.28), "tunic": (0.85, 0.82, 0.75), "breeches": (0.75, 0.35, 0.15),
                                   "boots": (0.55, 0.40, 0.25), "saddlecloth": (0.70, 0.15, 0.08), "knapsack": (0.85, 0.80, 0.70),
                                   "cap": (0.90, 0.55, 0.15), "hair": (0.05, 0.04, 0.04)})

SPEC = {"md": MD, "palette": PALETTE, "dress": "native", "shirt": True, "tack": "blanket", "hat": "crown", "held": "lance",
        "shield": True, "sabre": False, "seat_dz": -0.05, "motions": cavalry.NATIVE_MOTIONS,
        "coat": {"body": (0.30, 0.22, 0.16), "hair": (0.10, 0.08, 0.07)}}


def build(ctx):
    cavalry.build(ctx, SPEC)
