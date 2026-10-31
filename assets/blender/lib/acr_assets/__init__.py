"""Shared helpers for the ACR asset scripts (Blender 5.2, headless). See docs/assets.md.

`conventions` is pure Python (importable and testable without Blender); the other modules
need `bpy`. Asset scripts anywhere under `assets/` import them after `build.py` (or their own
entry script) has put `assets/blender/lib` on `sys.path`:

    from acr_assets import conventions, scene, materials, project
"""

import json
import os

# Numbers library code measures while building (`heads.face_map`: the head's UV layout),
# merged into `<name>.stats.json` by `build.py`.
BUILD_STATS = {}


class View:
    """One stored direction of a sprite reference (`acr model-ref`)."""

    def __init__(self, ref, d):
        self.k = d["k"]
        self.dir = float(d["dir"])
        self.image = os.path.join(ref.dir, d["image"])


class Reference:
    """`ref.json` written by `acr model-ref`: the sprite views of an object type on one
    canvas, with the ground point at `anchor`."""

    def __init__(self, path):
        with open(path, encoding="utf-8") as f:
            data = json.load(f)
        self.path = path
        self.dir = os.path.dirname(os.path.abspath(path))
        self.type = data["type"]
        self.anim = data["anim"]
        self.dirs = data["dirs"]
        self.full_rotation = data["full_rotation"]
        self.width = data["width"]
        self.height = data["height"]
        self.anchor = tuple(data["anchor"])
        # ADDSECTION state drawn into the reference (`--sections`), -1 none.
        self.sections = data.get("sections", -1)
        # The frame of the clip a unit reference shows (`--step`).
        self.step = data.get("step", 0)
        self.views = [View(self, v) for v in data["views"]]
