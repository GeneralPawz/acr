//! `cargo xtask e2e`: scenario tests through the game's driver mode (`AmericanConquest
//! --drive`, see `docs/testing.md`).
//!
//! A scenario is a JSON file under `tests/e2e/`: driver commands as steps, each with
//! optional expectations on its answer. The runner starts one game process per scenario,
//! sends the steps one by one, checks the answers, writes a transcript and prints a
//! summary with timings. Scenarios that need the original game data are skipped without
//! `--game-dir` (or `ACR_GAME_DIR`), so CI only checks that every file parses.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};

/// Keys of a step that belong to the runner, not to the driver.
const META: [&str; 5] = ["expect", "capture", "note", "optional", "timeout_s"];

/// One game process (or a stand-in in tests) that answers driver requests.
pub trait Transport {
    /// Sends one request and returns its answer.
    fn request(&mut self, request: &Value, timeout: Duration) -> Result<Value, String>;
}

/// The game under the driver, as a child process.
pub struct Process {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<String>,
}

impl Process {
    /// Starts `exe --drive <args> <game_dir>` and waits for its `ready` line.
    pub fn spawn(exe: &Path, game_dir: &Path, args: &[String], work: &Path) -> Result<(Self, Value), String> {
        std::fs::create_dir_all(work).map_err(|e| format!("{}: {e}", work.display()))?;
        let mut cmd = Command::new(exe);
        if !args.iter().any(|a| a == "--drive" || a == "--drive-gpu") {
            cmd.arg("--drive");
        }
        cmd.args(args)
            .arg(game_dir)
            .env("ACR_VOLUME", "0")
            .env("ACR_SAVE_DIR", work.join("saves"))
            .env("ACR_LOG", work.join("game.log"))
            .env_remove("ACR_SETTINGS_DIR")
            .env_remove("ACR_START_MAP")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = cmd.spawn().map_err(|e| format!("{}: {e}", exe.display()))?;
        let stdout = child.stdout.take().ok_or("no stdout")?;
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let stdin = child.stdin.take();
        let mut p = Self { child, stdin, lines: rx };
        let ready = p.read(Duration::from_secs(60))?;
        if ready.get("event").and_then(Value::as_str) != Some("ready") {
            return Err(format!("the game did not start: {ready}"));
        }
        Ok((p, ready))
    }

    fn read(&mut self, timeout: Duration) -> Result<Value, String> {
        match self.lines.recv_timeout(timeout) {
            Ok(line) => serde_json::from_str(&line).map_err(|e| format!("bad answer {line:?}: {e}")),
            Err(RecvTimeoutError::Timeout) => Err(format!("no answer within {}s", timeout.as_secs())),
            Err(RecvTimeoutError::Disconnected) => {
                let status = self.child.wait().map(|s| s.to_string()).unwrap_or_default();
                Err(format!("the game exited ({status})"))
            }
        }
    }

