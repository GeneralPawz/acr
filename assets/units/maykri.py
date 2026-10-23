"""Maya peasant (`MayKri`, `MAYKRI.MD`, MAY, PEASANT): a white loincloth with woven ends,
sandals, the hair tied up with a band and a couple of feathers, a jade collar; a stone axe
in the right hand. Work sets from the `.MD`: WORK (building) and ATTACK1 (wood: the axe at
a trunk), ATTACK3 (the field: hacking the ground), ATTACK4 (the load: picked up with
PATTACK4/UATTACK4, carried on the tumpline), ATTACK5 and ATTACK (MECH: the axe's hack);
the numbered walks and ready lines of each set (`meso.work_clips`).
The `.MD` writes the second `@MOTION_RB 9 0 15 8` for `MOTION_LB` (the same frames as the
other units' LB); its second `#PSTAND3` repeats the first.
"""

from acr_assets import meso

TYPE = "MayKri"
CHECKS = ["--anim STAND", "--anim WORK", "--anim DEATHLIE1"]

MD = {
    "REST": "@REST 9 12 0 70",
    "STAND": "#STAND 9 1 12 0",
    "MOTION_L": "@MOTION_L 9 0 0 7",
    "MOTION_RB": "@MOTION_RB 9 0 7 0",
    "MOTION_R": "@MOTION_R 9 0 8 15",
    "MOTION_LB": "@MOTION_LB 9 0 15 8",
    "WORK": "@WORK 9 9 0 22",
    "ATTACK1": "@ATTACK1 9 8 0 43",
    "PATTACK1": "#PATTACK1 9 1 4 85",
    "UATTACK1": "#UATTACK1 9 1 4 85",
    "MOTION_L1": "@MOTION_L1 9 2 0 7",
    "MOTION_RB1": "@MOTION_RB1 9 2 7 0",
    "MOTION_R1": "@MOTION_R1 9 2 8 15",
    "MOTION_LB1": "@MOTION_LB1 9 2 15 8",
    "PSTAND1": "#PSTAND1 9 1 8 0",
    "PSTAND2": "#PSTAND2 9 1 6 57",
    "PSTAND3": "#PSTAND3 9 1 10 0",
    "PSTAND4": "#PSTAND4 9 1 4 85",
    "PSTAND5": "#PSTAND5 9 1 7 0",
    "ATTACK3": "@ATTACK3 9 10 0 57",
    "PATTACK3": "#PATTACK3 9 1 6 57",
    "UATTACK3": "#UATTACK3 9 1 6 57",
    "MOTION_L3": "@MOTION_L3 9 0 0 7",
    "MOTION_RB3": "@MOTION_RB3 9 0 7 0",
    "MOTION_R3": "@MOTION_R3 9 0 8 15",
    "MOTION_LB3": "@MOTION_LB3 9 0 15 8",
    "ATTACK": "@ATTACK 9 7 0 60",
    "PATTACK2": "#PATTACK2 9 1 6 57",
    "UATTACK2": "#UATTACK2 9 1 6 57",
    "ATTACK2": "#ATTACK2 9 1 6 0",
    "MOTION_L2": "@MOTION_L2 9 3 0 9",
    "MOTION_RB2": "@MOTION_RB2 9 3 9 0",
    "MOTION_R2": "@MOTION_R2 9 3 10 19",
    "MOTION_LB2": "@MOTION_LB2 9 3 19 10",
    "PATTACK4": "@PATTACK4 9 4 0 85",
    "UATTACK4": "@UATTACK4 9 4 85 0",
    "ATTACK4": "#ATTACK4 9 1 8 0",
    "MOTION_L4": "@MOTION_L4 9 1 0 9",
    "MOTION_RB4": "@MOTION_RB4 9 1 9 0",
    "MOTION_R4": "@MOTION_R4 9 1 10 19",
    "MOTION_LB4": "@MOTION_LB4 9 1 19 10",
    "PATTACK5": "#PATTACK5 9 1 7 0",
    "UATTACK5": "#UATTACK5 9 1 7 0",
    "ATTACK5": "@ATTACK5 9 7 0 53",
    "MOTION_L5": "@MOTION_L5 9 0 0 7",
    "MOTION_RB5": "@MOTION_RB5 9 0 7 0",
    "MOTION_R5": "@MOTION_R5 9 0 8 15",
    "MOTION_LB5": "@MOTION_LB5 9 0 15 8",
    "DEATH": "@DEATH 9 11 0 60",
    "DEATHLIE1": "#DEATHLIE1 9 1 11 60",
}
WEAPONS = {0: 'MECH'}
ACTIVE = {}

SPEC = meso.spec(MD, weapons=WEAPONS, active=ACTIVE, dress=("maxtlatl", "sandals"), head="topknot", hand_r="axe", shield=None,
                 melee="club", work=True, jewels=("ears", "neck"), mix=0.25)


def build(ctx):
    meso.build(ctx, SPEC)
