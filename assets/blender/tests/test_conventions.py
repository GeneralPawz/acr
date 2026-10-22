"""Tests of the camera and direction math (plain Python, no Blender):

    python -m unittest discover -s assets/blender/tests
"""

import math
import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "lib"))

from acr_assets import conventions as cv  # noqa: E402


def sprite_direction(d, dirs, full_rotation):
    """Port of `acr_render::scene::sprite_direction` (docs/simulation.md)."""
    r = max(dirs, 1)
    if full_rotation:
        per = max(256 // r, 1)
        v = (d + (per >> 1) - 64) % 256
        return min(v // per, r - 1), False
    if r & 1:
        n = max(2 * r - 2, 1)
        c, below = 255 // (2 * n), r - 1
    else:
        n, c, below = 2 * r, 0, r
    i = (((d + 64 + c) & 255) * n) >> 8
    return (r - 1 - i, True) if i < below else (i - below, False)


def close(a, b, eps=1e-9):
    return all(abs(x - y) < eps for x, y in zip(a, b))


class Projection(unittest.TestCase):
    def test_ground_and_height(self):
        # screen x = world x, screen y = world y / 2 - height (docs/formats/terrain.md)
        for x, y, h in [(0, 0, 0), (32, 0, 0), (0, 64, 0), (-17, 40, 0), (5, 6, 20)]:
            self.assertTrue(close(cv.project(cv.game_to_blender(x, y, h)), (x, y / 2 - h)))

    def test_true_vertical_length(self):
        # A vertical stick of L world pixels shows L cos 30 screen pixels.
        top = (0.0, 0.0, 30 * cv.METRES_PER_PIXEL)
        self.assertAlmostEqual(cv.project(top)[1], -30 * math.cos(math.radians(30)))

    def test_camera_rotation_matches_basis(self):
        a = cv.CAMERA_ROTATION[0]
        # Blender cameras look along local -Z with local +Y up; rotate them about X by a.
        self.assertTrue(close((0.0, math.sin(a), -math.cos(a)), cv.FORWARD))
        self.assertTrue(close((0.0, math.cos(a), math.sin(a)), cv.UP))
        self.assertAlmostEqual(cv.dot(cv.FORWARD, cv.UP), 0.0)

    def test_camera_puts_points_where_the_projection_does(self):
        w, h, anchor = 307, 254, (190, 170)
        for p in [(0, 0, 0), (1.0, -2.0, 0.5), (-3.0, 1.5, 2.0)]:
            self.assertTrue(close(cv.camera_to_canvas(p, w, h, anchor), cv.to_canvas(p, anchor)))
        self.assertTrue(close(cv.camera_to_canvas((0, 0, 0), w, h, anchor), anchor))


class Directions(unittest.TestCase):
    def test_yaw_turns_south_facing_models(self):
        south = (0.0, -1.0, 0.0)  # Blender: north is +Y
        for real_dir, expect in [(64, (0, -1)), (128, (-1, 0)), (192, (0, 1)), (0, (1, 0)), (96, (-0.7071, -0.7071))]:
            v = cv.rotate_z(south, cv.yaw(real_dir))
            self.assertTrue(close(v[:2], expect, 1e-4), (real_dir, v))

    def test_stored_directions_round_trip(self):
        for dirs, full in [(1, False), (2, False), (8, False), (9, False), (16, True), (65, False)]:
            for k in range(dirs):
                d = cv.stored_direction(k, dirs, full)
                # The last of an odd count (north) is only ever drawn mirrored (i = 0).
                mirrored = not full and dirs % 2 == 1 and dirs > 1 and k == dirs - 1
                self.assertEqual(sprite_direction(round(d) % 256, dirs, full), (k, mirrored), (dirs, k, d))
        self.assertEqual([cv.stored_direction(k, 9) for k in (0, 4, 8)], [64.0, 128.0, 192.0])

    def test_east_is_mirrored_west(self):
        # 9 stored directions: east (0) shows stored direction 4 (west) mirrored.
        self.assertEqual(sprite_direction(0, 9, False), (4, True))


if __name__ == "__main__":
    unittest.main()
