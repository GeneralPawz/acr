"""Corpse layers: decay (#178) and wound/blood/dirt/burn masks (#183).

One parameter `decay` (0 fresh .. 1 gone) drives every layer of a corpse; each material
carries its layer's windows in the glTF material extras (`acr_decay`). Per vertex:

- `COLOR_0` = colour attribute `decay_mask` (the in-game renderer reads COLOR_0 of a
  material with `acr_decay` extras as this mask, never as a colour; generic glTF viewers
  multiply it into the base colour): R low-frequency blotch noise, rank-normalised
  per mesh (a threshold x removes the fraction x of the surface, so dissolving runs evenly
  in time), G per-vertex white noise (frays the edges), B layer code / 8, A 1.
- `_GORE` (vec4, glTF custom attribute) = noise for the masks of #183, each in 0..1 and
  rank-normalised per mesh: x wounds (small blobs: a wound shows where the distance to the
  hit point < radius · (0.5 + x)), y blood (soaks outwards from wounds and downwards: blood
  where y < amount), z dirt (grows with the corpse's age and from the ground up: dirt where
  z < amount − world height · k), w burn (charring: burned where w < amount).
- `_PART` (float) = body part id (`rig.PARTS`; stump caps 16 + joint index for the
  parent side, 32 + joint index for the child side), for cutting limbs off (#183).

The shader (Blender: `material`; the same in WGSL):

    mask   = 0.75 * color.r + 0.25 * color.g
    colour = mix(base, tint, k * ramp(decay, d0, d1))
    p      = ramp(decay, s0, s1);  th = p * (1 + edge)
    if (mask <= th) { discard; }
    if (p > 0 && mask - th < edge) { colour = edge_rgb; }

`ramp(x, a, b) = clamp((x - a) / (b - a), 0, 1)`. Timeline and death-counter mapping in
docs/assets-humans.md. The module's tables are pure Python; the functions that touch
Blender import `bpy` when called.
"""

import random

# Per layer: discolour window `d`, its tint (sRGB) and strength `k`, dissolve window `s`
# (None = never), dark edge width and colour.
LAYERS = {
    "flesh":   dict(code=0, d=(0.00, 0.22), tint=(0.47, 0.45, 0.33), k=0.85, s=(0.20, 0.52),
                    edge=0.07, edge_rgb=(0.16, 0.09, 0.06)),
    "hair":    dict(code=1, d=(0.20, 0.50), tint=(0.30, 0.27, 0.22), k=0.5, s=(0.42, 0.70),
                    edge=0.04, edge_rgb=(0.12, 0.10, 0.08)),
    "linen":   dict(code=2, d=(0.05, 0.45), tint=(0.50, 0.46, 0.36), k=0.7, s=(0.36, 0.74),
                    edge=0.05, edge_rgb=(0.20, 0.16, 0.11)),
    "wool":    dict(code=3, d=(0.10, 0.55), tint=(0.25, 0.17, 0.12), k=0.55, s=(0.44, 0.84),
                    edge=0.05, edge_rgb=(0.10, 0.06, 0.04)),
    "leather": dict(code=4, d=(0.20, 0.70), tint=(0.12, 0.09, 0.06), k=0.6, s=(0.74, 0.95),
                    edge=0.04, edge_rgb=(0.05, 0.04, 0.03)),
    "wood":    dict(code=5, d=(0.30, 0.95), tint=(0.22, 0.16, 0.10), k=0.5, s=None,
                    edge=0.0, edge_rgb=(0, 0, 0)),
    "metal":   dict(code=6, d=(0.30, 0.95), tint=(0.36, 0.21, 0.12), k=0.6, s=None,
                    edge=0.0, edge_rgb=(0, 0, 0)),
    "bone":    dict(code=7, d=(0.50, 0.85), tint=(0.86, 0.85, 0.80), k=0.8, s=(0.94, 1.00),
                    edge=0.03, edge_rgb=(0.45, 0.42, 0.35)),
    # Animal hide with its hair (horse coat, mane, tail; #212): rots darker and browner,
    # never paler (the human `flesh` tint is a pale grey-green that bleaches a brown coat),
    # and holds out longer than human flesh.
    "hide":    dict(code=8, d=(0.04, 0.50), tint=(0.16, 0.11, 0.07), k=0.6, s=(0.30, 0.72),
                    edge=0.06, edge_rgb=(0.09, 0.05, 0.03)),
}

