"""Pueblo war chief (`PueVog`, `PUEVOG.MD`, "Häuptling"): Pueblo (`acr_assets.pueblo` on `native`,
docs/families/pueblo.md): loose hair in bangs, a red
headband, a Spanish-cut buckskin jacket with sleeves, a red and black plaid blanket over the shoulders,
a red sash, a dark kilt, red wrapped leggings, moccasins, turquoise and shell necklaces; a
stone-headed war club (MECH, ATTACK).
"""

from acr_assets import pueblo

TYPE = "PueVog"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

# `#` lists keep only their frame count (the full lists in PUEVOG.MD).
MD = {
    "STAND": "#STAND 9 1 3 0",
    "MOTION_L": "@MOTION_L 9 1 0 9",
    "MOTION_R": "@MOTION_R 9 1 10 19",
    "MOTION_LB": "@MOTION_LB 9 1 19 10",
    "MOTION_RB": "@MOTION_RB 9 1 9 0",
    "ATTACK": "@ATTACK 9 3 0 55",
    "PATTACK": "#PATTACK 9 1 3 0",
    "UATTACK": "#UATTACK 9 1 3 0",
    "DEATH": "@DEATH 9 4 0 60",
    "DEATHLIE1": "#DEATHLIE1 9 1 4 60",
}
WEAPONS = {0: 'MECH'}
ACTIVE = None
PALETTE = {'pue_jerkin': (0.72, 0.58, 0.3), 'pue_blanket': (0.58, 0.1, 0.07), 'pue_kilt': (0.2, 0.1, 0.08)}


def spec():
    return pueblo.spec(MD, WEAPONS, palette=PALETTE, active=ACTIVE, weapon='club', melee='club', paint='red_face_half', sleeves=True, dress=('moccasins',), pdress=('jerkin', 'blanket', 'kilt', 'sash', 'wraps'), kit=('knife',))


def build(ctx):
    pueblo.build(ctx, spec())


def make(root, name, build=None):
    s = spec()
    return pueblo.make(root, name, dict(s, build=build or s["build"]))


def pose_for(ch, clip):
    pueblo.pose_for(ch, spec(), clip)
