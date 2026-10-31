"""Frontiersmen and irregulars (family 11, docs/humans-roster.md; notes in
docs/families/frontier.md): EngTap (trapper), EngTapDragon (jäger), UsaMil (militia),
FrnBuk (buccaneer). The base of family 12 (`conquista.py`) as well: a man in draped work
clothes (`civil.peasant_coat` as hunting shirt, waistcoat or doublet; breeches; leggings or
hose), shell layers over them (shoulder cape, sash, cloak, cuirass), strapped kit (powder
horn, shot pouch, tomahawk and knife in the belt, scabbard) and a hat, with the long gun in
the right hand and the musketeer clips of `line.py` (present, fire and reload, the butt and
bayonet blows of `poses.musketeer_strikes`).

A unit script is data: its `.MD` lines and `spec(md, ...)`, then `build(ctx)` calls
`frontier.build(ctx, SPEC)`.

`SPEC` keys (`spec` fills the defaults):
- `md`: the `.MD` lines of the clips the family makes (STAND, MOTION_*, PATTACK, UATTACK,
  PSTAND, ATTACK, PATTACK1, UATTACK1, PSTAND1, ATTACK1, DEATH, DEATHLIE1);
- `palette`: colours by material key over `PALETTE`; `mix` the sprite mix, `fixed` keys kept;
- `body`, `sleeves`, `skirts`: the draped upper garment (`civil.peasant_coat`): its body and
  sleeve cloth keys, with `skirts` to mid-thigh (the hunting shirt);
- `legs`: the cloth key of the leggings or hose (the gaiter pattern without the garter);
- `layers`: shells over the cloth, in order: `cape` (the hunting shirt's shoulder cape),
  `blouse` (the shirt or doublet over the waistband), `sash`, `cloak` (over the back and
  shoulders), `cuirass` (breast and back plate), `tassets`, `slops` (puffed breeches);
- `kit`: registered gear: `horn`, `pouch`, `tomahawk_belt`, `knife`, `belt_l` (the strap of
  horn and pouch over the left shoulder), `c_scabbard` and `plume` (conquista.py);
- `hat`: a registered hat (`fur_cap`, `round_hat`, `buccaneer_hat`, `tricorn`, ... and
  conquista.py's `morion`, `cabasset`, `flat_cap`), `hat_at` its (dx, dy, dz, tilt°);
- `gun`: the long gun's length (m) or None (conquista.py's halberdier);
- `carry`: `"waist"` to hold the gun at the belt in both hands (`CARRIES`); `reload`: the
  reload of `poses.STYLES` (`"trapper"`: the short gun's own, with the rammer scaled to it);
- `load`: a load of pelts on the right shoulder, shown in the stance walks only (`WALKS`,
  `add_load`).
"""

import math

from . import civil, registry

# Own cloth keys, each with its own painter (`outfit.dress` paints one colour per painter).
for _key, _layer, _like, _fin, _rgb in (
        ("hunting", "linen", "linen", (0.95, 0.30, 0.0), (0.62, 0.52, 0.36)),
        ("leggings", "leather", "leather", (0.75, 0.10, 0.10), (0.45, 0.33, 0.20)),
        ("fsash", "wool", "wool", (1.0, 0.45, 0.0), (0.62, 0.08, 0.06)),
        ("fur", "hide", "felt", (1.0, 0.70, 0.0), (0.35, 0.24, 0.14)),
        ("horn", "bone", "leather", (0.55, 0.0, 0.30), (0.82, 0.74, 0.56)),
        ("pouch", "leather", "leather", (0.75, 0.05, 0.10), (0.28, 0.18, 0.10)),
        ("fcloak", "wool", "wool", (1.0, 0.50, 0.0), (0.70, 0.10, 0.06)),
        ("fshirt", "linen", "linen", (0.95, 0.20, 0.0), (0.88, 0.86, 0.80)),
        ("fvest", "wool", "kersey", (1.0, 0.30, 0.0), (0.55, 0.14, 0.08)),
        ("fhose", "linen", "linen", (0.95, 0.20, 0.0), (0.78, 0.76, 0.70)),
        ("doublet", "wool", "wool", (1.0, 0.40, 0.0), (0.72, 0.30, 0.14)),
        ("dsleeve", "wool", "wool", (1.0, 0.40, 0.0), (0.70, 0.12, 0.08)),
        ("slops", "wool", "wool", (1.0, 0.40, 0.0), (0.80, 0.62, 0.18)),
        ("plume", "wool", "felt", (1.0, 0.60, 0.0), (0.75, 0.08, 0.06)),
        ("pelt", "hide", "felt", (1.0, 0.70, 0.0), (0.78, 0.74, 0.66))):
    registry.painter(_key, like=_like, finish=_fin)
    registry.material(_key, _layer, painter=_key, fallback=_rgb)

