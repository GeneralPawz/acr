"""Pike and shot, 17th century (family 6, docs/humans-roster.md 2.6; notes in
docs/families/pike.md): pikemen (`EngPik`, `FrnPik`), musketeers (`EngMus`, `SpnMus`,
`FrnS17`), drummers (`*B17`) and standard bearers (`*F17`).

All are built through `line.make` (the draped uniform cut long in the skirts, breeches,
stockings and shoes), with the period's kit registered here:
- hats: `pike_broad` (broad-brimmed felt, the brim cocked up on the left), `pike_morion`
  (combed steel helmet, brim turned up front and back), `pike_pot` (the English pikeman's
  pot: a round skull with a broad sloping brim);
- gear: `pike_corselet` (back and breast with a skirt of tassets, rigid on the trunk),
  `pike_apostles` (the bandolier's powder charges, the "twelve apostles", hung on the
  `belt_l` strap), `pike_match` (the coiled slow match in the left hand, the lit end glowing);
  the sword on its baldric is `belt_r` and officers' `off_scabbard`.

The pikeman holds two pikes, one per hand socket, shown one at a time (the socket's scale,
`show`): `socket.hand_l` carries the pike gripped near the butt (ordered at the side,
shouldered on the march, de Gheyn's left-shoulder carry), `socket.hand_r` the pike gripped
two-thirds down (the charge: the thrust overhand, the butt high behind, so the point lands
at the game's fencing distance, 1.5 m ahead, and on a man on the ground at 2.0 m).
`line.make` only knows the musket: the pikeman is built with it, then disarmed (as
`officers.make`).
"""

import math

import bmesh
import bpy
from mathutils import Matrix, Vector

from . import armature as am
from . import clips as cl
from . import crawl, drape, faces, frontier, gear, heads, held, line, officers, outfit, poses, registry, rig  # noqa: F401
from .humans import LAYER_OF, prepare_export

# The pike: 16 ft of ash (4.9 m) with a leaf head and langets.
PIKE_LEN = 4.9
GRIP_ORDER = 1.30  # left hand, from the butt: ordered and shouldered
GRIP_CHARGE = 3.35  # right hand, from the butt: the charge (the point 1.81 m beyond the hand)
PIKE_TIP = PIKE_LEN - GRIP_CHARGE + 0.25 + 0.05 * 0.25

registry.material("match", "linen", fallback=(0.55, 0.45, 0.30))
registry.material("armour", "metal", fallback=(0.50, 0.51, 0.53))


# -- hats ----------------------------------------------------------------------------------

def broad_hat(name, n=16, lace=None):
    """A broad-brimmed felt hat with a tall crown, the brim cocked up on the left."""
    bm = bmesh.new()
    inner, outer = [], []
    for i in range(n):
        a = 2 * math.pi * i / n
        x, y = math.cos(a), math.sin(a)
        up = 0.09 * max(0.0, x) ** 2  # the left side (+X) cocked up
        inner.append(bm.verts.new((x * 0.105, y * 0.115, -0.03)))
        outer.append(bm.verts.new((x * 0.23, y * 0.24, -0.045 + up - 0.02 * max(0.0, -y))))
    for i in range(n):
        j = (i + 1) % n
        bm.faces.new((inner[i], inner[j], outer[j], outer[i]))
    top = bmesh.ops.create_cone(bm, cap_ends=True, segments=10, radius1=0.105, radius2=0.09, depth=0.15,
                                matrix=Matrix.Translation((0, 0, 0.045)))
    for v in top["verts"]:
        v.co.y *= 1.1
    return gear.from_bm(bm, name)


