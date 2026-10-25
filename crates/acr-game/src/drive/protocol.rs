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
    /// `hit_locations` turns on the optional hit-location rule (#225).
    Random {
        nation: usize,
        opponents: usize,
        seed: u32,
        resources: usize,
        size: usize,
        difficulty: usize,
        hit_locations: bool,
        /// The optional body-spacing rule (`docs/body-spacing.md`).
        body_spacing: bool,
        /// The optional mortars rule (`ArtMrt` types, `docs/simulation.md` "Mortar").
        mortars: bool,
        /// The optional corpse-piles rule (#222, `docs/simulation.md` "Corpse piles").
        corpse_piles: bool,
        /// The optional group-move rule (`docs/simulation.md` "Group moves").
        group_move: bool,
        /// The optional cannon-fire rule (round shot sets men on fire under hit locations).
        cannon_fire: bool,
        /// The optional ball-terrain rule (balls break trees and knock stones under hit locations).
        ball_terrain: bool,
    },
    /// The blank test field ("The blank field" in `docs/testing.md`): `players` (1 or 2)
    /// without start units and without AI, `nation` for the local player.
    /// `hud: false` hides the interface (see [`Request::Hud`]).
    Blank {
        setup: crate::ui::setup::BlankSetup,
        players: usize,
        nation: usize,
        hit_locations: bool,
        body_spacing: bool,
        mortars: bool,
        corpse_piles: bool,
        group_move: bool,
        /// The optional cannon-fire rule (round shot sets men on fire under hit locations).
        cannon_fire: bool,
        /// The optional ball-terrain rule (balls break trees and knock stones under hit locations).
        ball_terrain: bool,
        hud: bool,
        /// `"corpse_decay": "half"|"full"` overrides the setting (`corpse_decay`).
        corpse_decay_half: Option<bool>,
        /// `"decay_stain": "on"|"off"` overrides the setting (`decay_stain`).
        decay_stain: Option<bool>,
        /// `"ground_cover": true|false` (or `"on"|"off"`) overrides the setting
        /// (`ground_cover`); unset, a `white` or `greenscreen` ground has none.
        ground_cover: Option<bool>,
    },
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
        /// Only list objects whose type id contains this.
        kind: Option<String>,
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
    /// The camera centred on a world pixel, zoomed, and with `yaw`/`pitch` the free camera.
    Camera {
        x: i32,
        y: i32,
        zoom: Option<f32>,
        /// True pixels above the ground point that come to the middle (default 0.9 m).
        lift: f32,
        /// Radians: turn and elevation of the free camera.
        orbit: Option<[f32; 2]>,
    },
    /// Sun shadows forced on or off over the setting (`None`: the setting again).
    Shadows(Option<bool>),
    /// `{"cmd": "burn", "unit": 3, "amount": 1.0}`: the charring a unit's model shows
    /// (render only, #207), as if he had burned; 0 forgets it.
    Burn {
        unit: u32,
        amount: f32,
    },
    /// Normal maps and the glTF highlight (#234) forced on or off over the `material_detail`
    /// setting (`None`: the setting again).
    MaterialDetail(Option<bool>),
    /// `{"cmd": "weather", "mode": "rain"}`: off, rain, snow or fog over the `weather`
    /// setting; without `mode` back to the setting.
    Weather(Option<acr_render::weather::Weather>),
    /// The fog of war on or off (off: the whole map revealed, as on the blank field).
    Fog(bool),
    /// The map's trees and stones as models switched on or off over `map_models`.
    MapModels(bool),
    /// Trees or foliage drawn at ground points (render only): `model` (a `.glb` name),
    /// `at` (`[[x, y], ...]` world pixels), `wind` (bool over the setting).
    Props {
        model: String,
        at: Vec<[i32; 2]>,
        wind: Option<bool>,
    },
    /// A unit placed for the bench (`drive::bench::spawn`).
    Spawn(Value),
    /// A scripted hit (`drive::bench::hit`).
    Hit(Value),
    /// A scripted round striking the ground or water, hitting no one (`drive::bench::impact`).
    Impact(Value),
    /// Trees or stones put on the map as simulated map objects (`drive::bench::map_object`).
    MapObject(Value),
    /// `{"cmd":"gputime","frames":n}`: GPU milliseconds per pass, from timestamp queries.
    GpuTime(u32),
    /// `{"cmd":"frametimes","frames":n}`: wall milliseconds of `n` frames drawn back to back
    /// (each waited for on the GPU): mean and max, and the model textures' memory after.
    FrameTimes(u32),
    /// `{"cmd":"textures"}`: GPU memory of the model textures (loaded, streamed) and the
    /// streaming counts (docs/rendering-3d.md "Texture streaming").
    Textures,
    /// `{"cmd":"fps","fps":n}`: each driver frame advances 1/n s of game time (several frames
    /// per tick: the 3D view's per-frame motion, as in play at that frame rate); 0 goes back
    /// to one tick per frame.
    Fps(f64),
    /// `{"cmd":"profile"}`: CPU milliseconds per profiler scope (`acr_sim::prof`) since
    /// the last call, which also switches the profiler on.
    Profile,
    /// `{"cmd":"perf","frames":300,"recorder":true,"snapshot":true}`: plays `frames` whole
    /// frames (advance, an offscreen draw, the GPU waited for) through the performance
    /// recorder (`recorder: false` switches it off, for its overhead), returns their wall
    /// times and, with `snapshot`, writes one (docs/testing.md "Performance recorder").
    Perf {
        frames: u32,
        recorder: bool,
        snapshot: bool,
    },
    /// `count` screenshots, one every `every` frames, into `dir` as `<prefix>NNN.png`.
    Record {
        dir: String,
        prefix: String,
        count: u32,
        every: u64,
        crop: Option<[u32; 4]>,
    },
    /// Shows or hides the interface (panel, minimap, cursor) over the game.
    Hud(bool),
    /// The studio (#299, `drive::studio`): `studio`, `pose`, `camera` with `frame`, `sheet`,
    /// `measure`; `cmd` is the command's name.
    Studio {
        cmd: String,
        v: Value,
    },
    Quit,
}

