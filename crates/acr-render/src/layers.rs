//! Layer check of dressed models (docs/clothing.md, "Layers"): skins a model through every
//! clip and finds the vertices of an inner layer (skin, breeches, waistcoat, ...) that come
//! out through a layer above them (coat, facings, crossbelts, ...).
//!
//! In the rest pose every inner vertex that lies under an outer layer (its outward
//! projection meets an outer triangle within [`REACH`]) is bound to the outer triangles
//! around it. In each sampled frame the vertex is skinned again and its signed distance to
//! the nearest of those triangles (along the posed triangle's normal, only where it projects
//! inside the triangle) tells whether it is still underneath. A positive distance above
//! [`TOLERANCE`] is a crossing.

use crate::model::ModelData;

type V3 = [f32; 3];

/// Inner vertices further than this from the outer layer (m) are not under it.
pub const REACH: f32 = 0.04;
/// Outer triangles within this radius of the inner vertex (m) are its candidates.
const CANDIDATES: f32 = 0.04;
/// Crossings shallower than this (m) are not counted (the shader's layer bias hides them).
pub const TOLERANCE: f32 = 0.001;

/// Depth bias per layer rank (m, `model.wgsl`): belts (7) draw 7 mm nearer the camera than
/// the skin, a coat 1 mm nearer than the waistcoat.
pub const LAYER_BIAS: f32 = 0.001;

/// The layer order of a material key (`humans.LAYER_OF`): skin at the bottom, gear on top;
/// `None` for materials that are not layers (hair, bones, the musket, a stump cap).
pub fn layer_rank(key: &str) -> Option<u8> {
    Some(match key {
        "skin" => 0,
        "breeches" | "stockings" | "hose" => 1,
        "gaiters" | "garter" | "boots" | "boots_high" | "jackboots" | "slops" => 2,
        "shirt" | "stock" => 3,
        "waistcoat" | "doublet" | "dsleeve" => 4,
        "coat" | "tunic" | "cloak" | "buff" => 5,
        "facings" | "lining" | "sash" | "fallband" => 6,
        "belts" | "brass" => 7,
        "cartridge" | "knapsack" => 8,
        _ => return None,
    })
}

/// The material key of a material name (`engm18.coat.outfit.001.gltf` -> `coat`).
pub fn material_key(name: &str) -> &str {
    name.split('.').nth(1).unwrap_or(name)
}

/// One layer pair's result.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PairReport {
    pub inner: String,
    pub outer: String,
    /// Inner vertices covered by the outer layer in the rest pose.
    pub covered: usize,
    /// Of them, the ones that cross in at least one sampled frame.
    pub crossing: usize,
    /// Crossing (vertex, frame) samples.
    pub samples: usize,
    /// The deepest crossing (m) and where: clip name and frame.
    pub worst: f32,
    pub worst_at: (String, usize),
}

struct Mesh {
    key: String,
    rank: u8,
    /// Rest positions with their joints and weights.
    verts: Vec<(V3, [u32; 4], [f32; 4])>,
    tris: Vec<[u32; 3]>,
}

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn normal(pos: &[V3], t: [u32; 3]) -> V3 {
    let (a, b, c) = (pos[t[0] as usize], pos[t[1] as usize], pos[t[2] as usize]);
    let n = cross(sub(b, a), sub(c, a));
    let l = dot(n, n).sqrt().max(1e-12);
    [n[0] / l, n[1] / l, n[2] / l]
}

/// Signed distance of `p` above the triangle's plane along its normal and whether `p`
/// projects inside the triangle (with a small margin).
fn over(p: V3, a: V3, b: V3, c: V3) -> Option<f32> {
    let n = cross(sub(b, a), sub(c, a));
    let len = dot(n, n).sqrt();
    if len < 1e-12 {
        return None;
    }
    let n = [n[0] / len, n[1] / len, n[2] / len];
    let d = dot(sub(p, a), n);
    let q = sub(p, [n[0] * d, n[1] * d, n[2] * d]);
    let e = 1e-3 * len.sqrt().max(1e-3);
    let inside = [(a, b), (b, c), (c, a)].iter().all(|&(u, v)| dot(cross(sub(v, u), sub(q, u)), n) >= -e * e);
    inside.then_some(d)
}

