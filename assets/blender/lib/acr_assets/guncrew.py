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
    "ik": "staff",
}
STAFF_OUT = dict(STAFF_IN, spine=(-4, 0, 0), upperarm_r=(-40, -14, -6), forearm_r=(-60, 0, 10),
                 upperarm_l=(-48, 20, 14), forearm_l=(-50, 0, -10), thigh_l=(-10, -3, 0), shin_l=(10, 0, 0))
# Arms out and forward, head down: pushing on wheel spokes or reaching into the muzzle.
PUSH = {
    "spine": (-40, 0, 0), "spine1": (-14, 0, 0), "head": (34, 0, 0),
    "upperarm_r": (-60, -10, 0), "forearm_r": (-20, 0, 0), "upperarm_l": (-60, 10, 0), "forearm_l": (-20, 0, 0),
    "thigh_l": (-38, -3, 0), "shin_l": (48, 0, 0), "foot_l": (-8, 0, 0),
    "thigh_r": (26, 3, 0), "shin_r": (14, 0, 0), "foot_r": (14, 0, 0), "root_loc": (0, 0, -0.07),
    "ik": "wheel",
}
REACH = dict(STAND, spine=(-18, 0, 0), spine1=(-8, 0, 0), head=(16, 0, 0),
             upperarm_r=(-66, -8, -6), forearm_r=(-22, 0, 8), hand_r=(0, 0, 0))
BAG = dict(STAND, upperarm_r=(10, -14, 0), forearm_r=(-80, 0, 20), head=(20, 0, -10), spine=(-8, 0, 0))
# The handspike in the trail: bent over it, both hands on the lever.
SPIKE = {
    "spine": (-34, 0, 0), "spine1": (-10, 0, 0), "head": (24, 0, 0),
    "upperarm_r": (-50, -10, 0), "forearm_r": (-30, 0, 0), "upperarm_l": (-55, 14, 0), "forearm_l": (-30, 0, 0),
    "thigh_l": (-30, -3, 0), "shin_l": (36, 0, 0), "thigh_r": (14, 3, 0), "shin_r": (14, 0, 0),
    "ik": "spike",
}
# The linstock to the vent: right arm out, leaning in; recoiling from the shot.
TOUCH = dict(STAND, spine=(-16, 0, 6), spine1=(-6, 0, 0), head=(12, 0, 4),
             upperarm_r=(-58, -6, -4), forearm_r=(-10, 0, 0), hand_r=(0, 0, 0),
             upperarm_l=(10, 30, 0), thigh_l=(10, -3, 0), thigh_r=(-24, 3, 0), shin_r=(20, 0, 0), ik="linstock")
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


PUSH_X = 1.02  # the wheel pushers stand just outside their wheel (x = 0.82)
# and this far behind (or in front of) the axle: leant well in, their shoulders clear of the
# rim their hands are on (the original's `ArtPus_Tlk`: backs near level, heads at the hubs).
PUSH_Y = 1.45

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


# The ammunition chest (artpus.py's `chest` bone, on the ground beside the trail at rest):
# its rope handles at the ends, metres from its bottom centre (x, z).
CHEST_REST = (1.15, 3.5)
CHEST_HANDLE = (0.34, 0.28)
CHEST_SPAN = 0.64  # a carrier's hips from the chest's middle: clear of its end and rope loop
CARRY_Y = 3.8  # the carriers walk behind the trail end (2.75)
CARRY_Z = 0.6  # the chest's bottom when carried (the handles at the hanging hands)
# Lifting the trail for a turn: the carriage turns up about the axle (degrees about X).
TRAIL_UP = 10.0
# A 16th of a turn rolls the wheels a spoke's pitch (12 spokes): seamless between steps.
TURN_ROLL = 30.0

# Carrying the chest by its rope handle, the other hand free (the tool in the right).
CARRY = dict(STAND, spine=(-2, 0, 0), ik="chest")
# Bent down to the chest on the ground: knees bent, the back leaning in.
STOOP = dict(STAND, spine=(-48, 0, 0), spine1=(-16, 0, 0), head=(28, 0, 0),
             thigh_l=(-70, -3, 0), shin_l=(95, 0, 0), foot_l=(-25, 0, 0),
             thigh_r=(-60, 3, 0), shin_r=(90, 0, 0), foot_r=(-30, 0, 0), root_loc=(0, 0.12, -0.4), ik="chest")
# Heaving a wheel round by its spokes: the push and the deeper lean of the heave.
HEAVE = dict(PUSH, spine=(-48, 0, 0), spine1=(-16, 0, 0), thigh_l=(-46, -3, 0), shin_l=(56, 0, 0),
             thigh_r=(34, 3, 0), shin_r=(10, 0, 0), root_loc=(0, -0.08, -0.11))
