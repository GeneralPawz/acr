//! The commands that talk to the game's driver: `e2e`, `drive` and `jev`.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::e2e::{self, Transport};
use crate::{Result, jev, target_dir, workspace_root};

/// Flags that take a value, to tell them from file names.
const VALUE_FLAGS: [&str; 7] = ["--filter", "--game-dir", "--exe", "--goal", "--steps", "--png", "--expect"];

fn flag_value(args: &[String], name: &str) -> Option<String> {
    args.windows(2).find(|w| w[0] == name).map(|w| w[1].clone())
}

/// The game executable for the driver: `--exe`, else a fresh build (unless `--no-build`).
fn driver_exe(args: &[String]) -> Result<PathBuf> {
    if let Some(exe) = flag_value(args, "--exe") {
        return Ok(PathBuf::from(exe));
    }
    let root = workspace_root();
    let target = target_dir(&root);
    if args.iter().any(|a| a == "--no-build") {
        let exe = target.join("e2e").join(format!("AC-e2e{}", std::env::consts::EXE_SUFFIX));
        return if exe.is_file() { Ok(exe) } else { Err("no built AC-e2e executable; drop --no-build".into()) };
    }
    Ok(e2e::build_exe(&root, &target)?)
}

fn game_dir_or_env(game_dir: Option<PathBuf>) -> Result<PathBuf> {
    game_dir
        .or_else(|| std::env::var_os("ACR_GAME_DIR").map(PathBuf::from))
        .ok_or_else(|| "needs --game-dir or ACR_GAME_DIR (the original game data)".into())
}

/// Lets scenarios use `jev_check` steps; without a key they are skipped.
struct JevHooks {
    jev: Option<std::result::Result<jev::Jev, String>>,
    scratch: PathBuf,
}

impl e2e::Hooks for JevHooks {
    fn step(&mut self, transport: &mut dyn Transport, step: &Value) -> Option<Value> {
        if step["cmd"] != "jev_check" {
            return None;
        }
        let jev = self.jev.get_or_insert_with(|| jev::Jev::from_env(&self.scratch));
        Some(match jev {
            Ok(jev) => {
                let expectation = step["expectation"].as_str().unwrap_or_default();
                let min = step["min"].as_f64().unwrap_or(0.7);
                jev::check_expectation(jev, transport, expectation, min)
            }
            Err(why) => json!({"ok": true, "skipped": why}),
        })
    }
}

/// `cargo xtask e2e`.
pub fn run_e2e(args: &[String], game_dir: Option<PathBuf>) -> Result<()> {
    let root = workspace_root();
    let target = target_dir(&root);
    let mut files = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if VALUE_FLAGS.contains(&a.as_str()) {
            it.next();
        } else if !a.starts_with('-') {
            files.push(PathBuf::from(a));
        }
    }
    let options = e2e::Options {
        filter: flag_value(args, "--filter"),
        game_dir,
        exe: flag_value(args, "--exe").map(PathBuf::from),
        build: !args.iter().any(|a| a == "--no-build"),
        gpu: args.iter().any(|a| a == "--gpu"),
        list: args.iter().any(|a| a == "--list"),
        files,
        verbose: args.iter().any(|a| a == "-v" || a == "--verbose"),
    };
    let mut hooks = JevHooks { jev: None, scratch: target.join("e2e") };
    let passed = e2e::e2e(&root, &target, &options, &mut hooks)?;
    if let Some(Ok(j)) = &hooks.jev {
        println!("jev: {} calls, {} input + {} output tokens", j.usage.calls, j.usage.input, j.usage.output);
    }
    if passed { Ok(()) } else { Err("scenarios failed".into()) }
}

