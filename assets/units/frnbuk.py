"""French buccaneer (`FrnBuk`, `FRNBUK.MD`, `MECH`, `VISTREL`, `PIKA`): Exquemelin's
boucanier, a loose linen shirt bloused over a red sash, dark sailor's breeches and leggings,
a red cloak over the back and shoulders, a broad hat turned up on one side, horn, pouch and
knife, and the long buccaneer musket.

Family 11 (`acr_assets.frontier`, docs/families/frontier.md).
"""

from acr_assets import frontier

TYPE = "FrnBuk"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

MD = {
    "REST": "@REST 9 5 0 66",
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
    "ATTACK": "@ATTACK 9 6 0 54",
    "PATTACK1": "#PATTACK1 9 1 5 0",
    "UATTACK1": "#UATTACK1 9 1 5 0",
    "ATTACK1": "@ATTACK1 9 3 0 86",
    "DEATH": "@DEATH 9 4 0 33",
    "PSTAND": "#PSTAND 9 1 5 0",
    "PSTAND1": "#PSTAND1 9 1 5 0",
    "DEATHLIE1": "#DEATHLIE1 9 1 4 33",
}


def spec():
    return frontier.spec(MD, palette={"breeches": (0.20, 0.18, 0.16), "leggings": (0.22, 0.17, 0.12), "hat": (0.30, 0.20, 0.11)},
                          body="fshirt", sleeves="fshirt", skirts=False, layers=("blouse", "sash", "cloak"),
                          kit=("belt_l", "horn", "pouch", "knife"), hat="buccaneer_hat", hat_at=(0.0, 0.0, -0.06, -6),
                          gun=1.72, fixed=("fshirt",))


def build(ctx):
    frontier.build(ctx, spec())


def pose_for(ch, clip):
    frontier.pose_for(ch, spec(), clip)
