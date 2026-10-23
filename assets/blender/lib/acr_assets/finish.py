"""Finishing blows at a man on the ground (docs/melee.md, "Finishing blows"): pure data like
`clips.py`, the musket's poses solved in Blender (`musket_clips`).

Under the sim's `hit_locations` rule a man on foot closes in to stand over a man on the
ground (`melee::FINISH_REACH`, ~0.9 m centre to centre) and one blow kills him. These clips
play that blow: standing over the man, a short wind-up at frame 0, one decisive blow down at
`STRIKE_HIT_FRAME` (frame 2, the sim's hit frame), held one frame, then straight back to the
stand by ~30 % of the clip and kept there: no lunge, no lunge held.

Names (`strike_view::finish_names`): `ATTACK_<KIND>_FINISH` (`THRUST`, `CUT`, `SMASH`, the
sim's strike kind) and `ATTACK_FINISH` (before the first blow the kind is unknown). By weapon:

- sword, sabre, hanger (`sword`): the point stabbed down one-handed (`THRUST`; the edge's
  `CUT` plays the same stab);
- spear, pike, halberd, spontoon (`spear`): both hands, the point driven down (`THRUST`);
- axe, tomahawk, war club, macuahuitl, stone club (`club`, `tomahawk`): an overhead chop
  down (`CUT` and `SMASH`);
- knife (`knife`): down on one knee, a stab;
- musket with bayonet (`musket_clips`): both hands high, the bayonet driven down (`THRUST`);
- a point-blank shot keeps `ATTACK_SHOT_DOWN` (no finishing clip).

`register` wraps `clips`' melee builders so every family that builds sword, club or spear
strikes gets the finishing clips with them.
"""

from . import poses, rig

STRIKE_HIT_FRAME = poses.STRIKE_HIT_FRAME
_with = poses._with

# The man on the ground: his chest at the attacker's feet, 0.9 m ahead (`FINISH_REACH`
# 27 px at 30 px/m), a little right of the centre line (the weapon side).
TARGET_FINISH = (-0.05, -0.90, 0.20)
# Standing over him: the knees a little bent, the trunk forward (positive spine X bends
# forward on skeleton H), the left foot a short step in.
OVER = {"root_loc": (0, -0.10, -0.10), "thigh_l": (-30, -3, 0), "shin_l": (36, 0, 0), "thigh_r": (14, 3, 0),
        "shin_r": (18, 0, 0), "spine": (18, 0, -6), "spine1": (8, 0, 0), "spine2": (4, 0, 0), "head": (22, 0, 0)}
# Raised for the blow: upright, on straight legs.
RAISE = {"root_loc": (0, -0.04, 0.0), "thigh_l": (-14, -3, 0), "shin_l": (12, 0, 0), "spine": (-4, 0, -4),
         "head": (16, 0, 0)}
# Down on the right knee over the man (the knife).
KNEEL = {"root_loc": (0, -0.30, -0.48), "thigh_l": (-84, -3, 0), "shin_l": (86, 0, 0), "foot_l": (-4, 0, 0),
         "thigh_r": (6, 3, 0), "shin_r": (100, 0, 0), "foot_r": (-40, 0, 0), "spine": (22, 0, -6),
         "spine1": (8, 0, 0), "head": (20, 0, 0)}

# The weapon's angle at the blow ("world": (90, 0, 0) points it forward, 180 straight down).
SWORD_DOWN = (150, 0, 0)
SPEAR_DOWN = (156, 0, 0)
CLUB_DOWN = (128, 0, 0)
KNIFE_DOWN = (160, 0, 0)
MUSKET_DOWN = (155, 0, 0)
KNIFE_TIP = 0.20


def finish_keys(n, windup, hit, stand):
    """(fraction, pose) keys of a finishing clip of `n` frames: the wind-up at frame 0, the
    blow at `STRIKE_HIT_FRAME`, held one frame, back to `stand` by 30 % and kept."""
    last = max(n - 1, 1)
    h = STRIKE_HIT_FRAME / last
    return [(0.0, windup), (h, hit), (min((STRIKE_HIT_FRAME + 1) / last, 0.29), hit), (0.3, stand), (1.0, stand)]


def _clip(n, windup, hit, stand):
    from . import clips as cl

    return cl.frames(n, rig.clip_keys(n, finish_keys(n, windup, hit, stand)), event=STRIKE_HIT_FRAME)


