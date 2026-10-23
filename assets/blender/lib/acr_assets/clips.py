"""Reusable keyframed clips for skeleton H by weapon or action (pure data, no Blender; the
Blender side is `apply`): bow, thrown tomahawk or spear, sling, blowpipe, heal, drum, colour,
the one-handed sword, the club or tomahawk hack and the spear thrust on foot
(docs/assets-humans.md, "Clips"; docs/humans-roster.md, batch 0).

Pose specs are `poses.py`'s ({bone: (rx, ry, rz)}, "root", "root_loc", "world") plus three
keys that `apply` resolves before keying:
- "reach": {"r"|"l": (point, pole)}: the hand's grip (right: the head of `socket.hand_r`,
  left: the palm) goes to `point` (armature space, metres; -Y is the facing), the elbow
  bending towards `pole` (an offset from the shoulder); two-bone IK (`aim._solve`). The
  points are computed here from the held item's geometry (`held.py`), so the stick's tip
  lies on the drum head, the arrow's nock on the string and the blade's point where the
  blow lands.
- "show": {socket: 0 | 1}: the socket's scale (a thrown tomahawk, a loosed arrow or a cast
  dart is gone from the hand until the next is drawn). Keyed as a one-frame step.
- "world": the held item's orientation as in `poses.py`: (0, 0, 0) is the item upright (its
  +Z up, its edge or face -Y); (90, 0, 0) points it forward.

Clip names are the `.MD` animation names (`ATTACK`, `ATTACK1`, `ATTACK2`, `PSTAND1`,
`REST`, `MOTION_L`...) and the strike clips of docs/melee.md (`ATTACK_<KIND>_<HEIGHT>`).
Timing follows the sim: a missile leaves (and a heal lands) at the active frame of
`SETACTIVEPOINT #ATTACKn f`, rounded up to even (`release_frame`, `combat.rs` (A)); a melee
blow lands on `poses.STRIKE_HIT_FRAME` (frame 2). Every builder returns `Clip`s whose `event`
is that frame.
"""

import math

from . import poses, rig

STRIKE_HIT_FRAME = poses.STRIKE_HIT_FRAME
_with = poses._with

# Item lengths from `held.py` (grip to the point that matters).
ARROW = 0.72  # nock (the drawing hand) to the head
STICK = 0.42 * 2 / 3  # drumstick grip to tip
BOW_NOCK = 0.105  # bow grip to the string at brace (`longbow` socket.nock)
SPEAR_TIP = 1.42  # spear grip to its point
SWORD_TIP = 0.86  # smallsword / hanger grip to its point (about)
CLUB_TIP = 0.52  # war club / tomahawk grip to the striking face
BLOWPIPE = 1.80  # blowpipe length, mouthpiece at the grip


def release_frame(active):
    """The frame a missile leaves on: `SETACTIVEPOINT`'s frame, at least 1, rounded up to
    even (the engine steps two frames a tick)."""
    f = max(int(active), 1)
    return f + (f & 1)


class Clip(list):
    """[(frame, pose)] with `event`: the frame of the hit, release or heal (None: none)."""

    def __init__(self, keys, event=None):
        super().__init__(keys)
        self.event = event


def frames(n, keys, event=None):
    """A `Clip` of `n` frames from (frame, pose) keys: frames past the end are clamped, the
    last of two on one frame wins (like `rig.clip_keys`)."""
    last = max(n - 1, 0)
    out = []
    for f, pose in sorted(keys, key=lambda k: k[0]):
        f = min(max(int(f), 0), last)
        if out and out[-1][0] == f:
            out[-1] = (f, pose)
        else:
            out.append((f, pose))
    return Clip(out, event)


# -- small vector maths (armature space, `armature.euler` order Rz @ Ry @ Rx) -----------

def rot(r, v):
    """`v` turned by the "world" rotation `r` (degrees)."""
    x, y, z = v
    a, b, c = (math.radians(t) for t in r)
    y, z = y * math.cos(a) - z * math.sin(a), y * math.sin(a) + z * math.cos(a)
    x, z = x * math.cos(b) + z * math.sin(b), -x * math.sin(b) + z * math.cos(b)
    x, y = x * math.cos(c) - y * math.sin(c), x * math.sin(c) + y * math.cos(c)
    return (x, y, z)


def axis(r):
    """The held item's long axis (+Z) under "world" rotation `r`."""
    return rot(r, (0.0, 0.0, 1.0))


def add(a, b, s=1.0):
    return tuple(p + s * q for p, q in zip(a, b))


def grip_for(tip, r, length):
    """The grip that puts an item's point at `tip`: `length` along its axis, or, a tuple,
    the point's place in the item frame (an axe's edge lies before its handle: `held`'s
    `socket.tip`)."""
    if isinstance(length, (tuple, list)):
        return add(tip, rot(r, tuple(length)), -1.0)
    return add(tip, axis(r), -length)


def _r(point, pole=(-0.30, 0.10, -0.30)):
    return (tuple(round(c, 4) for c in point), pole)


def _l(point, pole=(0.30, 0.10, -0.30)):
    return (tuple(round(c, 4) for c in point), pole)


# -- stands ---------------------------------------------------------------------------

