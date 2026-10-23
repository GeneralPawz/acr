"""Registration hooks for the human families (pure Python, no Blender; docs/assets-humans.md,
"Family registry").

A family module (`line.py`, `native.py`, `meso.py`, ...) adds what it needs at import time
through these functions instead of editing the shared tables; the shared modules read the
same dict objects (`humans.LAYER_OF is registry.LAYER_OF`, `outfit.PAINTER is
registry.PAINTER`, ...). Registering a name twice with the same value is allowed (a module
imported twice); a different value raises, so two families cannot silently fight over a key.

- `material(key, layer, painter=None, fallback=None)`: a material key (`Character.add`,
  palettes): its decay layer (`humans.LAYER_OF`), the outfit painter that paints it
  (`outfit.PAINTER`; None keeps the detail-tile material) and the colour used when a
  palette leaves it out (`humans.FALLBACK`).
- `painter(name, fn=None, like=None, finish=(roughness, sheen, gloss))`: an outfit painter.
  `fn(p, n, rgb, L, seed) -> (N, 3)` linear colours (numpy; `outfit.paint`'s signature),
  or `like` an existing painter's name (`outfit.KIND`: an own atlas cell and colour,
  painted like that one).
- `garment(name, draft, cloth, ease=None, spacing=3.5)`: a sewing pattern
  (`garments.GARMENTS`): `draft(measure) -> (panels, seams)`, its sim settings
  (`garments.CLOTH`), girth ease and the drape's grid spacing (cm). Drape it with
  `drape.dress(ch, garments=(..., name))`, or pass `patterns={name: draft}` for a one-off.
- `hat(name, fn)`: `fn(object_name) -> object`, origin at the crown socket (`gear.tricorn`).
- `gear(name, fn)`: strapped gear, `fn(ch, name, spec) -> object or None`; the family calls
  `build_gear(...)` for the names its spec lists.
- `mask(name, fn)`: a painted mask over the outfit atlas (lace, body paint: `masks.py`),
  `fn(p, n, L, seed, **params) -> (N,)` weights 0..1; a character lists the masks it wears
  in `ch.paint_masks` (`masks.entry`).
"""

LAYER_OF = {
    "skin": "flesh", "hair": "hair", "bone": "bone",
    "coat": "wool", "tunic": "wool", "sash": "wool", "cloak": "wool", "facings": "wool", "lining": "wool",
    "stock": "leather", "garter": "leather",
    "waistcoat": "linen", "breeches": "linen", "shirt": "linen", "stockings": "linen",
    "gaiters": "leather", "belts": "leather", "hat": "leather", "boots": "leather", "shoes": "leather",
    "cartridge": "leather", "knapsack": "leather", "saddle": "leather", "tack": "leather",
    "wood": "wood", "metal": "metal", "brass": "metal",
    "horse": "hide", "mane": "hide", "hooves": "bone", "saddlecloth": "wool", "boots_high": "leather", "cap": "leather",
}
FALLBACK = {"brass": (0.72, 0.58, 0.30), "metal": (0.55, 0.55, 0.57), "garter": (0.07, 0.06, 0.05)}

PAINTER = {
    "skin": "skin", "hair": "skin",
    "coat": "wool", "tunic": "wool", "sash": "wool", "cloak": "wool",
    "facings": "facings", "lining": "lining", "stock": "stock",
    "waistcoat": "kersey", "breeches": "kersey", "shirt": "linen", "stockings": "linen",
    "gaiters": "gaiters", "boots": "boots", "shoes": "boots", "boots_high": "boots",
    "belts": "belts", "hat": "felt", "cap": "felt",
    "cartridge": "leather", "knapsack": "leather", "garter": "leather", "saddle": "leather", "tack": "leather",
}
KIND = {"facings": "wool", "lining": "wool", "stock": "linen"}
FINISH = {
    "wool": (1.0, 0.45, 0.0), "facings": (1.0, 0.45, 0.0), "lining": (1.0, 0.35, 0.0),
    "kersey": (1.0, 0.25, 0.0), "linen": (0.95, 0.12, 0.0), "stock": (0.95, 0.25, 0.0),
    "gaiters": (1.0, 0.30, 0.0), "felt": (1.0, 0.30, 0.0),
    "boots": (0.6, 0.0, 0.18), "belts": (0.8, 0.0, 0.05), "leather": (0.7, 0.0, 0.10), "skin": (0.75, 0.0, 0.0),
}
# Painters built into `outfit.paint` (the rest come from `painter(fn=...)`).
BUILTIN_PAINTERS = ("skin", "wool", "linen", "kersey", "gaiters", "boots", "belts", "felt", "leather")
PAINT_FN = {}  # painter name -> fn(p, n, rgb, L, seed)

