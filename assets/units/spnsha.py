"""The priest (`SpnSha`, `SPNSHA.MD`, "Priester"; England, France, Spain and the USA field
the same sprite): family 13 (`acr_assets.civil`). A Dominican as the sprite shows him: white
habit to the ankle, the black cappa open at the front with a hood collar, sandals, a red
breviary in the left hand, a wooden cross on the chest. `#ATTACK` (200 frames, `MEDIK`) is
`clips.heal`, the heal landing on frame 2 (`SETACTIVEPOINT #ATTACK 1`).
"""

from acr_assets import civil

TYPE = "SpnSha"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

MD = {
    "STAND": "#STAND 9 1 2 0",
    "REST": "@REST 9 2 0 79",
    "MOTION_L": "@MOTION_L 9 1 0 9",
    "MOTION_R": "@MOTION_R 9 1 10 19",
    "MOTION_LB": "@MOTION_LB 9 1 9 0",
    "MOTION_RB": "@MOTION_RB 9 1 19 10",
    "ATTACK": "@ATTACK 9 3 0 199",
    "DEATH": "@DEATH 9 4 0 66",
    "DEATHLIE1": "#DEATHLIE1 9 1 4 66",
}
PALETTE = {"habit": (0.86, 0.85, 0.80), "cappa": (0.09, 0.09, 0.10), "shoes": (0.40, 0.27, 0.15),
           "hair": (0.30, 0.22, 0.14)}


def spec():
    return civil.spec(MD, kind="priest", palette=PALETTE, fixed=("habit", "cappa"), build={"width": 0.95, "limbs": 1.05})


def make(root, name, build=None):
    s = spec()
    if build:
        s["build"] = build
    return civil.make(root, name, s)


def build(ctx):
    civil.build(ctx, spec())


def pose_for(ch, clip):
    civil.pose_for(ch, spec(), clip)
