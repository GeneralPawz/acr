"""Officers on foot, 17th and 18th century (family 4, docs/humans-roster.md; notes in
docs/families/officers.md): the line uniform of `line.py` in finer cloth, gold or silver
lace on the hat and the buttonholes, a gorget at the throat, a sash round the waist, an empty
scabbard on the left hip and a drawn sword (`held` officer's sword or smallsword) or a
spontoon in the right hand. No knapsack, no cartridge box, no musket.

A unit script is data like the line's: its `.MD` lines and a `SPEC` (`spec`), then
`build(ctx)` calls `officers.build(ctx, SPEC)`.

`SPEC` keys (`spec` fills the defaults): the line's (`line.spec`: `palette`, `build`, `fit`,
`hat`, `lace`, `mix`, `fixed`, `lapels`) plus
- `md`: the `.MD` lines by clip (STAND, REST, MOTION_*, ATTACK, PATTACK/UATTACK/PSTAND,
  DEATH, DEATHLIE1);
- `weapon`: a `held` item: `officer_sword`, `smallsword` (melee `sword`) or `spontoon`
  (melee `spear`, two hands);
- `kit`: the officer's gear (registered here): `off_sash`, `off_gorget`, `off_scabbard`.

`line.make` only knows the musket: the family builds the soldier through it, then takes
the musket away and puts the officer's weapon in the hand (no hook for another weapon in
`line.py`).

Clips (`clips.py` builders, keyed with `clips.apply`): STAND (sword at the side, the left
hand on the hip), REST (the command: the sword raised and pointed ahead, held, lowered),
MOTION_*, DEATH, DEATHLIE1, the crawl clips, and the strike clips on ATTACK's frames, the
blow on frame 2 (`clips.sword_strikes` with the weapon's real point: ATTACK_CUT_MID,
ATTACK_CUT_HIGH, ATTACK_THRUST_MID, ATTACK_THRUST_DOWN = ATTACK_DOWN, the stab down at a
man on the ground; the spontoon: `clips.spear_strikes`). ATTACK plays the first strike.
"""

import math

import bmesh
import bpy
from mathutils import Matrix, Vector

from . import armature as am
from . import clips as cl
from . import crawl, faces, gear, heads, line, outfit, poses, registry, rig
from . import held as library
from .humans import LAYER_OF, prepare_export

# Officer's colours over the line's: crimson sash, gilt lace and gorget.
PALETTE = {"sash": (0.62, 0.06, 0.10), "lace": (0.83, 0.67, 0.30), "brass": (0.80, 0.66, 0.34),
           "scabbard": (0.07, 0.06, 0.05)}
SILVER = (0.80, 0.80, 0.78)
KIT = ("off_sash", "off_gorget", "off_scabbard")
DEFAULTS = {"weapon": "officer_sword", "kit": KIT}
MELEE = {"officer_sword": "sword", "smallsword": "sword", "hanger": "sword", "spontoon": "spear"}

# Finer cloth: the officer's broadcloth (smoother, a little sheen), lace and gorget fixed.
registry.painter("officer_wool", like="wool", finish=(0.82, 0.60, 0.06))
registry.material("officer_coat", "wool", painter="officer_wool", fallback=(0.66, 0.09, 0.06))
registry.material("scabbard", "leather", painter="boots", fallback=(0.07, 0.06, 0.05))


def spec(md, palette=None, weapon="officer_sword", kit=KIT, lace=None, fixed=(), **kw):
    """A full SPEC: `line.spec` with the officer's palette, kit (no musket gear) and lace
    (default: gilt braid on the hat and lace on the buttonholes)."""
    lace = {"hat": 0.014, "buttonholes": True} if lace is None else lace
    s = line.spec(md, palette=dict(PALETTE, **(palette or {})), gear=tuple(kit), lace=lace or None,
                  fixed=tuple(fixed) + ("sash", "lace", "brass", "scabbard"), **kw)
    s.update(weapon=weapon)
    return s


def _cloth_points(ch):
    import numpy as np

    pts = []
    for o in ch.layers["clothes"]:
        if o.type != "MESH":
            continue
        m = o.matrix_world
        pts.extend(tuple(m @ v.co) for v in o.data.vertices)
    return np.array(pts, np.float32) if pts else np.zeros((0, 3), np.float32)


