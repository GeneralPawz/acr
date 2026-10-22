"""English dragoon, 18th century (`EngD18`, `ENGD18.MD`): a brown warhorse on rig R with a
red saddlecloth, and a rider on skeleton H (bones `rider.*`, its root on the horse's
`socket.saddle`) in a red coat, buff breeches, high boots and a tall cap, sabre at the hip.
One armature, so every clip moves horse and rider together; the horse has its own bone
layer and decay masks (#178), the rider all layers of the musketeer (#183). The horse is a
bay with black points, bridle and saddle on one baked 1024 px atlas (`horse.dress`, #215).

Clips (keyed on ENGD18.MD's frame counts): STAND, REST (86 frames, the horse lowers its
head and swishes its tail), MOTION_L/R (the two halves of a 14-frame trot), MOTION_LB/RB
(backwards), WALK (extra, 16 frames, four-beat walk for slow moves; not an .MD key),
PATTACK/UATTACK/PSTAND (carbine raised), ATTACK (fire), ATTACK1 (sabre cut), DEATH (the
horse falls, the rider is thrown), DEATHLIE1.
"""

import math

from mathutils import Matrix, Vector

from acr_assets import crawl
from acr_assets import armature as am
from acr_assets import body as bd
from acr_assets import decay, gear, horse, rig
from acr_assets import faces, heads
from acr_assets.humans import LAYER_OF, Character, prepare_export

TYPE = "EngD18"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

MD = {
    "STAND": "#STAND 9 1 0 0",
    "REST": "@REST 9 0 0 85",
    "MOTION_L": "@MOTION_L 9 1 0 5",
    "MOTION_R": "@MOTION_R 9 1 6 13",
    "MOTION_LB": "@MOTION_LB 9 1 13 6",
    "MOTION_RB": "@MOTION_RB 9 1 5 0",
    "WALK": "@WALK 9 1 0 15",
    "PATTACK": "#PATTACK 9 1 3 0",
    "UATTACK": "#UATTACK 9 1 3 0",
    "PSTAND": "#PSTAND 9 1 3 0",
    "ATTACK": "@ATTACK 9 3 0 61",
    "ATTACK1": "@ATTACK1 9 4 0 155",
    "DEATH": "@DEATH 9 2 0 37",
    "DEATHLIE1": "#DEATHLIE1 9 1 2 37",
}

PALETTE = {
    "skin": (0.80, 0.58, 0.45), "bone": (0.85, 0.81, 0.69), "coat": (0.70, 0.08, 0.05),
    "waistcoat": (0.85, 0.80, 0.62), "breeches": (0.85, 0.80, 0.62), "boots_high": (0.08, 0.07, 0.06),
    "belts": (0.85, 0.80, 0.62), "cap": (0.70, 0.08, 0.05), "metal": (0.70, 0.70, 0.72), "wood": (0.42, 0.24, 0.12),
    "horse": (0.33, 0.21, 0.11), "mane": (0.10, 0.07, 0.05), "saddlecloth": (0.72, 0.10, 0.06),
    "saddle": (0.25, 0.15, 0.08), "tack": (0.27, 0.15, 0.07), "hat": (0.70, 0.08, 0.05),
}

# The rider's skeleton stands 0.72 m higher and 0.10 m back: seated, the hips are over the
# saddle (socket.saddle).
RIDER = Vector((0.0, -0.10, 0.72))
OZ = RIDER.z
BUILD = {"width": 1.0, "shoulders": 1.08, "limbs": 1.12, "origin": tuple(RIDER)}

# Horse body fitted to the sprites (tools/silhouette.py): body sections scaled along the
# length about the girth, deepened and shifted.
HFIT = {"len": 1.0, "depth": 1.45, "width": 1.4, "dz": -0.225, "y0": 0.0125, "neck": 0.06}


def seat():
    """How much the back (and so saddle and rider) moves down with the fitted body."""
    return 0.31 * (HFIT["depth"] - 1.0) + HFIT["dz"]

TORSO = {"hips", "spine", "spine1", "spine2", "clavicle_l", "clavicle_r"}
ARMS = {"upperarm_l", "upperarm_r", "forearm_l", "forearm_r"}
LEGS = {"thigh_l", "thigh_r"}
SHINS = {"shin_l", "shin_r"}
FEET = {"foot_l", "foot_r", "toe_l", "toe_r"}


