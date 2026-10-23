"""Conquistadors on foot, 16th century (family 12, docs/humans-roster.md; notes in
docs/families/conquista.md): SpnS15 and FrnS15 (arquebusiers) and SpnAlb (halberdier). The
mounted SpnRei is `cavalry.py`'s.

The man is `frontier.make`'s: a doublet (`civil.peasant_coat` without skirts, body and
sleeves in own cloth), hose, puffed slops over the breeches, a steel cuirass (and tassets
for the halberdier) as shells over the cloth, a morion, cabasset or flat cap, the sword in
its scabbard on the left hip (`c_scabbard`). The arquebusiers carry the
arquebus (a short long gun) with the musketeer clips (`frontier.build`); the halberdier
the halberd (`held.halberd`, longer) with his own clips: the thrust from the hip and the
stab down at a man on the ground, both leaning forward (the shared `clips.STOOP` and the
spear strike's lean bend the trunk back).

Registered here: hats `morion`, `cabasset`, `flat_cap`; gear `plume`, `c_scabbard`.
"""

import math

from . import clips as cl
from . import frontier, registry
from . import held as library
from . import poses, rig

# -- helmets and caps (origin at the crown socket; the facing is -Y) --------------------------


def morion(name, plume=True):
    """A combed morion: a steel dome, the brim turned up front and back into points, a comb,
    and a red plume at the back (`plume`)."""
    import bmesh
    from mathutils import Matrix

    from . import gear

    bm = bmesh.new()
    bmesh.ops.create_uvsphere(bm, u_segments=14, v_segments=8, radius=0.118)
    for v in bm.verts:
        v.co.z = max(v.co.z, -0.01) * 1.30
    rim = bmesh.ops.create_cone(bm, cap_ends=False, segments=16, radius1=0.16, radius2=0.115, depth=0.02,
                                matrix=Matrix.Translation((0, 0, -0.01)))
    for v in rim["verts"]:
        if v.co.xy.length > 0.13:
            v.co.x *= 0.85
            v.co.y *= 1.28
            v.co.z += 0.13 * (abs(v.co.y) / 0.20) ** 3
    bmesh.ops.create_cube(bm, size=1.0, matrix=Matrix.Translation((0, 0, 0.15)) @ Matrix.Diagonal((0.012, 0.20, 0.07, 1)))
    obj = gear.from_bm(bm, name)
    return obj


def cabasset(name):
    """A cabasset: a tall pear-shaped steel crown with a small stalk, a narrow flat brim."""
    import bmesh
    from mathutils import Matrix

    from . import gear

    bm = bmesh.new()
    bmesh.ops.create_uvsphere(bm, u_segments=14, v_segments=10, radius=0.115)
    for v in bm.verts:
        z = max(v.co.z, -0.01)
        t = max(z, 0.0) / 0.115
        v.co.x *= 1.0 - 0.35 * t * t
        v.co.y *= 1.08 * (1.0 - 0.35 * t * t)
        v.co.z = z * 1.55
    bmesh.ops.create_cone(bm, cap_ends=True, segments=16, radius1=0.15, radius2=0.15, depth=0.006,
                          matrix=Matrix.Translation((0, 0, -0.01)) @ Matrix.Diagonal((1.0, 1.12, 1.0, 1)))
    bmesh.ops.create_cone(bm, cap_ends=True, segments=6, radius1=0.012, radius2=0.004, depth=0.04,
                          matrix=Matrix.Translation((0, 0.01, 0.19)))
    return gear.from_bm(bm, name)


def flat_cap(name, n=16):
    """A flat cap (a beret with a stiff band), worn a little to the side."""
    import bmesh
    from mathutils import Matrix

    from . import gear

    bm = bmesh.new()
    bmesh.ops.create_cone(bm, cap_ends=False, segments=n, radius1=0.104, radius2=0.108, depth=0.04,
                          matrix=Matrix.Translation((0, 0, 0.02)))
    bmesh.ops.create_uvsphere(bm, u_segments=n, v_segments=6, radius=0.16,
                              matrix=Matrix.Translation((0.02, 0, 0.055)) @ Matrix.Diagonal((1.0, 1.0, 0.25, 1)))
    return gear.from_bm(bm, name)


