# Cloth: pattern-based draping for game garments

The method of pattern tools (Marvelous Designer, CLO) done in Blender's cloth solver (Blender Manual, "Physics › Cloth"): flat panels with a grain, sewn together around the body, simulated once in the rest pose, then skinned. No cloth runs per frame in the game for hundreds of men.

## Drafting

- **Draft from measurements plus ease**, per garment: chest, waist, seat, neck, arm, thigh girths and lengths measured on the actual body. Then the same draft fits every build. Clamp measurements to a sane range so an outlier slice (armpit) does not widen a panel.
- **Panels are flat 2D polygons with the grain along one axis**, counter-clockwise outlines, named edges for seams. Mirror panels for the other side instead of drafting twice.
- **Pair seam edges by arc-length fraction**, not by sample count: two edges cut into different numbers of samples otherwise pair skewed and open holes.
- **Inlays** (lapels, turnbacks, facings) cut into their base panel as constraint edges, with their own material: one surface, a crisp outline, no z-fighting between two cloth sheets.
- **Mesh the panels evenly** (constrained Delaunay on a triangular lattice, e.g. `mathutils.geometry.delaunay_2d_cdt`), spacing chosen per garment: finer for small parts (2 cm stock), coarser for large plain areas (4–5 cm breeches).

## Placing and sewing

- **Wrap each panel round its body region** at a small gap (elliptic cylinders for the torso, round ones for limbs), arc length preserved, faces outwards. Torso panels go over the shoulders so shoulder seams start a few cm apart, not a body's depth.
- **Sew in a gravity-free phase**: sewing springs ramp up while gravity and shrink are off, so panels close before they fall. Then fade gravity in, drop the sewing force and let it settle. Keyframe these settings in one run.
- **Pin what is held**: waistbands, garters, a yoke on the shoulders. Pin only the middle of a band if its ends must still close a seam.
- **Weld** sewn pairs that ended within about one triangle edge; drop the rest of the sewing edges. Report the pairs that stayed open (and by how much): they are where the draft is wrong.

## Collision

- **Collision margins grow outwards per layer** (e.g. breeches 3 mm, waistcoat 4 mm, coat 6 mm). An outer garment must collide with the inner garments already settled, using the inner one's margin as its thickness.
- **Margin larger than the step travel**: thin colliders and fast settling tunnel. Raise quality steps rather than margin when cloth passes through.
- **Self-collision is expensive**; leave it off for fitted garments, consider it only for long loose cloaks that fold onto themselves.
- **Simulate in the rest pose (or a neutral stand pose), then skin.** A garment draped on a posed body must be inverse-skinned back to the rest pose before binding.

## After the simulation

- Single-sided cloth is enough when the renderer shades back faces; 2–3 mm of thickness is a tenth of a pixel at game zoom. Add a solidify rim only along visible hems.
- **Skin from the surface beneath** (see `rigging-and-animation.md`): the layer order then holds under every pose without per-frame collision.
- **Push covered inner layers in** a few mm towards their bones (and the skin under clothes ~1 cm), so a millimetre of crossing never shows; keep them, since cuts and decay reveal them.
- **Give every cloth layer a rank** the renderer can bias in depth (1 mm per rank) so tiny crossings do not flicker.
- **Verify across all clips**: skin the result through every clip and count inner vertices that come out through outer layers (and those poking already at rest).

## Tuning cheat sheet

| symptom | likely cause | first fix |
|---|---|---|
| seams gape | pairs skewed, sewing force too low, sew phase too short | pair by arc length; ramp force; more sew frames |
| garment slides off or bunches at the hem | no pin, too little friction, too much gravity early | pin waistband; delay gravity |
| crumpled shoulders, puffed back | panel placed far from the body or drafted too long | smaller gap; check draft against measurements |
| cloth through the body | margin too small, steps too few, collider mesh too coarse | raise quality; use the full-resolution body as collider |
| spikes after skinning | weights from the wrong surface (hand, other side) | restrict the weight source; exclude hands |
| jagged "teeth" at necklines | shells derived from the body hull, not draped cloth | draft and drape the garment |
