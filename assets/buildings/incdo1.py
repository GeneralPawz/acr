"""Inca dwelling `IncDo1` (`INCDO1.MD`, `#STANDLO` = frames 4 and 5 of `INCDO1.GP`): a
stone-walled hut with a thatched hip roof, a ridge pole and a net-drying frame.

The plan comes from the sprite: a silhouette fit of walls + roof (+ poles) projected through
the original camera (`acr model-check` IoU 0.90 for this primitive set). Lengths are world
pixels (30 per metre): the hut is 185 x 96 px (6.2 x 3.2 m), eaves at 70 px (2.3 m), ridge
at 99 px (3.3 m), long axis 46 degrees from east (north-east on screen).
"""

from acr_assets import materials, project, scene, shapes

TYPE = "IncDo1"

# Fitted plan (world pixels, game axes: x east, y south; z up; angles like RealDir).
CENTRE = (-8.6, 24.1)
ANGLE = -46.0
LENGTH, WIDTH = 185.0, 96.0
EAVE, RIDGE = 70.0, 98.6
OVERHANG_END, OVERHANG_SIDE = 11.2, 11.0
RIDGE_INSET = 12.1
PLINTH = 30.0
POLE_Z = 126.0
NET_POSTS = ((36.0, 165.0), (90.5, 120.0))  # (position along the ridge, top)
POLE_POST = -88.0

STONE = ("stone", (0.42, 0.44, 0.38))
WOOD = ("wood", (0.36, 0.27, 0.17))
THATCH = ("thatch", (0.70, 0.62, 0.40))
NET = ("net", (0.75, 0.75, 0.70))


def build(ctx):
    root = ctx.root
    parts = []  # (object, (material name, flat sRGB colour))

    def add(obj, colour):
        parts.append((obj, colour))
        return obj

    add(shapes.box(root, "plinth", CENTRE, ANGLE, LENGTH, WIDTH, 0.0, PLINTH), STONE)
    add(shapes.box(root, "walls", CENTRE, ANGLE, LENGTH - 6, WIDTH - 6, 0.0, EAVE), WOOD)
    for i, (su, sv) in enumerate(((-1, -1), (1, -1), (1, 1), (-1, 1))):
        c = shapes.along(CENTRE, ANGLE, su * (LENGTH / 2 - 8))
        c = shapes.along(c, ANGLE + 90, sv * (WIDTH / 2 - 8))
        add(shapes.box(root, f"pillar{i}", c, ANGLE, 16, 16, 0.0, EAVE), STONE)
    for side in (-1, 1):
        for j in range(1, 6):
            c = shapes.along(CENTRE, ANGLE, -LENGTH / 2 + j * LENGTH / 6)
            c = shapes.along(c, ANGLE + 90, side * (WIDTH / 2 - 1))
            add(shapes.post(root, f"post{side + 1}{j}", c, PLINTH, EAVE, radius=2.5), WOOD)
    add(
        shapes.hip_roof(
            root,
            "roof",
            CENTRE,
            ANGLE,
            LENGTH + 2 * OVERHANG_END,
            WIDTH + 2 * OVERHANG_SIDE,
            EAVE,
            RIDGE,
            RIDGE_INSET,
        ),
        THATCH,
    )
    # Ridge pole on a short post at the front end and the first net post.
    p0 = shapes.along(CENTRE, ANGLE, POLE_POST - 10)
    p1 = shapes.along(CENTRE, ANGLE, NET_POSTS[0][0] + 6)
    add(shapes.beam(root, "pole", (*p0, POLE_Z), (*p1, POLE_Z)), WOOD)
    pp = shapes.along(CENTRE, ANGLE, POLE_POST)
    add(shapes.post(root, "pole_post", pp, RIDGE - 25, POLE_Z + 4, radius=1.8), WOOD)
    tops = []
    for i, (s, top) in enumerate(NET_POSTS):
        p = shapes.along(CENTRE, ANGLE, s)
        add(shapes.post(root, f"net_post{i}", p, RIDGE - 25, top, radius=2.0), WOOD)
        tops.append((*p, top))
    add(shapes.beam(root, "net_bar", tops[0], tops[1], radius=1.2), WOOD)
    (ax, ay, at), (bx, by, bt) = tops
    add(shapes.quad(root, "net", [(ax, ay, RIDGE), (bx, by, RIDGE), (bx, by, bt - 6), (ax, ay, at - 6)]), NET)

    scene.socket(root, "roof", shapes.m(*CENTRE, RIDGE))

    if ctx.ref:
        project.project([o for o, _ in parts], ctx.ref)
    else:
        for obj, colour in parts:
            materials.assign(obj, materials.flat(*colour))
