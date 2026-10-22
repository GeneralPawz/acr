"""Units, axes and the original camera, as pure math (no `bpy`; unit-tested without Blender).

See docs/assets.md. In short:

- 1 world pixel of the game = METRES_PER_PIXEL metres; Blender units are metres.
- Blender axes: +X east (game x), +Y north (game -y), +Z up. glTF (+Y up) therefore gets
  +X east, +Y up, +Z south (game +y).
- Models face south (-Y in Blender, +Z in glTF): `RealDir` 64, stored direction 0.
- The original view is an orthographic projection looking north and 30 degrees down:
  screen x = world x, screen y = world y / 2 - height, with the height in screen pixels
  (docs/formats/terrain.md). A true orthographic camera at 30 degrees shows a vertical
  length z as z * cos 30 on screen, so one screen pixel of height is 1 / cos 30 world pixels.
"""

import math

# Scale: a standing musketeer (EngM18, #STAND) reaches 45-47 screen pixels above his ground
# point, i.e. 52-54 world pixels (/ cos 30); a man with hat is about 1.8 m.
PIXELS_PER_METRE = 30.0
METRES_PER_PIXEL = 1.0 / PIXELS_PER_METRE

# Camera elevation: the ground is compressed 2:1 (screen y = world y / 2), sin(30) = 1/2.
ELEVATION = math.radians(30.0)
SIN_E = math.sin(ELEVATION)
COS_E = math.cos(ELEVATION)

# Camera basis in Blender coordinates (metres): screen right, screen up, view direction.
RIGHT = (1.0, 0.0, 0.0)
UP = (0.0, SIN_E, COS_E)
FORWARD = (0.0, COS_E, -SIN_E)
# Blender camera rotation (XYZ Euler, radians): a camera at X = 90 deg looks north; 60 deg
# tilts it 30 deg down.
CAMERA_ROTATION = (math.radians(90.0) - ELEVATION, 0.0, 0.0)
# Distance of the camera from the scene along the view direction (metres); orthographic, so
# it only has to clear the model.
CAMERA_DISTANCE = 200.0


def dot(a, b):
    return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]


def game_to_blender(x, y, height=0.0):
    """A game position (world pixels; height in screen pixels like the terrain's) in metres."""
    return (x * METRES_PER_PIXEL, -y * METRES_PER_PIXEL, height / COS_E * METRES_PER_PIXEL)


def yaw(real_dir):
    """Rotation about +Z (radians, counter-clockwise from above) that turns a south-facing
    model to face `RealDir` real_dir (0..256: 0 east, 64 south, 128 west, 192 north;
    clockwise on screen because the game's y points south)."""
    return -(real_dir - 64.0) * 2.0 * math.pi / 256.0


def rotate_z(p, angle):
    c, s = math.cos(angle), math.sin(angle)
    return (p[0] * c - p[1] * s, p[0] * s + p[1] * c, p[2])


def project(p):
    """Screen offset (pixels, y down) of Blender point p (metres) from the ground point."""
    return (dot(p, RIGHT) * PIXELS_PER_METRE, -dot(p, UP) * PIXELS_PER_METRE)


def to_canvas(p, anchor, real_dir=64.0):
    """Canvas pixel coordinates of model point p (metres, model space) when the model stands
    with its origin on the canvas point `anchor` and faces `real_dir`. Pixel (i, j) covers
    [i, i+1) x [j, j+1); the anchor is a pixel corner, like a frame origin."""
    sx, sy = project(rotate_z(p, yaw(real_dir)))
    return (anchor[0] + sx, anchor[1] + sy)


def stored_direction(k, dirs, full_rotation=False):
    """`RealDir` shown by stored direction k (port of `acr model-ref`'s `stored_direction`,
    the inverse of `acr_render::scene::sprite_direction`)."""
    r = max(dirs, 1)
    if full_rotation:
        d = 64.0 + 256.0 * k / r
    elif r == 1:
        d = 64.0
    elif r % 2 == 1:
        d = 64.0 + 128.0 * k / (r - 1)
    else:
        d = 64.0 + 128.0 * (k + 0.5) / r
    return d % 256.0


def camera_setup(width, height, anchor):
    """Orthographic camera for a canvas of width x height pixels whose ground point (model
    origin) is at pixel `anchor`: returns (location, rotation_euler, ortho_scale). Blender
    fits ortho_scale to the larger side (sensor_fit AUTO) and maps the camera centre to the
    canvas centre, so the camera is moved by the anchor's distance from the centre."""
    ax, ay = anchor
    off_r = (width / 2.0 - ax) * METRES_PER_PIXEL
    off_u = (ay - height / 2.0) * METRES_PER_PIXEL
    loc = tuple(RIGHT[i] * off_r + UP[i] * off_u - FORWARD[i] * CAMERA_DISTANCE for i in range(3))
    return loc, CAMERA_ROTATION, max(width, height) * METRES_PER_PIXEL


def camera_to_canvas(p, width, height, anchor):
    """Where the camera from camera_setup puts world point p (metres) on the canvas: the
    generic orthographic camera mapping, used by the tests to check camera_setup."""
    loc, _, _ = camera_setup(width, height, anchor)
    d = tuple(p[i] - loc[i] for i in range(3))
    return (width / 2.0 + dot(d, RIGHT) * PIXELS_PER_METRE, height / 2.0 - dot(d, UP) * PIXELS_PER_METRE)
