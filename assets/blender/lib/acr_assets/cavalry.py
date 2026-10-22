"""Mounted units on the WildMesh horse (#287, #275): the dragoon (EngD18), the cavalry
standard bearers (*KF18), the Spanish reiters (SpnReiG1/2) and the native riders.

A unit script gives a spec and calls `build(ctx, spec)`:

    md        {key: ".MD line"} (`rig.md_clip_frames` reads the frame counts)
    palette   material keys -> colour (the sprite reference refines them)
    dress     "coat" (18th-century coat, waistcoat, breeches, high boots, cap), "native"
              (bare chest, breechcloth, leggings)
    tack      "military" (scripted saddle with pistol holsters on a saddlecloth),
              "blanket" (a blanket, no saddle), "bare"
    held      gear: "sabre", "flag" (a standard in the right hand), "lance"
    hat       "cap" (tall mitre cap), "tricorne", None

The horse is `acr_assets.mount`; the rider stands on skeleton H, his root on the horse's
`saddle` bone. Seated pose: legs to the stirrups, both arms solved (two-bone IK) to closed
hands on the reins over the withers; the right arm leaves the reins for aiming, cutting or
the flag. Death: the horse falls onto its side with bent legs; the rider is thrown clear,
a metre beyond its back, lying along it.
"""

import math

import bmesh
import bpy
from mathutils import Matrix, Vector

from . import armature as am
from . import body as bd
from . import crawl, faces, gear, heads, horse, mount, rig
from .humans import Character, prepare_export

P = "rider."
TORSO = {"hips", "spine", "spine1", "spine2", "clavicle_l", "clavicle_r"}
ARMS = {"upperarm_l", "upperarm_r", "forearm_l", "forearm_r"}
LEGS = {"thigh_l", "thigh_r"}
SHINS = {"shin_l", "shin_r"}
FEET = {"foot_l", "foot_r", "toe_l", "toe_r"}
BUILD = {"width": 1.0, "shoulders": 1.08, "limbs": 1.12}
# The dragoon's colours; other units override some (the sprites refine all).
PALETTE = {
    "skin": (0.80, 0.58, 0.45), "bone": (0.85, 0.81, 0.69), "coat": (0.70, 0.08, 0.05),
    "waistcoat": (0.85, 0.80, 0.62), "breeches": (0.85, 0.80, 0.62), "boots_high": (0.08, 0.07, 0.06),
    "boots": (0.30, 0.20, 0.12), "belts": (0.85, 0.80, 0.62), "cap": (0.70, 0.08, 0.05), "metal": (0.70, 0.70, 0.72),
    "wood": (0.42, 0.24, 0.12), "saddle": (0.25, 0.15, 0.08), "tack": (0.08, 0.07, 0.06), "saddlecloth": (0.72, 0.10, 0.06), "sash": (0.75, 0.10, 0.10), "hat": (0.10, 0.09, 0.08),
}

# The horse's clips: {key: (source action, mode, frames[, a, b])}; frames come from the MD.
GAITS = {
    "STAND": ("Idle", "pose", None, 0.0),
    "REST": ("Idle", "loop", None),
    "MOTION_L": ("G1_Trot", "loop", None, 0.0, 0.5),
    "MOTION_R": ("G1_Trot", "loop", None, 0.5, 1.0),
    "WALK": ("G0_Walk", "loop", 16),
    "GALLOP": ("G3_Gallop", "loop", 12),
    "ATTACK": ("Idle_Fidget_08", "once", None, 0.0, 0.7),
    "ATTACK1": ("Idle_Fidget_07", "once", None),
    "DEATH": ("Idle", "fall", None, 0.0, -90),
}

LEGS_SEAT = {"thigh_l": (-70, -12, 0), "shin_l": (96, 0, 0), "foot_l": (-22, 0, 0),
             "thigh_r": (-70, 12, 0), "shin_r": (96, 0, 0), "foot_r": (-22, 0, 0),
             "spine": (-2, 0, 0), "head": (4, 0, 0)}
