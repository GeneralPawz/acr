"""English drummer, 18th century (`EngB18`, `ENGB18.MD`): EngM18's royal regiment, the drummer
in the royal livery (Morier; Clothing Warrant 1751): red coat faced blue, laced on the
lapels and cuffs in yellow worsted, a tricorn bound with the lace. The side drum (shell in
the blue facings) hangs from the carriage belt over the right shoulder; drumming, marching
and the `0 PIKA` jab from `clips.drummer_clips` / `clips.unit_clips`.
"""

from acr_assets import line

TYPE = "EngB18"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

MD = {
    "REST": "@REST 9 0 0 69",
    "STAND": "#STAND 9 1 0 0",
    "MOTION_L": "@MOTION_L 9 3 0 9",
    "MOTION_R": "@MOTION_R 9 3 10 19",
    "MOTION_LB": "@MOTION_LB 9 3 19 10",
    "MOTION_RB": "@MOTION_RB 9 3 9 0",
    "MOTION_L0": "@MOTION_L0 9 1 0 9",
    "MOTION_R0": "@MOTION_R0 9 1 10 19",
    "MOTION_LB0": "@MOTION_LB0 9 1 19 10",
    "MOTION_RB0": "@MOTION_RB0 9 1 9 0",
    "DEATH": "@DEATH 9 4 0 39",
    "DEATHLIE1": "#DEATHLIE1 9 1 4 39",
    "PATTACK": "#PATTACK 9 1 0 0",
    "ATTACK": "@ATTACK 9 2 0 19",
    "UATTACK": "#UATTACK 9 1 0 0",
    "PSTAND": "#PSTAND 9 1 2 0",
}

PALETTE = {"lace": (0.84, 0.72, 0.30), "belts": (0.85, 0.78, 0.55)}


def spec():
    return line.spec(MD, palette=PALETTE, weapon="drum", gear=("belt_r",), lace={"hat": 0.012, "buttonholes": True}, fixed=("lace",))


def make(root, name, build=None):
    return line.make(root, name, dict(spec(), build=build or line.BUILD))


def build(ctx):
    line.build(ctx, spec())


def pose_for(ch, clip):
    line.pose_for(ch, spec(), clip)