PALETTE = {
    "skin": (0.80, 0.58, 0.45), "bone": (0.85, 0.81, 0.69), "hair": (0.40, 0.28, 0.16),
    "hunting": (0.62, 0.52, 0.36), "leggings": (0.45, 0.33, 0.20), "fsash": (0.62, 0.08, 0.06),
    "fur": (0.35, 0.24, 0.14), "horn": (0.82, 0.74, 0.56), "pouch": (0.28, 0.18, 0.10), "fcloak": (0.70, 0.10, 0.06),
    "fshirt": (0.88, 0.86, 0.80), "fvest": (0.55, 0.14, 0.08), "fhose": (0.78, 0.76, 0.70),
    "doublet": (0.72, 0.30, 0.14), "dsleeve": (0.70, 0.12, 0.08), "slops": (0.80, 0.62, 0.18),
    "plume": (0.75, 0.08, 0.06), "pelt": (0.78, 0.74, 0.66), "breeches": (0.42, 0.32, 0.20), "shoes": (0.18, 0.12, 0.07),
    "hat": (0.30, 0.22, 0.14), "wood": (0.42, 0.24, 0.12), "metal": (0.58, 0.58, 0.60),
    "belts": (0.30, 0.20, 0.11), "scabbard": (0.07, 0.06, 0.05), "brass": (0.80, 0.66, 0.34),
}
BUILD = {"width": 0.98, "shoulders": 1.06, "limbs": 1.15}
KIT = ("belt_l", "horn", "pouch", "tomahawk_belt", "knife")
FIXED = ("horn", "pouch", "metal", "wood", "belts", "scabbard", "brass", "plume", "pelt")
DEFAULTS = {"body": "hunting", "sleeves": "hunting", "skirts": True, "legs": "leggings", "layers": ("cape", "sash"),
            "kit": KIT, "hat": "fur_cap", "hat_at": (0.0, 0.005, -0.07, -8), "gun": 1.62, "mix": 0.4, "fixed": (),
            "build": BUILD, "melee": "musket", "drape": False, "carry": None, "reload": None,
            "load": False}
# With `drape` the shell layers `blouse`, `sash` and `cloak` are drafted and draped instead
# (`garments.tunic`, `band`, `cloak`; #287 drape pilot): the blouse replaces the draped shirt.
DRAPED = {"blouse": "frn_blouse", "sash": "frn_sash", "cloak": "frn_cape"}

# Clips the family makes from the `.MD`: the musketeer's, with the reload stance of a
# SLOWRECHARGE gunman (#255, `poses.STANCE4_CLIPS`). Extra stances of the sprites,
# MOTION_*0..4 and ATTACK2, are not modelled: the game falls back to the plain ones.
CLIPS = ("STAND", "MOTION_L", "MOTION_R", "MOTION_LB", "MOTION_RB", "PATTACK", "UATTACK", "PSTAND", "ATTACK",
         "PATTACK1", "UATTACK1", "PSTAND1", "ATTACK1", "PSTAND3", "PATTACK3", "UATTACK3", "ATTACK3", "TRANSX3",
         "TRANS3X", "DEATH", "DEATHLIE1")
# The stance walk of a food carrier (`TAKERESSTAGES FOOD .. 5`, `RATE 4 8`: the sim walks it
# at half speed on `MOTION_L4`, 10 frames a step), made as a walk (`poses.motion_gait`).
WALKS = ("MOTION_L4", "MOTION_R4", "MOTION_LB4", "MOTION_RB4")
CARRIES = {"waist": lambda arm: _carry_waist(arm)}


def _carry_waist(arm):
    """The stand with the short gun held in both hands at the belt (`poses.CARRY_WAIST`)."""
    from . import aim as aiming
    from . import poses

    return aiming.hold(arm, poses.CARRY_WAIST, pocket=(0.04, -0.12, -0.36), left_along=0.32)


