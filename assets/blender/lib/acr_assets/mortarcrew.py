"""The crew of the mortar `ArtMrt` (#301): three artillerymen on skeleton H in the mortar's
armature (bones `c0.` .. `c2.`), with the gun crew's poses, placement, stepping and hand IK
(`acr_assets.guncrew`).

The mortar faces -Y on its bed (no wheels; bed x = ±0.64, y = -0.89 .. 0.76, the barrel
fixed at 60°, its muzzle about 1.2 m up at y ≈ -0.45). The sim draws the mortar with the
field gun's `.MD` (`ArtPus`), so the clips are the cannon's, filled with a mortar's work:

  c0 loader (left): the powder charge from his bag into the muzzle
  c1 loader (right): the shell from his bag, lowered in with c0
  c2 firer: the linstock with its slow match; primes the vent

Moving (`GUN_PUSH`; `CHEST_CARRY` takes it up, `DEPLOY` sets it down): c0/c1 carry the bed
by the iron handles on its sides, c2 by the one at its rear (see `CARRY_AT`).

`ATTACK` (420 frames, the shot at frame 29 like the cannon's): c2 touches the linstock to
the vent (24..29), the flash and the bed's jolt (artmrt.py), the loaders stand clear
(26..44); c0 charges the bore (70..120), both lower the shell in (140..190), c2 primes
(250..275). `TRAIL_LIFT`/`TRAIL_WALK` (turning): c0/c1 heave the bed round at its rear corners.

Pure data here (no bpy).
"""

from . import guncrew as gc
from . import poses

PREFIXES = ("c0.", "c1.", "c2.")
KITS = {"c0.": "bag", "c1.": "bag", "c2.": "linstock"}
FIRE = gc.FIRE

# Stations (x, y, yaw) by the mortar.
HOME = {
    "c0.": (-0.95, -0.45, 90),
    "c1.": (0.95, -0.45, -90),
    "c2.": (0.95, 0.55, -90),
}
HELD = {"c0.": gc.STAND, "c1.": gc.STAND, "c2.": gc.tool(gc.STAND, gc.UP)}
# Moving (history: a mortar had no wheels; on the field men shifted it by its lifting handles
# or with handspikes through the bed, light Coehorn mortars carried by 2-4 men): c0/c1 carry
# the bed by the iron handles on its side cheeks (`HANDLE_Y`), facing the way it goes (-Y),
# c2 behind it by the handle on its rear transom, the linstock in his other hand. The bed
# rides `LIFT` off the ground. The handles are added to the source bed by artmrt.py.
LIFT = 0.15  # m the bed is carried off the ground
HANDLE_Y = 0.0  # the side handles' middle (bed frame, m)
HANDLE_OUT = 0.07  # the handles' grip bars outside the bed's side and rear
GRIP_Z = 0.66  # the grip bars over the ground with the bed set down
GRIP_LEN = 0.2
CARRY_AT = {"c0.": (-0.98, HANDLE_Y, 0), "c1.": (0.98, HANDLE_Y, 0), "c2.": (-0.2, 1.15, 0)}
CARRY_IK = {"c0.": "handle", "c1.": "handle", "c2.": "rear"}
# Under the load: the trunk a little forward, the head up to see the way, the free arm
# hanging; the knees bent (`_bend`). Positive spine X leans the trunk forward (the gun crew's
# HAULING, PUSH and REACH lean back with their negative spine: the "pushing" look).
CARRYING = dict(gc.STAND, spine=(9, 0, 0), spine1=(4, 0, 0), head=(-7, 0, 0),
                upperarm_l=(4, 12, 0), forearm_l=(-8, 0, 0), upperarm_r=(4, -12, 0), forearm_r=(-8, 0, 0))
