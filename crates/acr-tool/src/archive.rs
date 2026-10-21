//! `list`, `cat`, `extract`: browse the files of an installation.

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use acr_formats::Vfs;

use crate::{Command, Result};

pub const COMMANDS: &[Command] = &[
    Command {
        name: "list",
        usage: "<game-dir> [filter]",
        run: |a| match a {
            [dir, rest @ ..] => list(Path::new(dir), rest.first().copied()),
            _ => Err(crate::WRONG_ARGS.into()),
        },
    },
    Command {
        name: "cat",
        usage: "<game-dir> <path>",
        run: |a| match a {
            [dir, path] => cat(Path::new(dir), path),
            _ => Err(crate::WRONG_ARGS.into()),
        },
    },
    Command {
        name: "extract",
        usage: "<game-dir> <out-dir> [filter]",
        run: |a| match a {
            [dir, out, rest @ ..] => extract(Path::new(dir), Path::new(out), rest.first().copied()),
            _ => Err(crate::WRONG_ARGS.into()),
        },
    },
];

fn list(dir: &Path, filter: Option<&str>) -> Result<()> {
    let vfs = Vfs::open_game_dir(dir)?;
    let mut out = io::stdout().lock();
    for (path, source) in vfs.list().filter(|(p, _)| crate::matches(p, filter)) {
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
        .filter(|(p, source)| *source != "<loose>" && crate::matches(p, filter))
        .map(|(p, _)| p.to_owned())
        .collect()
}
