"""Bald eagle (`AnmEgl`, `ANMEGL.MD`) from the WildMesh pack's BaldEagle (#287).

The pack ships the eagle as FBX only (mesh plus one FBX per clip) and its materials do not
survive import: body and feather textures are assigned by hand, the feathers alpha-clipped.

ANMEGL.MD has STAND, MOTION_L (236 frames), DEATH and DEATHLIE1, and every one of them is
a flying bird drawn around the ground point (`acr model-ref`, measured on all 9 views): the
silhouette's centre stays about 0.3 m (screen height) above the ground point, its lowest
wing tip 0.3-0.8 m *below* it, and no shadow is drawn. MOTION_L flaps three times
(frames 0-95), glides (104-231) and flaps again into frame 0 (232-235). The sprite therefore
gives the flight no height of its own; in 3D the wings would go into the ground at the
sprite's height, so MOTION_L is lifted just enough that its lowest wing tip stays clear
(`lift: "clear"`), which keeps the bird as close to the sprite's place on screen as a body
above the ground can be. Standing is the pack's perched pose; ATTACK1 (not in the MD, for
mauling) is a swoop from the glide at flight height; DEATH falls from standing.
"""

from acr_assets import animals

TYPE = "AnmEgl"
CHECKS = ["--anim STAND"]

TEX = "wildmesh/BaldEagle/Textures/"
ANIM = "wildmesh/BaldEagle/Animations/"

SPEC = {
    "fbx": "wildmesh/BaldEagle/Meshes/BaldEagle.fbx",
    "anims": {
        "stand": ANIM + "Stand_Ground_00.fbx",
        "fly": ANIM + "Fly_Heavy.fbx",
        "glide": ANIM + "Fly_Glide.fbx",
        "death": ANIM + "Death_Stand_Ground_Left.fbx",
    },
    "textures": {
        "Feathers": (TEX + "BaldEagle_Feathers_BaseColor.png", True),
        "Eyes": "#c89a2a",
        "*": TEX + "BaldEagle_BaseColor.png",
    },
    # A bald eagle stands about 0.8 m tall.
    "height": 0.8,
    "tris": 12000,
    "lift": {"MOTION_L": "clear", "ATTACK1": "clear"},
    "clips": {
        "STAND": ("stand", "pose", 1, 0.0),
        # The sprite's phases: flapping 0-103, gliding 104-231, flapping 232-235.
        "MOTION_L": [("fly", "cycle", 104), ("glide", "cycle", 128), ("fly", "cycle", 4)],
        "ATTACK1": ("glide", "lunge", 20, 0.0, 0.5),
        "DEATH": ("death", "real", 30),
        "DEATHLIE1": ("death", "pose", 1, 1.0),
    },
}


def build(ctx):
    animals.build(ctx, SPEC)
