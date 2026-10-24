"""Jäger (`EngTapDragon`, `ENGTAPDRAGON.MD`, the neutral hunter; it plays EngTap's sprites):
a hunting shirt dyed green with the shoulder cape, a green sash, buckskin leggings, a round
hat, horn, pouch, tomahawk and knife, and the rifle.

Family 11 (`acr_assets.frontier`, docs/families/frontier.md).
"""

from acr_assets import frontier, line

TYPE = "EngTapDragon"
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
    # The reload stance of a SLOWRECHARGE musketeer (#255, `poses.STANCE4_CLIPS`).
    "PSTAND3": "#PSTAND3 9 1 3 87",
    "PATTACK3": "#PATTACK3 9 21 3 149 3 148 3 147 3 146 3 145 3 144 3 143 3 142 3 141 3 140 3 139 3 138 3 137 3 136 3 135 3 134 3 133 3 132 3 131 3 130 3 129",
    "UATTACK3": "#UATTACK3 9 21 3 129 3 130 3 131 3 132 3 133 3 134 3 135 3 136 3 137 3 138 3 139 3 140 3 141 3 142 3 143 3 144 3 145 3 146 3 147 3 148 3 149",
    "ATTACK3": "#ATTACK3 9 84 3 87 3 88 3 89 3 90 3 91 3 92 3 93 3 94 3 95 3 96 3 97 3 98 3 99 3 100 3 101 3 102 3 103 3 104 3 105 3 106 3 107 3 108 3 109 3 110 3 111 3 112 3 113 3 114 3 115 3 116 3 117 3 118 3 119 3 120 3 121 3 122 3 123 3 124 3 125 3 126 3 127 3 128 3 129 3 128 3 127 3 126 3 125 3 124 3 123 3 122 3 121 3 120 3 119 3 118 3 117 3 116 3 115 3 114 3 113 3 112 3 111 3 110 3 109 3 108 3 107 3 106 3 105 3 104 3 103 3 102 3 101 3 100 3 99 3 98 3 97 3 96 3 95 3 94 3 93 3 92 3 91 3 90 3 89 3 88",
    "TRANSX3": "#TRANSX3 9 21 3 149 3 148 3 147 3 146 3 145 3 144 3 143 3 142 3 141 3 140 3 139 3 138 3 137 3 136 3 135 3 134 3 133 3 132 3 131 3 130 3 129",
    "TRANS3X": "#TRANS3X 9 21 3 129 3 130 3 131 3 132 3 133 3 134 3 135 3 136 3 137 3 138 3 139 3 140 3 141 3 142 3 143 3 144 3 145 3 146 3 147 3 148 3 149",
    "DEATH": "@DEATH 9 4 0 33",
    "PSTAND": "#PSTAND 9 1 5 0",
    "PSTAND1": "#PSTAND1 9 1 5 0",
    "DEATHLIE1": "#DEATHLIE1 9 1 4 33",
}


def spec():
    return frontier.spec(MD, palette={"hunting": (0.24, 0.31, 0.17), "fsash": (0.16, 0.22, 0.12), "hat": (0.12, 0.10, 0.08)},
                          hat="round_hat", hat_at=(0.0, 0.0, -0.06, -6), mix=0.15, fixed=("hunting", "fsash", "hat"))


def build(ctx):
    frontier.build(ctx, spec(), make_fn=line.with_sidearm(frontier.make, "tomahawk"))


def pose_for(ch, clip):
    frontier.pose_for(ch, spec(), clip)
