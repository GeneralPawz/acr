"""American militia (`UsaMil`, `USAMIL.MD`, `MECH`, `VISTREL`): a farmer under arms, white
shirt sleeves under a red-brown waistcoat, brown breeches, pale stockings, a black round
hat cocked as a tricorn, powder horn and shot pouch on a strap, and his musket.

Family 11 (`acr_assets.frontier`, docs/families/frontier.md).
"""

from acr_assets import frontier

TYPE = "UsaMil"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

MD = {
    "STAND": "#STAND 9 1 2 0",
    "PSTAND1": "#PSTAND1 9 1 1 0",
    "REST": "@REST 9 2 0 65",
    "PSTAND": "#PSTAND 9 1 4 0",
    "MOTION_L": "@MOTION_L 9 0 0 9",
    "MOTION_R": "@MOTION_R 9 0 10 19",
    "MOTION_RB": "@MOTION_RB 9 0 19 10",
    "MOTION_LB": "@MOTION_LB 9 0 9 0",
    "MOTION_L0": "@MOTION_L0 9 0 0 9",
    "MOTION_R0": "@MOTION_R0 9 0 10 19",
    "MOTION_RB0": "@MOTION_RB0 9 0 19 10",
    "MOTION_LB0": "@MOTION_LB0 9 0 9 0",
    "ATTACK1": "@ATTACK1 9 3 0 86",
    "ATTACK": "@ATTACK 9 4 0 53",
    "PATTACK": "#PATTACK 9 1 4 0",
    "UATTACK": "#UATTACK 9 1 4 0",
    "PATTACK1": "#PATTACK1 9 1 3 0",
    "UATTACK1": "#UATTACK1 9 1 3 0",
    "DEATH": "@DEATH 9 1 0 33",
    "DEATHLIE1": "#DEATHLIE1 9 1 1 33",
}


def spec():
    return frontier.spec(MD, palette={"fvest": (0.50, 0.16, 0.10), "breeches": (0.40, 0.29, 0.17), "hat": (0.08, 0.08, 0.08)},
                          body="fvest", sleeves="fshirt", skirts=False, legs="fhose", layers=("blouse",),
                          kit=("belt_l", "horn", "pouch"), hat="tricorn", hat_at=(0.0, 0.0, -0.02, -6), gun=1.58,
                          fixed=("fshirt",))


def build(ctx):
    frontier.build(ctx, spec())


def pose_for(ch, clip):
    frontier.pose_for(ch, spec(), clip)
