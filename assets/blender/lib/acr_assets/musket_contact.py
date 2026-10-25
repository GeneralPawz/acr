"""Bake dressed-body musket contact without changing the body's gait keys."""

import copy
import math

import bpy
from mathutils import Matrix, Vector

from . import aim, armature as am, grips, penetration


def carry(arm, base):
    """Shoulder arms on the left, with the supporting fist at the butt."""
    spec = dict(base)
    spec["free_hands"] = ("r", "l")
    spec["world"] = dict(base.get("world", {}), **{"socket.hand_r": (0, 0, 0)})
    am.pose(arm, spec)
    sh = arm.pose.bones["upperarm_l"].head.copy()
    gun = penetration.find_musket(arm)
    r = gun.matrix_world.to_3x3().normalized()
    # Palm up under the butt, wrist behind its heel. A fore-end grip here
    # instead put the cuff through the plate and could never be translated clear.
    frame = Matrix((-r.col[2], r.col[1], r.col[0])).transposed()
    hand = (frame @ grips.frame(arm, "l").inverted()).to_euler("XYZ")
    spec["world"]["hand_l"] = tuple(math.degrees(x) for x in hand)
    # Stock alongside the left chest, muzzle above and behind that shoulder.
    point = sh + Vector((0.05, 0, -0.08))
    restore = penetration.show(penetration._skinned(arm))
    try:
        hit = aim._surface_hit(arm, point + Vector((0, -0.5, 0)), Vector((0, 1, 0)))
    finally:
        restore()
    z0 = min(v.co.z for v in gun.data.vertices)
    g = (hit - r @ Vector((0, 0.025, 0.05)) - Vector((0, 0.003, 0))
         if hit is not None else sh + Vector((0.10, -0.25, -0.13)))
    left = g + r @ Vector((0, 0.015, z0 - 0.005))
    return aim.place(arm, spec, g, right=None, left=tuple(left),
                     elbow_l=(0.16, -0.02, -0.35))


def carry_keys(arm, clips):
    cache = {}
    out = dict(clips)
    stand = clips.get("STAND", [(None, None)])[0][1]
    for clip, keys in clips.items():
        if clip == "STAND" or clip.startswith("MOTION") or any(sp is stand for _f, sp in keys):
            new = []
            for f, sp in keys:
                if clip != "STAND" and not clip.startswith("MOTION") and sp is not stand:
                    new.append((f, sp))
                    continue
                if id(sp) not in cache:
                    cache[id(sp)] = carry(arm, sp)
                new.append((f, cache[id(sp)]))
            out[clip] = new
    return out


def _spec(arm, gun):
    out = {}
    for pb in arm.pose.bones:
        if pb.name.startswith("socket."):
            continue
        r = pb.bone.matrix_local.to_3x3()
        e = (r @ pb.rotation_quaternion.to_matrix() @ r.inverted()).to_euler("XYZ")
        out[pb.name] = tuple(math.degrees(x) for x in e)
    root = arm.pose.bones["root"]
    out["root_loc"] = tuple(root.bone.matrix_local.to_3x3() @ root.location)
    m = arm.matrix_world.inverted() @ gun.matrix_world
    e = m.to_3x3().normalized().to_euler("XYZ")
    left = arm.pose.bones["hand_l"]
    le = (left.matrix.to_3x3() @ left.bone.matrix_local.to_3x3().inverted()).to_euler("XYZ")
    out["world"] = {"socket.hand_r": tuple(math.degrees(x) for x in e),
                    "hand_l": tuple(math.degrees(x) for x in le)}
    out["world_loc"] = {"socket.hand_r": tuple(arm.pose.bones["socket.hand_r"].head)}
    hand = arm.pose.bones["hand_r"]
    fist = hand.matrix @ hand.bone.matrix_local.inverted() @ arm.data.bones["socket.hand_r"].head_local
    if (fist - arm.pose.bones["socket.hand_r"].head).length > 0.03:
        out["free_hands"] = ("r",)
    return out


