"""Two-handed musket poses solved with inverse kinematics (needs `bpy`): the butt in the
right shoulder, the right hand at the grip, the left hand under the barrel, the cheek down
on the stock. The arms are solved by a two-bone IK on the posed body and written back as
ordinary pose angles (`armature.pose` specs), so the clips stay plain data.

The musket hangs on `socket.hand_r` with its origin at the grip, 0.25 m in front of the butt
(`gear.musket`); the spec's "world" orientation of the socket sets where it points.
"""

import math

import bpy
from mathutils import Matrix, Vector

from . import armature as am

GRIP_TO_BUTT = 0.25


def _to_spec(pb, m3, parent_m3):
    """Spec angles of bone `pb` for the armature-space orientation `m3` under a parent
    posed at `parent_m3` (the inverse of `armature.pose`)."""
    rest = pb.bone.matrix_local.to_3x3()
    rel = pb.parent.bone.matrix_local.to_3x3().inverted() @ rest
    basis = (parent_m3 @ rel).inverted() @ m3
    r = (rest @ basis @ rest.inverted()).to_euler("XYZ")
    return tuple(round(math.degrees(x), 3) for x in r)


def _aim_bone(m3, direction):
    """`m3` turned by the smallest rotation that points its Y axis (the bone) along
    `direction`."""
    q = (m3 @ Vector((0, 1, 0))).rotation_difference(direction)
    return q.to_matrix() @ m3


def _solve(arm_obj, spec, goals, iterations=6, show=None):
    """Solves the arms of `goals` = {"r"|"l": (point, pole)} analytically (two-bone IK in
    the plane of shoulder, goal and pole) on the posed `spec` so the right hand's grip point
    (the head of `socket.hand_r`, as the hand carries it) or the left palm reaches `point`
    (armature space), the elbow bending towards `pole`. A goal out of reach is approached
    with the arm straight. `show`: the pose's socket scales (an item hidden in it, e.g. the
    pikeman's left pike at the charge, leaves its hand free). Writes the angles into `spec`
    (in place) and returns it."""
    arm = arm_obj.pose.bones
    # A socket placed in armature space ("world_loc") does not follow the hand: measure the
    # hand's own grip point from a pose without it.
    loose = {k: v for k, v in spec.items() if k != "world_loc"}
    am.pose(arm_obj, loose)
    grip = bool(arm_obj.get("acr_grip"))
    for side, (point, pole) in goals.items():
        ua, fa = arm[f"upperarm_{side}"], arm[f"forearm_{side}"]
        a, b = ua.length, fa.length
        point, pole = Vector(point), Vector(pole)
        for _ in range(iterations):
            if grip and side == "l":
                point = _off_hand(arm_obj, spec, loose, goals, point, show)
            if side == "r" or grip:
                eff = arm[f"socket.hand_{side}"].head - fa.tail
            else:
                eff = (arm["hand_l"].tail - fa.tail) * 0.5
            sh = ua.head.copy()
            wrist = point - eff
            d_vec = wrist - sh
            d = min(max(d_vec.length, abs(a - b) + 1e-4), (a + b) * 0.999)
            u = d_vec.normalized()
            v = (pole - sh) - u * (pole - sh).dot(u)
            v = v.normalized() if v.length > 1e-6 else Vector((0, 0, -1))
            x = (a * a + d * d - b * b) / (2 * d)
            elbow = sh + u * x + v * math.sqrt(max(a * a - x * x, 0.0))
            wrist = sh + u * d
            um = _aim_bone(ua.matrix.to_3x3(), (elbow - sh).normalized())
            fm2 = _aim_bone(fa.matrix.to_3x3(), (wrist - elbow).normalized())
            spec[ua.name] = loose[ua.name] = _to_spec(ua, um, ua.parent.matrix.to_3x3())
            spec[fa.name] = loose[fa.name] = _to_spec(fa, fm2, um)
            am.pose(arm_obj, loose)
    am.pose(arm_obj, spec)
    return spec


OFF_HAND = 0.08  # m: a left goal this near the right item's axis is a hand on it (grip mode)