# Arms relaxed at the sides, empty hands (the A-pose brought in).
STAND_FREE = {
    "upperarm_l": (4, 22, 0), "forearm_l": (-12, 0, 0), "upperarm_r": (4, -22, 0), "forearm_r": (-12, 0, 0),
    "thigh_l": (2, 1, 0), "thigh_r": (2, -1, 0), "head": (4, 0, 0),
}
# A weapon in the right hand at the side, upright (club, tomahawk, sword, spear at the order).
STAND_ARMED = _with(STAND_FREE, forearm_r=(-40, 0, 16), world={"socket.hand_r": (10, 0, 0)})

# -- bow (STRELA / OSTRELA): draw, aim, loose, nock the next arrow ---------------------

# Bladed stance, the left side towards the target.
BOW_STANCE = {"root": (0, 0, -70), "root_loc": (0.0, 0.0, -0.02), "thigh_l": (-8, -6, 0), "thigh_r": (6, 6, 0),
              "shin_l": (6, 0, 0), "spine": (0, 0, 8), "spine2": (0, 0, 10), "neck": (0, 0, 30), "head": (6, 0, 34)}
BOW_FULL_L = (0.07, -0.74, 1.46)  # bow hand at full draw, arm out at shoulder height
BOW_LOW_L = (0.10, -0.42, 1.10)  # bow low before the body (nocked, ready)
BOW_LOW = (30, 0, -6)  # the bow tipped forward, lying a little over
BOW_UP = (0, -8, 0)  # upright, canted
QUIVER_MOUTH = (-0.06, 0.24, 1.60)  # quiver on the back, its mouth behind the right shoulder


def nock(bow_hand, bow_rot):
    """The string's middle (where the arrow nocks) for the bow hand and bow rotation."""
    return add(bow_hand, rot(bow_rot, (0.0, BOW_NOCK, 0.0)))


def arrow_rot(frm, to):
    """The arrow's "world" rotation pointing from `frm` to `to` (no roll)."""
    d = add(to, frm, -1.0)
    n = math.sqrt(sum(c * c for c in d))
    dx, dy, dz = (c / n for c in d)
    rx = math.degrees(math.acos(max(-1.0, min(1.0, dz))))
    rz = math.degrees(math.atan2(dx, -dy)) if abs(dx) + abs(dy) > 1e-9 else 0.0
    return (rx, 0.0, rz)


def _bow(l, bow_r, r_point, arrow=None, show=1, base=None, **over):
    p = _with(base or BOW_STANCE, **over)
    p["world"] = dict(p.get("world", {}), **{"socket.hand_l": bow_r})
    if arrow is not None:
        p["world"]["socket.hand_r"] = arrow
    p["reach"] = {"l": _l(l, (0.30, 0.0, -0.25)), "r": _r(r_point, (-0.35, 0.25, 0.0))}
    p["show"] = {"socket.hand_r": show}
    return p


def bow_ready():
    """Nocked and low: the bow before the body, the arrow on the string."""
    n = nock(BOW_LOW_L, BOW_LOW)
    return _bow(BOW_LOW_L, BOW_LOW, n, arrow=arrow_rot(n, BOW_LOW_L))


def bow_full():
    """Full draw: bow arm out, the string at the jaw, the arrow level at the target."""
    r = add(BOW_FULL_L, (0.0, ARROW - 0.04, 0.01))
    return _bow(BOW_FULL_L, BOW_UP, r, arrow=arrow_rot(r, BOW_FULL_L))


def bow(n, release):
    """ATTACK1 / ATTACK2 of an archer (`IroLut`: 79 frames, the arrow leaves on 44): ready,
    raise and draw, hold the aim, loose on `release` (the arrow gone), follow through,
    lower, reach back to the quiver, draw an arrow, nock it, ready."""
    ready, full = bow_ready(), bow_full()
    hl = tuple((a + b) / 2 for a, b in zip(BOW_LOW_L, BOW_FULL_L))
    hn = nock(hl, (12, -4, 0))
    half = _bow(hl, (12, -4, 0), add(hn, (0.0, 0.20, 0.02)), arrow=arrow_rot(add(hn, (0.0, 0.20, 0.02)), hl))
    loosed = _bow(BOW_FULL_L, BOW_UP, add(BOW_FULL_L, (0.03, ARROW + 0.06, 0.03)), show=0)
    follow = _bow(add(BOW_FULL_L, (0.0, 0.03, -0.03)), BOW_UP, add(BOW_FULL_L, (0.06, ARROW + 0.10, 0.0)), show=0)
    lowered = _bow(BOW_LOW_L, BOW_LOW, add(BOW_LOW_L, (-0.10, 0.25, 0.10)), show=0)
    grasp = _bow(BOW_LOW_L, BOW_LOW, QUIVER_MOUTH, arrow=(-15, 0, 0), show=1)
    drawn = _bow(BOW_LOW_L, BOW_LOW, add(QUIVER_MOUTH, (0.0, -0.10, 0.25)), arrow=(-10, 0, 0), show=1)
    nk = nock(BOW_LOW_L, BOW_LOW)
    nocking = _bow(BOW_LOW_L, BOW_LOW, add(nk, (-0.04, 0.02, 0.06)), arrow=arrow_rot(add(nk, (-0.04, 0.02, 0.06)), BOW_LOW_L))
    last = n - 1
    rel = min(release, last)
    keys = [(0, ready), (rel * 0.25, half), (rel * 0.55, full), (rel - 1, full), (rel, loosed), (rel + 3, follow)]
    rest = last - rel
    keys += [(rel + rest * 0.35, lowered), (rel + rest * 0.55, grasp), (rel + rest * 0.68, drawn),
             (rel + rest * 0.85, nocking), (last, ready)]
    return frames(n, [(round(f), p) for f, p in keys], event=rel)