    /// Asks the game to quit and waits a little; kills it otherwise.
    pub fn quit(mut self) {
        let _ = self.request(&json!({"cmd": "quit"}), Duration::from_secs(5));
        drop(self.stdin.take());
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if let Ok(Some(_)) = self.child.try_wait() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        if let Ok(None) = self.child.try_wait() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

impl Transport for Process {
    fn request(&mut self, request: &Value, timeout: Duration) -> Result<Value, String> {
        let stdin = self.stdin.as_mut().ok_or("stdin closed")?;
        writeln!(stdin, "{request}").and_then(|_| stdin.flush()).map_err(|e| format!("cannot send: {e}"))?;
        self.read(timeout)
    }
}

/// A scenario file.
#[derive(Debug, Clone)]
pub struct Scenario {
    pub name: String,
    pub description: String,
    /// Needs the original game data (all real scenarios do).
    pub needs_data: bool,
    /// Extra driver flags, e.g. `["--dialogs", "report"]`.
    pub args: Vec<String>,
    /// Seconds a step may take by default.
    pub step_timeout: u64,
    /// Error lines in the game's log fail the scenario.
    pub strict_log: bool,
    pub steps: Vec<Value>,
}

impl Scenario {
    pub fn parse(text: &str, fallback_name: &str) -> Result<Self, String> {
        let v: Value = serde_json::from_str(text).map_err(|e| format!("bad JSON: {e}"))?;
        let steps = v.get("steps").and_then(Value::as_array).ok_or("missing \"steps\"")?.clone();
        for (i, s) in steps.iter().enumerate() {
            let cmd = s.get("cmd").and_then(Value::as_str);
            if cmd.is_none() {
                return Err(format!("step {}: missing \"cmd\"", i + 1));
            }
            if let Some(e) = s.get("expect")
                && !(e.is_object() || e.is_array())
            {
                return Err(format!("step {}: \"expect\" must be an object or a list", i + 1));
            }
        }
        Ok(Self {
            name: v.get("name").and_then(Value::as_str).unwrap_or(fallback_name).to_owned(),
            description: v.get("description").and_then(Value::as_str).unwrap_or_default().to_owned(),
            needs_data: v.get("needs_data").and_then(Value::as_bool).unwrap_or(true),
            args: v
                .get("args")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_str).map(str::to_owned).collect())
                .unwrap_or_default(),
            step_timeout: v.get("step_timeout_s").and_then(Value::as_u64).unwrap_or(120),
            strict_log: v.get("strict_log").and_then(Value::as_bool).unwrap_or(false),
            steps,
        })
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("scenario");
        Self::parse(&text, stem).map_err(|e| format!("{}: {e}", path.display()))
    }
}

/// The value at a dotted path (`state.counts.0.sites`; numbers index lists).
pub fn lookup<'a>(v: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.').filter(|p| !p.is_empty()).try_fold(v, |v, key| match v {
        Value::Object(m) => m.get(key),
        Value::Array(a) => key.parse::<usize>().ok().and_then(|i| a.get(i)),
        _ => None,
    })
}

fn number(v: &Value) -> Option<f64> {
    v.as_f64().or_else(|| v.as_bool().map(|b| f64::from(u8::from(b))))
}

fn size(v: &Value) -> usize {
    match v {
        Value::Array(a) => a.len(),
        Value::Object(m) => m.len(),
        Value::String(s) => s.chars().count(),
        Value::Null => 0,
        _ => 1,
    }
}

fn contains(v: &Value, want: &Value) -> bool {
    match (v, want) {
        (Value::String(s), Value::String(w)) => s.to_lowercase().contains(&w.to_lowercase()),
        (Value::Array(a), w) => a.iter().any(|x| x == w || contains(x, w)),
        (Value::Object(m), Value::String(k)) => m.contains_key(k),
        _ => false,
    }
}