# The load of the food carrier's walk: a socket on the right clavicle over the shoulder
# (armature space, metres above the shoulder joint), shown in `WALKS` and hidden in every
# other clip (`key_load`).
LOAD = "socket.load"
LOAD_AT = (0.0, 0.0, 0.085)
LOAD_TILT = 12  # degrees: the roll's front end drooping towards the chest


def add_load(ch, name):
    """The load of pelts (`gear.pelt_load`) skinned rigidly to `socket.load`, a bone on the
    right clavicle over the shoulder that `key_load` scales to show or hide the load."""
    import bpy
    from mathutils import Matrix, Vector

    from . import gear

    bpy.context.view_layer.objects.active = ch.arm
    bpy.ops.object.mode_set(mode="EDIT")
    ebs = ch.arm.data.edit_bones
    at = ebs[ch.prefix + "upperarm_r"].head + Vector(LOAD_AT)
    eb = ebs.new(ch.prefix + LOAD)
    eb.head, eb.tail = at, at + Vector((0.0, 0.0, 0.1))
    eb.parent, eb.use_deform = ebs[ch.prefix + "clavicle_r"], True
    bpy.ops.object.mode_set(mode="OBJECT")
    load = gear.pelt_load(f"{name}.load")
    gear.rigid(load, ch.arm, ch.prefix + LOAD, Matrix.Translation(at + Vector((0.0, 0.07, 0.05)))
               @ Matrix.Rotation(math.radians(LOAD_TILT), 4, "X"))
    return ch.add("gear", load, "pelt")


def key_load(ch, clips):
    """Keys `socket.load` over every clip of `clips` (`bpy.data.actions` names): shown in the
    stance walks, hidden elsewhere (a scale of 0, `line.key_show`)."""
    import bpy

    from . import line

    for clip in clips:
        act = bpy.data.actions.get(clip)
        if act is not None:
            line.key_show(ch.arm, act, {LOAD: 1 if clip in WALKS else 0}, ch.prefix)


def spec(md, palette=None, build=None, fixed=(), **kw):
    """A full SPEC: the defaults with `palette` and `build` merged and the other keys replaced."""
    unknown = set(kw) - set(DEFAULTS)
    if unknown:
        raise KeyError(f"unknown SPEC keys {sorted(unknown)}")
    return dict(DEFAULTS, **kw, md=md, palette=dict(PALETTE, **(palette or {})),
                build=dict(BUILD, **(build or {})), fixed=tuple(FIXED) + tuple(fixed))


def legs(key):
    """Leggings or hose: the gaiter pattern without its garter, in cloth `key`."""

    def draft(m):
        from . import garments as gm

        panels, seams = gm.gaiters(m)
        panels = {k: p for k, p in panels.items() if k != "garter"}
        panels["gaiter"].key = key
        return panels, seams

    return draft


# -- shells over the cloth --------------------------------------------------------------------

TORSO = civil.TORSO
ARMS = civil.ARMS
LEGS = civil.LEGS
UPPER = {"upperarm_l", "upperarm_r"}


def _layer(ch, s, layer):
    if layer == "cape":
        # The hunting shirt's shoulder cape, fringed at its edge (a second, wider shell).
        ch.shell("cape", s["body"], lambda c, n, h, p: (h in TORSO | {"neck"} and c.z > 1.28)
                 or (h in UPPER and c.z > 1.30), 0.050)
        ch.shell("fringe", s["body"], lambda c, n, h, p: (h in TORSO and 1.24 < c.z < 1.31)
                 or (h in UPPER and 1.26 < c.z < 1.32), 0.062)
    elif layer == "blouse":
        ch.shell("blouse", s["body"], lambda c, n, h, p: h in TORSO and 0.90 < c.z < 1.22, 0.050)
    elif layer == "sash":
        ch.shell("sash", "fsash", lambda c, n, h, p: h in TORSO and 0.97 < c.z < 1.07, 0.058)
    elif layer == "cloak":
        def cloak(c, n, h, p):
            return (h in TORSO and c.z > 0.80 and c.y > -0.02) or (h in UPPER and c.z > 1.15 and c.y > -0.03) \
                or (h in TORSO and c.z > 1.36)

        ch.shell("cloak", "fcloak", cloak, 0.065)
    elif layer == "cuirass":
        ch.shell("cuirass", "metal", lambda c, n, h, p: h in TORSO - {"clavicle_l", "clavicle_r"} and 0.98 < c.z < 1.42, 0.068)
    elif layer == "tassets":
        ch.shell("tassets", "metal", lambda c, n, h, p: (h in LEGS | {"hips"} and 0.78 < c.z < 1.0 and c.y < 0.02), 0.080)
    elif layer == "slops":
        ch.shell("slops", "slops", lambda c, n, h, p: h in LEGS | {"hips"} and 0.66 < c.z < 1.0, 0.055)
    else:
        raise KeyError(f"frontier layer {layer!r}")