registry.hat("morion", morion)
registry.hat("cabasset", cabasset)
registry.hat("flat_cap", flat_cap)


def _plume(ch, name, s):
    """A plume rising at the back of the helmet (on the hat socket: drops with the helmet)."""
    import bmesh
    from mathutils import Matrix, Vector

    from . import armature as am
    from . import gear

    bm = bmesh.new()
    bmesh.ops.create_cone(bm, cap_ends=True, segments=6, radius1=0.035, radius2=0.012, depth=0.22,
                          matrix=Matrix.Rotation(math.radians(-30), 4, "X"))
    obj = gear.from_bm(bm, f"{name}.plume")
    at = am.world(ch.arm, "socket.hat").translation + Vector((0, 0.10, 0.20))
    am.attach(obj, ch.arm, "socket.hat", Matrix.Translation(at))
    ch.held["plume"] = obj
    ch.keys[obj.name] = "plume"
    return None


def _scabbard(ch, name, s):
    """The sword in its scabbard on the left hip, hanging back at ~35° (rigid on `hips`):
    a black leather sheath and a steel cup-hilt above it."""
    import bmesh
    from mathutils import Matrix

    from . import gear

    bm = bmesh.new()
    bmesh.ops.create_cone(bm, cap_ends=True, segments=6, radius1=0.010, radius2=0.017, depth=0.86,
                          matrix=Matrix.Translation((0, 0, -0.43)))
    for v in bm.verts:
        v.co.y *= 0.45
    obj = gear.from_bm(bm, f"{name}.scabbard")
    at = Matrix.Translation((0.19, 0.05, 0.98)) @ Matrix.Rotation(math.radians(35), 4, "X")         @ Matrix.Rotation(math.radians(4), 4, "Y")
    gear.rigid(obj, ch.arm, "hips", at)
    ch.add("gear", obj, "scabbard")
    bm = bmesh.new()
    bmesh.ops.create_uvsphere(bm, u_segments=8, v_segments=4, radius=0.045,
                              matrix=Matrix.Diagonal((1.0, 1.0, 0.6, 1)))
    bmesh.ops.create_cone(bm, cap_ends=True, segments=6, radius1=0.012, radius2=0.012, depth=0.13,
                          matrix=Matrix.Translation((0, 0, 0.08)))
    bmesh.ops.create_cube(bm, size=1.0, matrix=Matrix.Diagonal((0.24, 0.012, 0.012, 1)))
    hilt = gear.from_bm(bm, f"{name}.hilt")
    gear.rigid(hilt, ch.arm, "hips", at @ Matrix.Rotation(math.radians(180), 4, "X") @ Matrix.Translation((0, 0, -0.02)))
    return ch.add("gear", hilt, "metal")