def _off_hand(arm_obj, spec, loose, goals, point, show=None):
    """Grip mode (`grips.py`): a free left hand whose goal lies on (within `OFF_HAND` of)
    the axis of the right hand's item holds that item: its fist's centre goes onto the axis
    (the goal projected) and the hand turns the least so its knuckle line runs along it
    (keyed as the hand's "world" in `spec` and `loose`). Returns the goal."""
    from . import grips

    def shown(k):
        return k in held and ((show or {}).get(k, arm_obj.pose.bones[k].scale.x) > 0.5)

    held = arm_obj.get("acr_grip_held", ())
    if shown("socket.hand_l"):
        return point
    key = next((k for k in ("socket.hand_r", "socket.club_r", "socket.blade_r") if shown(k)), None)
    if key is None:
        return point
    o, z = grips.item_axis(arm_obj, key)
    if key in spec.get("world_loc", {}):
        o = Vector(spec["world_loc"][key])  # the musket placed in armature space (`place`)
    elif "r" in goals:
        o = Vector(goals["r"][0])
    v = point - o
    on = o + z * v.dot(z)
    if (point - on).length > OFF_HAND:
        return point
    # Out of the arm's reach (the fist's centre lies a hand further on than the palm the
    # poses were made for): slide along the shaft towards the shoulder until it reaches.
    arm = arm_obj.pose.bones
    sh = arm["upperarm_l"].head
    reach = 0.97 * (arm["upperarm_l"].length + arm["forearm_l"].length + (arm["socket.hand_l"].head - arm["hand_l"].head).length)
    s0, h = (sh - o).dot(z), ((sh - o) - z * (sh - o).dot(z)).length
    if h < reach:
        span = math.sqrt(reach * reach - h * h)
        on = o + z * min(max(v.dot(z), s0 - span), s0 + span)
    r = grips.align_off_hand(arm_obj, "l", z).to_euler("XYZ")
    for sp in (spec, loose):
        w = dict(sp.get("world", {}))
        w["hand_l"] = tuple(math.degrees(t) for t in r)
        sp["world"] = w
    am.pose(arm_obj, loose)
    return on


NEAR_SHAFT = 0.25  # m: a left fist this near the musket's axis in a derived pose belongs on it


def left_on_shaft(arm_obj, spec, along=0.30, drop=0.45):
    """Grip mode: `spec` with the left arm re-solved onto the right hand's item (`along` m
    from the grip, the elbow `drop` m below), when its left fist lies within `NEAR_SHAFT`
    of the item's axis. For poses derived from a solved one by turning the musket
    (`poses._with(ready, world=...)`: the port, the lunge, the blows), whose left arm kept
    the angles of the pose they came from. Others (and placed muskets) come back unchanged."""
    from . import grips

    if not arm_obj.get("acr_grip") or "world_loc" in spec or "socket.hand_r" not in arm_obj.get("acr_grip_held", ()):
        return spec
    arm = arm_obj.pose.bones
    am.pose(arm_obj, spec)
    o, z = grips.item_axis(arm_obj, "socket.hand_r")
    v = arm["socket.hand_l"].head - o
    if (v - z * v.dot(z)).length > NEAR_SHAFT:
        return spec
    goal = o + z * along
    return _solve(arm_obj, dict(spec), {"l": (goal, goal + Vector((0.0, 0.0, -drop)))})


# Contacts of the shouldered musket, measured (m2-aim-hd): the butt plate against the man's
# surface (`butt_gap`) and the right wrist's bend (`wrist_deg`), for `contacts`.
BUTT_PLATE_M = 0.012  # m: the musket's vertices this near its rear end are the butt plate
_butt_cache = {}


def _butt_points(arm_obj):
    """The butt plate's vertices of the held musket (armature space, current pose), or None."""
    from . import penetration

    gun = penetration.find_musket(arm_obj)
    if gun is None:
        return None
    if gun.name not in _butt_cache:
        z0 = min(v.co.z for v in gun.data.vertices)
        _butt_cache[gun.name] = [v.co.copy() for v in gun.data.vertices if v.co.z < z0 + BUTT_PLATE_M]
    m = arm_obj.matrix_world.inverted() @ gun.matrix_world
    return [m @ c for c in _butt_cache[gun.name]]


def butt_gap(arm_obj):
    """Signed distance (m) of the butt plate's deepest point from the man's surface (body,
    clothes, straps; hands and levels left out): negative inside. None without a musket or
    with no surface within 0.2 m."""
    import numpy as np
    from mathutils.bvhtree import BVHTree

    from . import penetration

    pts = _butt_points(arm_obj)
    if not pts:
        return None
    dg = bpy.context.evaluated_depsgraph_get()
    inv = arm_obj.matrix_world.inverted()
    p = np.array([tuple(x) for x in pts])
    lo, hi = p.min(0) - 0.2, p.max(0) + 0.2
    best = None
    for ob in penetration._skinned(arm_obj):
        key = ("tri", ob.name)
        if key not in _butt_cache:
            tri = penetration._tris(ob.data, np)
            hand = penetration._hand_mask(ob, np, penetration.HAND_BONES + ("forearm_",))  # the stock lies along it
            _butt_cache[key] = (tri[~hand[tri].any(1)], penetration.outward(ob, np))
        tri, wind = _butt_cache[key]
        me = ob.evaluated_get(dg).data
        co = np.empty(len(me.vertices) * 3)
        me.vertices.foreach_get("co", co)
        mb = np.array(inv @ ob.matrix_world)
        flip = wind * (-1.0 if np.linalg.det(mb[:3, :3]) < 0 else 1.0)  # inward winding, mirroring
        co = co.reshape(-1, 3) @ mb[:3, :3].T + mb[:3, 3]
        near = ((co >= lo) & (co <= hi)).all(1)
        sel = tri[near[tri].any(1)]
        if not len(sel):
            continue
        bvh = BVHTree.FromPolygons([tuple(c) for c in co], [tuple(int(i) for i in t) for t in sel])
        for q in pts:
            hit = bvh.find_nearest(q, 0.2)
            if hit[0] is None:
                continue
            d = hit[3] if flip * (q - hit[0]).dot(hit[1]) >= 0 else -hit[3]
            best = d if best is None else min(best, d)
    return best