# Right arm poses (armature axes) of the original dragoon (#183).
AIM_R = {"upperarm_r": (-70, -30, -20), "forearm_r": (-40, 0, 30), "head": (8, 0, -10)}
AIM_L = {"upperarm_l": (-75, 45, 25), "forearm_l": (-15, 0, -15)}
CUT_UP_R = {"upperarm_r": (-150, -20, 0), "forearm_r": (-20, 0, 0), "spine": (4, 0, -10)}
CUT_DOWN_R = {"upperarm_r": (-60, -10, 0), "forearm_r": (-30, 0, 0), "spine": (-14, 0, 15)}
FLAG_R = {"upperarm_r": (-20, -10, 0), "forearm_r": (-75, 0, 10)}


def rig_lie():
    """Rider lying prone (armature space); `world_loc` set by `lie_spot`."""
    return {
        "world": {"root": (90, 0, 0)},
        "upperarm_l": (12, -45, 0), "forearm_l": (0, -16, 0), "upperarm_r": (-2, 47, 0), "forearm_r": (0, 50, 0),
        "thigh_l": (-14, -6, 0), "thigh_r": (-10, 8, 0), "shin_l": (14, 0, 0), "shin_r": (15, 0, 0),
        "head": (-10, 18, 62), "foot_l": (55, 0, 0), "foot_r": (50, 0, 0),
    }


# ----------------------------------------------------------------------------- arms

def _ik(s, t, l1, l2, pole):
    """Elbow of a two-bone chain from `s` to `t` bent towards `pole`."""
    v = t - s
    d = min(v.length, (l1 + l2) * 0.995)
    u = v.normalized()
    a = (l1 * l1 - l2 * l2 + d * d) / (2 * d)
    h = math.sqrt(max(l1 * l1 - a * a, 0.0))
    p = (pole - u * pole.dot(u)).normalized()
    return s + u * a + p * h


def _turn(d0, d):
    e = d0.rotation_difference(d).to_euler("XYZ")
    return tuple(math.degrees(x) for x in e)


def reins_world(arm, targets, sides=("l", "r")):
    """{"world": {...}} orientations of upper arm, forearm and hand per side so the wrist
    reaches `targets[side]` (armature space, rest) and the hand, closed, points forward
    and down along the reins."""
    out = {}
    B = arm.data.bones
    for sd in sides:
        sgn = 1.0 if sd == "l" else -1.0
        up, fo, ha = B[P + "upperarm_" + sd], B[P + "forearm_" + sd], B[P + "hand_" + sd]
        s = up.head_local
        l1, l2 = (up.tail_local - up.head_local).length, (ha.head_local - fo.head_local).length
        t = targets[sd]
        e = _ik(s, t, l1, l2, Vector((0.6 * sgn, 0.7, -0.4)))
        out["upperarm_" + sd] = _turn((up.tail_local - up.head_local).normalized(), (e - s).normalized())
        df = (t - e).normalized()
        out["forearm_" + sd] = _turn((fo.tail_local - fo.head_local).normalized(), df)
        dh = (df + Vector((0.0, 0.0, -0.8))).normalized()
        out["hand_" + sd] = _turn((ha.tail_local - ha.head_local).normalized(), dh)
    return out


def _with_world(spec, world):
    s = dict(spec)
    w = dict(world)
    w.update(spec.get("world", {}))
    s["world"] = w
    for k in world:
        s.pop(k, None)
    return s


# ----------------------------------------------------------------------------- tack

def _body_section(hz, y, z):
    """Half width of the horse's body (M_Body) at length `y` around height `z`."""
    body = hz.meshes[0]
    mats = body.data.materials
    w = 0.0
    for poly in body.data.polygons:
        m = mats[poly.material_index]
        if not m or "body" not in m.name.lower():
            continue
        c = poly.center
        if abs(c.y - y) < 0.06 and abs(c.z - z) < 0.06:
            w = max(w, abs(c.x))
    return w