def _band(p, z0, z1, n=24, out=0.008, cy=None):
    """Radii (per angle round the waist centre) of the cloth between heights z0 and z1."""
    import numpy as np

    # The torso only: the hanging arms and cuffs reach down past the waist.
    sel = p[(p[:, 2] > z0) & (p[:, 2] < z1) & (np.abs(p[:, 0]) < 0.19) & (np.abs(p[:, 1]) < 0.20)]
    cy = float(np.median(sel[:, 1])) if cy is None else cy
    a = np.arctan2(sel[:, 1] - cy, sel[:, 0])
    r = np.hypot(sel[:, 0], sel[:, 1] - cy)
    radii = []
    for i in range(n):
        t = -math.pi + 2 * math.pi * i / n
        d = np.abs((a - t + math.pi) % (2 * math.pi) - math.pi)
        near = r[d < math.pi / n * 1.5]
        radii.append(min(float(near.max()), 0.20) + out if len(near) else 0.16)
    return cy, radii


def _two_sided(bm, pts):
    """A quad seen from both sides (a back face on its own vertices, 0.5 mm behind)."""
    a, b, c = pts[0], pts[1], pts[2]
    n = (b - a).cross(c - a).normalized() * 0.0005
    bm.faces.new([bm.verts.new(p) for p in pts])
    bm.faces.new([bm.verts.new(p - n) for p in reversed(pts)])


def _sash(ch, name, s):
    """A crimson silk sash round the waist over the coat, knotted on the left hip, the ends
    hanging with fringes (rigid on `hips`)."""
    p = _cloth_points(ch)
    z0, z1 = 0.99, 1.07
    cy, radii = _band(p, z0 - 0.02, z1 + 0.02, out=0.009)
    n = len(radii)
    bm = bmesh.new()
    rings = []
    for z, bulge in ((z0, 0.0), ((z0 + z1) / 2, 0.006), (z1, 0.0)):
        ring = []
        for i in range(n):
            t = -math.pi + 2 * math.pi * i / n
            r = radii[i] + bulge
            ring.append(bm.verts.new((math.cos(t) * r, cy + math.sin(t) * r, z)))
        rings.append(ring)
    for a, b in zip(rings, rings[1:]):
        for i in range(n):
            j = (i + 1) % n
            bm.faces.new((a[i], a[j], b[j], b[i]))
    # Knot and two hanging ends on the left hip (+X), a little forward.
    t = math.radians(-60)  # front-left (the facing is -Y)
    i = int(round((t + math.pi) / (2 * math.pi) * n)) % n
    r = radii[i] + 0.012
    knot = Vector((math.cos(t) * r, cy + math.sin(t) * r, (z0 + z1) / 2))
    bmesh.ops.create_icosphere(bm, subdivisions=1, radius=0.03, matrix=Matrix.Translation(knot))
    for dx, length in ((-0.025, 0.24), (0.02, 0.20)):
        top = knot + Vector((dx, -0.005, -0.02))
        bot = top + Vector((0.01, 0.0, -length))
        w0, w1 = 0.022, 0.03
        _two_sided(bm, [top + Vector((-w0, 0, 0)), top + Vector((w0, 0, 0)), bot + Vector((w1, -0.01, 0)),
                        bot + Vector((-w1, -0.01, 0))])
    obj = gear.from_bm(bm, f"{name}.sash")
    gear.rigid(obj, ch.arm, "hips", Matrix.Identity(4))
    return ch.add("gear", obj, "sash")