fn string(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn uint(v: &Value, key: &str) -> Option<u64> {
    v.get(key).and_then(Value::as_u64)
}

/// The `blank` of a `start`: size, ground, slope, hill, water (see [`Start::Blank`]).
pub(super) fn blank(b: &Value) -> Result<Start, String> {
    use crate::ui::setup::{BlankSetup, Ground, MapSize};
    let size = match b.get("size").and_then(Value::as_str).unwrap_or("normal") {
        "normal" => MapSize::Normal,
        "large" => MapSize::Large,
        "huge" => MapSize::Huge,
        other => return Err(format!("blank: unknown size {other:?}: normal, large or huge")),
    };
    let ground = match b.get("ground").and_then(Value::as_str).unwrap_or("grass") {
        "grass" => Ground::Grass,
        "dirt" => Ground::Dirt,
        "snow" => Ground::Snow,
        "white" => Ground::White,
        "greenscreen" | "green" => Ground::Greenscreen,
        other => {
            return Err(format!("blank: unknown ground {other:?}: grass, dirt, snow, white or greenscreen"));
        }
    };
    let c = crate::blank_map::playable(size) / 2;
    let num = |v: &Value, key: &str, default: i32| v.get(key).and_then(Value::as_f64).map_or(default, |f| f as i32);
    // A number is the ramp's angle in degrees; an object may also give where it runs.
    let slope = match b.get("slope") {
        None | Some(Value::Null) => None,
        Some(Value::Number(n)) => Some(((n.as_f64().unwrap_or(0.0) * 10.0).round() as i32, c - 750, c + 750)),
        Some(o @ Value::Object(_)) => {
            let deg = o.get("deg").and_then(Value::as_f64).ok_or("blank: slope needs \"deg\"")?;
            Some(((deg * 10.0).round() as i32, num(o, "x0", c - 750), num(o, "x1", c + 750)))
        }
        Some(_) => return Err("blank: slope is degrees or {\"deg\", \"x0\", \"x1\"}".into()),
    };
    let hill = match b.get("hill") {
        None | Some(Value::Null | Value::Bool(false)) => None,
        Some(Value::Bool(true)) => Some((c, c, 600, 100)),
        Some(o @ Value::Object(_)) => {
            Some((num(o, "x", c), num(o, "y", c), num(o, "radius", 600), num(o, "height", 100)))
        }
        Some(_) => return Err("blank: hill is true or {\"x\", \"y\", \"radius\", \"height\"}".into()),
    };
    let water = match b.get("water") {
        None | Some(Value::Null | Value::Bool(false)) => None,
        Some(Value::Bool(true)) => Some((c + 1000, c - 600, 2000, 1200)),
        Some(o @ Value::Object(_)) => {
            Some((num(o, "x", c + 1000), num(o, "y", c - 600), num(o, "w", 2000), num(o, "h", 1200)))
        }
        Some(_) => return Err("blank: water is true or {\"x\", \"y\", \"w\", \"h\"}".into()),
    };
    // Patches of other textures: [{"x", "y", "radius", "texture"}, ...].
    let patches = b
        .get("patches")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .map(|o| {
                    (num(o, "x", c), num(o, "y", c), num(o, "radius", 200), num(o, "texture", 0).clamp(0, 255) as u8)
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(Start::Blank {
        setup: BlankSetup {
            size,
            ground,
            slope,
            hill,
            water,
            patches,
            rough: num(b, "rough", 0).clamp(0, 100),
            editor: b.get("editor").and_then(Value::as_bool).unwrap_or(false),
        },
        players: uint(b, "players").unwrap_or(2).clamp(1, 2) as usize,
        nation: uint(b, "nation").unwrap_or(0) as usize,
        hit_locations: b.get("hit_locations").and_then(Value::as_bool).unwrap_or(false),
        body_spacing: b.get("body_spacing").and_then(Value::as_bool).unwrap_or(false),
        mortars: b.get("mortars").and_then(Value::as_bool).unwrap_or(false),
        corpse_piles: b.get("corpse_piles").and_then(Value::as_bool).unwrap_or(false),
        group_move: b.get("group_move").and_then(Value::as_bool).unwrap_or(false),
        cannon_fire: b.get("cannon_fire").and_then(Value::as_bool).unwrap_or(false),
        ball_terrain: b.get("ball_terrain").and_then(Value::as_bool).unwrap_or(false),
        hud: b.get("hud").and_then(Value::as_bool).unwrap_or(true),
        corpse_decay_half: b.get("corpse_decay").and_then(Value::as_str).map(|v| v == "half"),
        decay_stain: b.get("decay_stain").and_then(Value::as_str).map(|v| v == "on"),
        ground_cover: match b.get("ground_cover") {
            None | Some(Value::Null) => None,
            Some(Value::Bool(on)) => Some(*on),
            Some(Value::String(v)) if v == "on" || v == "off" => Some(v == "on"),
            Some(_) => return Err("blank: ground_cover is true, false, \"on\" or \"off\"".into()),
        },
    })
}

fn point(v: &Value) -> Option<[f64; 2]> {
    let a = v.as_array()?;
    match a.as_slice() {
        [x, y] => Some([x.as_f64()?, y.as_f64()?]),
        _ => None,
    }
}

fn crop(v: &Value) -> Option<[u32; 4]> {
    v.get("crop").and_then(Value::as_array).and_then(|a| match a.as_slice() {
        [x, y, w, h] => Some([x.as_u64()? as u32, y.as_u64()? as u32, w.as_u64()? as u32, h.as_u64()? as u32]),
        _ => None,
    })
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
            kind: v.get("type").and_then(Value::as_str).map(str::to_owned),
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
        "screenshot" => {
            Request::Screenshot { path: string(&v, "path").ok_or("screenshot: missing \"path\"")?, crop: crop(&v) }
        }
        "start" | "load_map" => {
            if let Some(map) = string(&v, "map") {
                Request::Start(Start::Map(map))
            } else if let Some(b) = v.get("blank") {
                Request::Start(blank(b)?)
            } else {
                let r = v.get("random").cloned().unwrap_or(Value::Object(Default::default()));
                Request::Start(Start::Random {
                    nation: uint(&r, "nation").unwrap_or(0) as usize,
                    opponents: uint(&r, "opponents").unwrap_or(1) as usize,
                    seed: uint(&r, "seed").unwrap_or(1) as u32,
                    resources: uint(&r, "resources").unwrap_or(0) as usize,
                    size: uint(&r, "size").unwrap_or(0) as usize,
                    difficulty: uint(&r, "difficulty").unwrap_or(1) as usize,
                    hit_locations: r.get("hit_locations").and_then(Value::as_bool).unwrap_or(false),
                    body_spacing: r.get("body_spacing").and_then(Value::as_bool).unwrap_or(false),
                    mortars: r.get("mortars").and_then(Value::as_bool).unwrap_or(false),
                    corpse_piles: r.get("corpse_piles").and_then(Value::as_bool).unwrap_or(false),
                    group_move: r.get("group_move").and_then(Value::as_bool).unwrap_or(false),
                    cannon_fire: r.get("cannon_fire").and_then(Value::as_bool).unwrap_or(false),
                    ball_terrain: r.get("ball_terrain").and_then(Value::as_bool).unwrap_or(false),
                })
            }
        }
        "hud" => Request::Hud(v.get("on").and_then(Value::as_bool).ok_or("hud: missing \"on\" (true or false)")?),
        "save" => Request::Save { name: string(&v, "name").ok_or("save: missing \"name\"")? },
        "load" => Request::Load { name: string(&v, "name").ok_or("load: missing \"name\"")? },
        "dialogs" => Request::Dialogs(
            string(&v, "policy").as_deref().and_then(DialogPolicy::parse).ok_or("dialogs: policy auto or report")?,
        ),
        "camera" if v.get("frame").is_some() => Request::Studio { cmd: "camera".into(), v: v.clone() },
        "studio" | "pose" | "sheet" | "measure" => Request::Studio { cmd: cmd.to_owned(), v: v.clone() },
        "camera" => {
            let num = |k: &str| v.get(k).and_then(Value::as_f64);
            let (x, y) = match v.get("world").and_then(point) {
                Some([x, y]) => (x, y),
                None => (num("x").ok_or("camera: missing \"x\"")?, num("y").ok_or("camera: missing \"y\"")?),
            };
            let deg = |k: &str| num(k).or_else(|| num(&format!("{k}_deg")).map(f64::to_radians));
            let orbit = match (deg("yaw"), deg("pitch")) {
                (None, None) if v.get("free").and_then(Value::as_bool) != Some(true) => None,
                (yaw, pitch) => Some([yaw.unwrap_or(0.0) as f32, pitch.unwrap_or(0.52) as f32]),
            };
            let lift = num("lift").unwrap_or(27.0) as f32;
            Request::Camera { x: x as i32, y: y as i32, zoom: num("zoom").map(|z| z as f32), lift, orbit }
        }
        "shadows" => Request::Shadows(v.get("on").and_then(Value::as_bool)),
        "burn" => Request::Burn {
            unit: uint(&v, "unit").ok_or("burn: missing \"unit\"")? as u32,
            amount: v.get("amount").and_then(Value::as_f64).unwrap_or(1.0) as f32,
        },
        "material_detail" => Request::MaterialDetail(v.get("on").and_then(Value::as_bool)),
        "weather" => Request::Weather(match string(&v, "mode") {
            Some(m) => Some(acr_render::weather::Weather::parse(&m).ok_or(format!("weather: unknown mode {m:?}"))?),
            None => None,
        }),
        "fog" => Request::Fog(v.get("on").and_then(Value::as_bool).unwrap_or(true)),
        "map_models" => Request::MapModels(v.get("on").and_then(Value::as_bool).unwrap_or(true)),
        "props" => Request::Props {
            model: string(&v, "model").ok_or("props: missing \"model\"")?,
            at: v
                .get("at")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter().filter_map(|p| Some([p.get(0)?.as_i64()? as i32, p.get(1)?.as_i64()? as i32])).collect()
                })
                .unwrap_or_default(),
            wind: v.get("wind").and_then(Value::as_bool),
        },
        "spawn" => Request::Spawn(v.clone()),
        "hit" => Request::Hit(v.clone()),
        "impact" => Request::Impact(v.clone()),
        "map_object" => Request::MapObject(v.clone()),
        "profile" => Request::Profile,
        "perf" => Request::Perf {
            frames: uint(&v, "frames").unwrap_or(0).min(100_000) as u32,
            recorder: v.get("recorder").and_then(Value::as_bool).unwrap_or(true),
            snapshot: v.get("snapshot").and_then(Value::as_bool).unwrap_or(false),
        },
        "fps" => Request::Fps(v.get("fps").and_then(Value::as_f64).unwrap_or(0.0).clamp(0.0, 1000.0)),
        "gputime" => Request::GpuTime(uint(&v, "frames").unwrap_or(30).clamp(1, 1000) as u32),
        "frametimes" => Request::FrameTimes(uint(&v, "frames").unwrap_or(60).clamp(1, 10000) as u32),
        "textures" => Request::Textures,
        "record" => Request::Record {
            dir: string(&v, "dir").ok_or("record: missing \"dir\"")?,
            prefix: string(&v, "prefix").unwrap_or_else(|| "frame".into()),
            count: uint(&v, "count").or_else(|| uint(&v, "frames")).unwrap_or(20).max(1) as u32,
            every: uint(&v, "every").unwrap_or(1).max(1),
            crop: crop(&v),
        },
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
    fn parses_bench_commands() {
        let (_, r) =
            parse(r#"{"cmd": "camera", "x": 3000, "y": 4000, "zoom": 5, "yaw_deg": 90, "pitch": 0.2}"#).unwrap();
        let Request::Camera { x: 3000, y: 4000, zoom: Some(5.0), lift: 27.0, orbit: Some([yaw, pitch]) } = r else {
            panic!("{r:?}")
        };
        assert!((yaw - std::f32::consts::FRAC_PI_2).abs() < 1e-5 && (pitch - 0.2).abs() < 1e-6);
        let (_, r) = parse(r#"{"cmd": "camera", "world": [10, 20]}"#).unwrap();
        assert_eq!(r, Request::Camera { x: 10, y: 20, zoom: None, lift: 27.0, orbit: None });
        let (_, r) = parse(r#"{"cmd": "record", "dir": "out", "count": 3, "every": 4}"#).unwrap();
        assert_eq!(r, Request::Record { dir: "out".into(), prefix: "frame".into(), count: 3, every: 4, crop: None });
        assert!(matches!(parse(r#"{"cmd": "hit", "unit": 5}"#).unwrap().1, Request::Hit(_)));
        assert!(matches!(parse(r#"{"cmd": "impact", "at": [900, 900]}"#).unwrap().1, Request::Impact(_)));
        assert!(parse(r#"{"cmd": "camera", "x": 1}"#).is_err());
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
            Request::Start(Start::Random {
                nation: 2,
                opponents: 1,
                seed: 9,
                resources: 0,
                size: 0,
                difficulty: 1,
                hit_locations: false,
                body_spacing: false,
                mortars: false,
                corpse_piles: false,
                group_move: false,
                cannon_fire: false,
                ball_terrain: false,
            })
        );
        let (_, r) = parse(r#"{"cmd": "load_map", "map": "Missions\\a.m3d"}"#).unwrap();
        assert_eq!(r, Request::Start(Start::Map("Missions\\a.m3d".into())));
        let (_, r) = parse(r#"{"cmd": "start", "blank": {"ground": "snow", "slope": 4, "water": true}}"#).unwrap();
        let Request::Start(Start::Blank {
            setup,
            players: 2,
            nation: 0,
            hit_locations: false,
            body_spacing: false,
            mortars: false,
            corpse_piles: false,
            group_move: false,
            cannon_fire: false,
            ball_terrain: false,
            hud: true,
            corpse_decay_half: None,
            decay_stain: None,
            ground_cover: None,
        }) = r
        else {
            panic!()
        };
        assert_eq!(setup.ground, crate::ui::setup::Ground::Snow);
        assert_eq!(setup.slope, Some((40, 7680 - 750, 7680 + 750)));
        assert_eq!(setup.water, Some((8680, 7080, 2000, 1200)));
        assert_eq!(setup.hill, None);
        let (_, r) = parse(r#"{"cmd": "start", "blank": {"size": "large", "hill": {"height": 50}}}"#).unwrap();
        let Request::Start(Start::Blank { setup, .. }) = r else { panic!() };
        assert_eq!(setup.hill, Some((15360, 15360, 600, 50)));
        let (_, r) = parse(r#"{"cmd": "start", "blank": {"ground": "white", "ground_cover": "on"}}"#).unwrap();
        let Request::Start(Start::Blank { ground_cover, .. }) = r else { panic!() };
        assert_eq!(ground_cover, Some(true));
        assert!(parse(r#"{"cmd": "start", "blank": {"ground": "lava"}}"#).is_err());
        let (_, r) = parse(r#"{"cmd": "start", "blank": {"ground": "greenscreen", "hud": false}}"#).unwrap();
        let Request::Start(Start::Blank { setup, hud: false, .. }) = r else { panic!() };
        assert_eq!(setup.ground, crate::ui::setup::Ground::Greenscreen);
        assert_eq!(parse(r#"{"cmd": "hud", "on": true}"#).unwrap().1, Request::Hud(true));
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