/// Checks one expectation against an answer; returns what failed.
pub fn check(answer: &Value, expect: &Value) -> Vec<String> {
    let list: Vec<&Value> = match expect {
        Value::Array(a) => a.iter().collect(),
        other => vec![other],
    };
    let mut failed = Vec::new();
    for e in list {
        let Some(e) = e.as_object() else {
            failed.push(format!("bad expectation {e}"));
            continue;
        };
        if let Some(path) = e.get("path").and_then(Value::as_str) {
            let got = lookup(answer, path);
            for (op, want) in e.iter().filter(|(k, _)| k.as_str() != "path") {
                let ok = match (op.as_str(), got) {
                    ("exists", g) => g.is_some() == want.as_bool().unwrap_or(true),
                    ("nonempty", g) => g.is_some_and(|g| size(g) > 0) == want.as_bool().unwrap_or(true),
                    (_, None) => false,
                    ("eq", Some(g)) => g == want || number(g).zip(number(want)).is_some_and(|(a, b)| a == b),
                    ("ne", Some(g)) => g != want,
                    ("gt", Some(g)) => number(g).zip(number(want)).is_some_and(|(a, b)| a > b),
                    ("ge", Some(g)) => number(g).zip(number(want)).is_some_and(|(a, b)| a >= b),
                    ("lt", Some(g)) => number(g).zip(number(want)).is_some_and(|(a, b)| a < b),
                    ("le", Some(g)) => number(g).zip(number(want)).is_some_and(|(a, b)| a <= b),
                    ("contains", Some(g)) => contains(g, want),
                    ("len_ge", Some(g)) => want.as_u64().is_some_and(|n| size(g) as u64 >= n),
                    ("len_eq", Some(g)) => want.as_u64().is_some_and(|n| size(g) as u64 == n),
                    (other, _) => {
                        failed.push(format!("unknown operator {other:?}"));
                        continue;
                    }
                };
                if !ok {
                    let shown = got.map_or("missing".to_owned(), |g| {
                        let s = g.to_string();
                        if s.len() > 200 { format!("{}…", &s[..200]) } else { s }
                    });
                    failed.push(format!("{path} {op} {want}: got {shown}"));
                }
            }
            continue;
        }
        for (key, want) in e {
            let ok = match key.as_str() {
                "ok" => answer.get("ok") == Some(want),
                "screen" => answer.get("screen").and_then(Value::as_str) == want.as_str(),
                "dismissed" => answer.get("dismissed").is_some_and(|d| size(d) > 0) == want.as_bool().unwrap_or(true),
                "blocking" => answer.get("blocking").is_some_and(|b| !b.is_null()) == want.as_bool().unwrap_or(true),
                "no_errors" => !log_errors(answer).is_empty() != want.as_bool().unwrap_or(true),
                other => {
                    failed.push(format!("unknown expectation {other:?}"));
                    continue;
                }
            };
            if !ok {
                failed.push(format!("{key} = {want}: got {}", answer.get(key).cloned().unwrap_or(Value::Null)));
            }
        }
    }
    failed
}

/// Error lines in an answer's `log`.
pub fn log_errors(answer: &Value) -> Vec<String> {
    answer
        .get("log")
        .and_then(Value::as_array)
        .map(|l| l.iter().filter_map(Value::as_str).filter(|s| s.starts_with("ERROR")).map(str::to_owned).collect())
        .unwrap_or_default()
}

/// Replaces `${name}` in the strings of `v` by captured values (a whole-string reference
/// keeps the value's type).
pub fn substitute(v: &Value, vars: &HashMap<String, Value>) -> Value {
    match v {
        Value::String(s) => {
            if let Some(name) = s.strip_prefix("${").and_then(|r| r.strip_suffix('}'))
                && let Some(val) = vars.get(name)
            {
                return val.clone();
            }
            let mut out = s.clone();
            for (k, val) in vars {
                let text = val.as_str().map(str::to_owned).unwrap_or_else(|| val.to_string());
                out = out.replace(&format!("${{{k}}}"), &text);
            }
            Value::String(out)
        }
        Value::Array(a) => Value::Array(a.iter().map(|x| substitute(x, vars)).collect()),
        Value::Object(m) => Value::Object(m.iter().map(|(k, x)| (k.clone(), substitute(x, vars))).collect()),
        other => other.clone(),
    }
}

/// What the runner reports per step.
#[derive(Debug, Clone)]
pub struct StepReport {
    pub request: Value,
    pub answer: Value,
    pub failures: Vec<String>,
    pub wall: Duration,
}

#[derive(Debug, Clone, Default)]
pub struct Report {
    pub name: String,
    pub passed: bool,
    pub skipped: Option<String>,
    pub steps: Vec<StepReport>,
    pub wall: Duration,
    pub frames: u64,
    pub tick: Option<u64>,
    pub dismissed: usize,
    pub log_errors: Vec<String>,
    /// Why it failed, if it did.
    pub failure: Option<String>,
}

/// Steps the runner answers itself instead of the game (e.g. `jev_check`).
pub trait Hooks {
    /// Answers a step whose `cmd` the driver does not know, or `None` to send it on.
    fn step(&mut self, _transport: &mut dyn Transport, _step: &Value) -> Option<Value> {
        None
    }
}

