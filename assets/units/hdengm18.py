"""High-detail EngM18 (opt-in test unit, m2-hd-outfit): the HD body of `hdman` dressed in
the 1768 uniform by `acr_assets.outfit_hd` (each garment draped alone, one outfit mesh of
the visible layers, skin culled under the clothes, woven cloth maps, brass buttons, gore
and decay attributes, outfit levels 10k / 3k / 800 beside the body's). The coat skirts
swing with the thighs in the walk (`outfit_hd.skirt_swing`). Needs the local MakeHuman
sources under ACR_SOURCES/makehuman (see hdbody). No hat or gear yet."""

import os
import sys

from acr_assets import aim as aiming
from acr_assets import armature as am
from acr_assets import decay, hdbody, outfit_hd, poses, rig

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from engm18 import MD, PALETTE  # noqa: E402
from hdman import PORTRAIT  # noqa: E402

TYPE = "EngM18"
CHECKS = ["--anim STAND"]


def build(ctx):
    arm = am.build(f"{ctx.name}.rig", rig.skeleton_h(), parent=ctx.root)
    body, eyes, cage = hdbody.build_master(ctx, arm, os.path.join(hdbody._sources(), PORTRAIT))
    lods = hdbody.build_lods(ctx, body, cage, arm)
    # The skin's material key is `skin` (the layer check and the depth bias read the second
    # dot-separated part of the name).
    for ob in (body, *lods):
        for m in ob.data.materials:
            m.name = f"{ctx.name}.skin.{m.name.rsplit('.', 1)[-1]}"
            m["acr_decay"] = decay.extras("flesh")
    outfit, olods = outfit_hd.build(ctx, arm, body, lods, PALETTE)
    for ob in (body, *lods):
        outfit_hd.gore(ob, "flesh")
    grip = hdbody.grip_spec(arm)
    aim = aiming.hold(arm, poses.AIM_MUSKET)
    hip = aiming.hip(arm, poses.HIP_MUSKET)
    reload = aiming.drill(arm, poses.RELOAD_STEPS, {})
    charge = aiming.hold(arm, poses.CHARGE_MUSKET, pocket=(0.02, -0.12, -0.32), left_along=0.30)
    clips = poses.musketeer_clips(MD, aim=aim, charge=charge, reload=reload, hip=hip)
    swung = []
    for name, keys in clips.items():
        if name.startswith("MOTION"):
            keys = outfit_hd.skirt_swing(keys)
            swung.append(name)
        if name not in hdbody.GRIP_CLIPS_SKIP:
            keys = [(f, {**spec, **grip}) for f, spec in keys]
        am.add_clip(arm, name, keys)
    ctx.stats["outfit"]["skirt_swing_clips"] = swung
    ctx.stats["hd"]["clips"] = sorted(clips)
    ctx.stats["triangles"] = {"skin": len(body.data.polygons), "outfit": ctx.stats["outfit"]["tris"]["master"]}
    am.set_clip(arm, "STAND")
    arm.animation_data.action = None
    am.pose(arm, {**poses.STAND_MUSKET, **grip})
    for level, (ob, oo) in enumerate(zip(lods, olods), 1):
        ob["acr_lod"] = level
        oo["acr_lod"] = level
