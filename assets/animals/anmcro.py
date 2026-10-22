"""American alligator (`AnmCro`, `ANMCRO.MD`) from the WildMesh AmericanAligator (#287), 3.4 m long.

No death in the pack: an authored roll onto the back."""

from acr_assets import animals

TYPE = "AnmCro"
CHECKS = ["--anim STAND"]

SPEC = {'blend': 'wildmesh/AmericanAligator/SK_AmericanAligator.blend',
 'length': 3.4,
 'tris': 12000,
 'clips': {'STAND': ('Idle', 'pose', 1, 0.0),
           'REST': ('Idle', 'loop', 48),
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