def back_top(hz, y):
    body = hz.meshes[0]
    mats = body.data.materials
    top = -1.0
    for poly in body.data.polygons:
        m = mats[poly.material_index]
        if m and "body" in m.name.lower() and abs(poly.center.y - y) < 0.05 and abs(poly.center.x) < 0.06:
            top = max(top, poly.center.z)
    return top


def _box(bm, centre, size, rot=None):
    m = Matrix.Translation(centre) @ (rot or Matrix.Identity(4)) @ Matrix.Diagonal((*size, 1.0))
    bmesh.ops.create_cube(bm, size=1.0, matrix=m)


def _tube(bm, a, b, r1, r2, segs=10):
    d = b - a
    rot = d.to_track_quat("Z", "Y").to_matrix().to_4x4()
    m = Matrix.Translation((a + b) / 2) @ rot
    bmesh.ops.create_cone(bm, cap_ends=True, segments=segs, radius1=r1, radius2=r2, depth=d.length, matrix=m)


def _mat(name, rgb, rough=0.8):
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    bsdf = next(n for n in mat.node_tree.nodes if n.type == "BSDF_PRINCIPLED")
    bsdf.inputs["Base Color"].default_value = (*rgb, 1.0)
    bsdf.inputs["Roughness"].default_value = rough
    return mat


def military_saddle(hz, name, leather=(0.20, 0.12, 0.06), caps=(0.05, 0.04, 0.03)):
    """An 18th-century military saddle: padded seat between a high pommel and cantle,
    flaps down the barrel, and two pistol holsters with bearskin caps before the knees.
    Rigid on the `saddle` bone."""
    sad = hz.head("saddle")
    y0 = sad.y
    top = back_top(hz, y0)
    w = max(_body_section(hz, y0, top - 0.08), 0.12)
    wl = max(_body_section(hz, y0 - 0.05, top - 0.30), w)
    bm = bmesh.new()
    _box(bm, Vector((0, y0 + 0.02, top + 0.035)), (0.30, 0.50, 0.07))  # seat
    _box(bm, Vector((0, y0 - 0.22, top + 0.10)), (0.24, 0.08, 0.16), Matrix.Rotation(math.radians(-15), 4, "X"))  # pommel
    _box(bm, Vector((0, y0 + 0.25, top + 0.09)), (0.30, 0.07, 0.13), Matrix.Rotation(math.radians(20), 4, "X"))  # cantle
    for sx in (-1, 1):
        # Flap: from the seat's edge down the barrel.
        a = Vector((sx * (w + 0.015), y0 - 0.02, top - 0.02))
        b = Vector((sx * (wl + 0.03), y0 - 0.04, top - 0.34))
        ang = math.atan2((b - a).x, -(b - a).z)
        _box(bm, (a + b) / 2, (0.012, 0.36, (b - a).length), Matrix.Rotation(ang, 4, "Y"))
    for sx in (-1, 1):
        a = Vector((sx * (w + 0.06), y0 - 0.28, top + 0.04))
        b = a + Vector((sx * 0.06, -0.10, -0.30))
        _tube(bm, a, b, 0.045, 0.03)
    holster_n = len(bm.faces)
    for sx in (-1, 1):
        a = Vector((sx * (w + 0.06), y0 - 0.28, top + 0.04))
        _tube(bm, a + Vector((sx * 0.005, 0.0, -0.03)), a + Vector((-sx * 0.01, 0.015, 0.06)), 0.058, 0.05)
    for i, f in enumerate(bm.faces):
        f.material_index = 1 if i >= holster_n else 0
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    obj = bpy.data.objects.new(name, me)
    bpy.context.scene.collection.objects.link(obj)
    me.materials.append(_mat(name + ".leather", leather))
    me.materials.append(_mat(name + ".bearskin", caps, 0.95))
    gear.rigid(obj, hz.arm, "saddle", Matrix.Identity(4), part=horse.HORSE_PART)
    return obj


# ----------------------------------------------------------------------------- held gear

