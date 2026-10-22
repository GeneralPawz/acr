"""Scene setup, naming and sockets (needs `bpy`)."""

import bmesh
import bpy

from . import conventions as cv


def reset():
    """Empties the scene (`--factory-startup` still adds a cube, a light and a camera) and
    sets metres as the unit."""
    for obj in list(bpy.data.objects):
        bpy.data.objects.remove(obj, do_unlink=True)
    for coll in (bpy.data.meshes, bpy.data.materials, bpy.data.images, bpy.data.cameras, bpy.data.lights,
                 bpy.data.armatures, bpy.data.actions, bpy.data.shape_keys):
        for block in list(coll):
            try:
                coll.remove(block)
            except (RuntimeError, AttributeError, ReferenceError):
                pass
    bpy.data.orphans_purge(do_recursive=True)
    s = bpy.context.scene
    s.unit_settings.system = "METRIC"
    s.unit_settings.scale_length = 1.0
    # Colours exactly as stored: no view transform on renders.
    s.view_settings.view_transform = "Standard"
    s.view_settings.look = "None"
    return s


def root(name, **props):
    """The asset's root node (an empty at the ground point; glTF node `name`). Custom
    properties become glTF extras."""
    obj = bpy.data.objects.new(name, None)
    obj.empty_display_type = "PLAIN_AXES"
    bpy.context.scene.collection.objects.link(obj)
    obj["acr_pixels_per_metre"] = cv.PIXELS_PER_METRE
    for k, v in props.items():
        obj[k] = v
    return obj


def socket(parent, name, location):
    """An attachment point (glTF node `socket.<name>`): fire, smoke, flag, muzzle, hand, ...
    `location` in metres, model space."""
    obj = bpy.data.objects.new(f"socket.{name}", None)
    obj.empty_display_type = "ARROWS"
    obj.empty_display_size = 0.2
    obj.location = location
    obj.parent = parent
    bpy.context.scene.collection.objects.link(obj)
    return obj


def mesh_object(parent, name, mesh):
    """Links a mesh under the root as `<root>.<name>`."""
    obj = bpy.data.objects.new(f"{parent.name}.{name}", mesh)
    obj.parent = parent
    bpy.context.scene.collection.objects.link(obj)
    return obj


def from_pydata(name, verts, faces, recalc=True):
    """A mesh from vertices (metres) and faces; with `recalc` (closed solids) the normals
    are made to point outwards whatever the winding."""
    mesh = bpy.data.meshes.new(name)
    mesh.from_pydata(verts, [], faces)
    mesh.validate()
    if recalc:
        bm = bmesh.new()
        bm.from_mesh(mesh)
        bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
        bm.to_mesh(mesh)
        bm.free()
    mesh.update()
    return mesh
