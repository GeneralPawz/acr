"""The gun crew of the field cannon `ArtPus` (#287): four artillerymen on skeleton H, one
armature with the gun (bones `c0.` .. `c3.`, the riders' idea: the crew is part of the
gun's model, so every clip moves gun and crew together).

The gun faces -Y (muzzle at y = -1.4 in battery, trail end at +2.75, wheels at x = ±0.82).
A man's pose is given in his own frame (facing -Y, his right = -X, `acr_assets.poses`
conventions) and placed at a station (x, y) turned by a yaw (degrees about +Z; -90 faces
-X, 90 faces +X, 180 faces the gun from in front).

  c0 sponger and rammer: the sponge/rammer staff
  c1 loader: the leather cartridge bag on his hip, cartridge and ball into the muzzle
  c2 ventsman and layer: the handspike in the trail (traverse), thumbs and primes the vent
  c3 firer: the linstock with its slow match

`ATTACK` (420 frames, the shot at frame 29 like the gun's keys): lay (c2 shifts the trail
with the handspike, 0..20), all stand clear, c3 touches the linstock to the vent (24..29),
recoil (29..36: the piece runs 0.9 m back), sponge (60..130), load (135..170), ram
(175..235), prime (250..275), run back into battery (300..400: c0/c1 on the wheels, c2 on
the trail), back to the stations (400..419).

Pure data and placement here; `build` needs bpy.
"""

import math

from . import poses

PREFIXES = ("c0.", "c1.", "c2.", "c3.")
FIRE = 29  # the gun's shot frame (artpus.py) = the linstock on the vent
BACK = 0.9  # recoil (artpus.py)
# Staffs (metres along the socket's up axis from the hand): (butt, head).
STAFF = (-0.95, 1.45)
LINSTOCK = (-0.35, 0.85)
HANDSPIKE = (-0.25, 1.25)

# Tool orientations (socket.hand_r "world", the man's frame): upright, forward level,
# forward and down (into the trail), pointing at the vent below and to the front.
UP = (0, 0, 0)
LEVEL = (90, 0, 0)
DOWN = (125, 0, 0)
VENT = (105, 0, 18)

STAND = {
    "upperarm_l": (6, 14, 0), "forearm_l": (-10, 0, 0),
    "upperarm_r": (-8, -12, 0), "forearm_r": (-55, 0, 10), "hand_r": (0, 0, -10),
    "spine": (-3, 0, 0), "head": (6, 0, 0),
}
CLEAR = dict(STAND, spine=(6, 0, 0), spine1=(4, 0, 0), head=(-4, 0, 0), thigh_l=(10, 0, 0), shin_l=(6, 0, 0))
# Both hands on a level staff before the chest, leaning in (sponge, ram).
STAFF_IN = {
    "spine": (-14, 0, 0), "spine1": (-6, 0, 0), "head": (10, 0, 0),
    "upperarm_r": (-62, -16, -10), "forearm_r": (-38, 0, 10),
    "upperarm_l": (-70, 22, 20), "forearm_l": (-20, 0, -10),
    "thigh_l": (-26, -3, 0), "shin_l": (24, 0, 0), "thigh_r": (12, 3, 0), "shin_r": (6, 0, 0),
}
STAFF_OUT = dict(STAFF_IN, spine=(-4, 0, 0), upperarm_r=(-40, -14, -6), forearm_r=(-60, 0, 10),
                 upperarm_l=(-48, 20, 14), forearm_l=(-50, 0, -10), thigh_l=(-10, -3, 0), shin_l=(10, 0, 0))
# Arms out and forward, head down: pushing on wheel spokes or reaching into the muzzle.
PUSH = {
    "spine": (-22, 0, 0), "spine1": (-8, 0, 0), "head": (14, 0, 0),
    "upperarm_r": (-68, -10, 0), "forearm_r": (-18, 0, 0), "upperarm_l": (-68, 10, 0), "forearm_l": (-18, 0, 0),
    "thigh_l": (-30, -3, 0), "shin_l": (30, 0, 0), "thigh_r": (20, 3, 0), "shin_r": (10, 0, 0),
}
REACH = dict(STAND, spine=(-18, 0, 0), spine1=(-8, 0, 0), head=(16, 0, 0),
             upperarm_r=(-66, -8, -6), forearm_r=(-22, 0, 8), hand_r=(0, 0, 0))
