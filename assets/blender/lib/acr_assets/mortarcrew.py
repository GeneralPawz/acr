"""The crew of the mortar `ArtMrt` (#301): three artillerymen on skeleton H in the mortar's
armature (bones `c0.` .. `c2.`), with the gun crew's poses, placement, stepping and hand IK
(`acr_assets.guncrew`).

The mortar faces -Y on its bed (no wheels; bed x = ±0.64, y = -0.89 .. 0.76, the barrel
fixed at 60°, its muzzle about 1.2 m up at y ≈ -0.45). The sim draws the mortar with the
field gun's `.MD` (`ArtPus`), so the clips are the cannon's, filled with a mortar's work:

  c0 loader (left): the powder charge from his bag into the muzzle
  c1 loader (right): the shell from his bag, lowered in with c0
  c2 firer: the linstock with its slow match; primes the vent

`ATTACK` (420 frames, the shot at frame 29 like the cannon's): c2 touches the linstock to
the vent (24..29), the flash and the bed's jolt (artmrt.py), the loaders stand clear
(26..44); c0 charges the bore (70..120), both lower the shell in (140..190), c2 primes
(250..275). `GUN_PUSH` (moving): c0/c1 haul the bed by its front rings, c2 follows;
`CHEST_CARRY` (taking up), `DEPLOY` (setting down) walk between that and the stations;
`TRAIL_LIFT`/`TRAIL_WALK` (turning): c0/c1 heave the bed round at its rear corners.

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
# Hauling the bed by its front rings (walking towards -Y in front of it).
HAUL = {"c0.": (-0.62, -1.25, 0), "c1.": (0.62, -1.25, 0), "c2.": (0.35, 1.25, 0)}
# Heaving the bed round at its rear corners (facing it from behind).
HEAVE = {"c0.": (-0.62, 1.15, 0), "c1.": (0.62, 1.15, 0), "c2.": (1.25, 0.6, -90)}
# Loading: a step nearer the muzzle.
LOAD = {"c0.": (-0.85, -0.5, 90), "c1.": (0.85, -0.5, -90)}

HAULING = dict(gc.STAND, spine=(-14, 0, 0), spine1=(-4, 0, 0), head=(10, 0, 0),
               upperarm_l=(20, 16, 0), upperarm_r=(20, -16, 0), forearm_l=(-10, 0, 0), forearm_r=(-10, 0, 0))
STOOPED = {k: v for k, v in gc.STOOP.items() if k != "ik"}
BRACED = dict(gc.HEAVE)
BRACED.pop("ik", None)
# The right hand on the muzzle's rim (charge or shell going in).
INTO = dict(gc.REACH, ik="muzzle")


def _held(p, spec):
    return gc.tool(spec, gc.UP) if p == "c2." else spec


def stand_keys():
    return {p: [(0, gc.place(HELD[p], HOME[p][:2], HOME[p][2]))] for p in PREFIXES}


def motion_keys(n=36):
    """GUN_PUSH: c0/c1 haul the bed, c2 walks behind it."""
    out = {}
    for p in PREFIXES:
        x, y, yaw = HAUL[p]
        pose = _held(p, HAULING if p != "c2." else gc.STAND)
        out[p] = [(f, gc.place(s, (x, y), yaw)) for f, s in gc._walk(pose, n)]
    return out


def _between(a, b, n, hold=4):
    """Each man from stations `a` (with the stand pose) to the first key of `b`'s walk."""
    k = {}
    for p in PREFIXES:
        (xa, ya, ra), (xb, yb, rb) = a[p], b[p]
        pa = HELD[p]
        pb = _held(p, HAULING if p != "c2." else gc.STAND)
        k[p] = gc.stepped([(0, gc.place(pa, (xa, ya), ra)), (hold, gc.place(pa, (xa, ya), ra)),
                           (n, gc.place(gc._walk(pb)[0][1], (xb, yb), rb))], min_move=0.3)
    return k


def chest_keys(n=41):
    """CHEST_CARRY (`#PATTACK0`): from the stations to the hauling places."""
    return _between(HOME, HAUL, n)


def deploy_keys(n=170):
    """DEPLOY (`#PATTACK4/5`): from the hauling places back to the stations."""
    k = {}
    for p in PREFIXES:
        (xa, ya, ra), (xb, yb, rb) = HAUL[p], HOME[p]
        pa = _held(p, HAULING if p != "c2." else gc.STAND)
        k[p] = gc.stepped([(0, gc.place(pa, (xa, ya), ra)), (20, gc.place(pa, (xa, ya), ra)),
                           (70, gc.place(HELD[p], (xb, yb), rb)), (n, gc.place(HELD[p], (xb, yb), rb))])
    return k


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
