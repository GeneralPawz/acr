"""AtcKre: Aztec fort, scripted (`acr_assets.stonebuild`; Pixal3D melted it, IoU 0.64, #305):
a long walled compound of stepped stone halls with bell-shaped temple roofs (stacked
tapering tiers and a finial), cornices and doorways, a wide stair at the west end and a tall
tower in four tapering storeys with a temple top and roof comb at the east end. Colours from
the sprite (`stonebuild.finish`).

The game draws a finished AtcKre as `#STANDLO` (the front halls and the tower) plus its three
`ADDSECTION` parts in their first state: the keep with its domed corner turret, the west
tower with its wings and the courtyard wall with turrets. The model is the whole compound,
fitted to that picture (`acr model-ref --sections 0`); fitted to `#STANDLO` alone it stood
as the front row only, right of the drawn compound and smaller. The section states 1..3
differ in roofs and detail, not in outline, and are not separate groups."""

from acr_assets import stonebuild

TYPE = "AtcKre"
CHECKS = ["--anim STANDLO --sections 0", "--anim STANDLO"]


def build(ctx):
    ctx.ref = ctx.refs.get("STANDLO-0", ctx.ref)
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
    # The sections: the keep (north-west), the west tower and its wings, the courtyard wall.
    k.box("stone", (-9.0, 6.0), (24.0, 12.0), 0.0, 1.2)  # courtyard terrace
    z = k.tiers("stone", (-11.0, 9.5), [(9.5, 9.0)], 1.2, [8.5], taper=0.05)  # keep
    k.cornice("stone", (-11.0, 9.5), (8.6, 8.1), z - 1.6, 0.4, 0.3)
    k.merlons("stone", (-11.0, 9.5), (8.6, 8.1), z, 0.9)
    k.door("stone", -11.0, 5.0, w=1.6, h=2.6, z0=1.2)
    t = k.tiers("stone", (-14.6, 6.4), [(2.6, 2.6), (2.0, 2.0)], z, [1.4, 1.0], taper=0.1)  # domed turret
    k.box("stone", (-14.6, 6.4), (1.2, 1.2), t, t + 1.0, taper=0.35)
    z = k.tiers("stone", (-21.5, -3.0), [(5.0, 5.0), (4.2, 4.2), (3.4, 3.4)], 0.0, [5.5, 4.5, 3.0], taper=0.04)
    for zz, s in ((5.5, 5.4), (10.0, 4.5)):
        k.cornice("stone", (-21.5, -3.0), (s, s), zz - 0.15, 0.35, 0.25)
    k.box("stone", (-21.5, -3.0), (2.6, 2.6), z, z + 2.5, taper=0.35)  # west tower top
    k.box("stone", (-19.0, 4.0), (4.0, 9.0), 0.0, 3.6)  # west wing
    k.cornice("stone", (-19.0, 4.0), (4.0, 9.0), 3.3, 0.3, 0.2)
    k.box("stone", (-18.0, -7.5), (7.0, 4.0), 0.0, 3.4)  # south-west wing
    k.cornice("stone", (-18.0, -7.5), (7.0, 4.0), 3.1, 0.3, 0.2)
    k.box("stone", (-14.5, -7.5), (2.0, 2.0), 3.4, 5.0, taper=0.2)
    k.box("stone", (0.5, 9.0), (14.0, 2.2), 0.0, 4.2)  # courtyard wall, north
    k.box("stone", (8.0, 5.0), (2.2, 8.0), 0.0, 4.2)  # and east, to the tower
    for x, y in ((-2.0, 9.0), (4.0, 9.0), (8.0, 6.0)):
        k.box("stone", (x, y), (1.8, 1.4), 4.2, 6.2)
    k.heap("rubble", (-4.0, 1.0), 20.0, 70, 0.6, 0.3, ring=0.85, squash=0.6)
    stonebuild.finish(ctx, k)
