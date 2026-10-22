"""Decay layers: masks, materials and the decay timeline (needs `bpy`).

One parameter `decay` (0 fresh .. 1 gone) drives every layer of a corpse. Each material
reads it from the object's custom property `decay` (Attribute node of type OBJECT), i.e. per
corpse; a game renderer passes it per instance as a uniform. Each mesh carries the colour
attribute `decay_mask` (exported as glTF COLOR_0):

- R: low-frequency blotch noise, rank-normalised per mesh (a threshold of x removes a
  fraction x of the surface, so dissolving proceeds evenly in time),
- G: per-vertex white noise (frays the edges of the blotches),
- B: layer code (LAYERS[..]["code"] / 8, for engines that merge layers into one mesh),
- A: 1.

The shader keeps a fragment while mask = 0.75 R + 0.25 G > threshold(decay, layer) and
darkens a band `edge` wide above the threshold (rotten edge). That is all a WGSL/GLSL port
needs: one uniform, one vertex colour, an alpha test (`discard`) and two mixes.
"""

import random

import bpy
from mathutils import Vector, noise

# Per layer: discolour window `d` (decay values), its tint and strength `k`, dissolve window
# `s` (None = never dissolves), dark edge width and colour. Timeline in docs/assets-humans.md.
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
}

# The whole corpse (bones, rags, gear on the ground) sinks by SINK metres over this window.
SINK_WINDOW = (0.78, 1.00)
SINK = 0.20

# Decay starts where the original turns the corpse half transparent (death counter 100) and
# ends at the type's removal tick (1200 without #DEATHLIE2, 2000 with it).
FADE_START = 100


def decay_from_counter(counter, remove=1200):
    return min(max((counter - FADE_START) / float(remove - FADE_START), 0.0), 1.0)


def sink_offset(decay):
    a, b = SINK_WINDOW
    return -SINK * min(max((decay - a) / (b - a), 0.0), 1.0)


def srgb_to_linear(c):
    return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4


def lin(rgb):
    return tuple(srgb_to_linear(c) for c in rgb) + (1.0,)


def add_mask(obj, layer, seed, freq=7.0):
    """Writes the `decay_mask` colour attribute (see module doc) from rest positions."""
    mesh = obj.data
    rnd = random.Random(seed)
    off = Vector((rnd.uniform(0, 100), rnd.uniform(0, 100), rnd.uniform(0, 100)))
    raw = [noise.noise(v.co * freq + off) + 0.5 * noise.noise(v.co * freq * 2.3 + off)
           for v in mesh.vertices]
    order = sorted(range(len(raw)), key=raw.__getitem__)
    rank = [0.0] * len(raw)
    for i, vi in enumerate(order):
        rank[vi] = (i + 0.5) / max(len(raw), 1)
    code = LAYERS[layer]["code"] / 8.0
    attr = mesh.color_attributes.get("decay_mask") or mesh.color_attributes.new(
        "decay_mask", "FLOAT_COLOR", "POINT")
    for i, d in enumerate(attr.data):
        d.color = (rank[i], rnd.random(), code, 1.0)
    mesh.color_attributes.active_color = attr
    obj["acr_layer"] = layer


def _math(nodes, links, op, a, b=None, clamp=False):
    n = nodes.new("ShaderNodeMath")
    n.operation = op
    n.use_clamp = clamp
    for i, v in enumerate((a, b)):
        if v is None:
            continue
        if isinstance(v, (int, float)):
            n.inputs[i].default_value = v
        else:
            links.new(v, n.inputs[i])
    return n.outputs[0]


def _ramp01(nodes, links, x, a, b):
    """clamp((x - a) / (b - a))"""
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


def material(name, rgb, layer, roughness=0.85, metallic=0.0):
    """A decaying material in sRGB colour `rgb` for `layer` (key of LAYERS). Custom
    properties carry the parameters into glTF `extras` (export_extras)."""
    p = LAYERS[layer]
    mat = bpy.data.materials.new(name)
    mat["acr_decay"] = {"layer": layer, "discolour": list(p["d"]), "tint": list(p["tint"]),
                        "tint_strength": p["k"], "dissolve": list(p["s"]) if p["s"] else [],
                        "edge": p["edge"], "edge_rgb": list(p["edge_rgb"])}
    mat["acr_base_rgb"] = list(rgb)
    mat.diffuse_color = lin(rgb)
    nt = mat.node_tree
    nodes, links = nt.nodes, nt.links
    nodes.clear()
    out = nodes.new("ShaderNodeOutputMaterial")
    bsdf = nodes.new("ShaderNodeBsdfPrincipled")
    bsdf.inputs["Roughness"].default_value = roughness
    bsdf.inputs["Metallic"].default_value = metallic
    t = nodes.new("ShaderNodeAttribute")
    t.attribute_type = "OBJECT"
    t.attribute_name = "decay"
    t = t.outputs["Fac"]
    vc = nodes.new("ShaderNodeVertexColor")
    vc.layer_name = "decay_mask"
    sep = nodes.new("ShaderNodeSeparateColor")
    links.new(vc.outputs["Color"], sep.inputs["Color"])
    mask = _math(nodes, links, "MULTIPLY_ADD", sep.outputs[0], 0.75)
    links.new(_math(nodes, links, "MULTIPLY", sep.outputs[1], 0.25), mask.node.inputs[2])
    # Discolouration.
    dfac = _math(nodes, links, "MULTIPLY", _ramp01(nodes, links, t, *p["d"]), p["k"])
    col = _mix(nodes, links, dfac, lin(rgb), lin(p["tint"]))
    keep = None
    if p["s"]:
        prog = _ramp01(nodes, links, t, *p["s"])
        # threshold = prog * (1 + edge) so the edge band also passes the top of the mask.
        th = _math(nodes, links, "MULTIPLY", prog, 1.0 + p["edge"] + 0.001)
        keep = _math(nodes, links, "GREATER_THAN", mask, th)  # 1 = keep
        if p["edge"] > 0:
            above = _math(nodes, links, "SUBTRACT", mask, th)
            e = _math(nodes, links, "LESS_THAN", above, p["edge"])
            started = _math(nodes, links, "GREATER_THAN", prog, 0.0)
            e = _math(nodes, links, "MULTIPLY", e, started)
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


def gltf_material(mat):
    """Plain Principled copy of `mat` for glTF export (node shaders do not export): base
    colour only; the decay parameters stay in the custom properties (glTF extras)."""
    m = bpy.data.materials.new(mat.name + ".gltf")
    for k in mat.keys():
        m[k] = mat[k]
    nodes = m.node_tree.nodes
    bsdf = nodes.get("Principled BSDF")
    if bsdf is None:
        nodes.clear()
        bsdf = nodes.new("ShaderNodeBsdfPrincipled")
        o = nodes.new("ShaderNodeOutputMaterial")
        m.node_tree.links.new(bsdf.outputs[0], o.inputs["Surface"])
    bsdf.inputs["Base Color"].default_value = lin(tuple(mat["acr_base_rgb"]))
    bsdf.inputs["Roughness"].default_value = 0.85
    m.use_backface_culling = False
    return m