# -- strapped kit (rigid on deforming bones, rest pose; the facing is -Y, +X the left) ---------

def _bm():
    import bmesh

    return bmesh.new()


def _rigid(ch, bm, name, bone, at, key):
    from . import gear

    obj = gear.from_bm(bm, name)
    gear.rigid(obj, ch.arm, bone, at)
    return ch.add("gear", obj, key)


def _horn(ch, name, s):
    """A powder horn on the right hip, its tip forward and down, a wooden plug at the base."""
    import bmesh
    from mathutils import Matrix

    bm = _bm()
    bmesh.ops.create_cone(bm, cap_ends=True, segments=8, radius1=0.032, radius2=0.006, depth=0.26,
                          matrix=Matrix.Rotation(math.radians(90), 4, "Y"))
    for v in bm.verts:  # the horn's curve
        v.co.z += 0.9 * (v.co.x / 0.13) ** 2 * 0.03
    at = Matrix.Translation((-0.20, -0.02, 1.00)) @ Matrix.Rotation(math.radians(-25), 4, "Y") \
        @ Matrix.Rotation(math.radians(70), 4, "Z")
    # Seated on the hunting shirt over the right hip (`gear.seat`, docs/assets-humans.md,
    # "Seated gear"), skinned to the blended hip frame, lagging on its own bone.
    from . import gear

    obj = gear.from_bm(bm, f"{name}.horn")
    gear.seat(ch, obj, at, (1.0, -0.1, 0.0), turn=False)
    return ch.add("gear", obj, "horn")


def _pouch(ch, name, s):
    """The shot pouch on the right hip, behind the horn."""
    from mathutils import Matrix

    from . import gear

    box = gear.box(f"{name}.pouch", (0.05, 0.15, 0.14))
    gear.seat(ch, box, Matrix.Translation((-0.19, 0.08, 0.93)), (0.95, -0.3, 0.0))
    return ch.add("gear", box, "pouch")


def _tomahawk_belt(ch, name, s):
    """A tomahawk thrust through the sash at the left hip, the head up behind."""
    import bmesh
    from mathutils import Matrix

    bm = _bm()
    bmesh.ops.create_cone(bm, cap_ends=True, segments=6, radius1=0.013, radius2=0.011, depth=0.50,
                          matrix=Matrix.Translation((0, 0, -0.12)))
    bmesh.ops.create_cube(bm, size=1.0, matrix=Matrix.Translation((0, -0.06, 0.11)) @ Matrix.Diagonal((0.012, 0.11, 0.05, 1)))
    return _rigid(ch, bm, f"{name}.tomahawk_belt", "hips",
                  Matrix.Translation((0.17, 0.07, 1.02)) @ Matrix.Rotation(math.radians(-28), 4, "X"), "wood")


def _knife(ch, name, s):
    """A scalping knife in its sheath before the left hip."""
    from mathutils import Matrix

    from . import gear

    box = gear.box(f"{name}.knife", (0.035, 0.018, 0.24))
    gear.rigid(box, ch.arm, "hips", Matrix.Translation((0.12, -0.13, 0.95)) @ Matrix.Rotation(math.radians(12), 4, "Y"))
    return ch.add("gear", box, "pouch")


registry.gear("horn", _horn)
registry.gear("pouch", _pouch)
registry.gear("tomahawk_belt", _tomahawk_belt)
registry.gear("knife", _knife)


# -- hats (origin at the crown socket, the brim's plane; the facing is -Y) ---------------------

def fur_cap(name, n=14):
    """A round fur cap with a tail hanging at the back."""
    import bmesh
    from mathutils import Matrix

    from . import gear

    bm = _bm()
    bmesh.ops.create_cone(bm, cap_ends=True, segments=n, radius1=0.112, radius2=0.10, depth=0.14,
                          matrix=Matrix.Translation((0, 0, 0.05)))
    for v in bm.verts:
        v.co.y *= 1.12
    bmesh.ops.create_cone(bm, cap_ends=True, segments=6, radius1=0.03, radius2=0.012, depth=0.30,
                          matrix=Matrix.Translation((0.02, 0.15, -0.06)) @ Matrix.Rotation(math.radians(-15), 4, "X"))
    return gear.from_bm(bm, name)


