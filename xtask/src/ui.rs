//! The Svelte UI's tooling (docs/webui.md): `cargo xtask ui` builds `ui/` with Node, and
//! `cargo xtask ui-diff` compares two screenshots.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::Result;

/// Pixels within this distance per channel count as equal (docs/webui.md "Parity").
const TOLERANCE: u8 = 8;
/// Share of equal pixels `ui-diff` demands unless `--min` says otherwise.
const MIN_SHARE: f64 = 99.5;

fn npm() -> Command {
    // `npm` is a script on Windows.
    Command::new(if cfg!(windows) { "npm.cmd" } else { "npm" })
}

fn run(root: &Path, args: &[&str]) -> Result<()> {
    let status = npm().current_dir(root.join("ui")).args(args).status()?;
    if status.success() { Ok(()) } else { Err(format!("npm {}: {status}", args.join(" ")).into()) }
}

/// Installs the packages when the lock file is newer than the installed set.
fn install(root: &Path) -> Result<()> {
    let ui = root.join("ui");
    let stamp = ui.join("node_modules").join(".package-lock.json");
    let newer =
        |a: &Path, b: &Path| match (a.metadata().and_then(|m| m.modified()), b.metadata().and_then(|m| m.modified())) {
            (Ok(a), Ok(b)) => a > b,
            _ => true,
        };
    if newer(&ui.join("package-lock.json"), &stamp) { run(root, &["ci"]) } else { Ok(()) }
}

/// `cargo xtask ui [--watch]`: installs and builds the bundle into `ui/dist`.
pub fn ui(root: &Path, args: &[String]) -> Result<()> {
    install(root)?;
    if args.iter().any(|a| a == "--watch") {
        run(root, &["run", "build", "--", "--watch"])
    } else {
        run(root, &["run", "build"])
    }
}

/// The folder of the built bundle for `xtask web`, or `None` to leave the UI out (the game
/// then uses the engine's UI). `ACR_UI_DIST` names a bundle built elsewhere (CI); without Node
/// the UI is skipped with a warning, never an error.
pub fn bundle(root: &Path, no_ui: bool) -> Result<Option<PathBuf>> {
    if no_ui {
        return Ok(None);
    }
    if let Some(dist) = std::env::var_os("ACR_UI_DIST") {
        let dist = PathBuf::from(dist);
        return if dist.join("acr-ui.js").is_file() {
            Ok(Some(dist))
        } else {
            Err(format!("ACR_UI_DIST {} has no acr-ui.js", dist.display()).into())
        };
    }
    if npm().arg("--version").output().is_err() {
        eprintln!("xtask: warning: npm not found, the web build has no Svelte UI (engine UI only)");
        return Ok(None);
    }
    ui(root, &[])?;
    Ok(Some(root.join("ui").join("dist")))
}

struct Picture {
    width: usize,
    height: usize,
    rgba: Vec<u8>,
}

fn read_png(path: &str) -> Result<Picture> {
    let mut reader = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path)?)).read_info()?;
    let mut buf = vec![0; reader.output_buffer_size().ok_or("png too large")?];
    let info = reader.next_frame(&mut buf)?;
    if info.bit_depth != png::BitDepth::Eight {
        return Err(format!("{path}: only 8-bit PNGs").into());
    }
    let data = &buf[..info.buffer_size()];
    let rgba = match info.color_type {
        png::ColorType::Rgba => data.to_vec(),
        png::ColorType::Rgb => data.as_chunks::<3>().0.iter().flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        other => return Err(format!("{path}: unsupported PNG colour type {other:?}").into()),
    };
    Ok(Picture { width: info.width as usize, height: info.height as usize, rgba })
}

/// Share (percent) of pixels of `a` and `b` whose channels all differ by at most
/// [`TOLERANCE`], and an image of the differences: equal pixels dim, others red.
fn compare(a: &Picture, b: &Picture) -> Result<(f64, Picture)> {
    if (a.width, a.height) != (b.width, b.height) {
        return Err(format!("sizes differ: {}x{} and {}x{}", a.width, a.height, b.width, b.height).into());
    }
    let mut diff = Vec::with_capacity(a.rgba.len());
    let mut same = 0usize;
    for (p, q) in a.rgba.as_chunks::<4>().0.iter().zip(b.rgba.as_chunks::<4>().0.iter()) {
        let worst = p.iter().zip(q).take(3).map(|(x, y)| x.abs_diff(*y)).max().unwrap_or(0);
        if worst <= TOLERANCE {
            same += 1;
            diff.extend([p[0] / 4, p[1] / 4, p[2] / 4, 255]);
        } else {
            diff.extend([255, worst.saturating_mul(2).min(120), 0, 255]);
        }
    }
    let share = 100.0 * same as f64 / (a.width * a.height).max(1) as f64;
    Ok((share, Picture { width: a.width, height: a.height, rgba: diff }))
}

fn write_png(path: &str, picture: &Picture) -> Result<()> {
    let mut encoder = png::Encoder::new(
        std::io::BufWriter::new(std::fs::File::create(path)?),
        picture.width as u32,
        picture.height as u32,
    );
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(&picture.rgba)?;
    Ok(())
}

/// `cargo xtask ui-diff A.png B.png [--out D.png] [--min PERCENT]`.
pub fn ui_diff(args: &[String]) -> Result<()> {
    let option = |name: &str| args.windows(2).find(|w| w[0] == name).map(|w| w[1].clone());
    let files: Vec<&String> = args
        .iter()
        .enumerate()
        .filter(|(i, a)| !a.starts_with("--") && (*i == 0 || !args[i - 1].starts_with("--")))
        .map(|(_, a)| a)
        .collect();
    let [a, b] = files.as_slice() else {
        return Err("usage: cargo xtask ui-diff A.png B.png [--out D.png] [--min PERCENT]".into());
    };
    let min = option("--min").map(|m| m.parse::<f64>()).transpose()?.unwrap_or(MIN_SHARE);
    let (share, diff) = compare(&read_png(a)?, &read_png(b)?)?;
    if let Some(out) = option("--out") {
        write_png(&out, &diff)?;
    }
    println!("{share:.3}% of the pixels within +-{TOLERANCE} (need {min}%)");
    if share < min { Err(format!("only {share:.3}% of the pixels match").into()) } else { Ok(()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(rgba: [u8; 4], n: usize) -> Picture {
        Picture { width: n, height: 1, rgba: rgba.repeat(n) }
    }

    #[test]
    fn counts_pixels_within_the_tolerance() {
        let a = flat([100, 100, 100, 255], 4);
        let mut b = flat([108, 92, 100, 255], 4);
        b.rgba[0] = 109;
        let (share, diff) = compare(&a, &b).unwrap();
        assert_eq!(share, 75.0);
        assert_eq!(&diff.rgba[..2], &[255, 18]);
        assert!(compare(&a, &flat([0; 4], 3)).is_err());
    }
}