def morion(name, lace=None):
    """A combed morion: a high steel skull, brim turned up front and back."""
    bm = bmesh.new()
    bmesh.ops.create_uvsphere(bm, u_segments=14, v_segments=8, radius=0.115)
    for v in bm.verts:
        v.co.z = max(v.co.z, -0.02) * 1.30
    rim = bmesh.ops.create_cone(bm, cap_ends=False, segments=16, radius1=0.16, radius2=0.115, depth=0.02,
                                matrix=Matrix.Translation((0, 0, -0.02)))
    for v in rim["verts"]:
        if v.co.xy.length > 0.13:
            v.co.x *= 0.80
            v.co.y *= 1.30
            v.co.z += 0.12 * (abs(v.co.y) / 0.21) ** 3
    bmesh.ops.create_cube(bm, size=1.0, matrix=Matrix.Translation((0, 0, 0.14)) @ Matrix.Diagonal((0.01, 0.20, 0.06, 1)))
    return gear.from_bm(bm, name)


def pot(name, lace=None):
    """The English pikeman's pot: a round skull and a broad brim sloping down all round."""
    bm = bmesh.new()
    bmesh.ops.create_uvsphere(bm, u_segments=14, v_segments=8, radius=0.112)
    for v in bm.verts:
        v.co.z = max(v.co.z, -0.02) * 1.12
    bmesh.ops.create_cone(bm, cap_ends=False, segments=16, radius1=0.20, radius2=0.112, depth=0.05,
                          matrix=Matrix.Translation((0, 0, -0.045)))
    return gear.from_bm(bm, name)


registry.hat("pike_broad", broad_hat)
registry.hat("pike_morion", morion)
registry.hat("pike_pot", pot)


# -- gear ----------------------------------------------------------------------------------

def _front_y(p, x, z, dz=0.03, dx=0.03):
    sel = p[(abs(p[:, 0] - x) < dx) & (abs(p[:, 2] - z) < dz) & (p[:, 1] < 0.0)]
    return float(sel[:, 1].min()) if len(sel) else -0.14


def _corselet(ch, name, s):
    """Back and breast over the coat (fitted per angle to the draped cloth between 1.00
    and 1.42 m, the arms left out), a peascod ridge in front, and four tassets hanging
    over the thighs (rigid: breast on `spine1`, tassets on `hips`)."""
    p = officers._cloth_points(ch)
    rings = []
    zs = (1.00, 1.08, 1.18, 1.28, 1.36, 1.42)
    n = 24
    for z in zs:
        cy, radii = officers._band(p, z - 0.03, z + 0.03, n=n, out=0.012)
        rings.append((z, cy, radii))
    bm = bmesh.new()
    vs = []
    for z, cy, radii in rings:
        ring = []
        for i in range(n):
            t = -math.pi + 2 * math.pi * i / n
            r = radii[i]
            front = max(0.0, -math.sin(t))
            r += 0.012 * front ** 4 * (1.0 if z < 1.2 else 0.5)  # the ridge
            ring.append(bm.verts.new((math.cos(t) * r, cy + math.sin(t) * r, z)))
        vs.append(ring)
    for a, b in zip(vs, vs[1:]):
        for i in range(n):
            j = (i + 1) % n
            bm.faces.new((a[i], a[j], b[j], b[i]))
    breast = gear.from_bm(bm, f"{name}.corselet")
    gear.rigid(breast, ch.arm, "spine1", Matrix.Identity(4))
    ch.add("gear", breast, "armour")
    # Tassets: two plates each side of the front, flaring out from the waist.
    z0, cy, radii = rings[0]
    bm = bmesh.new()
    for side in (-1, 1):
        for k, (a0, a1) in enumerate(((-100, -70), (-70, -40))):
            t0, t1 = math.radians(a0), math.radians(a1)
            if side < 0:
                t0, t1 = math.radians(-180 - a1), math.radians(-180 - a0)
            pts = []
            for t, z, out in ((t0, z0 + 0.01, 0.0), (t1, z0 + 0.01, 0.0), (t1, z0 - 0.24, 0.05), (t0, z0 - 0.24, 0.05)):
                i = int(round((t + math.pi) / (2 * math.pi) * n)) % n
                r = radii[i] + 0.004 + out
                pts.append(Vector((math.cos(t) * r, cy + math.sin(t) * r, z)))
            officers._two_sided(bm, pts)
    tassets = gear.from_bm(bm, f"{name}.tassets")
    gear.rigid(tassets, ch.arm, "hips", Matrix.Identity(4))
    return ch.add("gear", tassets, "armour")