BAG = dict(STAND, upperarm_r=(10, -14, 0), forearm_r=(-80, 0, 20), head=(20, 0, -10), spine=(-8, 0, 0))
# The handspike in the trail: bent over it, both hands on the lever.
SPIKE = {
    "spine": (-34, 0, 0), "spine1": (-10, 0, 0), "head": (24, 0, 0),
    "upperarm_r": (-50, -10, 0), "forearm_r": (-30, 0, 0), "upperarm_l": (-55, 14, 0), "forearm_l": (-30, 0, 0),
    "thigh_l": (-30, -3, 0), "shin_l": (36, 0, 0), "thigh_r": (14, 3, 0), "shin_r": (14, 0, 0),
}
# The linstock to the vent: right arm out, leaning in; recoiling from the shot.
TOUCH = dict(STAND, spine=(-16, 0, 6), spine1=(-6, 0, 0), head=(12, 0, 4),
             upperarm_r=(-58, -6, -4), forearm_r=(-10, 0, 0), hand_r=(0, 0, 0),
             upperarm_l=(10, 30, 0), thigh_l=(10, -3, 0), thigh_r=(-24, 3, 0), shin_r=(20, 0, 0))
FLINCH = dict(STAND, spine=(10, 0, -8), spine1=(6, 0, 0), head=(-8, 0, -20), upperarm_l=(-30, 30, 20),
              forearm_l=(-90, 0, 0), thigh_r=(14, 3, 0))
# Thumb on the vent / pricking and priming: bent over the breech.
VENTING = dict(STAND, spine=(-30, 0, 0), spine1=(-10, 0, 0), head=(20, 0, 0),
               upperarm_l=(-60, 10, 0), forearm_l=(-30, 0, 0), thigh_l=(-14, -3, 0), shin_l=(14, 0, 0))
PRIMING = dict(VENTING, upperarm_r=(-66, -6, 0), forearm_r=(-40, 0, 0), hand_r=(0, 0, 0))


def tool(spec, rot):
    d = dict(spec)
    d["world"] = {"socket.hand_r": rot}
    return d


def place(spec, station, yaw):
    """`spec` in the man's frame moved to `station` (x, y) and turned by `yaw` degrees."""
    d = {k: v for k, v in spec.items() if k not in ("world", "root", "root_loc")}
    rx, ry, rz = spec.get("root", (0, 0, 0))
    d["root"] = (rx, ry, rz + yaw)
    lx, ly, lz = spec.get("root_loc", (0, 0, 0))
    c, s = math.cos(math.radians(yaw)), math.sin(math.radians(yaw))
    d["root_loc"] = (station[0] + c * lx - s * ly, station[1] + s * lx + c * ly, lz)
    d["world"] = {k: (v[0], v[1], v[2] + yaw) for k, v in spec.get("world", {}).items()}
    return d


def _walk(pose, n=36, strides=3):
    """Keys [(frame, pose)] of `strides` step cycles over `n` frames with `pose` on top."""
    cyc = poses.walk_cycle(STAND)
    out = []
    per = n / strides
    for i in range(strides):
        for j, w in enumerate(cyc):
            legs = {k: v for k, v in w.items() if k.startswith(("thigh", "shin", "foot")) or k == "root_loc"}
            out.append((round(i * per + j * per / 4), dict(pose, **legs)))
    out.append((n, dict(pose, **{k: v for k, v in cyc[0].items() if k.startswith(("thigh", "shin", "foot")) or k == "root_loc"})))
    return out


# Stations (x, y, yaw) in battery.
HOME = {
    "c0.": (1.25, -0.2, -90),
    "c1.": (-1.25, -0.2, 90),
    "c2.": (0.0, 3.15, 0),
    "c3.": (1.3, 0.9, -90),
}
HELD = {"c0.": tool(STAND, UP), "c1.": STAND, "c2.": tool(STAND, UP), "c3.": tool(STAND, UP)}


def stand_keys():
    return {p: [(0, place(HELD[p], HOME[p][:2], HOME[p][2]))] for p in PREFIXES}


def motion_keys(n=36):
    """Moving: c0/c1 push on the wheels from behind, c2 on the trail, c3 walks beside."""
    plan = {"c0.": (tool(PUSH, UP), (1.2, 1.05), 0), "c1.": (PUSH, (-1.2, 1.05), 0),
            "c2.": (tool(SPIKE, DOWN), (0.0, 3.15), 0), "c3.": (tool(STAND, UP), (1.45, 1.4), 0)}
    out = {}
    for p, (pose, st, yaw) in plan.items():
        out[p] = [(f, place(s, st, yaw)) for f, s in _walk(pose, n)]
    return out