def table():
    t = rig.rig_r()
    for b in rig.skeleton_h("rider.", tuple(RIDER + Vector((0, 0, seat())))):
        if b.name == "rider.root":
            b = b._replace(parent="socket.saddle")
        t.append(b)
    return t


# ----------------------------------------------------------------------------- poses

SEAT = {
    "thigh_l": (-78, -22, 0), "shin_l": (82, 0, 0), "foot_l": (-10, 0, 0),
    "thigh_r": (-78, 22, 0), "shin_r": (82, 0, 0), "foot_r": (-10, 0, 0),
    "upperarm_l": (-28, 22, 0), "forearm_l": (-55, 0, -15),
    "upperarm_r": (-10, -20, 0), "forearm_r": (-30, 0, 10),
    "spine": (-4, 0, 0), "head": (4, 0, 0),
    "world": {"socket.hand_r": (0, 0, 0)},
}
AIM = dict(SEAT, upperarm_r=(-70, -30, -20), forearm_r=(-40, 0, 30), upperarm_l=(-75, 45, 25), forearm_l=(-15, 0, -15),
           head=(8, 0, -10), world={"socket.hand_r": (88, 0, 0)})
CUT_UP = dict(SEAT, upperarm_r=(-150, -20, 0), forearm_r=(-20, 0, 0), spine=(4, 0, -10), world={"socket.hand_r": (-40, 0, 0)})
CUT_DOWN = dict(SEAT, upperarm_r=(-60, -10, 0), forearm_r=(-30, 0, 0), spine=(-14, 0, 15), world={"socket.hand_r": (110, 0, 0)})


def thrown(t):
    """The rider leaving the saddle (t 0..1) and lying beside the horse (t = 1)."""
    lie = {k: v for k, v in rig_lie().items()}
    if t >= 1.0:
        return lie
    return dict(SEAT, spine=(-30 * t, 0, 0), upperarm_l=(-120 * t, 40, 0), upperarm_r=(-120 * t, -40, 0),
                thigh_l=(-50, -30, 0), thigh_r=(-50, 30, 0))


def rig_lie():
    """Rider lying prone on the ground left of the horse (armature space)."""
    return {
        "world": {"root": (90, 0, 20)}, "world_loc": {"root": (-1.3, -1.1, 0.13)},
        "upperarm_l": (12, -45, 0), "forearm_l": (0, -16, 0), "upperarm_r": (-2, 47, 0), "forearm_r": (0, 50, 0),
        "thigh_l": (-14, -6, 0), "thigh_r": (-10, 8, 0), "shin_l": (14, 0, 0), "shin_r": (15, 0, 0),
        "head": (-10, 18, 62), "foot_l": (55, 0, 0), "foot_r": (50, 0, 0),
    }


def clips():
    """{clip: [(frame, horse pose, rider pose)]}."""
    trot = [(i / 14.0, horse.gait(i / 14.0)) for i in range(15)]
    walk = [(i / 16.0, horse.gait(i / 16.0, trot=False)) for i in range(17)]
    rest = [(0.0, horse.STAND), (0.3, horse.REST), (0.7, horse.REST), (1.0, horse.STAND)]
    keys = {
        "STAND": [(0.0, horse.STAND, SEAT)],
        "REST": [(t, h, SEAT) for t, h in rest],
        "MOTION_L": [(t * 14 / 6, h, SEAT) for t, h in trot if t <= 6 / 14 + 1e-6],
        "MOTION_R": [((t - 6 / 14) * 14 / 8, h, SEAT) for t, h in trot if t >= 6 / 14 - 1e-6],
        "WALK": [(t, h, SEAT) for t, h in walk],
        "PATTACK": [(0.0, horse.STAND, AIM)],
        "UATTACK": [(0.0, horse.STAND, AIM)],
        "PSTAND": [(0.0, horse.STAND, AIM)],
        "ATTACK": [(0.0, horse.STAND, AIM), (0.08, horse.STAND, dict(AIM, spine=(4, 0, 0))), (0.25, horse.STAND, AIM),
                   (1.0, horse.STAND, AIM)],
        "ATTACK1": [(0.0, horse.STAND, SEAT), (0.3, horse.STAND, CUT_UP), (0.45, horse.STAND, CUT_DOWN), (0.7, horse.STAND, SEAT),
                    (1.0, horse.STAND, SEAT)],
        "DEATH": [(0.0, horse.STAND, SEAT), (0.35, horse.falling(), thrown(0.5)), (0.75, horse.fallen(), thrown(0.9)),
                  (1.0, horse.fallen(), thrown(1.0))],
        "DEATHLIE1": [(0.0, horse.fallen(), thrown(1.0))],
    }
    keys["MOTION_LB"] = [(1.0 - t, h, r) for t, h, r in keys["MOTION_R"]][::-1]
    keys["MOTION_RB"] = [(1.0 - t, h, r) for t, h, r in keys["MOTION_L"]][::-1]
    out = {}
    for name, ks in keys.items():
        n = rig.md_clip_frames(MD[name])[0]
        out[name] = [(int(round(t * (n - 1))), h, r) for t, h, r in ks]
    return out


