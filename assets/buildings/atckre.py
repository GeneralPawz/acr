"""AtcKre: Aztec fort, scripted (`acr_assets.stonebuild`; Pixal3D melted it, IoU 0.64, #305):
a long walled compound of stepped stone halls with pyramid caps, a wide stair at the west
end and a tall tower in three tapering storeys with a temple top at the east end."""

from acr_assets import stonebuild

TYPE = "AtcKre"
CHECKS = ["--anim STANDLO"]


def build(ctx):
    k = stonebuild.Kit(seed=51)
    k.box("stone", (0, 0), (26.0, 9.0), 0.0, 1.2)  # platform
    k.box("stone", (-1.0, 0), (22.0, 7.0), 1.2, 4.0)  # hall block
    for x, h in ((-9.0, 3.0), (-3.5, 4.0), (2.0, 3.4)):
        z = k.tiers("stone", (x, 0), [(5.0, 6.0), (4.0, 5.0)], 4.0, [0.8, 0.8], taper=0.05)
        k.box("stone", (x, 0), (3.6, 4.4), z, z + h, taper=0.42)  # pyramid cap
        k.box("stone", (x, 0), (0.7, 0.7), z + h - 0.3, z + h + 1.0, taper=0.2)  # finial
    k.stairs("stone", (-14.5, 0), 4.0, 2.4, 0.0, 4.0, 10, 270)
    z = k.tiers("stone", (10.0, 0.5), [(6.0, 6.0), (5.0, 5.0), (4.2, 4.2), (3.4, 3.4)], 0.0,
                [5.0, 4.0, 3.5, 2.5], taper=0.04)
    k.box("stone", (10.0, 0.5), (3.0, 3.0), z, z + 3.0, taper=0.4)
    k.box("stone", (10.0, 0.5), (0.8, 0.8), z + 2.6, z + 4.0, taper=0.25)
    k.heap("rubble", (0, 0), 14.0, 50, 0.6, 0.3, ring=0.85, squash=0.4)
    stonebuild.finish(ctx, k)