def attack_keys():
    """The drill on the 420 frames of ATTACK."""
    B = BACK
    k = {p: [] for p in PREFIXES}

    def at(p, f, pose, x, y, yaw):
        k[p].append((f, place(pose, (x, y), yaw)))

    # c0: sponge and ram.
    at("c0.", 0, tool(STAND, UP), 1.25, -0.2, -90)
    at("c0.", 20, tool(STAND, UP), 1.25, -0.2, -90)
    at("c0.", 26, tool(CLEAR, UP), 1.45, -1.05, -90)
    at("c0.", 44, tool(CLEAR, UP), 1.45, -1.05, -90)
    at("c0.", 56, tool(STAND, UP), 0.6, -1.9, 180)
    for i, f in enumerate(range(64, 131, 11)):  # sponge: in and out of the bore
        at("c0.", f, tool(STAFF_IN if i % 2 == 0 else STAFF_OUT, LEVEL), -0.08, -1.55 - (0.0 if i % 2 == 0 else 0.35), 180)
    at("c0.", 138, tool(STAND, UP), 0.6, -1.9, 180)
    at("c0.", 168, tool(STAND, UP), 0.6, -1.9, 180)
    for i, f in enumerate(range(178, 236, 14)):  # ram twice home
        at("c0.", f, tool(STAFF_OUT if i % 2 == 0 else STAFF_IN, LEVEL), -0.08, -1.9 if i % 2 == 0 else -1.45, 180)
    at("c0.", 246, tool(STAND, UP), 0.6, -1.9, 180)
    at("c0.", 290, tool(STAND, UP), 1.2, 1.15 + B, 0)
    at("c0.", 300, tool(PUSH, UP), 1.2, 1.05 + B, 0)
    at("c0.", 400, tool(PUSH, UP), 1.2, 1.05, 0)
    at("c0.", 419, tool(STAND, UP), 1.25, -0.2, -90)
    # c1: clear, then cartridge and ball from the bag into the muzzle.
    at("c1.", 0, STAND, -1.25, -0.2, 90)
    at("c1.", 20, STAND, -1.25, -0.2, 90)
    at("c1.", 26, CLEAR, -1.45, -1.05, 90)
    at("c1.", 44, CLEAR, -1.45, -1.05, 90)
    at("c1.", 60, STAND, -1.05, -0.75, 90)
    at("c1.", 125, STAND, -1.05, -0.75, 90)
    at("c1.", 135, BAG, -1.05, -0.75, 90)
    at("c1.", 150, REACH, -0.75, -0.75, 90)
    at("c1.", 162, REACH, -0.65, -0.75, 90)
    at("c1.", 172, STAND, -1.05, -0.75, 90)
    at("c1.", 290, STAND, -1.2, 1.15 + B, 0)
    at("c1.", 300, PUSH, -1.2, 1.05 + B, 0)
    at("c1.", 400, PUSH, -1.2, 1.05, 0)
    at("c1.", 419, STAND, -1.25, -0.2, 90)
    # c2: lays the gun with the handspike, stands clear, serves the vent, primes.
    at("c2.", 0, tool(SPIKE, DOWN), 0.0, 3.15, 0)
    at("c2.", 6, tool(dict(SPIKE, spine=(-34, 0, 12)), DOWN), -0.06, 3.15, 0)
    at("c2.", 12, tool(dict(SPIKE, spine=(-34, 0, -12)), DOWN), 0.06, 3.15, 0)
    at("c2.", 18, tool(SPIKE, DOWN), 0.0, 3.15, 0)
    at("c2.", 26, tool(CLEAR, UP), -1.3, 2.6, 90)
    at("c2.", 44, tool(CLEAR, UP), -1.3, 2.6, 90)
    at("c2.", 58, tool(VENTING, UP), -0.45, 2.3, 0)
    at("c2.", 240, tool(VENTING, UP), -0.45, 2.3, 0)
    at("c2.", 255, tool(PRIMING, LEVEL), -0.45, 2.3, 0)
    at("c2.", 262, tool(dict(PRIMING, forearm_r=(-55, 0, 0)), LEVEL), -0.45, 2.3, 0)
    at("c2.", 270, tool(PRIMING, LEVEL), -0.45, 2.3, 0)
    at("c2.", 282, tool(STAND, UP), -0.3, 3.4 + B, 0)
    at("c2.", 300, tool(SPIKE, DOWN), 0.0, 3.15 + B, 0)
    at("c2.", 400, tool(SPIKE, DOWN), 0.0, 3.15, 0)
    at("c2.", 419, tool(SPIKE, DOWN), 0.0, 3.15, 0)
    # c3: the linstock to the vent on FIRE, flinch from the recoil, stand ready.
    at("c3.", 0, tool(STAND, UP), 1.3, 0.9, -90)
    at("c3.", 18, tool(STAND, UP), 1.3, 0.9, -90)
    at("c3.", 25, tool(TOUCH, VENT), 1.22, 0.9, -90)
    at("c3.", FIRE, tool(TOUCH, VENT), 1.22, 0.9, -90)
    at("c3.", FIRE + 4, tool(FLINCH, UP), 1.5, 1.0, -90)
    at("c3.", 50, tool(STAND, UP), 1.4, 0.95, -90)
    at("c3.", 419, tool(STAND, UP), 1.3, 0.9, -90)
    return {p: stepped(ks) for p, ks in k.items()}


