//! `cargo xtask assets`: rebuilds the 3D assets with Blender, headless (see `docs/assets.md`).
//!
//! An asset is a Python script under `assets/` (outside `assets/blender/`, which holds the
//! shared helpers and entry scripts) that defines `def build(ctx)`. Its glTF goes to
//! `target/assets/<folder>/<name>.glb`. A script with `TYPE = "<MD NAME>"` gets the sprite
//! views of that object type (`acr model-ref`) when a game folder is known, and can take its
//! colours from them; `--check` then scores it with `acr model-check`, once per entry of a
//! `CHECKS = ["--anim STAND", "--anim DEATHLIE1"]` line (arguments of `acr model-check`).

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

/// `blender -b --factory-startup ... build.py -- <script>` with a fixed string-hash seed.
/// Blender's Python randomises string hashes per run, so sets and dicts of bone or object
/// names would iterate in a new order every build (docs/assets.md "Repeatable builds").
/// Blender ignores PYTHON* variables without `--python-use-system-env`; PYTHONHOME and
/// PYTHONPATH are dropped so no system Python leaks in.
fn blender_command(blender: &Path, build_py: &Path, script: &Path) -> Command {
    let mut cmd = Command::new(blender);
    cmd.env("PYTHONHASHSEED", "0").env_remove("PYTHONHOME").env_remove("PYTHONPATH");
    cmd.args(["-b", "--factory-startup", "--python-use-system-env", "--python"]);
    cmd.arg(build_py).arg("--").arg(script);
    cmd
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

/// The strings of a top-level list `NAME = ["a", "b"]` (it may span lines).
fn list(source: &str, name: &str) -> Option<Vec<String>> {
    let start = source.lines().position(|l| l.strip_prefix(name).is_some_and(|r| r.trim_start().starts_with('=')))?;
    let text: String = source.lines().skip(start).collect::<Vec<_>>().join(
        "
",
    );
    let inner = text.split_once('[')?.1.split_once(']')?.0;
    Some(
        inner
            .split(',')
            .map(|s| s.trim().trim_matches(|c| c == '"' || c == '\'').to_owned())
            .filter(|s| !s.is_empty())
            .collect(),
    )
}

/// Folder name of a check: its values without the flags and without `--min-iou X`
/// (`--anim STANDLO --sections 2` -> `STANDLO-2`).
fn label(check: &str) -> String {
    ref_args(check).iter().filter(|w| !w.starts_with("--")).copied().collect::<Vec<_>>().join("-")
}

/// The arguments of a check that select the reference (all but `--min-iou X`).
fn ref_args(check: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut it = check.split_whitespace();
    while let Some(w) = it.next() {
        if w == "--min-iou" {
            it.next();
        } else {
            out.push(w);
        }
    }
    out
}

/// Builds the `acr` tool in release mode once and returns the binary, so each `model-ref`
/// and `model-check` is a process start instead of a `cargo run`.
fn acr_binary(root: &Path) -> Result<PathBuf> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let status = Command::new(cargo)
        .current_dir(root)
        .args(["build", "--quiet", "--release", "--package", "acr-tool", "--bin", "acr"])
        .status()
        .map_err(|e| format!("cannot run cargo for the acr tool: {e}"))?;
    if !status.success() {
        return Err("building the acr tool failed".into());
    }
    let target = std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| root.join("target"), PathBuf::from);
    let bin = target.join("release").join(format!("acr{}", std::env::consts::EXE_SUFFIX));
    if bin.is_file() { Ok(bin) } else { Err(format!("{} not found after building", bin.display()).into()) }
}

/// Runs the `acr` binary with `args`.
fn acr(bin: &Path, args: &[&std::ffi::OsStr]) -> Result<()> {
    let status = Command::new(bin).args(args).status().map_err(|e| format!("cannot run {}: {e}", bin.display()))?;
    if status.success() { Ok(()) } else { Err(format!("acr {:?} failed", args.first()).into()) }
}

