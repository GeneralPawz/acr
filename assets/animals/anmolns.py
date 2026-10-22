"""Hind (`AnmOlns`, `ANMOLNS.MD`) from the WildMesh Doe (#287), 1.1 m at the shoulder.

No death in the pack: an authored fall."""

from acr_assets import animals

TYPE = "AnmOlns"
CHECKS = ["--anim STAND"]

SPEC = {'blend': 'wildmesh/Doe/SK_Doe.blend',
 'height': 1.25,
 'tris': 10000,
 'clips': {'STAND': ('Idle', 'pose', 1, 0.0),
           'REST': ('Idle', 'loop', 50),
           'MOTION_L': ('Run', 'loop', 6, 0.0, 0.5),
           'MOTION_R': ('Run', 'loop', 6, 0.5, 1.0),
           'MOTION_LB': ('Run', 'loop', 6, 0.0, 0.5),
           'MOTION_RB': ('Run', 'loop', 6, 0.5, 1.0),
           'ATTACK': ('Idle', 'pose', 1, 0.0),
           'DEATH': ('Idle', 'fall', 80, 0.0, 88)}}


def build(ctx):
    animals.build(ctx, SPEC)
