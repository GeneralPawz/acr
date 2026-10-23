//! Levels of detail (docs/rendering-3d.md, "Levels of detail").
//!
//! Index-only: every level is a new index list over the primitive's own vertices, so skin
//! weights, morph targets, `_PART`/`_LIMB`/`_GORE`, the hair variant, decay colours and the
//! look variants' vertex buffers stay exactly as loaded. The lists come from quadric error
//! simplification on the rest pose ([`levels`], meshopt-rs) with the borders between body
//! parts and hair variants locked. Built when a model is added (no asset change).

use std::collections::{HashMap, HashSet};

use crate::model::Vertex;

/// Levels per primitive, the full mesh first.
pub const LEVELS: usize = 4;
/// Share of the triangles kept per level.
pub const RATIOS: [f32; LEVELS] = [1.0, 0.35, 0.12, 0.03];
/// Projected size (screen pixels across the model's culling sphere) below which a model
/// drops to level 1, 2, 3.
pub const SIZES: [f32; LEVELS - 1] = [520.0, 260.0, 160.0];
/// Hysteresis: a model returns to a finer level only this much above the threshold.
pub const HYSTERESIS: f32 = 1.15;
/// The finest level drawing the baked face and body variants (#283): each variant is a
/// draw call per primitive it touches, and per-draw cost dominates on a tiler once the
/// triangles are few; coarser levels draw the base look (hair and beards still vary).
pub const LOOKS_UP_TO: usize = 1;
/// The coarsest level for a damaged instance (cut, fractured, torn in two, or a severed
/// piece): the far meshes weld the body parts, so a triangle may join a kept part to a
/// discarded one or take its corner from another part's bones (a piece then drags
/// stretched triangles); the level-1 lists keep the borders between parts.
pub const DAMAGED_UP_TO: usize = FAR_FROM - 1;
/// The first level drawn with the cheaper fragment shader (`fs_far` in model.wgsl).
pub const FAR_FROM: usize = 2;
/// Shadow casters are drawn this many levels coarser than seen.
pub const SHADOW_STEP: usize = 1;
/// The extra list for shadow casters past the last level ([`levels`] returns it after the
/// levels): "sloppy" grid simplification to this share of the triangles, which keeps no seams
/// or borders (a shadow has neither) and only roughly the shape.
pub const SHADOW: usize = LEVELS;
pub const SHADOW_RATIO: f32 = 0.03;

/// The level for a projected size, staying at `last` within the hysteresis band.
pub fn choose(size_px: f32, last: Option<usize>) -> usize {
    let plain = SIZES.iter().take_while(|&&t| size_px < t).count();
    match last {
        // Coarser at once; finer only when clearly above the threshold.
        Some(l) if plain < l => SIZES.iter().take_while(|&&t| size_px < t * HYSTERESIS).count().min(l),
        _ => plain,
    }
}

/// The body part and hair variant of a vertex: borders between them never move.
fn region(v: &Vertex) -> (i32, i32) {
    (v.body[0].round() as i32, v.body[3].round() as i32)
}

/// Rest-pose position as the simplifier sees it.
#[derive(Clone, Copy)]
struct Pos([f32; 3]);

impl meshopt_rs::vertex::Position for Pos {
    fn pos(&self) -> [f32; 3] {
        self.0
    }
}

/// Bit-exact position key (the simplifier groups vertices by identical position).
fn key(p: [f32; 3]) -> [u32; 3] {
    p.map(f32::to_bits)
}

/// The positions to lock: those whose vertices (or the triangles around them) span two body
/// parts or hair variants. meshopt-rs (a port of meshoptimizer 0.15) has no vertex locks, but it
/// classifies a position shared by more than two vertices as `Locked`, so one unreferenced copy
/// appended to the vertices pins it.
fn locks(vertices: &[Vertex], indices: &[u32]) -> Vec<Pos> {
    let mut first: HashMap<[u32; 3], (i32, i32)> = HashMap::new();
    let mut locked: HashSet<[u32; 3]> = HashSet::new();
    for t in indices.as_chunks::<3>().0 {
        let r = t.map(|i| region(&vertices[i as usize]));
        let mixed = r[0] != r[1] || r[1] != r[2];
        for k in 0..3 {
            let p = key(vertices[t[k] as usize].position);
            if mixed || *first.entry(p).or_insert(r[k]) != r[k] {
                locked.insert(p);
            }
        }
    }
    let mut keys: Vec<[u32; 3]> = locked.into_iter().collect();
    keys.sort_unstable();
    keys.into_iter().map(|k| Pos(k.map(f32::from_bits))).collect()
}