def _apostles(ch, name, s):
    """The bandolier's charges: wooden bottles with leather caps hung in a row down the
    `belt_l` strap's front run (left shoulder to right hip), a bullet bag at its end."""
    p = officers._cloth_points(ch)
    bm = bmesh.new()
    a, b = Vector((0.13, 0.0, 1.40)), Vector((-0.17, 0.0, 1.00))
    for k in range(9):  # the front nine of the twelve
        t = (k + 0.5) / 9
        c = a.lerp(b, t)
        y = _front_y(p, c.x, c.z) - 0.035
        bmesh.ops.create_cone(bm, cap_ends=True, segments=6, radius1=0.016, radius2=0.014, depth=0.085,
                              matrix=Matrix.Translation((c.x, y, c.z - 0.06)))
    y = _front_y(p, -0.18, 0.95) - 0.04
    bmesh.ops.create_icosphere(bm, subdivisions=1, radius=0.045, matrix=Matrix.Translation((-0.19, y, 0.92))
                               @ Matrix.Diagonal((1.0, 0.6, 1.2, 1)))
    obj = gear.from_bm(bm, f"{name}.apostles")
    gear.rigid(obj, ch.arm, "spine1", Matrix.Identity(4))
    return ch.add("gear", obj, "cartridge")


def _match(ch, name, s):
    """The slow match: a coil of cord in the left hand, both ends lit (rigid on `hand_l`,
    placed at the hand's rest head)."""
    h = ch.arm.pose.bones["hand_l"].bone
    at = h.head_local + (h.tail_local - h.head_local) * 0.6
    bm = bmesh.new()
    pts = []
    for i in range(25):
        u = i / 24 * 4 * math.pi
        pts.append(Vector((0.03 * math.cos(u), 0.03 * math.sin(u), -0.10 * i / 24 - 0.02)))
    for q0, q1 in zip(pts, pts[1:]):
        d = q1 - q0
        rot = d.to_track_quat("Z", "Y").to_matrix().to_4x4()
        bmesh.ops.create_cone(bm, cap_ends=False, segments=4, radius1=0.005, radius2=0.005, depth=d.length,
                              matrix=Matrix.Translation(at + (q0 + q1) / 2) @ rot)
    for end in (pts[0] + Vector((0, -0.08, 0.04)), pts[-1] + Vector((0, -0.06, -0.06))):
        bmesh.ops.create_icosphere(bm, subdivisions=1, radius=0.008, matrix=Matrix.Translation(at + end))
    obj = gear.from_bm(bm, f"{name}.match")
    gear.rigid(obj, ch.arm, "hand_l", Matrix.Identity(4))
    return ch.add("gear", obj, "match")


registry.gear("pike_corselet", _corselet)
registry.gear("pike_apostles", _apostles)
registry.gear("pike_match", _match)


# -- specs ---------------------------------------------------------------------------------

# A 17th-century soldier over the line's: the long coat closed (lapels in the coat's cloth),
# stockings for gaiters, buff belts.
PALETTE = {"buff": (0.74, 0.58, 0.36), "fallband": (0.92, 0.91, 0.86), "slashes": (0.86, 0.80, 0.62),
           "jackboots": (0.20, 0.13, 0.08), "belts": (0.62, 0.45, 0.25), "gaiters": (0.55, 0.52, 0.48), "hat": (0.10, 0.09, 0.08),
           "cartridge": (0.12, 0.09, 0.06), "match": (0.55, 0.45, 0.30), "armour": (0.50, 0.51, 0.53),
           "scabbard": (0.07, 0.06, 0.05), "brass": (0.72, 0.58, 0.30)}