# Garments beyond garments.GARMENTS' uniform are added to that same dict; drape spacing (cm).
SPACING = {"coat": 3.5, "waistcoat": 5.0, "breeches": 4.2, "gaiters": 3.5, "shirt": 2.0,
           "doublet": 3.5, "doublet_slashed": 3.5, "buffcoat": 4.0, "buffcoat_bare": 4.0, "hose": 3.5, "slops": 3.5,
           "shirt17": 2.0}

HATS = {}  # name -> fn(object_name, **kw)
MASKS = {}  # name -> fn(p, n, L, seed, **params)
GEAR = {}  # name -> fn(ch, name, spec)

LAYERS = ("flesh", "hair", "bone", "wool", "linen", "leather", "wood", "metal", "hide")


def _put(table, key, value, what):
    if key in table and table[key] != value:
        raise ValueError(f"{what} {key!r} is registered as {table[key]!r}, not {value!r}")
    table[key] = value


def material(key, layer, painter=None, fallback=None):
    """Registers material key `key` (decay `layer`, outfit `painter`, `fallback` sRGB)."""
    if layer not in LAYERS:
        raise ValueError(f"unknown decay layer {layer!r} (one of {LAYERS})")
    _put(LAYER_OF, key, layer, "material key")
    if painter is not None:
        if painter not in PAINTER.values() and painter not in BUILTIN_PAINTERS and painter not in PAINT_FN \
                and painter not in KIND:
            raise ValueError(f"painter {painter!r} is not registered")
        _put(PAINTER, key, painter, "painter of")
    if fallback is not None:
        _put(FALLBACK, key, tuple(fallback), "fallback colour of")


def painter(name, fn=None, like=None, finish=(0.9, 0.0, 0.0)):
    """Registers outfit painter `name`: its own function `fn` or painted `like` another."""
    if (fn is None) == (like is None):
        raise ValueError("painter: give exactly one of fn and like")
    if like is not None:
        if like not in BUILTIN_PAINTERS and like not in PAINT_FN:
            raise ValueError(f"painter {like!r} is not registered")
        _put(KIND, name, like, "painter")
    else:
        if name in BUILTIN_PAINTERS:
            raise ValueError(f"painter {name!r} is built in")
        _put(PAINT_FN, name, fn, "painter")
    _put(FINISH, name, tuple(finish), "finish of")


def garment(name, draft, cloth, ease=None, spacing=3.5):
    """Registers sewing pattern `name` (`draft(measure) -> (panels, seams)`)."""
    from . import garments as gm

    for k in ("thickness", "mass", "tension", "bending", "shrink", "margin"):
        if k not in cloth:
            raise KeyError(f"garment {name!r}: cloth setting {k!r} missing")
    _put(gm.GARMENTS, name, draft, "garment")
    _put(gm.CLOTH, name, dict(cloth), "cloth of")
    if ease is not None:
        _put(gm.EASE, name, float(ease), "ease of")
    _put(SPACING, name, float(spacing), "spacing of")


def hat(name, fn):
    """Registers hat builder `name`."""
    _put(HATS, name, fn, "hat")


def gear(name, fn):
    """Registers strapped-gear builder `name`."""
    _put(GEAR, name, fn, "gear")


def mask(name, fn):
    """Registers painted mask `name` (`masks.py`)."""
    _put(MASKS, name, fn, "mask")


def build_hat(name, obj_name, **kw):
    """Builds hat `name` (keyword arguments to its builder, e.g. `lace` for the tricorn)."""
    if name not in HATS:
        raise KeyError(f"hat {name!r} is not registered (have {sorted(HATS)})")
    return HATS[name](obj_name, **kw)


def build_gear(names, ch, name, spec):
    """Calls the gear builders of `names` in order; returns {name: object}."""
    unknown = [g for g in names if g not in GEAR]
    if unknown:
        raise KeyError(f"gear {unknown} is not registered (have {sorted(GEAR)})")
    out = {}
    for g in names:
        o = GEAR[g](ch, name, spec)
        if o is not None:
            out[g] = o
    return out