fn simplify(indices: &[u32], pos: &[Pos], target: usize, error: f32) -> Vec<u32> {
    if target >= indices.len() {
        return indices.to_vec();
    }
    let mut dest = vec![0u32; indices.len()];
    let n = meshopt_rs::simplify::simplify(&mut dest, indices, pos, target, error);
    dest.truncate(n);
    dest
}

/// Simplification of the shape alone: vertices at one position (and in one region) are welded,
/// so hard edges and UV seams no longer stop it; each corner of a kept triangle then takes the
/// vertex at its position whose normal is nearest the triangle's (the far levels only: a UV
/// seam may smear across a few pixels there).
fn simplify_welded(vertices: &[Vertex], indices: &[u32], locks: &[Pos], target: usize, error: f32) -> Vec<u32> {
    simplify_welded_by(vertices, indices, locks, target, error, region)
}

/// [`simplify_welded`] with vertices welded only within one `region`.
fn simplify_welded_by(
    vertices: &[Vertex],
    indices: &[u32],
    locks: &[Pos],
    target: usize,
    error: f32,
    region: fn(&Vertex) -> (i32, i32),
) -> Vec<u32> {
    let mut ids: HashMap<([u32; 3], (i32, i32)), u32> = HashMap::new();
    let mut members: Vec<Vec<u32>> = Vec::new();
    let mut pos: Vec<Pos> = Vec::new();
    let welded: Vec<u32> = indices
        .iter()
        .map(|&i| {
            let v = &vertices[i as usize];
            let id = *ids.entry((key(v.position), region(v))).or_insert_with(|| {
                pos.push(Pos(v.position));
                members.push(Vec::new());
                pos.len() as u32 - 1
            });
            if !members[id as usize].contains(&i) {
                members[id as usize].push(i);
            }
            id
        })
        .collect();
    pos.extend_from_slice(locks);
    let kept = simplify(&welded, &pos, target, error);
    let mut out = Vec::with_capacity(kept.len());
    for t in kept.as_chunks::<3>().0 {
        let p = t.map(|w| pos[w as usize].0);
        let (u, v) = ((0..3).map(|c| p[1][c] - p[0][c]), (0..3).map(|c| p[2][c] - p[0][c]));
        let (u, v): (Vec<f32>, Vec<f32>) = (u.collect(), v.collect());
        let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
        for &w in t {
            let best = members[w as usize].iter().copied().max_by(|&a, &b| {
                let d = |i: u32| (0..3).map(|c| vertices[i as usize].normal[c] * n[c]).sum::<f32>();
                d(a).total_cmp(&d(b))
            });
            out.push(best.unwrap_or(0));
        }
    }
    out
}

/// The index lists of every level of one primitive; level 0 is `indices` itself.
///
/// Quadric error simplification (meshoptimizer's edge collapse onto existing vertices, so the
/// lists stay index-only): it keeps the silhouette and the attribute seams (positions shared
/// by vertices with other normals or UVs collapse together or not at all); the borders between
/// body parts and hair variants are locked. Each level simplifies the one before. Where the
/// seams stop it far short of the ratio, the far levels simplify the welded shape
/// ([`simplify_welded`]).
pub fn levels(vertices: &[Vertex], indices: &[u32]) -> [Vec<u32>; LEVELS + 1] {
    let mut out: [Vec<u32>; LEVELS + 1] = Default::default();
    out[0] = indices.to_vec();
    let tris = indices.len() / 3;
    if tris < 16 {
        for l in out.iter_mut().skip(1) {
            *l = indices.to_vec();
        }
        return out;
    }
    let locks = locks(vertices, indices);
    let mut pos: Vec<Pos> = vertices.iter().map(|v| Pos(v.position)).collect();
    pos.extend_from_slice(&locks);
    let mut prev = indices.to_vec();
    for (level, ratio) in RATIOS.iter().enumerate().skip(1) {
        let target = ((tris as f32 * ratio) as usize).max(4) * 3;
        let mut list = simplify(&prev, &pos, target, MAX_ERROR[level]);
        if level >= WELDED_FROM && list.len() > target * 3 / 2 {
            let welded = simplify_welded(vertices, &prev, &locks, target, MAX_ERROR[level]);
            if welded.len() < list.len() {
                list = welded;
            }
        }
        if !list.is_empty() && list.len() < prev.len() {
            prev = list;
        }
        out[level] = prev.clone();
    }
    let target = ((tris as f32 * SHADOW_RATIO) as usize).max(4) * 3;
    let mut dest = vec![0u32; prev.len()];
    let n = if target < prev.len() { meshopt_rs::simplify::simplify_sloppy(&mut dest, &prev, &pos, target) } else { 0 };
    dest.truncate(n);
    out[SHADOW] = if dest.is_empty() { prev } else { dest };
    out
}

