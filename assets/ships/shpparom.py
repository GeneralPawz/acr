"""ShpParom: the ferry (scripted; the generated model came out as a white box on a raft).

A river ferry raft after the sprite: 10.5 m long, 5.4 m wide, of 11 peeled logs along the
length, pinned by cross beams and decked with planks across; log rails on posts down both
sides, braced with crossed ropes; a plank ramp raised at each end (bow south, -Y); a 9 m
flag pole on the port side with a red pennant, and a steering sweep over the stern. The
deck stands 0.25 m above the waterline, the logs' undersides about 0.4 m below it (the
ground point is the waterline, `acr_draft`).

Rig and clips as every ship (`ships._clips`: hull > intact / wreck; `STAND`, `MOTION_*`,
`ATTACK`, `DEATH`, `DEATHLIE1`). The original carries its passengers inside, unseen
(`acr-sim` `desant.rs`, `ship.rs`), so the deck stays empty. Stats: `triangles`,
`length_m`, `width_m`, `height_m`, `draft_m`, `size_m`, `logs`, `planks`, `posts`.
"""

import math
import random

from mathutils import Vector

from acr_assets import scripted as sc
from acr_assets import ships as sh
from acr_assets import vessels as vs

TYPE = "ShpParom"
CHECKS = ["--anim STAND"]

L, W = 10.5, 5.4  # raft length (y) and width (x), metres
R = 0.24  # log radius
DRAFT = 0.4
LOGS = 11
DECK = 0.25  # deck top above the waterline


def _box(bm, c, half, rot=0.0):
    """A box turned `rot` radians about Z, centred at `c`, with half extents `half`."""
    ca, sa = math.cos(rot), math.sin(rot)
    hx, hy, hz = half
    v = [(c[0] + x * ca - y * sa, c[1] + x * sa + y * ca, c[2] + z)
         for z in (-hz, hz) for y in (-hy, hy) for x in (-hx, hx)]
    centre = Vector(c)
    for q in ((0, 1, 3, 2), (4, 6, 7, 5), (0, 4, 5, 1), (2, 3, 7, 6), (0, 2, 6, 4), (1, 5, 7, 3)):
        sc.face(bm, [v[i] for i in q], ((0, 0), (2 * hx, 0), (2 * hx, 2 * hy), (0, 2 * hy)),
                outward=lambda m: m - centre)