FIXED = ("belts", "match", "armour", "scabbard", "brass", "cartridge", "buff", "fallband", "slashes", "jackboots")
# The 17th century's own clothes (docs/clothing.md, "17th century"), drape order: the
# breeches, stockings (the gaiter slot) and linen cuffs under a doublet with tabbed skirts
# (musketeers) or a buff coat with cloth sleeves (pikemen, under the armour); the falling
# band is modelled over them (`drape.falling_band`, gear).
MUSKETEER_DRAPE = ("breeches", "gaiters", "shirt17", "doublet")
PIKEMAN_DRAPE = ("breeches", "gaiters", "shirt17", "buffcoat")
MUSKET_KEYS = ("STAND", "MOTION_L", "MOTION_R", "MOTION_LB", "MOTION_RB", "PATTACK", "UATTACK", "PSTAND", "ATTACK",
               "PATTACK1", "UATTACK1", "PSTAND1", "ATTACK1", "DEATH", "DEATHLIE1")


def _palette(palette):
    pal = dict(PALETTE, **(palette or {}))
    pal.setdefault("facings", pal.get("coat"))
    # The doublet and the cloth sleeves in the unit's coat colour.
    pal.setdefault("doublet", pal.get("coat"))
    pal.setdefault("dsleeve", pal.get("coat"))
    return pal


def _drape(kw, plan):
    kw["build"] = dict({"drape": plan}, **(kw.get("build") or {}))
    return kw


def musketeer_spec(md, palette=None, hat="pike_broad", **kw):
    """The musketeer: broad hat, bandolier with the apostles over the left shoulder, sword on
    a baldric over the right, the matchlock (the line's musket path: present, fire and the
    reload drill) and the slow match in the left hand. `md` is cut to the clips the
    musket path keys (the stance-3 rest clips are not built)."""
    md = {k: v for k, v in md.items() if k in MUSKET_KEYS}
    pal = _palette(palette)
    _drape(kw, MUSKETEER_DRAPE)
    return line.spec(md, palette=pal, hat=hat,
                     gear=("belt_l", "belt_r", "falling_band", "pike_apostles", "pike_match", "off_scabbard"),
                     fixed=FIXED + tuple(kw.pop("fixed", ())), **kw)


def support_spec(md, weapon, palette=None, hat="pike_broad", **kw):
    """A 17th-century drummer (`drum`) or standard bearer (`colour`) through the line's
    support path. A drummer's `.MD` without `PSTAND` or `MOTION_L0` gets a one-frame
    `PSTAND` so `line.support_clips` keys him as a drummer."""
    md = dict(md)
    if weapon == "drum" and "PSTAND" not in md and "MOTION_L0" not in md:
        md["PSTAND"] = "#PSTAND 9 1 0 0"
    pal = _palette(palette)
    _drape(kw, MUSKETEER_DRAPE)
    gear_ = ("belt_r", "falling_band") if weapon == "drum" else ("belt_l", "falling_band")
    return line.spec(md, palette=pal, hat=hat, gear=gear_, weapon=weapon, fixed=FIXED + tuple(kw.pop("fixed", ())), **kw)


def pikeman_spec(md, palette=None, hat="pike_morion", **kw):
    """The pikeman: helmet, back and breast with tassets, a sword at the left hip, the pike."""
    pal = _palette(palette)
    _drape(kw, PIKEMAN_DRAPE)
    s = line.spec(md, palette=pal, hat=hat, gear=("falling_band", "pike_corselet", "off_scabbard"),
                  fixed=FIXED + tuple(kw.pop("fixed", ())), **kw)
    s["weapon"] = "pike"
    return s


# -- the pikeman ---------------------------------------------------------------------------

def make_pikeman(root, name, s):
    """The pikeman of SPEC `s`: `line.make` with the musket, disarmed, two pikes in hand."""
    ch = line.make(root, name, dict(s, weapon="musket"))
    gun = ch.held.pop("musket")
    ch.keys.pop(gun.name, None)
    bpy.data.objects.remove(gun, do_unlink=True)
    am.pose(ch.arm, {})
    for side, grip in (("l", GRIP_ORDER), ("r", GRIP_CHARGE)):
        it = held.spear(length=PIKE_LEN, grip=grip, head=0.25)
        it.name = "pike"
        gear.hold(ch, it, f"{name}.pike_{side}", f"socket.hand_{side}", slot=f"pike_{side}")
    return ch


