//! The driver's requests: one JSON object per line, `{"cmd": "...", ...}`, parsed into
//! [`Request`]. See `docs/testing.md` for the reference.

use serde_json::Value;

/// How long a click waits for its target to appear, and the default of `wait`, in frames.
pub const CLICK_TIMEOUT: u64 = 600;
pub const WAIT_TIMEOUT: u64 = 6000;

/// What a pointer command aims at.
#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    /// A widget, list row or HUD icon by its text: an exact match (case and spacing aside)
    /// wins, else the first one containing the text unless `exact`; `index` picks among
    /// several.
    Label { text: String, exact: bool, index: usize },
    /// A widget id from `ui`, e.g. `MainMenu/7` or `hud/14`.
    Id(String),
    /// Window pixels.
    Pos([f32; 2]),
    /// A unit or building by its entity id (the view centres on it if needed).
    Unit(u32),
    /// The first living object (by id) that matches.
    UnitOf(UnitFilter),
    /// A world pixel on the ground (the view centres on it if needed).
    World([i32; 2]),
    /// The ground at the mean position of the matching objects plus an offset (world
    /// pixels), e.g. a free spot next to the peasants for a building.
    Near(UnitFilter, [i32; 2]),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Mods {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

/// Objects by owner and type (a part of the type id, ignoring case).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct UnitFilter {
    pub owner: Option<u8>,
    pub kind: Option<String>,
}

/// A condition `wait` waits for; all conditions of one `until` must hold.
#[derive(Debug, Clone, PartialEq)]
pub enum Cond {
    /// The top screen has this name (e.g. `RandomMap`, `MessageWindow`); `game` means the
    /// running game with no screen over it.
    Screen(String),
    /// A modal message (mission page, question, choice) is open; with a name, that screen.
    Dialog(Option<String>),
    NoDialog,
    /// A game is shown.
    InGame,
    /// The menus are shown (no game).
    Menus,
    /// A visible widget, HUD hint or message contains this text.
    Label(String),
    /// The simulation reached this tick.
    Tick(u64),
    /// At least `min` living objects match.
    Units {
        owner: Option<u8>,
        kind: Option<String>,
        building: Option<bool>,
        finished: Option<bool>,
        min: usize,
    },
    /// At least this many units are selected.
    Selected(usize),
    /// The player has at least this many formations (brigades with living members).
    Formations(usize),
    /// The mission ended: won (`true`) or lost.
    Ended(Option<bool>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Wait {
    pub frames: Option<u64>,
    pub ticks: Option<u64>,
    pub until: Vec<Cond>,
    /// Frames before the wait fails.
    pub timeout: u64,
}

/// What the dialog policy does with modal messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogPolicy {
    /// Press the default button (OK, the first choice) and report it.
    Auto,
    /// Leave them open; answers report them as `blocking`.
    Report,
}

impl DialogPolicy {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "auto" | "dismiss" => Some(Self::Auto),
            "report" | "block" | "keep" => Some(Self::Report),
            _ => None,
        }
    }
}

/// A shortcut start of a game.
#[derive(Debug, Clone, PartialEq)]
pub enum Start {
    /// A map file (`Missions\...\x.m3d`) without its script.
    Map(String),
    /// A random map: the local player's nation and the computer opponents.
    Random { nation: usize, opponents: usize, seed: u32, resources: usize, size: usize, difficulty: usize },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    Ui {
        all: bool,
    },
    Click {
        target: Target,
        button: Button,
        double: bool,
        mods: Mods,
        timeout: u64,
    },
    Move {
        target: Target,
    },
    Key {
        key: String,
        mods: Mods,
    },
    Drag {
        from: Target,
        to: Target,
        button: Button,
        mods: Mods,
    },
    /// A selection rectangle over the living objects that match (first `max` by id).
    DragAround {
        filter: UnitFilter,
        max: usize,
        mods: Mods,
    },
    Text {
        text: String,
    },
    Wheel {
        lines: f32,
    },
    Wait(Wait),
    State {
        units: bool,
        owner: Option<u8>,
        visible_only: bool,
        /// Add each unit's orders, animation, stance and reload counters.
        detail: bool,
    },
    /// A simulation command (see `drive::command`).
    Command(Value),
    /// Replace the selection with these units.
    Select(Vec<u32>),
    Screenshot {
        path: String,
        crop: Option<[u32; 4]>,
    },
    Start(Start),
    Save {
        name: String,
    },
    Load {
        name: String,
    },
    Dialogs(DialogPolicy),
    Quit,
}

