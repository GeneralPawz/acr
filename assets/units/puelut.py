"""Pueblo archer (`PueLut`, `PUELUT.MD`, "Bogenschütze"): Pueblo (`acr_assets.pueblo` on `native`,
docs/families/pueblo.md): loose hair in bangs, a red headband,
a sleeveless shirt of woven cotton (green and black zig-zags), a dark kilt with a woven border, a red
sash, red wrapped leggings, moccasins, turquoise and shell necklaces; a self bow and quiver (fire arrow
1, arrow 2, loosed on 44), a stone-headed war club for the melee (index 0, `socket.club_r`).
"""

from acr_assets import pueblo

TYPE = "PueLut"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

# `#` lists keep only their frame count (the full lists in PUELUT.MD).
MD = {
    "STAND": "#STAND 9 1 1 0",
    "PSTAND": "#PSTAND 9 1 1 0",
    "PSTAND1": "#PSTAND1 9 1 1 0",
    "PSTAND2": "#PSTAND2 9 1 1 0",
    "MOTION_L": "@MOTION_L 9 0 0 7",
    "MOTION_R": "@MOTION_R 9 0 8 15",
    "MOTION_RB": "@MOTION_RB 9 0 15 8",
    "MOTION_LB": "@MOTION_LB 9 0 7 0",
    "MOTION_L0": "@MOTION_L0 9 0 0 7",
    "MOTION_R0": "@MOTION_R0 9 0 8 15",
    "MOTION_LB0": "@MOTION_LB0 9 0 15 8",
    "MOTION_RB0": "@MOTION_RB0 9 0 7 0",
    "ATTACK2": "#ATTACK2 9 79",
    "ATTACK1": "#ATTACK1 9 79",
    "ATTACK": "@ATTACK 9 2 0 60",
    "DEATH": "@DEATH 9 3 0 65",
    "DEATHLIE1": "#DEATHLIE1 9 1 3 65",
    "DEATHLIE2": "#DEATHLIE2 9 1 3 65",
    "DEATHLIE3": "#DEATHLIE3 9 1 3 65",
}
WEAPONS = {0: 'MECH', 1: 'OSTRELA', 2: 'STRELA'}
ACTIVE = {'ATTACK1': 44, 'ATTACK2': 44}
PALETTE = {'pue_kilt': (0.16, 0.16, 0.2)}


def spec():
    return pueblo.spec(MD, WEAPONS, palette=PALETTE, active=ACTIVE, weapon='bow', melee='club', paint='red_black', dress=('moccasins',), pdress=('tunic', 'kilt', 'sash', 'wraps'), kit=('quiver', 'knife'))


def build(ctx):
    pueblo.build(ctx, spec())


def make(root, name, build=None):
    s = spec()
    return pueblo.make(root, name, dict(s, build=build or s["build"]))


def pose_for(ch, clip):
    pueblo.pose_for(ch, spec(), clip)