# Colours of the masks of #183 (sRGB): fresh blood darkens to brown with age.
GORE = {
    "wound": (0.30, 0.02, 0.02),
    "blood_fresh": (0.45, 0.02, 0.02),
    "blood_old": (0.20, 0.08, 0.05),
    "dirt": (0.27, 0.22, 0.15),
    "burn": (0.06, 0.05, 0.04),
    "stump_meat": (0.55, 0.10, 0.08),
    "stump_bone": (0.88, 0.85, 0.75),
}

# Flesh and clothes collapse onto the bones (morph target `deflate`).
DEFLATE_WINDOW = (0.18, 0.55)
# The whole corpse sinks by SINK metres over this window.
SINK_WINDOW = (0.78, 1.00)
SINK = 0.20
# Decay starts where the original turns the corpse half transparent (death counter 100) and
# ends at the type's removal tick (1200, or 2000 with #DEATHLIE2).
FADE_START = 100


def ramp(x, a, b):
    return min(max((x - a) / (b - a), 0.0), 1.0)


def decay_from_counter(counter, remove=1200):
    return ramp(counter, FADE_START, remove)


def deflate_weight(decay):
    return ramp(decay, *DEFLATE_WINDOW)


def sink_offset(decay):
    return -SINK * ramp(decay, *SINK_WINDOW)


def srgb_to_linear(c):
    return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4


def lin(rgb):
    return tuple(srgb_to_linear(c) for c in rgb) + (1.0,)


def extras(layer):
    """The material extras `acr_decay` of a layer."""
    p = LAYERS[layer]
    return {"layer": layer, "code": p["code"], "discolour": list(p["d"]), "tint": list(p["tint"]),
            "tint_strength": p["k"], "dissolve": list(p["s"]) if p["s"] else [],
            "edge": p["edge"], "edge_rgb": list(p["edge_rgb"])}


# ----------------------------------------------------------------------------- masks

def _rank(values):
    order = sorted(range(len(values)), key=values.__getitem__)
    out = [0.0] * len(values)
    for i, vi in enumerate(order):
        out[vi] = (i + 0.5) / max(len(values), 1)
    return out


def add_masks(obj, layer, seed, part_of_vertex=None):
    """Writes `decay_mask` (COLOR_0), `_gore` and `_part` from the rest positions."""
    from mathutils import Vector, noise

    mesh = obj.data
    rnd = random.Random(seed)

    def field(freq, octave=0.5):
        off = Vector((rnd.uniform(0, 100), rnd.uniform(0, 100), rnd.uniform(0, 100)))
        return _rank([noise.noise(v.co * freq + off) + octave * noise.noise(v.co * freq * 2.3 + off)
                      for v in mesh.vertices])

    blotch = field(7.0)
    wound, blood, dirt, burn = field(22.0, 0.3), field(5.0), field(9.0), field(4.0)
    code = LAYERS[layer]["code"] / 8.0
    attr = mesh.color_attributes.get("decay_mask") or mesh.color_attributes.new("decay_mask", "FLOAT_COLOR", "POINT")
    for i, d in enumerate(attr.data):
        d.color = (blotch[i], rnd.random(), code, 1.0)
    mesh.color_attributes.active_color = attr
    gore = mesh.attributes.get("_gore") or mesh.attributes.new("_gore", "FLOAT_COLOR", "POINT")
    for i, d in enumerate(gore.data):
        d.color = (wound[i], blood[i], dirt[i], burn[i])
    part = mesh.attributes.get("_part") or mesh.attributes.new("_part", "FLOAT", "POINT")
    for i, d in enumerate(part.data):
        d.value = float(part_of_vertex(i) if part_of_vertex else 0)
    obj["acr_layer"] = layer


# ----------------------------------------------------------------------------- materials

def _math(nodes, links, op, a, b=None):
    n = nodes.new("ShaderNodeMath")
    n.operation = op
    for i, v in enumerate((a, b)):
        if v is None:
            continue
        if isinstance(v, (int, float)):
            n.inputs[i].default_value = v
        else:
            links.new(v, n.inputs[i])
    return n.outputs[0]


def _ramp(nodes, links, x, a, b):
    n = nodes.new("ShaderNodeMapRange")
    n.clamp = True
    links.new(x, n.inputs["Value"])
    n.inputs["From Min"].default_value = a
    n.inputs["From Max"].default_value = b
    return n.outputs["Result"]


def _mix(nodes, links, fac, a, b):
    n = nodes.new("ShaderNodeMix")
    n.data_type = "RGBA"
    links.new(fac, n.inputs["Factor"])
    for sock, v in ((n.inputs["A"], a), (n.inputs["B"], b)):
        if isinstance(v, tuple):
            sock.default_value = v
        else:
            links.new(v, sock)
    return n.outputs["Result"]


