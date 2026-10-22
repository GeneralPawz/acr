"""English wooden fort (`EngKre`, `ENGKRE.MD`): a palisade square about 18 m across, rotated
45 degrees, with two blockhouses (north and south corners), a long barracks with a gate
building against the north-east wall, a long house against the north-west wall and a
house on stilts at the east corner. Section 0 (`ADDSECTION 0 0 10000`, frames 10..13, the
west corner) is the wooden corner in state 0, the stone house in scaffolding (1), the stone
house (2) and its ruin (3). Construction stages `#BUILDLO_0..3` (frames 0..3) and the ruin
`#DEATHLIE1` (frames 14, 15) are their own node groups.

States are node groups with glTF extras: `acr_state` = the animation that shows them
(`STANDLO`, `BUILDLO_0`, ..., `DEATHLIE1`), section parts `acr_section` = 0 and
`acr_section_state` = 0..3 (drawn with `STANDLO` as the game draws the section frame on the
finished building). Fire and smoke anchors (`socket.fire.<i>`, `socket.smoke.<i>`,
`socket.stage_fire.0.<i>`, `socket.stage_smoke.0.<i>`) sit where the sprite's `FIRES`,
`SMOKE`, `STAGE_FIRES` and `STAGE_SMOKE` points meet the model.

Plan in game ground coordinates (world pixels, x east, y south; heights true pixels),
measured on the sprites and fitted with tools/silhouette.py. Colours are projected from the
sprite of each state (one atlas per state, cropped to what it uses).
"""

import math

from mathutils import Vector

from acr_assets import Reference, kit, project, scene, shapes
from acr_assets import conventions as cv

TYPE = "EngKre"
CHECKS = ["--anim STANDLO --sections 0", "--anim STANDLO --sections 1", "--anim STANDLO --sections 2",
          "--anim STANDLO --sections 3", "--anim BUILDLO_0", "--anim BUILDLO_1", "--anim BUILDLO_2",
          "--anim BUILDLO_3", "--anim DEATHLIE1"]
# Building atlases are large photos of wood and stone: JPEG keeps the file small.
IMAGE_FORMAT = "JPEG"

# Picture origin = position + LOCATION; the FIRES/SMOKE points are picture coordinates.
LOCATION = (-496, -474)
FIRES = [(422, 561), (670, 457), (734, 456), (739, 457), (507, 167), (511, 165), (504, 170), (398, 171),
         (425, 455), (420, 454), (601, 372), (598, 371), (532, 337), (528, 335), (418, 74), (415, 73),
         (687, 422), (682, 417), (675, 411), (669, 404), (627, 391), (632, 391), (619, 393), (663, 397),
         (638, 389), (658, 391), (646, 389), (652, 389)]
SMOKE = [(487, 561), (490, 562), (422, 561), (419, 564), (670, 457), (665, 460), (734, 456), (739, 457),
         (507, 167), (511, 165), (504, 170), (398, 171), (425, 455), (420, 454), (601, 372), (598, 371),
         (532, 337), (528, 335), (418, 74), (415, 73), (687, 422), (682, 417), (675, 411), (669, 404),
         (627, 391), (619, 393), (638, 389), (646, 389)]
STAGE_FIRES = {0: [(232, 470), (226, 472), (236, 475), (184, 417), (180, 418), (190, 420), (142, 395), (136, 397), (147, 399)]}
STAGE_SMOKE = {0: [(275, 492), (269, 491), (279, 497), (232, 470), (226, 472), (236, 475), (184, 417), (190, 420), (136, 397)]}