def wrist_deg(arm_obj, side="r"):
    """The wrist's bend (degrees, flexion and deviation, the twist left out): the angle
    between the hand's direction and where the forearm would carry it from rest."""
    arm = arm_obj.pose.bones
    fa, ha = arm[f"forearm_{side}"], arm[f"hand_{side}"]
    yh = ha.bone.matrix_local.to_3x3() @ Vector((0, 1, 0))
    carried = (fa.matrix.to_3x3() @ fa.bone.matrix_local.to_3x3().inverted()) @ yh
    now = ha.matrix.to_3x3() @ Vector((0, 1, 0))
    return round(math.degrees(carried.angle(now, 0.0)), 1)


def contacts(arm_obj, clips=("FIRE_AIMED", "FIRE_HIP")):
    """`stats.json` "fire_contacts" (m2-aim-hd): per clip at its first frame (the aimed
    hold), the butt plate's signed gap to the man's surface (m, negative inside) and the
    right wrist's bend (degrees)."""
    from . import penetration

    out = {}
    restore = penetration.show(penetration._skinned(arm_obj))
    try:
        for clip in clips:
            if clip in bpy.data.actions:
                am.set_clip(arm_obj, clip, int(bpy.data.actions[clip].frame_range[0]))
                gap = butt_gap(arm_obj)
                out[clip] = {"butt_gap_m": None if gap is None else round(gap, 4), "wrist_deg": wrist_deg(arm_obj, "r")}
    finally:
        restore()
        arm_obj.animation_data.action = None
    print("ACR fire contacts (m2-aim-hd):", out)
    return out


def hold(arm_obj, base, pocket=(0.03, -0.05, -0.04), left_along=0.34, left_drop=0.035,
         elbow_r=(0.35, -0.15, -0.35), elbow_l=0.45):
    """`base` (a pose spec with the musket socket's "world" orientation) with both arms
    solved so the musket's butt sits in the right shoulder pocket (offset `pocket` from the
    right shoulder joint: inwards, forwards, down, metres), the right hand holds the grip
    and the left hand the barrel `left_along` m further on, `left_drop` m below it. The
    right elbow points `elbow_r` (outwards, backwards along the musket, up) from the
    shoulder, the left elbow `elbow_l` m straight below the left hand. Returns the new spec."""
    spec = dict(base)
    arm = arm_obj.pose.bones
    am.pose(arm_obj, spec)
    sh = arm["upperarm_r"].head.copy()
    centre = (arm["upperarm_r"].head + arm["upperarm_l"].head) / 2
    inward = (centre - sh)
    inward.z = 0
    inward = inward.normalized() if inward.length > 1e-6 else Vector((0, 0, 0))
    # Where the musket points: the socket's +Z (the musket runs along it from the grip).
    sock = arm["socket.hand_r"]
    axis = (sock.matrix.to_3x3() @ Vector((0, 0, 1))).normalized()
    flat = Vector((axis.x, axis.y, 0)).normalized()
    butt = sh + inward * pocket[0] + Vector((0, 0, pocket[2])) + flat * pocket[1] * -1
    grip = butt + axis * GRIP_TO_BUTT
    left = grip + axis * left_along + Vector((0, 0, -left_drop))
    goals = {
        "r": (grip, sh - inward * elbow_r[0] - flat * elbow_r[1] + Vector((0, 0, elbow_r[2]))),
        "l": (left, left + Vector((0, 0, -elbow_l)) + flat * 0.10),
    }
    return _solve(arm_obj, spec, goals)


# Aimed fire (#296): the sight line. The musket's top (local +Y, `gear.musket`: the barrel sits on the
# stock, the butt drops below it) carries a rear sight over the breech and a front sight
# behind the muzzle; aiming puts the eye on the line through both ("Kimme und Korn").
SIGHT_UP = 0.026  # m above the musket's axis (top of the barrel + the blade)
REAR_SIGHT = 0.06  # m from the grip towards the muzzle (over the breech)
FRONT_SIGHT_BACK = 0.03  # m behind the muzzle
EYE_HALF = 0.032  # m from the face's centre line to the eye
SIGHT_LIMIT = 0.01  # m: the eye's allowed distance from the sight line
FIRE_CLIPS = ("FIRE_AIMED", "FIRE_HIP")
BUTT_HEEL, BUTT_TOE = 0.03, 0.05  # m of the butt plate above and below the axis (`gear.musket`)


