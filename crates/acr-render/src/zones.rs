//! Triangle zones of a body for per-zone wound volume culling (m2-perf4).
//!
//! A wound volume (`holes` in `model.wgsl`) changes nothing farther than 2.6 radii from its
//! capsule, yet a corpse with one wound drew all its parts through the `discard` pipelines
//! (no early depth test on the tiler). Every triangle of a model with a rig gets a zone: the
//! half of its body part (`_PART` 0..9; the limbs halved by `_LIMB`, trunk and head by
//! height), the gear (`_PART` −1) or the parts the look variants move. A vertex shared by
//! triangles of different zones is copied, so each vertex carries one zone (in `body.w`
//! above the hair variant: `variant + 1 + 256 × (zone + 1)`) and a triangle's three
//! vertices agree. Per instance [`mask`] marks the zones a volume can reach by their
//! rest-pose bounds; the vertex shader drops the other zones' triangles in the `discard`
//! pipelines and the reached ones in the clean pipelines, so each triangle draws once.

use crate::damage::Volume;
use crate::model::Vertex;

/// Zones: two per body part, the gear, the look variants' parts.
pub const COUNT: usize = 22;
const GEAR: u8 = 20;
const TOUCHED: u8 = 21;
/// The bit of zone 0 in the instance's `extra.z` (the player below).
pub const SHIFT: u32 = 8;
/// `extra.z`: the instance draws by zones (wound volumes alone, else clean).
pub const ZONED: u32 = 1 << 30;
/// `extra.z`: the inner layers show only through the wounds (not dissolved): the zones no
/// volume reaches leave them out.
pub const INNER_HIDDEN: u32 = 1 << 31;
/// How far a volume reaches, in radii (`holes`: `dist > r * 2.6` is skipped).
const REACH: f32 = 2.6;

/// Rest-pose bounds (min, max); empty when min > max.
pub type Bounds = [[f32; 3]; 2];

const EMPTY: Bounds = [[f32::MAX; 3], [f32::MIN; 3]];

fn grow(b: &mut Bounds, p: [f32; 3]) {
    for k in 0..3 {
        b[0][k] = b[0][k].min(p[k]);
        b[1][k] = b[1][k].max(p[k]);
    }
}

/// Assigns the zones of `vertices` referenced by `indices` (triangle lists), copying the
/// vertices whose triangles disagree; the first `touched` vertices (the look variants'
/// primitives) are one zone and never copied (a variant's buffer holds just them). Encodes
/// the zones into `body.w` of the vertices and of the `variants`; returns each zone's
/// bounds (the variants' positions included).
pub fn split(
    vertices: &mut Vec<Vertex>,
    indices: &mut [u32],
    touched: usize,
    variants: &mut [Vec<Vertex>],
) -> Vec<Bounds> {
    // Trunk and head halved at their middle height.
    let mut mid = [EMPTY; 2];
    for v in vertices.iter() {
        let part = v.body[0].round() as i32;
        if (0..2).contains(&part) {
            grow(&mut mid[part as usize], v.position);
        }
    }
    let natural = |i: usize, v: &Vertex| -> u8 {
        if i < touched {
            return TOUCHED;
        }
        match v.body[0].round() as i32 {
            p @ 0..=1 => {
                let m = &mid[p as usize];
                2 * p as u8 + u8::from(v.position[1] >= 0.5 * (m[0][1] + m[1][1]))
            }
            p @ 2..=9 => 2 * p as u8 + u8::from(v.body[1] >= 0.5),
            _ => GEAR,
        }
    };
    const UNSET: u8 = u8::MAX;
    let mut zone: Vec<u8> = vec![UNSET; vertices.len()];
    zone[..touched.min(vertices.len())].fill(TOUCHED);
    let mut copies: std::collections::HashMap<(u32, u8), u32> = std::collections::HashMap::new();
    for t in indices.as_chunks_mut::<3>().0 {
        let now: [u8; 3] = std::array::from_fn(|k| {
            let i = t[k] as usize;
            if zone[i] == UNSET { natural(i, &vertices[i]) } else { zone[i] }
        });
        // The zone most of its corners have (else the first corner's).
        let z = if now[1] == now[2] { now[1] } else { now[0] };
        for i in t.iter_mut() {
            let at = *i as usize;
            if zone[at] == UNSET {
                zone[at] = z;
            } else if zone[at] != z {
                *i = *copies.entry((*i, z)).or_insert_with(|| {
                    vertices.push(vertices[at]);
                    zone.push(z);
                    vertices.len() as u32 - 1
                });
            }
        }
    }
    let mut bounds = vec![EMPTY; COUNT];
    for (v, &z) in vertices.iter_mut().zip(&zone) {
        if z == UNSET {
            continue;
        }
        grow(&mut bounds[usize::from(z)], v.position);
        v.body[3] += 256.0 * f32::from(z + 1);
    }
    for variant in variants.iter_mut() {
        for (v, &z) in variant.iter_mut().zip(&zone) {
            if z == UNSET {
                continue;
            }
            grow(&mut bounds[usize::from(z)], v.position);
            v.body[3] += 256.0 * f32::from(z + 1);
        }
    }
    bounds
}