def _gorget(ch, name, s):
    """The gorget: a gilt crescent hung below the throat on the chest (rigid on `spine2`)."""
    import numpy as np

    p = _cloth_points(ch)
    mid = p[np.abs(p[:, 0]) < 0.04]
    top_z, bot_z = 1.445, 1.375
    y_top = float(mid[(mid[:, 2] > top_z - 0.02) & (mid[:, 2] < top_z + 0.01)][:, 1].min()) - 0.006
    y_bot = float(mid[(mid[:, 2] > bot_z - 0.02) & (mid[:, 2] < bot_z + 0.02)][:, 1].min()) - 0.008
    cy = y_top + 0.065  # the neck's centre behind the gorget's top edge
    bm = bmesh.new()
    inner, outer = [], []
    k = 9
    for j in range(k):
        a = math.radians(-62 + 124 * j / (k - 1))
        c = math.cos(a)
        ri = 0.065
        inner.append(Vector((math.sin(a) * ri, cy - c * ri, top_z - 0.004 * (1 - c))))
        drop = 0.018 + (top_z - bot_z - 0.018) * c * c
        ro = cy - (y_bot + (y_top - y_bot) * (1 - c * c)) if c > 0 else ri
        ro = max(ro, ri + 0.004)
        outer.append(Vector((math.sin(a) * (ro + 0.006), cy - c * ro, top_z - drop)))
    for j in range(k - 1):
        _two_sided(bm, [inner[j], inner[j + 1], outer[j + 1], outer[j]])
    obj = gear.from_bm(bm, f"{name}.gorget")
    gear.rigid(obj, ch.arm, "spine2", Matrix.Identity(4))
    return ch.add("gear", obj, "brass")


def _scabbard(ch, name, s):
    """The empty scabbard on the left hip, hanging back at ~38° (rigid on `hips`)."""
    bm = bmesh.new()
    bmesh.ops.create_cone(bm, cap_ends=True, segments=6, radius1=0.012, radius2=0.02, depth=0.80,
                          matrix=Matrix.Translation((0, 0, -0.40)))
    for v in bm.verts:
        v.co.y *= 0.45
    bmesh.ops.create_cube(bm, size=1.0, matrix=Matrix.Translation((0, 0, -0.01)) @ Matrix.Diagonal((0.05, 0.03, 0.04, 1)))
    obj = gear.from_bm(bm, f"{name}.scabbard")
    # +X about X tips the point back (+Y; the facing is -Y), a little in towards the leg.
    at = Matrix.Translation((0.19, 0.05, 0.98)) @ Matrix.Rotation(math.radians(38), 4, "X") \
        @ Matrix.Rotation(math.radians(4), 4, "Y")
    gear.rigid(obj, ch.arm, "hips", at)
    return ch.add("gear", obj, "scabbard")


registry.gear("off_sash", _sash)
registry.gear("off_gorget", _gorget)
registry.gear("off_scabbard", _scabbard)


def make(root, name, s):
    """The officer of SPEC `s` under `root`: the line soldier (`line.make`) with the
    officer's coat and kit, the musket replaced by the officer's weapon."""
    ch = line.make(root, name, dict(s, weapon="musket"))
    gun = ch.held.pop("musket")
    ch.keys.pop(gun.name, None)
    bpy.data.objects.remove(gun, do_unlink=True)
    # Finer cloth: the coat's broadcloth in the officer's painter.
    for o in ch.layers["clothes"]:
        if ch.keys.get(o.name) == "coat":
            ch.keys[o.name] = "officer_coat"
        mk = o.get("acr_material_keys")
        if mk:
            o["acr_material_keys"] = ["officer_coat" if k == "coat" else k for k in mk]
    s["palette"].setdefault("officer_coat", s["palette"]["coat"])
    w = gear.hold(ch, s["weapon"], f"{name}.{s['weapon']}", "socket.hand_r", slot="weapon")
    ch.keys[w.name] = "metal"
    return ch


def tip_length(weapon):
    """Grip to point of the held weapon (its `socket.tip`)."""
    return library.build(weapon).sockets["socket.tip"][2]


# -- clips ---------------------------------------------------------------------------------

HIP_L = (0.20, -0.02, 1.02)  # the left hand on the hip, elbow out
# The sword at the side, upright and a little forward, the left fist on the hip.
STAND = dict(cl.STAND_ARMED, reach={"l": cl._l(HIP_L, (0.40, 0.10, -0.05))})
# The spontoon at the order: grounded beside the right foot, the hand at the chest.
STAND_SPONTOON = dict(cl._with(cl.STAND_FREE, world={"socket.hand_r": (2, 0, 0)}),
                      reach={"r": cl._r((-0.22, -0.10, 1.12)), "l": cl._l(HIP_L, (0.40, 0.10, -0.05))})