def flag(ch, name):
    """A cavalry standard in the right hand: a pole (`wood`) and a square cloth (`sash`,
    the standard's colour), two held objects. Returns (pole, cloth)."""
    sock = am.world(ch.arm, P + "socket.hand_r")
    out = []
    for part, key in (("pole", "wood"), ("cloth", "sash")):
        bm = bmesh.new()
        if part == "pole":
            _tube(bm, Vector((0, 0, -0.55)), Vector((0, 0, 1.75)), 0.016, 0.014, 8)
            _tube(bm, Vector((0, 0, 1.75)), Vector((0, 0, 1.86)), 0.03, 0.002, 8)
        else:
            _box(bm, Vector((0.0, 0.27, 1.42)), (0.01, 0.52, 0.48))
        me = bpy.data.meshes.new(f"{name}_{part}")
        bm.to_mesh(me)
        bm.free()
        obj = bpy.data.objects.new(f"{name}_{part}", me)
        bpy.context.scene.collection.objects.link(obj)
        am.attach(obj, ch.arm, P + "socket.hand_r", Matrix.Translation(sock.translation + Vector((0, 0, -0.05))))
        ch.held["flag_" + part] = obj
        ch.keys[obj.name] = key
        out.append(obj)
    return out


# ----------------------------------------------------------------------------- build

def frames(md, key):
    return rig.md_clip_frames(md[key])[0]


# Gait cycles of the source per MOTION_L + MOTION_R (one sprite cycle): 2 plays a full
# stride in each half, so that the stride matches the game's speed (no sliding hooves).
# Source strides per MOTION_L + MOTION_R. The dragoons move 13.6 px (0.45 m) a tick: 6.4 m
# per 14-frame cycle = three trot strides of 2.14 m (the pack's trot, at 1.5 m withers),
# so the hooves do not slide (the trot plays 1.4 times faster). The natives' 25 px a tick
# over 10 frames (8.3 m) is two gallop strides of 3.9 m.
CYCLES = {"MOTION": 3, "MOTION1": 2}


def horse_clips(md, cycles=CYCLES):
    out = {}
    for key, clip in GAITS.items():
        if key in ("ATTACK", "ATTACK1") and key not in md:
            continue
        n = clip[2] or (frames(md, key) if key in md else 1)
        out[key] = (clip[0], clip[1], n) + tuple(clip[3:])
    # MOTION_L covers its share of the cycles (by frames), MOTION_R the rest.
    for sfx, gait, k in (("", "G1_Trot", cycles.get("MOTION", 1)), ("1", "G3_Gallop", cycles.get("MOTION1", 1))):
        if "MOTION_L" + sfx not in md:
            continue
        nl, nr = frames(md, "MOTION_L" + sfx), frames(md, "MOTION_R" + sfx)
        mid = k * nl / (nl + nr)
        out["MOTION_L" + sfx] = (gait, "cycles", nl, 0.0, mid)
        out["MOTION_R" + sfx] = (gait, "cycles", nr, mid, float(k))
    return out


def lie_spot(hz):
    """The thrown rider's root (at his feet) a metre beyond the fallen horse's back (from
    its hooves through its spine), placed so that he lies along the back, head towards
    the horse's head; and his heading."""
    last = hz.worlds["DEATH"][-1]
    hooves = [b.name for b in hz.arm.data.bones if b.head_local.z < 0.12 and b.parent is not None and "root" not in b.name]
    s = last["spine_02"].translation
    f = sum((last[n].translation for n in hooves), Vector()) / max(len(hooves), 1)
    d = Vector((s.x - f.x, s.y - f.y, 0.0)).normalized()
    head = last["head"].translation
    along = Vector((head.x - s.x, head.y - s.y, 0.0)).normalized()
    return (s.x + d.x * 1.0 - along.x * 0.8, s.y + d.y * 1.0 - along.y * 0.8, 0.13), math.degrees(math.atan2(along.x, -along.y))


