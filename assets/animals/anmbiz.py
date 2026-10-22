"""American bison (`AnmBiz`, `ANMBIZ.MD`), 1.85 m at the hump (#287).

Source: "Animated Free Bison" by AneesAnimates (Sketchfab, CC-BY 4.0, see
assets/LICENSES.md), a rigged bison with one walk clip. Idle is the walk's first pose;
ATTACK is an authored head-down charge (lunge), DEATH an authored fall onto the side.
"""

from acr_assets import animals

TYPE = "AnmBiz"
CHECKS = ["--anim STAND"]

WALK = "Armature|Armature|ArmatureAction"

SPEC = {
    "fbx": "sketchfab/animated-free-bison/source/Bison.fbx",
    "textures": {"*": "sketchfab/animated-free-bison/textures/AlbedoMap.png"},
    "head_bone": "Bone.005",
    "tail_bone": "Bone",
    "neck_bones": ["Bone.004", "Bone.005"],
    "height": 1.85,
    "tris": 12000,
    "fps": 24,
    "clips": {
        "STAND": (WALK, "pose", 1, 0.0),
        "REST": (WALK, "pose", 1, 0.0),
        "MOTION_L": (WALK, "loop", 12, 0.0, 0.5),
        "MOTION_R": (WALK, "loop", 12, 0.5, 1.0),
        "MOTION_LB": (WALK, "loop", 12, 0.0, 0.5),
        "MOTION_RB": (WALK, "loop", 12, 0.5, 1.0),
        "ATTACK": (WALK, "lunge", 13, 0.0, 0.45),
        "PATTACK": (WALK, "pose", 1, 0.0),
        "UATTACK": (WALK, "pose", 1, 0.0),
        "PSTAND": (WALK, "pose", 1, 0.0),
        "DEATH": (WALK, "fall", 48, 0.0, 88),
    },
}


def build(ctx):
    animals.build(ctx, SPEC)
