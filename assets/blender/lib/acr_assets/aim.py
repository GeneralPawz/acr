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


def _solve(arm_obj, spec, goals, iterations=6):
    """Solves the arms of `goals` = {"r"|"l": (point, pole)} analytically (two-bone IK in
    the plane of shoulder, goal and pole) on the posed `spec` so the right hand's grip point
    (the head of `socket.hand_r`, as the hand carries it) or the left palm reaches `point`
    (armature space), the elbow bending towards `pole`. A goal out of reach is approached
    with the arm straight. Writes the angles into `spec` (in place) and returns it."""
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
                point = _off_hand(arm_obj, spec, loose, goals, point)
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


def _off_hand(arm_obj, spec, loose, goals, point):
    """Grip mode (`grips.py`): a free left hand whose goal lies on (within `OFF_HAND` of)
    the axis of the right hand's item holds that item: its fist's centre goes onto the axis
    (the goal projected) and the hand turns the least so its knuckle line runs along it
    (keyed as the hand's "world" in `spec` and `loose`). Returns the goal."""
    from . import grips

    held = arm_obj.get("acr_grip_held", ())
    if "socket.hand_l" in held:
        return point
    key = next((k for k in ("socket.hand_r", "socket.club_r", "socket.blade_r") if k in held
                and arm_obj.pose.bones[k].scale.x > 0.5), None)
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


def place(arm_obj, base, grip, right="grip", left=0.40, elbow_r=(-0.30, 0.15, -0.30), elbow_l=(0.25, 0.0, -0.35),
          butt=False):
    """A loading pose: the musket (orientation from `base`'s "world" socket rotation) with
    its grip at `grip` (armature space), whichever hand holds it. `right`/`left` say where
    each hand goes: "grip", a distance in metres along the musket from the grip (the left
    palm under the stock, the right hand on the rammer or the muzzle), a point in armature
    space (`(x, y, z)`), or None to leave the arm as `base` has it. The elbows bend towards
    the shoulder + `elbow_r` / `elbow_l` (x, y, z). With `butt`, `grip` is where the butt
    stands instead (the musket grounded). Returns the new spec."""
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
    return spec


def drill(arm_obj, steps):
    """{name: spec} for the loading steps {name: {"base", "grip", ...}} (`place` arguments)."""
    return {name: place(arm_obj, **kw) for name, kw in steps.items()}


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
ROD_STOWED = Matrix.Translation((0, 0.02, 0.50))
ROD_SOCKET = "socket.hand_l"


ROD_LENGTH = 0.9


def _rod(mode, arm, musket):
    """The ramrod's matrix (tail at the origin, +Z to the head) in `mode`: "stowed" in its
    pipes; "slide" drawn up along the barrel, the right hand near its head; "swing" turned
    out to the man's right, held in the middle; "bore" head down the barrel, the hand at
    its tail."""
    if mode == "stowed":
        return musket @ ROD_STOWED
    hand = arm["hand_r"]
    palm = (hand.head + hand.tail) / 2
    m3 = musket.to_3x3()
    axis = m3 @ Vector((0, 0, 1))
    on_bore = musket.translation + axis * (palm - musket.translation).dot(axis)
    if mode == "slide":
        r, held, at = m3, ROD_LENGTH - 0.03, on_bore + m3 @ Vector((0, 0.02, 0))
    elif mode == "swing":
        r, held, at = m3 @ Matrix.Rotation(math.radians(-80), 3, "Y"), ROD_LENGTH * 0.5, palm
    else:
        r, held, at = m3 @ Matrix.Rotation(math.pi, 3, "X"), 0.02, on_bore
    m = r.to_4x4()
    m.translation = at - r @ Vector((0, 0, held))
    return m


def _rod_at(phases, f, arm, musket):
    """The rod on frame `f` between the (frame, mode) `phases` (stowed outside them)."""
    if not phases or f <= phases[0][0] or f >= phases[-1][0]:
        return _rod("stowed", arm, musket)
    i = max(j for j, (k, _) in enumerate(phases) if k <= f)
    (f0, a), (f1, b) = phases[i], phases[i + 1]
    m0, m1 = _rod(a, arm, musket), _rod(b, arm, musket)
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


def bake_musket(arm_obj, clip, keys, rod_phases=None):
    """Keys the musket (`socket.hand_r`) on every frame of `clip` along the path between its
    keys' placements (armature space: location lerped, rotation slerped), so it does not
    swing with a hand whose grip changes between keys, and the ramrod: in its pipes, or in
    the right hand through the (frame, mode) `rod_phases` (`_rod`)."""
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
        _key(rod, _rod_at(rod_phases, f, arm, m), f)
    _linear(arm_obj)
    arm_obj.animation_data.action = None


def stow_rammer(arm_obj, clip):
    """Keys the ramrod in its pipes on every frame of `clip` (the musket as the clip has it)."""
    act = bpy.data.actions[clip]
    first, last = (int(x) for x in act.frame_range)
    am.set_clip(arm_obj, clip, first)
    arm = arm_obj.pose.bones
    for f in range(first, last + 1):
        bpy.context.scene.frame_set(f)
        _key(arm[ROD_SOCKET], _rod("stowed", arm, arm["socket.hand_r"].matrix.copy()), f)
    _linear(arm_obj)
    arm_obj.animation_data.action = None


def _linear(arm_obj):
    for fc in am._fcurves(arm_obj.animation_data.action):
        for kp in fc.keyframe_points:
            kp.interpolation = "LINEAR"
