"""Young alligator (`AnmCroSm`, `ANMCROSM.MD`): the alligator at 1.6 m."""

from acr_assets import animals

TYPE = "AnmCroSm"
CHECKS = ["--anim STAND"]

SPEC = {'blend': 'wildmesh/AmericanAligator/SK_AmericanAligator.blend',
 'length': 1.6,
 'tris': 8000,
 'clips': {'STAND': ('Idle', 'pose', 1, 0.0),
           'MOTION_L': ('Walk fwd', 'loop', 24, 0.0, 0.5),
           'MOTION_R': ('Walk fwd', 'loop', 24, 0.5, 1.0),
           'MOTION_LB': ('Walk fwd', 'loop', 24, 0.0, 0.5),
           'MOTION_RB': ('Walk fwd', 'loop', 24, 0.5, 1.0),
           'ATTACK': ('Attack Aggressive fwd', 'once', 25),
           'PATTACK': ('Attack Aggressive fwd', 'pose', 1, 0.4),
           'UATTACK': ('Attack Aggressive fwd', 'pose', 1, 0.4),
           'PSTAND': ('Idle', 'pose', 1, 0.0),
           'DEATH': ('Idle', 'fall', 50, 0.0, 170)}}


def build(ctx):
    animals.build(ctx, SPEC)
