//! `cargo xtask jev`: cheap, typed decisions from TypeSafe's System One model Jev for
//! routine test steps, so a reasoning model is only needed when Jev is unsure.
//!
//! - Which widget of a `ui` tree leads towards a goal (Choice over widget keys plus `none`).
//! - Whether a state matches an expectation in plain words (Noul), see the `jev_check`
//!   scenario step.
//! - What kind of screen or dialog is shown (Choice).
//! - Whether a screenshot looks broken (Noul with an image).
//!
//! Low confidence returns [`Decision::NeedsReasoning`] instead of guessing. The API key is
//! read at run time from `~/.config/typesafe/api_key` (or the file `TYPESAFE_API_KEY_FILE`
//! names) and only ever handed to `curl` through its standard input: it is never printed,
//! logged or passed on a command line. Without a key the helper says so and callers fall
//! back. Every call's token usage is counted.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};

use crate::e2e::Transport;

const URL: &str = "https://api.typesafe.ai/v1/systemone";
const MODEL: &str = "jev-latest";
/// A widget is clicked only if Jev's choice is at least this likely and confident.
pub const MIN_CHOICE: f64 = 0.5;
pub const MIN_CONFIDENCE: f64 = 0.4;
/// The goal counts as reached above this probability.
pub const GOAL_REACHED: f64 = 0.8;

/// Tokens used so far.
#[derive(Debug, Default, Clone, Copy)]
pub struct Usage {
    pub calls: u32,
    pub input: u64,
    pub output: u64,
    pub millis: u64,
}

pub struct Jev {
    key: String,
    pub usage: Usage,
    /// Where request bodies are written for `curl` (they hold no secret).
    scratch: PathBuf,
}

/// The key file: `TYPESAFE_API_KEY_FILE`, else `~/.config/typesafe/api_key`.
pub fn key_path() -> PathBuf {
    if let Some(p) = std::env::var_os("TYPESAFE_API_KEY_FILE") {
        return PathBuf::from(p);
    }
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).unwrap_or_default();
    PathBuf::from(home).join(".config").join("typesafe").join("api_key")
}

impl Jev {
    /// Reads the key; the error says where it was looked for (never what it is).
    pub fn from_env(scratch: &Path) -> Result<Self, String> {
        let path = key_path();
        let key = std::fs::read_to_string(&path).map(|k| k.trim().to_owned()).unwrap_or_default();
        if key.is_empty() {
            return Err(format!("no TypeSafe API key in {} (set TYPESAFE_API_KEY_FILE); falling back", path.display()));
        }
        if key.contains(['"', '\n', '\\']) {
            return Err(format!("the key in {} has unexpected characters", path.display()));
        }
        std::fs::create_dir_all(scratch).map_err(|e| e.to_string())?;
        Ok(Self { key, usage: Usage::default(), scratch: scratch.to_owned() })
    }

