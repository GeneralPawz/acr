"""English musketeer, 18th century (`EngM18`, `ENGM18.MD`): red coat with open skirts,
white waistcoat and breeches, black gaiters, tricorn, buff crossbelts, knapsack, cartridge
box, Brown Bess. Skeleton H with all layers of #178/#183 (docs/assets-humans.md).

Colours are projected from the `#STAND` sprites (lighting reduced, mirrored directions for
the symmetric layers); without a game folder the palette below is used.
"""

from acr_assets import line

TYPE = "EngM18"
# Sprite checks run by `cargo xtask assets --check` (arguments of `acr model-check`).
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

# Animation lines of ENGM18.MD: the clips are keyed on their frame counts.
MD = {
    "STAND": "#STAND 9 1 5 0",
    "MOTION_L": "@MOTION_L 9 1 0 9",
    "MOTION_R": "@MOTION_R 9 1 10 19",
    "MOTION_LB": "@MOTION_LB 9 1 19 10",
    "MOTION_RB": "@MOTION_RB 9 1 9 0",
    "PATTACK": "@PATTACK 9 5 0 35",
    "UATTACK": "@UATTACK 9 5 35 0",
    "PSTAND": "#PSTAND 9 1 5 35",
    "PATTACK1": "@PATTACK1 9 5 0 35",
    "UATTACK1": "@UATTACK1 9 5 35 0",
    "PSTAND1": "#PSTAND1 9 1 5 35",
    "ATTACK": "@ATTACK 9 6 0 70",
    "ATTACK1": "@ATTACK1 9 3 0 86",
    "DEATH": "@DEATH 9 4 0 70",
    "DEATHLIE1": "#DEATHLIE1 9 1 4 70",
}

# Colours, build and fit: the line infantry defaults (`line.PALETTE`, `BUILD`, `FIT`) are
# this unit's, picked from the sprites and fitted to the #STAND silhouette (tools/silhouette.py).
# Facings blue: a royal regiment. `BUILD["garments"]` False builds the shells of #178.
PALETTE = dict(line.PALETTE)
BUILD = dict(line.BUILD)
FIT = dict(line.FIT)


def spec():
    return line.spec(MD, palette=PALETTE, build=BUILD, fit=FIT)


def make(root, name, build=None):
    """The musketeer under `root` (the silhouette tool's entry)."""
    return line.make(root, name, dict(spec(), build=build or BUILD))


def build(ctx):
    line.build(ctx, spec())


def pose_for(ch, clip):
    line.pose_for(ch, spec(), clip)