# Stooped to the handles to lift or set down the bed: knees deep, back straight and inclined.
SETTING = dict(CARRYING, spine=(26, 0, 0), spine1=(8, 0, 0), head=(-16, 0, 0),
               thigh_l=(-48, -3, 0), shin_l=(70, 0, 0), foot_l=(-22, 0, 0),
               thigh_r=(-48, 3, 0), shin_r=(70, 0, 0), foot_r=(-22, 0, 0), root_loc=(0, 0.12, -0.17))
CARRY_STRIDES = 4  # short steps: four cycles over GUN_PUSH's 36 frames
LEG_SCALE = 0.7  # of the walk cycle's leg swing
KNEE = {"thigh": -20.0, "shin": 32.0, "foot": -12.0}  # the bend added to every walking key
KNEE_DROP = 0.045  # m the hips sink with it
TAKE_UP = (26, 36)  # CHEST_CARRY: stooped to the handles at 26, the bed up by 36
SET_DOWN = (10, 26)  # DEPLOY: carried until 10, set down by 26, hands off at 32

# Heaving the bed round at its rear corners (facing it from behind).
HEAVE = {"c0.": (-0.62, 1.15, 0), "c1.": (0.62, 1.15, 0), "c2.": (1.25, 0.6, -90)}
# Loading: a step nearer the muzzle.
LOAD = {"c0.": (-0.85, -0.5, 90), "c1.": (0.85, -0.5, -90)}

STOOPED = {k: v for k, v in gc.STOOP.items() if k != "ik"}
BRACED = dict(gc.HEAVE)
BRACED.pop("ik", None)
# The right hand on the muzzle's rim (charge or shell going in).
INTO = dict(gc.REACH, ik="muzzle")


def _held(p, spec):
    return gc.tool(spec, gc.UP) if p == "c2." else spec


def stand_keys():
    return {p: [(0, gc.place(HELD[p], HOME[p][:2], HOME[p][2]))] for p in PREFIXES}


def _bend(spec):
    """A walking key made a carrier's: shorter leg swing, knees bent, hips lower."""
    d = dict(spec)
    for k in gc.LEG_BONES:
        x, y, z = d.get(k, (0, 0, 0))
        d[k] = (x * LEG_SCALE + KNEE[k.split("_")[0]], y, z)
    lx, ly, lz = d.get("root_loc", (0, 0, 0))
    d["root_loc"] = (lx, ly, lz * LEG_SCALE - KNEE_DROP)
    return d


def _carry_pose(p):
    pose = dict(CARRYING, ik=CARRY_IK[p])
    if p == "c2.":  # the linstock upright in his right hand, as he stands with it
        pose.update({b: gc.STAND[b] for b in ("upperarm_r", "forearm_r", "hand_r")})
        return gc.tool(pose, gc.UP)
    return pose


def _carry_walk(p, n=36):
    return [(f, _bend(s)) for f, s in gc._walk(_carry_pose(p), n, CARRY_STRIDES)]


def _setting(p):
    pose = dict(SETTING, ik=CARRY_IK[p])
    if p == "c2.":
        pose.update({b: gc.STAND[b] for b in ("upperarm_r", "forearm_r", "hand_r")})
        return gc.tool(pose, gc.UP)
    return pose


def motion_keys(n=36):
    """GUN_PUSH: the bed carried `LIFT` up by its handles, short steps, knees bent."""
    out = {}
    for p in PREFIXES:
        x, y, yaw = CARRY_AT[p]
        out[p] = [(f, gc.place(s, (x, y), yaw)) for f, s in _carry_walk(p, n)]
    return out