/// Starts the game under the driver for the commands that talk to it directly.
fn spawn_driver(args: &[String], game_dir: Option<PathBuf>, name: &str) -> Result<e2e::Process> {
    let exe = driver_exe(args)?;
    let dir = game_dir_or_env(game_dir)?;
    let mut extra = Vec::new();
    if args.iter().any(|a| a == "--gpu") {
        extra.push("--drive-gpu".to_owned());
    }
    let work = target_dir(&workspace_root()).join("e2e").join("runs").join(name);
    let (p, ready) = e2e::Process::spawn(&exe, &dir, &extra, &work)?;
    eprintln!("{ready}");
    Ok(p)
}

/// `cargo xtask drive`: requests from stdin, answers on stdout.
pub fn drive(args: &[String], game_dir: Option<PathBuf>) -> Result<()> {
    use std::io::BufRead;
    let mut game = spawn_driver(args, game_dir, "drive")?;
    for line in std::io::stdin().lock().lines() {
        let line = line?;
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let request: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                println!("{}", json!({"ok": false, "error": format!("bad JSON: {e}")}));
                continue;
            }
        };
        let quit = request["cmd"] == "quit";
        let answer = game.request(&request, Duration::from_secs(600))?;
        println!("{answer}");
        if quit {
            return Ok(());
        }
    }
    game.quit();
    Ok(())
}

/// `cargo xtask jev`.
pub fn run_jev(args: &[String], game_dir: Option<PathBuf>) -> Result<()> {
    let scratch = target_dir(&workspace_root()).join("e2e");
    match args.first().map(String::as_str) {
        Some("status") | None => {
            match jev::Jev::from_env(&scratch) {
                Ok(_) => println!("TypeSafe key found in {}", jev::key_path().display()),
                Err(e) => println!("{e}"),
            }
            Ok(())
        }
        Some("explore") => {
            let goal = flag_value(args, "--goal").ok_or("explore needs --goal TEXT")?;
            let steps = flag_value(args, "--steps").and_then(|s| s.parse().ok()).unwrap_or(12);
            let mut jev = jev::Jev::from_env(&scratch)?;
            let mut game = spawn_driver(args, game_dir, "explore")?;
            let t = Instant::now();
            let result = jev::explore(&mut jev, &mut game, &goal, steps);
            game.quit();
            let result = result?;
            println!("goal: {goal}");
            for (i, s) in result.trace.iter().enumerate() {
                let what = match &s.decision {
                    jev::Decision::Click { label, choice, .. } => {
                        format!("click {label:?} (p {:.2}, confidence {:.2})", choice.probability, choice.confidence)
                    }
                    jev::Decision::Done(p) => format!("goal reached (p {p:.2})"),
                    jev::Decision::NeedsReasoning(why) => format!("needs reasoning: {why}"),
                };
                println!("{:>2}. {:<14} {what}  [{} ms]", i + 1, s.screen, s.millis);
            }
            let u = jev.usage;
            println!(
                "{}: {} Jev calls, {} input + {} output tokens, {:.1} s in Jev, {:.1} s total",
                if result.reached { "reached" } else { "stopped" },
                u.calls,
                u.input,
                u.output,
                u.millis as f64 / 1000.0,
                t.elapsed().as_secs_f64()
            );
            if !result.reached {
                println!("stopped: {}", result.stopped);
            }
            Ok(())
        }
        Some("shot") => {
            let png = std::fs::read(flag_value(args, "--png").ok_or("shot needs --png FILE")?)?;
            let what = flag_value(args, "--expect").unwrap_or_else(|| "the game's interface".into());
            let mut jev = jev::Jev::from_env(&scratch)?;
            let p = jev::looks_broken(&mut jev, png, &what)?;
            let verdict = match p {
                p if p >= 0.7 => "looks broken: have a reasoning model look",
                p if p <= 0.3 => "looks fine",
                _ => "unsure: needs reasoning",
            };
            println!("broken p {p:.2}: {verdict} ({} input + {} output tokens)", jev.usage.input, jev.usage.output);
            Ok(())
        }
        Some(other) => Err(format!("unknown jev command {other}").into()),
    }
}