STRIDE = 0.55  # metres a step carries a man between stations
LEG_BONES = ("thigh_l", "thigh_r", "shin_l", "shin_r", "foot_l", "foot_r")


def stepped(keys, min_move=0.5):
    """Placed keys [(frame, spec)] with walking steps keyed into each move of more than
    `min_move` metres: alternate contact and passing poses of `poses.walk_cycle`, the root
    sliding along the way with the cycle's bob, the upper body of the key left (first half)
    and of the key reached (second half), turned towards the new facing after the first step."""
    cyc = poses.walk_cycle(STAND)
    out = [keys[0]]
    for (f0, a), (f1, b) in zip(keys, keys[1:]):
        (x0, y0, z0), (x1, y1, _z1) = a["root_loc"], b["root_loc"]
        d = math.hypot(x1 - x0, y1 - y0)
        if d > min_move and f1 - f0 >= 8:
            n = max(2, round(d / STRIDE))
            for j in range(1, 2 * n):
                t = j / (2 * n)
                f = f0 + round((f1 - f0) * t)
                if f <= out[-1][0] or f >= f1:
                    continue
                body = a if t < 0.5 else b
                c = cyc[j % 4]
                spec = {k: v for k, v in body.items() if k not in LEG_BONES}
                spec.update({k: c[k] for k in LEG_BONES if k in c})
                spec["root"] = a["root"] if j == 1 else b["root"]
                spec["root_loc"] = (x0 + (x1 - x0) * t, y0 + (y1 - y0) * t, z0 + c.get("root_loc", (0, 0, 0))[2])
                spec["world"] = dict((a if j == 1 else b).get("world", {}))
                out.append((f, spec))
        out.append((f1, b))
    return out


def death_keys(n=73):
    """All four fall where they stand (the gun destroyed with its crew); lie in DEATHLIE1."""
    lie = poses.lie_prone(arms_out=True)
    out, lies = {}, {}
    for i, p in enumerate(PREFIXES):
        x, y, yaw = HOME[p]
        base = HELD[p]
        stand = dict(base, world=dict(base.get("world", {})))
        ks = poses.death_keys(stand, lie)
        delay = 2 * i
        out[p] = [(min(n - 1, delay + round(t * (n - 1 - 6))), place(s, (x, y), yaw + 25 * (i - 1.5))) for t, s in ks]
        out[p].append((n - 1, out[p][-1][1]))
        lies[p] = [(0, out[p][-1][1])]
    return out, lies


def clips():
    """{clip: {prefix: [(frame, spec)]}} for every gun clip."""
    st = stand_keys()
    mo = motion_keys()
    death, lie = death_keys()
    return {"STAND": st, "MOTION_L": mo, "MOTION_R": mo, "ATTACK": attack_keys(), "DEATH": death, "DEATHLIE1": lie}


# --- build (bpy) ---------------------------------------------------------------------------

