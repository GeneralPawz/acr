"""High-detail base human (opt-in test unit, m2-hd-human): the `acr_assets.hdbody` master
(about 78k triangles, 4k skin) on skeleton H with the musketeer clips of `EngM18` and the
musket grip on the fingers; its LOD chain (10k / 3k / 800, each with its own seam-cut
atlas and baked normal and albedo) rides in the same glb as nodes with `extras.acr_lod` =
1..3 and `acr_lod_of` = the master's node, on the master's skin (docs/rendering-3d.md,
"Authored levels"). Holds the `gear_hd` musket and bayonet masters (with their own levels)
on socket.hand_r. Needs the local MakeHuman sources under ACR_SOURCES/makehuman (see
hdbody)."""

import json
import os
import sys

import bpy

from acr_assets import armature as am
from acr_assets import aim as aiming
from acr_assets import gear_hd, hdbody, penetration, poses, rig

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from engm18 import MD  # noqa: E402

TYPE = "EngM18"
PORTRAIT = "generated/faces/aligned/eu1_s1.png"


def build(ctx):
    arm = am.build(f"{ctx.name}.rig", rig.skeleton_h(), parent=ctx.root)
    body, eyes, cage = hdbody.build_master(ctx, arm, os.path.join(hdbody._sources(), PORTRAIT))
    lods = hdbody.build_lods(ctx, body, cage, arm)
    # The contact grip (docs/assets-humans.md "Contact grip"): the sockets move onto the
    # stock's axis in the fist and the fingers wrap it; the fixed-angle curl it replaced is
    # measured on the same stock as the baseline.
    old_grip = hdbody.grip_spec(arm)
    grip = hdbody.contact_grip(ctx, arm, body)
    fist = hdbody.fist_spec(ctx, arm)
    # The hand poses for tools/hand_closeups.py (a text block: not exported).
    bpy.data.texts.new("acr_hand_poses").write(json.dumps({"grip": grip, "fist": fist}))
    # The Short Land Pattern master with its levels and the fixed bayonet on socket.hand_r,
    # in gear.musket's frame (`gear_hd.hold`, m2-hd-musket).
    gun, gun_lods = gear_hd.build_item(ctx, "musket")
    bay, bay_lods = gear_hd.build_item(ctx, "bayonet")
    gear_hd.hold(arm, gun, gun_lods, bay, bay_lods)
    # The two-handed holds IK-solved on this body, so FIRE_AIMED, FIRE_HIP and the reload
    # exist (and get the grip) as on the line musketeer.
    aim = aiming.hold(arm, poses.AIM_MUSKET)
    hip = aiming.hip(arm, poses.HIP_MUSKET)
    reload = aiming.drill(arm, poses.RELOAD_STEPS, {})
    charge = aiming.hold(arm, poses.CHARGE_MUSKET, pocket=(0.02, -0.12, -0.32), left_along=0.30)
    clips = poses.musketeer_clips(MD, aim=aim, charge=charge, reload=reload, hip=hip)
    ctx.stats["hd"]["grip_clips"] = sorted(k for k in clips if k not in hdbody.GRIP_CLIPS_SKIP)
    for name, keys in clips.items():
        if name not in hdbody.GRIP_CLIPS_SKIP:
            keys = [(f, {**spec, **grip}) for f, spec in keys]
        am.add_clip(arm, name, keys)
    ctx.stats["hd"]["clips"] = sorted(clips)
    ctx.stats["triangles"] = {"master": ctx.stats["hd"]["master_tris"]}
    am.set_clip(arm, "STAND")
    arm.animation_data.action = None
    gst = ctx.stats["hd"]["grip"]
    am.pose(arm, {**poses.STAND_MUSKET, **old_grip})
    gst["STAND_before"] = hdbody.grip_gaps(arm, body)
    # Per clip (ATTACK1 = the reload): the right hand on the wrist of the stock, the left on
    # the fore-end (the musket's axis, `socket.hand_r`) where it holds it.
    for clip in ("FIRE_AIMED", "FIRE_HIP", "ATTACK1"):
        if clip in clips:
            frames = [f for f, _s in clips[clip]]
            am.set_clip(arm, clip, frames[len(frames) // 2])
            gst[clip] = hdbody.grip_gaps(arm, body)
            gst[clip + "_left"] = hdbody.grip_gaps(arm, body, "l", on="socket.hand_r",
                                                   diameter=hdbody.STOCK["l"])
            arm.animation_data.action = None
    am.pose(arm, {**poses.STAND_MUSKET, **grip})
    gst["STAND"] = hdbody.grip_gaps(arm, body)
    am.pose(arm, {**poses.STAND_MUSKET, **fist})
    ctx.stats["hd"]["fist"]["gaps"] = hdbody.grip_gaps(arm, body)
    for level, ob in enumerate(lods, 1):  # authored levels in the glb (docs/rendering-3d.md)
        ob["acr_lod"] = level
    # The musket against the body over every frame of every clip (m2-aim-hd).
    ctx.stats["musket_penetration"] = penetration.measure(arm, sorted(clips), gun=gun)
    ctx.stats["fire_contacts"] = aiming.contacts(arm)
    am.pose(arm, {**poses.STAND_MUSKET, **fist})
