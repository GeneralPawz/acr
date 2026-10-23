"""Texture layers of the detailed ground (docs/ground.md) and their CC0 sets.

One layer per material class in class order, then the second sets that the classes mix in
against tiling. The order is the layer order of the texture arrays; keep it in sync with
`acr_render::ground::LAYERS`. A set is `<source folder>/<id>` under the sources root
(D:\ACR-models): Poly Haven sets hold diff/nor_gl/rough/disp.jpg, ambientCG sets
<id>_2K-JPG_{Color,NormalGL,Roughness,Displacement}.jpg.
"""
SETS = {
    "grass": "ambientcg/Grass001",
    "meadow": "ambientcg/Grass007",
    "dirt": "polyhaven-textures/dirt",
    "mud": "polyhaven-textures/brown_mud_02",
    "sand": "polyhaven-textures2/sand_01",
    "rock": "polyhaven-textures2/rock_ground",
    "forest": "polyhaven-textures2/forest_ground_04",
    "snow": "polyhaven-textures/snow_02",
    "field": "polyhaven-textures/dry_mud_field_001",
    "cracked": "polyhaven-textures2/mud_cracked_dry_03",
    "red": "polyhaven-textures2/red_dirt_mud_01",
    "gravel": "polyhaven-textures2/gravel_ground_01",
    "desert": "polyhaven-textures2/dry_ground_rocks",
    "steppe": "polyhaven-textures2/grass_ground",
    "snowy": "polyhaven-textures2/snow_03",
    "grass2": "ambientcg/Grass004",
    "meadow2": "ambientcg/Ground037",
    "dirt2": "polyhaven-textures2/park_dirt",
    "desert2": "polyhaven-textures2/sandy_gravel_02",
    "rock2": "polyhaven-textures/rocky_terrain_02",
}