/// No runner-side steps.
#[cfg(test)]
pub struct NoHooks;
#[cfg(test)]
impl Hooks for NoHooks {}

/// Runs the steps of `scenario` against `transport`; stops at the first failure.
pub fn run(transport: &mut dyn Transport, scenario: &Scenario, hooks: &mut dyn Hooks) -> Report {
    let started = Instant::now();
    let mut report = Report { name: scenario.name.clone(), passed: true, ..Default::default() };
    let mut vars: HashMap<String, Value> = HashMap::new();
    for (i, raw) in scenario.steps.iter().enumerate() {
        let step = substitute(raw, &vars);
        let mut request: Map<String, Value> = step.as_object().cloned().unwrap_or_default();
        for k in META {
            request.remove(k);
        }
        let request = Value::Object(request);
        let timeout =
            Duration::from_secs(step.get("timeout_s").and_then(Value::as_u64).unwrap_or(scenario.step_timeout));
        let t = Instant::now();
        let answer = match hooks.step(transport, &request) {
            Some(a) => Ok(a),
            None => transport.request(&request, timeout),
        };
        let wall = t.elapsed();
        let answer = match answer {
            Ok(a) => a,
            Err(e) => {
                report.passed = false;
                report.failure = Some(format!("step {}: {e}", i + 1));
                report.steps.push(StepReport { request, answer: Value::Null, failures: vec![e], wall });
                break;
            }
        };
        let mut failures = Vec::new();
        let expect = step.get("expect");
        let wants_ok = expect.and_then(|e| e.get("ok")).and_then(Value::as_bool).unwrap_or(true);
        let skipped = answer.get("skipped").is_some();
        if !skipped && answer.get("ok").and_then(Value::as_bool) != Some(wants_ok) {
            let why = answer.get("error").and_then(Value::as_str).unwrap_or("not ok");
            failures.push(format!("ok = {wants_ok} expected: {why}"));
        }
        if let Some(e) = expect
            && !skipped
        {
            failures.extend(check(&answer, e).into_iter().filter(|f| !f.starts_with("ok =")));
        }
        if let Some(c) = step.get("capture").and_then(Value::as_object) {
            for (name, path) in c {
                match path.as_str().and_then(|p| lookup(&answer, p)) {
                    Some(v) => {
                        vars.insert(name.clone(), v.clone());
                    }
                    None => failures.push(format!("capture {name}: nothing at {path}")),
                }
            }
        }
        report.frames = answer.get("frame").and_then(Value::as_u64).unwrap_or(report.frames);
        report.tick = answer.get("tick").and_then(Value::as_u64).or(report.tick);
        report.dismissed += answer.get("dismissed").map_or(0, size);
        let errors = log_errors(&answer);
        if scenario.strict_log && !errors.is_empty() {
            failures.push(format!("errors in the log: {}", errors.join("; ")));
        }
        report.log_errors.extend(errors);
        let optional = step.get("optional").and_then(Value::as_bool).unwrap_or(false);
        let failed = !failures.is_empty() && !optional;
        if failed {
            report.passed = false;
            report.failure = Some(format!("step {} ({}): {}", i + 1, step_label(&request), failures.join("; ")));
        }
        report.steps.push(StepReport { request, answer, failures, wall });
        if failed {
            break;
        }
    }
    report.wall = started.elapsed();
    report
}

/// A short description of a step for summaries.
pub fn step_label(request: &Value) -> String {
    let cmd = request.get("cmd").and_then(Value::as_str).unwrap_or("?");
    let detail = ["label", "id", "key", "text", "map"]
        .iter()
        .find_map(|k| request.get(*k).and_then(Value::as_str).map(|s| format!(" {s:?}")))
        .or_else(|| request.get("until").map(|u| format!(" until {u}")))
        .or_else(|| request.get("ticks").map(|t| format!(" {t} ticks")))
        .unwrap_or_default();
    format!("{cmd}{detail}")
}