def round_hat(name, n=16):
    """A round hat: a low crown and a broad flat brim (the frontier's wool felt)."""
    import bmesh
    from mathutils import Matrix

    from . import gear

    bm = _bm()
    bmesh.ops.create_cone(bm, cap_ends=True, segments=n, radius1=0.105, radius2=0.098, depth=0.11,
                          matrix=Matrix.Translation((0, 0, 0.055)))
    bmesh.ops.create_cone(bm, cap_ends=True, segments=n, radius1=0.19, radius2=0.19, depth=0.008)
    for v in bm.verts:
        v.co.y *= 1.08
    return gear.from_bm(bm, name)


def buccaneer_hat(name, n=16):
    """A broad hat with the brim turned up on the left (the buccaneer's)."""
    import bmesh
    from mathutils import Matrix

    from . import gear

    bm = _bm()
    bmesh.ops.create_cone(bm, cap_ends=True, segments=n, radius1=0.11, radius2=0.09, depth=0.13,
                          matrix=Matrix.Translation((0, 0, 0.065)))
    brim = bmesh.ops.create_cone(bm, cap_ends=True, segments=n, radius1=0.22, radius2=0.22, depth=0.008)
    for v in brim["verts"]:
        if v.co.x > 0.08:
            v.co.z += (v.co.x - 0.08) * 0.9
        elif v.co.xy.length > 0.15:
            v.co.z -= 0.02
    return gear.from_bm(bm, name)


registry.hat("fur_cap", fur_cap)
registry.hat("round_hat", round_hat)
registry.hat("buccaneer_hat", buccaneer_hat)


# -- the man ----------------------------------------------------------------------------------

def make(root, name, s):
    """The man of SPEC `s` under `root`: body, draped clothes, shells, kit, hat, gun."""
    from mathutils import Matrix, Vector

    from . import armature as am
    from . import drape, gear, held, line, mbody, poses
    from . import garments as gm
    from .humans import Character

    ch = Character(root, name, s["build"])
    ch.body(shoulders=False, grip=held.fists("musket" if s.get("gun") else None))  # a drafted coat
    ch.shoes = ch.add("clothes", mbody.shoes(ch.arm, ch.flesh, f"{name}.footwear", ch.prefix, key="shoes"),
                      "shoes", symmetric=True)
    ch.drape_report = {}
    ch.stand = poses.STAND_MUSKET  # a cloak is draped with these arms (`drape.arms_down`)
    draped = [DRAPED[x] for x in s["layers"] if s["drape"] and x in DRAPED]
    upper_g = [g for g in draped if g == "frn_blouse"] or ["coat"]
    patterns = {"coat": civil.peasant_coat(s["body"], s["sleeves"], s["skirts"]), "gaiters": legs(s["legs"]),
                "frn_blouse": gm.keyed(gm.GARMENTS["frn_blouse"], s["body"], s["sleeves"])}
    made = drape.dress(ch, garments=("breeches", "gaiters", *upper_g, *[g for g in draped if g != "frn_blouse"]),
                       report=ch.drape_report, patterns=patterns)
    cloth = [o for o in ch.layers["clothes"] if o.name in made]
    drape.hide_skin(ch, cloth)
    upper = {s["body"], s["sleeves"]}
    for o in cloth:
        if made[o.name] == "breeches":
            drape.hide_under(ch, o, [x for x in cloth if made[x.name] in upper | {s["legs"]}], 0.004)
    for layer in s["layers"]:
        if not (s["drape"] and layer in DRAPED):
            _layer(ch, s, layer)
    straps = [g for g in s["kit"] if g in line.STRAPS]
    if straps:
        ch.strap_surface = drape.surface(cloth, f"{name}.belt_surface")
        ch.strap_offsets = (0.020, 0.024)
        ch.strap_under = cloth
        registry.build_gear(straps, ch, name, s)
    ch.bone_layer()
    ch.caps()
    registry.build_gear([g for g in s["kit"] if g not in line.STRAPS], ch, name, s)
    ch.paint_masks = []

    am.pose(ch.arm, poses.STAND_MUSKET)
    dx, dy, dz, tilt = s["hat_at"]
    hat = registry.build_hat(s["hat"], f"{name}.{s['hat']}")
    am.attach(hat, ch.arm, "socket.hat", Matrix.Translation(am.world(ch.arm, "socket.hat").translation + Vector((dx, dy, dz)))
              @ Matrix.Rotation(math.radians(tilt), 4, "X"))
    ch.held["hat"] = hat
    ch.keys[hat.name] = "fur" if s["hat"] == "fur_cap" else ("metal" if s["hat"] in ("morion", "cabasset") else "hat")
    if s["gun"]:
        gun = gear.musket(f"{name}.musket", length=s["gun"], bayonet=0.0)
        gun["acr_material_keys"] = ["wood", "metal"]
        from . import grips

        am.attach(gun, ch.arm, "socket.hand_r", grips.hold_matrix(ch.arm, "socket.hand_r", (0.0, 0.01, 0.0)))
        ch.held["musket"] = gun
        ch.keys[gun.name] = "wood"
    if s["load"]:
        add_load(ch, name)
    return ch