/// The hair or beard variant of a vertex alone: the far mesh may join body parts (a cut then
/// discards along a coarser line) but never two hair variants (one at most is worn).
fn hair(v: &Vertex) -> (i32, i32) {
    (0, v.body[3].round() as i32)
}

/// The far meshes' share of a model's full triangles (per material), levels
/// [`FAR_FROM`].. .
pub const FAR_RATIO: [f32; LEVELS - FAR_FROM] = [0.12, 0.06];
/// The far meshes' error caps, relative to the extent of all the material's primitives
/// together (a soldier: about 1.8 m, so 3.6 and 7 cm).
pub const FAR_ERROR: [f32; LEVELS - FAR_FROM] = [0.02, 0.04];

/// A far mesh of one material (level `level` ≥ [`FAR_FROM`], built per model rather than
/// per primitive): `indices` are the concatenated lists of every primitive with that material
/// (into the model's vertices) at the level before, simplified as one welded shape with only
/// the hair variants apart, so the small gear (buttons, straps, buckles, the separate pieces
/// of a uniform) melts into the rest. `full` is the material's triangles at level 0.
pub fn far_mesh(vertices: &[Vertex], indices: &[u32], full: usize, level: usize) -> Vec<u32> {
    let (ratio, error) = (FAR_RATIO[level - FAR_FROM], FAR_ERROR[level - FAR_FROM]);
    let target = ((full as f32 * ratio) as usize).max(4) * 3;
    if indices.len() < 48 || target >= indices.len() {
        return indices.to_vec();
    }
    let list = simplify_welded_by(vertices, indices, &[], target, error, hair);
    if list.is_empty() || list.len() >= indices.len() { indices.to_vec() } else { list }
}

/// The shadow casters' list of a far mesh: "sloppy" grid simplification ([`SHADOW_RATIO`] of
/// the material's `full` triangles).
pub fn far_shadow(vertices: &[Vertex], indices: &[u32], full: usize) -> Vec<u32> {
    let target = ((full as f32 * SHADOW_RATIO) as usize).max(4) * 3;
    if target >= indices.len() {
        return indices.to_vec();
    }
    let pos: Vec<Pos> = vertices.iter().map(|v| Pos(v.position)).collect();
    let mut dest = vec![0u32; indices.len()];
    let n = meshopt_rs::simplify::simplify_sloppy(&mut dest, indices, &pos, target);
    dest.truncate(n);
    if dest.is_empty() { indices.to_vec() } else { dest }
}

/// The first level that may simplify the welded shape.
pub const WELDED_FROM: usize = 2;