/// The command line of `cargo xtask e2e`.
#[derive(Debug, Default)]
pub struct Options {
    pub filter: Option<String>,
    pub game_dir: Option<PathBuf>,
    pub exe: Option<PathBuf>,
    pub build: bool,
    pub gpu: bool,
    pub list: bool,
    pub files: Vec<PathBuf>,
    pub verbose: bool,
}

/// The scenario files: the ones named, else `tests/e2e/*.json`.
pub fn scenario_files(root: &Path, options: &Options) -> Result<Vec<PathBuf>, String> {
    if !options.files.is_empty() {
        return Ok(options.files.clone());
    }
    let dir = root.join("tests").join("e2e");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();
    Ok(files)
}

/// Builds the game and copies it under its own name, so test runs never touch (or are
/// mistaken for) a player's `AmericanConquest.exe`.
pub fn build_exe(root: &Path, target: &Path) -> Result<PathBuf, String> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let status = Command::new(cargo)
        .current_dir(root)
        .args(["build", "--release", "--package", "acr-game", "--bin", "AmericanConquest"])
        .status()
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("building the game failed".into());
    }
    let built = target.join("release").join(format!("AmericanConquest{}", std::env::consts::EXE_SUFFIX));
    let dir = target.join("e2e");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let exe = dir.join(format!("AC-e2e{}", std::env::consts::EXE_SUFFIX));
    std::fs::copy(&built, &exe).map_err(|e| format!("{} -> {}: {e}", built.display(), exe.display()))?;
    Ok(exe)
}

fn write_transcript(path: &Path, report: &Report) {
    let mut text = String::new();
    for s in &report.steps {
        text.push_str(
            &json!({"request": s.request, "answer": s.answer, "wall_ms": s.wall.as_millis() as u64}).to_string(),
        );
        text.push('\n');
    }
    let _ = std::fs::write(path, text);
}

/// Runs the scenarios; returns whether all that ran passed.
pub fn e2e(root: &Path, target: &Path, options: &Options, hooks: &mut dyn Hooks) -> Result<bool, String> {
    let mut scenarios = Vec::new();
    for f in scenario_files(root, options)? {
        let s = Scenario::load(&f)?;
        if options.filter.as_deref().is_none_or(|flt| s.name.contains(flt)) {
            scenarios.push(s);
        }
    }
    if options.list {
        for s in &scenarios {
            println!("{:<28} {:>3} steps  {}", s.name, s.steps.len(), s.description);
        }
        return Ok(true);
    }
    let game_dir = options.game_dir.clone().or_else(|| std::env::var_os("ACR_GAME_DIR").map(PathBuf::from));
    let needs_game = scenarios.iter().any(|s| s.needs_data);
    let exe = match (&options.exe, needs_game && game_dir.is_some()) {
        (Some(e), _) => Some(e.clone()),
        (None, true) if options.build => Some(build_exe(root, target)?),
        (None, true) => {
            let e = target.join("e2e").join(format!("AC-e2e{}", std::env::consts::EXE_SUFFIX));
            e.is_file().then_some(e)
        }
        (None, false) => None,
    };
    let out = target.join("e2e").join("runs");
    let _ = std::fs::create_dir_all(&out);
    let mut reports = Vec::new();
    for s in &scenarios {
        let report = match (&game_dir, &exe) {
            (None, _) if s.needs_data => {
                Report { name: s.name.clone(), skipped: Some("needs --game-dir".into()), ..Default::default() }
            }
            (_, None) => {
                Report { name: s.name.clone(), skipped: Some("no game executable".into()), ..Default::default() }
            }
            (dir, Some(exe)) => {
                let dir = dir.clone().unwrap_or_default();
                let work = out.join(&s.name);
                let _ = std::fs::remove_dir_all(&work);
                let mut args = s.args.clone();
                if options.gpu && !args.iter().any(|a| a == "--drive-gpu") {
                    args.push("--drive-gpu".into());
                }
                eprintln!("e2e: {} ...", s.name);
                let t = Instant::now();
                match Process::spawn(exe, &dir, &args, &work) {
                    Ok((mut p, _ready)) => {
                        let mut r = run(&mut p, s, hooks);
                        p.quit();
                        r.wall = t.elapsed();
                        write_transcript(&work.join("transcript.jsonl"), &r);
                        r
                    }
                    Err(e) => {
                        Report { name: s.name.clone(), failure: Some(e), wall: t.elapsed(), ..Default::default() }
                    }
                }
            }
        };
        if options.verbose {
            for st in &report.steps {
                eprintln!("  {:>6} ms  {}  {}", st.wall.as_millis(), step_label(&st.request), st.failures.join("; "));
            }
        }
        reports.push(report);
    }
    print_summary(&reports);
    Ok(reports.iter().all(|r| r.passed || r.skipped.is_some()))
}