FIT = {
    # Palisade corners (ground) and stake height.
    "W": [-387.0, -19.0], "S": [-38.0, 390.0], "E": [356.0, -32.0], "N": [-24.0, -398.0],
    "stake": 147.0, "dirt": 55.0,
    # North blockhouse: centre offset from N (towards the inside), sides, storeys, roof.
    "bn_in": 25.0, "bn_dx": 15.0, "bn_side": 125.0, "bn_h1": 110.0, "bn_side2": 155.0, "bn_h2": 195.0, "bn_ridge": 320.0, "bn_over": 13.0,
    # South blockhouse.
    "bs_in": 55.0, "bs_side": 140.0, "bs_h": 150.0, "bs_ridge": 270.0, "bs_over": 18.0,
    # North-east barracks (lean-to against the wall) and its gate building.
    "ne_t0": 0.20, "ne_t1": 0.90, "ne_w": 120.0, "ne_h": 75.0, "ne_high": 125.0, "ne_low": 80.0,
    "gate_t": 0.45, "gate_l": 110.0, "gate_ridge": 150.0,
    # North-west long house.
    "nw_t0": 0.18, "nw_t1": 0.62, "nw_w": 140.0, "nw_h": 80.0, "nw_ridge": 150.0,
    # East house on stilts.
    "eh_x": 291.0, "eh_y": 49.0, "eh_l": 160.0, "eh_w": 115.0, "eh_z0": 45.0, "eh_z1": 120.0, "eh_ridge": 175.0,
    # Section 0 (west corner).
    "ww_t0": 0.30, "ww_t1": 0.82, "ww_w": 120.0, "ww_h": 80.0, "ww_ridge": 150.0,
    "st_l": 260.0, "st_w": 170.0, "st_h": 140.0, "st_ridge": 230.0,
}


def P(name):
    return Vector(FIT[name])


def lerp(a, b, t):
    return a + (b - a) * t


def inward(a, b, d):
    """The point d px inside the fort from the wall a-b (the side towards the centre)."""
    c = (P("W") + P("S") + P("E") + P("N")) / 4
    m = (a + b) / 2
    n = Vector((-(b - a).y, (b - a).x)).normalized()
    if n.dot(c - m) < 0:
        n = -n
    return n * d


def angle(a, b):
    return math.degrees(math.atan2(b.y - a.y, b.x - a.x))


def along_wall(a, b, t0, t1, depth):
    """Centre, angle and length of a building against wall a-b from t0 to t1, `depth` deep."""
    p0, p1 = lerp(a, b, t0), lerp(a, b, t1)
    c = (p0 + p1) / 2 + inward(a, b, depth / 2 + 6)
    return tuple(c), angle(a, b), (p1 - p0).length


# ----------------------------------------------------------------------------- parts

def blockhouse(g, name, corner, inset, side, h1, side2, h2, ridge, over, roof=True, upper=True, lower=1.0, dx=0.0):
    W, S, E, N = P("W"), P("S"), P("E"), P("N")
    c = (W + S + E + N) / 4
    d = (c - corner).normalized()
    centre = tuple(corner + d * inset + Vector((-d.y, d.x)) * dx)
    a = angle(W, S)
    kit.box(g, f"{name}_lower", centre, a, side, side, 0.0, h1 * lower)
    if upper:
        kit.box(g, f"{name}_upper", centre, a, side2, side2, h1, h2)
    if roof:
        kit.hip_roof(g, f"{name}_roof", centre, a, side2, side2, h2, ridge, over)


def palisade_ring(g, name, keep=None, walls=("fl", "fr", "br", "bl"), t=None):
    W, S, E, N = P("W"), P("S"), P("E"), P("N")
    lines = {"fl": (W, S), "fr": (S, E), "br": (E, N), "bl": (N, W)}
    for w in walls:
        a, b = lines[w]
        t0, t1 = (t or {}).get(w, (0.0, 1.0))
        pts = [tuple(lerp(a, b, t0)), tuple(lerp(a, b, t1))]
        kit.palisade(g, f"{name}_{w}", pts, FIT["stake"], keep=keep, seed=hash(w) % 97)