fn skin(palette: &crate::model::Palette, v: &(V3, [u32; 4], [f32; 4])) -> V3 {
    let mut out = [0.0; 3];
    for k in 0..4 {
        let w = v.2[k];
        if w == 0.0 {
            continue;
        }
        let rows = &palette[v.1[k] as usize];
        for (i, o) in out.iter_mut().enumerate() {
            *o += w * (rows[i][0] * v.0[0] + rows[i][1] * v.0[1] + rows[i][2] * v.0[2] + rows[i][3]);
        }
    }
    out
}

fn meshes(model: &ModelData) -> Vec<Mesh> {
    let mut out: Vec<Mesh> = Vec::new();
    for p in &model.primitives {
        let key = material_key(&model.materials[p.material].name).to_owned();
        let Some(rank) = layer_rank(&key) else { continue };
        let base = out.iter().position(|m| m.key == key);
        let m = match base {
            Some(i) => &mut out[i],
            None => {
                out.push(Mesh { key, rank, verts: Vec::new(), tris: Vec::new() });
                out.last_mut().unwrap()
            }
        };
        let off = m.verts.len() as u32;
        m.verts.extend(p.vertices.iter().map(|v| (v.position, v.joints, v.weights)));
        m.tris.extend(p.indices.as_chunks::<3>().0.iter().map(|t| [t[0] + off, t[1] + off, t[2] + off]));
    }
    out
}

/// Checks every pair of layers through every clip at `steps` frames each (plus the rest
/// pose).
pub fn check(model: &ModelData, steps: usize) -> Vec<PairReport> {
    let meshes = meshes(model);
    let rest = model.pose(None, 0.0);
    let posed: Vec<Vec<V3>> = meshes.iter().map(|m| m.verts.iter().map(|v| skin(&rest, v)).collect()).collect();
    // Frames: (clip name, frame, palette).
    let mut frames = Vec::new();
    for (ci, clip) in model.clips.iter().enumerate() {
        for f in 0..steps {
            let t = clip.duration * f as f32 / steps.max(1) as f32;
            frames.push((clip.name.clone(), f, model.pose(Some(ci), t)));
        }
    }
    let frame_pos: Vec<Vec<Vec<V3>>> = frames
        .iter()
        .map(|(_, _, pal)| meshes.iter().map(|m| m.verts.iter().map(|v| skin(pal, v)).collect()).collect())
        .collect();
    let mut out = Vec::new();
    for (ii, inner) in meshes.iter().enumerate() {
        for (oi, outer) in meshes.iter().enumerate() {
            if outer.rank <= inner.rank {
                continue;
            }
            let op = &posed[oi];
            let centres: Vec<(V3, f32)> = outer
                .tris
                .iter()
                .map(|t| {
                    let (a, b, c) = (op[t[0] as usize], op[t[1] as usize], op[t[2] as usize]);
                    let m = [(a[0] + b[0] + c[0]) / 3.0, (a[1] + b[1] + c[1]) / 3.0, (a[2] + b[2] + c[2]) / 3.0];
                    let r = [a, b, c].iter().map(|&p| dot(sub(p, m), sub(p, m)).sqrt()).fold(0.0, f32::max);
                    (m, r)
                })
                .collect();
            let mut report = PairReport { inner: inner.key.clone(), outer: outer.key.clone(), ..Default::default() };
            for (vi, &p) in posed[ii].iter().enumerate() {
                let cands: Vec<usize> = centres
                    .iter()
                    .enumerate()
                    .filter(|(_, (m, r))| dot(sub(p, *m), sub(p, *m)).sqrt() < CANDIDATES + r)
                    .map(|(i, _)| i)
                    .collect();
                let nearest = |pos: &[V3], q: V3| -> Option<(usize, f32)> {
                    cands
                        .iter()
                        .filter_map(|&ti| {
                            let t = outer.tris[ti];
                            over(q, pos[t[0] as usize], pos[t[1] as usize], pos[t[2] as usize]).map(|d| (ti, d))
                        })
                        .min_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
                };
                // Covered in the rest pose: under an outer triangle, within reach.
                let Some((t0, d)) = nearest(op, p) else { continue };
                if d >= 0.0 || d <= -REACH {
                    continue;
                }
                // Only the outer surface over the vertex, not another one facing it (the
                // torso's coat under a sleeve that swings in): triangles turned like the one
                // above the vertex.
                let n0 = normal(op, outer.tris[t0]);
                let cands: Vec<usize> =
                    cands.iter().copied().filter(|&t| dot(normal(op, outer.tris[t]), n0) > 0.5).collect();
                let nearest = |pos: &[V3], q: V3| -> Option<f32> {
                    cands
                        .iter()
                        .filter_map(|&t| {
                            let t = outer.tris[t];
                            over(q, pos[t[0] as usize], pos[t[1] as usize], pos[t[2] as usize])
                        })
                        .min_by(|a, b| a.abs().total_cmp(&b.abs()))
                };
                report.covered += 1;
                let mut crossed = false;
                for (fi, fp) in frame_pos.iter().enumerate() {
                    let q = fp[ii][vi];
                    if let Some(d) = nearest(&fp[oi], q)
                        && d > TOLERANCE
                        && d < REACH
                    {
                        crossed = true;
                        report.samples += 1;
                        if d > report.worst {
                            report.worst = d;
                            report.worst_at = (frames[fi].0.clone(), frames[fi].1);
                        }
                    }
                }
                report.crossing += usize::from(crossed);
            }
            if report.covered > 0 {
                out.push(report);
            }
        }
    }
    out
}