def build(ctx, s, make_fn=None):
    """Builds, animates, paints and prepares for export the gunman of SPEC `s` (the
    musketeer clips of `line.py`)."""
    import bpy

    from . import aim as aiming
    from . import armature as am
    from . import crawl, faces, heads, line, outfit, poses
    from .humans import LAYER_OF, prepare_export

    md = {k: v for k, v in s["md"].items() if k in CLIPS + WALKS}
    ch = (make_fn or make)(ctx.root, ctx.name, s)
    line._musket_clips(ctx, ch, md, carry=CARRIES[s["carry"]] if s["carry"] else None,
                       walks=[k for k in WALKS if k in md], style=s["reload"],
                       gun=s["gun"] if s["reload"] else None)
    if s["load"]:
        key_load(ch, [a.name for a in bpy.data.actions if a.name.isupper()])
    am.set_clip(ch.arm, "STAND")
    report = {}
    colours = ch.colours(ctx.ref, s["palette"], report=report, mix=s["mix"], fixed=s["fixed"])
    outfit.dress(ch, colours, LAYER_OF)
    heads.finish(ch)
    ch.arm.animation_data.action = None
    for pb in ch.arm.pose.bones:
        pb.scale = (1.0, 1.0, 1.0)
    am.pose(ch.arm, poses.STAND_MUSKET)
    stowed = aiming.rod_for(s["gun"])["stowed"] if s["reload"] else aiming.ROD_STOWED
    ch.arm.pose.bones[aiming.ROD_SOCKET].matrix = ch.arm.pose.bones["socket.hand_r"].matrix @ stowed
    tris = ch.triangles()
    names = [k for k in poses.musketeer_clips(md) if k in md or k == "STAND" or k not in CLIPS] + list(crawl.CLIPS)
    names += [k for k in WALKS if k in md]
    names += [c for c in aiming.FIRE_CLIPS if c in (getattr(ctx, "fire", None) or {})]
    prepare_export(ch.all_meshes(), ctx.root, acr_triangles=sum(tris.values()), acr_clips=names,
                   acr_limb_radius=ch.limb_radius, acr_light=report.get("light_dir_camera", []),
                   acr_layers=list(ch.layers), **faces.extras("regular"))
    ctx.stats = {"triangles": tris, "light": report, "colours": colours, "aim_errors_m": ctx.aim_errors,
                 "drape": getattr(ch, "drape_report", None), "fire": getattr(ctx, "fire", None),
                 "reload": getattr(ctx, "reload", None)}


def pose_for(ch, s, clip):
    """Shows the first frame of `clip` (the last of DEATH) for the silhouette tool."""
    from . import armature as am
    from . import poses

    md = {k: v for k, v in s["md"].items() if k in CLIPS + WALKS}
    stand = getattr(ch, "stand_pose", poses.STAND_MUSKET)
    all_keys = poses.musketeer_clips({k: v for k, v in md.items() if k in CLIPS}, stand, swing=not s["carry"],
                                     style=s["reload"])
    all_keys.update(poses.step_keys({k: v for k, v in md.items() if k in WALKS}, stand, not s["carry"]))
    keys = all_keys[clip]
    ch._pose_spec = keys[-1][1] if clip == "DEATH" else keys[0][1]
    am.pose(ch.arm, ch._pose_spec)