PALETTE = {
    # A neutral artillery coat: blue with red facings (Royal Artillery and the French
    # Corps royal de l'artillerie both wore blue; the other nations fall back to it).
    "skin": (0.80, 0.58, 0.45), "bone": (0.85, 0.81, 0.69),
    "coat": (0.13, 0.16, 0.32), "waistcoat": (0.70, 0.12, 0.08), "breeches": (0.80, 0.78, 0.70),
    "gaiters": (0.10, 0.10, 0.10), "belts": (0.85, 0.80, 0.62), "hat": (0.07, 0.07, 0.07),
    "cartridge": (0.25, 0.16, 0.08), "wood": (0.42, 0.28, 0.15), "metal": (0.30, 0.30, 0.32),
    "sash": (0.55, 0.09, 0.05), "knapsack": (0.20, 0.17, 0.15),
}
TORSO = {"hips", "spine", "spine1", "spine2", "clavicle_l", "clavicle_r"}
ARMS = {"upperarm_l", "upperarm_r", "forearm_l", "forearm_r"}
LEGS = {"thigh_l", "thigh_r"}
SHINS = {"shin_l", "shin_r"}
FEET = {"foot_l", "foot_r", "toe_l", "toe_r"}
BUILD = {"width": 1.02, "shoulders": 1.10, "limbs": 1.18, "hips": 1.0, "mh_body": True}


def _staff(name, span, r, heads=()):
    """A staff along +Z from span[0] to span[1] (the hand at 0), with cylinders `heads`
    = [(z, radius, length)]."""
    import bmesh
    from mathutils import Matrix

    from . import gear

    bm = bmesh.new()
    a, b = span
    bmesh.ops.create_cone(bm, cap_ends=True, segments=6, radius1=r, radius2=r, depth=b - a,
                          matrix=Matrix.Translation((0, 0, (a + b) / 2)))
    for z, hr, hl in heads:
        bmesh.ops.create_cone(bm, cap_ends=True, segments=8, radius1=hr, radius2=hr, depth=hl,
                              matrix=Matrix.Translation((0, 0, z)))
    return gear.from_bm(bm, name)


def _hold(ch, obj, key, slot):
    from mathutils import Vector

    from . import armature as am

    arm = ch.arm
    P = ch.prefix
    sock = am.world(arm, P + "socket.hand_r")
    rest_up = sock.to_3x3() @ arm.pose.bones[P + "socket.hand_r"].bone.matrix_local.to_3x3().inverted()
    from mathutils import Matrix

    am.attach(obj, arm, P + "socket.hand_r", Matrix.Translation(sock.translation + Vector((0, -0.01, 0))) @ rest_up.to_4x4())
    ch.held[slot] = obj
    ch.keys[obj.name] = key


def man(root, name, prefix, kit):
    """One artilleryman (coat, waistcoat, breeches, gaiters, tricorn) with his `kit`."""
    from mathutils import Matrix, Vector

    from . import armature as am
    from . import body as bd
    from . import gear, rig
    from .humans import Character

    ch = Character(root, name, BUILD, prefix=prefix, table=rig.skeleton_h(prefix))
    P = prefix
    ch.body(faces=False)
    strip = lambda h: h[len(P):] if h.startswith(P) else h
    ch.shell("waistcoat", "waistcoat", lambda c, n, h, p: strip(h) in TORSO and 0.97 < c.z < 1.47, 0.012)
    ch.shell("coat", "coat", lambda c, n, h, p: (strip(h) in TORSO and 0.97 < c.z < 1.49
                                                 and not (c.y < -0.04 and abs(c.x) < 0.075)) or strip(h) in ARMS, 0.03)
    ch.shell("breeches", "breeches", lambda c, n, h, p: (strip(h) in {"hips"} | LEGS and c.z < 1.0)
             or (strip(h) in SHINS and c.z > 0.40), 0.010)
    ch.shell("gaiters", "gaiters", lambda c, n, h, p: (strip(h) in SHINS and c.z <= 0.44) or strip(h) in FEET, 0.016)
    skirt = bd.tube(ch.arm, f"{name}.coat_skirt", rig.SKIRT_TOP, rig.SKIRT_BOTTOM, (0.24, 0.17), (0.21, 0.15),
                    gap_deg=72, flare_back=0.03, columns=rig.SKIRT_COLUMNS, prefix=P)
    ch.add("clothes", skirt, "coat", symmetric=True)
    belt = bd.strap(ch.arm, f"{name}.belt", ch.flesh, Vector((0.12, 0, 1.47)), Vector((-0.17, 0, 0.95)), 0.05, 0.04,
                    bones=[P + b for b in rig.BODY_BONES])
    ch.add("clothes", belt, "belts", symmetric=True)
    ch.bone_layer()
    ch.caps()
    if kit == "bag":
        bag = gear.box(f"{name}.cartridge_bag", (0.20, 0.08, 0.22))
        gear.rigid(bag, ch.arm, P + "hips", Matrix.Translation((-0.20, 0.06, 0.88)))
        ch.add("gear", bag, "cartridge")
    am.pose(ch.arm, STAND, prefix=P)
    hat = gear.tricorn(f"{name}.tricorn")
    am.attach(hat, ch.arm, P + "socket.hat", Matrix.Translation(am.world(ch.arm, P + "socket.hat").translation
                                                                + Vector((0, 0, -0.02))) @ Matrix.Scale(0.93, 4))
    ch.held["hat"] = hat
    ch.keys[hat.name] = "hat"
    if kit == "staff":
        # Sponge (fleece) at the head, rammer at the butt.
        _hold(ch, _staff(f"{name}.staff", STAFF, 0.022, [(STAFF[1] - 0.12, 0.075, 0.24), (STAFF[0] + 0.05, 0.068, 0.10)]),
              "wood", "staff")
        ch.held["staff"]["acr_material_keys"] = ["wood"]
    elif kit == "linstock":
        _hold(ch, _staff(f"{name}.linstock", LINSTOCK, 0.018, [(LINSTOCK[1] - 0.04, 0.035, 0.08)]), "wood", "linstock")
        match = _staff(f"{name}.match", (LINSTOCK[1], LINSTOCK[1] + 0.07), 0.012)
        _hold(ch, match, "sash", "match")
    elif kit == "handspike":
        _hold(ch, _staff(f"{name}.handspike", HANDSPIKE, 0.03), "wood", "handspike")
    am.pose(ch.arm, {}, prefix=P)
    return ch


