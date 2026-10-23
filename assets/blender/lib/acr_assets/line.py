"""European line infantry of the 18th century as one family (docs/humans-roster.md): the
sewn uniform of docs/clothing.md (shirt and stock, waistcoat, coat with facings and
turnbacks, breeches, gaiters, shoes), crossbelts, a hat, strapped gear and a musket with
the musketeer clips. A unit script is data: its `.MD` clip lines and a `SPEC` of
parameters (`spec`), then `build(ctx)` calls `line.build(ctx, SPEC)`.

`SPEC` keys (all but `md` optional; `spec` fills the defaults):
- `md`: the `.MD` animation lines by clip, as `poses.musketeer_clips` keys them;
- `palette`: colours by material key (sRGB) over `PALETTE`: the nation's coat, the
  regiment's `facings`, the `lining` of the turnbacks, `waistcoat`, `breeches`, `gaiters`,
  `belts`, `hat`, ... Sprite colours are mixed in with `mix`;
- `build`: body build (`Character` build; `garments` False builds the shells of #178);
- `fit`: part sizes fitted to the sprite silhouette (hat, musket, pack, shell skirt);
- `hat`: `tricorn`, `slouch` or `cocked` (`gear.py`);
- `gear`: strapped gear, any of `crossbelts`, `cartridge`, `knapsack`;
- `weapon`: `musket` (with rammer, aim, reload and charge clips).
"""

import math

from mathutils import Matrix, Vector

from . import aim as aiming
from . import armature as am
from . import body as bd
from . import crawl, faces, gear, heads, outfit, poses, rig
from .humans import LAYER_OF, Character, prepare_export

# The English musketeer's colours (`EngM18`): the defaults a nation overrides.
PALETTE = {
    "skin": (0.80, 0.58, 0.45), "bone": (0.85, 0.81, 0.69),
    "coat": (0.66, 0.09, 0.06), "waistcoat": (0.86, 0.85, 0.80), "breeches": (0.86, 0.85, 0.80),
    "gaiters": (0.10, 0.10, 0.10), "belts": (0.85, 0.75, 0.30), "hat": (0.07, 0.07, 0.07),
    "cartridge": (0.08, 0.07, 0.06), "knapsack": (0.40, 0.27, 0.14), "wood": (0.42, 0.24, 0.12),
    "metal": (0.55, 0.55, 0.57),
    # Draped garments (docs/clothing.md): regimental facings, the lining of the turnbacks,
    # shirt, black stock and shoes.
    "facings": (0.13, 0.17, 0.36), "lining": (0.88, 0.86, 0.80), "shirt": (0.92, 0.91, 0.88),
    "stock": (0.08, 0.08, 0.08), "shoes": (0.08, 0.07, 0.06),
}
BUILD = {"width": 1.02, "shoulders": 1.12, "limbs": 1.23, "hips": 1.0, "garments": True}
FIT = {
    "hat_z": -0.02, "hat_scale": 0.925, "musket_len": 1.69,
    "skirt_top": [0.245, 0.175], "skirt_bot": [0.21, 0.15], "coat": 0.033,
    "pack": [0.32, 0.10, 0.32], "pack_y": 0.1925, "skirt_gap": 72,
}
DEFAULTS = {"build": BUILD, "fit": FIT, "hat": "tricorn", "gear": ("crossbelts", "cartridge", "knapsack"),
            "weapon": "musket", "mix": 0.5}
HATS = {"tricorn": gear.tricorn, "slouch": gear.slouch_hat, "cocked": gear.cocked_hat}

TORSO = {"hips", "spine", "spine1", "spine2", "clavicle_l", "clavicle_r"}
ARMS = {"upperarm_l", "upperarm_r", "forearm_l", "forearm_r"}
LEGS = {"thigh_l", "thigh_r"}
SHINS = {"shin_l", "shin_r"}
FEET = {"foot_l", "foot_r", "toe_l", "toe_r"}


def spec(md, palette=None, build=None, fit=None, **kw):
    """A full SPEC: the defaults with `palette`, `build`, `fit` merged key by key and the
    other keys (`hat`, `gear`, `weapon`, `mix`) replaced."""
    unknown = set(kw) - set(DEFAULTS)
    if unknown:
        raise KeyError(f"unknown SPEC keys {sorted(unknown)}")
    return dict(DEFAULTS, **kw, md=md, palette=dict(PALETTE, **(palette or {})), build=dict(BUILD, **(build or {})),
                fit=dict(FIT, **(fit or {})))


