"""High-detail base human (opt-in test unit, m2-hd-human): the `acr_assets.hdbody` master
(about 78k triangles, 4k skin) on skeleton H with the musketeer clips of `EngM18` and the
musket grip on the fingers; its LOD chain (10k / 3k / 800, seam-locked, baked normal and
albedo each) rides in the same glb as nodes with `extras.acr_lod` = 1..3 and
`acr_lod_of` = the master's node, on the master's skin (docs/rendering-3d.md, "Authored
levels"). Needs the local MakeHuman sources under ACR_SOURCES/makehuman (see hdbody)."""

import os
import sys

import bpy

from acr_assets import armature as am
from acr_assets import hdbody, poses, rig

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from engm18 import MD  # noqa: E402

TYPE = "EngM18"
PORTRAIT = "generated/faces/aligned/eu1_s1.png"


def build(ctx):
    arm = am.build(f"{ctx.name}.rig", rig.skeleton_h(), parent=ctx.root)
    body, eyes, cage = hdbody.build_master(ctx, arm, os.path.join(hdbody._sources(), PORTRAIT))
    lods = hdbody.build_lods(ctx, body, cage, arm)
    grip = hdbody.grip_spec(arm)
    clips = poses.musketeer_clips(MD)
    for name, keys in clips.items():
        if name not in hdbody.GRIP_CLIPS_SKIP:
            keys = [(f, {**spec, **grip}) for f, spec in keys]
        am.add_clip(arm, name, keys)
    ctx.stats["hd"]["clips"] = sorted(clips)
    ctx.stats["triangles"] = {"master": ctx.stats["hd"]["master_tris"]}
    am.set_clip(arm, "STAND")
    arm.animation_data.action = None
    am.pose(arm, {**poses.STAND_MUSKET, **grip})
    for level, ob in enumerate(lods, 1):  # authored levels in the glb (docs/rendering-3d.md)
        ob["acr_lod"] = level