# -- thrown weapon (TAMAGAVK, KOPIE) ---------------------------------------------------

THROW = {
    # (wind-up rotation, release rotation, ready rotation) of the weapon in the hand.
    "tomahawk": ((-70, 0, 0), (40, 0, 0), (10, 0, 0)),
    "spear": ((96, 0, 4), (86, 0, 0), (8, 0, 0)),
}


def _throw_pose(base, r_point, weapon_rot, show=1, l_point=None, **over):
    p = _with(base, **over)
    p["world"] = dict(p.get("world", {}), **{"socket.hand_r": weapon_rot})
    p["reach"] = {"r": _r(r_point, (-0.35, 0.20, -0.20))}
    if l_point is not None:
        p["reach"]["l"] = _l(l_point)
    p["show"] = {"socket.hand_r": show}
    return p


def throw(n, release, weapon="tomahawk"):
    """ATTACK1 of a thrower (`IroTam` tomahawk: 50 frames, leaves on 16; `AtcMet` spear:
    55 frames, leaves on 28): the weapon drawn back over the right shoulder, the left arm
    pointing, the trunk turned away; the throw (the weapon gone on `release`), the follow
    through across the body, recover, take the next from the belt, ready."""
    wind_r, rel_r, ready_r = THROW[weapon]
    ready = _throw_pose(STAND_FREE, (-0.22, -0.20, 1.05), ready_r)
    spear = weapon == "spear"
    wind = _throw_pose(dict(STAND_FREE, spine=(4, 0, 22), spine2=(2, 0, 12), head=(4, 0, -30),
                            thigh_r=(8, 4, 0), thigh_l=(-10, -2, 0)),
                       (-0.30, 0.28 if not spear else 0.20, 1.72 if not spear else 1.62), wind_r,
                       l_point=(0.12, -0.55, 1.45))
    cast = _throw_pose(dict(STAND_FREE, spine=(-14, 0, -18), spine2=(-6, 0, -10), head=(2, 0, 20),
                            root_loc=(0, -0.12, -0.03), thigh_l=(-22, -3, 0), shin_l=(16, 0, 0), thigh_r=(12, 3, 0)),
                       (-0.12, -0.62, 1.52), rel_r, show=0, l_point=(0.25, -0.05, 1.00))
    follow = _throw_pose(dict(STAND_FREE, spine=(-22, 0, -24), spine2=(-8, 0, -10), root_loc=(0, -0.16, -0.06),
                              thigh_l=(-26, -3, 0), shin_l=(22, 0, 0), thigh_r=(16, 3, 0)),
                         (0.10, -0.42, 0.98), rel_r, show=0)
    empty = _throw_pose(STAND_FREE, (-0.22, -0.18, 1.00), ready_r, show=0)
    draw = _throw_pose(STAND_FREE, (-0.20, 0.06, 0.98), (-20, 0, 0) if not spear else (6, 0, 0), show=1)
    last = n - 1
    rel = min(release, last)
    keys = [(0, ready), (max(rel - 6, 1), wind), (rel - 1, wind), (rel, cast), (rel + 4, follow),
            (rel + (last - rel) * 0.35, empty), (rel + (last - rel) * 0.6, draw), (last, ready)]
    return frames(n, [(round(f), p) for f, p in keys], event=rel)


# -- sling (KAMEN) -----------------------------------------------------------------------

def sling(n, release, turns=4):
    """ATTACK1 of a slinger (`IncPra`: 68 frames, the stone leaves on 56): load the pouch
    at the waist, raise, whirl over the head (`turns` turns, a quarter every two frames),
    the overhand cast on `release` (the release cord let go), follow through, load again."""
    load = _throw_pose(STAND_FREE, (-0.06, -0.24, 1.05), (180, 0, 0), l_point=(0.02, -0.24, 0.98))
    raised = _throw_pose(STAND_FREE, (-0.14, -0.10, 1.70), (100, 0, 0))
    whirl = []
    for q in range(4):
        whirl.append(_throw_pose(dict(STAND_FREE, spine=(0, 0, 10)), (-0.12, -0.04, 1.86), (95, 0, 90 * q)))
    wind = _throw_pose(dict(STAND_FREE, spine=(4, 0, 22), spine2=(2, 0, 10)), (-0.26, 0.18, 1.80), (-60, 0, 0),
                       l_point=(0.15, -0.50, 1.40))
    cast = _throw_pose(dict(STAND_FREE, spine=(-14, 0, -18), root_loc=(0, -0.10, -0.03), thigh_l=(-20, -3, 0),
                            shin_l=(14, 0, 0)), (-0.10, -0.58, 1.58), (60, 0, 0))
    follow = _throw_pose(dict(STAND_FREE, spine=(-20, 0, -22), root_loc=(0, -0.12, -0.05)), (0.08, -0.40, 1.00),
                         (150, 0, 0))
    last = n - 1
    rel = min(release, last)
    whirl_end = rel - 6
    whirl_start = max(whirl_end - turns * 8, 8)
    keys = [(0, load), (whirl_start - 4, raised)]
    f, q = whirl_start, 0
    while f < whirl_end:
        keys.append((f, whirl[q % 4]))
        f, q = f + 2, q + 1
    keys += [(rel - 2, wind), (rel, cast), (rel + 4, follow), (last, load)]
    return frames(n, keys, event=rel)


