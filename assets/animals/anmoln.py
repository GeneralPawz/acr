"""Elk stag (`AnmOln`, `ANMOLN.MD`) from the WildMesh Elk (#287), 1.5 m at the shoulder.

No attack or death in the pack: an authored antler butt and fall."""

from acr_assets import animals

TYPE = "AnmOln"
CHECKS = ["--anim STAND"]

SPEC = {'blend': 'wildmesh/Elk/SK_Elk.blend',
 'height': 1.5,
 'measure_skip': ['Chifre'],
 'tris': 12000,
 'clips': {'STAND': ('Idle', 'pose', 1, 0.0),
           'REST': ('Idle', 'loop', 50),
           'MOTION_L': ('Walk', 'loop', 8, 0.0, 0.5),
           'MOTION_R': ('Walk', 'loop', 8, 0.5, 1.0),
           'MOTION_LB': ('Walk', 'loop', 8, 0.0, 0.5),
           'MOTION_RB': ('Walk', 'loop', 8, 0.5, 1.0),
           'ATTACK': ('Idle', 'lunge', 50, 0.0, 0.5),
           'DEATH': ('Idle', 'fall', 80, 0.0, 88)}}


def build(ctx):
    animals.build(ctx, SPEC)
