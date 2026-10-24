"""AtcKre: Aztec fort, scripted (`acr_assets.stonebuild`; Pixal3D melted it, IoU 0.64, #305):
a long walled compound of stepped stone halls with bell-shaped temple roofs (stacked
tapering tiers and a finial), cornices and doorways, a wide stair at the west end and a tall
tower in four tapering storeys with a temple top and roof comb at the east end. Colours from
the sprite (`stonebuild.finish`)."""

from acr_assets import stonebuild

TYPE = "AtcKre"
CHECKS = ["--anim STANDLO"]


def build(ctx):
    k = stonebuild.Kit(seed=51)
    k.box("stone", (0, 0), (26.0, 9.0), 0.0, 1.2)  # platform
    k.box("stone", (-1.0, 0), (22.0, 7.0), 1.2, 4.0)  # hall block
    k.cornice("stone", (-1.0, 0), (22.0, 7.0), 3.6, 0.4, 0.25)
    k.cornice("stone", (0, 0), (26.0, 9.0), 1.2, 0.25, 0.15)
    for x, h in ((-9.0, 3.0), (-3.5, 4.0), (2.0, 3.4)):
        z = k.tiers("stone", (x, 0), [(5.0, 6.0), (4.0, 5.0)], 4.0, [0.8, 0.8], taper=0.05)
        # bell roof: three tapering tiers, then a finial
        z = k.tiers("stone", (x, 0), [(3.8, 4.6), (3.0, 3.6), (2.0, 2.4)], z, [h * 0.35, h * 0.35, h * 0.3], taper=0.12)
        k.box("stone", (x, 0), (0.8, 0.8), z, z + 1.0, taper=0.25)
        k.door("stone", x, -3.5, w=1.2, h=2.0, z0=1.2)
    k.stairs("stone", (-14.5, 0), 4.0, 2.4, 0.0, 4.0, 10, 270)
    z = k.tiers("stone", (10.0, 0.5), [(6.0, 6.0), (5.0, 5.0), (4.2, 4.2), (3.4, 3.4)], 0.0,
                [5.0, 4.0, 3.5, 2.5], taper=0.04)
    for zz, s in ((5.0, 5.6), (9.0, 4.7), (12.5, 3.9)):
        k.cornice("stone", (10.0, 0.5), (s, s), zz - 0.15, 0.35, 0.25)
    k.door("stone", 10.0, -2.5, w=1.4, h=2.6)
    k.box("stone", (10.0, 0.5), (3.0, 3.0), z, z + 3.0, taper=0.4)
    k.box("stone", (10.0, 0.5), (2.2, 0.4), z + 2.0, z + 4.2, taper=0.1)  # roof comb
    k.heap("rubble", (0, 0), 14.0, 50, 0.6, 0.3, ring=0.85, squash=0.4)
    stonebuild.finish(ctx, k)
