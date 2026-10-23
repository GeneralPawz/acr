"""American black bear (`AnmMedSm`, `ANMMEDSM.MD`) from the WildMesh pack's BrownBear (#287),
reshaped and recoloured (`acr_assets.black_bear`): black fur with a tan muzzle and a small
white chest blaze, no shoulder hump, a straight (Roman) face, larger upright ears, shorter
claws, a leaner trunk on longer-looking legs; about 0.9 m at the shoulder.

Clip lengths are ANMMED.MD's: REST 60, MOTION_L/R 18 (the two halves of a walk), *1 = the
run when hunting (9 + 9), ATTACK1 76, DEATH 80, DEATHLIE* its last frame.
"""

from acr_assets import animals

TYPE = "AnmMedSm"
CHECKS = ["--anim STAND"]

SPEC = {
    "blend": "wildmesh/BrownBear/SK_Bear.blend",
    # An adult male black bear: about 0.9 m at the shoulder on all fours (90-150 kg); with no
    # hump the idle pose's top is the withers.
    "height": 0.9,
    "recolour": "black_bear",
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