pub fn print_summary(reports: &[Report]) {
    println!("{:<28} {:>6} {:>6} {:>8} {:>7} {:>6}  result", "scenario", "steps", "wall s", "frames", "ticks", "dlgs");
    for r in reports {
        let result = match (&r.skipped, r.passed) {
            (Some(why), _) => format!("skipped ({why})"),
            (None, true) => "ok".to_owned(),
            (None, false) => format!("FAILED: {}", r.failure.clone().unwrap_or_default()),
        };
        println!(
            "{:<28} {:>6} {:>6.1} {:>8} {:>7} {:>6}  {result}",
            r.name,
            r.steps.len(),
            r.wall.as_secs_f64(),
            r.frames,
            r.tick.map_or("-".into(), |t| t.to_string()),
            r.dismissed,
        );
        if !r.log_errors.is_empty() {
            println!("    {} error line(s) in the game log, first: {}", r.log_errors.len(), r.log_errors[0]);
        }
    }
    let ran: Vec<&Report> = reports.iter().filter(|r| r.skipped.is_none()).collect();
    let passed = ran.iter().filter(|r| r.passed).count();
    let wall: f64 = ran.iter().map(|r| r.wall.as_secs_f64()).sum();
    println!("{passed}/{} passed, {} skipped, {wall:.1} s", ran.len(), reports.len() - ran.len());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stand-in game: answers by command from a script, records what it got.
    struct Fake {
        got: Vec<Value>,
        tick: u64,
    }

    impl Transport for Fake {
        fn request(&mut self, request: &Value, _timeout: Duration) -> Result<Value, String> {
            self.got.push(request.clone());
            let cmd = request["cmd"].as_str().unwrap_or("");
            Ok(match cmd {
                "click" if request["label"] == "Missing" => json!({"ok": false, "error": "no clickable widget"}),
                "click" => json!({"ok": true, "screen": "RandomMap", "target": {"id": "MainMenu/5"}}),
                "wait" => {
                    self.tick += request["ticks"].as_u64().unwrap_or(0);
                    json!({"ok": true, "tick": self.tick, "frame": self.tick, "dismissed": [{"text": "Hallo"}]})
                }
                "state" => {
                    json!({"ok": true, "state": {"resources": {"wood": 500}, "counts": {"0": {"sites": {"Fort": 1}}}}})
                }
                "boom" => return Err("the game exited".into()),
                _ => json!({"ok": true, "log": ["ERROR acr: bad thing"]}),
            })
        }
    }

    fn scenario(steps: Value) -> Scenario {
        Scenario::parse(&json!({"name": "t", "steps": steps}).to_string(), "x").unwrap()
    }

    #[test]
    fn parses_scenarios_and_rejects_bad_ones() {
        let s = Scenario::parse(r#"{"steps": [{"cmd": "ui"}], "args": ["--dialogs", "report"]}"#, "file").unwrap();
        assert_eq!((s.name.as_str(), s.needs_data, s.args.len()), ("file", true, 2));
        assert!(Scenario::parse(r#"{"steps": [{"label": "x"}]}"#, "f").unwrap_err().contains("cmd"));
        assert!(Scenario::parse(r#"{"steps": [{"cmd": "ui", "expect": 3}]}"#, "f").is_err());
        assert!(Scenario::parse("{", "f").unwrap_err().starts_with("bad JSON"));
    }

    #[test]
    fn checks_paths_and_shorthands() {
        let a = json!({"ok": true, "screen": "game", "state": {"wood": 500, "units": [1, 2]}, "dismissed": [1]});
        assert!(check(&a, &json!({"path": "state.wood", "lt": 1000, "gt": 100})).is_empty());
        assert!(check(&a, &json!({"path": "state.units.1", "eq": 2})).is_empty());
        assert!(
            check(&a, &json!([{"screen": "game"}, {"dismissed": true}, {"path": "state.units", "len_ge": 2}]))
                .is_empty()
        );
        assert_eq!(check(&a, &json!({"path": "state.wood", "eq": 1})).len(), 1);
        assert_eq!(check(&a, &json!({"path": "state.nothing", "exists": true})).len(), 1);
        assert!(check(&a, &json!({"path": "state.nothing", "exists": false})).is_empty());
        assert_eq!(check(&a, &json!({"bogus": 1})), vec!["unknown expectation \"bogus\"".to_owned()]);
    }

    #[test]
    fn runs_until_the_first_failure_and_captures() {
        let s = scenario(json!([
            {"cmd": "click", "label": "Einzelspieler", "expect": {"screen": "RandomMap"}, "capture": {"w": "target.id"}},
            {"cmd": "wait", "ticks": 5, "expect": {"dismissed": true}},
            {"cmd": "click", "id": "${w}"},
            {"cmd": "state", "expect": {"path": "state.counts.0.sites", "nonempty": true}},
            {"cmd": "click", "label": "Missing"},
            {"cmd": "state"},
        ]));
        let mut fake = Fake { got: Vec::new(), tick: 0 };
        let r = run(&mut fake, &s, &mut NoHooks);
        assert!(!r.passed);
        assert_eq!(r.steps.len(), 5, "stops after the failing click");
        assert_eq!(fake.got[2]["id"], "MainMenu/5", "captured and substituted");
        assert!(fake.got.iter().all(|g| g.get("expect").is_none()), "runner keys are not sent");
        assert_eq!((r.tick, r.dismissed), (Some(5), 1));
        assert!(r.failure.unwrap().contains("step 5"));
    }

    #[test]
    fn expected_failures_logs_and_crashes() {
        let s = scenario(json!([{"cmd": "click", "label": "Missing", "expect": {"ok": false}}, {"cmd": "ui"}]));
        let mut fake = Fake { got: Vec::new(), tick: 0 };
        let r = run(&mut fake, &s, &mut NoHooks);
        assert!(r.passed, "{:?}", r.failure);
        assert_eq!(r.log_errors.len(), 1);
        let mut strict = s.clone();
        strict.strict_log = true;
        assert!(!run(&mut fake, &strict, &mut NoHooks).passed);
        let crash = scenario(json!([{"cmd": "boom"}]));
        let r = run(&mut fake, &crash, &mut NoHooks);
        assert!(!r.passed && r.failure.unwrap().contains("exited"));
    }

    #[test]
    fn hooks_answer_runner_steps() {
        struct Jevish;
        impl Hooks for Jevish {
            fn step(&mut self, _t: &mut dyn Transport, step: &Value) -> Option<Value> {
                (step["cmd"] == "jev_check").then(|| json!({"skipped": "no key"}))
            }
        }
        let s = scenario(json!([{"cmd": "jev_check", "expect": {"path": "p", "gt": 0.9}}]));
        let mut fake = Fake { got: Vec::new(), tick: 0 };
        let r = run(&mut fake, &s, &mut Jevish);
        assert!(r.passed && fake.got.is_empty());
    }

    /// Every scenario in the repository parses (runs in CI without the game data).
    #[test]
    fn repository_scenarios_parse() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("tests").join("e2e");
        let mut n = 0;
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|x| x == "json") {
                let s = Scenario::load(&path).unwrap();
                assert!(!s.steps.is_empty(), "{}", path.display());
                n += 1;
            }
        }
        assert!(n >= 5, "found {n} scenarios");
    }
}