def command(stand, weapon_rot=(62, 0, -6), point=(-0.24, -0.50, 1.50)):
    """The command or point: the arm raised forward, the blade aimed ahead and up."""
    p = cl._with(stand, spine=(4, 0, 6), neck=(-4, 0, 0), head=(-6, 0, 4))
    p["world"] = dict(p.get("world", {}), **{"socket.hand_r": weapon_rot})
    p["reach"] = dict(p.get("reach", {}), r=cl._r(point, (-0.35, 0.10, -0.25)))
    return p


def officer_clips(s):
    """{clip: [(frame, pose)]} for the officer of SPEC `s` from its `.MD` lines."""
    md = s["md"]
    melee = MELEE[s["weapon"]]
    stand = STAND_SPONTOON if melee == "spear" else STAND
    plain = cl.bare(stand)  # without the IK goals (walk and death keep the arms free)
    lie = poses.lie_prone()
    c_l, p_l, c_r, p_r = poses.walk_cycle(stand)
    point = command(stand) if melee == "sword" else command(stand, (40, 0, -4), (-0.26, -0.40, 1.30))
    keyed = {
        "STAND": [(0.0, stand)],
        "REST": [(0.0, stand), (0.18, point), (0.62, point), (0.85, stand), (1.0, stand)],
        "MOTION_L": [(0.0, c_l), (0.5, p_l), (1.0, c_r)],
        "MOTION_R": [(0.0, c_r), (0.5, p_r), (1.0, c_l)],
        "MOTION_LB": [(0.0, c_l), (0.5, p_r), (1.0, c_r)],
        "MOTION_RB": [(0.0, c_r), (0.5, p_l), (1.0, c_l)],
        "DEATH": poses.death_keys(plain, lie),
        "DEATHLIE1": [(0.0, lie)],
    }
    out = {}
    for key, keys in keyed.items():
        n = rig.md_clip_frames(md[key])[0] if key in md else 1
        if key in md or key == "STAND":
            out[key] = cl.frames(n, rig.clip_keys(n, keys))
    if "ATTACK" in md:
        n = cl.md_frames(md["ATTACK"])
        tip = tip_length(s["weapon"])
        strikes = sword_strikes(n, tip) if melee == "sword" else cl.spear_strikes(n, tip=tip)
        out.update(strikes)
        first = "ATTACK_CUT_MID" if "ATTACK_CUT_MID" in strikes else "ATTACK_THRUST_MID"
        out["ATTACK"] = strikes[first]
        ready = strikes["ATTACK_THRUST_MID"][0][1]
        for key, keys in (("PSTAND", [(0.0, ready)]), ("PATTACK", [(0.0, stand), (1.0, ready)]),
                          ("UATTACK", [(0.0, ready), (1.0, stand)])):
            if key in md:
                m = cl.md_frames(md[key])
                out[key] = cl.frames(m, rig.clip_keys(m, keys))
    return out


def build(ctx, s):
    """Builds, animates, paints and prepares for export the officer of SPEC `s`."""
    ch = make(ctx.root, ctx.name, s)
    heads.variants(ch)
    ch.morphs(extra_deflate=ch.layers["gear"])
    ch.masks()
    clips = officer_clips(s)
    for clip, keys in clips.items():
        cl.apply(ch.arm, clip, keys)
    for clip, keys in crawl.clips().items():
        am.add_clip(ch.arm, clip, keys)
    am.set_clip(ch.arm, "STAND")
    report = {}
    colours = ch.colours(ctx.ref, s["palette"], report=report, mix=s["mix"], fixed=s["fixed"])
    outfit.dress(ch, colours, LAYER_OF)
    heads.finish(ch)
    ch.arm.animation_data.action = None
    am.pose(ch.arm, cl.solve(ch.arm, clips["STAND"][0][1]))
    tris = ch.triangles()
    prepare_export(ch.all_meshes(), ctx.root, acr_triangles=sum(tris.values()),
                   acr_clips=list(clips) + list(crawl.CLIPS), acr_limb_radius=ch.limb_radius,
                   acr_light=report.get("light_dir_camera", []), acr_layers=list(ch.layers),
                   **faces.extras("regular"))
    ctx.stats = {"triangles": tris, "light": report, "colours": colours, "drape": getattr(ch, "drape_report", None),
                 "clips": sorted(clips)}


def pose_for(ch, s, clip):
    """Shows the first frame of `clip` (the last of DEATH) for the silhouette tool."""
    keys = officer_clips(s)[clip]
    ch._pose_spec = cl.solve(ch.arm, keys[-1][1] if clip == "DEATH" else keys[0][1])
    am.pose(ch.arm, ch._pose_spec)