def _plate(ch):
    """(radius(t, z), cy) of the draped cloth round the trunk: radii per angle at 8
    heights between 0.96 and 1.46 m, interpolated."""
    from . import officers

    p = officers._cloth_points(ch)
    zs = [0.96 + 0.5 * k / 7 for k in range(8)]
    bands = [officers._band(p, z - 0.03, z + 0.03, n=24, out=0.0) for z in zs]
    cy = sorted(b[0] for b in bands)[len(bands) // 2]
    n = 24

    def radius(t, z):
        f = min(max((z - zs[0]) / (zs[-1] - zs[0]) * 7, 0.0), 6.999)
        k = int(f)
        w = f - k
        i = (t + math.pi) / (2 * math.pi) * n
        i0 = int(math.floor(i)) % n
        a = i - math.floor(i)
        r0 = bands[k][1][i0] * (1 - a) + bands[k][1][(i0 + 1) % n] * a
        r1 = bands[k + 1][1][i0] * (1 - a) + bands[k + 1][1][(i0 + 1) % n] * a
        return r0 * (1 - w) + r1 * w

    return radius, cy


def _cuirass(ch, name, s):
    """Breast and back (`armour`, rigid on `spine1`) fitted over the draped doublet: the
    breast curved like a 16th-century peascod (the fullest point low on the belly, a
    medial ridge, the lower edge dipping to a point in front), a scooped neck, arm openings
    (the band stays inside the arms), a short flange (fauld) turned out at the waist."""
    import bmesh
    from mathutils import Matrix, Vector

    from . import gear

    radius, cy = _plate(ch)
    n, rows = 32, 9
    bm = bmesh.new()
    grid = []
    for k in range(rows + 1):
        f = k / rows
        row = []
        for i in range(n):
            t = -math.pi + 2 * math.pi * i / n
            front = max(0.0, -math.sin(t))
            side = abs(math.cos(t))
            z_bot = 1.00 - 0.045 * front ** 3
            z_top = 1.44 - 0.07 * front ** 2 - 0.03 * side ** 4
            z = z_bot + f * (z_top - z_bot)
            r = radius(t, z) + 0.010
            # Peascod: the breast swells forward to a fullness low on the belly, the ridge.
            bulge = 0.030 * math.exp(-((z - 1.10) / 0.11) ** 2) + 0.010
            r += bulge * front ** 2 + 0.007 * front ** 12
            # The sides drawn in under the arms, a flatter back.
            r -= 0.006 * side ** 6 * (1.0 if z > 1.2 else 0.0)
            row.append(bm.verts.new((math.cos(t) * r, cy + math.sin(t) * r, z)))
        grid.append(row)
    for a, b in zip(grid, grid[1:]):
        for i in range(n):
            j = (i + 1) % n
            bm.faces.new((a[i], a[j], b[j], b[i]))
    # The fauld: the lower edge turned out and down 2.5 cm.
    lip = []
    for v in grid[0]:
        d = (v.co.xy - Vector((0.0, cy))).normalized() if (v.co.xy - Vector((0.0, cy))).length > 1e-6 else Vector((1, 0))
        lip.append(bm.verts.new((v.co.x + d.x * 0.018, v.co.y + d.y * 0.018, v.co.z - 0.025)))
    for i in range(n):
        j = (i + 1) % n
        bm.faces.new((lip[i], lip[j], grid[0][j], grid[0][i]))
    obj = gear.from_bm(bm, f"{name}.cuirass")
    gear.rigid(obj, ch.arm, "spine1", Matrix.Identity(4))
    ch.add("gear", obj, "armour")
    return obj


def _tassets(ch, name, s, lames=4):
    """Tassets: on each thigh a plate of `lames` overlapping curved lames hanging from the
    fauld, each lame lapping over the one below (rigid on the thigh)."""
    import bmesh
    from mathutils import Matrix

    from . import gear

    radius, cy = _plate(ch)
    out = None
    for side, bone in ((1, "thigh_l"), (-1, "thigh_r")):
        bm = bmesh.new()
        t0, t1 = (math.radians(-125), math.radians(-60)) if side > 0 else (math.radians(-120), math.radians(-55))
        if side < 0:
            t0, t1 = -math.pi - t1, -math.pi - t0
        for k in range(lames):
            z0 = 0.985 - 0.058 * k
            z1 = z0 - 0.07
            ring0, ring1, back0, back1 = [], [], [], []
            for j in range(6):
                t = t0 + (t1 - t0) * j / 5
                r0 = radius(t, 1.0) + 0.032 + 0.010 * k
                r1 = r0 + 0.012
                ring0.append(bm.verts.new((math.cos(t) * r0, cy + math.sin(t) * r0, z0)))
                ring1.append(bm.verts.new((math.cos(t) * r1, cy + math.sin(t) * r1, z1)))
                # The inside on its own vertices 0.5 mm in (one face twice over the same
                # vertices is "face already exists" in bmesh, `held.vane`).
                back0.append(bm.verts.new((math.cos(t) * (r0 - 0.0005), cy + math.sin(t) * (r0 - 0.0005), z0)))
                back1.append(bm.verts.new((math.cos(t) * (r1 - 0.0005), cy + math.sin(t) * (r1 - 0.0005), z1)))
            for j in range(5):
                bm.faces.new((ring0[j], ring0[j + 1], ring1[j + 1], ring1[j]))
                bm.faces.new((back1[j], back1[j + 1], back0[j + 1], back0[j]))
        obj = gear.from_bm(bm, f"{name}.tasset_{bone[-1]}")
        gear.rigid(obj, ch.arm, ch.prefix + bone, Matrix.Identity(4))
        out = ch.add("gear", obj, "armour")
    return out


registry.material("armour", "metal", fallback=(0.50, 0.51, 0.53))
registry.gear("c_cuirass", _cuirass)
registry.gear("c_tassets", _tassets)
registry.gear("plume", _plume)
registry.gear("c_scabbard", _scabbard)
registry.material("scabbard", "leather", painter="boots", fallback=(0.07, 0.06, 0.05))

# -- specs ------------------------------------------------------------------------------------

DEFAULTS = {"body": "doublet", "sleeves": "dsleeve", "skirts": False, "legs": "fhose",
            "layers": ("blouse", "slops", "cuirass"), "kit": ("belt_r", "c_scabbard"), "hat": "morion",
            "hat_at": (0.0, 0.0, -0.02, -4), "gun": 1.38}


# Draped (docs/clothing.md, "17th century"): long hose, paned slops gathered at the thigh,
# the linen cuffs and a doublet with slashed sleeves; the armour is modelled over them
# (`c_cuirass`, `c_tassets`), not the shells of `frontier._layer`.
DRAPE = ("hose", "slops", "shirt17", "doublet_slashed")
SHELLS = ("blouse", "slops", "cuirass", "tassets")


def spec(md, palette=None, build=None, **kw):
    """A conquistador's SPEC: `frontier.spec` with the family's defaults (doublet, hose,
    slops, cuirass, morion, scabbard, arquebus): the clothes draped (`DRAPE`), the cuirass
    and (where the layers list `tassets`) the tassets modelled."""
    s = dict(DEFAULTS, **kw)
    armour = ("c_cuirass",) * ("cuirass" in s["layers"]) + ("c_tassets",) * ("tassets" in s["layers"])
    s["layers"] = tuple(x for x in s["layers"] if x not in SHELLS)
    s["kit"] = tuple(s["kit"]) + armour
    pal = dict(palette or {})
    pal.setdefault("hose", pal.get("fhose", frontier.PALETTE["fhose"]))
    pal.setdefault("slashes", (0.86, 0.80, 0.62))
    pal.setdefault("shirt", (0.88, 0.86, 0.80))
    return frontier.spec(md, palette=pal, build=dict({"drape": DRAPE}, **(build or {})),
                         fixed=("slashes", "armour"), **s)


# -- the halberdier ---------------------------------------------------------------------------

HALBERD = {"length": 2.25, "grip": 0.92}


def halberd_tip():
    return library.halberd(**HALBERD).sockets["socket.tip"][2]


# The halberd at the order: grounded beside the right foot, the right hand at the chest.
STAND = dict(cl._with(cl.STAND_FREE, world={"socket.hand_r": (2, 0, 0)}), reach={"r": cl._r((-0.22, -0.10, 1.04))})
# A man on the ground as the sim leaves him (about 2.0 m centre to centre).
TARGET_DOWN = (0.0, -2.0, 0.20)


def _two(base, r_point, r, along):
    return cl._blow(base, r_point, r, l_point=cl.add(r_point, cl.axis(r), along))


def halberd_strikes(n, tip):
    """ATTACK_THRUST_MID (driven forward from the hip, the left hand forward on the shaft)
    and ATTACK_THRUST_DOWN = ATTACK_DOWN (an overhand stab down at a man on the ground, the
    trunk bent forward, the left hand above and behind the right). `socket.tip` on the
    target on frame 2. Positive spine X bends the trunk forward here."""
    level = (88, 0, 0)
    lean = dict(cl.STAND_FREE, spine=(10, 0, -12), spine1=(4, 0, 0), root_loc=(0, -0.16, -0.04),
                thigh_l=(-26, -3, 0), shin_l=(20, 0, 0), thigh_r=(12, 3, 0), shin_r=(8, 0, 0))
    hit_mid = _two(lean, cl.grip_for(cl.TARGET_MID, level, tip), level, 0.45)
    back = _two(dict(cl.STAND_FREE, spine=(0, 0, 10)), cl.add(hit_mid["reach"]["r"][0], (0, 0.30, -0.03)), level, 0.45)
    down_r = (125, 0, 0)
    bend = dict(cl.STAND_FREE, spine=(26, 0, -6), spine1=(8, 0, 0), spine2=(4, 0, 0), head=(10, 0, 0),
                root_loc=(0, -0.30, -0.14), thigh_l=(-50, -3, 0), shin_l=(52, 0, 0), foot_l=(-4, 0, 0),
                thigh_r=(26, 3, 0), shin_r=(20, 0, 0), foot_r=(-8, 0, 0))
    hit_down = _two(bend, cl.grip_for(TARGET_DOWN, down_r, tip), down_r, -0.35)
    raised = _two(dict(cl.STAND_FREE, spine=(10, 0, 0), root_loc=(0, -0.12, -0.04), thigh_l=(-24, -3, 0),
                       shin_l=(20, 0, 0)), cl.add(hit_down["reach"]["r"][0], (0, 0.25, 0.30)), (112, 0, 0), -0.35)
    return {
        "ATTACK_THRUST_MID": cl._strike(n, back, hit_mid, back),
        "ATTACK_THRUST_DOWN": cl._strike(n, raised, hit_down, raised),
        "ATTACK_DOWN": cl._strike(n, raised, hit_down, raised),
    }


TARGETS = {"ATTACK_THRUST_MID": cl.TARGET_MID, "ATTACK_THRUST_DOWN": TARGET_DOWN, "ATTACK_DOWN": TARGET_DOWN}
POLE_CLIPS = ("STAND", "MOTION_L", "MOTION_R", "MOTION_LB", "MOTION_RB", "MOTION_L0", "MOTION_R0", "MOTION_LB0",
              "MOTION_RB0", "PSTAND", "PSTAND1", "PATTACK", "UATTACK", "ATTACK", "DEATH", "DEATHLIE1")


def halberd_clips(md, tip=None):
    """{clip: Clip} of the halberdier from his `.MD` lines."""
    tip = halberd_tip() if tip is None else tip
    plain = cl.bare(STAND)
    lie = poses.lie_prone()
    c_l, p_l, c_r, p_r = poses.walk_cycle(STAND)
    keyed = {
        "STAND": [(0.0, STAND)],
        "MOTION_L": [(0.0, c_l), (0.5, p_l), (1.0, c_r)],
        "MOTION_R": [(0.0, c_r), (0.5, p_r), (1.0, c_l)],
        "MOTION_LB": [(0.0, c_l), (0.5, p_r), (1.0, c_r)],
        "MOTION_RB": [(0.0, c_r), (0.5, p_l), (1.0, c_l)],
        "DEATH": poses.death_keys(plain, lie),
        "DEATHLIE1": [(0.0, lie)],
    }
    for k in ("MOTION_L", "MOTION_R", "MOTION_LB", "MOTION_RB"):
        keyed[k + "0"] = keyed[k]
    out = {}
    for key, keys in keyed.items():
        if key in md or key == "STAND":
            n = cl.md_frames(md[key]) if key in md else 1
            out[key] = cl.frames(n, rig.clip_keys(n, keys))
    if "ATTACK" in md:
        strikes = halberd_strikes(cl.md_frames(md["ATTACK"]), tip)
        out.update(strikes)
        out["ATTACK"] = strikes["ATTACK_THRUST_MID"]
        ready = strikes["ATTACK_THRUST_MID"][0][1]
        for key, keys in (("PSTAND", [(0.0, ready)]), ("PSTAND1", [(0.0, ready)]),
                          ("PATTACK", [(0.0, STAND), (1.0, ready)]), ("UATTACK", [(0.0, ready), (1.0, STAND)])):
            if key in md:
                m = cl.md_frames(md[key])
                out[key] = cl.frames(m, rig.clip_keys(m, keys))
    return out


def probe(ch, name, clips):
    """{clip: (tip fwd, up, off the target)} of the halberd's point on each strike's hit frame."""
    import bpy

    from . import armature as am

    out = {}
    tip_obj = bpy.data.objects.get(f"{name}.halberd.socket.tip")
    if tip_obj is None:
        return out
    for clip, target in TARGETS.items():
        if clip not in clips:
            continue
        hit = next(p for f, p in clips[clip] if f == poses.STRIKE_HIT_FRAME)
        am.pose(ch.arm, cl.solve(ch.arm, hit))
        bpy.context.view_layer.update()
        t = tip_obj.matrix_world.translation
        off = math.dist((t.x, t.y, t.z), target)
        out[clip] = (round(-t.y, 3), round(t.z, 3), round(off, 3))
        print(f"PROBE {name} {clip} f{poses.STRIKE_HIT_FRAME}: tip fwd {-t.y:.2f} up {t.z:.2f} right {-t.x:.2f} "
              f"| target fwd {-target[1]:.2f} up {target[2]:.2f} (off {off:.2f})")
    return out


def build_pole(ctx, s):
    """Builds, animates, paints and prepares for export the halberdier of SPEC `s`."""
    from . import armature as am
    from . import crawl, faces, gear, heads, outfit
    from .humans import LAYER_OF, prepare_export

    md = {k: v for k, v in s["md"].items() if k in POLE_CLIPS}
    ch = frontier.make(ctx.root, ctx.name, dict(s, gun=None))
    gear.hold(ch, library.halberd(**HALBERD), f"{ctx.name}.halberd", "socket.hand_r", slot="weapon")
    heads.variants(ch)
    ch.morphs(extra_deflate=ch.layers["gear"])
    ch.masks()
    clips = halberd_clips(md)
    for clip, keys in clips.items():
        cl.apply(ch.arm, clip, keys)
    for clip, keys in crawl.clips().items():
        am.add_clip(ch.arm, clip, keys)
    ch.arm.animation_data.action = None
    ctx.probe = probe(ch, ctx.name, clips)
    am.set_clip(ch.arm, "STAND")
    report = {}
    colours = ch.colours(ctx.ref, s["palette"], report=report, mix=s["mix"], fixed=s["fixed"])
    outfit.dress(ch, colours, LAYER_OF)
    heads.finish(ch)
    ch.arm.animation_data.action = None
    for pb in ch.arm.pose.bones:
        pb.scale = (1.0, 1.0, 1.0)
    am.pose(ch.arm, cl.solve(ch.arm, clips["STAND"][0][1]))
    tris = ch.triangles()
    prepare_export(ch.all_meshes(), ctx.root, acr_triangles=sum(tris.values()),
                   acr_clips=list(clips) + list(crawl.CLIPS), acr_limb_radius=ch.limb_radius,
                   acr_light=report.get("light_dir_camera", []), acr_layers=list(ch.layers),
                   **faces.extras("regular"))
    ctx.stats = {"triangles": tris, "light": report, "colours": colours, "drape": getattr(ch, "drape_report", None),
                 "clips": sorted(clips), "probe": ctx.probe}


def pose_for(ch, s, clip):
    """Shows the first frame of `clip` (the last of DEATH) for the silhouette tool."""
    from . import armature as am

    if s.get("gun"):
        return frontier.pose_for(ch, s, clip)
    keys = halberd_clips(s["md"])[clip]
    ch._pose_spec = cl.solve(ch.arm, keys[-1][1] if clip == "DEATH" else keys[0][1])
    am.pose(ch.arm, ch._pose_spec)


def build(ctx, s):
    """Builds the arquebusier of SPEC `s` (`frontier.build`: the musketeer clips)."""
    frontier.build(ctx, s)