/// Marker file of a finished `model-ref` output directory.
const REF_STAMP: &str = ".model-ref";

/// Whether `dir` holds the output of `acr model-ref` for `key` (game folder, type and
/// arguments) that is not older than the `acr` binary.
fn ref_is_fresh(dir: &Path, key: &str, bin: &Path) -> bool {
    let stamp = dir.join(REF_STAMP);
    let modified = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    let (Some(made), Some(built)) = (modified(&stamp), modified(bin)) else {
        return false;
    };
    made >= built && dir.join("ref.json").is_file() && std::fs::read_to_string(&stamp).is_ok_and(|k| k == key)
}

/// `acr model-ref <game> <ty> <dir> <args..>`, skipped when `dir` is up to date
/// ([`ref_is_fresh`]); the stamp is written only after the tool succeeded.
fn model_ref(bin: &Path, game: &Path, ty: &str, dir: &Path, args: &[&str]) -> Result<()> {
    let key = format!("{}\n{ty}\n{}\n", game.display(), args.join(" "));
    if ref_is_fresh(dir, &key, bin) {
        return Ok(());
    }
    let _ = std::fs::remove_file(dir.join(REF_STAMP));
    let mut all: Vec<&std::ffi::OsStr> = vec!["model-ref".as_ref(), game.as_os_str(), ty.as_ref(), dir.as_os_str()];
    all.extend(args.iter().map(std::ffi::OsStr::new));
    acr(bin, &all)?;
    std::fs::write(dir.join(REF_STAMP), key)?;
    Ok(())
}