def ne_barracks(g, roof=True, frame=False, wall=1.0):
    E, N = P("E"), P("N")
    c, a, ln = along_wall(N, E, FIT["ne_t0"], FIT["ne_t1"], FIT["ne_w"])
    kit.box(g, "ne_walls", c, a, ln, FIT["ne_w"], 0.0, FIT["ne_h"] * wall)
    if roof:
        kit.lean_to(g, "ne_roof", c, a + 180, ln, FIT["ne_w"], FIT["ne_low"], FIT["ne_high"])
        gc, _a, _l = along_wall(N, E, FIT["gate_t"], FIT["gate_t"] + FIT["gate_l"] / (E - N).length, FIT["ne_w"] + 30)
        kit.gable_roof(g, "gate_roof", gc, a + 90, FIT["ne_w"] + 30, FIT["gate_l"], FIT["ne_low"], FIT["gate_ridge"])
    if frame:
        kit.beams(g, "ne_rafters", kit.rafters(c, a, ln, FIT["ne_w"], FIT["ne_h"], FIT["ne_high"], 9))


def nw_house(g, roof=True, frame=False, wall=1.0, t0=None, t1=None, name="nw"):
    N, W = P("N"), P("W")
    c, a, ln = along_wall(N, W, t0 if t0 is not None else FIT["nw_t0"], t1 if t1 is not None else FIT["nw_t1"], FIT["nw_w"])
    kit.box(g, f"{name}_walls", c, a, ln, FIT["nw_w"], 0.0, FIT["nw_h"] * wall)
    if roof:
        kit.gable_roof(g, f"{name}_roof", c, a, ln, FIT["nw_w"], FIT["nw_h"], FIT["nw_ridge"])
    if frame:
        kit.beams(g, f"{name}_rafters", kit.rafters(c, a, ln, FIT["nw_w"], FIT["nw_h"], FIT["nw_ridge"], 8))


def east_house(g, roof=True, platform_only=False):
    S, E = P("S"), P("E")
    a = angle(S, E)
    c = (FIT["eh_x"], FIT["eh_y"])
    u = Vector((math.cos(math.radians(a)), math.sin(math.radians(a))))
    v = Vector((-u.y, u.x))
    L, Wd = FIT["eh_l"] / 2, FIT["eh_w"] / 2
    corners = [tuple(Vector(c) + u * su * L + v * sv * Wd) for su in (-1, 1) for sv in (-1, 1)]
    kit.posts(g, "eh_posts", corners, -5.0, FIT["eh_z0"] + 4, radius=4.0)
    kit.box(g, "eh_floor", c, a, FIT["eh_l"] + 10, FIT["eh_w"] + 10, FIT["eh_z0"], FIT["eh_z0"] + 6)
    if platform_only:
        return
    kit.box(g, "eh_walls", c, a, FIT["eh_l"], FIT["eh_w"], FIT["eh_z0"], FIT["eh_z1"])
    if roof:
        kit.gable_roof(g, "eh_roof", c, a, FIT["eh_l"], FIT["eh_w"], FIT["eh_z1"], FIT["eh_ridge"])


def west_corner_wood(g):
    """Section 0, state 0: the wooden west corner (palisade, shed, house with chimney)."""
    W, S, N = P("W"), P("S"), P("N")
    palisade_ring(g, "s0_pal", walls=("fl", "bl"), t={"fl": (0.0, 0.78), "bl": (0.45, 1.0)})
    c, a, ln = along_wall(W, S, FIT["ww_t0"], FIT["ww_t1"], FIT["ww_w"])
    kit.box(g, "ww_walls", c, a, ln, FIT["ww_w"], 0.0, FIT["ww_h"])
    kit.gable_roof(g, "ww_roof", c, a, ln, FIT["ww_w"], FIT["ww_h"], FIT["ww_ridge"])
    nw_house(g, t0=FIT["nw_t1"], t1=0.92, name="nw_end")
    ch = Vector(c) + (P("N") - P("W")).normalized() * (FIT["ww_w"] / 2 + 20)
    kit.box(g, "chimney", tuple(ch), a, 22, 22, 0.0, FIT["ww_ridge"] + 45)
    sc, sa, sl = along_wall(W, S, 0.04, 0.24, 70)
    kit.lean_to(g, "shed", sc, sa + 180, sl, 70, 75, 105)
    kit.box(g, "shed_walls", sc, sa, sl, 70, 0, 75)