def make(root, name, s):
    """The soldier of SPEC `s` under `root`: body, uniform, belts, strapped and held gear."""
    build, fit = s["build"], s["fit"]
    ch = Character(root, name, build)
    ch.body()

    def front(c):
        return c.y < -0.04 and abs(c.x) < 0.075 and c.z < 1.41

    if build.get("garments"):
        # Sewing patterns draped on the body (docs/clothing.md); shoes over a last.
        from . import drape, mbody

        ch.shoes = ch.add("clothes", mbody.shoes(ch.arm, ch.flesh, f"{name}.footwear", ch.prefix, key="shoes"),
                          "shoes", symmetric=True)
        ch.drape_report = {}
        made = drape.dress(ch, report=ch.drape_report)
        cloth = [o for o in ch.layers["clothes"] if o.name in made]
        drape.hide_skin(ch, cloth)
        # Covered under-layers keep a margin below the layers on them (docs/clothing.md, "Layers").
        fixed = [o for o in cloth if not o.name.endswith(("coat_skirts", "turnbacks"))]
        for inner, outer, depth in (("waistcoat", ("coat", "facings"), 0.006), ("shirt", ("coat", "facings"), 0.004),
                                    ("breeches", ("waistcoat", "gaiters"), 0.004)):
            for o in fixed:
                if made[o.name] == inner:
                    drape.hide_under(ch, o, [x for x in fixed if made[x.name] in outer], depth)
        surf = drape.surface(cloth, f"{name}.belt_surface")
        drape.buttons(ch, cloth, name)
        offsets = (0.014, 0.018)
    else:
        ch.shell("waistcoat", "waistcoat", lambda c, n, h, p: h in TORSO and 0.97 < c.z < 1.47, 0.012)
        ch.shell("coat", "coat", lambda c, n, h, p: (h in TORSO and 0.97 < c.z < 1.49 and not front(c)) or h in ARMS, fit["coat"])
        ch.shell("cuffs", "coat", lambda c, n, h, p: h in {"forearm_l", "forearm_r"} and c.z < 1.02, 0.045)
        ch.shell("breeches", "breeches", lambda c, n, h, p: (h in {"hips"} | LEGS and c.z < 1.0) or (h in SHINS and c.z > 0.40), 0.010)
        ch.shell("gaiters", "gaiters", lambda c, n, h, p: (h in SHINS and c.z <= 0.44) or h in FEET, 0.016)
        skirt = bd.tube(ch.arm, f"{name}.coat_skirt", rig.SKIRT_TOP, rig.SKIRT_BOTTOM, fit["skirt_top"], fit["skirt_bot"],
                        gap_deg=fit["skirt_gap"], flare_back=0.03, columns=rig.SKIRT_COLUMNS)
        ch.add("clothes", skirt, "coat", symmetric=True)
        surf, offsets = ch.flesh, (0.038, 0.042)
    if "crossbelts" in s["gear"]:
        n = 28 if build.get("garments") else 12  # follow the folds of the draped coat
        b1 = bd.strap(ch.arm, f"{name}.crossbelts", surf, Vector((0.12, 0, 1.47)), Vector((-0.17, 0, 0.95)), 0.06,
                      offsets[0], samples=n)
        b2 = bd.strap(ch.arm, f"{name}.crossbelt2", surf, Vector((-0.12, 0, 1.47)), Vector((0.17, 0, 0.95)), 0.06,
                      offsets[1], samples=n)
        if build.get("garments"):
            # The belts take the weights of the garment under them: they move with the coat.
            for b in (b1, b2):
                drape.inherit_weights(b, ch, fixed)
        ch.add("clothes", b1, "belts", symmetric=True)
        ch.add("clothes", b2, "belts", symmetric=True)
    ch.bone_layer()
    ch.caps()

    # Strapped gear (rest pose, rigid on deforming bones: follows `deflate`).
    if "cartridge" in s["gear"]:
        box = gear.box(f"{name}.cartridge_box", (0.16, 0.06, 0.12))
        gear.rigid(box, ch.arm, "hips", Matrix.Translation((-0.15, 0.17, 0.97)) @ Matrix.Rotation(math.radians(30), 4, "Z"))
        ch.add("gear", box, "cartridge")
    if "knapsack" in s["gear"]:
        pack = gear.box(f"{name}.knapsack", tuple(fit["pack"]))
        gear.rigid(pack, ch.arm, "spine2", Matrix.Translation((0, fit["pack_y"], 1.28)))
        ch.add("gear", pack, "knapsack")

    # Held gear on sockets, placed in the standing pose (drops at death).
    am.pose(ch.arm, poses.STAND_MUSKET)
    hat = HATS[s["hat"]](f"{name}.{s['hat']}")
    am.attach(hat, ch.arm, "socket.hat", Matrix.Translation(am.world(ch.arm, "socket.hat").translation + Vector((0, 0, fit["hat_z"])))
              @ Matrix.Rotation(math.radians(-6), 4, "X") @ Matrix.Scale(fit["hat_scale"], 4))
    ch.held["hat"] = hat
    ch.keys[hat.name] = "hat"
    if s["weapon"] == "musket":
        gun = gear.musket(f"{name}.musket", length=fit["musket_len"], bayonet=0.0)
        gun["acr_material_keys"] = ["wood", "metal"]
        # The socket's orientation is "upright" in its rest frame; the clips turn it.
        sock = am.world(ch.arm, "socket.hand_r")
        rest_up = sock.to_3x3() @ ch.arm.pose.bones["socket.hand_r"].bone.matrix_local.to_3x3().inverted()
        am.attach(gun, ch.arm, "socket.hand_r", Matrix.Translation(sock.translation + Vector((0.0, -0.01, 0.0))) @ rest_up.to_4x4())
        ch.held["musket"] = gun
        ch.keys[gun.name] = "wood"
    else:
        raise ValueError(f"line infantry weapon {s['weapon']!r}")
    return ch


