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


def export_glb(path, compression="none"):
    """Exports the scene. Skins, morph targets (shape keys), one animation per action
    (`STAND`, `DEATH`, ...) and custom attributes whose names start with `_` (`_decay` ->
    `_DECAY`, `_gore` -> `_GORE`, `_part` -> `_PART`, `_wind` -> `_WIND`) go out when the
    scene has them."""
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
        export_image_format="AUTO",
        export_skins=True,
        # Node transforms = the current pose (units: STAND), not the bind pose.
        export_rest_position_armature=False,
        export_morph=True,
        export_morph_normal=False,
        export_animations=True,
        export_animation_mode="ACTIONS",
        export_force_sampling=True,
        export_optimize_animation_size=True,
        # No COLOR_0: glTF multiplies it into the base colour; masks go out as `_` attributes.
        export_vertex_color="NONE",
        export_all_vertex_colors=False,
        export_active_vertex_color_when_no_material=False,
        export_attributes=True,
        export_meshopt_compression_enable=compression == "meshopt",
        export_draco_mesh_compression_enable=compression == "draco",
    )
    return os.path.getsize(path)
