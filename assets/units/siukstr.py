"""Sioux mounted warrior with a gun (`SiuKStr`, `SIUKSTR.MD`, `SiuKStr(SI)` of SIU.NDS) on
the WildMesh horse (#287): bareback on a painted blanket, a rope bridle (the pack's bridle
and reins; girth and stirrups gone), a grey horse. The rider: bare legs in leggings and
breechcloth, a shirt, long black hair with two feathers, a round hide shield on the left
arm, the gun upright on the right thigh (SHOTFOGFORPISTOL, the shot: ATTACK1); the cut
(ATTACK) is index 0 (MECH). He gallops: 25 px a tick, two gallop strides per 10-frame cycle
(`cavalry.NATIVE_MOTIONS`). Colours from the sprites.
"""

from acr_assets import cavalry

TYPE = "SiuKStr"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

MD = {
    "REST": "@REST 9 0 0 61",
    "STAND": "#STAND 9 1 0 0",
    "MOTION_L": "@MOTION_L 9 1 0 4",
    "MOTION_R": "@MOTION_R 9 1 5 9",
    "MOTION_LB": "@MOTION_LB 9 1 9 5",
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

PALETTE = dict(cavalry.PALETTE, **{"skin": (0.62, 0.40, 0.28), "tunic": (0.75, 0.15, 0.10), "breeches": (0.55, 0.40, 0.25),
                                   "boots": (0.55, 0.40, 0.25), "saddlecloth": (0.55, 0.60, 0.25), "knapsack": (0.80, 0.70, 0.55),
                                   "cap": (0.92, 0.90, 0.85), "hair": (0.05, 0.04, 0.04)})

SPEC = {"md": MD, "palette": PALETTE, "dress": "native", "shirt": True, "tack": "blanket", "hat": "feathers", "held": None,
        "gun": "carbine", "shield": True, "sabre": False, "seat_dz": -0.05, "motions": cavalry.NATIVE_MOTIONS,
        "coat": {"body": (0.95, 0.95, 0.95), "hair": (0.30, 0.30, 0.30)}}


def build(ctx):
    cavalry.build(ctx, SPEC)
