"""Jaguar (`AnmJug`, `ANMJUG.MD`) from the WildMesh Leopard (#287): rosettes, 0.7 m at the
shoulder, recoloured towards a jaguar (warmer ground, bolder rosettes; no inner dots)."""

from acr_assets import animals

TYPE = "AnmJug"
CHECKS = ["--anim STAND"]

SPEC = {'blend': 'wildmesh/Leopard/SK_Leopard.blend',
 'height': 0.75,
 'recolour': 'jaguar',
 'tris': 12000,
 'fps': 24,
 'clips': {'STAND': ('Idle', 'pose', 1, 0.0),
           'PSTAND1': ('Idle_3', 'pose', 1, 0.0),
           'MOTION_L': ('Walk-IP', 'loop', 16, 0.0, 0.5),
           'MOTION_R': ('Walk-IP', 'loop', 16, 0.5, 1.0),
           'MOTION_LB': ('Walk-IP', 'loop', 16, 0.0, 0.5),
           'MOTION_RB': ('Walk-IP', 'loop', 16, 0.5, 1.0),
           'MOTION_L1': ('Run-IP', 'loop', 6, 0.0, 0.5),
           'MOTION_R1': ('Run-IP', 'loop', 6, 0.5, 1.0),
           'MOTION_LB1': ('Run-IP', 'loop', 6, 0.0, 0.5),
           'MOTION_RB1': ('Run-IP', 'loop', 6, 0.5, 1.0),
           'ATTACK1': ('Attack_Head_01-IP', 'once', 46),
           'PATTACK1': ('Attack_Legs_02-IP', 'once', 52),
           'UATTACK1': ('Attack_Legs_02-IP', 'once', 52),
           'DEATH': ('Death_Stand_Right-IP', 'real', 92),
           'DEATHLIE1': ('Death_Stand_Right-IP', 'pose', 1, 1.0)}}


def build(ctx):
    animals.build(ctx, SPEC)