def stone_house(g, state):
    """Section 0, states 1..3: the stone house in scaffolding, done, ruined."""
    W, S, N = P("W"), P("S"), P("N")
    a = angle(W, S)
    u = (S - W).normalized()
    v = (N - W).normalized()
    c = W + u * (FIT["st_l"] / 2 + 12) + v * (FIT["st_w"] / 2 + 12)
    h = FIT["st_h"]
    if state == 3:
        kit.box(g, "st_walls", tuple(c), a, FIT["st_l"], FIT["st_w"], 0.0, h * 0.45)
        kit.rubble(g, "st_rubble", tuple(c - v * 30), FIT["st_l"] * 0.55, 70, height=40)
        palisade_ring(g, "s3_pal", walls=("fl",), t={"fl": (0.62, 0.8)})
        return
    kit.box(g, "st_walls", tuple(c), a, FIT["st_l"], FIT["st_w"], 0.0, h)
    palisade_ring(g, f"s{state}_pal", walls=("fl",), t={"fl": (0.62, 0.8)})
    if state == 1:
        kit.beams(g, "st_rafters", kit.rafters(tuple(c), a, FIT["st_l"], FIT["st_w"], h, FIT["st_ridge"] - 20, 7))
        kit.beams(g, "st_scaffold", kit.scaffold(tuple(c), a, FIT["st_l"], FIT["st_w"], h, levels=3, bays=3), radius=1.6)
    else:
        kit.hip_roof(g, "st_roof", tuple(c), a, FIT["st_l"], FIT["st_w"], h, FIT["st_ridge"], 12, inset=FIT["st_w"] / 2)
        for t in (0.35, 0.6):
            p = W + u * (12 + FIT["st_l"] * t) + v * (FIT["st_w"] + 12)
            kit.box(g, f"st_chimney{t}", tuple(p), a, 24, 24, h, FIT["st_ridge"] + 40)


def dirt(g, name):
    """Trodden earth around the walls (the sprites draw it)."""
    W, S, E, N = P("W"), P("S"), P("E"), P("N")
    c = (W + S + E + N) / 4
    pts = [tuple(p + (p - c).normalized() * FIT["dirt"] * 1.4) for p in (W, S, E, N)]
    kit.ground_patch(g, name, pts)


def finished(g):
    W, S, E, N = P("W"), P("S"), P("E"), P("N")
    dirt(g, "dirt")
    palisade_ring(g, "pal", walls=("fr", "br", "bl"), t={"bl": (0.0, 0.45)})
    blockhouse(g, "bn", N, FIT["bn_in"], FIT["bn_side"], FIT["bn_h1"], FIT["bn_side2"], FIT["bn_h2"], FIT["bn_ridge"], FIT["bn_over"],
               dx=FIT["bn_dx"])
    blockhouse(g, "bs", S, FIT["bs_in"], FIT["bs_side"], FIT["bs_h"], FIT["bs_side"], FIT["bs_h"], FIT["bs_ridge"], FIT["bs_over"],
               upper=False)
    ne_barracks(g)
    nw_house(g)
    east_house(g)