def eye_rest(ch):
    """The right (aiming) eye at rest (armature space): from the refined head's frame, else
    estimated from the head bone."""
    bones = ch.arm.data.bones
    side = 1.0 if bones["upperarm_r"].head_local.x > 0 else -1.0
    fr = getattr(ch, "face_frame", None)
    if fr is not None:
        return Vector((fr.cx + side * EYE_HALF, fr.y_front + 0.008, fr.eye_z))
    h = bones["head"]
    return Vector((h.head_local.x + side * EYE_HALF, h.head_local.y - 0.085, h.head_local.z + 0.09))


def _musket(arm_obj):
    """The musket object on `socket.hand_r` and the local position of its muzzle."""
    for o in arm_obj.children:
        if o.type == "MESH" and o.parent_bone == "socket.hand_r" and "musket" in o.name:
            m = next((c for c in o.children if c.name.startswith("socket.muzzle")), None)
            return o, (Vector(m.location) if m else Vector((0.0, 0.01, 1.30)))
    return None, None


def sight_line(arm_obj, eye):
    """(rear sight, front sight, eye, musket matrix) in armature space for the current pose;
    `eye` = the eye at rest. None without a musket."""
    obj, muzzle = _musket(arm_obj)
    if obj is None:
        return None
    m = arm_obj.matrix_world.inverted() @ obj.matrix_world
    rear = m @ Vector((0.0, SIGHT_UP, REAR_SIGHT))
    front = m @ Vector((0.0, SIGHT_UP, muzzle.z - FRONT_SIGHT_BACK))
    hb = arm_obj.pose.bones["head"]
    e = hb.matrix @ hb.bone.matrix_local.inverted() @ Vector(eye)
    return rear, front, e, m


def _off_line(rear, front, e):
    u = (front - rear).normalized()
    v = e - rear
    return (v - u * v.dot(u)).length


def _pocket(arm_obj, pocket, axis):
    arm = arm_obj.pose.bones
    sh = arm["upperarm_r"].head.copy()
    inward = (arm["upperarm_r"].head + arm["upperarm_l"].head) / 2 - sh
    inward.z = 0
    inward = inward.normalized() if inward.length > 1e-6 else Vector((0, 0, 0))
    flat = Vector((axis.x, axis.y, 0)).normalized()
    return sh + inward * pocket[0] + Vector((0, 0, pocket[2])) - flat * pocket[1], sh, inward, flat


def _sighted(arm_obj, spec, eye, pocket):
    """For the pose `spec` (its arms do not matter): the socket point that puts the sight
    line through the eye with the butt nearest the shoulder pocket, that butt's distance
    from the pocket, and the musket's axis."""
    am.pose(arm_obj, {k: v for k, v in spec.items() if k != "world_loc"})
    rear, front, e, m = sight_line(arm_obj, eye)
    sock = arm_obj.pose.bones["socket.hand_r"].head
    o = m.translation - sock  # the musket's origin from the socket's head
    r3 = m.to_3x3().normalized()
    axis = r3 @ Vector((0, 0, 1))
    rear_rel = rear - m.translation
    u = (front - rear).normalized()
    butt_rel = r3 @ Vector((0, 0, -GRIP_TO_BUTT))
    p = _pocket(arm_obj, pocket, axis)[0]
    k = (e - rear_rel + butt_rel - p).dot(u)
    origin = e - rear_rel - u * k
    # The butt plate runs from the heel (`BUTT_HEEL` above the axis) to the toe (`BUTT_TOE`
    # below it): the pocket may take it anywhere along that edge.
    up = r3 @ Vector((0, 1, 0))
    heel, toe = origin + butt_rel + up * BUTT_HEEL, origin + butt_rel - up * BUTT_TOE
    t = min(max((p - heel).dot(toe - heel) / (toe - heel).length_squared, 0.0), 1.0)
    return origin - o, (heel.lerp(toe, t) - p).length, axis