def add_clips(arm):
    for name, ks in clips().items():
        horse_keys, rider_keys = [], []
        for f, h, r in ks:
            if horse_keys and horse_keys[-1][0] == f:
                horse_keys[-1], rider_keys[-1] = (f, h), (f, r)
            else:
                horse_keys.append((f, h))
                rider_keys.append((f, r))
        am.add_clip(arm, name, None, prefix_keys={"": horse_keys, "rider.": rider_keys})
    # Crawl clips (#262): the dismounted rider crawls where DEATHLIE1 throws him, beside
    # the fallen horse; his root is placed in armature space ("world"/"world_loc"), since
    # in the combined rig it hangs from the saddle.
    lie_loc = rig_lie()["world_loc"]["root"]
    for name, ks in crawl.clips().items():
        rider_keys = []
        for f, spec in ks:
            spec = dict(spec)
            loc = spec.pop("root_loc")
            spec["world"] = {"root": spec.pop("root")}
            spec["world_loc"] = {"root": (lie_loc[0] + loc[0], lie_loc[1] + loc[1] - crawl.PRONE_LOC[1], loc[2])}
            rider_keys.append((f, spec))
        horse_keys = [(f, horse.fallen()) for f, _s in ks]
        am.add_clip(arm, name, None, prefix_keys={"": horse_keys, "rider.": rider_keys})


# ----------------------------------------------------------------------------- model

