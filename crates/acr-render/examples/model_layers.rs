//! `cargo run --release -p acr-render --example model_layers -- <model.glb>... [--steps N]
//! [--max-crossing N] [--max-poking N]`: skins each model through every clip and reports the
//! inner clothing layers that come out through outer ones (`acr_render::layers`,
//! docs/clothing.md), already in the rest pose (poking) or in some frame (crossing). Ends
//! with one summary line per model; fails when a model exceeds a limit.

use acr_render::layers;
use acr_render::model::ModelData;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (mut steps, mut max, mut max_poke, mut paths) = (24, None, None, Vec::new());
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--steps" => steps = args.next().ok_or("--steps needs a value")?.parse()?,
            "--max-crossing" => max = Some(args.next().ok_or("--max-crossing needs a value")?.parse::<usize>()?),
            "--max-poking" => max_poke = Some(args.next().ok_or("--max-poking needs a value")?.parse::<usize>()?),
            _ => paths.push(a),
        }
    }
    let mut summary = Vec::new();
    for path in &paths {
        let model = ModelData::from_glb(&std::fs::read(path)?)?;
        let reports = layers::check(&model, steps);
        println!("{path}");
        print!("{}", layers::table(&reports));
        let crossing: usize = reports.iter().map(|r| r.crossing).sum();
        let poking: usize = reports.iter().map(|r| r.poking).sum();
        summary.push((path.clone(), poking, crossing));
    }
    let mut failed = Vec::new();
    for (path, poking, crossing) in &summary {
        println!("summary {path} poking {poking} crossing {crossing}");
        if max.is_some_and(|m| *crossing > m) || max_poke.is_some_and(|m| *poking > m) {
            failed.push(path.as_str());
        }
    }
    if !failed.is_empty() {
        return Err(format!("over the limit: {}", failed.join(", ")).into());
    }
    Ok(())
}
