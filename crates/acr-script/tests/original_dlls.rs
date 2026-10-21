//! Runs every original mission/AI DLL of an installed game. Skipped unless `ACR_GAME_DIR`
//! points to the American Conquest directory.

use std::path::{Path, PathBuf};

use acr_script::analysis::reachable_code;
use acr_script::dmcr::RecordingHost;
use acr_script::runtime::Machine;

fn script_dlls(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack: Vec<PathBuf> =
        ["AI", "Missions", "History_battl", "UserMissions"].iter().map(|d| root.join(d)).collect();
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("dll")) {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

#[test]
fn all_original_dlls_decode_and_initialise() {
    let Some(root) = std::env::var_os("ACR_GAME_DIR").map(PathBuf::from) else {
        eprintln!("ACR_GAME_DIR not set, skipping");
        return;
    };
    let dlls = script_dlls(&root);
    assert!(!dlls.is_empty(), "no DLLs under {}", root.display());
    let mut failures = Vec::new();
    for path in &dlls {
        let rel = path.strip_prefix(&root).unwrap().to_string_lossy().replace('/', "\\");
        let mut m = Machine::new(&std::fs::read(path).unwrap(), &rel).unwrap();

        let cov = reachable_code(&m.mem, &m.image);
        if let Some(e) = cov.unsupported.first() {
            failures.push(format!("{rel}: {} unsupported, e.g. {e}", cov.unsupported.len()));
        }

        let mut host = RecordingHost::default();
        match m.attach(&mut host) {
            Ok(true) => {}
            other => {
                failures.push(format!("{rel}: DllMain returned {other:?}"));
                continue;
            }
        }
        let init = if m.image.export("OnInit").is_some() { "OnInit" } else { "InitAI" };
        if let Err(e) = m.call_export(&mut host, init, &[]) {
            failures.push(format!("{rel}: {init}: {e}"));
        }
    }
    assert!(failures.is_empty(), "{} of {} DLLs failed:\n{}", failures.len(), dlls.len(), failures.join("\n"));
}
