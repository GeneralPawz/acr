"""Pueblo medicine man (`PueSha`, `PUESHA.MD`, "Schamane"): Pueblo (`acr_assets.pueblo` on `native`,
docs/families/pueblo.md): loose hair in bangs, a red
headband, a white face band (generic paint, no sacred mask reproduced), a white pelt over the shoulders,
a red woven stole from the right hip over the left shoulder, a dark kilt, red wrapped leggings,
moccasins, necklaces; a gourd rattle in the left hand, a stone-headed club in the right. He heals
(MEDIK, ATTACK1, `clips.heal`) and strikes with the club (ATTACK).
"""

from acr_assets import pueblo

TYPE = "PueSha"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

# `#` lists keep only their frame count (the full lists in PUESHA.MD).
MD = {
    "STAND": "#STAND 9 1 3 0",
    "MOTION_L": "@MOTION_L 9 1 0 9",
    "MOTION_R": "@MOTION_R 9 1 10 19",
    "MOTION_LB": "@MOTION_LB 9 1 19 10",
    "MOTION_RB": "@MOTION_RB 9 1 9 0",
    "MOTION_L0": "@MOTION_L0 9 1 0 9",
    "MOTION_R0": "@MOTION_R0 9 1 10 19",
    "MOTION_LB0": "@MOTION_LB0 9 1 19 10",
    "MOTION_RB0": "@MOTION_RB0 9 1 9 0",
    "PATTACK0": "#PATTACK0 9 1 6 0",
    "UATTACK0": "#UATTACK0 9 1 6 0",
    "ATTACK": "@ATTACK 9 6 0 50",
    "PATTACK1": "#PATTACK1 9 1 3 0",
    "UATTACK1": "#UATTACK1 9 1 3 0",
    "ATTACK1": "@ATTACK1 9 3 0 89",
    "DEATH": "@DEATH 9 4 0 40",
    "PSTAND": "#PSTAND 9 1 6 0",
    "PSTAND1": "#PSTAND1 9 1 3 0",
    "DEATHLIE1": "#DEATHLIE1 9 1 4 40",
}
WEAPONS = {0: 'MECH', 1: 'MEDIK'}
ACTIVE = {'ATTACK1': 1}
PALETTE = {'pelt': (0.86, 0.84, 0.78), 'pue_kilt': (0.2, 0.18, 0.2)}


def spec():
    return pueblo.spec(MD, WEAPONS, palette=PALETTE, active=ACTIVE, weapon='club', melee='club', paint='white_band', gourd=True, dress=('moccasins', 'cloak'), pdress=('kilt', 'bandolier', 'wraps'), kit=('knife',))


def build(ctx):
    pueblo.build(ctx, spec())


def make(root, name, build=None):
    s = spec()
    return pueblo.make(root, name, dict(s, build=build or s["build"]))


def pose_for(ch, clip):
    pueblo.pose_for(ch, spec(), clip)