def present(arm_obj, base, eye, pocket=(0.06, 0.0, -0.02), left_along=0.34, left_drop=0.035,
            elbow_r=(0.40, 0.10, 0.08), elbow_l=0.45, head=None):
    """Aimed fire (#296): `base` (with the musket's "world" orientation) with the head bowed
    onto the comb (cheek weld) so the eye (`eye` at rest) lies on the sight line while the
    butt plate sits nearest the shoulder pocket: the bow (pitch) and the tilt onto the stock,
    shared by neck and head, are searched over `head` = (pitches, tilts) (degrees added). The
    musket is placed through the eye, then both arms are solved as in `hold` (right hand on
    the wrist of the stock, the elbow raised out; left hand under the fore-end). Returns
    (spec, report {sight_m, butt_m, bow})."""
    pitches, tilts = head or (range(-6, 40, 3), range(-30, 7, 3))

    def bowed(dp, dt):
        # The bow shared by the neck (a third) and the head.
        spec = dict(base)
        for bone, part in (("neck", 1 / 3), ("head", 2 / 3)):
            r = base.get(bone, (0, 0, 0))
            spec[bone] = (round(r[0] + dp * part, 2), round(r[1] + dt * part, 2), r[2])
        return spec

    best = None
    for dp in pitches:
        for dt in tilts:
            butt = _sighted(arm_obj, bowed(dp, dt), eye, pocket)[1]
            if best is None or butt < best[0] - 1e-6:
                best = (butt, (dp, dt))
    spec = bowed(*best[1])
    grip, butt, axis = _sighted(arm_obj, spec, eye, pocket)
    _, sh, inward, flat = _pocket(arm_obj, pocket, axis)
    left = grip + axis * left_along + Vector((0, 0, -left_drop))
    goals = {
        "r": (grip, sh - inward * elbow_r[0] - flat * elbow_r[1] + Vector((0, 0, elbow_r[2]))),
        "l": (left, left + Vector((0, 0, -elbow_l)) + flat * 0.10),
    }
    _solve(arm_obj, spec, goals)
    rear, front, e, _ = sight_line(arm_obj, eye)
    report = {"sight_m": round(_off_line(rear, front, e), 4), "butt_m": round(butt, 4),
              "bow": list(best[1])}
    print("ACR present (#296):", report)
    return spec, report


def hip(arm_obj, base, pocket=(0.04, -0.10, -0.30), left_along=0.32):
    """Hip fire (#296, close range): the musket level at the right hip (`hold` with the butt
    low against the hip), both hands on it."""
    return hold(arm_obj, base, pocket=pocket, left_along=left_along, left_drop=0.03,
                elbow_r=(0.30, 0.20, -0.40), elbow_l=0.40)


def fire_poses(ch):
    """(aimed, hip, report): the shouldered present with the eye on the sight line and the
    shot from the hip (#296), both IK-solved on `ch`."""
    from . import poses

    eye = eye_rest(ch)
    aimed, report = present(ch.arm, poses.AIM_MUSKET, eye)
    return aimed, hip(ch.arm, poses.HIP_MUSKET), report


def fire_report(arm_obj, eye, clip, frames):
    """Per frame of `clip` as played: the eye's sight-line error (m) and the fists' misses
    (right fist centre from the grip, left fist centre from the grip axis; both fists, so
    `frames` counts two per frame)."""
    from . import grips

    arm = arm_obj.pose.bones
    am.set_clip(arm_obj, clip)
    rest = {s: grips.centre(arm_obj, s) for s in ("r", "l")}
    sight, miss = [], []
    for f in frames:
        bpy.context.scene.frame_set(f)
        bpy.context.view_layer.update()
        s = sight_line(arm_obj, eye)
        if s is None:
            continue
        rear, front, e, m = s
        sight.append(round(_off_line(rear, front, e), 5))
        # The grip axis: the socket's (the musket sits on it with its stock's offset).
        o, z = grips.item_axis(arm_obj, "socket.hand_r")
        fist = {sd: arm[f"hand_{sd}"].matrix @ arm[f"hand_{sd}"].bone.matrix_local.inverted() @ rest[sd]
                for sd in ("r", "l")}
        miss.append(round((fist["r"] - o).length, 5))
        w = fist["l"] - o
        miss.append(round((w - z * w.dot(z)).length, 5))
    arm_obj.animation_data.action = None

    def summary(xs):
        return {"frames": len(xs), "mean_m": round(sum(xs) / max(len(xs), 1), 5), "max_m": max(xs, default=0.0)}

    return {"sight": {**summary(sight), "per_frame": sight}, "grip": summary(miss)}


def fire_stats(ch, report=None):
    """`stats.json` "fire" (#296): per fire clip, over the frames of its hold (before the
    shot), the sight line and the grip misses as played."""
    from . import poses

    eye = eye_rest(ch)
    out = {"present": report}
    for clip in FIRE_CLIPS:
        if clip in bpy.data.actions:
            out[clip] = fire_report(ch.arm, eye, clip, range(0, poses.SHOT_FRAME))
    print("ACR fire (#296):", {k: (v["sight"]["max_m"], v["grip"]) for k, v in out.items() if k in FIRE_CLIPS})
    return out


