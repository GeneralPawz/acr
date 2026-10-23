# Mesh, shading, UVs, materials, export

## Topology and edge flow for deformation

- **Loops follow the deformation, not the outline.** Rings around limbs, loops round the shoulder and hip that follow the muscle and joint direction. Why: linear blend skinning interpolates vertices between bones; a ring across the bend axis lets the bend spread over several rings, keeping volume.
- **Two to three loops per hinge joint** (elbow, knee, finger knuckles if they bend), a little denser on the inside of the bend. One loop pinches; five waste triangles that the LOD pass then removes unevenly.
- **Keep poles (vertices with 3 or 5+ edges) off joints and off the face's moving parts.** They pinch and shade unevenly under deformation.
- **Rigid parts stay rigid.** Cuirass, buttons, buckles, gun parts: separate pieces or single-bone weights. Bending a rigid object reads as rubber.
- **Generated meshes** (Skin modifier hulls, Delaunay cloth lattices, decimated scans): check for inconsistent winding, slivers (angle under ~10°), non-manifold edges and duplicate faces after generation; `Mesh.validate()` and `bmesh.ops.recalc_face_normals` are cheap. A face twice over the same vertices makes the glTF exporter report "mesh not valid".
- **Thin two-sided parts** (fletching, blades, flags): two faces on separate vertices a fraction of a millimetre apart, or a closed thin lens; never one face duplicated over the same vertices.
- **Cloth meshes for simulation want even, roughly equilateral triangles** at a spacing that matches the smallest fold you want (ACR coat: 3.5 cm). Quads or uneven triangles bias the folds.

## Triangle budgets and LODs

- **Set the budget by screen size.** At about 45 px for a 1.8 m figure, 1 px ≈ 4 cm: features under 2–3 px (fingers, buttons as geometry, eyelids) only matter in the close-up camera. Spend on the silhouette (hat brim, coat skirts, weapon) and on deforming joints.
- **Budget per part, check per part.** A total hides a 3k-triangle buckle. Record per-layer counts in the build's stats.
- **Decimate in the order: planar → collapse with symmetry → protect seams.** Collapse decimation with symmetry keeps faces mirror-identical; check UV seams and part borders afterwards.
- **LOD practice:** each level about half the previous, switching by projected size; keep the silhouette and attribute seams (UV, material, skin part) locked; drop interior or covered geometry first. Automatic simplifiers (meshoptimizer-style quadric error) do this well on clean meshes and badly on triangle soup, so a clean level 0 matters more than hand LODs.
- **Hidden geometry costs twice:** vertex work and overdraw. Delete faces that are always covered (skin under a full coat) only when every pose and every damage state keeps them covered; cut or decaying bodies need them.

## Normals and shading

- Smooth shading everywhere except true creases; in Blender 4.1+ sharpness is an edge attribute (`sharp_edge`, set with "Shade Auto Smooth" or `Mesh.set_sharp_from_angle`), the old mesh auto-smooth flag is gone.
- Every split normal or hard edge duplicates vertices in glTF. Hard edges on cloth or skin look faceted under a Lambert light.
- Recalculate outward after any generation, boolean or mirror. A renderer without back-face culling hides flipped faces from the eye but not from layer offsets, push-outs or lighting.
- Custom normals (data transfer from a smooth proxy) hide seams where separate meshes meet (head on neck, sleeve on hand).
- Morph targets export without normals in ACR: big shape changes keep the base normals, so keep morphs moderate or accept slightly wrong shading.

## UVs and texel density

- **Even texel density across a character** (pixels per metre on the surface), raised only where the eye goes (face). Why: uneven density shows as blurry and sharp patches side by side.
- **Seams where they are hidden or natural:** garment seams, inner arm, under the chin, the back of the head.
- **Padding (gutters) ≥ 2 px at the smallest mip you use, more for small islands.** Mipmaps average across islands; too little padding bleeds neighbour colours along seams. A margin given as a fraction of the whole atlas shrinks below a pixel when the island sits in a small cell.
- **Straighten islands of woven or striped materials** so the weave, stripes and lace run with the garment's grain.
- **One atlas per character** keeps draw calls and texture switches low; one material slot per distinct look still needs its own primitive.

## Materials: PBR and what a simple renderer reads

- **Albedo is the surface colour without light.** Plausible sRGB ranges: charcoal ~50, dark wool ~40–70, white linen ~200–230, never pure 0 or 255. Lit or shadowed sprite colours must be de-lit before use.
- When the renderer reads base colour only (Lambert, no normal or roughness maps), bake **ambient occlusion, cavity and fold shading** into the albedo at moderate strength (form reads, but no light direction). Baked highlights or cast shadows look wrong when the unit turns.
- Metal and leather need their look painted into the albedo (darker, slight value variation, edge wear) because no specular term will help.
- Keep materials simple for export: Principled BSDF, Image Texture straight into Base Color; bake procedural nodes to images first.
- Alpha: prefer geometry or alpha MASK (cutout) over BLEND for hair cards and fringes: blended transparency sorts badly and costs overdraw.

## Export (glTF 2.0)

- Triangulate deliberately (Triangulate modifier, "Beauty") rather than leaving the exporter to pick diagonals on bent quads.
- Apply transforms (scale 1, rotation 0) on meshes and armatures before skinning; unapplied scale on an armature leaks into every bone track.
- Name everything: nodes, materials, actions, attributes. Engines look things up by name.
- Images: PNG for lossless masks and atlases; JPEG/WebP only where the engine decodes them. KTX2/Basis needs an external tool.
- Compression (meshopt, Draco) needs a decoder in the engine and quantises UVs and positions; measure the visual change before turning it on.