_with = poses._with
_r, _l = cl._r, cl._l
SHOW_ORDER = {"socket.hand_l": 1, "socket.hand_r": 0}
SHOW_CHARGE = {"socket.hand_l": 0, "socket.hand_r": 1}

# Ordered: the pike upright at the left side, the butt by the left foot, the left hand at
# the chest; the right arm hangs.
ORDER = dict(_with(cl.STAND_FREE, world={"socket.hand_l": (0, 0, 0)}),
             reach={"l": _l((0.24, -0.06, 1.30), (0.40, 0.10, -0.20))}, show=SHOW_ORDER)
# Shouldered for the march (left shoulder): the butt forward and low, the pike sloping
# back over the shoulder.
SHOULDER = dict(_with(cl.STAND_FREE, world={"socket.hand_l": (-48, 0, 0)}),
                reach={"l": _l((0.16, -0.24, 1.28), (0.40, 0.0, -0.30))}, show=SHOW_ORDER)

# Targets (docs/melee.md): a standing man's chest at the fencing distance, a man on the
# ground where the sim leaves him (2.0 m, `officers.TARGET_DOWN`).
TARGET_MID = cl.TARGET_MID
TARGET_DOWN = officers.TARGET_DOWN


def _two(base, r_point, rot, left_along=0.55, pole=(-0.35, 0.25, -0.25)):
    p = cl._blow(base, r_point, rot, pole=pole)
    p["reach"]["l"] = _l(cl.add(r_point, cl.axis(rot), left_along), (0.35, 0.0, -0.30))
    p["show"] = SHOW_CHARGE
    return p


# The charge, overhand: the right hand at the shoulder, the point down a little.
LEVEL = (98, 0, 0)
CHARGE_BASE = dict(cl.STAND_FREE, thigh_l=(-22, -4, 0), shin_l=(16, 0, 0), thigh_r=(10, 4, 0), root_loc=(0, -0.04, -0.03),
                   spine=(6, 0, -10))
# The push: a long step, the trunk thrown forward (positive spine X: forward on skeleton H;
# the shared `clips.STOOP` leans back).
PUSH_BASE = dict(cl.STAND_FREE, thigh_l=(-50, -4, 0), shin_l=(46, 0, 0), thigh_r=(26, 4, 0), shin_r=(10, 0, 0),
                 foot_r=(-16, 0, 0), root_loc=(0, -0.26, -0.10), spine=(14, 0, -10), spine1=(6, 0, 0))
DOWN_BASE = dict(cl.STAND_FREE, **dict(officers.LUNGE_DOWN, root_loc=(0, -0.62, -0.22), spine=(30, 0, -8), spine1=(10, 0, 0),
                                       spine2=(4, 0, 0), head=(14, 0, 0)))


def pike_ready():
    """The charge (PSTAND): levelled overhand, the hand drawn back."""
    hit = cl.grip_for(TARGET_MID, LEVEL, PIKE_TIP)
    return _two(CHARGE_BASE, cl.add(hit, (0, 0.62, 0.05)), LEVEL)


def pike_thrust():
    return _two(PUSH_BASE, cl.grip_for(TARGET_MID, LEVEL, PIKE_TIP), LEVEL)


DOWN = (140, 0, 0)


def pike_down():
    return _two(DOWN_BASE, cl.grip_for(TARGET_DOWN, DOWN, PIKE_TIP), DOWN, left_along=0.50)


def pike_down_raised():
    hit = pike_down()
    base = _with(DOWN_BASE, root_loc=(0, -0.30, -0.08), spine=(14, 0, -4), thigh_l=(-40, -3, 0), shin_l=(40, 0, 0),
                 thigh_r=(20, 3, 0), shin_r=(14, 0, 0))
    return _two(base, cl.add(hit["reach"]["r"][0], (0, 0.30, 0.35)), (122, 0, 0), left_along=0.50)