def chest_keys(n=41):
    """CHEST_CARRY (`#PATTACK0`, taking up): from the stations to the handles, stoop, lift the
    bed (`gun_lift`) and step off into GUN_PUSH's first key."""
    s0, s1 = TAKE_UP
    k = {}
    for p in PREFIXES:
        (xa, ya, ra), (xb, yb, rb) = HOME[p], CARRY_AT[p]
        stand = dict(_carry_pose(p), **{b: v for b, v in gc.STAND.items() if b.startswith(("spine", "head"))})
        stand.pop("ik")
        k[p] = gc.stepped([(0, gc.place(HELD[p], (xa, ya), ra)), (4, gc.place(HELD[p], (xa, ya), ra)),
                           (s0 - 6, gc.place(stand, (xb, yb), rb)), (s0, gc.place(_setting(p), (xb, yb), rb)),
                           (s1, gc.place(_bend(_carry_pose(p)), (xb, yb), rb)),
                           (n, gc.place(_carry_walk(p)[0][1], (xb, yb), rb))], min_move=0.3)
    return k


def deploy_keys(n=170):
    """DEPLOY (`#PATTACK4/5`, arriving): the bed carried in, a plain stoop to set it down
    (`gun_lift`), the hands off, upright, then back to the stations."""
    s0, s1 = SET_DOWN
    k = {}
    for p in PREFIXES:
        (xa, ya, ra), (xb, yb, rb) = CARRY_AT[p], HOME[p]
        up = dict(_carry_pose(p), **{b: v for b, v in gc.STAND.items() if b.startswith(("spine", "head"))})
        up.pop("ik")
        off = dict(_setting(p))
        off.pop("ik")
        k[p] = gc.stepped([(0, gc.place(_carry_walk(p)[0][1], (xa, ya), ra)),
                           (s0, gc.place(_bend(_carry_pose(p)), (xa, ya), ra)),
                           (s1, gc.place(_setting(p), (xa, ya), ra)), (s1 + 6, gc.place(off, (xa, ya), ra)),
                           (s1 + 18, gc.place(up, (xa, ya), ra)),
                           (80, gc.place(HELD[p], (xb, yb), rb)), (n, gc.place(HELD[p], (xb, yb), rb))],
                          min_move=0.3)
    return k


def gun_lift(push=36, chest=41, deploy=170):
    """{clip: [(frame, z)]}: the bed's height off the ground (artmrt.py's `gun` bone) in the
    clips that carry it."""
    return {"GUN_PUSH": [(0, LIFT), (push, LIFT)],
            "CHEST_CARRY": [(0, 0.0), (TAKE_UP[0], 0.0), (TAKE_UP[1], LIFT), (chest, LIFT)],
            "DEPLOY": [(0, LIFT), (SET_DOWN[0], LIFT), (SET_DOWN[1], 0.0), (deploy, 0.0)]}


def lift_keys(n=70):
    """TRAIL_LIFT (`#PATTACK1`): c0/c1 to the bed's rear corners and stoop to it."""
    k = {}
    for p in PREFIXES:
        x, y, yaw = HOME[p]
        tx, ty, tyaw = HEAVE[p]
        ks = [(0, gc.place(HELD[p], (x, y), yaw)), (6, gc.place(HELD[p], (x, y), yaw)),
              (44, gc.place(HELD[p], (tx, ty), tyaw))]
        if p != "c2.":
            ks += [(58, gc.place(STOOPED, (tx, ty), tyaw)), (n, gc.place(BRACED, (tx, ty), tyaw))]
        else:
            ks += [(n, gc.place(HELD[p], (tx, ty), tyaw))]
        k[p] = gc.stepped(ks)
    return k


