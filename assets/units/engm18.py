"""English musketeer, 18th century (`EngM18`, `ENGM18.MD`): red coat with open skirts,
white waistcoat and breeches, black gaiters, tricorn, buff crossbelts, knapsack, cartridge
box, Brown Bess. Skeleton H with all layers of #178/#183 (docs/assets-humans.md).

Colours are projected from the `#STAND` sprites (lighting reduced, mirrored directions for
the symmetric layers); without a game folder the palette below is used.
"""

from acr_assets import line, line_kit

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
    # The reload stance of a SLOWRECHARGE musketeer (#255, `poses.STANCE4_CLIPS`).
    "PSTAND3": "#PSTAND3 9 1 3 130",
    "PATTACK3": "#PATTACK3 9 56 5 0 5 1 5 2 5 3 5 4 5 5 5 6 5 7 5 8 5 9 5 10 5 11 5 12 5 13 5 14 5 15 5 16 5 17 5 18 5 19 5 20 5 21 5 22 5 23 5 24 5 25 5 26 5 27 5 28 5 29 5 30 5 31 5 32 5 33 5 34 5 35 3 149 3 148 3 147 3 146 3 145 3 144 3 143 3 142 3 141 3 140 3 139 3 138 3 137 3 136 3 135 3 134 3 133 3 132 3 131 3 130",
    "UATTACK3": "#UATTACK3 9 56 3 130 3 131 3 132 3 133 3 134 3 135 3 136 3 137 3 138 3 139 3 140 3 141 3 142 3 143 3 144 3 145 3 146 3 147 3 148 3 149 5 35 5 34 5 33 5 32 5 31 5 30 5 29 5 28 5 27 5 26 5 25 5 24 5 23 5 22 5 21 5 20 5 19 5 18 5 17 5 16 5 15 5 14 5 13 5 12 5 11 5 10 5 9 5 8 5 7 5 6 5 5 5 4 5 3 5 2 5 1 5 0",
    "ATTACK3": "#ATTACK3 9 88 3 87 3 88 3 89 3 90 3 91 3 92 3 93 3 94 3 95 3 96 3 97 3 98 3 99 3 100 3 101 3 102 3 103 3 104 3 105 3 106 3 107 3 108 3 109 3 110 3 111 3 112 3 113 3 114 3 115 3 116 3 117 3 118 3 119 3 120 3 121 3 122 3 123 3 124 3 125 3 126 3 127 3 128 3 129 3 130 3 129 3 128 3 127 3 126 3 125 3 124 3 123 3 122 3 121 3 120 3 119 3 118 3 117 3 116 3 115 3 114 3 113 3 112 3 111 3 110 3 109 3 108 3 107 3 106 3 105 3 104 3 103 3 102 3 101 3 100 3 99 3 98 3 97 3 96 3 95 3 94 3 93 3 92 3 91 3 90 3 89 3 88 3 87 3 86",
    "TRANSX3": "#TRANSX3 9 20 3 149 3 148 3 147 3 146 3 145 3 144 3 143 3 142 3 141 3 140 3 139 3 138 3 137 3 136 3 135 3 134 3 133 3 132 3 131 3 130",
    "TRANS3X": "#TRANS3X 9 20 3 130 3 131 3 132 3 133 3 134 3 135 3 136 3 137 3 138 3 139 3 140 3 141 3 142 3 143 3 144 3 145 3 146 3 147 3 148 3 149",
    "DEATH": "@DEATH 9 4 0 70",
    "DEATHLIE1": "#DEATHLIE1 9 1 4 70",
}

# Colours, build and fit: the line infantry defaults (`line.PALETTE`, `BUILD`, `FIT`) are
# this unit's, picked from the sprites and fitted to the #STAND silhouette (tools/silhouette.py).
# Facings blue: a royal regiment. `BUILD["garments"]` False builds the shells of #178.
PALETTE = dict(line.PALETTE)
BUILD = dict(line.BUILD)
FIT = dict(line.FIT)


# None: the scripted tricorn (`gear.tricorn`); a generated hat (`genfit`, local source under
# ACR_SOURCES/generated, docs/assets-humans.md "Generated gear"): fitted to this head, baked.
HAT_SOURCE = "pixal3d/eq_tricorn_hat.glb"


# Worn kit (`line_kit`): solid crossbelts and modelled items over the coat.
KIT = line_kit.KITS["eng"]


def spec(hat_source=HAT_SOURCE):
    return line.spec(MD, kit=KIT, palette=PALETTE, build=BUILD, fit=FIT, hat_source=hat_source)


def make(root, name, build=None):
    """The musketeer under `root` (the silhouette tool's entry)."""
    return line.make(root, name, dict(spec(), build=build or BUILD))


def build(ctx):
    line.build(ctx, spec())


def pose_for(ch, clip):
    line.pose_for(ch, spec(), clip)
