//! `cargo run --release -p acr-render --example gear_volume -- <model.glb>... [--steps N]
//! [--keys knapsack,cartridge,...]`: solid gear against the body (docs/assets-humans.md,
//! "Seated gear"). `model_layers` treats every outer layer as a sheet, which misreads a closed
//! solid; this skins each model through every clip (and the rest pose) and counts the body
//! and clothing vertices that lie inside a closed item: each closed, connected piece of the
//! gear materials (`--keys`, default the carried gear) and the slung musket (the mesh on the
//! `socket.sling` node), by parity of a ray through its triangles. Prints per item and
//! penetrating material the vertices inside in some frame, the worst clip and frame.

use std::collections::HashMap;

use acr_render::layers::material_key;
use acr_render::model::{ModelData, Palette, Slot};

type V3 = [f32; 3];
/// A skinned vertex: position, palette slots, weights.
type SkinV = (V3, [u32; 4], [f32; 4]);

const KEYS: &[&str] = &["knapsack", "cartridge", "quiver", "hide", "shield", "horn", "pouch", "leather", "drum"];

struct Item {
    name: String,
    prim: usize,
    /// (position, joints, weights) and triangles (indices into them).
    verts: Vec<(V3, [u32; 4], [f32; 4])>,
    tris: Vec<[usize; 3]>,
}