# The shared `clips.STOOP` and the cut's lean bend the spine back (negative X leans the
# trunk back here: the shoulder 0.3 m behind where a stoop puts it), so the stab down ended
# 0.46 m short of the man (`strike_view` probe). The family's own blows lean forward.
# A man on the ground as the sim leaves him before an officer (bench: 2.0 m centre to
# centre from 2 m, 2.57 m when the officer walks up from 5 m; `ACR_STRIKE_DEBUG`'s "victim
# at"): the clip reaches 2.0 m, the strike layer's step (0.5 m) the rest.
TARGET_DOWN = (0.0, -2.0, 0.20)
# The long lunge: the left foot far forward, the right leg straight behind, low.
LUNGE_DOWN = {"thigh_l": (-76, -3, 0), "shin_l": (84, 0, 0), "foot_l": (8, 0, 0),
              "thigh_r": (40, 3, 0), "shin_r": (22, 0, 0), "foot_r": (-30, 0, 0)}


def stab_down(tip, blade=140, root=(0, -0.80, -0.27), spine=(30, 0, -8), spine1=(10, 0, 0), target=TARGET_DOWN):
    """The officer's stab down at a man on the ground (`ATTACK_THRUST_DOWN`): a long lunge,
    the trunk bent forward over the man, the arm driven down, the point on the target."""
    r = (blade, 0, 0)
    base = dict(cl.STAND_FREE, **dict(cl.STOOP, root_loc=root, spine=spine, spine1=spine1, spine2=(4, 0, 0),
                                      head=(18, 0, 0), **LUNGE_DOWN))
    return cl._blow(base, cl.grip_for(target, r, tip), r, pole=(-0.30, 0.25, -0.10))


def stab_raised(tip):
    """The wind-up of the stab: the same lunge half taken, the hilt raised high, the point down."""
    hit = stab_down(tip)
    base = cl._with(hit, root_loc=(0, -0.40, -0.10), spine=(12, 0, -4), spine1=(4, 0, 0),
                    thigh_l=(-40, -3, 0), shin_l=(40, 0, 0), thigh_r=(20, 3, 0), shin_r=(14, 0, 0))
    base.pop("reach")
    return cl._blow(base, cl.add(hit["reach"]["r"][0], (0, 0.12, 0.42)), (165, 0, 0), pole=(-0.35, 0.20, -0.05))


def cut(tip, target, r):
    """A cut at a standing man: the trunk leaning in (forward), the edge on `target`."""
    lean = dict(cl.STAND_FREE, spine=(8, 0, -14), spine1=(4, 0, 0), root_loc=(0, -0.14, -0.04),
                thigh_l=(-26, -3, 0), shin_l=(20, 0, 0))
    return cl._blow(lean, cl.grip_for(target, r, tip), r)


def sword_strikes(n, tip):
    """`clips.sword_strikes` with the officer's own stab down and cuts (forward lean)."""
    out = cl.sword_strikes(n, tip=tip)
    up, guard = out["ATTACK_CUT_MID"][0][1], out["ATTACK_THRUST_MID"][-1][1]
    guard = out["ATTACK_CUT_MID"][3][1] if len(out["ATTACK_CUT_MID"]) > 3 else guard
    raised, stab = stab_raised(tip), stab_down(tip)
    out["ATTACK_CUT_MID"] = cl._strike(n, up, cut(tip, cl.TARGET_MID, (88, 0, 18)), guard)
    out["ATTACK_CUT_HIGH"] = cl._strike(n, up, cut(tip, cl.TARGET_HIGH, (74, 0, 16)), guard)
    out["ATTACK_THRUST_DOWN"] = cl._strike(n, raised, stab, raised)
    out["ATTACK_DOWN"] = cl._strike(n, raised, stab, raised)
    return out


TARGETS = {"ATTACK_CUT_MID": cl.TARGET_MID, "ATTACK_CUT_HIGH": cl.TARGET_HIGH, "ATTACK_THRUST_MID": cl.TARGET_MID,
           "ATTACK_THRUST_DOWN": TARGET_DOWN, "ATTACK_DOWN": TARGET_DOWN}