    /// One System One request: `questions` over `state`, with PNG `images` as assets
    /// keyed by their placeholder. Returns the answers.
    pub fn ask(&mut self, state: &Value, questions: &Value, images: &[(String, Vec<u8>)]) -> Result<Value, String> {
        let mut body = json!({"model": MODEL, "state": state, "questions": questions});
        if !images.is_empty() {
            use base64::Engine;
            let assets: Map<String, Value> = images
                .iter()
                .map(|(k, png)| {
                    let data = base64::engine::general_purpose::STANDARD.encode(png);
                    (k.clone(), json!({"type": "image", "format": "png", "data_base64": data}))
                })
                .collect();
            body["assets"] = Value::Object(assets);
        }
        let file = self.scratch.join(format!("jev-request-{}.json", std::process::id()));
        std::fs::write(&file, body.to_string()).map_err(|e| e.to_string())?;
        // The key goes through curl's config on stdin only.
        let mut config = format!(
            "url = \"{URL}\"\nheader = \"Authorization: Bearer {}\"\nheader = \"Content-Type: application/json\"\n\
             data-binary = \"@{}\"\nsilent\nshow-error\nmax-time = 60\nwrite-out = \"\\n%{{http_code}}\"\n",
            self.key,
            file.to_string_lossy().replace('\\', "/"),
        );
        if !images.is_empty() {
            config.push_str("header = \"TypeSafe-Danger-Zone-Images: true\"\n");
        }
        let t = Instant::now();
        let mut child = Command::new("curl")
            .args(["-K", "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("curl: {e}"))?;
        child.stdin.take().ok_or("curl stdin")?.write_all(config.as_bytes()).map_err(|e| e.to_string())?;
        let out = child.wait_with_output().map_err(|e| e.to_string())?;
        let _ = std::fs::remove_file(&file);
        let millis = t.elapsed().as_millis() as u64;
        if !out.status.success() {
            return Err(format!("curl failed: {}", String::from_utf8_lossy(&out.stderr).trim()));
        }
        let text = String::from_utf8_lossy(&out.stdout);
        let (json_text, status) = text.rsplit_once('\n').unwrap_or((&text, ""));
        if status.trim() != "200" {
            let mut shown: String = json_text.chars().take(300).collect();
            shown = shown.replace(&self.key, "<key>");
            return Err(format!("TypeSafe answered HTTP {}: {shown}", status.trim()));
        }
        let v: Value = serde_json::from_str(json_text).map_err(|e| format!("bad answer: {e}"))?;
        self.usage.calls += 1;
        self.usage.input += v.pointer("/usage/input_tokens").and_then(Value::as_u64).unwrap_or(0);
        self.usage.output += v.pointer("/usage/output_tokens").and_then(Value::as_u64).unwrap_or(0);
        self.usage.millis += millis;
        v.get("answers").cloned().ok_or("no answers".into())
    }
}

/// A Choice answer.
#[derive(Debug, Clone, PartialEq)]
pub struct Choice {
    pub choice: String,
    pub confidence: f64,
    pub probability: f64,
}

pub fn choice(answer: Option<&Value>) -> Option<Choice> {
    let a = answer?;
    let choice = a.get("choice")?.as_str()?.to_owned();
    let probability = a.get("probabilities").and_then(|p| p.get(&choice)).and_then(Value::as_f64).unwrap_or(0.0);
    Some(Choice { confidence: a.get("confidence").and_then(Value::as_f64).unwrap_or(0.0), probability, choice })
}

pub fn noul(answer: Option<&Value>) -> Option<f64> {
    answer?.get("noul")?.as_f64()
}

/// What to do next on the way to a goal.
#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    /// The goal is reached (its probability).
    Done(f64),
    /// Click the widget with this id.
    Click { id: String, label: String, choice: Choice },
    /// Jev is unsure or sees no way: a reasoning model (or a person) should look.
    NeedsReasoning(String),
}

/// The widgets Jev may choose from: labelled, clickable ones (not input boxes, combo boxes,
/// sliders or scroll bars), as `(key, id, label)`.
pub fn candidates(ui: &Value) -> Vec<(String, String, String)> {
    let widgets = ui.get("widgets").and_then(Value::as_array).cloned().unwrap_or_default();
    widgets
        .iter()
        .filter(|w| w.get("clickable").and_then(Value::as_bool).unwrap_or(false))
        .filter(|w| !matches!(w["kind"].as_str(), Some("input" | "combo" | "slider" | "scrollbar" | "picture")))
        .filter(|w| w.get("enabled").and_then(Value::as_bool).unwrap_or(true))
        .filter_map(|w| {
            let label = w["label"].as_str()?.trim().to_owned();
            (!label.is_empty()).then(|| (w["id"].as_str().unwrap_or_default().to_owned(), label))
        })
        .enumerate()
        .map(|(i, (id, label))| (format!("w{i}"), id, label))
        .collect()
}

/// The state and questions of one navigation step: is the goal reached, and which widget
/// comes next.
pub fn navigation_request(goal: &str, ui: &Value, history: &[String]) -> (Value, Value, Vec<(String, String, String)>) {
    let cands = candidates(ui);
    let mut criteria: Map<String, Value> =
        cands.iter().map(|(k, _, label)| (k.clone(), Value::String(format!("Click \"{label}\"")))).collect();
    criteria.insert("none".into(), "None of the widgets brings the game closer to the goal.".into());
    let state = json!({
        "goal": goal,
        "game": "American Conquest, a real-time strategy game with a German user interface",
        "screen": ui.get("screen"),
        "screens_open": ui.get("stack"),
        "game_running": ui.get("in_game"),
        "dialog": ui.get("dialog"),
        "texts_on_screen": ui.get("widgets").and_then(Value::as_array).map(|w| w
            .iter()
            .filter(|w| !w.get("clickable").and_then(Value::as_bool).unwrap_or(false))
            .filter_map(|w| w["label"].as_str())
            .take(30)
            .collect::<Vec<_>>()),
        "clicked_so_far": history,
    });
    let questions = json!({
        "reached": {
            "type": "noul",
            "instructions": "Has the goal in `goal` been reached? Judge from `screen`, `game_running` and the clicks in `clicked_so_far` that led here: a game counts as started when `game_running` is true and `screen` is `game`; which kind of game it is follows from the menu entries clicked on the way.",
        },
        "next": {
            "type": "choice",
            "instructions": "Which click brings the game closer to the goal in `goal`? Menu entries lead to sub-menus. A settings screen for the goal (its title names it) is confirmed with its start button: in this game every game, also a random map, is started with \"Mission spielen!\". Avoid going back or quitting.",
            "criteria": criteria,
        },
    });
    (state, questions, cands)
}

/// Turns the answers of [`navigation_request`] into a decision with the thresholds.
pub fn decide(answers: &Value, cands: &[(String, String, String)]) -> Decision {
    let reached = noul(answers.get("reached")).unwrap_or(0.0);
    if reached >= GOAL_REACHED {
        return Decision::Done(reached);
    }
    let Some(c) = choice(answers.get("next")) else { return Decision::NeedsReasoning("no answer".into()) };
    if c.choice == "none" {
        return Decision::NeedsReasoning(format!(
            "Jev sees no widget towards the goal (p {:.2}; goal reached p {reached:.2})",
            c.probability
        ));
    }
    if c.probability < MIN_CHOICE || c.confidence < MIN_CONFIDENCE {
        return Decision::NeedsReasoning(format!(
            "unsure: {} at p {:.2}, confidence {:.2}",
            c.choice, c.probability, c.confidence
        ));
    }
    match cands.iter().find(|(k, _, _)| *k == c.choice) {
        Some((_, id, label)) => Decision::Click { id: id.clone(), label: label.clone(), choice: c },
        None => Decision::NeedsReasoning(format!("unknown choice {}", c.choice)),
    }
}

/// One line of an exploration's trace.
#[derive(Debug, Clone)]
pub struct TraceStep {
    pub screen: String,
    pub decision: Decision,
    pub millis: u64,
}

pub struct Exploration {
    pub reached: bool,
    pub stopped: String,
    pub trace: Vec<TraceStep>,
}

/// Goes towards `goal` by asking Jev for the next click on each screen, at most
/// `max_steps` clicks. Stops when the goal is reached, Jev is unsure or nothing changes.
pub fn explore(jev: &mut Jev, game: &mut dyn Transport, goal: &str, max_steps: usize) -> Result<Exploration, String> {
    let timeout = Duration::from_secs(120);
    let mut history: Vec<String> = Vec::new();
    let mut trace = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for _ in 0..=max_steps {
        let answer = game.request(&json!({"cmd": "ui"}), timeout)?;
        let ui = answer.get("ui").cloned().unwrap_or(Value::Null);
        let screen = ui["screen"].as_str().unwrap_or("?").to_owned();
        // The same screen with the same widgets three times: no progress.
        let fingerprint =
            format!("{screen}|{}", candidates(&ui).iter().map(|c| c.2.as_str()).collect::<Vec<_>>().join(","));
        seen.push(fingerprint.clone());
        if seen.iter().filter(|f| **f == fingerprint).count() >= 3 {
            return Ok(Exploration { reached: false, stopped: "no progress".into(), trace });
        }
        let (state, questions, cands) = navigation_request(goal, &ui, &history);
        let t = Instant::now();
        let answers = jev.ask(&state, &questions, &[])?;
        let decision = decide(&answers, &cands);
        let millis = t.elapsed().as_millis() as u64;
        trace.push(TraceStep { screen: screen.clone(), decision: decision.clone(), millis });
        match decision {
            Decision::Done(_) => return Ok(Exploration { reached: true, stopped: "goal reached".into(), trace }),
            Decision::NeedsReasoning(why) => {
                return Ok(Exploration { reached: false, stopped: format!("needs reasoning: {why}"), trace });
            }
            Decision::Click { id, label, .. } => {
                let r = game.request(&json!({"cmd": "click", "id": id}), timeout)?;
                if r.get("ok").and_then(Value::as_bool) != Some(true) {
                    return Ok(Exploration {
                        reached: false,
                        stopped: format!("click failed: {}", r.get("error").cloned().unwrap_or_default()),
                        trace,
                    });
                }
                history.push(label);
            }
        }
    }
    Ok(Exploration { reached: false, stopped: format!("{max_steps} steps used"), trace })
}

/// Asks whether the game's current `ui` and `state` match `expectation` (a `jev_check`
/// step); answers like a driver reply with `p` and `pass`.
pub fn check_expectation(jev: &mut Jev, game: &mut dyn Transport, expectation: &str, min: f64) -> Value {
    let timeout = Duration::from_secs(60);
    let ui = game.request(&json!({"cmd": "ui"}), timeout).ok().and_then(|a| a.get("ui").cloned());
    let st = game.request(&json!({"cmd": "state"}), timeout).ok().and_then(|a| a.get("state").cloned());
    let state = json!({"expectation": expectation, "ui": ui, "game_state": st});
    let questions = json!({"matches": {
        "type": "noul",
        "instructions": "Does the game situation in `ui` and `game_state` match the statement in `expectation`?",
    }});
    match jev.ask(&state, &questions, &[]) {
        Ok(answers) => {
            let p = noul(answers.get("matches")).unwrap_or(0.0);
            let pass = p >= min;
            let mut r = json!({"ok": pass, "p": p, "min": min});
            if !pass {
                r["error"] = format!("Jev: {p:.2} < {min} for {expectation:?}; needs reasoning").into();
            }
            r
        }
        Err(e) => json!({"ok": false, "error": e}),
    }
}

/// Asks whether a screenshot looks broken (missing graphics, garbage, black areas).
pub fn looks_broken(jev: &mut Jev, png: Vec<u8>, what: &str) -> Result<f64, String> {
    let key = "c1a5e0d2-screenshot".to_owned();
    let state = json!({"screenshot": key, "expected": what});
    let questions = json!({"broken": {
        "type": "noul",
        "instructions": "Does `screenshot` look broken compared with what `expected` describes: large black or garbage areas, missing graphics, misplaced or overlapping interface parts?",
    }});
    let answers = jev.ask(&state, &questions, &[(key, png)])?;
    noul(answers.get("broken")).ok_or("no answer".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ui() -> Value {
        json!({"screen": "MainMenu", "in_game": false, "widgets": [
            {"id": "MainMenu/5", "kind": "text", "label": "Einzelspieler", "clickable": true},
            {"id": "MainMenu/6", "kind": "text", "label": "Mehrspieler", "clickable": true},
            {"id": "RandomMap/10", "kind": "input", "label": "EINSTEIN", "clickable": true},
            {"id": "MainMenu/20", "kind": "text", "label": "V 1.46+"},
            {"id": "MainMenu/7", "kind": "button", "label": "", "clickable": true},
        ]})
    }

    #[test]
    fn candidates_are_labelled_buttons() {
        let c = candidates(&ui());
        assert_eq!(c.len(), 2);
        assert_eq!(c[0], ("w0".into(), "MainMenu/5".into(), "Einzelspieler".into()));
        let (state, questions, _) = navigation_request("start a random map game", &ui(), &[]);
        assert_eq!(state["texts_on_screen"], json!(["V 1.46+"]));
        let criteria = questions["next"]["criteria"].as_object().unwrap();
        assert!(criteria.contains_key("w1") && criteria.contains_key("none"));
    }

    #[test]
    fn decisions_follow_the_thresholds() {
        let c = candidates(&ui());
        let answers = |reached: f64, pick: &str, p: f64, conf: f64| json!({"reached": {"noul": reached}, "next": {"choice": pick, "confidence": conf, "probabilities": {pick: p}}});
        assert_eq!(decide(&answers(0.9, "w0", 0.9, 0.9), &c), Decision::Done(0.9));
        assert!(matches!(decide(&answers(0.1, "w0", 0.9, 0.8), &c), Decision::Click { id, .. } if id == "MainMenu/5"));
        assert!(matches!(decide(&answers(0.1, "w1", 0.3, 0.8), &c), Decision::NeedsReasoning(_)));
        assert!(matches!(decide(&answers(0.1, "w1", 0.9, 0.1), &c), Decision::NeedsReasoning(_)));
        assert!(matches!(decide(&answers(0.1, "none", 0.9, 0.9), &c), Decision::NeedsReasoning(_)));
        assert!(matches!(decide(&json!({}), &c), Decision::NeedsReasoning(_)));
    }

    #[test]
    fn missing_key_says_where_it_looked() {
        // SAFETY of the test: only this test sets the variable.
        let dir = std::env::temp_dir().join("acr-jev-test-no-key");
        unsafe { std::env::set_var("TYPESAFE_API_KEY_FILE", dir.join("none")) };
        let e = Jev::from_env(&dir).err().unwrap();
        unsafe { std::env::remove_var("TYPESAFE_API_KEY_FILE") };
        assert!(e.contains("no TypeSafe API key") && e.contains("falling back"));
    }
}
