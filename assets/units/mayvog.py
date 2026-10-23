"""Maya chief (`MayVog`, `MAYVOG.MD`, MAY): a war leader in the jaguar tlahuiztli (Bonampak
Room 2's captains wear jaguar pelts), a tall feather headdress, sandals, a spear with a
broad stone head (PIKA: the thrust).
"""

from acr_assets import meso

TYPE = "MayVog"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

MD = {
    "STAND": "#STAND 9 1 3 0",
    "MOTION_L": "@MOTION_L 9 1 0 9",
    "MOTION_R": "@MOTION_R 9 1 10 19",
    "MOTION_LB": "@MOTION_LB 9 1 19 10",
    "MOTION_RB": "@MOTION_RB 9 1 9 0",
    "ATTACK": "@ATTACK 9 3 0 52",
    "PATTACK": "#PATTACK 9 1 3 0",
    "UATTACK": "#UATTACK 9 1 3 0",
    "DEATH": "@DEATH 9 4 0 70",
    "DEATHLIE1": "#DEATHLIE1 9 1 4 70",
}
WEAPONS = {0: 'PIKA'}
ACTIVE = {}

SPEC = meso.spec(MD, weapons=WEAPONS, active=ACTIVE, dress=("tlahuiztli", "hipcloth", "sandals"), suit="jaguar", head="maya", hand_r="spear", shield=None,
                 melee="spear", plume=("macaw", "quetzal"))


def build(ctx):
    meso.build(ctx, SPEC)
