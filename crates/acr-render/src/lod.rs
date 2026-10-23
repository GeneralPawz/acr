//! Levels of detail (docs/rendering-3d.md, "Levels of detail").
//!
//! Index-only: every level is a new index list over the primitive's own vertices, so skin
//! weights, morph targets, `_PART`/`_LIMB`/`_GORE`, the hair variant, decay colours and the
//! look variants' vertex buffers stay exactly as loaded. The lists come from vertex
//! clustering on the rest pose: vertices in one grid cell that belong to the same body part,
//! dominant joint and hair variant collapse onto the one nearest the cell's mean, and the
//! triangles that collapse vanish. The cell size is searched per level for the ratio of
//! [`RATIOS`]. Built when a model is added (a few ms per model, no asset change).

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

use crate::model::Vertex;

/// Levels per primitive, the full mesh first.
pub const LEVELS: usize = 4;
/// Share of the triangles kept per level.
pub const RATIOS: [f32; LEVELS] = [1.0, 0.35, 0.12, 0.03];
/// Projected size (screen pixels across the model's culling sphere) below which a model
/// drops to level 1, 2, 3.
pub const SIZES: [f32; LEVELS - 1] = [520.0, 260.0, 110.0];
/// Hysteresis: a model returns to a finer level only this much above the threshold.
pub const HYSTERESIS: f32 = 1.15;
/// The finest level drawing the baked face and body variants (#283): each variant is a
/// draw call per primitive it touches, and per-draw cost dominates on a tiler once the
/// triangles are few; coarser levels draw the base look (hair and beards still vary).
pub const LOOKS_UP_TO: usize = 1;
/// Shadow casters are drawn this many levels coarser than seen.
pub const SHADOW_STEP: usize = 1;

/// The level for a projected size, staying at `last` within the hysteresis band.
pub fn choose(size_px: f32, last: Option<usize>) -> usize {
    let plain = SIZES.iter().take_while(|&&t| size_px < t).count();
    match last {
        // Coarser at once; finer only when clearly above the threshold.
        Some(l) if plain < l => SIZES.iter().take_while(|&&t| size_px < t * HYSTERESIS).count().min(l),
        _ => plain,
    }
}

#[derive(Default)]
struct KeyHasher(u64);

impl Hasher for KeyHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 ^ u64::from(b)).wrapping_mul(0x100_0000_01b3);
        }
    }
    fn write_u64(&mut self, v: u64) {
        self.0 = (self.0.rotate_left(29) ^ v).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    }
}

type Map<K, V> = HashMap<K, V, BuildHasherDefault<KeyHasher>>;

fn group(v: &Vertex) -> u64 {
    let (mut j, mut w) = (0, f32::MIN);
    for k in 0..4 {
        if v.weights[k] > w {
            (j, w) = (v.joints[k], v.weights[k]);
        }
    }
    let part = (v.body[0] + 1.0).clamp(0.0, 255.0) as u64;
    let hair = v.body[3].clamp(0.0, 255.0) as u64;
    (u64::from(j) & 0xfff) | part << 12 | hair << 20
}

/// The index list of one clustering with cells of `cell` metres.
fn cluster(vertices: &[Vertex], used: &[u32], groups: &[u64], indices: &[u32], cell: f32) -> Vec<u32> {
    let key = |i: u32| {
        let p = vertices[i as usize].position;
        let q = |c: f32| ((c / cell).floor() as i64 & 0xffff) as u64;
        (q(p[0]) | q(p[1]) << 16 | q(p[2]) << 32, groups[i as usize])
    };
    // Mean per cell, then the vertex nearest to it.
    let mut sums: Map<(u64, u64), ([f32; 3], f32)> = Map::default();
    for &i in used {
        let p = vertices[i as usize].position;
        let e = sums.entry(key(i)).or_insert(([0.0; 3], 0.0));
        (0..3).for_each(|c| e.0[c] += p[c]);
        e.1 += 1.0;
    }
    let mut best: Map<(u64, u64), (u32, f32)> = Map::default();
    for &i in used {
        let k = key(i);
        let (s, n) = sums[&k];
        let p = vertices[i as usize].position;
        let d: f32 = (0..3).map(|c| (p[c] - s[c] / n).powi(2)).sum();
        let e = best.entry(k).or_insert((i, f32::MAX));
        if d < e.1 {
            *e = (i, d);
        }
    }
    let mut out = Vec::new();
    let mut seen: Map<(u32, u32, u32), ()> = Map::default();
    for t in indices.as_chunks::<3>().0 {
        let r = [best[&key(t[0])].0, best[&key(t[1])].0, best[&key(t[2])].0];
        if r[0] == r[1] || r[1] == r[2] || r[0] == r[2] {
            continue;
        }
        // Rotate so the smallest comes first: the same triangle twice is dropped.
        let m = (0..3).min_by_key(|&k| r[k]).unwrap_or(0);
        let c = (r[m], r[(m + 1) % 3], r[(m + 2) % 3]);
        if seen.insert(c, ()).is_none() {
            out.extend([c.0, c.1, c.2]);
        }
    }
    out
}

/// The index lists of every level of one primitive; level 0 is `indices` itself.
pub fn levels(vertices: &[Vertex], indices: &[u32]) -> [Vec<u32>; LEVELS] {
    let mut out: [Vec<u32>; LEVELS] = Default::default();
    out[0] = indices.to_vec();
    let tris = indices.len() / 3;
    let mut used: Vec<u32> = indices.to_vec();
    used.sort_unstable();
    used.dedup();
    if tris < 16 || used.is_empty() {
        for l in out.iter_mut().skip(1) {
            *l = indices.to_vec();
        }
        return out;
    }
    let groups: Vec<u64> = vertices.iter().map(group).collect();
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for &i in &used {
        let p = vertices[i as usize].position;
        for c in 0..3 {
            lo[c] = lo[c].min(p[c]);
            hi[c] = hi[c].max(p[c]);
        }
    }
    let diag = (0..3).map(|c| (hi[c] - lo[c]).powi(2)).sum::<f32>().sqrt().max(1e-4);
    let mut prev = indices.to_vec();
    let mut low = diag / 2000.0;
    for (level, ratio) in RATIOS.iter().enumerate().skip(1) {
        let target = ((tris as f32 * ratio) as usize).max(4);
        // The smallest cell (log scale) giving at most `target` triangles.
        let mut a = low.ln();
        let mut b = diag.ln();
        let mut found: Option<(f32, Vec<u32>)> = None;
        for _ in 0..9 {
            let m = 0.5 * (a + b);
            let list = cluster(vertices, &used, &groups, indices, m.exp());
            if list.len() / 3 <= target {
                b = m;
                found = Some((m.exp(), list));
            } else {
                a = m;
            }
        }
        let (cell, list) = found.unwrap_or_else(|| (diag, cluster(vertices, &used, &groups, indices, diag)));
        // Never more than the finer level; keep a little when everything collapsed.
        let list = if list.is_empty() || list.len() > prev.len() { prev.clone() } else { list };
        low = cell;
        out[level] = list.clone();
        prev = list;
    }
    out
}

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
}
