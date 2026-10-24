//! `cargo run --release -p acr-render --example model_diff -- <a.glb> <b.glb>`: how far two
//! builds of one model differ as the renderer loads them (#199: a packed or quantised glb
//! against its source). Prints the vertex, primitive and clip counts, the largest change of
//! rest positions, morph offsets (mm), normals (degrees) and UVs, the clip names and lengths,
//! and the largest distance between the two skinned meshes over every clip at 9 times
//! (mm), and each file's size and parse time. Exit code 1 when the counts or clips differ.

use acr_render::model::{ModelData, Palette};

fn load(p: &str) -> ModelData {
    let bytes = std::fs::read(p).unwrap_or_else(|e| panic!("{p}: {e}"));
    ModelData::from_glb(&bytes).unwrap_or_else(|e| panic!("{p}: {e}"))
}

fn skin(palette: &Palette, p: [f32; 3], j: [u32; 4], w: [f32; 4]) -> [f32; 3] {
    let mut out = [0.0; 3];
    for k in 0..4 {
        if w[k] == 0.0 {
            continue;
        }
        let rows = &palette[j[k] as usize];
        for (i, o) in out.iter_mut().enumerate() {
            *o += w[k] * (rows[i][0] * p[0] + rows[i][1] * p[1] + rows[i][2] * p[2] + rows[i][3]);
        }
    }
    out
}

fn dist(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [a, b] = [&args[0], &args[1]].map(|p| {
        // Parse time: the best of 5 loads.
        let mut best = f64::MAX;
        for _ in 0..5 {
            let t = std::time::Instant::now();
            drop(load(p));
            best = best.min(t.elapsed().as_secs_f64());
        }
        let size = std::fs::metadata(p).map_or(0, |m| m.len());
        println!("{p}: {:.2} MB, parsed in {:.1} ms", size as f64 / 1e6, best * 1000.0);
        load(p)
    });
    let mut bad = false;
    let verts = |m: &ModelData| m.primitives.iter().map(|p| p.vertices.len()).sum::<usize>();
    println!("primitives {} / {}, vertices {} / {}", a.primitives.len(), b.primitives.len(), verts(&a), verts(&b));
    bad |= a.primitives.len() != b.primitives.len() || verts(&a) != verts(&b);
    let (mut pos, mut morph, mut nrm, mut uv, mut wt) = (0f32, 0f32, 0f32, 0f32, 0f32);
    for (pa, pb) in a.primitives.iter().zip(&b.primitives) {
        bad |= pa.indices != pb.indices || pa.material != pb.material;
        for (va, vb) in pa.vertices.iter().zip(&pb.vertices) {
            pos = pos.max(dist(va.position, vb.position));
            for k in 0..2 {
                morph = morph.max(dist(va.morph[k], vb.morph[k]));
            }
            let d = (va.normal[0] * vb.normal[0] + va.normal[1] * vb.normal[1] + va.normal[2] * vb.normal[2])
                / (dist(va.normal, [0.0; 3]) * dist(vb.normal, [0.0; 3])).max(1e-12);
            nrm = nrm.max(d.clamp(-1.0, 1.0).acos().to_degrees());
            uv = uv.max((va.uv[0] - vb.uv[0]).abs().max((va.uv[1] - vb.uv[1]).abs()));
            wt = wt.max((0..4).map(|k| (va.weights[k] - vb.weights[k]).abs()).fold(0.0, f32::max));
            bad |= va.joints != vb.joints;
        }
    }
    println!(
        "rest position {:.4} mm, morph {:.4} mm, normal {:.3} deg, uv {:.6}, weight {:.5}",
        pos * 1000.0,
        morph * 1000.0,
        nrm,
        uv,
        wt
    );
    let clips = |m: &ModelData| m.clips.iter().map(|c| (c.name.clone(), c.duration)).collect::<Vec<_>>();
    let same_clips = clips(&a) == clips(&b);
    println!(
        "clips {} / {}, names and lengths {}",
        a.clips.len(),
        b.clips.len(),
        if same_clips { "same" } else { "DIFFER" }
    );
    bad |= !same_clips;
    let mut worst = (0f32, String::new());
    let mut clip_ids: Vec<Option<usize>> = vec![None];
    clip_ids.extend((0..a.clips.len().min(b.clips.len())).map(Some));
    for c in clip_ids {
        let len = c.map_or(0.0, |c| a.clips[c].duration);
        for i in 0..9 {
            let t = len * i as f32 / 8.0;
            let (qa, qb) = (a.pose(c, t), b.pose(c, t));
            for (pa, pb) in a.primitives.iter().zip(&b.primitives) {
                for (va, vb) in pa.vertices.iter().zip(&pb.vertices) {
                    let d = dist(
                        skin(&qa, va.position, va.joints, va.weights),
                        skin(&qb, vb.position, vb.joints, vb.weights),
                    );
                    if d > worst.0 {
                        let name = c.map_or("bind".to_owned(), |c| a.clips[c].name.clone());
                        worst = (d, format!("{name} t={t:.3}"));
                    }
                }
            }
        }
    }
    println!("skinned {:.4} mm (worst {})", worst.0 * 1000.0, worst.1);
    if bad {
        println!("DIFFER");
        std::process::exit(1);
    }
}
