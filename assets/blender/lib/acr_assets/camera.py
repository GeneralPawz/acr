"""The original camera in Blender (needs `bpy`); the math is in `conventions`."""

import bpy

from . import conventions as cv


def original_camera(width, height, anchor, name="original"):
    """An orthographic camera that renders the game's view of a model standing on canvas
    pixel `anchor` (a canvas of width x height pixels, 1 pixel = 1 world pixel). The
    direction a model faces is set on the model (see `conventions.yaw`), not the camera."""
    loc, rot, scale = cv.camera_setup(width, height, anchor)
    data = bpy.data.cameras.new(name)
    data.type = "ORTHO"
    data.ortho_scale = scale
    data.sensor_fit = "AUTO"
    data.clip_start = 0.1
    data.clip_end = cv.CAMERA_DISTANCE * 2.0
    obj = bpy.data.objects.new(name, data)
    obj.location = loc
    obj.rotation_mode = "XYZ"
    obj.rotation_euler = rot
    s = bpy.context.scene
    s.collection.objects.link(obj)
    s.camera = obj
    s.render.resolution_x = width
    s.render.resolution_y = height
    s.render.resolution_percentage = 100
    s.render.pixel_aspect_x = 1.0
    s.render.pixel_aspect_y = 1.0
    return obj
