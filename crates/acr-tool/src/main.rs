//! `acr` - inspect and extract original American Conquest data.
//!
//! Usage:
//!   acr list    <game-dir> [filter]
//!   acr cat     <game-dir> <path>
//!   acr extract <game-dir> <out-dir> [filter]

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use acr_formats::{Vfs, normalize_path};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["list", dir, rest @ ..] => list(Path::new(dir), rest.first().copied()),
        ["cat", dir, path] => cat(Path::new(dir), path),
        ["extract", dir, out, rest @ ..] => {
            extract(Path::new(dir), Path::new(out), rest.first().copied())
        }
        _ => {
            eprintln!(
                "usage:\n  acr list <game-dir> [filter]\n  acr cat <game-dir> <path>\n  acr extract <game-dir> <out-dir> [filter]"
            );
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

/// Case-insensitive substring match on normalized paths.
fn matches(path: &str, filter: Option<&str>) -> bool {
    filter.is_none_or(|f| normalize_path(path).contains(&normalize_path(f)))
}

fn list(dir: &Path, filter: Option<&str>) -> io::Result<()> {
    let vfs = Vfs::open_game_dir(dir)?;
    let mut out = io::stdout().lock();
    for (path, source) in vfs.list().filter(|(p, _)| matches(p, filter)) {
        writeln!(out, "{path}\t{source}")?;
    }
    Ok(())
}

fn cat(dir: &Path, path: &str) -> io::Result<()> {
    let mut vfs = Vfs::open_game_dir(dir)?;
    io::stdout().lock().write_all(&vfs.read(path)?)
}

fn extract(dir: &Path, out: &Path, filter: Option<&str>) -> io::Result<()> {
    let mut vfs = Vfs::open_game_dir(dir)?;
    let paths: Vec<String> = vfs
        .list()
        .filter(|(p, source)| *source != "<loose>" && matches(p, filter))
        .map(|(p, _)| p.to_owned())
        .collect();
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