def material(name, layer, rgb=(0.5, 0.5, 0.5), image=None, uv_map="sprite", roughness=0.85, metallic=0.0,
             interpolation="Closest"):
    """A decaying preview material (Cycles/Eevee) for `layer`: base colour `rgb` (sRGB) or
    `image` through `uv_map`; reads the object property `decay`. `export_material` makes the
    plain glTF twin."""
    import bpy

    p = LAYERS[layer]
    mat = bpy.data.materials.new(name)
    mat["acr_decay"] = extras(layer)
    mat["acr_base_rgb"] = list(rgb)
    if image is not None:
        mat["acr_image"] = image.name
        mat["acr_uv"] = uv_map
        mat["acr_interp"] = interpolation
    mat.diffuse_color = lin(rgb)
    nodes, links = mat.node_tree.nodes, mat.node_tree.links
    nodes.clear()
    out = nodes.new("ShaderNodeOutputMaterial")
    bsdf = nodes.new("ShaderNodeBsdfPrincipled")
    bsdf.inputs["Roughness"].default_value = roughness
    bsdf.inputs["Metallic"].default_value = metallic
    if image is not None:
        tex = nodes.new("ShaderNodeTexImage")
        tex.image = image
        tex.interpolation = interpolation
        uv = nodes.new("ShaderNodeUVMap")
        uv.uv_map = uv_map
        links.new(uv.outputs["UV"], tex.inputs["Vector"])
        base = tex.outputs["Color"]
    else:
        base = lin(rgb)
    t = nodes.new("ShaderNodeAttribute")
    t.attribute_type = "OBJECT"
    t.attribute_name = "decay"
    t = t.outputs["Fac"]
    vc = nodes.new("ShaderNodeAttribute")
    vc.attribute_name = "decay_mask"
    sep = nodes.new("ShaderNodeSeparateColor")
    links.new(vc.outputs["Color"], sep.inputs["Color"])
    mask = _math(nodes, links, "MULTIPLY_ADD", sep.outputs[0], 0.75)
    links.new(_math(nodes, links, "MULTIPLY", sep.outputs[1], 0.25), mask.node.inputs[2])
    dfac = _math(nodes, links, "MULTIPLY", _ramp(nodes, links, t, *p["d"]), p["k"])
    col = _mix(nodes, links, dfac, base, lin(p["tint"]))
    keep = None
    if p["s"]:
        prog = _ramp(nodes, links, t, *p["s"])
        th = _math(nodes, links, "MULTIPLY", prog, 1.0 + p["edge"] + 0.001)
        keep = _math(nodes, links, "GREATER_THAN", mask, th)
        if p["edge"] > 0:
            above = _math(nodes, links, "SUBTRACT", mask, th)
            e = _math(nodes, links, "LESS_THAN", above, p["edge"])
            e = _math(nodes, links, "MULTIPLY", e, _math(nodes, links, "GREATER_THAN", prog, 0.0))
            col = _mix(nodes, links, e, col, lin(p["edge_rgb"]))
    links.new(col, bsdf.inputs["Base Color"])
    if keep is not None:
        tr = nodes.new("ShaderNodeBsdfTransparent")
        mix = nodes.new("ShaderNodeMixShader")
        links.new(keep, mix.inputs[0])
        links.new(tr.outputs[0], mix.inputs[1])
        links.new(bsdf.outputs[0], mix.inputs[2])
        links.new(mix.outputs[0], out.inputs["Surface"])
    else:
        links.new(bsdf.outputs[0], out.inputs["Surface"])
    return mat


def export_material(mat):
    """Plain Principled twin of a decay material for glTF (node shaders do not export):
    base colour or texture; the decay parameters stay in the custom properties (extras)."""
    import bpy

    m = bpy.data.materials.new(mat.name + ".gltf")
    for k in mat.keys():
        m[k] = mat[k]
    nodes, links = m.node_tree.nodes, m.node_tree.links
    bsdf = nodes.get("Principled BSDF")
    bsdf.inputs["Roughness"].default_value = 0.85
    if "acr_image" in mat:
        tex = nodes.new("ShaderNodeTexImage")
        tex.image = bpy.data.images[mat["acr_image"]]
        tex.interpolation = mat.get("acr_interp", "Closest")
        uv = nodes.new("ShaderNodeUVMap")
        uv.uv_map = mat["acr_uv"]
        links.new(uv.outputs["UV"], tex.inputs["Vector"])
        links.new(tex.outputs["Color"], bsdf.inputs["Base Color"])
    else:
        bsdf.inputs["Base Color"].default_value = lin(tuple(mat["acr_base_rgb"]))
    m.diffuse_color = mat.diffuse_color
    return m