def bake(arm, clips, rod_phases=None, frames=None):
    """Correct every played frame, including transitions, then re-solve the hands.

    Only arm, hand, head and socket channels are written. Leg/step keys remain untouched.
    """
    gun = penetration.find_musket(arm)
    if gun is None:
        return {}
    info = penetration.prepare(arm)
    restore = penetration.show([x[0] for x in info])
    report = {}
    eye_cache = {}
    try:
        for clip in clips:
            if clip.startswith(("DEATH", "CRAWL", "WRITHE")):
                continue
            action = bpy.data.actions.get(clip)
            if action is None:
                continue
            first, last = (int(x) for x in action.frame_range)
            snapshots = []
            for f in range(first, last + 1):
                if frames is not None and f not in frames:
                    continue
                am.set_clip(arm, clip, f)
                if arm.pose.bones["socket.hand_r"].scale.x < 0.5:
                    continue
                snapshots.append((f, _spec(arm, gun)))
            moved = worst = 0.0
            count = 0
            for f, sp in snapshots:
                arm.animation_data.action = None
                if clip == "STAND" or clip.startswith("MOTION"):
                    sp["free_hands"] = ("r", "l")
                with aim._rig_only(arm):
                    am.pose(arm, sp)
                bones = arm.pose.bones
                origin = bones["socket.hand_r"].head.copy()
                goals = {}
                for s in ("r", "l"):
                    hand = bones[f"hand_{s}"]
                    rest = hand.bone.matrix_local.inverted() @ arm.data.bones[f"socket.hand_{s}"].head_local
                    goals[s] = hand.matrix @ rest
                poles = {s: bones[f"forearm_{s}"].head.copy() for s in ("r", "l")}
                gm = (arm.matrix_world.inverted() @ gun.matrix_world).inverted()
                holding = {s: Vector((*(gm @ goals[s])[:2], 0)).length < 0.12 for s in goals}
                aimed_hold = (clip in ("ATTACK1", "FIRE_AIMED", "PSTAND1") and f < 18)
                if aimed_hold:
                    sp.pop("free_hands", None)
                    goals["r"] = origin.copy()
                    holding["r"] = True
                if "r" in sp.get("free_hands", ()):
                    sp["hand_r"] = (0, 0, 0)
                # Re-solve even a clear interpolated frame: its supporting palm
                # may otherwise retain the A-pose roll and put the cuff on top.
                aim._solve(arm, sp, {s: (goals[s], poles[s]) for s in goals})
                delta = Vector()
                best = None
                for _ in range(12):
                    depth, push = penetration.push_out(arm, gun, info)
                    worst = max(worst, depth)
                    if aimed_hold:
                        gap, seat = aim.seat_move(arm)
                        if gap is not None and (gap < 0 or (gap > 0.005 and depth < 0.001)):
                            push = seat
                    score = depth + (max(abs(gap - 0.002) - 0.003, 0) if aimed_hold and gap is not None else 0)
                    if best is None or score < best[0]:
                        best = (score, copy.deepcopy(sp), delta.copy())
                    if push is None or depth < 0.001:
                        if not aimed_hold or push is None:
                            break
                        if gap is None or 0 <= gap <= 0.005:
                            break
                    if (delta + push).length > 0.20:
                        break
                    delta += push
                    sp["world_loc"] = {"socket.hand_r": tuple(origin + delta)}
                    aim._solve(arm, sp, {s: (goals[s] + (delta if holding[s] else Vector()), poles[s])
                                        for s in goals})
                if best is not None:
                    _score, sp, delta = best
                    with aim._rig_only(arm):
                        am.pose(arm, sp)
                if delta.length > 0:
                    count += 1
                    moved = max(moved, delta.length)
                if aimed_hold:
                    key = tuple(round(v, 4) for v in sp["world_loc"]["socket.hand_r"])
                    if key not in eye_cache:
                        eye_cache[key] = aim.fit_eye(arm, sp)
                    sp.update(eye_cache[key][0])
                # Key the corrected arms and the independent gun at this frame.
                am.set_clip(arm, clip, f)
                # Apply after action evaluation so no previous frame is inherited.
                with aim._rig_only(arm):
                    am.pose(arm, sp)
                names = ("upperarm_r", "forearm_r", "hand_r", "upperarm_l", "forearm_l", "hand_l")
                for name in names + (("neck", "head") if aimed_hold else ()):
                    bones[name].keyframe_insert("rotation_quaternion", frame=f, group=name)
                aim._key(bones["socket.hand_r"], bones["socket.hand_r"].matrix.copy(), f)
                phases = (rod_phases or {}).get(clip)
                aim._key(bones[aim.ROD_SOCKET], aim._rod_at(phases, f, bones, bones["socket.hand_r"].matrix,
                                                        hand_side="l"), f)
            aim._linear(arm)
            report[clip] = {"frames": len(snapshots), "corrected": count,
                            "max_move_m": round(moved, 4), "initial_depth_m": round(worst, 4)}
            print("ACR musket contact bake", clip, report[clip], flush=True)
    finally:
        restore()
        arm.animation_data.action = None
    return report
