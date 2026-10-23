"""English trapper (`EngTap`, `ENGTAP.MD`, `MECH`, `VISTREL`, `PIKA`): a backwoodsman in a
fringed linen hunting shirt to mid-thigh with a shoulder cape, a red sash, buckskin
leggings, a fur cap with a tail, powder horn and shot pouch on a strap, tomahawk and knife
in the belt, and a long rifle.

Family 11 (`acr_assets.frontier`, docs/families/frontier.md).
"""

from acr_assets import frontier

TYPE = "EngTap"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

MD = {
    "REST": "@REST 9 5 0 65",
    "STAND": "#STAND 9 1 5 0",
    "MOTION_L": "@MOTION_L 9 1 0 7",
    "MOTION_R": "@MOTION_R 9 1 8 15",
    "MOTION_LB": "@MOTION_LB 9 1 15 8",
    "MOTION_RB": "@MOTION_RB 9 1 7 0",
    "MOTION_L0": "@MOTION_L0 9 1 0 7",
    "MOTION_R0": "@MOTION_R0 9 1 8 15",
    "MOTION_LB0": "@MOTION_LB0 9 1 15 8",
    "MOTION_RB0": "@MOTION_RB0 9 1 7 0",
    "PATTACK": "#PATTACK 9 1 5 0",
    "UATTACK": "#UATTACK 9 1 5 0",
    "ATTACK": "@ATTACK 9 6 0 53",
    "PATTACK1": "#PATTACK1 9 1 5 0",
    "UATTACK1": "#UATTACK1 9 1 5 0",
    "ATTACK1": "@ATTACK1 9 3 0 86",
    "DEATH": "@DEATH 9 4 0 33",
    "PSTAND": "#PSTAND 9 1 5 0",
    "PSTAND1": "#PSTAND1 9 1 5 0",
    "DEATHLIE1": "#DEATHLIE1 9 1 4 33",
}


def spec():
    return frontier.spec(MD)


def build(ctx):
    frontier.build(ctx, spec())


def pose_for(ch, clip):
    frontier.pose_for(ch, spec(), clip)