def turn_keys(n=40):
    """TRAIL_WALK (`#MOTION_L1`): c0/c1 heave the bed round, c2 stands by. First and last
    frames are alike."""
    out = {}
    for p in ("c0.", "c1."):
        x, y, yaw = HEAVE[p]
        out[p] = [(f, gc.place(BRACED if (f // 10) % 2 == 0 else STOOPED, (x, y), yaw)) for f in range(0, n + 1, 10)]
    x, y, yaw = HEAVE["c2."]
    out["c2."] = [(0, gc.place(HELD["c2."], (x, y), yaw)), (n, gc.place(HELD["c2."], (x, y), yaw))]
    return out


def attack_keys():
    """The drill on the 420 frames of ATTACK."""
    k = {p: [] for p in PREFIXES}

    def at(p, f, pose, st):
        k[p].append((f, gc.place(pose, st[:2], st[2])))

    for p, sd in (("c0.", -1), ("c1.", 1)):
        home, load = HOME[p], LOAD[p]
        clear = (sd * 1.35, -0.2, home[2])
        at(p, 0, gc.STAND, home)
        at(p, 20, gc.STAND, home)
        at(p, 26, gc.CLEAR, clear)
        at(p, 44, gc.CLEAR, clear)
        at(p, 58, gc.STAND, home)
        if p == "c0.":
            # The powder charge: from the bag into the bore.
            at(p, 70, gc.BAG, load)
            at(p, 84, INTO, load)
            at(p, 118, INTO, load)
            at(p, 128, gc.STAND, load)
        else:
            at(p, 120, gc.STAND, home)
            at(p, 130, gc.BAG, load)
        # The shell lowered in by both.
        at(p, 142 if p == "c0." else 140, gc.BAG, load)
        at(p, 152, INTO, load)
        at(p, 188, INTO, load)
        at(p, 198, gc.STAND, load)
        at(p, 214, gc.STAND, home)
        at(p, 419, gc.STAND, home)
    h = HOME["c2."]
    lin = gc.tool
    at("c2.", 0, lin(gc.STAND, gc.UP), h)
    at("c2.", 18, lin(gc.STAND, gc.UP), h)
    at("c2.", 25, lin(gc.TOUCH, gc.VENT), (0.88, 0.55, -90))
    at("c2.", FIRE, lin(gc.TOUCH, gc.VENT), (0.88, 0.55, -90))
    at("c2.", FIRE + 4, lin(gc.FLINCH, gc.UP), (1.15, 0.65, -90))
    at("c2.", 50, lin(gc.STAND, gc.UP), h)
    at("c2.", 240, lin(gc.STAND, gc.UP), h)
    at("c2.", 252, lin(gc.PRIMING, gc.LEVEL), (0.7, 0.5, -90))
    at("c2.", 262, lin(dict(gc.PRIMING, forearm_r=(-55, 0, 0)), gc.LEVEL), (0.7, 0.5, -90))
    at("c2.", 272, lin(gc.PRIMING, gc.LEVEL), (0.7, 0.5, -90))
    at("c2.", 284, lin(gc.STAND, gc.UP), h)
    at("c2.", 419, lin(gc.STAND, gc.UP), h)
    return {p: gc.stepped(ks) for p, ks in k.items()}


def death_keys(n=73):
    """All three fall where they stand; lie in DEATHLIE1."""
    lie = poses.lie_prone(arms_out=True)
    out, lies = {}, {}
    for i, p in enumerate(PREFIXES):
        x, y, yaw = HOME[p]
        base = HELD[p]
        stand = dict(base, world=dict(base.get("world", {})))
        ks = poses.death_keys(stand, lie)
        delay = 3 * i
        out[p] = [(min(n - 1, delay + round(t * (n - 1 - 6))), gc.place(s, (x, y), yaw + 30 * (i - 1))) for t, s in ks]
        out[p].append((n - 1, out[p][-1][1]))
        lies[p] = [(0, out[p][-1][1])]
    return out, lies


def clips():
    """{clip: {prefix: [(frame, spec)]}} for every clip the field gun's `.MD` plays."""
    death, lie = death_keys()
    return {"STAND": stand_keys(), "GUN_PUSH": motion_keys(), "CHEST_CARRY": chest_keys(), "TRAIL_LIFT": lift_keys(),
            "TRAIL_WALK": turn_keys(), "DEPLOY": deploy_keys(), "ATTACK": attack_keys(), "DEATH": death,
            "DEATHLIE1": lie}