KITS = {"c0.": "staff", "c1.": "bag", "c2.": "handspike", "c3.": "linstock"}


def join(target, arm):
    """Moves the bones of armature `arm` into `target`; its meshes follow."""
    import bpy

    kids = [o for o in bpy.context.scene.objects if o.parent is arm]
    saved = [(o, o.parent_type, o.parent_bone, o.matrix_parent_inverse.copy(), o.matrix_basis.copy()) for o in kids]
    for o in bpy.context.view_layer.objects:
        o.select_set(False)
    if bpy.context.object and bpy.context.object.mode != "OBJECT":
        bpy.ops.object.mode_set(mode="OBJECT")
    arm.select_set(True)
    target.select_set(True)
    bpy.context.view_layer.objects.active = target
    bpy.ops.object.join()
    for o, ptype, pbone, inv, basis in saved:
        o.parent = target
        o.parent_type = ptype
        if ptype == "BONE":
            o.parent_bone = pbone
        o.matrix_parent_inverse = inv
        o.matrix_basis = basis
        for md in o.modifiers:
            if md.type == "ARMATURE":
                md.object = target
    bpy.context.view_layer.update()


def build(root, gun_arm, name="artpus"):
    """The four men joined into `gun_arm`. Returns the Characters."""
    crew = []
    for i, p in enumerate(PREFIXES):
        ch = man(root, f"{name}.crew{i}", p, KITS[p])
        crew.append(ch)
    for ch in crew:
        join(gun_arm, ch.arm)
        ch.arm = gun_arm
    for ch in crew:
        ch.masks()
        ch.colours(None, PALETTE)
    return crew


def key_clips(arm, actions):
    """Keys the crew's specs into the gun's actions {clip: action}."""
    import bpy

    from . import armature as am

    still = ("index_", "middle_", "ring_", "pinky_", "thumb_", "socket.")
    plan = clips()
    for clip, act in actions.items():
        arm.animation_data_create()
        arm.animation_data.action = act
        if hasattr(arm.animation_data, "action_slot") and act.slots:
            arm.animation_data.action_slot = act.slots[0]
        for p, ks in plan[clip].items():
            rot = {b.name[len(p):] for b in arm.data.bones if b.name.startswith(p)
                   and not b.name[len(p):].startswith(still)}
            loc = {"root"}
            for _f, s in ks:
                r, lo = am.spec_bones(s)
                rot |= r
                loc |= lo
            for f, s in ks:
                am.pose(arm, s, frame=f, prefix=p, keyed=(rot, loc))
        for fc in am._fcurves(act):
            if fc.data_path.startswith('pose.bones["c'):
                for kp in fc.keyframe_points:
                    kp.interpolation = "BEZIER"
        arm.animation_data.action = None
    bpy.context.view_layer.update()
    # The exported scene pose is STAND.
    for p, ks in plan["STAND"].items():
        am.pose(arm, ks[0][1], prefix=p)