def stage(g, k):
    """Construction stage k (#BUILDLO_k)."""
    W, S, E, N = P("W"), P("S"), P("E"), P("N")
    dirt(g, f"b{k}_dirt")
    if k == 0:
        palisade_ring(g, "b0_pal", t={"fl": (0.0, 0.85), "fr": (0.12, 0.45), "br": (0.0, 1.0), "bl": (0.0, 0.35)},
                      walls=("fl", "fr", "br", "bl"))
        kit.logs(g, "b0_pile", tuple(lerp(W, E, 0.5) + Vector((-60, -40))), tuple(lerp(W, E, 0.5) + Vector((40, 30))), 8, 5, 0,
                 step=0.0, spread=25)
        ne_barracks(g, roof=False, frame=True, wall=0.6)
        kit.beams(g, "b0_scaffold", [((N.x, N.y, 0), (N.x, N.y, 160)), ((N.x + 60, N.y + 40, 0), (N.x + 60, N.y + 40, 160)),
                                      ((N.x - 10, N.y - 10, 160), (N.x + 160, N.y + 100, 160))], radius=3)
    else:
        palisade_ring(g, f"b{k}_pal", t={"fr": (0.0, 1.0) if k > 1 else (0.0, 0.62)})
        ne_barracks(g, roof=k >= 2, frame=k == 1)
        nw_house(g, roof=True, t0=FIT["nw_t0"], t1=0.92)
        if k >= 2:
            east_house(g, roof=k >= 3, platform_only=k == 2)
            blockhouse(g, "bs", S, FIT["bs_in"], FIT["bs_side"], FIT["bs_h"], FIT["bs_side"], FIT["bs_h"], FIT["bs_ridge"],
                       FIT["bs_over"], upper=False, roof=False, lower=0.6 if k == 2 else 0.8)
        if k >= 3:
            blockhouse(g, "bn", N, FIT["bn_in"], FIT["bn_side"], FIT["bn_h1"], FIT["bn_side2"], FIT["bn_h2"], FIT["bn_ridge"],
                       FIT["bn_over"], roof=False, upper=False)
            c, a, ln = along_wall(W, S, FIT["ww_t0"], FIT["ww_t1"], FIT["ww_w"])
            kit.box(g, "b3_ww", c, a, ln, FIT["ww_w"], 0.0, FIT["ww_h"])
            kit.gable_roof(g, "b3_ww_roof", c, a, ln, FIT["ww_w"], FIT["ww_h"], FIT["ww_ridge"])


def ruin(g):
    """#DEATHLIE1: the destroyed fort: rubble of the west corner, broken roofs."""
    W, S, E, N = P("W"), P("S"), P("E"), P("N")
    dirt(g, "r_dirt")
    palisade_ring(g, "r_pal", t={"fl": (0.55, 1.0), "fr": (0.0, 1.0), "br": (0.0, 1.0), "bl": (0.0, 0.3)},
                  walls=("fl", "fr", "br", "bl"))
    kit.rubble(g, "r_rubble", tuple(lerp(W, S, 0.3) + Vector((40, -10))), 170, 160, height=30)
    ne_barracks(g, roof=True)
    nw_house(g, roof=False, frame=True, wall=0.7)
    blockhouse(g, "bs", S, FIT["bs_in"], FIT["bs_side"], FIT["bs_h"], FIT["bs_side"], FIT["bs_h"], FIT["bs_ridge"], FIT["bs_over"],
               upper=False)
    blockhouse(g, "bn", N, FIT["bn_in"], FIT["bn_side"], FIT["bn_h1"] * 0.7, FIT["bn_side2"], FIT["bn_h2"], FIT["bn_ridge"],
               FIT["bn_over"], roof=False, upper=False)
    east_house(g)


STATES = [
    # (group name, extras, builder, reference key)
    ("finished", {"acr_state": "STANDLO"}, finished, ("STANDLO", -1)),
    ("section0_0", {"acr_section": 0, "acr_section_state": 0}, west_corner_wood, ("STANDLO", 0)),
    ("section0_1", {"acr_section": 0, "acr_section_state": 1}, lambda g: stone_house(g, 1), ("STANDLO", 1)),
    ("section0_2", {"acr_section": 0, "acr_section_state": 2}, lambda g: stone_house(g, 2), ("STANDLO", 2)),
    ("section0_3", {"acr_section": 0, "acr_section_state": 3}, lambda g: stone_house(g, 3), ("STANDLO", 3)),
    ("build0", {"acr_state": "BUILDLO_0"}, lambda g: stage(g, 0), ("BUILDLO_0", -1)),
    ("build1", {"acr_state": "BUILDLO_1"}, lambda g: stage(g, 1), ("BUILDLO_1", -1)),
    ("build2", {"acr_state": "BUILDLO_2"}, lambda g: stage(g, 2), ("BUILDLO_2", -1)),
    ("build3", {"acr_state": "BUILDLO_3"}, lambda g: stage(g, 3), ("BUILDLO_3", -1)),
    ("ruin", {"acr_state": ["DEATHLIE1", "DEATHLIE2"]}, ruin, ("DEATHLIE1", -1)),
]