def place(arm_obj, base, grip, right="grip", left=0.40, elbow_r=(-0.30, 0.15, -0.30), elbow_l=(0.25, 0.0, -0.35),
          butt=False, info=None):
    """A loading pose: the musket (orientation from `base`'s "world" socket rotation) with
    its grip at `grip` (armature space), whichever hand holds it. `right`/`left` say where
    each hand goes: "grip", a distance in metres along the musket from the grip (the left
    palm under the stock, the right hand on the rammer or the muzzle), a point in armature
    space (`(x, y, z)`), or None to leave the arm as `base` has it. The elbows bend towards
    the shoulder + `elbow_r` / `elbow_l` (x, y, z). With `butt`, `grip` is where the butt
    stands instead (the musket grounded). Returns the new spec; `info` (a dict) receives
    what `between` needs: the musket's matrix, the hands' goals in its frame, the elbows."""
    spec = dict(base)
    am.pose(arm_obj, spec)
    arm = arm_obj.pose.bones
    axis = (arm["socket.hand_r"].matrix.to_3x3() @ Vector((0, 0, 1))).normalized()
    g = Vector(grip) + (axis * GRIP_TO_BUTT if butt else Vector())
    spec["world_loc"] = {"socket.hand_r": tuple(round(c, 4) for c in g)}

    def where(t):
        if t == "grip":
            return g
        if isinstance(t, (int, float)):
            return g + axis * t
        return Vector(t)

    goals = {}
    for side, t, el in (("r", right, elbow_r), ("l", left, elbow_l)):
        if t is not None:
            goals[side] = (where(t), arm[f"upperarm_{side}"].head + Vector(el))
    _solve(arm_obj, spec, goals)
    # How far each hand ended from its goal (metres; reported by the build).
    am.pose(arm_obj, {k: v for k, v in spec.items() if k != "world_loc"})
    at = {"r": arm["socket.hand_r"].head, "l": arm["socket.hand_l"].head if arm_obj.get("acr_grip")
          else (arm["hand_l"].tail + arm["forearm_l"].tail) * 0.5}
    spec_err = {s: round((at[s] - p).length, 3) for s, (p, _) in goals.items()}
    am.pose(arm_obj, spec)
    print("ACR place errors (m):", spec_err)
    if info is not None:
        m = arm["socket.hand_r"].matrix.copy()
        inv = m.inverted()
        info.update(spec=spec, musket=m, goals={s: inv @ p for s, (p, _) in goals.items()},
                    elbows={s: Vector(el) for s, t, el in (("r", right, elbow_r), ("l", left, elbow_l)) if t is not None})
    return spec


def drill(arm_obj, steps, infos=None):
    """{name: spec} for the loading steps {name: {"base", "grip", ...}} (`place` arguments);
    `infos` (a dict) receives each step's `place` info by name (for `solve_holds`)."""
    out = {}
    for name, kw in steps.items():
        info = {}
        out[name] = place(arm_obj, **kw, info=info)
        if infos is not None:
            infos[name] = info
    return out


def between(arm_obj, a, b, t):
    """The pose a fraction `t` of the way from loading step `a` to `b` (`place` infos): the
    body's angles blended, the musket's "world" rotation slerped and its grip lerped and both arms IK-solved onto the hands' goals blended in the musket's
    frame. Returns (spec, {side: goal in the musket's frame})."""
    sa, sb = a["spec"], b["spec"]
    spec = {}
    for k in sorted(set(sa) | set(sb)):
        if k in ("world", "world_loc"):
            continue
        va, vb = sa.get(k, (0.0, 0.0, 0.0)), sb.get(k, (0.0, 0.0, 0.0))
        spec[k] = tuple(x + (y - x) * t for x, y in zip(va, vb))
    # The socket's "world" rotation slerped (in grip mode the hand turns to it, so the
    # musket's matrix is read back from the posed result), its grip lerped.
    qa, qb = (am.euler(x["spec"]["world"]["socket.hand_r"]).to_quaternion() for x in (a, b))
    r = qa.slerp(qb, t).to_euler("XYZ")
    spec["world"] = {"socket.hand_r": tuple(math.degrees(x) for x in r)}
    spec["world_loc"] = {"socket.hand_r": tuple(a["musket"].translation.lerp(b["musket"].translation, t))}
    arm = arm_obj.pose.bones
    am.pose(arm_obj, spec)
    m = arm["socket.hand_r"].matrix.copy()
    local = {s: a["goals"][s].lerp(b["goals"][s], t) for s in a["goals"] if s in b["goals"]}
    goals = {s: (m @ g, arm[f"upperarm_{s}"].head + a["elbows"][s].lerp(b["elbows"][s], t)) for s, g in local.items()}
    _solve(arm_obj, spec, goals)
    return spec, local


