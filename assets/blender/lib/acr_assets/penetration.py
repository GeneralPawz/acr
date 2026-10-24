"""Body-gear penetration of the held musket, over every frame of every clip as played
(`stats.json` "musket_penetration"; m2-aim-hd).

Two directions, both against the skinned meshes of the man (body and outfit, the levels of
detail and the hands left out: the fists close round the stock by design, `contact_grip`):
- `gun_in_body`: musket vertices behind the nearest body or outfit surface (the butt in the
  chest, the barrel through the arm);
- `body_in_gun`: body or outfit vertices inside the musket's closed mesh.

A vertex counts when it lies deeper than `COUNT_M`; `depth_m` is the deepest one (distance to
the nearest surface it is behind). The summary and the tolerance check are plain Python
(`summarise`, `over`: tested without Blender); `measure` needs `bpy`.
"""

COUNT_M = 0.002  # m: a vertex deeper than this counts as inside
TOL_M = 0.01  # m: the deepest allowed penetration of a checked clip (`over`)
NEAR_M = 0.10  # m: body triangles this near the musket's box are tested against it
DEPTH_CAP_M = 0.04  # m: musket vertices further behind the nearest face are not counted (the
# sign test of an open garment shell is unreliable beyond); a depth of 0.04 means "0.04 or more"
HAND_BONES = ("hand_", "thumb_", "index_", "middle_", "ring_", "pinky_", "socket.")


def summarise(frames):
    """{"frames", "gun_in_body": {"max_n", "depth_m"}, "body_in_gun": {...}, "worst": frame}
    from [(frame, gun_in_body (n, depth), body_in_gun (n, depth))]."""
    out = {"frames": len(frames)}
    worst = (-1.0, None)
    for i, key in ((1, "gun_in_body"), (2, "body_in_gun")):
        n = max((f[i][0] for f in frames), default=0)
        d = max((f[i][1] for f in frames), default=0.0)
        out[key] = {"max_n": int(n), "depth_m": round(float(d), 4)}
    for f in frames:
        d = max(f[1][1], f[2][1])
        if d > worst[0]:
            worst = (d, f[0])
    out["worst_frame"] = worst[1]
    return out


def over(stats, tol=TOL_M, clips=None):
    """[(clip, depth m)] of the clips (all, or `clips`) of a "musket_penetration" stats dict
    whose deepest penetration either way is beyond `tol`."""
    bad = []
    for clip, s in sorted((stats or {}).get("clips", {}).items()):
        if clips is not None and clip not in clips:
            continue
        d = max(s["gun_in_body"]["depth_m"], s["body_in_gun"]["depth_m"])
        if d > tol:
            bad.append((clip, d))
    return bad


def total(per_clip):
    """The "all" line of a {clip: summary}: the deepest either way and where."""
    if not per_clip:
        return {}
    clip, s = max(per_clip.items(), key=lambda kv: (max(kv[1]["gun_in_body"]["depth_m"], kv[1]["body_in_gun"]["depth_m"]), kv[0]))
    return {"depth_m": max(s["gun_in_body"]["depth_m"], s["body_in_gun"]["depth_m"]), "worst": [clip, s["worst_frame"]],
            "clips_over_tol": [c for c, _ in over({"clips": per_clip})]}


# ----------------------------------------------------------------------------- Blender


def find_musket(arm_obj, socket="socket.hand_r"):
    """The musket (or matchlock) master held on `socket` (levels left out), or None."""
    for o in sorted(arm_obj.children, key=lambda o: o.name):
        if (o.type == "MESH" and o.parent_bone.endswith(socket) and "acr_lod" not in o
                and any(k in o.name for k in ("musket", "matchlock")) and "slung" not in o.name):
            return o
    return None


def _skinned(arm_obj):
    out = []
    for o in sorted(arm_obj.children_recursive, key=lambda o: o.name):
        if o.type != "MESH" or o.parent_type == "BONE" or "acr_lod" in o or o.hide_render:
            continue
        if any(m.type == "ARMATURE" for m in o.modifiers):
            out.append(o)
    return out


def show(objs):
    """Makes `objs` evaluated (shown in the viewport and the view layer, their armature
    modifiers on); returns a function that puts everything back."""
    import bpy

    was = []
    for o in objs:
        mods = [(m, m.show_viewport) for m in o.modifiers if m.type == "ARMATURE"]
        was.append((o, o.hide_viewport, o.hide_get(), mods))
        o.hide_viewport = False
        o.hide_set(False)
        for m, _v in mods:
            m.show_viewport = True
    bpy.context.view_layer.update()

    def restore():
        for o, hv, hg, mods in was:
            o.hide_viewport = hv
            o.hide_set(hg)
            for m, v in mods:
                m.show_viewport = v
    return restore


def outward(ob, np):
    """+1 when the mesh's triangles wind outwards (positive signed volume at rest), else -1:
    the side test of the nearest face's normal is flipped for a mesh wound inwards."""
    co = np.empty(len(ob.data.vertices) * 3)
    ob.data.vertices.foreach_get("co", co)
    co = co.reshape(-1, 3)
    t = _tris(ob.data, np)
    vol = float(np.einsum("ij,ij->i", co[t[:, 0]], np.cross(co[t[:, 1]], co[t[:, 2]])).sum())
    return 1.0 if vol >= 0 else -1.0