# -- blowpipe (VAFLIA) ------------------------------------------------------------------

def _pipe(r_point, pipe_rot, l_along=0.42, **over):
    p = _with(STAND_FREE, **over)
    p["world"] = {"socket.hand_r": pipe_rot}
    p["reach"] = {"r": _r(r_point, (-0.30, 0.10, -0.30)), "l": _l(add(r_point, axis(pipe_rot), l_along))}
    p["show"] = {}
    return p


MOUTH = (0.0, -0.11, 1.585)  # the lips (the pipe's mouthpiece just behind the grip)


def blowpipe(n, release):
    """ATTACK1 of a blowpipe warrior (`IncTrb`: 61 frames, the dart leaves on 18): load a
    dart at the mouthpiece, raise the pipe to the lips (both hands near the mouth end, the
    pipe level at the target), blow on `release` (cheeks and chest: a small jerk), hold,
    lower, load."""
    level = (90, 0, 0)
    load = _pipe((-0.10, -0.22, 1.20), (-25, 0, 0), l_along=-0.0)
    load["reach"]["l"] = _l((0.02, -0.25, 1.38))  # the left hand at the mouthpiece with a dart
    raised = _pipe(add(MOUTH, (0.0, -0.04, -0.03)), (80, 0, 0))
    aim = _pipe(add(MOUTH, (0.0, -0.035, 0.0)), level, spine=(-4, 0, 0), head=(-4, 0, 0))
    blow = _pipe(add(MOUTH, (0.0, -0.035, 0.0)), level, spine=(-8, 0, 0), spine2=(-4, 0, 0), head=(-6, 0, 0))
    last = n - 1
    rel = min(release, last)
    keys = [(0, load), (rel * 0.4, raised), (rel - 4, aim), (rel - 1, aim), (rel, blow), (rel + 4, aim),
            (rel + (last - rel) * 0.4, raised), (rel + (last - rel) * 0.7, load), (last, load)]
    return frames(n, [(round(f), p) for f, p in keys], event=rel)


# -- heal (MEDIK) -----------------------------------------------------------------------

def heal(n, active=1):
    """The healer's ATTACK (`SpnSha` 200 frames, shamans' ATTACK1 75-101): a looping
    blessing; the heal lands on `release_frame(active)` (2): both hands out over the man,
    palms down, then the arms rise and sink slowly (the priest's blessing, the shaman's
    rattle shaken), back to the opening pose."""
    rel = release_frame(active)
    open_ = _with(STAND_FREE, spine=(-6, 0, 0), head=(14, 0, 0))
    open_["reach"] = {"r": _r((-0.20, -0.40, 1.30)), "l": _l((0.20, -0.40, 1.30))}
    bless = _with(STAND_FREE, spine=(-12, 0, 0), spine1=(-4, 0, 0), head=(20, 0, 0))
    bless["reach"] = {"r": _r((-0.24, -0.52, 1.12)), "l": _l((0.24, -0.52, 1.12))}
    raised = _with(STAND_FREE, spine=(4, 0, 0), head=(-12, 0, 0))
    raised["reach"] = {"r": _r((-0.30, -0.25, 1.80), (-0.40, 0.10, 0.0)), "l": _l((0.30, -0.25, 1.80), (0.40, 0.10, 0.0))}
    last = n - 1
    keys = [(0, open_), (rel, bless), (rel + 4, bless)]
    # Rise and sink every ~40 frames.
    f, up = rel + 20, True
    while f < last - 10:
        keys.append((f, raised if up else bless))
        f, up = f + 20, not up
    keys.append((last, open_))
    return frames(n, keys, event=rel)


# -- drum: sticks on the head ------------------------------------------------------------

# The drum hangs on its carriage belt at the left thigh, the batter head tilted up and to
# the right (`held.drum` on a rigid mount at these numbers); `DRUM_HEAD` its centre,
# `DRUM_ROT` the drum's "world" rotation (its +Z, the head's normal).
DRUM_ROT = (-20, -40, 0)
DRUM_HEAD = (0.07, -0.20, 0.86)
DRUM_NORMAL = axis(DRUM_ROT)
# Where each stick strikes: a little either side of the centre.
STRIKE_R = add(DRUM_HEAD, (-0.03, -0.03, 0.0))
STRIKE_L = add(DRUM_HEAD, (0.03, 0.03, 0.0))
STICK_DOWN_R = (130, 0, 50)  # the right stick down onto the head, from the right
STICK_UP_R = (60, 0, 50)  # raised
STICK_DOWN_L = (115, 0, 140)  # the left stick in the traditional grip, from the left
STICK_UP_L = (55, 0, 140)


def stick_tip(grip, r):
    """Where a drumstick's tip is for its grip and "world" rotation."""
    return add(grip, axis(r), STICK)


def drum_pose(right_down, left_down, base=None, lift=0.10):
    """Both sticks: the down stick's tip on the head, the up stick raised `lift` above it."""
    p = dict(base or STAND_FREE)
    p["world"] = dict(p.get("world", {}))
    reach = {}
    for side, down, strike, rd, ru, pole in (("r", right_down, STRIKE_R, STICK_DOWN_R, STICK_UP_R, (-0.35, 0.15, -0.20)),
                                             ("l", left_down, STRIKE_L, STICK_DOWN_L, STICK_UP_L, (0.35, 0.15, -0.20))):
        r = rd if down else ru
        tip = strike if down else add(strike, DRUM_NORMAL, lift)
        grip = grip_for(tip, r, STICK)
        p["world"][f"socket.hand_{side}"] = r
        reach[side] = (tuple(round(c, 4) for c in grip), pole)
    p["reach"] = reach
    return p