def make(ctx, spec):
    name, root = ctx.name, ctx.root
    md = spec["md"]
    hz = mount.load(name, root, horse_clips(md, spec.get("cycles", CYCLES)))
    if spec.get("tack") in ("military", "blanket", "bare"):
        mount.drop_faces(hz, ["saddle1"] if spec["tack"] == "military" else ["saddle1", "saddle2"])
    lie, heading = lie_spot(hz)
    sad = hz.head("saddle")
    origin = Vector((sad.x, sad.y + 0.02, sad.z - 0.90 + spec.get("seat_dz", 0.0)))
    oz, cy = origin.z, origin.y
    ch = Character(root, name, dict(BUILD, origin=tuple(origin)), prefix=P, table=rig.skeleton_h(P, tuple(origin)))
    mount.join(ch.arm, hz, P + "root")
    ch.body()
    if spec.get("dress", "coat") == "coat":
        ch.shell("waistcoat", "waistcoat", lambda c, n, h, p: h in TORSO and 0.97 + oz < c.z < 1.47 + oz, 0.012)
        ch.shell("coat", "coat", lambda c, n, h, p: (h in TORSO and 0.97 + oz < c.z < 1.49 + oz and not (c.y - cy < -0.14 and abs(c.x) < 0.07))
                 or h in ARMS, 0.03)
        ch.shell("breeches", "breeches", lambda c, n, h, p: (h in {"hips"} | LEGS and c.z < 1.0 + oz), 0.010)
        ch.shell("boots", "boots_high", lambda c, n, h, p: h in SHINS | FEET or (h in LEGS and c.z < 0.58 + oz), 0.02)
        skirt = bd.tube(ch.arm, f"{name}.coat_skirt", rig.SKIRT_TOP + oz, 0.72 + oz, (0.20, 0.15), (0.24, 0.20), gap_deg=80,
                        flare_back=0.03, columns=rig.SKIRT_COLUMNS, centre=(origin.x, origin.y), prefix=P)
        ch.add("clothes", skirt, "coat", symmetric=True)
        belt = bd.strap(ch.arm, f"{name}.belt", ch.flesh, Vector((0.12, cy, 1.47 + oz)), Vector((-0.17, cy, 0.95 + oz)),
                        0.06, 0.04, bones=[P + b for b in rig.BODY_BONES])
        ch.add("clothes", belt, "belts", symmetric=True)
    else:
        ch.shell("breechcloth", "breeches", lambda c, n, h, p: h in {"hips"} | LEGS and 0.80 + oz < c.z < 1.0 + oz, 0.012)
        ch.shell("leggings", "boots", lambda c, n, h, p: h in SHINS | FEET or (h in LEGS and c.z < 0.80 + oz), 0.012)
    ch.bone_layer()
    ch.caps()
    extra = []
    if spec.get("tack", "military") != "bare":
        extra.append(mount.saddlecloth(hz, f"{name}.saddlecloth", spec["palette"]["saddlecloth"],
                                       half_len=0.36 if spec.get("tack", "military") == "military" else 0.42))
    if spec.get("tack", "military") == "military":
        sad_obj = military_saddle(hz, f"{name}.saddle")
        # Leather like the rider's gear: decay material and colour from the sprites.
        sad_obj["acr_material_keys"] = ["saddle", "tack"]
        ch.add("gear", sad_obj, "saddle", symmetric=True)
    ch.horse = {"mount": hz, "extra": extra}

    # Seated pose: legs, then arms solved onto the reins.
    reins = hz.head("reins_01_l") if "reins_01_l" in hz.arm.data.bones else sad + Vector((0.1, -0.27, 0.0))
    targets = {s: Vector((0.10 * k, reins.y + 0.06, sad.z + 0.17)) for s, k in (("l", 1), ("r", -1))}
    am.pose(ch.arm, LEGS_SEAT, prefix=P)
    both = reins_world(ch.arm, targets)
    left = {k: v for k, v in both.items() if k.endswith("_l")}
    seat = _with_world(LEGS_SEAT, both)
    poses = {"seat": seat, "lean": _with_world(dict(LEGS_SEAT, spine=(-12, 0, 0)), both),
             "aim": _with_world(dict(LEGS_SEAT, **AIM_R, **AIM_L), {"socket.hand_r": (88, 0, 0)}),
             "cut_up": _with_world(dict(LEGS_SEAT, **CUT_UP_R), dict(left, **{"socket.hand_r": (-40, 0, 0)})),
             "cut_down": _with_world(dict(LEGS_SEAT, **CUT_DOWN_R), dict(left, **{"socket.hand_r": (110, 0, 0)})),
             "flag": _with_world(dict(LEGS_SEAT, **FLAG_R), dict(left, **{"socket.hand_r": (0, 0, 0)}))}
    lie_spec = dict(rig_lie(), world={"root": (90, 0, heading)}, world_loc={"root": lie})
    ch.poses, ch.lie = poses, lie_spec
    base = poses["flag"] if spec.get("held") == "flag" else poses["seat"]
    am.pose(ch.arm, base, prefix=P)

    hat = spec.get("hat", "cap")
    if hat:
        if hat == "cap":
            cap = gear.box(f"{name}.cap", (0.15, 0.17, 0.26), bevel=0.3)
            for v in cap.data.vertices:
                if v.co.z > 0:
                    v.co.x *= 0.55
                    v.co.y *= 0.7
            lift = 0.10
        else:
            cap = gear.box(f"{name}.cap", (0.30, 0.28, 0.09), bevel=0.4)
            lift = 0.05
        am.attach(cap, ch.arm, P + "socket.hat", Matrix.Translation(am.world(ch.arm, P + "socket.hat").translation + Vector((0, 0, lift))))
        ch.held["cap"] = cap
        ch.keys[cap.name] = "cap"
    if spec.get("held") == "flag":
        flag(ch, f"{name}.flag")
    if spec.get("sabre", True):
        sab = gear.sabre(f"{name}.sabre")
        am.attach(sab, ch.arm, P + "socket.hip_l", Matrix.Translation(am.world(ch.arm, P + "socket.hip_l").translation)
                  @ Matrix.Rotation(math.radians(40), 4, "X"))
        ch.held["sabre"] = sab
        ch.keys[sab.name] = "metal"
    return ch