def _hand_mask(ob, np, bones=HAND_BONES):
    """True for the vertices whose strongest group is a hand, finger or socket bone."""
    names = {g.index: g.name for g in ob.vertex_groups}
    mask = np.zeros(len(ob.data.vertices), bool)
    for v in ob.data.vertices:
        best = max(v.groups, key=lambda g: g.weight, default=None)
        if best is not None and names.get(best.group, "").split(".")[-1].startswith(bones):
            mask[v.index] = True
    return mask


def _tris(me, np):
    me.calc_loop_triangles()
    t = np.empty(len(me.loop_triangles) * 3, np.int32)
    me.loop_triangles.foreach_get("vertices", t)
    return t.reshape(-1, 3)


def measure(arm_obj, clips, gun=None, extra=None):
    """{"clips": {clip: summary}, "all": ...} of the musket against the man over every frame
    of `clips` (action names), each played on `arm_obj`. `gun`: the musket object (default
    `find_musket`); `extra`: more skinned meshes to test (default the armature's)."""
    import bpy
    import numpy as np
    from mathutils import Vector
    from mathutils.bvhtree import BVHTree

    gun = gun or find_musket(arm_obj)
    if gun is None:
        return None
    bodies = extra if extra is not None else _skinned(arm_obj)
    gco = np.empty(len(gun.data.vertices) * 3)
    gun.data.vertices.foreach_get("co", gco)
    gco = gco.reshape(-1, 3)
    gtri = _tris(gun.data, np)
    gbvh = BVHTree.FromPolygons([tuple(c) for c in gco], [tuple(int(i) for i in t) for t in gtri])
    lo, hi = gco.min(0), gco.max(0)
    gwind = outward(gun, np)
    info = []
    for ob in bodies:
        tri = _tris(ob.data, np)
        hand = _hand_mask(ob, np)
        info.append((ob, tri[~hand[tri].any(1)], hand, outward(ob, np)))
    # The levels of detail are not evaluated meanwhile (they are hidden in the viewport); the
    # measured meshes are (a mesh hidden in the viewport keeps its last evaluated shape).
    hidden = [o for o in bpy.data.objects if "acr_lod" in o and not o.hide_viewport]
    for o in hidden:
        o.hide_viewport = True
    restore = show([i[0] for i in info] + [gun])
    per_clip = {}
    try:
        for clip in clips:
            if clip not in bpy.data.actions:
                continue
            act = bpy.data.actions[clip]
            f0, f1 = (int(round(x)) for x in act.frame_range)
            frames = []
            for f in range(f0, f1 + 1):
                from . import armature as am

                am.set_clip(arm_obj, clip, f)
                dg = bpy.context.evaluated_depsgraph_get()
                mg = np.array(gun.matrix_world)
                if abs(np.linalg.det(mg[:3, :3])) < 1e-9:
                    continue  # the musket hidden (its socket scaled to 0) on this frame
                gw = gco @ mg[:3, :3].T + mg[:3, 3]
                gin = [0, 0.0]
                bin_ = [0, 0.0]
                for ob, tri, hand, wind in info:
                    ev = ob.evaluated_get(dg)
                    me = ev.data
                    co = np.empty(len(me.vertices) * 3)
                    me.vertices.foreach_get("co", co)
                    mb = np.array(ob.matrix_world)
                    # A mirroring transform turns the triangles inside out: flip the side test.
                    flip = wind * (-1.0 if np.linalg.det(mb[:3, :3]) < 0 else 1.0)
                    co = co.reshape(-1, 3) @ mb[:3, :3].T + mb[:3, 3]
                    # Body vertices inside the musket (its own frame).
                    inv = np.linalg.inv(mg)
                    loc = co @ inv[:3, :3].T + inv[:3, 3]
                    cand = np.nonzero(~hand & (loc >= lo).all(1) & (loc <= hi).all(1))[0]
                    for i in cand:
                        p = Vector(loc[i])
                        hit = gbvh.find_nearest(p)
                        if hit[0] is not None and gwind * (p - hit[0]).dot(hit[1]) < 0:
                            if hit[3] > COUNT_M:
                                bin_[0] += 1
                            bin_[1] = max(bin_[1], hit[3])
                    # Musket vertices behind the body's surface near it.
                    blo, bhi = gw.min(0) - NEAR_M, gw.max(0) + NEAR_M
                    near = ((co >= blo) & (co <= bhi)).all(1)
                    sel = tri[near[tri].any(1)]
                    if not len(sel):
                        continue
                    bvh = BVHTree.FromPolygons([tuple(c) for c in co], [tuple(int(i) for i in t) for t in sel])
                    for p in gw:
                        hit = bvh.find_nearest(Vector(p), DEPTH_CAP_M)
                        if hit[0] is not None and flip * (Vector(p) - hit[0]).dot(hit[1]) < 0:
                            if hit[3] > COUNT_M:
                                gin[0] += 1
                            gin[1] = max(gin[1], hit[3])
                frames.append((f, tuple(gin), tuple(bin_)))
            per_clip[clip] = summarise(frames)
    finally:
        for o in hidden:
            o.hide_viewport = False
        restore()
        if arm_obj.animation_data:
            arm_obj.animation_data.action = None
    out = {"count_m": COUNT_M, "tol_m": TOL_M, "musket": gun.name, "bodies": [o.name for o in bodies],
           "clips": per_clip, "all": total(per_clip)}
    print("ACR musket penetration:", out["all"], {c: max(s["gun_in_body"]["depth_m"], s["body_in_gun"]["depth_m"])
                                                  for c, s in per_clip.items()})
    return out