def drum_ready(lift=0.035):
    """Ready to drum: both sticks held low over the head as for a stroke, the tips `lift`
    above it (the idle drummer's STAND: the sim plays STAND nearly always, motion.rs)."""
    p = dict(STAND_FREE)
    p["world"] = {}
    reach = {}
    for side, strike, r, pole in (("r", STRIKE_R, STICK_DOWN_R, (-0.35, 0.15, -0.20)),
                                  ("l", STRIKE_L, STICK_DOWN_L, (0.35, 0.15, -0.20))):
        grip = grip_for(add(strike, DRUM_NORMAL, lift), r, STICK)
        p["world"][f"socket.hand_{side}"] = r
        reach[side] = (tuple(round(c, 4) for c in grip), pole)
    p["reach"] = reach
    return p


STAND_DRUM = drum_ready()


def drum_roll(n, beat=2):
    """REST of a drummer (`EngB18` 70 frames): the sticks alternate on the head, one stroke
    every `beat` frames (a flam every eighth stroke), the drum still. Looping."""
    keys, f, i = [], 0, 0
    while f < n:
        r_down = i % 2 == 0
        keys.append((f, drum_pose(r_down, not r_down or i % 8 == 7)))
        f, i = f + beat, i + 1
    keys.append((n - 1, keys[0][1]))
    return frames(n, keys, event=None)


