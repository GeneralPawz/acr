//! Sprite commands: contact sheets, decoder checks, blend tables and palettes.

use std::collections::BTreeMap;
use std::path::Path;

use acr_formats::raster::{Image, RasterOptions, draw_frame};
use acr_formats::{GpFile, Palette, Vfs};

use crate::{Command, Result};

pub const COMMANDS: &[Command] = &[
    Command {
        name: "gp-sheet",
        usage: "<game-dir> <gp-path> <out.png> [--pal PATH] [--player N] [--only ENC,ENC..] [--frames FROM..TO]",
        run: |a| match a {
            [dir, gp, out, rest @ ..] => {
                SheetOptions::parse(rest).and_then(|o| gp_sheet(Path::new(dir), gp, Path::new(out), &o))
            }
            _ => Err(crate::WRONG_ARGS.into()),
        },
    },
    Command {
        name: "gp-check",
        usage: "<game-dir> [filter]",
        run: |a| match a {
            [dir, rest @ ..] => gp_check(Path::new(dir), rest.first().copied()),
            _ => Err(crate::WRONG_ARGS.into()),
        },
    },
    Command {
        name: "grd-fit",
        usage: "<game-dir>   (blend strengths of the sprite lookup tables)",
        run: |a| match a {
            [dir] => grd_fit(Path::new(dir)),
            _ => Err(crate::WRONG_ARGS.into()),
        },
    },
    Command {
        name: "gp-palette",
        usage: "<game-dir> [filter]   (rank palettes per sprite pack)",
        run: |a| match a {
            [dir, rest @ ..] => gp_palette(Path::new(dir), rest.first().copied()),
            _ => Err(crate::WRONG_ARGS.into()),
        },
    },
];

struct SheetOptions {
    /// Defaults to the palette the sprite pack was drawn for.
    palette: Option<String>,
    player: u8,
    only: Option<Vec<u8>>,
    frames: Option<std::ops::Range<usize>>,
}

impl SheetOptions {
    fn parse(args: &[&str]) -> Result<Self> {
        let mut o = Self { palette: None, player: 0, only: None, frames: None };
        let mut it = args.iter();
        while let Some(&flag) = it.next() {
            let value = *it.next().ok_or_else(|| format!("{flag} needs a value"))?;
            match flag {
                "--pal" => o.palette = Some(value.to_owned()),
                "--player" => o.player = value.parse()?,
                "--only" => o.only = Some(value.split(',').map(str::parse).collect::<std::result::Result<_, _>>()?),
                "--frames" => {
                    let (a, b) = value.split_once("..").ok_or("--frames expects FROM..TO")?;
                    o.frames = Some(a.parse()?..b.parse()?);
                }
                _ => return Err(format!("unknown option {flag}").into()),
            }
        }
        Ok(o)
    }
}

/// Renders the frames of a sprite pack into one grid image.
fn gp_sheet(dir: &Path, gp: &str, out: &Path, o: &SheetOptions) -> Result<()> {
    let mut vfs = Vfs::open_game_dir(dir)?;
    let pal_path = o.palette.as_deref().unwrap_or(acr_formats::sprite_palette(gp));
    let palette = crate::images::load_palette(&mut vfs, pal_path)?;
    let file = GpFile::parse(vfs.read(gp)?)?;

    let range = o.frames.clone().unwrap_or(0..file.frame_count());
    let n = file.frame_count();
    let frames: Vec<_> = (range.start.min(n)..range.end.min(n)).map(|i| file.decode_frame(i)).collect();
    for (i, f) in frames.iter().enumerate() {
        if let Err(e) = f {
            eprintln!("frame {i}: {e}");
        }
    }
    let ok: Vec<_> = frames.iter().flatten().collect();
    // Use one cell size for all frames so animations line up.
    let (mut x0, mut y0, mut x1, mut y1) = (0, 0, 1, 1);
    for (a, b, c, d) in ok.iter().filter_map(|f| f.bounds()) {
        (x0, y0, x1, y1) = (x0.min(a), y0.min(b), x1.max(c), y1.max(d));
    }
    let (cell_w, cell_h) = ((x1 - x0 + 2) as u32, (y1 - y0 + 2) as u32);
    let cols = (ok.len() as f64).sqrt().ceil().max(1.0) as u32;
    let rows = (ok.len() as u32).div_ceil(cols).max(1);
    let mut image = Image::new(cols * cell_w, rows * cell_h);
    let opt = RasterOptions { palette: &palette, player: o.player, only: o.only.as_deref() };
    for (i, frame) in ok.iter().enumerate() {
        let (cx, cy) = ((i as u32 % cols) * cell_w, (i as u32 / cols) * cell_h);
        draw_frame(&mut image, frame, cx as i32 - x0 + 1, cy as i32 - y0 + 1, &opt);
    }
    crate::write_png(out, &image)?;
    eprintln!("{} of {} frames, cell {cell_w}x{cell_h} -> {}", ok.len(), frames.len(), out.display());
    Ok(())
}