fn string(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn uint(v: &Value, key: &str) -> Option<u64> {
    v.get(key).and_then(Value::as_u64)
}

fn point(v: &Value) -> Option<[f64; 2]> {
    let a = v.as_array()?;
    match a.as_slice() {
        [x, y] => Some([x.as_f64()?, y.as_f64()?]),
        _ => None,
    }
}

fn mods(v: &Value) -> Mods {
    let list: Vec<String> = match v.get("mods") {
        Some(Value::Array(a)) => a.iter().filter_map(Value::as_str).map(str::to_lowercase).collect(),
        Some(Value::String(s)) => s.split('+').map(str::to_lowercase).collect(),
        _ => Vec::new(),
    };
    let has = |names: &[&str]| list.iter().any(|m| names.contains(&m.as_str()));
    Mods { ctrl: has(&["ctrl", "control"]), shift: has(&["shift"]), alt: has(&["alt"]) }
}

fn button(v: &Value, default: Button) -> Result<Button, String> {
    match v.get("button").and_then(Value::as_str) {
        None => Ok(default),
        Some("left") => Ok(Button::Left),
        Some("right") => Ok(Button::Right),
        Some("middle") => Ok(Button::Middle),
        Some(other) => Err(format!("unknown button {other:?}")),
    }
}

/// The target named by the keys of `v`: `label`, `id`, `pos`, `unit` or `world`.
pub fn target(v: &Value) -> Result<Target, String> {
    if let Some(p) = point(v) {
        return Ok(Target::Pos([p[0] as f32, p[1] as f32]));
    }
    if let Some(text) = string(v, "label") {
        let exact = v.get("exact").and_then(Value::as_bool).unwrap_or(false);
        let index = uint(v, "index").unwrap_or(0) as usize;
        return Ok(Target::Label { text, exact, index });
    }
    if let Some(id) = string(v, "id") {
        return Ok(Target::Id(id));
    }
    if let Some(p) = v.get("pos").and_then(point) {
        return Ok(Target::Pos([p[0] as f32, p[1] as f32]));
    }
    if let Some(u) = uint(v, "unit") {
        return Ok(Target::Unit(u as u32));
    }
    if let Some(f) = v.get("unit").filter(|f| f.is_object()) {
        return Ok(Target::UnitOf(UnitFilter { owner: uint(f, "owner").map(|o| o as u8), kind: string(f, "type") }));
    }
    if let Some(p) = v.get("world").and_then(point) {
        return Ok(Target::World([p[0] as i32, p[1] as i32]));
    }
    if let Some(f) = v.get("near").filter(|f| f.is_object()) {
        let offset = v.get("offset").and_then(point).unwrap_or([0.0, 0.0]);
        let filter = UnitFilter { owner: uint(f, "owner").map(|o| o as u8), kind: string(f, "type") };
        return Ok(Target::Near(filter, [offset[0] as i32, offset[1] as i32]));
    }
    Err("no target: give label, id, pos, unit or world".into())
}

fn cond(key: &str, v: &Value) -> Result<Cond, String> {
    let truth = v.as_bool();
    Ok(match key {
        "screen" => Cond::Screen(v.as_str().ok_or("screen: a name")?.to_owned()),
        "dialog" => match (v.as_str(), truth) {
            (Some(name), _) => Cond::Dialog(Some(name.to_owned())),
            (_, Some(true)) => Cond::Dialog(None),
            (_, Some(false)) => Cond::NoDialog,
            _ => return Err("dialog: true, false or a screen name".into()),
        },
        "in_game" if truth == Some(false) => Cond::Menus,
        "in_game" => Cond::InGame,
        "menus" if truth == Some(false) => Cond::InGame,
        "menus" => Cond::Menus,
        "label" => Cond::Label(v.as_str().ok_or("label: a text")?.to_owned()),
        "tick" => Cond::Tick(v.as_u64().ok_or("tick: a number")?),
        "selected" => Cond::Selected(v.as_u64().ok_or("selected: a number")? as usize),
        "formations" => Cond::Formations(v.as_u64().ok_or("formations: a number")? as usize),
        "ended" => Cond::Ended(match v.as_str() {
            Some("won") => Some(true),
            Some("lost") => Some(false),
            _ => None,
        }),
        "units" => Cond::Units {
            owner: uint(v, "owner").map(|o| o as u8),
            kind: string(v, "type"),
            building: v.get("building").and_then(Value::as_bool),
            finished: v.get("finished").and_then(Value::as_bool),
            min: uint(v, "min").unwrap_or(1) as usize,
        },
        other => return Err(format!("unknown condition {other:?}")),
    })
}

/// Parses one request line.
pub fn parse(line: &str) -> Result<(Option<Value>, Request), String> {
    let v: Value = serde_json::from_str(line).map_err(|e| format!("bad JSON: {e}"))?;
    let seq = v.get("seq").cloned();
    let cmd = v.get("cmd").and_then(Value::as_str).ok_or("missing \"cmd\"")?;
    let request = match cmd {
        "ui" => Request::Ui { all: v.get("all").and_then(Value::as_bool).unwrap_or(false) },
        "click" | "rclick" => Request::Click {
            target: target(&v)?,
            button: if cmd == "rclick" { Button::Right } else { button(&v, Button::Left)? },
            double: v.get("double").and_then(Value::as_bool).unwrap_or(false),
            mods: mods(&v),
            timeout: uint(&v, "timeout").unwrap_or(CLICK_TIMEOUT),
        },
        "move" => Request::Move { target: target(&v)? },
        "key" => Request::Key { key: string(&v, "key").ok_or("key: missing \"key\"")?, mods: mods(&v) },
        "drag" if v.get("around").is_some() => {
            let a = &v["around"];
            Request::DragAround {
                filter: UnitFilter { owner: uint(a, "owner").map(|o| o as u8), kind: string(a, "type") },
                max: uint(a, "max").unwrap_or(u64::MAX) as usize,
                mods: mods(&v),
            }
        }
        "drag" => Request::Drag {
            from: target(v.get("from").ok_or("drag: missing \"from\"")?)?,
            to: target(v.get("to").ok_or("drag: missing \"to\"")?)?,
            button: button(&v, Button::Left)?,
            mods: mods(&v),
        },
        "text" => Request::Text { text: string(&v, "text").ok_or("text: missing \"text\"")? },
        "wheel" => Request::Wheel { lines: v.get("lines").and_then(Value::as_f64).unwrap_or(1.0) as f32 },
        "wait" => {
            let mut until = Vec::new();
            match v.get("until") {
                Some(Value::Object(map)) => {
                    for (k, c) in map {
                        until.push(cond(k, c)?);
                    }
                }
                Some(_) => return Err("until: an object of conditions".into()),
                None => {}
            }
            let (frames, ticks) = (uint(&v, "frames"), uint(&v, "ticks"));
            if frames.is_none() && ticks.is_none() && until.is_empty() {
                return Err("wait: give frames, ticks or until".into());
            }
            Request::Wait(Wait { frames, ticks, until, timeout: uint(&v, "timeout").unwrap_or(WAIT_TIMEOUT) })
        }
        "state" => Request::State {
            units: v.get("units").and_then(Value::as_bool).unwrap_or(false),
            owner: uint(&v, "owner").map(|o| o as u8),
            visible_only: v.get("visible_only").and_then(Value::as_bool).unwrap_or(false),
            detail: v.get("detail").and_then(Value::as_bool).unwrap_or(false),
        },
        "command" => Request::Command(v.get("command").cloned().ok_or("command: missing \"command\"")?),
        "select" => Request::Select(
            v.get("units")
                .and_then(Value::as_array)
                .ok_or("select: missing \"units\"")?
                .iter()
                .filter_map(Value::as_u64)
                .map(|u| u as u32)
                .collect(),
        ),
        "screenshot" => Request::Screenshot {
            path: string(&v, "path").ok_or("screenshot: missing \"path\"")?,
            crop: v.get("crop").and_then(Value::as_array).and_then(|a| match a.as_slice() {
                [x, y, w, h] => Some([x.as_u64()? as u32, y.as_u64()? as u32, w.as_u64()? as u32, h.as_u64()? as u32]),
                _ => None,
            }),
        },
        "start" | "load_map" => {
            if let Some(map) = string(&v, "map") {
                Request::Start(Start::Map(map))
            } else {
                let r = v.get("random").cloned().unwrap_or(Value::Object(Default::default()));
                Request::Start(Start::Random {
                    nation: uint(&r, "nation").unwrap_or(0) as usize,
                    opponents: uint(&r, "opponents").unwrap_or(1) as usize,
                    seed: uint(&r, "seed").unwrap_or(1) as u32,
                    resources: uint(&r, "resources").unwrap_or(0) as usize,
                    size: uint(&r, "size").unwrap_or(0) as usize,
                    difficulty: uint(&r, "difficulty").unwrap_or(1) as usize,
                })
            }
        }
        "save" => Request::Save { name: string(&v, "name").ok_or("save: missing \"name\"")? },
        "load" => Request::Load { name: string(&v, "name").ok_or("load: missing \"name\"")? },
        "dialogs" => Request::Dialogs(
            string(&v, "policy").as_deref().and_then(DialogPolicy::parse).ok_or("dialogs: policy auto or report")?,
        ),
        "quit" => Request::Quit,
        other => return Err(format!("unknown cmd {other:?}")),
    };
    Ok((seq, request))
}

/// Lower case with runs of white space as one space, for label matching.
pub fn normalize(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// The best match for `text` among `labels`: exact (normalized) matches first, then
/// containing ones unless `exact`; `index` picks among them.
pub fn pick_label<'a>(
    labels: impl Iterator<Item = &'a str> + Clone,
    text: &str,
    exact: bool,
    index: usize,
) -> Option<usize> {
    let want = normalize(text);
    if want.is_empty() {
        return None;
    }
    let exact_hits: Vec<usize> =
        labels.clone().enumerate().filter(|(_, l)| normalize(l) == want).map(|(i, _)| i).collect();
    if !exact_hits.is_empty() {
        return exact_hits.get(index).copied();
    }
    if exact {
        return None;
    }
    labels.enumerate().filter(|(_, l)| normalize(l).contains(&want)).map(|(i, _)| i).nth(index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_clicks_by_label_id_and_position() {
        let (seq, r) = parse(r#"{"seq": 7, "cmd": "click", "label": "Einzelspieler"}"#).unwrap();
        assert_eq!(seq, Some(Value::from(7)));
        assert_eq!(
            r,
            Request::Click {
                target: Target::Label { text: "Einzelspieler".into(), exact: false, index: 0 },
                button: Button::Left,
                double: false,
                mods: Mods::default(),
                timeout: CLICK_TIMEOUT,
            }
        );
        let (_, r) = parse(r#"{"cmd": "rclick", "pos": [10, 20.5], "mods": ["Shift"]}"#).unwrap();
        let Request::Click { target, button, mods, .. } = r else { panic!() };
        assert_eq!((target, button, mods.shift), (Target::Pos([10.0, 20.5]), Button::Right, true));
        let (_, r) = parse(r#"{"cmd": "click", "id": "hud/3"}"#).unwrap();
        assert!(matches!(r, Request::Click { target: Target::Id(id), .. } if id == "hud/3"));
        let (_, r) = parse(r#"{"cmd": "drag", "from": [1, 2], "to": {"world": [300, 400]}}"#).unwrap();
        assert!(matches!(r, Request::Drag { from: Target::Pos(_), to: Target::World([300, 400]), .. }));
        let (_, r) = parse(r#"{"cmd": "drag", "around": {"owner": 0, "type": "Kri", "max": 4}}"#).unwrap();
        assert_eq!(
            r,
            Request::DragAround {
                filter: UnitFilter { owner: Some(0), kind: Some("Kri".into()) },
                max: 4,
                mods: Mods::default()
            }
        );
    }

    #[test]
    fn parses_waits_with_conditions() {
        let (_, r) =
            parse(r#"{"cmd": "wait", "until": {"screen": "RandomMap", "dialog": false}, "timeout": 50}"#).unwrap();
        let Request::Wait(w) = r else { panic!() };
        assert_eq!(w.timeout, 50);
        assert!(w.until.contains(&Cond::Screen("RandomMap".into())) && w.until.contains(&Cond::NoDialog));
        let (_, r) = parse(r#"{"cmd": "wait", "until": {"units": {"owner": 0, "type": "HOUSE", "min": 2}}}"#).unwrap();
        let Request::Wait(w) = r else { panic!() };
        assert_eq!(
            w.until,
            vec![Cond::Units { owner: Some(0), kind: Some("HOUSE".into()), building: None, finished: None, min: 2 }]
        );
        assert!(parse(r#"{"cmd": "wait"}"#).is_err());
        assert!(parse(r#"{"cmd": "wait", "until": {"nonsense": 1}}"#).is_err());
    }

    #[test]
    fn rejects_bad_lines() {
        assert!(parse("not json").unwrap_err().starts_with("bad JSON"));
        assert!(parse(r#"{"x": 1}"#).unwrap_err().contains("cmd"));
        assert!(parse(r#"{"cmd": "fly"}"#).unwrap_err().contains("unknown cmd"));
        assert!(parse(r#"{"cmd": "click"}"#).unwrap_err().contains("no target"));
    }

    #[test]
    fn starts_and_policies() {
        let (_, r) = parse(r#"{"cmd": "start", "random": {"nation": 2, "seed": 9}}"#).unwrap();
        assert_eq!(
            r,
            Request::Start(Start::Random { nation: 2, opponents: 1, seed: 9, resources: 0, size: 0, difficulty: 1 })
        );
        let (_, r) = parse(r#"{"cmd": "load_map", "map": "Missions\\a.m3d"}"#).unwrap();
        assert_eq!(r, Request::Start(Start::Map("Missions\\a.m3d".into())));
        let (_, r) = parse(r#"{"cmd": "dialogs", "policy": "report"}"#).unwrap();
        assert_eq!(r, Request::Dialogs(DialogPolicy::Report));
    }

    #[test]
    fn labels_match_exactly_first_then_by_containing() {
        let labels = ["Einzelspieler", "Mehrspieler", "Spiel laden", "  Optionen "];
        let pick = |t: &str, exact| pick_label(labels.iter().copied(), t, exact, 0);
        assert_eq!(pick("einzelspieler", false), Some(0));
        assert_eq!(pick("optionen", true), Some(3));
        assert_eq!(pick("spieler", false), Some(0));
        assert_eq!(pick_label(labels.iter().copied(), "spieler", false, 1), Some(1));
        assert_eq!(pick("spieler", true), None);
        assert_eq!(pick("", false), None);
    }
}