def make(root, name):
    global OZ
    OZ = RIDER.z + seat()
    build = dict(BUILD, origin=tuple(RIDER + Vector((0, 0, seat()))))
    ch = Character(root, name, build, prefix="rider.", table=table())
    P = "rider."
    ch.body()
    ch.shell("waistcoat", "waistcoat", lambda c, n, h, p: h in TORSO and 0.97 + OZ < c.z < 1.47 + OZ, 0.012)
    ch.shell("coat", "coat", lambda c, n, h, p: (h in TORSO and 0.97 + OZ < c.z < 1.49 + OZ and not (c.y < -0.14 and abs(c.x) < 0.07))
             or h in ARMS, 0.03)
    ch.shell("breeches", "breeches", lambda c, n, h, p: (h in {"hips"} | LEGS and c.z < 1.0 + OZ), 0.010)
    ch.shell("boots", "boots_high", lambda c, n, h, p: h in SHINS | FEET or (h in LEGS and c.z < 0.58 + OZ), 0.02)
    skirt = bd.tube(ch.arm, f"{name}.coat_skirt", rig.SKIRT_TOP + OZ, 0.72 + OZ, (0.20, 0.15), (0.24, 0.20), gap_deg=80,
                    flare_back=0.03, columns=rig.SKIRT_COLUMNS, centre=(RIDER.x, RIDER.y), prefix=P)
    ch.add("clothes", skirt, "coat", symmetric=True)
    belt = bd.strap(ch.arm, f"{name}.belt", ch.flesh, Vector((0.12, 0, 1.47 + OZ)) + Vector((0, RIDER.y, 0)),
                    Vector((-0.17, 0, 0.95 + OZ)) + Vector((0, RIDER.y, 0)), 0.06, 0.04, bones=[P + b for b in rig.BODY_BONES])
    ch.add("clothes", belt, "belts", symmetric=True)
    ch.bone_layer()
    ch.caps()

    # Horse.
    secs = []
    for i, (y, z, w, h) in enumerate(horse.SECTIONS):
        dz = HFIT["dz"] - (HFIT["neck"] if i >= 7 else 0.0)
        secs.append((y * HFIT["len"] + HFIT["y0"], z + dz, w * HFIT["width"], h * HFIT["depth"]))
    hull = horse.hull(ch.arm, f"{name}.horse", sections=secs)
    mane = horse.mane_tail(ch.arm, f"{name}.mane", sections=secs)
    cloth = horse.saddlecloth(hull, f"{name}.saddlecloth")
    sad = horse.saddle(f"{name}.saddle")
    gear.rigid(sad, ch.arm, "spine_02", Matrix.Translation((0, 0, seat())), part=horse.HORSE_PART)
    hbones = horse.bones(ch.arm, f"{name}.horse_bones", secs)
    brd = horse.bridle(ch.arm, f"{name}.bridle", secs)
    ch.horse = {"hull": hull, "mane": mane, "cloth": cloth, "saddle": sad, "bridle": brd, "bones": hbones}
    ch.horse_sections = secs

    # Held gear: cap and sabre (drop at death), placed in the seated pose.
    am.pose(ch.arm, SEAT, prefix=P)
    cap = gear.box(f"{name}.cap", (0.15, 0.17, 0.26), bevel=0.3)
    for v in cap.data.vertices:
        if v.co.z > 0:
            v.co.x *= 0.55
            v.co.y *= 0.7
    am.attach(cap, ch.arm, P + "socket.hat", Matrix.Translation(am.world(ch.arm, P + "socket.hat").translation + Vector((0, 0, 0.10))))
    ch.held["cap"] = cap
    ch.keys[cap.name] = "cap"
    sab = gear.sabre(f"{name}.sabre")
    am.attach(sab, ch.arm, P + "socket.hip_l", Matrix.Translation(am.world(ch.arm, P + "socket.hip_l").translation)
              @ Matrix.Rotation(math.radians(40), 4, "X"))
    ch.held["sabre"] = sab
    ch.keys[sab.name] = "metal"
    return ch


def pose_for(ch, clip):
    ks = clips()[clip]
    f, h, r = ks[-1] if clip == "DEATH" else ks[0]
    am.pose(ch.arm, h)
    am.pose(ch.arm, r, prefix="rider.")
    ch._pose_spec = h


def build(ctx):
    ch = make(ctx.root, ctx.name)
    H = ch.horse
    heads.variants(ch)  # hair and beard variants (#283), before the morphs
    ch.morphs()
    rider = ch.layers["flesh"] + ch.layers["clothes"]
    for o in (H["hull"], H["mane"], H["cloth"], H["saddle"], H["bridle"]):
        bd.toward_bones(o, ch.arm, "deflate", 0.6 if o is not H["saddle"] else 0.4, horse.DEFORM)
    ch.add("flesh", H["hull"], "horse", symmetric=True)
    ch.add("flesh", H["mane"], "mane", symmetric=True)
    ch.add("clothes", H["cloth"], "saddlecloth", symmetric=True)
    ch.add("gear", H["saddle"], "saddle", symmetric=True)
    ch.add("gear", H["bridle"], "tack", symmetric=True)
    ch.add("bones", H["bones"], "bone")
    ch.masks()
    add_clips(ch.arm)
    am.set_clip(ch.arm, "STAND")
    report = {}
    colours = ch.colours(ctx.ref, PALETTE, report=report)
    # The horse gets a realistic baked coat and tack (#215) instead of sprite colours.
    horse.dress(ch, ch.horse_sections, PALETTE["saddlecloth"], LAYER_OF)
    heads.finish(ch, rider)  # the rider's face and body morphs, `_FACE`, variants (#283)
    ch.arm.animation_data.action = None
    pose_for(ch, "STAND")
    tris = ch.triangles()
    prepare_export(ch.all_meshes(), ctx.root, acr_triangles=sum(tris.values()), acr_clips=list(MD) + list(crawl.CLIPS), acr_limb_radius=ch.limb_radius,
                   acr_layers=list(ch.layers), acr_extra_clips=["WALK"], **faces.extras("regular"))
    ctx.stats = {"triangles": tris, "colours": colours}