def make(root, name, only=None):
    groups = {}
    for gname, props, builder, _key in STATES:
        if only and gname not in only:
            continue
        g = kit.group(root, gname, **props)
        builder(g)
        groups[gname] = g
    return groups


def pose_for(groups, clip):
    """Silhouette tool: show the state of the reference only (section state via clip
    `STANDLO:<state>`)."""
    anim, _, sec = clip.partition(":")
    for gname, props, _b, key in STATES:
        g = groups.get(gname)
        if g is None:
            continue
        show = key[0] == anim and (key[1] == -1 or str(key[1]) == sec)
        for o in [g] + list(g.children_recursive):
            o.hide_render = not show


def _meshes(g):
    return [o for o in g.children_recursive if o.type == "MESH"]


def anchors(root, groups, finished_and_wood):
    """Fire/smoke sockets: the picture points cast through the original camera onto the
    finished fort (section 0 in the state the list belongs to); misses go to the ground."""
    import bpy

    from acr_assets.project import _Scene, canvas_ray

    class Ref:
        anchor = (0.0, 0.0)

    lists = [("fire", FIRES, "wood"), ("smoke", SMOKE, "wood"),
             ("stage_fire.0", STAGE_FIRES[0], "stone"), ("stage_smoke.0", STAGE_SMOKE[0], "stone")]
    for label, pts, which in lists:
        objs = finished_and_wood[which]
        sc = _Scene(objs)
        for i, (px, py) in enumerate(pts):
            sx, sy = px + LOCATION[0], py + LOCATION[1]
            o, d = canvas_ray(Ref, 64.0, sx - 0.5, sy - 0.5)
            hit = sc.tree.ray_cast(o, d)[0]
            if hit is None:
                # The ground point under the picture point: screen y = y / 2.
                hit = Vector(shapes.m(sx, 2 * sy, 0))
            s = scene.socket(root, f"{label}.{i}", tuple(hit))
            s["acr_anchor"] = label
    bpy.context.view_layer.update()


def build(ctx):
    groups = make(ctx.root, ctx.name)
    stats = {}
    refs = getattr(ctx, "refs", {}) or {}
    for gname, _props, _b, key in STATES:
        ref = refs.get(key[0] if key[1] < 0 else f"{key[0]}-{key[1]}")
        if ref is None and key == ("STANDLO", -1):
            ref = ctx.ref  # the default reference is #STANDLO
        g = groups[gname]
        objs = _meshes(g)
        stats[gname] = {"objects": len(objs)}
        if ref is None:
            continue
        # Project this state's sprite with only this state visible.
        for o in ctx.root.children_recursive:
            o.hide_render = True
        for o in [g] + list(g.children_recursive):
            o.hide_render = False
        if key[0] == "STANDLO" and key[1] >= 0:
            for o in [groups["finished"]] + list(groups["finished"].children_recursive):
                o.hide_render = False
        project.project(objs, ref, name=f"{ctx.name}.{gname}", crop=True)
    for o in ctx.root.children_recursive:
        o.hide_render = False
    anchors(ctx.root, groups, {"wood": _meshes(groups["finished"]) + _meshes(groups["section0_0"]),
                               "stone": _meshes(groups["finished"]) + _meshes(groups["section0_2"])})
    tris = 0
    for o in ctx.root.children_recursive:
        if o.type == "MESH":
            o.data.calc_loop_triangles()
            tris += len(o.data.loop_triangles)
    ctx.root["acr_triangles"] = tris
    ctx.stats = {"triangles": tris, "states": stats}