fn skin(palette: &Palette, v: &(V3, [u32; 4], [f32; 4])) -> V3 {
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

/// Closed connected pieces of the triangles `tris` over `pos` (vertices welded by position).
fn closed_pieces(pos: &[V3], tris: &[[usize; 3]]) -> Vec<Vec<usize>> {
    let key = |p: V3| [(p[0] * 1e4).round() as i64, (p[1] * 1e4).round() as i64, (p[2] * 1e4).round() as i64];
    let mut weld: HashMap<[i64; 3], usize> = HashMap::new();
    let id: Vec<usize> = pos
        .iter()
        .map(|&p| {
            let n = weld.len();
            *weld.entry(key(p)).or_insert(n)
        })
        .collect();
    // Union-find over welded vertices.
    let mut parent: Vec<usize> = (0..weld.len()).collect();
    fn find(p: &mut [usize], mut i: usize) -> usize {
        while p[i] != i {
            p[i] = p[p[i]];
            i = p[i];
        }
        i
    }
    for t in tris {
        let (a, b, c) = (find(&mut parent, id[t[0]]), find(&mut parent, id[t[1]]), find(&mut parent, id[t[2]]));
        parent[b] = a;
        let c2 = find(&mut parent, c);
        parent[c2] = a;
    }
    let mut pieces: HashMap<usize, Vec<usize>> = HashMap::new();
    for (ti, t) in tris.iter().enumerate() {
        let r = find(&mut parent, id[t[0]]);
        pieces.entry(r).or_default().push(ti);
    }
    pieces
        .into_values()
        .filter(|ts| {
            let mut edges: HashMap<(usize, usize), i32> = HashMap::new();
            for &ti in ts {
                let t = tris[ti];
                for k in 0..3 {
                    let (a, b) = (id[t[k]], id[t[(k + 1) % 3]]);
                    *edges.entry((a.min(b), a.max(b))).or_default() += 1;
                }
            }
            ts.len() >= 4 && edges.values().all(|&n| n == 2)
        })
        .collect()
}

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Inside the closed triangles `tris` (positions `p`): parity of a skewed ray's crossings.
fn inside(q: V3, p: &[V3], tris: &[[usize; 3]]) -> bool {
    let d = [0.5773, 0.5774, 0.5775];
    let mut n = 0;
    for t in tris {
        let (a, b, c) = (p[t[0]], p[t[1]], p[t[2]]);
        let (e1, e2) = (sub(b, a), sub(c, a));
        let h = cross(d, e2);
        let det = dot(e1, h);
        if det.abs() < 1e-12 {
            continue;
        }
        let s = sub(q, a);
        let u = dot(s, h) / det;
        if !(0.0..=1.0).contains(&u) {
            continue;
        }
        let qv = cross(s, e1);
        let v = dot(d, qv) / det;
        if v < 0.0 || u + v > 1.0 {
            continue;
        }
        if dot(e2, qv) / det > 0.0 {
            n += 1;
        }
    }
    n % 2 == 1
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (mut steps, mut keys, mut paths) =
        (24usize, KEYS.iter().map(|s| s.to_string()).collect::<Vec<_>>(), Vec::new());
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--steps" => steps = args.next().ok_or("--steps needs a value")?.parse()?,
            "--keys" => keys = args.next().ok_or("--keys needs a value")?.split(',').map(str::to_owned).collect(),
            _ => paths.push(a),
        }
    }
    for path in &paths {
        let model = ModelData::from_glb(&std::fs::read(path)?)?;
        let sling = |p: &acr_render::model::Primitive| {
            p.vertices.first().is_some_and(|v| match model.slots.get(v.joints[0] as usize) {
                Some(Slot::Node(n)) => {
                    let mut n = Some(*n);
                    while let Some(i) = n {
                        if model.nodes[i].name.ends_with("socket.sling") || model.nodes[i].name.contains("musket_slung")
                        {
                            return true;
                        }
                        n = model.nodes[i].parent;
                    }
                    false
                }
                _ => false,
            })
        };
        // Items and the rest (the body and clothes tested against them).
        let mut items: Vec<Item> = Vec::new();
        let mut others: Vec<(String, usize, Vec<SkinV>)> = Vec::new();
        for (pi, p) in model.primitives.iter().enumerate() {
            let key = material_key(&model.materials[p.material].name).to_owned();
            let verts: Vec<_> = p.vertices.iter().map(|v| (v.position, v.joints, v.weights)).collect();
            let is_sling = sling(p);
            if is_sling || keys.contains(&key) {
                let tris: Vec<[usize; 3]> = p
                    .indices
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .map(|t| [t[0] as usize, t[1] as usize, t[2] as usize])
                    .collect();
                let pos: Vec<V3> = verts.iter().map(|v| v.0).collect();
                for (k, piece) in closed_pieces(&pos, &tris).into_iter().enumerate() {
                    let name = if is_sling { format!("musket_slung/{key}#{k}") } else { format!("{key}#{k}") };
                    items.push(Item {
                        name,
                        prim: pi,
                        verts: verts.clone(),
                        tris: piece.iter().map(|&i| tris[i]).collect(),
                    });
                }
            }
            if !is_sling && !["wood", "metal", "belts"].contains(&key.as_str()) || keys.contains(&key) {
                others.push((key, pi, verts));
            }
        }
        let mut frames = vec![("rest".to_owned(), 0usize, model.pose(None, 0.0))];
        for (ci, clip) in model.clips.iter().enumerate() {
            for f in 0..steps {
                frames.push((clip.name.clone(), f, model.pose(Some(ci), clip.duration * f as f32 / steps as f32)));
            }
        }
        println!("{path}: {} closed items, {} frames", items.len(), frames.len());
        for it in &items {
            // (other key) -> (vertices inside in some frame, worst count, worst clip/frame)
            let mut hit: HashMap<String, (std::collections::HashSet<usize>, usize, String)> = HashMap::new();
            let mut rest_in = 0;
            for (clip, f, pal) in &frames {
                let p: Vec<V3> = it.verts.iter().map(|v| skin(pal, v)).collect();
                let used: Vec<usize> = it.tris.iter().flatten().copied().collect();
                let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
                for &i in &used {
                    for k in 0..3 {
                        lo[k] = lo[k].min(p[i][k]);
                        hi[k] = hi[k].max(p[i][k]);
                    }
                }
                for (key, prim, verts) in &others {
                    if *prim == it.prim {
                        continue;
                    }
                    let mut count = 0;
                    let e = hit.entry(key.clone()).or_insert_with(|| (Default::default(), 0, String::new()));
                    for (vi, v) in verts.iter().enumerate() {
                        let q = skin(pal, v);
                        if (0..3).any(|k| q[k] < lo[k] || q[k] > hi[k]) {
                            continue;
                        }
                        if inside(q, &p, &it.tris) {
                            count += 1;
                            e.0.insert(vi);
                        }
                    }
                    if count > e.1 {
                        e.1 = count;
                        e.2 = format!("{clip}@{f}");
                    }
                    if clip == "rest" {
                        rest_in += count;
                    }
                }
            }
            let mut rows: Vec<_> = hit.into_iter().filter(|(_, h)| !h.0.is_empty()).collect();
            rows.sort_by_key(|(_, h)| std::cmp::Reverse(h.0.len()));
            let total: usize = rows.iter().map(|(_, h)| h.0.len()).sum();
            println!("  {:<24} inside: {total} vertices in some frame, {rest_in} at rest", it.name);
            for (key, (set, worst, at)) in rows {
                println!("    {key:<14} {:>5} vertices, worst frame {worst} ({at})", set.len());
            }
        }
    }
    Ok(())
}
