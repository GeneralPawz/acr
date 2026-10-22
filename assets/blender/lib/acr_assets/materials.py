"""Materials (needs `bpy`). Kept to what the glTF exporter maps: Principled BSDF base colour,
roughness, metallic and an image texture on the base colour."""

import bpy


def srgb_to_linear(c):
    return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4


def flat(name, rgb, roughness=0.9):
    """A plain material; `rgb` in sRGB 0..1 (as picked from a sprite)."""
    mat = bpy.data.materials.get(name) or bpy.data.materials.new(name)
    mat.use_nodes = True
    bsdf = mat.node_tree.nodes["Principled BSDF"]
    bsdf.inputs["Base Color"].default_value = (*[srgb_to_linear(c) for c in rgb], 1.0)
    bsdf.inputs["Roughness"].default_value = roughness
    mat.diffuse_color = (*[srgb_to_linear(c) for c in rgb], 1.0)
    return mat


def textured(name, image, uv_map, roughness=0.9, closest=True):
    """A material showing `image` through UV map `uv_map`."""
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    nodes, links = mat.node_tree.nodes, mat.node_tree.links
    bsdf = nodes["Principled BSDF"]
    bsdf.inputs["Roughness"].default_value = roughness
    tex = nodes.new("ShaderNodeTexImage")
    tex.image = image
    tex.interpolation = "Closest" if closest else "Linear"
    uv = nodes.new("ShaderNodeUVMap")
    uv.uv_map = uv_map
    links.new(uv.outputs["UV"], tex.inputs["Vector"])
    links.new(tex.outputs["Color"], bsdf.inputs["Base Color"])
    return mat


def assign(obj, mat):
    obj.data.materials.clear()
    obj.data.materials.append(mat)