def solve_holds(arm_obj, clips, steps, infos):
    """Keys every frame between two loading steps of each clip's keys (acr-blender-pipeline
    pitfall 5: IK on the keys alone lets the hands drift off between them): {clip: keys}
    with the in-betweens IK-solved (`between`), and {clip: {frame: hand goals in the
    musket's frame}} over those holds for `reload_report`. `steps` = {name: spec} as
    `drill` returned them, `infos` its place infos."""
    name_of = {id(v): k for k, v in steps.items()}
    cache, out, plan = {}, {}, {}
    for clip, keys in clips.items():
        new, goals = [], {}
        for (f0, s0), (f1, s1) in zip(keys, keys[1:] + [(None, None)]):
            new.append((f0, s0))
            a, b = name_of.get(id(s0)), name_of.get(id(s1))
            if a is None:
                continue
            goals[f0] = infos[a]["goals"]
            if b is None:
                continue
            for f in range(f0 + 1, f1):
                k = (a, b, f - f0, f1 - f0)
                if k not in cache:
                    cache[k] = between(arm_obj, infos[a], infos[b], (f - f0) / (f1 - f0))
                spec, local = cache[k]
                new.append((f, spec))
                goals[f] = local
        out[clip] = new
        if goals:
            plan[clip] = goals
    return out, plan


def _carried(arm_obj, side):
    """(hand bone, point in its rest frame) of the point `_solve` puts on a hand's goal, or
    (None, None) for the bare left palm."""
    if side == "r" or arm_obj.get("acr_grip"):
        hand = arm_obj.data.bones[f"hand_{side}"]
        return f"hand_{side}", hand.matrix_local.inverted() @ arm_obj.data.bones[f"socket.hand_{side}"].head_local
    return None, None


def reload_report(arm_obj, plan):
    """`stats.json` "reload" (#255): per clip, over every frame of its loading holds as
    played, how far the right hand's grip point is from its goal and the left palm from its
    goal on the musket (m; both hands, so two per frame), and the worst [clip, frame]."""
    arm = arm_obj.pose.bones
    out, every = {}, []
    for clip, goals in sorted(plan.items()):
        am.set_clip(arm_obj, clip)
        miss = []
        for f in sorted(goals):
            bpy.context.scene.frame_set(f)
            bpy.context.view_layer.update()
            m = arm["socket.hand_r"].matrix
            for side, g in goals[f].items():
                bone, p = _carried(arm_obj, side)
                if bone:
                    at = arm[bone].matrix @ p
                else:
                    at = (arm["hand_l"].tail + arm["forearm_l"].tail) * 0.5
                miss.append(((at - m @ g).length, f))
        arm_obj.animation_data.action = None
        if miss:
            worst = max(miss)
            out[clip] = {"frames": len(goals), "mean_m": round(sum(d for d, _ in miss) / len(miss), 4),
                         "max_m": round(worst[0], 4), "worst_frame": worst[1]}
            every += [(d, clip, f) for d, f in miss]
    if every:
        worst = max(every)
        out["all"] = {"frames": len(every) // 2, "mean_m": round(sum(d for d, *_ in every) / len(every), 4),
                      "max_m": round(worst[0], 4), "worst": [worst[1], worst[2]]}
    print("ACR reload (#255):", out.get("all"))
    return out


def errors(arm_obj, spec, left_along=0.34, left_drop=0.035):
    """How far (metres) the right hand's socket is from where the butt meets the shoulder
    (grip − 0.25 m along the musket), and the left hand from its place on the barrel."""
    am.pose(arm_obj, spec)
    arm = arm_obj.pose.bones
    sock = arm["socket.hand_r"]
    axis = (sock.matrix.to_3x3() @ Vector((0, 0, 1))).normalized()
    butt = sock.head - axis * GRIP_TO_BUTT
    left_target = sock.head + axis * left_along + Vector((0, 0, -left_drop))
    hand_l = (arm["hand_l"].head + arm["hand_l"].tail) / 2
    return (butt - arm["upperarm_r"].head).length, (hand_l - left_target).length


# The ramrod in its pipes under the barrel: tail 0.50 m along the musket from the grip,
# head towards the muzzle (`gear.rammer` hangs on `socket.hand_l`, placed every frame).
ROD_STOWED = Matrix.Translation((0, -0.02, 0.50))
ROD_SOCKET = "socket.hand_l"


ROD_LENGTH = 0.9


def rod_for(gun, musket=1.62):
    """`_rod`'s options for a gun of `gun` metres: a short gun's rod is no longer than its
    barrel (88 %) and lies in the pipes with its head at the muzzle (the trapper's rifle,
    #347)."""
    muzzle = gun - GRIP_TO_BUTT
    length = min(ROD_LENGTH, 0.88 * muzzle)
    return {"length": length, "stowed": Matrix.Translation((0, -0.02, muzzle - length + 0.03))}