# The trail lifted on the handspike: upright enough to carry it, both hands on the lever.
LIFT = dict(SPIKE, spine=(-16, 0, 0), spine1=(-6, 0, 0), head=(14, 0, 0),
            thigh_l=(-14, -3, 0), shin_l=(16, 0, 0), thigh_r=(8, 3, 0), shin_r=(8, 0, 0))

# Turning (the original's `ArtPus_pvrt`): c2 lifts the trail end with the handspike and walks
# it round, c1 heaves the left wheel from behind, c0 the right one from in front (they turn
# opposite ways about the axle), c3 stands by the chest set down beside the trail.
TURN = {"c0.": (PUSH_X, -PUSH_Y, 180), "c1.": (-PUSH_X, PUSH_Y, 0), "c2.": (0.0, 3.15, 0), "c3.": (1.3, 2.95, -90)}


def motion_keys(n=36):
    """GUN_PUSH (`#MOTION_L0/R0`, the sprites' `ArtPus_Tlk`): c0/c1 push the gun on its wheels
    from behind, c2/c3 carry the ammunition chest behind the trail between them."""
    plan = {"c0.": (tool(PUSH, UP), (PUSH_X, PUSH_Y), 0), "c1.": (PUSH, (-PUSH_X, PUSH_Y), 0),
            "c2.": (tool(CARRY, UP), (-CHEST_SPAN, CARRY_Y), 0), "c3.": (tool(CARRY, UP), (CHEST_SPAN, CARRY_Y), 0)}
    out = {}
    for p, (pose, st, yaw) in plan.items():
        out[p] = [(f, place(s, st, yaw)) for f, s in _walk(pose, n)]
    return out


def chest_keys(n=41):
    """CHEST_CARRY (`#PATTACK0`, 42 frames, `ArtPus_Pdg_Tlk`; `#UATTACK0` plays it backwards):
    from the stations c0/c1 go to the wheels, c2/c3 to the chest's handles, stoop, lift it and
    carry it in behind the trail."""
    cx, cy = CHEST_REST
    k = {p: [] for p in PREFIXES}

    def at(p, f, pose, x, y, yaw):
        k[p].append((f, place(pose, (x, y), yaw)))

    for p, sd in (("c0.", 1), ("c1.", -1)):
        pose = tool(PUSH, UP) if p == "c0." else PUSH
        x, y, yaw = HOME[p]
        at(p, 0, HELD[p], x, y, yaw)
        at(p, 4, HELD[p], x, y, yaw)
        at(p, 30, pose, sd * PUSH_X, PUSH_Y, 0)
        k[p].append((n, place(_walk(pose)[0][1], (sd * PUSH_X, PUSH_Y), 0)))
    for p, sd in (("c2.", -1), ("c3.", 1)):
        x, y, yaw = HOME[p]
        at(p, 0, HELD[p], x, y, yaw)
        at(p, 14, tool(STAND, UP), cx + sd * CHEST_SPAN, cy, 0)
        at(p, 20, tool(STOOP, UP), cx + sd * CHEST_SPAN, cy, 0)
        at(p, 28, tool(CARRY, UP), cx + sd * CHEST_SPAN, cy, 0)
        k[p].append((n, place(_walk(tool(CARRY, UP))[0][1], (sd * CHEST_SPAN, CARRY_Y), 0)))
    return {p: stepped(ks, min_move=0.3) for p, ks in k.items()}


def lift_keys(n=70):
    """TRAIL_LIFT (`#PATTACK1`, 71 frames, `ArtPus_Pdg_pvrt`; `#UATTACK1` plays it backwards):
    from the stations to the turn's, c2 sets the handspike in the trail and lifts it."""
    k = {p: [] for p in PREFIXES}

    def at(p, f, pose, x, y, yaw):
        k[p].append((f, place(pose, (x, y), yaw)))

    for p in PREFIXES:
        x, y, yaw = HOME[p]
        at(p, 0, HELD[p], x, y, yaw)
        at(p, 6, HELD[p], x, y, yaw)
    tx, ty, tyaw = TURN["c0."]
    at("c0.", 36, tool(STAND, UP), tx, ty, tyaw)
    at("c0.", 46, tool(PUSH, UP), tx, ty, tyaw)
    at("c0.", n, tool(PUSH, UP), tx, ty, tyaw)
    tx, ty, tyaw = TURN["c1."]
    at("c1.", 36, STAND, tx, ty, tyaw)
    at("c1.", 46, PUSH, tx, ty, tyaw)
    at("c1.", n, PUSH, tx, ty, tyaw)
    tx, ty, tyaw = TURN["c2."]
    at("c2.", 30, tool(SPIKE, DOWN), tx, ty, tyaw)
    at("c2.", 44, tool(SPIKE, DOWN), tx, ty, tyaw)
    at("c2.", 66, tool(LIFT, DOWN), tx, ty, tyaw)
    k["c2."].append((n, place(_walk(tool(LIFT, DOWN), 40, 2)[0][1], (tx, ty), tyaw)))
    tx, ty, _tyaw = TURN["c3."]
    ready = _walk(tool(STAND, UP), 40, 1)[0][1]  # TRAIL_WALK's first frame: turned to step along
    at("c3.", 36, tool(STAND, UP), tx, ty, turn_heading(tx, ty))
    at("c3.", 52, ready, tx, ty, turn_heading(tx, ty))
    k["c3."].append((n, place(ready, (tx, ty), turn_heading(tx, ty))))
    return {p: stepped(ks) for p, ks in k.items()}


