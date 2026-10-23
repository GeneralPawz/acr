---
name: blender-craft
description: "Industry practice for real-time game assets built by bpy scripts in headless Blender 5.x. Covers topology, budgets, normals, UVs, glTF limits, weights, IK and contacts, animation principles, cloth draping, bpy idioms and build speed. Use when modelling, rigging, animating or scripting Blender."
---

# Blender craft for real-time assets

General rules, each with its reason, adapted to a game whose renderer reads glTF 2.0, skins 4 influences, lights with Lambert, shows a soldier about 45 px tall at default zoom and up close in a free camera. Pipeline facts specific to ACR live in the `acr-blender-pipeline` skill.

Read the reference when you do the work:
- `references/mesh-and-materials.md` when modelling, decimating, unwrapping, texturing or exporting;
- `references/rigging-and-animation.md` when weighting, posing, solving IK or keying clips;
- `references/cloth.md` when draping or fixing garments;
- `references/bpy.md` when writing or speeding up build scripts.

## Ten rules that prevent most bad results

1. **Spend triangles on the silhouette and on joints.** At game zoom a hand is 2–3 px; the outline and the bends are what read. Detail that changes neither belongs in the texture.
2. **Deforming areas need loops across the bend.** Two or three edge loops at elbows, knees, shoulders and hips keep volume; a single loop collapses (the "candy wrapper"). Long thin triangles shade and skin badly.
3. **Normals must agree with the winding.** Recalculate outward after generating or boolean work; mark sharp edges only where the form is really sharp (each hard edge splits vertices).
4. **Bake what the engine cannot compute.** The renderer reads base colour only: ambient occlusion, cavity, folds and wear go into the albedo; no light direction, no specular.
5. **Weights: at most 4 bones per vertex, summed to 1, no weight on sockets.** The renderer reads one set of 4 joints; the exporter drops the weaker influences silently, so the game deforms differently from what you checked in Blender.
6. **Clothes take their weights from the surface beneath them**, not from bones by distance. Then layers move together and cannot cross under any pose.
7. **IK is a tool, the export is FK.** glTF stores node transforms only; solve IK in the script, write the result as keys, and check contacts on every frame, not just keys.
8. **Animate for the camera that sees it.** Strong silhouettes, clear key poses, readable timing on the sprite frame counts; arcs and weight make a drill look real, not more keys.
9. **Measure before judging.** Count triangles, gaps, crossings and misses in the build and write them to stats; look at a trimmed contact sheet only to confirm.
10. **Determinism.** Same inputs, same file: seeded randomness, sorted iteration, no wall-clock or hash-order dependence. Then a diff of stats or pixels means a real change.

## Order of work for a new asset

1. Reference: period sources, sprite frames, real measurements (height, weapon length, garment ease).
2. Blockout at real scale on the shared rig, posed in the key poses, checked against the sprite silhouette.
3. Final geometry within budget; clean normals; UVs at even texel density.
4. Weights; deformation checked in the extreme poses of every clip (deep lunge, prone, crawl).
5. Clips: key poses first, then breakdowns, then contacts solved per frame.
6. Bake textures; export; measure (stats, layer check, IoU); then pictures.

## glTF 2.0: what survives export

| survives | lost or changed |
|---|---|
| meshes (triangulated), split normals, UV maps, colour attributes (ACR: the active one as `COLOR_0`), `_`-prefixed point attributes | modifiers unless applied, n-gons as authored, loose edges, attributes on edges or faces |
| skins: joints and weights (4 per set, more sets only if asked) | bone constraints, IK, drivers, custom shapes, B-bone curves |
| node TRS animation (step, linear, cubic), one animation per action | F-curve modifiers, NLA blending, non-transform properties |
| shape keys as morph targets (positions; normals optional), with their default weights | drivers on shape keys (only the weights stored in clips or the default play) |
| Principled BSDF with image textures plugged in directly | procedural nodes, mix shaders, anything not a plain image chain |
| custom properties as `extras` | Blender object data not mapped by the exporter |

Check every bake chain ends as an image texture on the base colour before export. Apply generative modifiers in the script before adding shape keys: a mesh with shape keys cannot have modifiers applied.

## Sources worth naming

Blender Manual (Modeling, Rigging, Animation, Physics: Cloth; Python API reference); Khronos glTF 2.0 Specification and the Blender glTF exporter docs; meshoptimizer documentation (simplification, vertex cache); Mikktspace tangent space; Polycount wiki (topology, texel density, baking); "The Animator's Survival Kit" (Richard Williams) and "The Illusion of Life" (Thomas and Johnston) for animation principles; Marvelous Designer-style pattern draping for garments; period drill manuals (the British 1764 Manual Exercise) for military motion.
