"""Bear cub (`AnmMedSm`, `ANMMEDSM.MD`): the brown bear at 0.62 m from the WildMesh pack's BrownBear (#287).

Clip lengths are ANMMED.MD's: REST 60, MOTION_L/R 18 (the two halves of a walk), *1 = the
run when hunting (9 + 9), ATTACK1 76, DEATH 80, DEATHLIE* its last frame.
"""

from acr_assets import animals

TYPE = "AnmMedSm"
CHECKS = ["--anim STAND"]

SPEC = {
    "blend": "wildmesh/BrownBear/SK_Bear.blend",
    # A brown bear: about 1 m at the shoulder; the idle pose's top (hump) a little above.
    "height": 0.62,
    "tris": 8000,
    "clips": {
        "STAND": ("Idle", "pose", 1, 0.0),
        "REST": ("Idle_Breathing_01", "loop", 60),
        "MOTION_L": ("Walk", "loop", 18, 0.0, 0.5),
        "MOTION_R": ("Walk", "loop", 18, 0.5, 1.0),
        "MOTION_LB": ("Walk", "loop", 18, 0.0, 0.5),
        "MOTION_RB": ("Walk", "loop", 18, 0.5, 1.0),
        "MOTION_L1": ("Run", "loop", 9, 0.0, 0.5),
        "MOTION_R1": ("Run", "loop", 9, 0.5, 1.0),
        "MOTION_LB1": ("Run", "loop", 9, 0.0, 0.5),
        "MOTION_RB1": ("Run", "loop", 9, 0.5, 1.0),
        "ATTACK1": ("Attack_IdleAngry_01_High", "once", 76),
        "PATTACK1": ("Attack_IdleAngry_01_High", "pose", 1, 0.4),
        "UATTACK1": ("Attack_IdleAngry_01_High", "pose", 1, 0.4),
        "PSTAND1": ("IdleAngry_Breathing_01", "pose", 1, 0.0),
        "DEATH": ("Death_Idle_R01", "real", 80),
        "DEATHLIE1": ("Death_Idle_R01", "pose", 1, 1.0),
        "DEATHLIE2": ("Death_Idle_R01", "pose", 1, 1.0),
        "DEATHLIE3": ("Death_Idle_R01", "pose", 1, 1.0),
    },
}


def build(ctx):
    animals.build(ctx, SPEC)
