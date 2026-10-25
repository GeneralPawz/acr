"""High-detail EngM18 (opt-in test unit, m2-hd-outfit): the HD body of `hdman` dressed in
the 1768 uniform by `acr_assets.outfit_hd` (each garment draped alone, one outfit mesh of
the visible layers, skin culled under the clothes, woven cloth maps, brass buttons, gore
and decay attributes, outfit levels 10k / 3k / 800 beside the body's). The coat skirts
follow the breeches through every clip (`outfit_hd.skirt_keys`). Needs the local MakeHuman
sources under ACR_SOURCES/makehuman (see hdbody). Wears the `gear_hd` kit (`kit_hd`): the
fitted tricorn on socket.hat, crossbelts with plate and buckle on the coat, cartridge box,
scabbards and hanger, knapsack and canteen; holds the musket with its bayonet as hdman does."""

import os
import sys

from acr_assets import aim as aiming
from acr_assets import armature as am
from acr_assets import decay, gear, gear_hd, hdbody, kit_hd, musket_contact, outfit_hd, penetration, poses, rig

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from engm18 import MD, PALETTE  # noqa: E402
from hdman import PORTRAIT  # noqa: E402

TYPE = "EngM18"
CHECKS = ["--anim STAND"]


def build(ctx):
    arm = am.build(f"{ctx.name}.rig", rig.skeleton_h(), parent=ctx.root)
    outfit_hd.skirt_rig(arm)
    body, eyes, cage = hdbody.build_master(ctx, arm, os.path.join(hdbody._sources(), PORTRAIT))
    lods = hdbody.build_lods(ctx, body, cage, arm)
    for level, ob in enumerate(lods, 1):
        ob["acr_lod"] = level
    # The skin's material key is `skin` (the layer check and the depth bias read the second
    # dot-separated part of the name).
    for ob in (body, *lods):
        for m in ob.data.materials:
            m.name = f"{ctx.name}.skin.{m.name.rsplit('.', 1)[-1]}"
            m["acr_decay"] = decay.extras("flesh")
    outfit, olods = outfit_hd.build(ctx, arm, body, lods, PALETTE)
    for level, ob in enumerate(olods, 1):
        ob["acr_lod"] = level
    for ob in (body, *lods):
        outfit_hd.gore(ob, "flesh")
    # The kit worn over the outfit (m2-hd-kit-worn): tricorn on socket.hat, crossbelts and
    # pack straps laid on the coat, the strapped items seated on the trunk (`kit_hd`).
    kit, belts, items = kit_hd.wear(ctx, arm, body, outfit)
    # The musket with the fixed bayonet on the hand, gripped by contact, as on hdman.
    grip = hdbody.contact_grip(ctx, arm, body)
    gun, gun_lods = gear_hd.build_item(ctx, "musket")
    bay, bay_lods = gear_hd.build_item(ctx, "bayonet")
    gear_hd.hold(arm, gun, gun_lods, bay, bay_lods)
    aim, hip, ctx.stats["fire_present"] = aiming.fire_poses(arm)
    infos = {}
    reload = aiming.drill(arm, poses.RELOAD_STEPS, infos)
    charge = aiming.hold(arm, poses.CHARGE_MUSKET, pocket=(0.02, -0.12, -0.32), left_along=0.30)
    clips = poses.musketeer_clips(MD, aim=aim, charge=charge, reload=reload, hip=hip)
    clips = musket_contact.carry_keys(arm, clips)
    rods = poses.rod_phases(MD)
    holds, plan = aiming.solve_holds(arm, {k: clips[k] for k in rods if k in clips}, reload, infos)
    clips.update(holds)
    rod = gear.rammer(f"{ctx.name}.rammer")
    am.attach(rod, arm, aiming.ROD_SOCKET, am.world(arm, aiming.ROD_SOCKET))
    swung = []
    for name, keys in clips.items():
        if name not in rods and not name.startswith(("STAND", "MOTION", "DEATH", "WRITHE")):
            keys = [(f, aiming.left_on_shaft(arm, spec)) for f, spec in keys]
        keys = outfit_hd.skirt_keys(keys)
        swung.append(name)
        if name not in hdbody.GRIP_CLIPS_SKIP:
            keys = [(f, {**spec, **grip}) for f, spec in keys]
        am.add_clip(arm, name, keys)
        if name in rods:
            aiming.bake_musket(arm, name, keys, rod_phases=rods[name])
        else:
            aiming.stow_rammer(arm, name)
    ctx.stats["outfit"]["skirt_follow_clips"] = swung
    ctx.stats["hd"]["clips"] = sorted(clips)
    ctx.stats["triangles"] = {"skin": len(body.data.polygons), "outfit": ctx.stats["outfit"]["tris"]["master"]}
    am.set_clip(arm, "STAND")
    arm.animation_data.action = None
    am.pose(arm, {**poses.STAND_MUSKET, **grip})
    for level, (ob, oo) in enumerate(zip(lods, olods), 1):
        ob["acr_lod"] = level
        oo["acr_lod"] = level
    # The musket against the body and outfit over every frame of every clip (m2-aim-hd).
    ctx.stats["musket_contact_bake"] = musket_contact.bake(arm, clips, rods)
    ctx.stats["reload"] = aiming.reload_report(arm, plan)
    ctx.stats["musket_penetration"] = penetration.measure(arm, sorted(clips), gun=gun)
    ctx.stats["fire_contacts"] = aiming.contacts(arm)
    ctx.stats["carry_contacts"] = aiming.carry_contacts(arm, clips)
    am.set_clip(arm, "FIRE_AIMED", 0)
    ctx.stats["hd"]["grip"]["FIRE_AIMED"] = hdbody.grip_gaps(arm, body)
    ctx.stats["hd"]["grip"]["FIRE_AIMED_left"] = hdbody.grip_gaps(
        arm, body, "l", on="socket.hand_r", diameter=hdbody.STOCK["l"])
    arm.animation_data.action = None
    ctx.stats["kit_hd"]["belt_clearance"] = kit_hd.clearance(arm, belts, outfit, sorted(clips))
    ctx.stats["kit_hd"]["collisions"] = kit_hd.collisions(arm, items, outfit, sorted(clips))
    ctx.stats["kit_hd"]["decay_missing"] = sorted({m.name for o in kit for m in o.data.materials if "acr_decay" not in m.keys()})
    ctx.stats["triangles"]["kit"] = sum(len(o.data.polygons) for o in kit if "acr_lod" not in o)
    am.set_clip(arm, "STAND")
    arm.animation_data.action = None
