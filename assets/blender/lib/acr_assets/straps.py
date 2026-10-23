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