/// A plain-text table of the reports.
pub fn table(reports: &[PairReport]) -> String {
    use std::fmt::Write as _;
    let mut s = String::from("inner      outer      covered crossing samples worst_mm  at\n");
    for r in reports {
        let _ = writeln!(
            s,
            "{:<10} {:<10} {:>7} {:>8} {:>7} {:>8.1}  {} {}",
            r.inner,
            r.outer,
            r.covered,
            r.crossing,
            r.samples,
            r.worst * 1000.0,
            r.worst_at.0,
            r.worst_at.1
        );
    }
    let total: usize = reports.iter().map(|r| r.crossing).sum();
    let _ = writeln!(s, "crossing vertices: {total}");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranks_follow_the_dressing_order() {
        let order = ["skin", "breeches", "gaiters", "shirt", "waistcoat", "coat", "facings", "belts", "cartridge"];
        let ranks: Vec<u8> = order.iter().map(|k| layer_rank(k).unwrap()).collect();
        assert!(ranks.windows(2).all(|w| w[0] < w[1]), "{ranks:?}");
        assert_eq!(layer_rank("hair"), None);
        assert_eq!(material_key("engm18.coat.outfit.001.gltf"), "coat");
    }

    /// The built musketeer (`ACR_MODELS_DIR`, skipped without it): the crossbelts stay on the
    /// coat (they take its weights), within a few vertices.
    #[test]
    fn belts_stay_on_the_coat() {
        let Some(dir) = std::env::var_os("ACR_MODELS_DIR") else { return };
        let Ok(bytes) = std::fs::read(std::path::Path::new(&dir).join("engm18.glb")) else { return };
        let model = ModelData::from_glb(&bytes).unwrap();
        let reports = check(&model, 12);
        let belts: usize = reports.iter().filter(|r| r.outer == "belts").map(|r| r.crossing).sum();
        assert!(belts <= 12, "{}", table(&reports));
    }
}