def drum_march(n_l, n_r):
    """MOTION_L and MOTION_R of a drummer (10 frames each): the step cycle of
    `poses.walk_cycle` with the sticks keeping time, a stroke a step and a tap between."""
    cl, pl, cr, pr = poses.walk_cycle(STAND_FREE)
    legs = ("root_loc", "thigh_l", "thigh_r", "shin_l", "shin_r", "foot_l", "foot_r")

    def at(gait, rd, ld):
        p = drum_pose(rd, ld)
        for k in legs:
            if k in gait:
                p[k] = gait[k]
        return p

    left = frames(n_l, [(0, at(cl, True, False)), ((n_l - 1) // 2, at(pl, False, True)), (n_l - 1, at(cr, True, False))])
    right = frames(n_r, [(0, at(cr, True, False)), ((n_r - 1) // 2, at(pr, False, True)), (n_r - 1, at(cl, True, False))])
    return left, right


# -- colour: carry and wave --------------------------------------------------------------

COLOUR_GRIP = (-0.10, -0.22, 1.28)  # the right hand on the staff, chest high


def colour_pose(staff_rot, base=None, grip=COLOUR_GRIP, left=0.32):
    """The colour's staff at `staff_rot` held by the right hand at `grip`, the left hand
    `left` m lower on the staff."""
    p = dict(base or STAND_FREE)
    p["world"] = dict(p.get("world", {}), **{"socket.hand_r": staff_rot})
    p["reach"] = {"r": _r(grip), "l": _l(add(grip, axis(staff_rot), -left))}
    return p


CARRY = (8, 0, 0)  # upright, the finial a little forward
STAND_COLOUR = colour_pose(CARRY)


def colour_carry(n_l, n_r):
    """MOTION_L / MOTION_R of a standard bearer: the step cycle with the colour carried
    upright, swaying a little with the step."""
    cl, pl, cr, pr = poses.walk_cycle(STAND_FREE)
    legs = ("root_loc", "thigh_l", "thigh_r", "shin_l", "shin_r", "foot_l", "foot_r")

    def at(gait, sway):
        p = colour_pose((8, sway, 0))
        for k in legs:
            if k in gait:
                p[k] = gait[k]
        return p

    left = frames(n_l, [(0, at(cl, 3)), ((n_l - 1) // 2, at(pl, 0)), (n_l - 1, at(cr, -3))])
    right = frames(n_r, [(0, at(cr, -3)), ((n_r - 1) // 2, at(pr, 0)), (n_r - 1, at(cl, 3))])
    return left, right


def colour_wave(n, period=30):
    """REST of a standard bearer (`EngF18` 120 frames): the colour swept in a figure of
    eight over the head, a sweep each `period` frames, upright between. Looping."""
    keys, f, i = [], 0, 0
    sweeps = [CARRY, (40, 0, 50), (12, 0, 0), (40, 0, -50)]
    while f < n:
        r = sweeps[i % 4]
        keys.append((f, colour_pose(r, base=_with(STAND_FREE, spine=(0, 0, r[2] * 0.2)), grip=(-0.06, -0.26, 1.18))))
        f, i = f + max(period // 4, 1), i + 1
    keys.append((n - 1, keys[0][1]))
    return frames(n, keys, event=None)


# -- melee on foot ----------------------------------------------------------------------

def _blow(base, r_point, weapon_rot, l_point=None, pole=(-0.35, 0.15, -0.20), **over):
    p = _with(base, **over)
    p["world"] = dict(p.get("world", {}), **{"socket.hand_r": weapon_rot})
    p["reach"] = {"r": _r(r_point, pole)}
    if l_point is not None:
        p["reach"]["l"] = _l(l_point)
    return p


LUNGE = {"root_loc": (0, -0.22, -0.08), "thigh_l": (-38, -3, 0), "shin_l": (34, 0, 0), "thigh_r": (18, 3, 0),
         "shin_r": (14, 0, 0), "spine": (-12, 0, -6)}
# The blow down at a man on the ground: a long lunge (the left foot ~1.3 m forward, the right
# leg straight behind, the hips ~0.78 m up), the trunk bent FORWARD over the man (positive
# spine X bends forward on skeleton H; until m2-shared-moves it was negative and the blows
# fell 0.45-0.9 m short).
STOOP = {"root_loc": (0, -0.80, -0.27), "thigh_l": (-76, -3, 0), "shin_l": (84, 0, 0), "foot_l": (8, 0, 0),
         "thigh_r": (40, 3, 0), "shin_r": (22, 0, 0), "foot_r": (-30, 0, 0), "spine": (30, 0, -8),
         "spine1": (10, 0, 0), "spine2": (4, 0, 0), "head": (18, 0, 0)}
# The cut's lean: the trunk in towards the man (forward), the left foot a step out.
LEAN = {"spine": (8, 0, -14), "spine1": (4, 0, 0), "root_loc": (0, -0.14, -0.04), "thigh_l": (-26, -3, 0),
        "shin_l": (20, 0, 0)}
# Targets (docs/melee.md, "Targets"): a standing man's body surface at the fencing distance
# (1.67 m centre to centre: 1.50 m ahead), his chest or head; a man on the ground where the
# sim leaves him before a man on foot, 2.0 m centre to centre (2.57 m when the attacker walks
# up: the strike layer's step takes the rest).
TARGET_MID = (0.0, -1.50, 1.15)
TARGET_HIGH = (0.0, -1.50, 1.55)
TARGET_DOWN = (0.0, -2.0, 0.20)


def stoop_raised(hit, r_point, weapon_rot, pole=(-0.35, 0.20, -0.05)):
    """The wind-up of a blow down: the lunge half taken, the trunk half bent, the weapon
    raised at `r_point`."""
    base = _with({k: v for k, v in hit.items() if k not in ("reach", "world")}, root_loc=(0, -0.40, -0.10),
                 spine=(12, 0, -4), spine1=(4, 0, 0), thigh_l=(-40, -3, 0), shin_l=(40, 0, 0), thigh_r=(20, 3, 0),
                 shin_r=(14, 0, 0), foot_l=(0, 0, 0), foot_r=(0, 0, 0))
    return _blow(base, r_point, weapon_rot, pole=pole)


def _strike(n, windup, hit, recover):
    return frames(n, rig.clip_keys(n, poses.strike_keys(n, windup, hit, recover)), event=STRIKE_HIT_FRAME)


def sword_strikes(n, tip=SWORD_TIP):
    """One-handed sword on foot (officers, `MECH`): `ATTACK_CUT_MID` and `ATTACK_CUT_HIGH`
    (from the blade up over the right shoulder, a diagonal cut ending on the target),
    `ATTACK_THRUST_MID` (a lunge), `ATTACK_THRUST_DOWN` = `ATTACK_DOWN` (the point stabbed
    down at a man on the ground). The hit on frame 2 puts `socket.tip` on the target."""
    guard_r = (60, 0, -10)
    guard = _blow(STAND_FREE, (-0.20, -0.35, 1.15), guard_r)
    up = _blow(dict(STAND_FREE, spine=(2, 0, 16)), (-0.30, 0.02, 1.72), (-40, 0, 30))
    cut_mid_r = (88, 0, 18)
    cut_high_r = (74, 0, 16)
    lean = dict(STAND_FREE, **LEAN)
    cut_mid = _blow(lean, grip_for(TARGET_MID, cut_mid_r, tip), cut_mid_r)
    cut_high = _blow(lean, grip_for(TARGET_HIGH, cut_high_r, tip), cut_high_r)
    thrust_r = (90, 0, 0)
    drawn = _blow(STAND_FREE, (-0.18, -0.10, 1.20), thrust_r)
    lunge = _blow(dict(STAND_FREE, **LUNGE), grip_for(TARGET_MID, thrust_r, tip), thrust_r)
    down_r = (140, 0, 0)
    stab = _blow(dict(STAND_FREE, **STOOP), grip_for(TARGET_DOWN, down_r, tip), down_r, pole=(-0.30, 0.25, -0.10))
    stoop_up = stoop_raised(stab, add(stab["reach"]["r"][0], (0, 0.12, 0.42)), (165, 0, 0))
    return {
        "ATTACK_CUT_MID": _strike(n, up, cut_mid, guard),
        "ATTACK_CUT_HIGH": _strike(n, up, cut_high, guard),
        "ATTACK_THRUST_MID": _strike(n, drawn, lunge, guard),
        "ATTACK_THRUST_DOWN": _strike(n, stoop_up, stab, stoop_up),
        "ATTACK_DOWN": _strike(n, stoop_up, stab, stoop_up),
    }


# The club's hack down: the stoop deeper (the club is ~0.35 m shorter than a sword), the
# head of the club square onto the man.
CLUB_DOWN = 130
CLUB_STOOP = dict(STOOP, root_loc=(0, -1.08, -0.38), thigh_l=(-88, -3, 0), shin_l=(92, 0, 0), thigh_r=(52, 3, 0),
                  spine=(42, 0, -8), spine1=(16, 0, 0), spine2=(8, 0, 0))


def club_strikes(n, tip=CLUB_TIP):
    """Club or tomahawk hack on foot (`MECH` natives, `TAMAGAVK` in melee): `ATTACK_CUT_MID`
    (overhead, down and forward onto the shoulder), `ATTACK_CUT_DOWN` = `ATTACK_DOWN` (the
    hack down at a man on the ground). The striking face (`socket.tip`) on the target at
    frame 2."""
    ready = _blow(STAND_FREE, (-0.22, -0.22, 1.12), (30, 0, 0))
    over = _blow(dict(STAND_FREE, spine=(4, 0, 12), spine2=(2, 0, 6)), (-0.16, 0.12, 1.86), (-75, 0, 0))
    mid_r = (78, 0, 6)
    step = dict(STAND_FREE, **dict(LUNGE, root_loc=(0, -0.44, -0.08), spine=(16, 0, -6), spine1=(6, 0, 0)))
    hack_mid = _blow(step, grip_for(add(TARGET_MID, (0, 0, 0.12)), mid_r, tip), mid_r)
    down_r = (CLUB_DOWN, 0, 0)
    hack_down = _blow(dict(STAND_FREE, **CLUB_STOOP), grip_for(TARGET_DOWN, down_r, tip), down_r,
                      pole=(-0.30, 0.25, -0.10))
    over_down = stoop_raised(hack_down, add(hack_down["reach"]["r"][0], (0, 0.45, 0.70)), (-40, 0, 0))
    return {
        "ATTACK_CUT_MID": _strike(n, over, hack_mid, ready),
        "ATTACK_CUT_DOWN": _strike(n, over_down, hack_down, over_down),
        "ATTACK_DOWN": _strike(n, over_down, hack_down, over_down),
    }


def spear_strikes(n, tip=SPEAR_TIP, left_along=0.45):
    """Spear or pike thrust on foot (`PIKA` natives: DelPik, MayPik, IncPik): two hands on the
    shaft, `ATTACK_THRUST_MID` (driven forward from the hip) and `ATTACK_THRUST_DOWN` =
    `ATTACK_DOWN` (point down at a man on the ground). `socket.tip` on the target at frame 2."""
    level = (88, 0, 0)

    def two(base, r_point, r):
        return _blow(base, r_point, r, l_point=add(r_point, axis(r), left_along))

    hit_mid = two(dict(STAND_FREE, spine=(-8, 0, -10), thigh_l=(-20, -3, 0), shin_l=(14, 0, 0)),
                  grip_for(TARGET_MID, level, tip), level)
    back = two(dict(STAND_FREE, spine=(0, 0, 12)), add(hit_mid["reach"]["r"][0], (0, 0.38, -0.05)), level)
    down_r = (130, 0, 0)
    hit_down = two(dict(STAND_FREE, spine=(-18, 0, -8), thigh_l=(-24, -3, 0), shin_l=(22, 0, 0),
                        root_loc=(0, -0.08, -0.06)), grip_for(TARGET_DOWN, down_r, tip), down_r)
    up_down = two(dict(STAND_FREE, spine=(-8, 0, 0)), add(hit_down["reach"]["r"][0], (0, 0.30, 0.22)), (122, 0, 0))
    return {
        "ATTACK_THRUST_MID": _strike(n, back, hit_mid, back),
        "ATTACK_THRUST_DOWN": _strike(n, up_down, hit_down, up_down),
        "ATTACK_DOWN": _strike(n, up_down, hit_down, up_down),
    }


# The shield up before the body on the left forearm (`held.shield` on socket.hand_l): a guard
# pose a family may key into ready stances (`PSTAND`); no clip of its own.
SHIELD_BLOCK = {
    "world": {"socket.hand_l": (0, 0, -10)},
    "reach": {"l": _l((0.10, -0.34, 1.22), (0.30, 0.0, -0.35))},
}


def with_shield(pose):
    """`pose` with the shield raised (the left arm's goal replaced)."""
    p = dict(pose)
    p["world"] = dict(p.get("world", {}), **SHIELD_BLOCK["world"])
    p["reach"] = dict(p.get("reach", {}), **SHIELD_BLOCK["reach"])
    return p


# -- by unit -------------------------------------------------------------------------------

MELEE = {"sword": sword_strikes, "club": club_strikes, "tomahawk": club_strikes, "spear": spear_strikes}
# The striking point in the item frame of the weapons whose point lies off the handle's
# axis (`held.war_club`, `held.tomahawk` `socket.tip`), for `unit_clips`.
MELEE_TIP = {"club": (0.0, -0.165, 0.39), "tomahawk": (0.0, -0.146, 0.38)}


def md_frames(line):
    return rig.md_clip_frames(line)[0]


def unit_clips(md, weapons, melee=None, active=None, thrown="tomahawk"):
    """{clip: Clip} for a unit from its `.MD` lines (`md`: {"ATTACK1": "#ATTACK1 9 79 ..."}),
    its `WEAPONKIND`s by attack index (`weapons`: {0: "MECH", 1: "STRELA"}), the melee
    weapon (`melee`: "sword", "club", "tomahawk", "spear") and the `SETACTIVEPOINT` frames
    by clip (`active`: {"ATTACK1": 44}). Index i plays `ATTACK<i>` ("ATTACK" for 0); a
    melee index adds the strike clips on ATTACK's frames. Clips whose `.MD` line is missing
    are left out. Families add the stand, walk and death clips (`poses`)."""
    active = active or {}
    out = {}
    for i, kind in sorted(weapons.items()):
        key = "ATTACK" if i == 0 else f"ATTACK{i}"
        if key not in md:
            continue
        n = md_frames(md[key])
        rel = release_frame(active.get(key, 1))
        if kind in ("STRELA", "OSTRELA"):
            out[key] = bow(n, rel)
            ready = bow_ready()
            for k in (f"PSTAND{i}", f"PATTACK{i}", f"UATTACK{i}"):
                if k in md:
                    m = md_frames(md[k])
                    out[k] = frames(m, [(0, ready)] if k.startswith("PSTAND") else
                                    [(0, STAND_FREE if k.startswith("P") else ready), (m - 1, ready if k.startswith("P") else STAND_FREE)])
        elif kind in ("TAMAGAVK", "KOPIE"):
            out[key] = throw(n, rel, "spear" if kind == "KOPIE" else thrown)
        elif kind == "KAMEN":
            out[key] = sling(n, rel)
        elif kind == "VAFLIA":
            out[key] = blowpipe(n, rel)
        elif kind == "MEDIK":
            out[key] = heal(n, active.get(key, 1))
        elif kind in ("MECH", "PIKA") and i == 0 and melee:
            strikes = MELEE[melee](n, **({"tip": MELEE_TIP[melee]} if melee in MELEE_TIP else {}))
            out.update(strikes)
            first = "ATTACK_CUT_MID" if "ATTACK_CUT_MID" in strikes else "ATTACK_THRUST_MID"
            out[key] = strikes[first]
    return out


def drummer_clips(md):
    """REST (drumming), STAND, MOTION_L/R and the `0` variants (marching with the drum) of
    an 18th-c. drummer from his `.MD` lines."""
    out = {}
    if "STAND" in md:
        out["STAND"] = frames(1, [(0, STAND_DRUM)])
    if "REST" in md:
        out["REST"] = drum_roll(md_frames(md["REST"]))
    for sfx in ("", "0"):
        if f"MOTION_L{sfx}" in md and f"MOTION_R{sfx}" in md:
            out[f"MOTION_L{sfx}"], out[f"MOTION_R{sfx}"] = drum_march(md_frames(md[f"MOTION_L{sfx}"]),
                                                                      md_frames(md[f"MOTION_R{sfx}"]))
    return out


def colour_clips(md):
    """REST (the colour waved), STAND and MOTION_L/R (carried) of a standard bearer."""
    out = {}
    if "STAND" in md:
        out["STAND"] = frames(1, [(0, STAND_COLOUR)])
    if "REST" in md:
        out["REST"] = colour_wave(md_frames(md["REST"]))
    if "MOTION_L" in md and "MOTION_R" in md:
        out["MOTION_L"], out["MOTION_R"] = colour_carry(md_frames(md["MOTION_L"]), md_frames(md["MOTION_R"]))
    return out


# -- Blender side -------------------------------------------------------------------------

EXTRA = ("reach", "show")


def bare(pose):
    """The pose spec without the keys `armature.pose` does not know."""
    return {k: v for k, v in pose.items() if k not in EXTRA}


def solve(arm_obj, pose):
    """The pose spec with its "reach" goals solved into arm angles (`aim._solve`)."""
    from mathutils import Vector

    from . import aim

    spec = bare(pose)
    reach = pose.get("reach")
    if not reach:
        return spec
    from . import armature as am

    am.pose(arm_obj, spec)
    arm = arm_obj.pose.bones
    goals = {s: (Vector(p), arm[f"upperarm_{s}"].head + Vector(pole)) for s, (p, pole) in reach.items()}
    return aim._solve(arm_obj, spec, goals, show=pose.get("show"))


def apply(arm_obj, name, clip):
    """Keys `clip` on the armature as action `name`: every pose solved (`solve`), the
    socket scales of "show" keyed as one-frame steps. Returns the action."""
    from . import armature as am

    cache = {}
    keys = []
    for f, pose in clip:
        k = id(pose)
        if k not in cache:
            cache[k] = solve(arm_obj, pose)
        keys.append((f, cache[k]))
    act = am.add_clip(arm_obj, name, keys)
    shows = [(f, p["show"]) for f, p in clip if p.get("show")]
    if shows:
        arm_obj.animation_data.action = act
        if hasattr(arm_obj.animation_data, "action_slot") and act.slots:
            arm_obj.animation_data.action_slot = act.slots[0]
        prev = {}
        for f, show in shows:
            for sock, v in show.items():
                pb = arm_obj.pose.bones[sock]
                if sock in prev and prev[sock] != v and f > 0:
                    pb.scale = (prev[sock],) * 3
                    pb.keyframe_insert("scale", frame=f - 1, group=pb.name)
                pb.scale = (float(v),) * 3
                pb.keyframe_insert("scale", frame=f, group=pb.name)
                prev[sock] = v
        for pb in arm_obj.pose.bones:
            pb.scale = (1.0, 1.0, 1.0)
        arm_obj.animation_data.action = None
    return act


# -- finishing blows (docs/melee.md, `finish.py`): the melee builders above also return the
# finishing clips (`ATTACK_FINISH`, `ATTACK_<KIND>_FINISH`).
import sys as _sys  # noqa: E402

from . import finish as _finish  # noqa: E402

_finish.register(_sys.modules[__name__])