# Charge for horse: kneeling, the butt set against the right foot, the point raised at a
# rider's chest.
BRACE = _two(dict(cl.STAND_FREE, root_loc=(0, 0.06, -0.40), thigh_l=(-80, -6, 0), shin_l=(82, 0, 0), thigh_r=(0, 6, 0),
                  shin_r=(100, 0, 0), foot_r=(30, 0, 0), spine=(10, 0, 0)), (-0.10, -0.10, 0.90), (62, 0, 0), left_along=0.45)


def pike_strikes(n):
    """ATTACK_THRUST_MID (the push at a man's chest), ATTACK_THRUST_DOWN = ATTACK_DOWN (down at
    a man on the ground at 2.0 m), the point (`socket.tip`) on the target on frame 2."""
    ready, raised = pike_ready(), pike_down_raised()
    return {
        "ATTACK_THRUST_MID": cl._strike(n, ready, pike_thrust(), ready),
        "ATTACK_THRUST_DOWN": cl._strike(n, raised, pike_down(), raised),
        "ATTACK_DOWN": cl._strike(n, raised, pike_down(), raised),
    }


TARGETS = {"ATTACK_THRUST_MID": TARGET_MID, "ATTACK_THRUST_DOWN": TARGET_DOWN, "ATTACK_DOWN": TARGET_DOWN}


def pikeman_clips(md):
    """{clip: Clip} for the pikeman from his `.MD` lines: STAND (ordered), MOTION_* (shouldered),
    MOTION_*0 (at the charge), PATTACK/UATTACK (port to the charge), PSTAND (charge), the
    strikes on ATTACK's frames, DEATH, DEATHLIE1, and the extra `BRACE` (charge for horse)."""
    lie = poses.lie_prone()
    c_l, p_l, c_r, p_r = poses.walk_cycle(cl.bare(SHOULDER))
    ready = pike_ready()
    rc_l, rp_l, rc_r, rp_r = poses.walk_cycle(cl.bare(ready))

    def keep(p, src):
        return dict(p, reach=src["reach"], show=src["show"])

    walk = [keep(x, SHOULDER) for x in (c_l, p_l, c_r, p_r)]
    rwalk = [keep(x, ready) for x in (rc_l, rp_l, rc_r, rp_r)]
    keyed = {
        "STAND": [(0.0, ORDER)],
        "PSTAND": [(0.0, ready)],
        "PATTACK": [(0.0, ORDER), (0.45, dict(ORDER, show=SHOW_ORDER)), (0.5, ready), (1.0, ready)],
        "UATTACK": [(0.0, ready), (0.5, ready), (0.55, ORDER), (1.0, ORDER)],
        "DEATH": [(f, dict(p, show=SHOW_CHARGE)) for f, p in poses.death_keys(cl.STAND_FREE, lie)],
        "DEATHLIE1": [(0.0, dict(lie, show=SHOW_CHARGE))],
    }
    for sfx, w in (("", walk), ("0", rwalk)):
        cl_, pl_, cr_, pr_ = w
        keyed.update({f"MOTION_L{sfx}": [(0.0, cl_), (0.5, pl_), (1.0, cr_)],
                      f"MOTION_R{sfx}": [(0.0, cr_), (0.5, pr_), (1.0, cl_)],
                      f"MOTION_LB{sfx}": [(0.0, cl_), (0.5, pr_), (1.0, cr_)],
                      f"MOTION_RB{sfx}": [(0.0, cr_), (0.5, pl_), (1.0, cl_)]})
    out = {}
    for key, keys in keyed.items():
        if key in md or key == "STAND":
            n = cl.md_frames(md[key]) if key in md else 1
            out[key] = cl.frames(n, rig.clip_keys(n, keys))
    if "ATTACK" in md:
        strikes = pike_strikes(cl.md_frames(md["ATTACK"]))
        out.update(strikes)
        out["ATTACK"] = strikes["ATTACK_THRUST_MID"]
    out["BRACE"] = cl.frames(1, [(0, BRACE)])
    return out