def _keyed(arm, specs):
    still = ("index_", "middle_", "ring_", "pinky_", "thumb_", "socket.")
    rot = {b.name[len(P):] for b in arm.data.bones if b.name.startswith(P)
           and not any(b.name[len(P):].startswith(t) for t in still)}
    loc = set()
    for s in specs:
        r, lo = am.spec_bones(s)
        rot |= r
        loc |= lo
    return rot, loc


def _key(ch, name, horse_frames, rider_keys):
    keyed = _keyed(ch.arm, [s for _f, s in rider_keys])
    mount.key_clip(ch.arm, name, horse_frames, rider_keys, lambda arm, s, f: am.pose(arm, s, frame=f, prefix=P, keyed=keyed))


def thrown(ch, t):
    if t >= 1.0:
        return ch.lie
    s = ch.poses["seat"]
    return dict(s, spine=(-30 * t, 0, 0), thigh_l=(-50, -30, 0), thigh_r=(-50, 30, 0),
                world={k: v for k, v in s["world"].items() if not k.startswith("upperarm")},
                upperarm_l=(-120 * t, 40, 0), upperarm_r=(-120 * t, -40, 0))


def add_clips(ch, md, held):
    W = ch.horse["mount"].worlds
    p = ch.poses
    ride = p["flag"] if held == "flag" else p["seat"]
    hold = [(0.0, ride), (1.0, ride)]
    plan = {
        "STAND": (W["STAND"], [(0.0, ride)]), "REST": (W["REST"], hold),
        "MOTION_L": (W["MOTION_L"], hold), "MOTION_R": (W["MOTION_R"], hold),
        "MOTION_LB": (W["MOTION_R"][::-1], hold), "MOTION_RB": (W["MOTION_L"][::-1], hold),
        "WALK": (W["WALK"], hold), "GALLOP": (W["GALLOP"], [(0.0, p["lean"]), (1.0, p["lean"])]),
        "PATTACK": (W["STAND"], [(0.0, p["aim"])]), "UATTACK": (W["STAND"], [(0.0, p["aim"])]),
        "PSTAND": (W["STAND"], [(0.0, p["aim"])]),
        "DEATH": (W["DEATH"], [(0.0, ride), (0.35, thrown(ch, 0.5)), (0.75, thrown(ch, 0.9)), (1.0, ch.lie)]),
        "DEATHLIE1": (W["DEATH"][-1:], [(0.0, ch.lie)]),
    }
    if "ATTACK" in W:
        plan["ATTACK"] = (W["ATTACK"], [(0.0, p["aim"]), (0.08, dict(p["aim"], spine=(4, 0, 0))), (0.25, p["aim"]), (1.0, p["aim"])])
    if "ATTACK1" in W:
        plan["ATTACK1"] = (W["ATTACK1"], [(0.0, ride), (0.3, p["cut_up"]), (0.45, p["cut_down"]), (0.7, ride), (1.0, ride)])
    if "MOTION_L1" in W:
        lean = [(0.0, p["lean"]), (1.0, p["lean"])]
        plan["MOTION_L1"], plan["MOTION_R1"] = (W["MOTION_L1"], lean), (W["MOTION_R1"], lean)
        plan["MOTION_LB1"], plan["MOTION_RB1"] = (W["MOTION_R1"][::-1], lean), (W["MOTION_L1"][::-1], lean)
    for name, (hf, rk) in plan.items():
        n = len(hf)
        _key(ch, name, hf, [(int(round(t * (n - 1))), s) for t, s in rk])
    lie_loc = ch.lie["world_loc"]["root"]
    for name, ks in crawl.clips().items():
        rider_keys = []
        for f, s in ks:
            s = dict(s)
            loc = s.pop("root_loc")
            s["world"] = {"root": s.pop("root")}
            s["world_loc"] = {"root": (lie_loc[0] + loc[0], lie_loc[1] + loc[1] - crawl.PRONE_LOC[1], loc[2])}
            rider_keys.append((f, s))
        _key(ch, name, W["DEATH"][-1:], rider_keys)
    return list(plan)


