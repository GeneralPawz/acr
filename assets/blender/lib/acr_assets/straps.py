"""Belts drawn over the clothes as registered gear (`registry.gear`; docs/assets-humans.md,
"Family registry"): the line infantry's crossbelts and single shoulder belts (the colour
belt, the drum's carriage belt, a shot-pouch strap).

A strap follows a surface: before calling `registry.build_gear` for straps the family sets
`ch.strap_surface` (the draped cloth's joined surface, or the body), `ch.strap_offsets`
(the first and second belt's distance off it) and `ch.strap_under` (the draped garments
whose weights the belts take, so they move with the coat; None on shells). Straps are built
with the cloth, before `bone_layer` (`line.STRAPS`).
"""

from mathutils import Vector
from mathutils.bvhtree import BVHTree

from . import body as bd
from . import registry

LEFT_SHOULDER, RIGHT_HIP = Vector((0.12, 0, 1.47)), Vector((-0.17, 0, 0.95))
RIGHT_SHOULDER, LEFT_HIP = Vector((-0.12, 0, 1.47)), Vector((0.17, 0, 0.95))


def belt(ch, obj_name, frm, to, offset, width=0.06, key="belts"):
    """One belt from `frm` to `to` over `ch.strap_surface`, `offset` m off it."""
    from . import drape

    garments = ch.strap_under is not None
    n = 28 if garments else 12  # follow the folds of the draped coat
    b = bd.strap(ch.arm, obj_name, ch.strap_surface, frm, to, width, offset, samples=n)
    if garments:
        drape.inherit_weights(b, ch, ch.strap_under)
    return b


def crossbelts(ch, name, s):
    """Two buff belts crossing on the chest (sword and cartridge box), as EngM18 wears them."""
    o1, o2 = ch.strap_offsets
    b1 = belt(ch, f"{name}.crossbelts", LEFT_SHOULDER, RIGHT_HIP, o1)
    b2 = belt(ch, f"{name}.crossbelt2", RIGHT_SHOULDER, LEFT_HIP, o2)
    ch.add("clothes", b1, "belts", symmetric=True)
    ch.add("clothes", b2, "belts", symmetric=True)
    return b1


def belt_l(ch, name, s):
    """One belt over the left shoulder to the right hip (the colour belt, a pouch strap)."""
    b = belt(ch, f"{name}.belt_l", LEFT_SHOULDER, RIGHT_HIP, ch.strap_offsets[0])
    return ch.add("clothes", b, "belts", symmetric=True)


def belt_r(ch, name, s):
    """One belt over the right shoulder to the left hip (the drum's carriage belt)."""
    b = belt(ch, f"{name}.belt_r", RIGHT_SHOULDER, LEFT_HIP, ch.strap_offsets[0])
    return ch.add("clothes", b, "belts", symmetric=True)


# Straps every family may list in its spec's `gear` (built with the cloth).
STRAPS = ("crossbelts", "belt_l", "belt_r")
registry.gear("crossbelts", crossbelts)
registry.gear("belt_l", belt_l)
registry.gear("belt_r", belt_r)


# -- carrying straps of seated gear (`gear.seat`; docs/assets-humans.md, "Seated gear") ----

CARRY_OFFSET = 0.004  # m over the outermost surface (clothes, belts) under the strap


def clear_folds(strap, surf, offset, k=5):
    """Lifts a strap laid by `bd.strap` (rows of edge pairs) wherever the surface under it
    rises between its edges or between its rows (a draped shirt's folds): `k` rays across
    each row and half way to the next, both pairs moved out until all clear by `offset`."""
    me = surf.data
    tree = BVHTree.FromPolygons([v.co for v in me.vertices], [p.vertices for p in me.polygons])
    ymid = sum(v.co.y for v in me.vertices) / max(len(me.vertices), 1)
    V = strap.data.vertices
    n = len(V) // 2
    lift = [0.0] * n
    for i in range(n):
        a, b = V[2 * i].co.copy(), V[2 * i + 1].co.copy()
        sign = 1.0 if (a.y + b.y) / 2 > ymid else -1.0
        nxt = i + 1 if i + 1 < n and ((V[2 * i + 2].co.y + V[2 * i + 3].co.y) / 2 > ymid) == (sign > 0) else None
        rows = [(a, b, (i,))]
        if nxt is not None:
            rows.append(((a + V[2 * nxt].co) / 2, (b + V[2 * nxt + 1].co) / 2, (i, nxt)))
        for p0, p1, who in rows:
            for t in range(k + 1):
                p = p0 + (p1 - p0) * (t / k)
                hit, _n, _i, _d = tree.ray_cast(Vector((p.x, sign * 1.0, p.z)), Vector((0.0, -sign, 0.0)))
                if hit is None:
                    continue
                need = (hit.y + sign * offset - p.y) * sign
                for j in who:
                    lift[j] = max(lift[j], need)
    for i, d in enumerate(lift):
        if d > 0.0:
            s = 1.0 if (V[2 * i].co.y + V[2 * i + 1].co.y) / 2 > ymid else -1.0
            V[2 * i].co.y += s * d
            V[2 * i + 1].co.y += s * d
    strap.data.update()


def carry(ch, obj_name, frm, to, width=0.035, key="belts", offset=CARRY_OFFSET, samples=24):
    """A carrying strap from `frm` to `to` (XZ line, front and back like `belt`) laid over the
    outermost surface: the skin, the clothes and the belts on them, `offset` off it; it takes
    the weights of the surface under it and joins the gear layer (the item seated on it
    afterwards rests on it)."""
    from . import drape

    under = [o for o in ch.layers["clothes"] if o.type == "MESH" and ch.keys.get(o.name) != "shoes"]
    surf = drape.surface([ch.flesh, *under], f"{obj_name}.surface")
    a, b = Vector((frm[0], 0.0, frm[-1])), Vector((to[0], 0.0, to[-1]))  # (x, z) or (x, y, z)
    b = bd.strap(ch.arm, obj_name, surf, a, b, width, offset, samples=samples)
    clear_folds(b, surf, offset)
    drape.inherit_weights(b, ch, under)
    return ch.add("gear", b, key, symmetric=True)