/// Decodes every sprite pack and reports failures grouped by reason.
fn gp_check(dir: &Path, filter: Option<&str>) -> Result<()> {
    let mut vfs = Vfs::open_game_dir(dir)?;
    let paths: Vec<String> = vfs
        .list()
        .filter(|(p, _)| p.to_ascii_uppercase().ends_with(".GP") && crate::matches(p, filter))
        .map(|(p, _)| p.to_owned())
        .collect();
    let (mut frames, mut layers) = (0usize, 0usize);
    let mut failures: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for path in &paths {
        let file = match GpFile::parse(vfs.read(path)?) {
            Ok(f) => f,
            Err(e) => {
                failures.entry(e.reason.to_owned()).or_default().push(path.clone());
                continue;
            }
        };
        for i in 0..file.frame_count() {
            frames += 1;
            match file.decode_frame(i) {
                Ok(f) => layers += f.layers.len(),
                Err(e) => failures.entry(e.reason.to_owned()).or_default().push(format!("{path}#{i}")),
            }
        }
    }
    println!("{} files, {frames} frames, {layers} layers decoded", paths.len());
    for (reason, items) in &failures {
        let sample: Vec<_> = items.iter().take(4).map(String::as_str).collect();
        println!("  {:>6}  {reason}  e.g. {}", items.len(), sample.join(", "));
    }
    Ok(())
}

/// Fits the blend tables the original uses for sprite layers to alpha values.
fn grd_fit(dir: &Path) -> Result<()> {
    use acr_formats::grd::BlendTable;

    let mut vfs = Vfs::open_game_dir(dir)?;
    for set in 0..3 {
        let palette = Palette::parse(&vfs.read(&format!("{set}\\AGEW_1.PAL"))?).ok_or("palette too short")?;
        let mut table = |name: &str| -> Result<BlendTable> {
            Ok(BlendTable::parse(&vfs.read(&format!("{set}\\{name}"))?).ok_or("not a GRD table")?)
        };
        for name in ["AGEW_TR4.GRD", "AGEW_TR.GRD"] {
            let w = table(name)?.mix_weight(&palette).ok_or("not a 256-row table")?;
            println!("{set}\\{name}: row colour weight {w:.3}");
        }
        let alphas: Vec<String> =
            table("AGEW_1D.GRD")?.darkness(&palette).iter().map(|a| format!("{}", (a * 255.0).round())).collect();
        println!("{set}\\AGEW_1D.GRD: black alpha per row [{}]", alphas.join(", "));
    }
    Ok(())
}

/// Ranks the sprite palette sets per sprite pack. Neighbouring pixels of artwork are similar
/// in the palette it was drawn for: the score is the mean squared RGB distance of horizontally
/// adjacent opaque pixels relative to that of random pixel pairs (lower is better).
fn gp_palette(dir: &Path, filter: Option<&str>) -> Result<()> {
    use acr_formats::Encoding;
    use acr_formats::pal::SPRITE_PALETTES;

    let mut vfs = Vfs::open_game_dir(dir)?;
    let palettes = SPRITE_PALETTES
        .iter()
        .map(|&name| Ok((name, Palette::parse(&vfs.read(name)?).ok_or("palette too short")?)))
        .collect::<Result<Vec<_>>>()?;
    let paths: Vec<String> = vfs
        .list()
        .filter(|(p, _)| p.to_ascii_uppercase().ends_with(".GP") && crate::matches(p, filter))
        .map(|(p, _)| p.to_owned())
        .collect();
    println!("sprite\tmapped\tscores");
    for path in &paths {
        let Ok(file) = GpFile::parse(vfs.read(path)?) else { continue };
        let mut pairs: BTreeMap<(u8, u8), u64> = BTreeMap::new();
        for frame in (0..file.frame_count()).filter_map(|i| file.decode_frame(i).ok()) {
            for layer in frame.layers.iter().filter(|l| matches!(l.encoding, Encoding::DictLz | Encoding::SelfLz(_))) {
                let mut pixels = layer.pixels.iter().copied();
                for run in &layer.runs {
                    let row: Vec<u8> = pixels.by_ref().take(usize::from(run.len)).collect();
                    for w in row.windows(2) {
                        *pairs.entry((w[0], w[1])).or_default() += 1;
                    }
                }
            }
        }
        let total: u64 = pairs.values().sum();
        if total == 0 {
            continue;
        }
        let mut hist = [0u64; 256];
        for (&(a, _), &n) in &pairs {
            hist[usize::from(a)] += n;
        }
        let scores: Vec<String> = palettes
            .iter()
            .map(|(name, pal)| {
                let dist = |a: u8, b: u8| -> f64 {
                    let (ca, cb) = (pal.rgb(a), pal.rgb(b));
                    (0..3).map(|k| (f64::from(ca[k]) - f64::from(cb[k])).powi(2)).sum()
                };
                let near: f64 = pairs.iter().map(|(&(a, b), &n)| dist(a, b) * n as f64).sum::<f64>() / total as f64;
                let mut far = 0.0;
                for a in 0..=255u8 {
                    for b in 0..=255u8 {
                        far += dist(a, b) * (hist[usize::from(a)] * hist[usize::from(b)]) as f64;
                    }
                }
                let far = far / (total * total) as f64;
                format!("{name} {:.3}", if far > 0.0 { near / far } else { 1.0 })
            })
            .collect();
        println!("{path}\t{}\t{}", acr_formats::sprite_palette(path), scores.join("\t"));
    }
    Ok(())
}
