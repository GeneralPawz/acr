"""glTF export with the project's settings (needs `bpy`).

Blender 5.2's exporter writes glTF 2.0 binary (`.glb`) and can compress meshes with Draco
(`KHR_draco_mesh_compression`) or meshopt (`EXT_meshopt_compression`, built in since
Blender 5.x); images go out as PNG, JPEG or WebP, not KTX2/Basis (that needs an external
tool such as `gltfpack -tc` or `toktx`). The default is no compression: the Rust side has no
meshopt/Draco decoder yet (see docs/assets.md).
"""

import os

import bpy

COMPRESSION = ("none", "meshopt", "draco")


def export_glb(path, compression="none", image_format="AUTO"):
    """Exports the scene. Skins, morph targets (shape keys), one animation per action
    (`STAND`, `DEATH`, ...), the active colour attribute as COLOR_0 and custom attributes
    whose names start with `_` (`_gore` -> `_GORE`, `_part` -> `_PART`, `_wind` -> `_WIND`)
    go out when the scene has them. Images go out as PNG (the renderer decodes PNG only)."""
    if compression not in COMPRESSION:
        raise ValueError(f"compression must be one of {COMPRESSION}")
    os.makedirs(os.path.dirname(os.path.abspath(path)), exist_ok=True)
    bpy.ops.export_scene.gltf(
        filepath=path,
        export_format="GLB",
        export_yup=True,
        export_apply=True,
        export_extras=True,
        export_cameras=False,
        export_lights=False,
        export_texcoords=True,
        export_normals=True,
        export_image_format=image_format,
        export_image_quality=88,
        export_skins=True,
        # Node transforms = the current pose (units: STAND), not the bind pose.
        export_rest_position_armature=False,
        export_morph=True,
        export_morph_normal=False,
        export_animations=True,
        export_animation_mode="ACTIONS",
        # Placeholder clips are linear between their keys: export the keys, not samples.
        export_force_sampling=True,
        export_optimize_animation_size=True,
        # Keep a clip's constant channels: a one-frame clip (STAND) is all constants.
        export_optimize_animation_keep_anim_armature=True,
        # COLOR_0 = the active colour attribute: the decay mask of units (read as a mask by
        # the renderer for `acr_decay` materials), baked shading of trees.
        export_vertex_color="ACTIVE",
        export_all_vertex_colors=False,
        export_active_vertex_color_when_no_material=False,
        export_attributes=True,
        export_meshopt_compression_enable=compression == "meshopt",
        export_draco_mesh_compression_enable=compression == "draco",
    )
    return os.path.getsize(path)