def build(ctx):
    rng = random.Random(5)
    root = ctx.root
    root["acr_clock"] = "world"
    root["acr_machine"] = True
    wood_rgb, wood_h = sc.wood()
    bark_rgb, bark_h = sc.bark()
    rope_rgb, rope_h = sc.canvas()
    m_log = sc.material("parom_logs", bark_rgb * 0.85, bark_h, roughness=0.9, strength=6.0)
    m_wood = sc.material("parom_wood", wood_rgb, wood_h, roughness=0.85, strength=5.0)
    m_rope = sc.material("parom_rope", rope_rgb * 0.8, rope_h, roughness=0.9, strength=2.0)
    m_flag = vs.flat_material("parom_flag", (0.55, 0.04, 0.03))

    logs, wood, rope, flag = sc.new_bm(), sc.new_bm(), sc.new_bm(), sc.new_bm()
    zlog = DECK - 0.06 - 0.12 - R  # log centres under the cross beams and the planks
    # Logs along the length, a little uneven in length and thickness.
    for i in range(LOGS):
        x = -W / 2 + R + i * (W - 2 * R) / (LOGS - 1)
        r = R * rng.uniform(0.9, 1.05)
        y0, y1 = -L / 2 - rng.uniform(0.0, 0.35), L / 2 + rng.uniform(0.0, 0.35)
        sc.tube(logs, [(x, y0, zlog), (x, 0, zlog), (x, y1, zlog)], r, sides=7, tile=2.0, seed=i)
    # Cross beams on the logs, planks across on the beams.
    for y in (-L / 2 + 0.5, -L / 4, 0.0, L / 4, L / 2 - 0.5):
        _box(wood, (0, y, zlog + R + 0.06), (W / 2 + 0.25, 0.14, 0.06))
    planks = 0
    y = -L / 2 + 0.9
    while y < L / 2 - 0.9:
        w = rng.uniform(0.26, 0.34)
        _box(wood, (rng.uniform(-0.03, 0.03), y + w / 2, DECK - 0.03), (W / 2 - 0.15, w / 2 - 0.015, 0.03),
             rot=math.radians(rng.uniform(-0.6, 0.6)))
        planks += 1
        y += w
    # Rails: posts down both sides, two log rails, crossed ropes between the posts.
    posts = 0
    py = [-L / 2 + 1.2 + k * (L - 2.4) / 6 for k in range(7)]
    for side in (-1, 1):
        x = side * (W / 2 - 0.15)
        for y in py:
            sc.tube(wood, [(x, y, DECK - 0.1), (x, y, DECK + 1.6)], 0.08, sides=6, seed=posts)
            posts += 1
        for h in (0.75, 1.5):
            sc.tube(logs, [(x, py[0] - 0.2, DECK + h), (x, py[-1] + 0.2, DECK + h)], 0.07, sides=6, tile=2.0)
        for a, b in zip(py, py[1:]):
            sc.tube(rope, [(x, a, DECK + 0.1), (x, b, DECK + 1.45)], 0.018, sides=4)
            sc.tube(rope, [(x, a, DECK + 1.45), (x, b, DECK + 0.1)], 0.018, sides=4)
    # Ramps at both ends, hinged on the deck and raised 35 degrees on two ropes.
    t = math.radians(35)
    for end in (-1, 1):
        hinge = end * (L / 2 - 0.9)
        tip = (hinge + end * 1.8 * math.cos(t), DECK + 0.03 + 1.8 * math.sin(t))
        for k in range(8):
            x = -1.4 + (k + 0.5) * 0.35
            sc.tube(wood, [(x, hinge, DECK + 0.03), (x, tip[0], tip[1])], 0.1, sides=5, seed=50 + k)
        for x in (-1.3, 1.3):
            sc.tube(rope, [(x, tip[0], tip[1]), (x * 1.6, hinge - end * 0.5, DECK + 1.5)], 0.02, sides=4)
    # Flag pole (port side, a little aft) and its pennant.
    fx, fy, top = -W / 2 + 0.6, 1.2, DECK + 9.0
    sc.tube(wood, [(fx, fy, DECK), (fx, fy, top)], 0.09, sides=6, seed=90)
    for s in (1, -1):  # both faces: the pennant is seen from either side
        pts = [(fx, fy, top - 0.1), (fx, fy, top - 1.0), (fx + 0.15, fy + 1.6, top - 0.6)]
        sc.face(flag, pts if s > 0 else pts[::-1], ((0, 0), (0, 1), (1, 0.5)))
    # Steering sweep over the stern (north end): a long pole on a crutch, blade in the water.
    sc.tube(wood, [(0, L / 2 - 1.2, DECK), (0, L / 2 - 1.2, DECK + 1.0)], 0.07, sides=5)
    sc.tube(wood, [(-0.8, L / 2 - 3.2, DECK + 1.3), (0, L / 2 - 1.2, DECK + 1.0),
                   (0.9, L / 2 + 2.2, -0.15)], 0.06, sides=6, seed=91)
    _box(wood, (0.95, L / 2 + 2.35, -0.2), (0.05, 0.6, 0.22), rot=math.radians(-15))

    objs = [sc.to_object(logs, "parom.logs", m_log, root), sc.to_object(wood, "parom.wood", m_wood, root),
            sc.to_object(rope, "parom.rope", m_rope, root), sc.to_object(flag, "parom.flag", m_flag, root)]
    hull = vs.join(objs, f"{root.name}.hull")
    hull.parent = None
    wreck = vs.burnt_copy(hull, f"{root.name}.wreck", max(2000, vs.triangles([hull]) // 3))
    vs.cut_below(hull, -1.0)
    root["acr_draft"] = DRAFT
    arm = vs.rig(root, f"{root.name}.rig", [("hull", (0, 0, 0), None), ("intact", (0, 0, 0), "hull"),
                                              ("wreck", (0, 0, 0), "hull")])
    vs.attach(hull, arm, "intact")
    vs.attach(wreck, arm, "wreck")
    sh._clips(arm, 0, L)
    zs = [v.co.z for v in hull.data.vertices]
    ctx.stats.update({"triangles": vs.triangles([hull, wreck]), "hull": vs.triangles([hull]),
                      "wreck": vs.triangles([wreck]), "length_m": L, "width_m": W,
                      "height_m": round(max(zs), 2), "draft_m": round(-min(zs), 2), "logs": LOGS,
                      "planks": planks, "posts": posts, "size_m": sc.size(root)})
