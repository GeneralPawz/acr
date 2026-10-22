//! `cargo run --release -p acr-render --example model_layers -- <model.glb>... [--steps N]
//! [--max-crossing N]`: skins each model through every clip and reports the inner clothing
//! layers that come out through outer ones (`acr_render::layers`, docs/clothing.md).

use acr_render::layers;
use acr_render::model::ModelData;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (mut steps, mut max, mut paths) = (24, None, Vec::new());
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--steps" => steps = args.next().ok_or("--steps needs a value")?.parse()?,
            "--max-crossing" => max = Some(args.next().ok_or("--max-crossing needs a value")?.parse::<usize>()?),
            _ => paths.push(a),
        }
    }
    let mut worst = 0;
    for path in &paths {
        let model = ModelData::from_glb(&std::fs::read(path)?)?;
        let reports = layers::check(&model, steps);
        println!("{path}");
        print!("{}", layers::table(&reports));
        worst = worst.max(reports.iter().map(|r| r.crossing).sum());
    }
    if let Some(max) = max
        && worst > max
    {
        return Err(format!("{worst} crossing vertices, more than {max}").into());
    }
    Ok(())
}
