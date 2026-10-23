"""Pueblo rifleman (`PueS15`, `PUES15.MD`, "Schütze"): Pueblo (`acr_assets.pueblo` on `native`,
docs/families/pueblo.md): hair in bangs and a chongo, a red
headband, a long white cotton shirt, a red woven sash with its end hanging in front, a dark kilt,
knee-high buckskin boot moccasins, turquoise and shell necklaces; a trade gun (shot on 15) with the
strap, powder horn and shot pouch; the musket's thrust and butt in the melee.
"""

from acr_assets import pueblo

TYPE = "PueS15"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

# `#` lists keep only their frame count (the full lists in PUES15.MD).
MD = {
    "STAND": "#STAND 9 1 3 0",
    "MOTION_L": "@MOTION_L 9 1 0 8",
    "MOTION_R": "@MOTION_R 9 1 9 15",
    "MOTION_LB": "@MOTION_LB 9 1 15 9",
    "MOTION_RB": "@MOTION_RB 9 1 8 0",
    "MOTION_L0": "@MOTION_L0 9 1 0 8",
    "MOTION_R0": "@MOTION_R0 9 1 9 15",
    "MOTION_LB0": "@MOTION_LB0 9 1 15 9",
    "MOTION_RB0": "@MOTION_RB0 9 1 8 0",
    "PATTACK": "#PATTACK 9 1 6 0",
    "UATTACK": "#UATTACK 9 1 6 0",
    "ATTACK": "@ATTACK 9 6 0 60",
    "PATTACK1": "#PATTACK1 9 1 3 0",
    "UATTACK1": "#UATTACK1 9 1 3 0",
    "ATTACK1": "@ATTACK1 9 3 0 49",
    "DEATH": "@DEATH 9 4 0 68",
    "PSTAND": "#PSTAND 9 1 6 0",
    "PSTAND1": "#PSTAND1 9 1 3 0",
    "DEATHLIE1": "#DEATHLIE1 9 1 4 68",
    "PSTAND3": "#PSTAND3 9 1 3 103",
    "PATTACK3": "#PATTACK3 9 23",
    "UATTACK3": "#UATTACK3 9 23",
    "ATTACK3": "#ATTACK3 9 160",
    "TRANSX3": "#TRANSX3 9 23",
    "TRANS3X": "#TRANS3X 9 23",
}
WEAPONS = {0: 'MECH', 1: 'VISTREL'}
ACTIVE = {'ATTACK1': 15}
PALETTE = {'pue_kilt': (0.18, 0.17, 0.2), 'pue_wraps': (0.7, 0.58, 0.4)}


def spec():
    return pueblo.spec(MD, WEAPONS, palette=PALETTE, active=ACTIVE, weapon='gun', melee=None, phair='chongo', paint=None, dress=('moccasins', 'shirt'), pdress=('kilt', 'sash', 'wraps'), kit=('belt_l', 'horn', 'pouch', 'knife'))


def build(ctx):
    pueblo.build(ctx, spec())


def make(root, name, build=None):
    s = spec()
    return pueblo.make(root, name, dict(s, build=build or s["build"]))


def pose_for(ch, clip):
    pueblo.pose_for(ch, spec(), clip)
