"""Bison (`AnmBiz`, `ANMBIZ.MD`) from the WildMesh Buffalo (#287), 1.85 m at the hump. NOTE: the
pack's Buffalo is an African (Cape) buffalo, not an American bison: needs another source.

The pack has no attack or death: ATTACK is an authored head-down charge (lunge), DEATH an
authored fall onto the side."""

from acr_assets import animals

TYPE = "AnmBiz"
CHECKS = ["--anim STAND"]

SPEC = {'blend': 'wildmesh/Buffalo/SK_Buffalo.blend',
 'height': 1.85,
 'measure_skip': ['Antler'],
 'tris': 12000,
 'fps': 24,
 'clips': {'STAND': ('Standstraight_Breathing', 'pose', 1, 0.0),
           'REST': ('GrazingOnSpotA_1', 'loop', 105),
           'MOTION_L': ('WalkMedium_F', 'loop', 12, 0.0, 0.5),
           'MOTION_R': ('WalkMedium_F', 'loop', 12, 0.5, 1.0),
           'MOTION_LB': ('WalkMedium_F', 'loop', 12, 0.0, 0.5),
           'MOTION_RB': ('WalkMedium_F', 'loop', 12, 0.5, 1.0),
           'ATTACK': ('Standstraight_Breathing', 'lunge', 13, 0.0, 0.45),
           'PATTACK': ('Standstraight_Breathing', 'pose', 1, 0.0),
           'UATTACK': ('Standstraight_Breathing', 'pose', 1, 0.0),
           'PSTAND': ('Standstraight_Breathing', 'pose', 1, 0.0),
           'DEATH': ('Standstraight_Breathing', 'fall', 48, 0.0, 88)}}


def build(ctx):
    animals.build(ctx, SPEC)