def build_pikeman(ctx, s):
    """Builds, animates, paints and prepares for export the pikeman of SPEC `s`."""
    ch = make_pikeman(ctx.root, ctx.name, s)
    heads.variants(ch)
    ch.morphs(extra_deflate=ch.layers["gear"])
    ch.masks()
    clips = pikeman_clips(s["md"])
    for clip, keys in clips.items():
        cl.apply(ch.arm, clip, keys)
    # The crawl: both pikes hidden (dropped with the fall).
    for clip, keys in crawl.clips().items():
        k = list(keys)
        k[0] = (k[0][0], dict(k[0][1], show={"socket.hand_l": 0, "socket.hand_r": 0}))
        cl.apply(ch.arm, clip, k)
    am.set_clip(ch.arm, "STAND")
    report = {}
    colours = ch.colours(ctx.ref, s["palette"], report=report, mix=s["mix"], fixed=s["fixed"])
    outfit.dress(ch, colours, LAYER_OF)
    heads.finish(ch)
    ch.arm.animation_data.action = None
    am.pose(ch.arm, cl.solve(ch.arm, clips["STAND"][0][1]))
    # Every pose-bone scale back to 1 before export: a zero socket scale would be baked
    # into the pikes' node transforms (the clips key the sockets' scale).
    for pb in ch.arm.pose.bones:
        pb.scale = (1.0, 1.0, 1.0)
    tris = ch.triangles()
    prepare_export(ch.all_meshes(), ctx.root, acr_triangles=sum(tris.values()),
                   acr_clips=list(clips) + list(crawl.CLIPS), acr_limb_radius=ch.limb_radius,
                   acr_light=report.get("light_dir_camera", []), acr_layers=list(ch.layers),
                   **faces.extras("regular"))
    ctx.stats = {"triangles": tris, "light": report, "colours": colours, "drape": getattr(ch, "drape_report", None),
                 "clips": sorted(clips)}


def matchlocks(ch, name, rest=True):
    """The line's flintlock shapes swapped for `held.matchlock` (the musket in hand and the
    slung one, same frame, sockets and slots), and with `rest` the forked musket rest
    (`held.musket_rest`) in the left hand with the match."""
    import bpy as _bpy

    for slot in ("musket", "musket_slung"):
        gun = ch.held.get(slot)
        if gun is None:
            continue
        tmp = gear.held(held.matchlock(), f"{name}.matchlock_tmp")
        old = gun.data
        gun.data = tmp.data
        gun.data.name = gun.name
        gun["acr_material_keys"] = list(tmp["acr_material_keys"])
        gun["acr_held_item"] = "matchlock"
        for c in list(tmp.children):
            _bpy.data.objects.remove(c)
        _bpy.data.objects.remove(tmp)
        _bpy.data.meshes.remove(old)
    if rest:
        am.pose(ch.arm, {})
        gear.hold(ch, held.musket_rest(), f"{name}.musket_rest", ch.prefix + "socket.hand_l", slot="rest")
        am.pose(ch.arm, poses.STAND_MUSKET)
    return ch


_LINE_MAKE = line.make


def make_musketeer(root, name, s):
    return matchlocks(_LINE_MAKE(root, name, s), name, rest=s["weapon"] == "musket")


def build(ctx, s):
    if s["weapon"] == "pike":
        build_pikeman(ctx, s)
    elif s["weapon"] == "musket":
        # `line.build` has no hook for its `make` (line.py belongs to the moves work): it is
        # swapped for the build's duration so the matchlock and the rest go in.
        line.make = make_musketeer
        try:
            line.build(ctx, s)
        finally:
            line.make = _LINE_MAKE
    else:
        line.build(ctx, s)


def make(root, name, s):
    if s["weapon"] == "pike":
        return make_pikeman(root, name, s)
    return make_musketeer(root, name, s) if s["weapon"] == "musket" else _LINE_MAKE(root, name, s)


def pose_for(ch, s, clip):
    if s["weapon"] != "pike":
        return line.pose_for(ch, s, clip)
    keys = pikeman_clips(s["md"])[clip]
    ch._pose_spec = cl.solve(ch.arm, keys[-1][1] if clip == "DEATH" else keys[0][1])
    am.pose(ch.arm, ch._pose_spec)