/// The largest error per level the simplifier may accept, relative to the primitive's extent:
/// it stops short of the ratio rather than spoil the shape.
pub const MAX_ERROR: [f32; LEVELS] = [0.0, 0.01, 0.015, 0.02];

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(n: usize) -> (Vec<Vertex>, Vec<u32>) {
        let mut v = Vec::new();
        for y in 0..=n {
            for x in 0..=n {
                v.push(Vertex {
                    position: [x as f32 / n as f32, y as f32 / n as f32, 0.0],
                    normal: [0.0, 0.0, 1.0],
                    uv: [0.0; 2],
                    color: [1.0; 4],
                    joints: [0; 4],
                    weights: [1.0, 0.0, 0.0, 0.0],
                    morph: [[0.0; 3]; 2],
                    body: [-1.0, -1.0, 0.5, 0.0],
                    face: [0; 4],
                });
            }
        }
        let mut i = Vec::new();
        let w = n as u32 + 1;
        for y in 0..n as u32 {
            for x in 0..n as u32 {
                let a = y * w + x;
                i.extend([a, a + 1, a + w, a + 1, a + w + 1, a + w]);
            }
        }
        (v, i)
    }

    #[test]
    fn levels_shrink_towards_their_ratios() {
        let (v, i) = grid(60);
        let l = levels(&v, &i);
        let tris = i.len() / 3;
        assert_eq!(l[0], i);
        for k in 1..LEVELS {
            let n = l[k].len() / 3;
            assert!(n <= (tris as f32 * RATIOS[k]) as usize + 1, "level {k}: {n} of {tris}");
            assert!(n > 0 && n <= l[k - 1].len() / 3);
            assert!(l[k].iter().all(|&x| (x as usize) < v.len()));
        }
    }

    #[test]
    fn hair_variants_never_merge() {
        // Two sheets at the same place, the second a hair variant.
        let (mut v, mut i) = grid(30);
        let n = v.len() as u32;
        let second: Vec<Vertex> = v.iter().map(|x| Vertex { body: [-1.0, -1.0, 0.5, 2.0], ..*x }).collect();
        v.extend(second);
        let more: Vec<u32> = i.iter().map(|k| k + n).collect();
        i.extend(more);
        let l = levels(&v, &i);
        for t in l[2].as_chunks::<3>().0 {
            let h: Vec<f32> = t.iter().map(|&k| v[k as usize].body[3]).collect();
            assert!(h.iter().all(|&x| x == h[0]), "a triangle across hair variants");
        }
    }

    #[test]
    fn choice_has_hysteresis() {
        assert_eq!(choose(1000.0, None), 0);
        assert_eq!(choose(50.0, None), 3);
        let t = SIZES[0];
        // Just below the threshold: coarser.
        assert_eq!(choose(t * 0.99, Some(0)), 1);
        // Back just above it: stays coarse; well above: finer.
        assert_eq!(choose(t * 1.05, Some(1)), 1);
        assert_eq!(choose(t * 1.2, Some(1)), 0);
    }

    /// Filled pixels of the triangles projected along `axis` into an `n` × `n` grid over `lo..hi`.
    fn silhouette(tris: &[[[f32; 3]; 3]], axis: usize, lo: [f32; 3], hi: [f32; 3], n: usize) -> usize {
        let (a, b) = ((axis + 1) % 3, (axis + 2) % 3);
        let s = (hi[a] - lo[a]).max(hi[b] - lo[b]).max(1e-6) / n as f32;
        let mut grid = vec![false; n * n];
        for t in tris {
            let p = t.map(|v| [(v[a] - lo[a]) / s, (v[b] - lo[b]) / s]);
            let x0 = p.iter().map(|q| q[0]).fold(f32::MAX, f32::min).floor().max(0.0) as usize;
            let x1 = (p.iter().map(|q| q[0]).fold(f32::MIN, f32::max).ceil() as usize).min(n - 1);
            let y0 = p.iter().map(|q| q[1]).fold(f32::MAX, f32::min).floor().max(0.0) as usize;
            let y1 = (p.iter().map(|q| q[1]).fold(f32::MIN, f32::max).ceil() as usize).min(n - 1);
            let e = |u: [f32; 2], v: [f32; 2], x: f32, y: f32| (v[0] - u[0]) * (y - u[1]) - (v[1] - u[1]) * (x - u[0]);
            for y in y0..=y1 {
                for x in x0..=x1 {
                    let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
                    let w = [e(p[0], p[1], fx, fy), e(p[1], p[2], fx, fy), e(p[2], p[0], fx, fy)];
                    if w.iter().all(|&w| w >= 0.0) || w.iter().all(|&w| w <= 0.0) {
                        grid[y * n + x] = true;
                    }
                }
            }
        }
        grid.iter().filter(|&&f| f).count()
    }

    /// The levels of real models: `ACR_LOD_MODELS=<folder of .glb> cargo test --release -p
    /// acr-render lod::tests::real_models -- --ignored --nocapture`. Prints triangles and the
    /// silhouette area per level against level 0 (three axes, the worst one).
    #[test]
    #[ignore]
    fn real_models() {
        let Ok(dir) = std::env::var("ACR_LOD_MODELS") else { return };
        let mut paths: Vec<_> = std::fs::read_dir(dir).unwrap().flatten().map(|e| e.path()).collect();
        paths.retain(|p| p.extension().is_some_and(|e| e == "glb"));
        paths.sort();
        for path in paths {
            let Ok(data) = crate::model::ModelData::from_glb(&std::fs::read(&path).unwrap()) else { continue };
            let t = std::time::Instant::now();
            let mut tris: [Vec<[[f32; 3]; 3]>; LEVELS] = Default::default();
            // The last level as the renderer builds it: one far mesh per material.
            let mut all: Vec<Vertex> = Vec::new();
            let mut groups: Vec<(usize, Vec<u32>, usize)> = Vec::new();
            for p in &data.primitives {
                let l = levels(&p.vertices, &p.indices);
                for k in 0..FAR_FROM {
                    tris[k].extend(l[k].as_chunks::<3>().0.iter().map(|t| t.map(|i| p.vertices[i as usize].position)));
                }
                let base = all.len() as u32;
                all.extend_from_slice(&p.vertices);
                let g = match groups.iter().position(|g| g.0 == p.material) {
                    Some(g) => g,
                    None => {
                        groups.push((p.material, Vec::new(), 0));
                        groups.len() - 1
                    }
                };
                groups[g].1.extend(l[FAR_FROM - 1].iter().map(|i| i + base));
                groups[g].2 += p.indices.len() / 3;
            }
            let mut shadow_tris = 0;
            for (_, near, full) in &groups {
                let mut list = near.clone();
                for (k, t) in tris.iter_mut().enumerate().skip(FAR_FROM) {
                    list = far_mesh(&all, &list, *full, k);
                    t.extend(list.as_chunks::<3>().0.iter().map(|t| t.map(|i| all[i as usize].position)));
                }
                shadow_tris += far_shadow(&all, &list, *full).len() / 3;
            }
            let ms = t.elapsed().as_secs_f32() * 1000.0;
            if std::env::var("ACR_LOD_PRIMS").is_ok() {
                for p in &data.primitives {
                    let l = levels(&p.vertices, &p.indices);
                    let locks = locks(&p.vertices, &p.indices).len();
                    let mut parts: Vec<(i32, i32)> = p.vertices.iter().map(region).collect();
                    parts.sort_unstable();
                    parts.dedup();
                    println!(
                        "  prim mat {} verts {} locks {} regions {} tris {:?}",
                        p.material,
                        p.vertices.len(),
                        locks,
                        parts.len(),
                        l.each_ref().map(|x| x.len() / 3)
                    );
                }
            }
            let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
            for v in tris[0].iter().flatten() {
                for c in 0..3 {
                    lo[c] = lo[c].min(v[c]);
                    hi[c] = hi[c].max(v[c]);
                }
            }
            let mut line = format!("{:<14} {:>6.0} ms", path.file_stem().unwrap().to_string_lossy(), ms);
            for k in 0..LEVELS {
                // At 400 px and at the size the level is first used.
                let px = if k == 0 { 400 } else { SIZES[k - 1] as usize };
                let worst = |n: usize| {
                    (0..3)
                        .map(|axis| {
                            let a0 = silhouette(&tris[0], axis, lo, hi, n) as f32;
                            silhouette(&tris[k], axis, lo, hi, n) as f32 / a0.max(1.0) - 1.0
                        })
                        .fold(0.0f32, |w, d| if d.abs() > w.abs() { d } else { w })
                };
                line += &format!(
                    " | L{k} {:>6} tris {:+5.1} / {:+5.1} %",
                    tris[k].len(),
                    worst(400) * 100.0,
                    worst(px) * 100.0
                );
            }
            println!("{line} | shadow {shadow_tris} tris, {} materials", groups.len());
        }
    }
}
