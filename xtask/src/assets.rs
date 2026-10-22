//! `cargo xtask assets`: rebuilds the 3D assets with Blender, headless (see `docs/assets.md`).
//!
//! An asset is a Python script under `assets/` (outside `assets/blender/`, which holds the
//! shared helpers and entry scripts) that defines `def build(ctx)`. Its glTF goes to
//! `target/assets/<folder>/<name>.glb`. A script with `TYPE = "<MD NAME>"` gets the sprite
//! views of that object type (`acr model-ref`) when a game folder is known, and can take its
//! colours from them; `--check` then scores it with `acr model-check`.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::Result;

pub struct Options {
    /// Asset names (file stems, any case); empty = all.
    pub names: Vec<String>,
    pub game_dir: Option<PathBuf>,
    pub compress: String,
    pub check: bool,
    pub blend: bool,
}

/// The Blender executable: `$BLENDER`, the default install path of Blender 5.2 on Windows,
/// or `blender` on the `PATH` (same rule as `acr model-check`).
fn blender() -> PathBuf {
    if let Some(p) = std::env::var_os("BLENDER") {
        return PathBuf::from(p);
    }
    let default = Path::new(r"C:\Program Files\Blender Foundation\Blender 5.2\blender.exe");
    if default.is_file() { default.to_path_buf() } else { PathBuf::from("blender") }
}

/// Asset scripts below `dir`, skipping `assets/blender`.
fn scripts(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let mut entries: Vec<PathBuf> =
        std::fs::read_dir(dir)?.map(|e| e.map(|e| e.path())).collect::<std::io::Result<_>>()?;
    entries.sort();
    for path in entries {
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n != "blender" && n != "__pycache__") {
                scripts(&path, out)?;
            }
        } else if path.extension().is_some_and(|e| e == "py")
            && std::fs::read_to_string(&path)?.lines().any(|l| l.starts_with("def build("))
        {
            out.push(path);
        }
    }
    Ok(())
}

/// The value of a top-level `NAME = "value"` line.
fn constant(source: &str, name: &str) -> Option<String> {
    source.lines().find_map(|l| {
        let rest = l.strip_prefix(name)?.trim_start().strip_prefix('=')?.trim();
        let rest = rest.split('#').next()?.trim();
        Some(rest.trim_matches(|c| c == '"' || c == '\'').to_owned()).filter(|s| !s.is_empty())
    })
}

/// Runs the `acr` tool (built in release mode) with `args`.
fn acr(root: &Path, args: &[&std::ffi::OsStr]) -> Result<()> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let status = Command::new(cargo)
        .current_dir(root)
        .args(["run", "--quiet", "--release", "--package", "acr-tool", "--bin", "acr", "--"])
        .args(args)
        .status()
        .map_err(|e| format!("cannot run cargo for the acr tool: {e}"))?;
    if status.success() { Ok(()) } else { Err(format!("acr {:?} failed", args.first()).into()) }
}

pub fn assets(root: &Path, o: &Options) -> Result<()> {
    let assets = root.join("assets");
    let mut all = Vec::new();
    scripts(&assets, &mut all)?;
    let wanted: Vec<&PathBuf> = all
        .iter()
        .filter(|p| {
            o.names.is_empty()
                || p.file_stem().is_some_and(|s| o.names.iter().any(|n| s.to_string_lossy().eq_ignore_ascii_case(n)))
        })
        .collect();
    if wanted.is_empty() {
        return Err(format!("no asset script matches {:?} in {}", o.names, assets.display()).into());
    }
    let game_dir = o.game_dir.clone().or_else(|| std::env::var_os("ACR_GAME_DIR").map(PathBuf::from));
    if o.check && game_dir.is_none() {
        return Err("--check needs --game-dir or ACR_GAME_DIR".into());
    }
    let blender = blender();
    let out_root = root.join("target").join("assets");
    let build_py = assets.join("blender").join("build.py");
    for script in wanted {
        let source = std::fs::read_to_string(script)?;
        let name = script.file_stem().ok_or("file name")?.to_string_lossy().into_owned();
        let folder = script.parent().and_then(|p| p.strip_prefix(&assets).ok()).unwrap_or(Path::new(""));
        let glb = out_root.join(folder).join(format!("{name}.glb"));
        let ty = constant(&source, "TYPE");
        let reference = match (&ty, &game_dir) {
            (Some(ty), Some(game)) => {
                let dir = out_root.join("ref").join(ty);
                acr(root, &["model-ref".as_ref(), game.as_os_str(), ty.as_ref(), dir.as_os_str()])?;
                Some(dir.join("ref.json"))
            }
            _ => None,
        };
        let mut cmd = Command::new(&blender);
        cmd.args(["-b", "--factory-startup", "--python"]).arg(&build_py).arg("--").arg(script);
        cmd.arg("--out").arg(&glb).args(["--compress", &o.compress]);
        if let Some(r) = &reference {
            cmd.arg("--ref").arg(r);
        }
        if o.blend {
            cmd.arg("--blend").arg(glb.with_extension("blend"));
        }
        eprintln!("asset {name}: {}", glb.display());
        let status =
            cmd.status().map_err(|e| format!("cannot run Blender ({}): {e}; set BLENDER", blender.display()))?;
        if !status.success() {
            return Err(format!("Blender failed on {}", script.display()).into());
        }
        if o.check {
            let (Some(ty), Some(game)) = (&ty, &game_dir) else {
                eprintln!("asset {name}: no TYPE, no sprite check");
                continue;
            };
            let dir = out_root.join("check").join(&name);
            acr(root, &["model-check".as_ref(), game.as_os_str(), ty.as_ref(), glb.as_os_str(), dir.as_os_str()])?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_constants() {
        let src = "\"\"\"doc\"\"\"\nTYPE = \"IncDo1\"  # the .MD\nOTHER=1\n";
        assert_eq!(constant(src, "TYPE").as_deref(), Some("IncDo1"));
        assert_eq!(constant(src, "MISSING"), None);
    }
}