/// The zones (bit per zone) any of the `volumes` can change: each zone's bounds against the
/// capsule's axis widened by the reach of `holes` (the axis sampled, the samples' spacing
/// added to the reach, so never less than the capsule reaches).
pub fn mask(bounds: &[Bounds], volumes: &[Volume]) -> u32 {
    const STEPS: usize = 8;
    let mut m = 0;
    for v in volumes {
        let reach = v.radius.max(0.005) * REACH + 1e-3;
        let len = (0..3).map(|k| (v.b[k] - v.a[k]).powi(2)).sum::<f32>().sqrt();
        let r = reach + 0.5 * len / STEPS as f32;
        for (z, b) in bounds.iter().enumerate() {
            if m & (1 << z) != 0 || b[0][0] > b[1][0] {
                continue;
            }
            let hit = (0..STEPS).any(|s| {
                let t = (s as f32 + 0.5) / STEPS as f32;
                let d2: f32 = (0..3)
                    .map(|k| {
                        let p = v.a[k] + (v.b[k] - v.a[k]) * t;
                        (b[0][k] - p).max(p - b[1][k]).max(0.0).powi(2)
                    })
                    .sum();
                d2 <= r * r
            });
            if hit {
                m |= 1 << z;
            }
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vert(p: [f32; 3], part: f32, limb: f32) -> Vertex {
        Vertex {
            position: p,
            normal: [0.0, 1.0, 0.0],
            uv: [0.0; 2],
            color: [1.0; 4],
            joints: [0; 4],
            weights: [1.0, 0.0, 0.0, 0.0],
            morph: [[0.0; 3]; 2],
            body: [part, limb, 0.5, 0.0],
            face: [0; 4],
        }
    }

    #[test]
    fn triangles_agree_and_volumes_reach_their_zones() {
        // A trunk triangle sharing a vertex with an arm triangle.
        let mut v = vec![
            vert([0.0, 1.0, 0.0], 0.0, -1.0),
            vert([0.1, 1.2, 0.0], 0.0, -1.0),
            vert([0.2, 1.0, 0.0], 2.0, 0.1),
            vert([0.6, 1.0, 0.0], 2.0, 0.9),
            vert([0.6, 1.2, 0.0], 2.0, 0.9),
        ];
        let mut idx = vec![0, 1, 2, 2, 3, 4];
        let bounds = split(&mut v, &mut idx, 0, &mut []);
        let zone = |i: u32| (v[i as usize].body[3] / 256.0).round() as i32 - 1;
        for t in idx.chunks(3) {
            assert!(zone(t[0]) == zone(t[1]) && zone(t[1]) == zone(t[2]), "{t:?}");
        }
        assert_ne!(zone(idx[0]), zone(idx[3]));
        assert!(v.len() > 5, "the shared corner was copied");
        // A small volume at the arm's end reaches only the arm's outer half.
        let vol = Volume::sphere([0.6, 1.1, 0.0], 0.02, 0.0);
        let m = mask(&bounds, &[vol]);
        assert_eq!(m, 1 << zone(idx[3]));
        let big = Volume::sphere([0.3, 1.1, 0.0], 0.2, 0.0);
        assert_eq!(mask(&bounds, &[big]).count_ones(), 2);
    }
}
