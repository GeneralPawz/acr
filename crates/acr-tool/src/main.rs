//! `acr` - inspect and extract original American Conquest data.

use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use acr_formats::raster::{Image, RasterOptions, draw_frame};
use acr_formats::{GpFile, Palette, Vfs, normalize_path};

const USAGE: &str = "\
usage:
  acr list     <game-dir> [filter]
  acr cat      <game-dir> <path>
  acr extract  <game-dir> <out-dir> [filter]
  acr gp-sheet <game-dir> <gp-path> <out.png> [--pal PATH] [--player N]
                [--only ENC,ENC..] [--frames FROM..TO]
  acr gp-check <game-dir> [filter]";

/// Palette used for unit and building sprites unless overridden.
const DEFAULT_PALETTE: &str = "0\\AGEW_1.PAL";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = match args.as_slice() {
        ["list", dir, rest @ ..] => list(Path::new(dir), rest.first().copied()),
        ["cat", dir, path] => cat(Path::new(dir), path),
        ["extract", dir, out, rest @ ..] => {
            extract(Path::new(dir), Path::new(out), rest.first().copied())
        }
        ["gp-sheet", dir, gp, out, rest @ ..] => {
            SheetOptions::parse(rest).and_then(|o| gp_sheet(Path::new(dir), gp, Path::new(out), &o))
        }
        ["gp-check", dir, rest @ ..] => gp_check(Path::new(dir), rest.first().copied()),
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Case-insensitive substring match on normalized paths.
fn matches(path: &str, filter: Option<&str>) -> bool {
    filter.is_none_or(|f| normalize_path(path).contains(&normalize_path(f)))
}

fn list(dir: &Path, filter: Option<&str>) -> Result<()> {
    let vfs = Vfs::open_game_dir(dir)?;
    let mut out = io::stdout().lock();
    for (path, source) in vfs.list().filter(|(p, _)| matches(p, filter)) {
        writeln!(out, "{path}\t{source}")?;
    }
    Ok(())
}

fn cat(dir: &Path, path: &str) -> Result<()> {
    let mut vfs = Vfs::open_game_dir(dir)?;
    io::stdout().lock().write_all(&vfs.read(path)?)?;
    Ok(())
}

fn extract(dir: &Path, out: &Path, filter: Option<&str>) -> Result<()> {
    let mut vfs = Vfs::open_game_dir(dir)?;
    let paths = archived_paths(&vfs, filter);
    for path in &paths {
        let target: PathBuf = out.join(path.replace('\\', std::path::MAIN_SEPARATOR_STR));
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&target, vfs.read(path)?)?;
    }
    eprintln!("extracted {} files to {}", paths.len(), out.display());
    Ok(())
}

fn archived_paths(vfs: &Vfs, filter: Option<&str>) -> Vec<String> {
    vfs.list()
        .filter(|(p, source)| *source != "<loose>" && matches(p, filter))
        .map(|(p, _)| p.to_owned())
        .collect()
}

struct SheetOptions {
    palette: String,
    player: u8,
    only: Option<Vec<u8>>,
    frames: Option<std::ops::Range<usize>>,
}

impl SheetOptions {
    fn parse(args: &[&str]) -> Result<Self> {
        let mut o = Self {
            palette: DEFAULT_PALETTE.to_owned(),
            player: 0,
            only: None,
            frames: None,
        };
        let mut it = args.iter();
        while let Some(&flag) = it.next() {
            let value = *it.next().ok_or_else(|| format!("{flag} needs a value"))?;
            match flag {
                "--pal" => o.palette = value.to_owned(),
                "--player" => o.player = value.parse()?,
                "--only" => {
                    o.only = Some(
                        value
                            .split(',')
                            .map(str::parse)
                            .collect::<std::result::Result<_, _>>()?,
                    )
                }
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
    let palette = Palette::parse(&vfs.read(&o.palette)?).ok_or("palette too short")?;
    let file = GpFile::parse(vfs.read(gp)?)?;

    let range = o.frames.clone().unwrap_or(0..file.frame_count());
    let n = file.frame_count();
    let frames: Vec<_> = (range.start.min(n)..range.end.min(n))
        .map(|i| file.decode_frame(i))
        .collect();
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
    let opt = RasterOptions {
        palette: &palette,
        player: o.player,
        only: o.only.as_deref(),
    };
    for (i, frame) in ok.iter().enumerate() {
        let (cx, cy) = ((i as u32 % cols) * cell_w, (i as u32 / cols) * cell_h);
        draw_frame(
            &mut image,
            frame,
            cx as i32 - x0 + 1,
            cy as i32 - y0 + 1,
            &opt,
        );
    }
    write_png(out, &image)?;
    eprintln!(
        "{} of {} frames, cell {cell_w}x{cell_h} -> {}",
        ok.len(),
        frames.len(),
        out.display()
    );
    Ok(())
}

/// Decodes every sprite pack and reports failures grouped by reason.
fn gp_check(dir: &Path, filter: Option<&str>) -> Result<()> {
    let mut vfs = Vfs::open_game_dir(dir)?;
    let paths: Vec<String> = vfs
        .list()
        .filter(|(p, _)| p.to_ascii_uppercase().ends_with(".GP") && matches(p, filter))
        .map(|(p, _)| p.to_owned())
        .collect();
    let (mut frames, mut layers) = (0usize, 0usize);
    let mut failures: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for path in &paths {
        let file = match GpFile::parse(vfs.read(path)?) {
            Ok(f) => f,
            Err(e) => {
                failures
                    .entry(e.reason.to_owned())
                    .or_default()
                    .push(path.clone());
                continue;
            }
        };
        for i in 0..file.frame_count() {
            frames += 1;
            match file.decode_frame(i) {
                Ok(f) => layers += f.layers.len(),
                Err(e) => failures
                    .entry(e.reason.to_owned())
                    .or_default()
                    .push(format!("{path}#{i}")),
            }
        }
    }
    println!(
        "{} files, {frames} frames, {layers} layers decoded",
        paths.len()
    );
    for (reason, items) in &failures {
        let sample: Vec<_> = items.iter().take(4).map(String::as_str).collect();
        println!("  {:>6}  {reason}  e.g. {}", items.len(), sample.join(", "));
    }
    Ok(())
}

fn write_png(path: &Path, image: &Image) -> Result<()> {
    let file = io::BufWriter::new(std::fs::File::create(path)?);
    let mut encoder = png::Encoder::new(file, image.width, image.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(&image.rgba)?;
    Ok(())
}