def _rod(mode, arm, musket, prefix="", length=ROD_LENGTH, stowed=ROD_STOWED):
    """The ramrod's matrix (tail at the origin, +Z to the head) in `mode`: "stowed" in its
    pipes; "slide" drawn up along the barrel, the right hand near its head; "swing" turned
    out to the man's right, held in the middle; "bore" head down the barrel, the hand at
    its tail. `prefix`, `length`, `stowed`: a rider's carbine (`cavalry.reload_clip`)."""
    if mode == "stowed":
        return musket @ stowed
    hand = arm[prefix + "hand_r"]
    palm = (hand.head + hand.tail) / 2
    m3 = musket.to_3x3()
    axis = m3 @ Vector((0, 0, 1))
    on_bore = musket.translation + axis * (palm - musket.translation).dot(axis)
    if mode == "slide":
        r, held, at = m3, length - 0.03, on_bore + m3 @ Vector((0, -0.02, 0))
    elif mode == "swing":
        r, held, at = m3 @ Matrix.Rotation(math.radians(-80), 3, "Y"), length * 0.5, palm
    else:
        r, held, at = m3 @ Matrix.Rotation(math.pi, 3, "X"), 0.02, on_bore
    m = r.to_4x4()
    m.translation = at - r @ Vector((0, 0, held))
    return m


def _rod_at(phases, f, arm, musket, **kw):
    """The rod on frame `f` between the (frame, mode) `phases` (before and after them: the
    first's and the last's mode; stowed without phases). `kw`: `_rod`'s options."""
    if not phases:
        return _rod("stowed", arm, musket, **kw)
    if f <= phases[0][0]:
        return _rod(phases[0][1], arm, musket, **kw)
    if f >= phases[-1][0]:
        return _rod(phases[-1][1], arm, musket, **kw)
    i = max(j for j, (k, _) in enumerate(phases) if k <= f)
    (f0, a), (f1, b) = phases[i], phases[i + 1]
    m0, m1 = _rod(a, arm, musket, **kw), _rod(b, arm, musket, **kw)
    if a == b:
        return m0
    t = (f - f0) / (f1 - f0)
    m = m0.to_quaternion().slerp(m1.to_quaternion(), t).to_matrix().to_4x4()
    m.translation = m0.translation.lerp(m1.translation, t)
    return m


def _key(pb, m, frame):
    pb.matrix = m
    bpy.context.view_layer.update()
    pb.keyframe_insert("rotation_quaternion", frame=frame, group=pb.name)
    pb.keyframe_insert("location", frame=frame, group=pb.name)


def bake_musket(arm_obj, clip, keys, rod_phases=None, rod_kw=None):
    """Keys the musket (`socket.hand_r`) on every frame of `clip` along the path between its
    keys' placements (armature space: location lerped, rotation slerped), so it does not
    swing with a hand whose grip changes between keys, and the ramrod: in its pipes, or in
    the right hand through the (frame, mode) `rod_phases` (`_rod`, options `rod_kw`)."""
    arm = arm_obj.pose.bones
    placed = []
    for f, spec in keys:
        am.pose(arm_obj, spec)
        placed.append((f, arm["socket.hand_r"].matrix.copy()))
    am.set_clip(arm_obj, clip)
    sock, rod = arm["socket.hand_r"], arm[ROD_SOCKET]
    frames = [f for f, _ in placed]
    for f in range(frames[0], frames[-1] + 1):
        i = max(j for j, k in enumerate(frames) if k <= f)
        (f0, m0), (f1, m1) = placed[i], placed[min(i + 1, len(placed) - 1)]
        t = (f - f0) / (f1 - f0) if f1 > f0 else 0.0
        m = m0.to_quaternion().slerp(m1.to_quaternion(), t).to_matrix().to_4x4()
        m.translation = m0.translation.lerp(m1.translation, t)
        bpy.context.scene.frame_set(f)
        _key(sock, m, f)
        _key(rod, _rod_at(rod_phases, f, arm, m, **(rod_kw or {})), f)
    _linear(arm_obj)
    arm_obj.animation_data.action = None


def stow_rammer(arm_obj, clip, rod_kw=None):
    """Keys the ramrod in its pipes on every frame of `clip` (the musket as the clip has it)."""
    act = bpy.data.actions[clip]
    first, last = (int(x) for x in act.frame_range)
    am.set_clip(arm_obj, clip, first)
    arm = arm_obj.pose.bones
    for f in range(first, last + 1):
        bpy.context.scene.frame_set(f)
        _key(arm[ROD_SOCKET], _rod("stowed", arm, arm["socket.hand_r"].matrix.copy(), **(rod_kw or {})), f)
    _linear(arm_obj)
    arm_obj.animation_data.action = None


def _linear(arm_obj):
    for fc in am._fcurves(arm_obj.animation_data.action):
        for kp in fc.keyframe_points:
            kp.interpolation = "LINEAR"