def _poses(kind, tip):
    """(wind-up, hit) of a finishing blow with weapon `kind` whose point is `tip` (a length
    along its axis or a vector in the item frame, `clips.grip_for`)."""
    from . import clips as cl

    if kind in ("club", "tomahawk"):
        hit = cl._blow(dict(cl.STAND_FREE, **OVER), cl.grip_for(TARGET_FINISH, CLUB_DOWN, tip), CLUB_DOWN,
                       pole=(-0.30, 0.25, -0.10))
        up = cl._blow(dict(cl.STAND_FREE, **RAISE), (-0.14, 0.06, 1.90), (-50, 0, 0))
        return up, hit
    if kind == "spear":
        r = SPEAR_DOWN
        grip = cl.grip_for(TARGET_FINISH, r, tip)
        hit = cl._blow(dict(cl.STAND_FREE, **OVER), grip, r, l_point=cl.add(grip, cl.axis(r), 0.45))
        up_grip = cl.add(grip, (0, 0.10, 0.30))
        up = cl._blow(dict(cl.STAND_FREE, **RAISE), up_grip, (148, 0, 0), l_point=cl.add(up_grip, cl.axis((148, 0, 0)), 0.45))
        return up, hit
    if kind == "knife":
        hit = cl._blow(dict(cl.STAND_FREE, **KNEEL), cl.grip_for(TARGET_FINISH, KNIFE_DOWN, tip), KNIFE_DOWN,
                       pole=(-0.30, 0.25, -0.10))
        up = cl._blow(dict(cl.STAND_FREE, **KNEEL), cl.add(hit["reach"]["r"][0], (0, 0.12, 0.40)), (175, 0, 0))
        return up, hit
    # The sword's point (sabre, hanger, smallsword).
    hit = cl._blow(dict(cl.STAND_FREE, **OVER), cl.grip_for(TARGET_FINISH, SWORD_DOWN, tip), SWORD_DOWN,
                   pole=(-0.30, 0.25, -0.10))
    up = cl._blow(dict(cl.STAND_FREE, **RAISE), cl.add(hit["reach"]["r"][0], (0, 0.14, 0.42)), (172, 0, 0))
    return up, hit


# The strike kinds each weapon's finishing clip answers (`ATTACK_<KIND>_FINISH`).
KINDS = {"sword": ("THRUST", "CUT"), "spear": ("THRUST",), "club": ("CUT", "SMASH"), "tomahawk": ("CUT", "SMASH"),
         "knife": ("THRUST", "CUT")}


def finish_clips(n, kind, tip):
    """{clip: Clip} of the finishing blow with weapon `kind` on ATTACK's `n` frames:
    `ATTACK_FINISH` and `ATTACK_<KIND>_FINISH` for the kinds in `KINDS`."""
    from . import clips as cl

    up, hit = _poses(kind, tip)
    stand = cl._blow(cl.STAND_FREE, (-0.22, -0.22, 1.10), (30, 0, 0))
    clip = _clip(n, up, hit, stand)
    out = {"ATTACK_FINISH": clip}
    for k in KINDS.get(kind, ("THRUST",)):
        out[f"ATTACK_{k}_FINISH"] = clip
    return out


def with_finish(builder, kind, default_tip):
    """`builder(n, tip=...)` (a `clips` melee builder) that adds the finishing clips."""

    def build(n, *args, **kw):
        out = builder(n, *args, **kw)
        tip = kw.get("tip", args[0] if args else default_tip)
        out.update(finish_clips(n, kind, tip))
        return out

    build.__wrapped__ = builder
    build.__doc__ = builder.__doc__
    return build


def register(cl):
    """Wraps `clips`' sword, club and spear builders (and `clips.MELEE`) with the finishing
    clips; idempotent."""
    if getattr(cl, "_finish_registered", False):
        return
    cl._finish_registered = True
    cl.sword_strikes = with_finish(cl.sword_strikes, "sword", cl.SWORD_TIP)
    cl.club_strikes = with_finish(cl.club_strikes, "club", cl.CLUB_TIP)
    cl.spear_strikes = with_finish(cl.spear_strikes, "spear", cl.SPEAR_TIP)
    cl.MELEE.update({"sword": cl.sword_strikes, "club": cl.club_strikes, "tomahawk": cl.club_strikes,
                     "spear": cl.spear_strikes})


def musket_clips(arm_obj, md, stand):
    """{clip: [(frame, spec)]} of the bayonet's finishing blow (a musketeer, Blender side):
    both hands on the musket high before the chest, the bayonet driven down into the man
    (`aim.place`: the grip so that the muzzle lands on `TARGET_FINISH`), back to `stand`
    (the charge pose)."""
    from . import aim as aiming
    from . import clips as cl

    if "ATTACK" not in md:
        return {}
    n = cl.md_frames(md["ATTACK"])
    reach = MUSKET_REACH
    grip = cl.add(TARGET_FINISH, cl.axis(MUSKET_DOWN), -reach)
    hit = aiming.place(arm_obj, _with(_bare(stand), world={"socket.hand_r": MUSKET_DOWN}, **OVER), grip, left=0.40)
    up_r = (146, 0, 0)
    up_grip = cl.add(grip, (0, 0.12, 0.28))
    up = aiming.place(arm_obj, _with(_bare(stand), world={"socket.hand_r": up_r}, **RAISE), up_grip, left=0.40)
    keys = rig.clip_keys(n, finish_keys(n, up, hit, stand))
    return {"ATTACK_FINISH": keys, "ATTACK_THRUST_FINISH": keys}


# The musket's grip (`socket.hand_r`) to its bayonet's point (`socket.muzzle` + the
# bayonet), as the strike layer reads it on EngM18 (docs/melee.md: 1.44 m).
MUSKET_REACH = 1.44


def _bare(pose):
    return {k: v for k, v in pose.items() if k not in ("reach", "show", "world_loc")}