def turn_keys(n=40):
    """TRAIL_WALK (`#MOTION_L1`, 40 frames, `ArtPus_pvrt`; `#MOTION_R1` plays it backwards, and
    `#PSTAND1` between the steps holds its first frame): c2 walks the lifted trail round, c0/c1
    heave the wheels (WHEEL_HEAVE), c3 stands by the chest. First and last frames are alike."""
    out = {"c2.": [(f, place(s, TURN["c2."][:2], TURN["c2."][2])) for f, s in _walk(tool(LIFT, DOWN), n, 2)]}
    for p in ("c0.", "c1."):
        x, y, yaw = TURN[p]
        a, b = (tool(PUSH, UP), tool(HEAVE, UP)) if p == "c0." else (PUSH, HEAVE)
        out[p] = [(f, place(a if (f // 10) % 2 == 0 else b, (x, y), yaw)) for f in range(0, n + 1, 10)]
    # c3 steps along with the turn: the ground turns under the gun (a 16th per cycle the
    # other way), so at his station it runs past him at `turn_slip` per cycle; he walks
    # against it on the spot, one stride (two steps) per cycle, his feet keeping pace with it.
    x, y, _yaw = TURN["c3."]
    out["c3."] = [(f, place(s, (x, y), turn_heading(x, y))) for f, s in _walk(tool(STAND, UP), n, 1)]
    return out


TURN_STEP = 22.5  # degrees the sim turns the gun per `#MOTION_L1` cycle (clockwise from above)


def turn_slip(x, y):
    """Metres the ground runs past station (x, y) in one turn cycle (gun frame)."""
    return math.hypot(x, y) * math.radians(TURN_STEP)


def turn_heading(x, y):
    """The yaw (degrees) a man at station (x, y) faces to walk against the ground running past
    him in `#MOTION_L1`: the gun turns clockwise seen from above (`crew_view::gun_yaw`, its
    facing growing), so the ground turns anticlockwise under it, and he walks clockwise."""
    # Clockwise tangent at (x, y) is (y, -x); a man at yaw looks along (sin yaw, -cos yaw).
    return math.degrees(math.atan2(y, x))


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
    at("c0.", 290, tool(STAND, UP), PUSH_X, PUSH_Y + 0.1 + B, 0)
    at("c0.", 300, tool(PUSH, UP), PUSH_X, PUSH_Y + B, 0)
    at("c0.", 400, tool(PUSH, UP), PUSH_X, PUSH_Y, 0)
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
    at("c1.", 290, STAND, -PUSH_X, PUSH_Y + 0.1 + B, 0)
    at("c1.", 300, PUSH, -PUSH_X, PUSH_Y + B, 0)
    at("c1.", 400, PUSH, -PUSH_X, PUSH_Y, 0)
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
                # A man stepping between stations holds no grip (his hands go to it, or
                # leave it, with the keys either side), unless he holds it at both
                # (running the gun back into battery on its wheels).
                spec = {k: v for k, v in body.items() if k not in LEG_BONES and k != "ik"}
                if a.get("ik") and a.get("ik") == b.get("ik"):
                    spec["ik"] = a["ik"]
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


# Deploying (`#PATTACK4/5`, 171 frames, `ArtPusPdg_Vstr_1/2`; `#UATTACK4/5` play it
# backwards: limbering up). The trail's lift and bedding (artpus.py keys the carriage):
DEPLOY_UP = (48, 96)  # the trail raised by, lowered onto its bed again by
DEPLOY_HEAVE = (52, 92)  # the wheels heaved (the piece run up) between


def deploy_keys(n=170):
    """DEPLOY: from the stations c2 seats the handspike and raises the trail, c0/c1 heave
    the wheels to run the piece up, c2 lays it (the trail shifted aside and back) and
    lowers it onto its bed; c3 goes to the ammunition chest set down beside the trail and
    stoops to it (the charges taken out). All end at their stations (`#PSTAND4/5` and
    `ATTACK` start there); played backwards it is the limbering up."""
    k = {p: [] for p in PREFIXES}

    def at(p, f, pose, x, y, yaw):
        k[p].append((f, place(pose, (x, y), yaw)))

    for p in PREFIXES:
        x, y, yaw = HOME[p]
        at(p, 0, HELD[p], x, y, yaw)
        at(p, 8, HELD[p], x, y, yaw)
    # c2: the handspike into the trail, raise, lay, lower, back.
    tx, ty, _ = TURN["c2."]
    up0, down1 = DEPLOY_UP
    at("c2.", 26, tool(SPIKE, DOWN), tx, ty, 0)
    at("c2.", up0 - 6, tool(SPIKE, DOWN), tx, ty, 0)
    at("c2.", up0 + 8, tool(LIFT, DOWN), tx, ty, 0)
    at("c2.", 72, tool(LIFT, DOWN), tx - 0.14, ty, 0)
    at("c2.", 86, tool(LIFT, DOWN), tx, ty, 0)
    at("c2.", down1 + 6, tool(SPIKE, DOWN), tx, ty, 0)
    at("c2.", 126, tool(SPIKE, DOWN), tx, ty, 0)
    x, y, yaw = HOME["c2."]
    at("c2.", 150, HELD["c2."], x, y, yaw)
    at("c2.", n, HELD["c2."], x, y, yaw)
    # c0/c1: behind their wheels, heaving while the trail is up.
    h0, h1 = DEPLOY_HEAVE
    for p, sd in (("c0.", 1), ("c1.", -1)):
        push, heave = (tool(PUSH, UP), tool(HEAVE, UP)) if p == "c0." else (PUSH, HEAVE)
        stand = tool(STAND, UP) if p == "c0." else STAND
        at(p, 34, stand, sd * PUSH_X, PUSH_Y + 0.1, 0)
        at(p, 44, push, sd * PUSH_X, PUSH_Y, 0)
        for i, f in enumerate(range(h0, h1 + 1, 10)):
            at(p, f, heave if i % 2 else push, sd * PUSH_X, PUSH_Y, 0)
        at(p, 110, push, sd * PUSH_X, PUSH_Y, 0)
        at(p, 120, stand, sd * PUSH_X, PUSH_Y + 0.1, 0)
        x, y, yaw = HOME[p]
        at(p, 146, HELD[p], x, y, yaw)
        at(p, n, HELD[p], x, y, yaw)
    # c3: to the chest beside the trail, stoops to it, up again, back.
    cx, cy = CHEST_REST
    sx = cx + CHEST_SPAN
    at("c3.", 34, tool(STAND, UP), sx, cy, 0)
    at("c3.", 46, tool(STOOP, UP), sx, cy, 0)
    at("c3.", 84, tool(STOOP, UP), sx, cy, 0)
    at("c3.", 96, tool(STAND, UP), sx, cy, 0)
    x, y, yaw = HOME["c3."]
    at("c3.", 130, HELD["c3."], x, y, yaw)
    at("c3.", n, HELD["c3."], x, y, yaw)
    return {p: stepped(ks) for p, ks in k.items()}


def ik_spans(keys):
    """[(first, last, kind)]: the frames of a man's keys [(frame, spec)] where his hands
    hold a grip (`spec["ik"]`): every frame between two keys of the same kind, and a lone
    key's own frame."""
    out = []
    for (f0, a), (f1, b) in zip(keys, keys[1:]):
        ka = a.get("ik")
        if ka and ka == b.get("ik"):
            if out and out[-1][2] == ka and out[-1][1] >= f0:
                out[-1] = (out[-1][0], f1, ka)
            else:
                out.append((f0, f1, ka))
    covered = {f for f0, f1, _k in out for f in range(f0, f1 + 1)}
    out += [(f, f, s["ik"]) for f, s in keys if s.get("ik") and f not in covered]
    return sorted(out)


def clips():
    """{clip: {prefix: [(frame, spec)]}} for every gun clip."""
    st = stand_keys()
    mo = motion_keys()
    death, lie = death_keys()
    return {"STAND": st, "GUN_PUSH": mo, "CHEST_CARRY": chest_keys(), "TRAIL_LIFT": lift_keys(),
            "TRAIL_WALK": turn_keys(), "DEPLOY": deploy_keys(), "ATTACK": attack_keys(), "DEATH": death,
            "DEATHLIE1": lie}


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
# The MakeHuman head with its modelled face and painted face map (`heads.refine`,
# `heads.face_map`); no face morphs, looks or hair variants (the gun has no looks).
FACES = True
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
    from . import armature as am

    arm = ch.arm
    P = ch.prefix
    from . import grips

    am.attach(obj, arm, P + "socket.hand_r", grips.hold_matrix(arm, P + "socket.hand_r", (0, -0.01, 0)))
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
    ch.body(faces=FACES)
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


def build(root, gun_arm, name="artpus", kits=None):
    """The men of `kits` ({prefix: kit}, default the cannon's four `KITS`) joined into
    `gun_arm`. Returns the Characters."""
    crew = []
    for i, (p, kit) in enumerate(sorted((kits or KITS).items())):
        ch = man(root, f"{name}.crew{i}", p, kit)
        crew.append(ch)
    for ch in crew:
        join(gun_arm, ch.arm)
        ch.arm = gun_arm
    for ch in crew:
        ch.masks()
        ch.colours(None, PALETTE)
        if FACES and ch.face_frame is not None:
            from . import heads

            heads.face_map(ch, PALETTE["skin"], tiles=1)  # no look texel: one tile
            # A plain skin colour: `acr_tint` "skin" would read a look texel the gun has not.
            for m in ch.flesh.data.materials:
                if m is not None and "acr_tint" in m:
                    del m["acr_tint"]
    return crew


def key_clips(arm, actions, plan=None, marks=None):
    """Keys the crew's specs into the gun's actions {clip: action}, then puts the hands on
    their grips frame by frame (`ik_spans`: the body stepped by `place_body`, the arms by
    `solve`) and measures every frame's misses (`measure`) as played. `plan` defaults to the
    cannon's `clips()`, `marks` ({name: (bone, local point)}) to its `gun_marks`."""
    import bpy

    from . import armature as am

    still = ("index_", "middle_", "ring_", "pinky_", "thumb_", "socket.")
    plan = plan if plan is not None else clips()
    marks = marks if marks is not None else gun_marks(arm)
    misses = {}
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
        for p, ks in plan[clip].items():
            root = arm.pose.bones[p + "root"]
            for f0, f1, kind in ik_spans(ks):
                frames = range(f0, f1 + 1)
                # The body first, from the clip's own keys: stepped in, never stretched.
                shifts = []
                for f in frames:
                    bpy.context.scene.frame_set(f)
                    shifts.append((place_body(arm, p, kind, marks), root.matrix.copy()))
                for f, (sh, m) in zip(frames, shifts):
                    if sh is not None:
                        m.translation += sh
                    root.matrix = m
                    bpy.context.view_layer.update()
                    root.keyframe_insert("location", frame=f, group=root.name)
                for f in frames:
                    bpy.context.scene.frame_set(f)
                    solve(arm, p, kind, marks, f)
        for fc in am._fcurves(act):
            if fc.data_path.startswith('pose.bones["c'):
                for kp in fc.keyframe_points:
                    kp.interpolation = "BEZIER"
        # Measured on every frame, as played (the keys interpolated).
        for p, ks in plan[clip].items():
            for f0, f1, kind in ik_spans(ks):
                for f in range(f0, f1 + 1):
                    bpy.context.scene.frame_set(f)
                    for what, d in measure(arm, p, kind, marks).items():
                        misses.setdefault(what, []).append((d, clip, p, f))
        arm.animation_data.action = None
    # The crew's clips for the joint-limit audit (`build._audit_limits`).
    arm["acr_clips"] = sorted(set(arm.get("acr_clips", [])) | set(actions))
    bpy.context.view_layer.update()
    # The exported scene pose is STAND.
    for p, ks in plan["STAND"].items():
        am.pose(arm, ks[0][1], prefix=p)
    out = {}
    for what, v in sorted(misses.items()):
        v.sort()
        worst = v[-1]
        out[what] = {"frames": len(v), "mean_m": round(sum(d for d, *_ in v) / len(v), 4),
                     "max_m": round(worst[0], 4), "worst": [worst[1], worst[2], worst[3]]}
    return {"grips": out, "marks_rest": {k: [round(x, 3) for x in loc] for k, (_b, loc) in marks.items()}}


# --- hands on the grips (bpy): body placement and two-bone IK, frame by frame -----------------

FELLOE_R = 0.66  # the felloe's middle (m from the hub): the wheel men's hands
RIM_OUT = 0.03  # the fists' centres outside the felloe's middle (fingers round it)
RIMS = {"r": 62.0, "l": 82.0}  # degrees behind (or in front of) the top of the wheel, per hand (low: leant in)
CHEST_GRIP = (0.33, 0.35)  # the top of a rope loop: (x from the chest's middle, z over its bottom)
STAFF_GRIP = -0.35  # the left hand on a level staff, metres behind the right (towards the butt)
SPIKE_GRIP = 0.22  # the left hand on the handspike's lever, behind the right
SPIKE_SEAT = 2.64  # y of the handspike's seat in the trail end's ring (m, gun frame)
SPIKE_IN = 0.10  # the handspike's head this far past its seat (under the trail's ring)
SPIKE_ARM = 0.36  # the right fist this far from the shoulder on the handspike
ARM_SPAN = 0.85  # a goal at most this share of the arm's full reach away (never straight)
NEAR = 0.24  # and at least this far from the shoulder (an arm folded no further)


def gun_marks(arm):
    """Points on the gun in the frame of the bone that carries them (bone, local point):
    the vent on the breech, the seat of the handspike on the trail."""
    from mathutils import Vector

    import bpy

    out = {}
    # The gun's bones at rest (the last clip keyed may have left the carriage scaled away).
    for b in ("gun", "carriage", "intact", "wreck", "barrel", "wheel_l", "wheel_r"):
        pb = arm.pose.bones.get(b)
        if pb is not None:
            pb.location, pb.scale = (0, 0, 0), (1, 1, 1)
            pb.rotation_mode = "QUATERNION"
            pb.rotation_quaternion = (1, 0, 0, 0)
    bpy.context.view_layer.update()
    inv = arm.matrix_world.inverted()
    for o in arm.children:
        if o.type != "MESH" or o.parent_type != "BONE" or o.parent_bone not in ("barrel", "intact"):
            continue
        vs_ = [inv @ o.matrix_world @ v.co for v in o.data.vertices]
        if o.parent_bone == "barrel":
            ymax = max(v.y for v in vs_)
            y = ymax - 0.24
            top = [v for v in vs_ if abs(v.x) < 0.04 and abs(v.y - y) < 0.05] or vs_
            pt = Vector((0.0, y, max(v.z for v in top) + 0.01))
        else:
            # The trail's top at the seat, between the nearest vertices before and behind it
            # (none lie within a few cm of it: the old window fell back to the whole
            # carriage's top, 0.7 m over the trail, and the handspike was seated in the air).
            trail = [v for v in vs_ if abs(v.x) < 0.3 and abs(v.y - SPIKE_SEAT) < 0.6]
            pt = Vector((0.0, SPIKE_SEAT, _top_at(trail, SPIKE_SEAT) - 0.02))
        pb = arm.pose.bones[o.parent_bone]
        out[o.parent_bone] = (o.parent_bone, pb.matrix.inverted() @ pt)
    return out


def _top_at(vs, y):
    """The top (z) of the vertices `vs` at `y`: their highest at the nearest y on either side,
    interpolated."""
    before = [v for v in vs if v.y <= y]
    after = [v for v in vs if v.y >= y]
    if not before or not after:
        return max(v.z for v in (before or after))
    yb, ya = max(v.y for v in before), min(v.y for v in after)
    zb = max(v.z for v in before if abs(v.y - yb) < 1e-3)
    za = max(v.z for v in after if abs(v.y - ya) < 1e-3)
    return zb if ya - yb < 1e-6 else zb + (za - zb) * (y - yb) / (ya - yb)


def _mark(arm, marks, name):
    bone, local = marks[name]
    return arm.pose.bones[bone].matrix @ local


def _ik(s, t, l1, l2, pole):
    """Elbow of a two-bone chain from `s` to `t` bent towards `pole` (a direction)."""
    v = t - s
    d = max(1e-4, min(v.length, (l1 + l2) * 0.995))
    u = v.normalized()
    a = (l1 * l1 - l2 * l2 + d * d) / (2 * d)
    h = math.sqrt(max(l1 * l1 - a * a, 0.0))
    p = (pole - u * pole.dot(u)).normalized()
    return s + u * a + p * h


def _aim(pb, direction):
    """Turns pose bone `pb` (armature space) so its Y axis points along `direction`."""
    import bpy

    m = pb.matrix.copy()
    q = m.to_3x3().col[1].normalized().rotation_difference(direction.normalized()).to_matrix()
    n = (q @ m.to_3x3()).to_4x4()
    n.translation = m.translation
    pb.matrix = n
    bpy.context.view_layer.update()


def _orient(pb, rot3):
    import bpy

    n = rot3.to_4x4()
    n.translation = pb.matrix.translation.copy()
    pb.matrix = n
    bpy.context.view_layer.update()


def _fist(arm, P, sd):
    """The centre of the fist `sd` (its grip socket, `grips.setup`)."""
    return arm.pose.bones[P + "socket.hand_" + sd].head.copy()


def _arm_reach(arm, P, sd):
    """(shoulder, full reach from it to the fist's centre) of arm `sd`."""
    B = arm.pose.bones
    up, fo, ha = B[P + "upperarm_" + sd], B[P + "forearm_" + sd], B[P + "hand_" + sd]
    full = (fo.head - up.head).length + (ha.head - fo.head).length + (_fist(arm, P, sd) - ha.head).length
    return up.head.copy(), full


def _reach(arm, P, sd, goal, frame):
    """Puts the fist `sd` of man `P` round `goal` (armature space): two-bone IK of the
    shoulder to the wrist, the wrist placed so the fist's centre (the hand keeping its
    orientation) lands on the goal; the elbow out and down. Keys the arm at `frame`; returns
    the miss (m)."""
    from mathutils import Vector

    B = arm.pose.bones
    up, fo, ha = B[P + "upperarm_" + sd], B[P + "forearm_" + sd], B[P + "hand_" + sd]
    keep = ha.matrix.to_3x3().copy()
    s = up.head.copy()
    out = (B[P + "upperarm_" + sd].head - B[P + "upperarm_" + ("r" if sd == "l" else "l")].head).normalized()
    l1, l2 = (fo.head - up.head).length, (ha.head - fo.head).length
    for _ in range(2):
        off = _fist(arm, P, sd) - ha.head  # the fist from the wrist, in the kept orientation
        t = goal - off
        e = _ik(s, t, l1, l2, out * 0.6 + Vector((0, 0, -1)))
        _aim(up, e - s)
        _aim(fo, t - fo.head)
        _orient(ha, keep)
    from . import armature

    armature.settle_arm(arm, sd, P)  # the elbow a hinge, the wrist's twist in the forearm
    for pb in (up, fo, ha):
        pb.keyframe_insert("rotation_quaternion", frame=frame, group=pb.name)
    return (_fist(arm, P, sd) - goal).length


def _tool_axis(sock):
    """The held tool's axis (armature space): +Z of the item frame in the fist
    (`grips.item_in_socket`, as `grips.hold_matrix` hung it), else the socket's
    rest-relative turn of +Z."""
    from mathutils import Vector

    from . import grips

    arm = sock.id_data
    if grips.is_grip(arm, sock.name):
        return (sock.matrix.to_3x3() @ grips.item_in_socket(arm, sock.name) @ Vector((0, 0, 1))).normalized()
    return (sock.matrix.to_3x3() @ sock.bone.matrix_local.to_3x3().inverted() @ Vector((0, 0, 1))).normalized()


def _point_tool(sock, at, frame):
    """Turns the socket so the held tool points from the hand at `at`; keys it."""
    from mathutils import Vector

    a = _tool_axis(sock)
    q = a.rotation_difference((Vector(at) - sock.head).normalized()).to_matrix()
    _orient(sock, q @ sock.matrix.to_3x3())
    sock.keyframe_insert("rotation_quaternion", frame=frame, group=sock.name)


def _off_grip(arm, P, sock, d):
    """The left hand's place on the right hand's tool: `d` m along its axis from the right
    fist, on the side towards the left shoulder (the butt end of a level staff, the near
    end of the handspike's lever)."""
    s = arm.pose.bones[P + "upperarm_l"].head
    a = _tool_axis(sock) * abs(d)
    return min((sock.head + a, sock.head - a), key=lambda g: (g - s).length)


def goals(arm, P, kind, marks):
    """{hand: goal} where the fists' centres belong for task `kind` at the current frame
    (armature space): "wheel" both on the felloe of his wheel (behind or in front of it),
    "chest" the hand on the chest's side on the top of its rope loop, "spike" the right fist
    on the handspike with its head under the trail's ring (`SPIKE_IN` past the seat),
    "staff" the left on the level staff, "muzzle" the right on the rim of a mortar's muzzle
    (marks `muzzle_l`/`muzzle_r`, the side nearer the man), any other kind with marks of
    its name the nearest of them by the nearer hand ("handle", "rear": the mortar's lifting
    handles). The left hand on the handspike
    follows the right (`solve`)."""
    from mathutils import Vector

    B = arm.pose.bones
    if kind == "wheel":
        side = "wheel_r" if B[P + "hips"].head.x > 0 else "wheel_l"
        hub = B[side].head
        sgn = 1.0 if hub.x > 0 else -1.0
        back = 1.0 if B[P + "hips"].head.y > hub.y else -1.0
        out = {}
        for sd, deg in RIMS.items():
            a = math.radians(deg)
            out[sd] = Vector((hub.x + sgn * RIM_OUT, hub.y + back * FELLOE_R * math.sin(a),
                              hub.z + FELLOE_R * math.cos(a)))
        return out
    if kind == "chest" and "chest" in B:
        # The rope loop at his end of the chest, in the hand nearer it. (Picking the loop by
        # the chest bone's local X put each carrier's hand on the far loop, so the body was
        # stepped across into the chest and into the other carrier: m2-cannon-crew.)
        chest = B["chest"]
        hx, hz = CHEST_GRIP
        hips = B[P + "hips"].head
        loop = min((chest.matrix @ Vector((x, hz, 0.0)) for x in (hx, -hx)), key=lambda g: (g - hips).length)
        sd = min("lr", key=lambda d: (B[P + "upperarm_" + d].head - loop).length)
        return {sd: loop}
    if kind == "spike" and "intact" in marks:
        seat = _mark(arm, marks, "intact")
        u = (B[P + "upperarm_r"].head - seat).normalized()
        return {"r": seat + u * (HANDSPIKE[1] - SPIKE_IN)}
    if kind == "staff":
        sock = B[P + "socket.hand_r"]
        return {"l": _off_grip(arm, P, sock, STAFF_GRIP)}
    if kind == "muzzle" and "muzzle_l" in marks:
        hips = B[P + "hips"].head
        rims = [_mark(arm, marks, m) for m in ("muzzle_l", "muzzle_r")]
        return {"r": min(rims, key=lambda g: (g - hips).length)}
    # A handle on the piece (the mortar's lifting handles, marks `<kind>`, `<kind>_l`,
    # `<kind>_r`): the one nearest the man, taken by the hand whose shoulder is nearer.
    near = [_mark(arm, marks, m) for m in (kind, kind + "_l", kind + "_r") if m in marks]
    if near and kind not in ("linstock", "spike", "staff", "wheel", "chest"):
        hips = B[P + "hips"].head
        g = min(near, key=lambda g: (g - hips).length)
        sd = min("lr", key=lambda s: (B[P + "upperarm_" + s].head - g).length)
        return {sd: g}
    return {}


def place_body(arm, P, kind, marks):
    """The step (armature-space vector, horizontal) that brings every goal of `kind` within
    `ARM_SPAN` of its shoulder and no nearer than `NEAR` (for the handspike: the right shoulder
    `SPIKE_ARM` behind the right fist on the seated spike), or None."""
    from mathutils import Vector

    if kind == "spike" and "intact" in marks:
        seat = _mark(arm, marks, "intact")
        s, _full = _arm_reach(arm, P, "r")
        want = HANDSPIKE[1] - SPIKE_IN + SPIKE_ARM
        dz = s.z - seat.z
        flat = Vector((s.x - seat.x, s.y - seat.y, 0.0))
        if flat.length < 1e-6:
            return None
        need = math.sqrt(max(want * want - dz * dz, 0.04))
        return flat.normalized() * (need - flat.length)
    if kind in ("linstock", "staff"):
        return None
    shift = None
    for sd, g in goals(arm, P, kind, marks).items():
        s, full = _arm_reach(arm, P, sd)
        v = g - s
        flat = Vector((v.x, v.y, 0.0))
        if flat.length < 1e-6:
            continue
        far = full * ARM_SPAN
        if v.length > far:
            # Step in until the goal is `far` away (the height difference stays).
            need = math.sqrt(max(far * far - v.z * v.z, 0.0))
        elif v.length < NEAR:
            need = math.sqrt(max(NEAR * NEAR - v.z * v.z, 0.0))
        else:
            continue
        step = flat.normalized() * (flat.length - need)
        if shift is None or step.length > shift.length:
            shift = step
    return shift


def solve(arm, P, kind, marks, frame):
    """Hand IK of man `P` at `frame` for the task `kind` (the current pose is the frame's):
    the fists onto `goals`; "spike" then points the handspike into its seat and puts the
    left fist down it; "linstock" the match on the vent. Keys what it moves."""
    B = arm.pose.bones
    sock = B[P + "socket.hand_r"]
    held = sock.matrix.to_3x3().copy()
    if kind == "linstock" and "barrel" in marks:
        vent = _mark(arm, marks, "barrel")
        tip = LINSTOCK[1] + 0.07
        for _ in range(6):
            want = vent - (vent - sock.head).normalized() * tip
            _reach(arm, P, "r", want, frame)
            _point_tool(sock, vent, frame)
        return
    for sd, g in goals(arm, P, kind, marks).items():
        _reach(arm, P, sd, g, frame)
    if kind == "spike" and "intact" in marks:
        _point_tool(sock, _mark(arm, marks, "intact"), frame)
        _reach(arm, P, "l", _off_grip(arm, P, sock, SPIKE_GRIP), frame)
        return
    if kind in ("wheel", "chest"):
        _orient(sock, held)  # a staff or linstock in the other hand stays upright
        sock.keyframe_insert("rotation_quaternion", frame=frame, group=sock.name)


def measure(arm, P, kind, marks):
    """{what: miss (m)} of man `P` at the current frame: each fist from its grip
    ("wheel_l", "chest_r", "staff_l", ...), "spike_l" the left fist from its place on the
    handspike, "spike_seat" the handspike's head from its place under the seat,
    "linstock" the match from the vent."""
    B = arm.pose.bones
    sock = B[P + "socket.hand_r"]
    if kind == "linstock" and "barrel" in marks:
        tip = sock.head + _tool_axis(sock) * (LINSTOCK[1] + 0.07)
        return {"linstock": (tip - _mark(arm, marks, "barrel")).length}
    if kind == "spike" and "intact" in marks:
        seat = _mark(arm, marks, "intact")
        axis = _tool_axis(sock)
        head = sock.head + axis * (HANDSPIKE[1] - SPIKE_IN)
        return {"spike_seat": (head - seat).length,
                "spike_l": (_fist(arm, P, "l") - _off_grip(arm, P, sock, SPIKE_GRIP)).length}
    return {f"{kind}_{sd}": (_fist(arm, P, sd) - g).length for sd, g in goals(arm, P, kind, marks).items()}