def build(ctx, spec):
    ch = make(ctx, spec)
    hz = ch.horse["mount"]
    heads.variants(ch)
    ch.morphs()
    rider = ch.layers["flesh"] + ch.layers["clothes"]
    ch.masks()
    made = add_clips(ch, spec["md"], spec.get("held"))
    am.set_clip(ch.arm, "STAND")
    report = {}
    colours = ch.colours(ctx.ref, spec["palette"], report=report)
    heads.finish(ch, rider)
    am.set_clip(ch.arm, "STAND")
    ch.arm.animation_data.action = None
    horse_meshes = hz.meshes + ch.horse["extra"]
    for o in horse_meshes:
        part = o.data.attributes.get("_part") or o.data.attributes.new("_part", "FLOAT", "POINT")
        for d in part.data:
            d.value = float(horse.HORSE_PART)
    tris = ch.triangles()
    tris["horse"] = hz.stats["triangles"]
    for o in ch.horse["extra"]:
        o.data.calc_loop_triangles()
        tris[o.name] = len(o.data.loop_triangles)
    extra_clips = [c for c in made if c not in spec["md"]]
    prepare_export(ch.all_meshes() + horse_meshes, ctx.root, acr_triangles=sum(tris.values()),
                   acr_clips=[c for c in made if c in spec["md"]] + list(crawl.CLIPS), acr_limb_radius=ch.limb_radius,
                   acr_layers=list(ch.layers), acr_extra_clips=extra_clips, **faces.extras(spec.get("faces", "regular")))
    # Where things are in the corpse (a check that the rider lies clear of the horse).
    am.set_clip(ch.arm, "DEATHLIE1")
    pb = ch.arm.pose.bones
    corpse = {k: [round(x, 2) for x in pb[b].head] for k, b in (("horse_spine", "spine_02"), ("horse_head", "head"),
                                                                ("rider_root", P + "root"), ("rider_head", P + "head"))}
    ch.arm.animation_data.action = None
    am.set_clip(ch.arm, "STAND")
    ch.arm.animation_data.action = None
    ctx.stats = {"triangles": tris, "colours": colours, "horse": hz.stats, "corpse": corpse}
