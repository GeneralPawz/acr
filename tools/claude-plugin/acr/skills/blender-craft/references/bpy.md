# bpy scripting idioms for headless Blender 5.x builds

Authority: the Blender Python API reference ("Gotchas" pages) and the Blender Manual.

## Data API over operators

- **Prefer `bpy.data` and object/mesh methods to `bpy.ops`.** Operators depend on context (active object, selection, mode, area), update the whole scene each call and are slow in loops. Create with `bpy.data.meshes.new` + `Mesh.from_pydata`, link with `collection.objects.link`, parent and set matrices directly.
- **Where an operator is unavoidable** (decimate apply, smart UV project, bake, glTF export), give it an explicit context. Blender 4.0+ removed dict overrides; use

  ```python
  with bpy.context.temp_override(object=o, active_object=o, selected_objects=[o]):
      bpy.ops.object.modifier_apply(modifier=m.name)
  ```

  In `-b` there are no windows or 3D-view areas: operators that poll for a `VIEW_3D` area fail; use their data-API equivalent.
- **Modifiers:** apply with `temp_override`, or read the result without applying (below). A mesh with shape keys cannot have modifiers applied: apply first, add shape keys after.

## Evaluated data and the depsgraph

- After changing pose bones, constraints, shape key values or modifiers, call `bpy.context.view_layer.update()` before reading matrices or positions; otherwise you read last update's values.
- `scene.frame_set(f)` evaluates animation for frame f (and updates); it is the expensive call in per-frame loops.
- Final positions with modifiers (armature, shape keys) applied:

  ```python
  dg = bpy.context.evaluated_depsgraph_get()
  ev = obj.evaluated_get(dg)
  me = ev.to_mesh()            # temporary mesh owned by ev
  ...                          # read me.vertices / foreach_get
  ev.to_mesh_clear()
  ```

- `pose_bone.matrix` is armature space (posed); `bone.matrix_local` is armature space at rest; world = `obj.matrix_world @ ...`. `pose_bone.head`/`tail` are posed, armature space.

## bmesh

- Use `bmesh` for topology work: `bm = bmesh.new(); bm.from_mesh(me)` … `bm.to_mesh(me); bm.free()`.
- `bmesh.ops.*` (delete, recalc_face_normals, remove_doubles, triangulate, bisect_plane) are fast and context-free.
- **Indices are not stable**: `bmesh.ops.delete` and others renumber. Key your data by `BMVert`/`BMFace` objects or a custom layer, and call `ensure_lookup_table()` before indexing.
- `bm.faces.new(verts)` raises "face already exists" for a duplicate: check `bm.faces.get(verts)` first in generators.

## Speed with numpy

- Read and write in bulk: `me.vertices.foreach_get("co", buf)` into a flat `numpy` float32 array, then `reshape(-1, 3)`; `foreach_set` back and `me.update()`. Orders of magnitude faster than per-vertex Python.
- Vertex group weights have no `foreach_get`; build them once per mesh into a dict or array and reuse.
- Nearest-surface queries: build a `mathutils.bvhtree.BVHTree` once (`FromPolygons` or `FromObject`) and reuse; `kdtree` for nearest points.

## Blender 5.x changes that bite

- **Layered actions:** `action.fcurves` is empty for new actions; F-curves live in `action.layers[].strips[].channelbags[].fcurves`. Assign `animation_data.action_slot` (e.g. `act.slots[0]`) after setting `animation_data.action`, or the action animates nothing.
- **`Mesh.materials.clear()` also removes the `material_index` attribute**: every face falls to slot 0. Save the indices and restore them, or replace slots in place.
- **New shape keys start at value 1.0** in 5.2: set `kb.value = 0.0` unless you want it on.
- Auto smooth is an edge attribute / modifier since 4.1 (`Mesh.set_sharp_from_angle`), not a mesh flag.
- Two armatures sharing one action export no animation in the glTF `ACTIONS` mode: merge them into one armature with prefixed bones.

## Determinism

- Seed every random source: `random.Random(seed)`, `numpy.random.default_rng(seed)`; derive per-part seeds from names with a stable hash (`zlib.crc32`), never Python's `hash()` of a string (randomised per process).
- **Iterate in a defined order**: sort sets and dict keys of names before generating geometry or keys; a `set` of strings iterates differently in every process.
- No wall-clock, no file-system order (`sorted(os.listdir())`), no thread-count dependence. Simulations: the same inputs, frame range and settings give the same result in the same Blender version; reset the cache explicitly.
- Write measured numbers to a stats file so a rebuild can be compared without pictures.

## Headless use

- Run `blender -b --factory-startup --python build.py -- <args>`: no user preferences or add-ons, arguments after `--` read from `sys.argv`.
- Catch exceptions at the top, print the traceback and `sys.exit(1)`: Blender otherwise exits 0 after a failing script.
- Print progress with `flush=True` for long steps; time them with `time.perf_counter()` into the stats.
- Save a `.blend` only on request (inspection); it is slow and large.

## Performance of builds

- **Profile first**: time each stage (body, drape, bake, clips, export) into stats; the sum tells where minutes go.
- `frame_set` and `view_layer.update` dominate per-frame loops: batch all edits of a frame, update once, read everything you need, move on. Solve per frame only where contacts require it.
- Bakes: smallest resolution that holds the look, few samples for AO and position/normal passes, bake once per character atlas not per part.
- Cloth: cost ∝ vertices × quality steps × frames. Coarser spacing on plain panels, fewer settle frames, no self-collision.
- Decimate imported sources once and cache only outside the repo; never commit derived data from bought sources.
- Remove temporary meshes and objects (`bpy.data.meshes.remove(me)`) in long builds; orphan data slows later lookups and export.