/// One Blender run that renders `glb` for every `(ref.json, out dir)` pair
/// (`render_views.py`: one import, one render per state and direction).
fn render_checks(blender: &Path, render_py: &Path, glb: &Path, jobs: &[(PathBuf, PathBuf)]) -> Result<()> {
    let mut cmd = Command::new(blender);
    cmd.env("PYTHONHASHSEED", "0").env_remove("PYTHONHOME").env_remove("PYTHONPATH");
    cmd.args(["-b", "--factory-startup", "--python-use-system-env", "--python"]);
    cmd.arg(render_py).arg("--").arg("--model").arg(glb);
    for (r, out) in jobs {
        cmd.arg("--ref").arg(r).arg("--out").arg(out);
    }
    let status = cmd.status().map_err(|e| format!("cannot run Blender ({}): {e}; set BLENDER", blender.display()))?;
    if status.success() { Ok(()) } else { Err(format!("Blender failed rendering {}", glb.display()).into()) }
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
    // Only needed with a game folder (sprite references and checks).
    let acr_bin = game_dir.as_ref().map(|_| acr_binary(root)).transpose()?;
    let out_root = root.join("target").join("assets");
    let build_py = assets.join("blender").join("build.py");
    for script in wanted {
        let source = std::fs::read_to_string(script)?;
        let name = script.file_stem().ok_or("file name")?.to_string_lossy().into_owned();
        let folder = script.parent().and_then(|p| p.strip_prefix(&assets).ok()).unwrap_or(Path::new(""));
        let glb = out_root.join(folder).join(format!("{name}.glb"));
        let ty = constant(&source, "TYPE");
        // `CHECKS = ["--anim STAND", "--anim DEATHLIE1"]`: one reference and one check per
        // entry, each in its own folder named after it; without it, the default animation.
        let checks = list(&source, "CHECKS").unwrap_or_else(|| vec![String::new()]);
        let reference = match (&ty, &game_dir) {
            (Some(ty), Some(game)) => {
                let acr_bin = acr_bin.as_deref().ok_or("acr binary")?;
                let dir = out_root.join("ref").join(ty.replace(':', "_"));
                model_ref(acr_bin, game, ty, &dir, &[])?;
                for check in checks.iter().filter(|c| !label(c).is_empty()) {
                    model_ref(acr_bin, game, ty, &dir.join(label(check)), &ref_args(check))?;
                }
                Some(dir.join("ref.json"))
            }
            _ => None,
        };
        let mut cmd = blender_command(&blender, &build_py, script);
        cmd.arg("--out").arg(&glb).args(["--compress", &o.compress]);
        if let Some(r) = &reference {
            cmd.arg("--ref").arg(r);
            cmd.arg("--refs").arg(r.parent().ok_or("ref dir")?);
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
            let acr_bin = acr_bin.as_deref().ok_or("acr binary")?;
            let check_dir = |check: &String| {
                let dir = out_root.join("check").join(&name);
                if label(check).is_empty() { dir } else { dir.join(label(check)) }
            };
            // References first, then one Blender run for all renders, then the scores.
            let mut jobs = Vec::new();
            for check in &checks {
                let dir = check_dir(check);
                model_ref(acr_bin, game, ty, &dir, &ref_args(check))?;
                jobs.push((dir.join("ref.json"), dir));
            }
            render_checks(&blender, &assets.join("blender").join("render_views.py"), &glb, &jobs)?;
            for check in &checks {
                let dir = check_dir(check);
                let mut args: Vec<&std::ffi::OsStr> =
                    vec!["model-check".as_ref(), game.as_os_str(), ty.as_ref(), glb.as_os_str(), dir.as_os_str()];
                args.extend(check.split_whitespace().map(std::ffi::OsStr::new));
                args.push("--rendered".as_ref());
                acr(acr_bin, &args)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blender_hash_seed_fixed() {
        let cmd = blender_command(Path::new("blender"), Path::new("build.py"), Path::new("a.py"));
        let envs: Vec<_> = cmd.get_envs().collect();
        assert!(envs.contains(&("PYTHONHASHSEED".as_ref(), Some("0".as_ref()))));
        assert!(envs.contains(&("PYTHONPATH".as_ref(), None)));
        assert!(cmd.get_args().any(|a| a == "--python-use-system-env"));
    }

    #[test]
    fn reads_constants() {
        let src = "\"\"\"doc\"\"\"\nTYPE = \"IncDo1\"  # the .MD\nOTHER=1\n";
        assert_eq!(constant(src, "TYPE").as_deref(), Some("IncDo1"));
        assert_eq!(constant(src, "MISSING"), None);
        let src = "CHECKS = [\"--anim STAND\", '--anim BUILDLO_0 --sections 2']
";
        assert_eq!(list(src, "CHECKS").unwrap(), ["--anim STAND", "--anim BUILDLO_0 --sections 2"]);
        assert_eq!(list(src, "OTHER"), None);
        let src = "CHECKS = [\"--anim A\",
          \"--anim B\"]
X = 1
";
        assert_eq!(list(src, "CHECKS").unwrap(), ["--anim A", "--anim B"]);
        assert_eq!(label("--anim STANDLO --sections 2 --min-iou 0.7"), "STANDLO-2");
        assert_eq!(ref_args("--anim STAND --min-iou 0.7"), ["--anim", "STAND"]);
    }

    #[test]
    fn model_ref_cache_needs_matching_key_and_newer_stamp() {
        let dir = std::env::temp_dir().join(format!("acr-xtask-ref-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let bin = dir.join("acr");
        std::fs::write(&bin, "").unwrap();
        let out = dir.join("out");
        std::fs::create_dir_all(&out).unwrap();
        assert!(!ref_is_fresh(&out, "k", &bin), "nothing written yet");
        std::fs::write(out.join("ref.json"), "{}").unwrap();
        std::fs::write(out.join(REF_STAMP), "k").unwrap();
        assert!(ref_is_fresh(&out, "k", &bin));
        assert!(!ref_is_fresh(&out, "other", &bin), "other arguments");
        // A binary built after the output invalidates it.
        let later = std::time::SystemTime::now() + std::time::Duration::from_secs(60);
        std::fs::File::options().write(true).open(&bin).unwrap().set_modified(later).unwrap();
        assert!(!ref_is_fresh(&out, "k", &bin), "older than the binary");
        std::fs::remove_file(out.join("ref.json")).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