def build(ctx, s):
    """Builds, animates, paints and prepares for export the soldier of SPEC `s`."""
    md = s["md"]
    ch = make(ctx.root, ctx.name, s)
    # Ramrod (in its pipes; drawn in the reload of ATTACK1): on the free left-hand socket,
    # placed on every frame by `aim.bake_musket` / `aim.stow_rammer`.
    rod = gear.rammer(f"{ctx.name}.rammer")
    am.attach(rod, ch.arm, aiming.ROD_SOCKET, am.world(ch.arm, aiming.ROD_SOCKET))
    ch.held["rammer"] = rod
    ch.keys[rod.name] = "metal"
    heads.variants(ch)  # hair and beard variants (#283), before the morphs
    ch.morphs(extra_deflate=ch.layers["gear"])
    ch.masks()
    # Present with both hands: butt in the shoulder, left hand under the barrel (IK).
    # Right elbow raised out to the side near shoulder height, left elbow under the barrel.
    aim = aiming.hold(ch.arm, poses.AIM_MUSKET, pocket=(0.06, 0.0, -0.02), elbow_r=(0.35, 0.10, -0.06))
    ctx.aim_errors = aiming.errors(ch.arm, aim)
    print("ACR aim errors (m):", ctx.aim_errors)
    # The reload drill after the shot (ATTACK1), each step IK-solved.
    reload = aiming.drill(ch.arm, poses.RELOAD_STEPS)
    # Charge bayonets: the same grip with the butt at the right hip.
    charge = aiming.hold(ch.arm, poses.CHARGE_MUSKET, pocket=(0.02, -0.12, -0.32), left_along=0.30)
    clips = poses.musketeer_clips(md, aim=aim, charge=charge, reload=reload)
    for clip, keys in clips.items():
        am.add_clip(ch.arm, clip, keys)
        if clip == "ATTACK1":
            aiming.bake_musket(ch.arm, clip, keys, rod_phases=poses.ROD_PHASES)
        else:
            aiming.stow_rammer(ch.arm, clip)
    # Crawl clips of the wounded (#262): keyframed, looping.
    for clip, keys in crawl.clips().items():
        am.add_clip(ch.arm, clip, keys)
        aiming.stow_rammer(ch.arm, clip)
    am.set_clip(ch.arm, "STAND")
    report = {}
    colours = ch.colours(ctx.ref, s["palette"], report=report, mix=s["mix"])
    # Realistic cloth, skin and leather painted over those colours (#217).
    outfit.dress(ch, colours, LAYER_OF)
    heads.finish(ch)  # face and body morphs, `_FACE`, variants (#283)
    # The exported scene pose (glTF node transforms) is STAND.
    ch.arm.animation_data.action = None
    am.pose(ch.arm, poses.STAND_MUSKET)
    ch.arm.pose.bones[aiming.ROD_SOCKET].matrix = ch.arm.pose.bones["socket.hand_r"].matrix @ aiming.ROD_STOWED
    tris = ch.triangles()
    prepare_export(ch.all_meshes(), ctx.root, acr_triangles=sum(tris.values()), acr_clips=list(md) + list(crawl.CLIPS),
                   acr_limb_radius=ch.limb_radius,
                   acr_light=report.get("light_dir_camera", []), acr_layers=list(ch.layers),
                   **faces.extras("regular"))
    ctx.stats = {"triangles": tris, "light": report, "colours": colours, "aim_errors_m": ctx.aim_errors,
                 "drape": getattr(ch, "drape_report", None)}


def pose_for(ch, s, clip):
    """Shows the first frame of `clip` (the last of DEATH) for the silhouette tool."""
    keys = poses.musketeer_clips(s["md"])[clip]
    ch._pose_spec = keys[-1][1] if clip == "DEATH" else keys[0][1]
    am.pose(ch.arm, ch._pose_spec)
