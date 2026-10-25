//! The studio (#299, `docs/testing.md`): exact, repeatable shots and measurements of posed
//! models. `studio` sets up a held blank field with the listed units, `pose` holds a unit's
//! model at a clip time, `camera` with `frame` fits units or a body part into the window
//! under the free camera, `sheet` renders a labelled grid of units × clip times × views into
//! one PNG, and `measure` returns hand, grip and foot numbers. All of it is render-side:
//! the simulation only stands still (`InGame::studio_hold`), its state is never changed by
//! a pose.

use acr_render::math::{self, Mat4, Vec3};
use acr_render::model::{ModelData, Slot};
use acr_render::terrain::Camera;
use acr_render::view3d::{self, Orbit, PIXELS_PER_METRE, View3d};
use serde_json::{Value, json};

use super::shot;
use crate::{Game, Mode};

/// Frames a studio command waits at most for its models to load.
const MODEL_TRIES: u32 = 40;
/// Height of a sheet cell's label strip (pixels).
const LABEL: u32 = 20;

/// A body part to frame (`part`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    Hands,
    Head,
    Feet,
    Weapon,
}

impl Part {
    pub fn parse(s: &str) -> Result<Self, String> {
        Ok(match s {
            "hands" | "hand" => Self::Hands,
            "head" => Self::Head,
            "feet" | "foot" => Self::Feet,
            "weapon" | "item" => Self::Weapon,
            other => return Err(format!("unknown part {other:?}: hands, head, feet or weapon")),
        })
    }
}

/// The camera that frames a set of world points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Framing {
    /// The ground point (z = 0) at the middle of the view.
    pub target: [f32; 2],
    /// Output pixels per world pixel.
    pub scale: f32,
}

/// The free camera's turn and tilt for a named view of a unit facing `dir` (`RealDir`):
/// `front` looks at his face, `back` at his back, `side` at his right side, `three_quarter`
/// at his front right, `top` straight down from above his front.
pub fn view_orbit(view: &str, dir: u8) -> Result<Orbit, String> {
    use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};
    // The camera looks along (sin yaw, −cos yaw); the unit faces (sin f, cos f), f its yaw.
    let face = -view3d::facing_yaw(dir);
    let (turn, pitch) = match view {
        "front" => (0.0, 0.12),
        "back" => (PI, 0.12),
        "side" => (FRAC_PI_2, 0.12),
        "three_quarter" | "3/4" => (FRAC_PI_4, 0.35),
        "top" => (0.0, Orbit::MAX_PITCH),
        other => return Err(format!("unknown view {other:?}: front, back, side, three_quarter or top")),
    };
    Ok(Orbit { yaw: (face + turn).rem_euclid(std::f32::consts::TAU), pitch })
}

/// The camera under `orbit` that shows all `points` (world pixels) in a view of `size`
/// output pixels, with `margin` (a fraction of the view on each side) left free.
pub fn frame(points: &[Vec3], orbit: Orbit, size: [f32; 2], margin: f32) -> Option<Framing> {
    if points.is_empty() {
        return None;
    }
    let view = View3d { camera: Camera { x: 0.0, y: 0.0, scale: 1.0 }, size, orbit: Some(orbit) };
    let b = view.basis();
    let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
    for &p in points {
        let q = [math::dot(p, b.right), math::dot(p, b.down)];
        for i in 0..2 {
            lo[i] = lo[i].min(q[i]);
            hi[i] = hi[i].max(q[i]);
        }
    }
    let free = (1.0 - 2.0 * margin.clamp(0.0, 0.45)).max(0.1);
    let extent = [(hi[0] - lo[0]).max(1.0), (hi[1] - lo[1]).max(1.0)];
    let scale = (size[0] * free / extent[0]).min(size[1] * free / extent[1]);
    let centre = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0];
    // The ground point whose screen coordinates are `centre`: along `right` by its x, and
    // along the view's horizontal direction so that `down` (−sin pitch along it) gives its y.
    let ahead = [b.forward[0], b.forward[1]];
    let len = (ahead[0] * ahead[0] + ahead[1] * ahead[1]).sqrt().max(1e-6);
    let along = -centre[1] / orbit.pitch.sin().max(1e-3);
    let target = [centre[0] * b.right[0] + along * ahead[0] / len, centre[0] * b.right[1] + along * ahead[1] / len];
    Some(Framing { target, scale })
}

/// Whether node `n` belongs to `part` (or with `None`, to the whole model).
fn in_part(model: &ModelData, n: usize, part: Option<Part>) -> bool {
    let Some(part) = part else { return true };
    let mut k = Some(n);
    let mut via_socket = false;
    while let Some(i) = k {
        let name = model.nodes[i].name.as_str();
        let base = name.strip_prefix("rider.").unwrap_or(name);
        let hit = match part {
            Part::Hands => (base == "hand_l" || base == "hand_r") && !via_socket,
            Part::Weapon => {
                base.starts_with("socket.hand_") || base.starts_with("socket.club") || base.starts_with("socket.blade")
            }
            Part::Head => base == "head",
            Part::Feet => base.starts_with("foot_") || base.starts_with("toe_"),
        };
        if hit {
            return true;
        }
        via_socket |= base.starts_with("socket.");
        k = model.nodes[i].parent;
    }
    false
}

/// The skinned vertices of `model` at `clip`/`time` placed by `transform` (world pixels),
/// those of `part` only (by each vertex's strongest joint).
pub fn points(model: &ModelData, transform: &Mat4, clip: Option<usize>, time: f32, part: Option<Part>) -> Vec<Vec3> {
    let palette = model.pose(clip, time);
    let node = |j: u32| match model.slots.get(j as usize) {
        Some(Slot::Node(n) | Slot::Joint { node: n, .. }) => Some(*n),
        None => None,
    };
    let keep: Vec<bool> =
        (0..model.slots.len()).map(|j| node(j as u32).is_some_and(|n| in_part(model, n, part))).collect();
    let mut out = Vec::new();
    for p in &model.primitives {
        for v in &p.vertices {
            let strongest = (0..4).max_by(|&a, &b| v.weights[a].total_cmp(&v.weights[b])).unwrap_or(0);
            if !keep.get(v.joints[strongest] as usize).copied().unwrap_or(false) {
                continue;
            }
            let total: f32 = v.weights.iter().sum();
            let mut q = [0.0; 3];
            for k in 0..4 {
                let w = if total > 0.0 {
                    v.weights[k] / total
                } else if k == 0 {
                    1.0
                } else {
                    0.0
                };
                if w == 0.0 {
                    continue;
                }
                let Some(rows) = palette.get(v.joints[k] as usize) else { continue };
                for (i, r) in rows.iter().enumerate() {
                    q[i] += w * (r[0] * v.position[0] + r[1] * v.position[1] + r[2] * v.position[2] + r[3]);
                }
            }
            out.push(math::transform_point(transform, q));
        }
    }
    out
}

/// What a studio command still has to do over the next frames.
pub enum Task {
    /// Answered after one more frame (a pose or a camera shows then).
    Done,
    /// `studio`: spawn the units once the field is up, then wait for their models.
    Populate {
        v: Value,
        ids: Vec<u32>,
        tries: u32,
    },
    /// `pose`, `camera` with `frame`, `measure` without `ticks`: once the models are drawn.
    Wait {
        cmd: String,
        v: Value,
        tries: u32,
    },
    Sheet(Box<Sheet>),
    /// `measure` with `ticks`: feet sampled every tick.
    Walk {
        id: u32,
        left: u32,
        samples: Vec<Value>,
        feet: Vec<[Vec3; 4]>,
        hold: bool,
    },
}

pub enum Next {
    Done,
    Frame(Task),
    /// Draw once (asks for missing files) and settle.
    Prime(Task),
}

fn ingame(game: &mut Game) -> Result<&mut crate::ingame::InGame, String> {
    match &mut game.mode {
        Mode::InGame(g) => Ok(g),
        _ => Err("no game".into()),
    }
}

/// The unit ids of `v[key]`: a number or a list.
fn ids(v: &Value, key: &str) -> Vec<u32> {
    match v.get(key) {
        Some(Value::Array(a)) => a.iter().filter_map(Value::as_u64).map(|n| n as u32).collect(),
        Some(n) => n.as_u64().map(|n| vec![n as u32]).unwrap_or_default(),
        None => Vec::new(),
    }
}

/// Whether all `ids` are drawn as models.
fn drawn(game: &mut Game, ids: &[u32]) -> Result<bool, String> {
    let g = ingame(game)?;
    Ok(ids.iter().all(|&id| g.posed(id).is_some()))
}

/// Carries a studio task one step further.
pub fn step(game: &mut Game, task: Task, reply: &mut serde_json::Map<String, Value>) -> Result<Next, String> {
    match task {
        Task::Done => Ok(Next::Done),
        Task::Populate { v, mut ids, tries } => {
            if ids.is_empty() {
                let units = v.get("units").and_then(Value::as_array).cloned().unwrap_or_default();
                let mut out = Vec::new();
                for (i, u) in units.iter().enumerate() {
                    let mut u = u.clone();
                    let o = u.as_object_mut().ok_or("studio: each unit is an object")?;
                    o.entry("x").or_insert(json!(7680 + 90 * i as i64));
                    o.entry("y").or_insert(json!(7680));
                    o.entry("owner").or_insert(json!(0));
                    let spawned = super::bench::spawn(game, &u)?;
                    ids.push(spawned["id"].as_u64().unwrap_or(0) as u32);
                    out.push(spawned);
                }
                if ids.is_empty() {
                    return Err("studio: no \"units\"".into());
                }
                reply.insert("units".into(), out.into());
                let run = v.get("run").and_then(Value::as_bool).unwrap_or(false);
                ingame(game)?.studio_hold = !run;
            }
            if !drawn(game, &ids)? {
                if tries >= MODEL_TRIES {
                    return Err(format!("studio: models of {ids:?} not drawn (no model, or not loaded)"));
                }
                return Ok(Next::Prime(Task::Populate { v, ids, tries: tries + 1 }));
            }
            let framing = json!({"frame": ids, "view": v.get("view").cloned().unwrap_or(json!("three_quarter"))});
            reply.insert("camera".into(), camera(game, &framing, None)?);
            Ok(Next::Frame(Task::Done))
        }
        Task::Wait { cmd, v, tries } => {
            let want = if cmd == "pose" || cmd == "measure" { ids(&v, "id") } else { ids(&v, "frame") };
            if want.is_empty() {
                return Err(format!("{cmd}: missing unit id(s)"));
            }
            if !drawn(game, &want)? {
                if tries >= MODEL_TRIES {
                    return Err(format!("{cmd}: unit(s) {want:?} not drawn as models"));
                }
                return Ok(Next::Prime(Task::Wait { cmd, v, tries: tries + 1 }));
            }
            match cmd.as_str() {
                "pose" => {
                    reply.insert("pose".into(), pose(game, &v)?);
                    Ok(Next::Frame(Task::Done))
                }
                "camera" => {
                    reply.insert("camera".into(), camera(game, &v, None)?);
                    Ok(Next::Frame(Task::Done))
                }
                _ => {
                    let id = want[0];
                    let ticks = v.get("ticks").and_then(Value::as_u64).unwrap_or(0) as u32;
                    reply.insert("measure".into(), hands(game, id)?);
                    if ticks == 0 {
                        return Ok(Next::Done);
                    }
                    let g = ingame(game)?;
                    let hold = g.studio_hold;
                    g.studio_hold = false;
                    g.hold_pose(id, None);
                    Ok(Next::Frame(Task::Walk { id, left: ticks, samples: Vec::new(), feet: Vec::new(), hold }))
                }
            }
        }
        Task::Walk { id, left, mut samples, mut feet, hold } => {
            let (sample, at) = foot_sample(game, id)?;
            samples.push(sample);
            feet.push(at);
            if left > 1 {
                return Ok(Next::Frame(Task::Walk { id, left: left - 1, samples, feet, hold }));
            }
            ingame(game)?.studio_hold = hold;
            let m = reply.entry("measure").or_insert(json!({}));
            let pair = |k: usize| feet.iter().map(|f| [f[k], f[k + 1]]).collect::<Vec<_>>();
            m["walk"] = json!({"samples": samples, "sliding": sliding(&pair(0), ["foot_l", "foot_r"]),
                               "hands_sliding": sliding(&pair(2), ["hand_l", "hand_r"])});
            Ok(Next::Done)
        }
        Task::Sheet(sheet) => sheet_step(game, sheet, reply),
    }
}

/// The clip index and time of `v`: `clip` (name; default the shown clip), and `t`
/// (seconds), `frame` (clip frames at the model's fps) or `phase` (0..1 of the clip).
fn clip_time(model: &ModelData, shown: Option<usize>, v: &Value) -> Result<(usize, f32), String> {
    let clip = match v.get("clip").and_then(Value::as_str) {
        Some(name) => model.clip(name).ok_or_else(|| {
            let names: Vec<&str> = model.clips.iter().map(|c| c.name.as_str()).collect();
            format!("no clip {name:?}; the model has {names:?}")
        })?,
        None => shown.ok_or("pose: the model has no clips")?,
    };
    let duration = model.clips[clip].duration;
    let num = |k: &str| v.get(k).and_then(Value::as_f64).map(|x| x as f32);
    let time = if let Some(t) = num("t") {
        t
    } else if let Some(f) = num("frame") {
        f / model.fps
    } else if let Some(p) = num("phase") {
        p * duration
    } else {
        0.0
    };
    Ok((clip, time.clamp(0.0, duration.max(0.0))))
}

/// `pose`: holds the unit's model at the clip time; `release: true` lets it go again.
fn pose(game: &mut Game, v: &Value) -> Result<Value, String> {
    let id = ids(v, "id")[0];
    let g = ingame(game)?;
    if v.get("release").and_then(Value::as_bool) == Some(true) {
        g.hold_pose(id, None);
        return Ok(json!({"id": id, "released": true}));
    }
    let (model, inst) = g.posed(id).ok_or("pose: not drawn")?;
    let (clip, time) = clip_time(model, inst.clip, v)?;
    let (name, duration, fps) = (model.clips[clip].name.clone(), model.clips[clip].duration, model.fps);
    g.hold_pose(id, Some((clip, time)));
    g.studio_hold = !v.get("run").and_then(Value::as_bool).unwrap_or(false);
    Ok(json!({"id": id, "clip": name, "t": time, "frame": time * fps, "duration": duration, "fps": fps}))
}

/// `camera` with `frame`: fits the units (or their `part`) into the view (or `size`).
fn camera(game: &mut Game, v: &Value, size: Option<[f32; 2]>) -> Result<Value, String> {
    let units = ids(v, "frame");
    let part = v.get("part").and_then(Value::as_str).map(Part::parse).transpose()?;
    let margin = v.get("margin").and_then(Value::as_f64).map_or(if part.is_some() { 0.15 } else { 0.06 }, |m| m as f32);
    let zoom = v.get("zoom").and_then(Value::as_f64).unwrap_or(1.0) as f32;
    let view = v.get("view").and_then(Value::as_str).unwrap_or("three_quarter");
    let g = ingame(game)?;
    let size = size.unwrap_or(g.window_size_px());
    let dir = units.first().and_then(|&id| g.world()?.unit(acr_sim::world::EntityId(id))).map_or(64, |u| u.dir);
    let mut orbit = view_orbit(view, dir)?;
    let deg = |k: &str| v.get(k).and_then(Value::as_f64).map(|d| (d as f32).to_radians());
    if let Some(y) = deg("yaw_deg") {
        orbit.yaw = y;
    }
    if let Some(p) = deg("pitch_deg") {
        orbit.pitch = p.clamp(0.05, Orbit::MAX_PITCH);
    }
    let mut pts = Vec::new();
    for &id in &units {
        let (model, inst) = g.posed(id).ok_or_else(|| format!("camera: unit {id} is not drawn as a model"))?;
        pts.extend(points(model, &inst.transform, inst.clip, inst.time, part));
    }
    // A held pose shows from the next frame on: frame that pose, not the last one drawn.
    if let [id] = units[..]
        && let Some((model, inst)) = g.posed(id)
        && let Some(&(clip, time)) = g.held_pose(id)
    {
        pts = points(model, &inst.transform, Some(clip), time, part);
    }
    let f = frame(&pts, orbit, size, margin).ok_or("camera: nothing to frame (no vertices of that part)")?;
    let scale = f.scale * zoom;
    g.place_camera(f.target, scale, orbit);
    Ok(json!({
        "target": f.target, "zoom": scale, "yaw_deg": orbit.yaw.to_degrees(), "pitch_deg": orbit.pitch.to_degrees(),
        "points": pts.len(),
    }))
}

/// Hands against held items: `socket.hand_*` (the fist's centre on the grip, m2-grips)
/// against each item hanging on a hand socket (its origin is its main grip, its +Z the long
/// axis): distance to the origin, position along the axis and gap to the axis, in mm.
fn hands(game: &mut Game, id: u32) -> Result<Value, String> {
    let g = ingame(game)?;
    let (model, inst) = g.posed(id).ok_or("measure: not drawn")?;
    let globals = model.globals(inst.clip, inst.time);
    let pos = |n: usize| [globals[n][3][0], globals[n][3][1], globals[n][3][2]];
    let is_hand = |n: usize| model.nodes[n].name.trim_start_matches("rider.").starts_with("socket.hand_");
    let sockets: Vec<usize> = (0..model.nodes.len()).filter(|&n| is_hand(n)).collect();
    let items: Vec<usize> = (0..model.nodes.len())
        .filter(|&n| !model.nodes[n].name.contains("socket.") && model.nodes[n].parent.is_some_and(is_hand))
        .collect();
    let mm = |x: f32| (x * 10000.0).round() / 10.0;
    let mut pairs = Vec::new();
    for &s in &sockets {
        for &i in &items {
            let axis = globals[i][2];
            let len = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt().max(1e-6);
            let z = [axis[0] / len, axis[1] / len, axis[2] / len];
            let d = math::sub(pos(s), pos(i));
            let along = math::dot(d, z);
            let off = math::sub(d, math::scale(z, along));
            pairs.push(json!({
                "socket": model.nodes[s].name, "item": model.nodes[i].name,
                "held_by": model.nodes[model.nodes[i].parent.unwrap_or(0)].name,
                "distance_mm": mm(math::dot(d, d).sqrt()), "along_mm": mm(along), "gap_mm": mm(math::dot(off, off).sqrt()),
            }));
        }
    }
    let clip = inst.clip.and_then(|c| model.clips.get(c));
    Ok(json!({
        "id": id,
        "clip": clip.map(|c| c.name.clone()),
        "t": inst.time,
        "frame": inst.time * model.fps,
        "sockets": sockets.iter().map(|&s| json!({"name": model.nodes[s].name, "at_m": pos(s)})).collect::<Vec<_>>(),
        "hands": pairs,
    }))
}

/// One walk sample: tick, clip, time, ground point, both feet and both hands (world metres;
/// z above the unit's ground point) as drawn, after the gait's IK.
fn foot_sample(game: &mut Game, id: u32) -> Result<(Value, [Vec3; 4]), String> {
    let g = ingame(game)?;
    let tick = g.world().map_or(0, |w| w.tick.0);
    let (model, inst) = g.posed(id).ok_or("measure: not drawn")?;
    let globals = model.globals(inst.clip, inst.time);
    let ground = inst.transform[3];
    // The pose drawn: the gait's (IK on the clip) where it posed him, else the clip's. A
    // joint slot's palette entry carries the joint's bind position to its posed place.
    let shown = g.shown_pose(id);
    let foot = |name: &str| -> Vec3 {
        let Some(n) = model.nodes.iter().position(|n| n.name == name) else { return [0.0; 3] };
        let posed = shown.and_then(|pal| {
            model.slots.iter().zip(pal).find_map(|(slot, m)| {
                let bind = match slot {
                    Slot::Joint { node, inverse_bind } if *node == n => {
                        acr_render::damage::joint_position(inverse_bind)
                    }
                    Slot::Node(node) if *node == n => [0.0; 3],
                    _ => return None,
                };
                Some(std::array::from_fn(|r| m[r][0] * bind[0] + m[r][1] * bind[1] + m[r][2] * bind[2] + m[r][3]))
            })
        });
        let local = posed.unwrap_or([globals[n][3][0], globals[n][3][1], globals[n][3][2]]);
        math::transform_point(&inst.transform, local)
    };
    let world = [foot("foot_l"), foot("foot_r"), foot("hand_l"), foot("hand_r")];
    let toes = [foot("toe_l"), foot("toe_r")];
    let feet = world.map(|p| [p[0] / PIXELS_PER_METRE, p[1] / PIXELS_PER_METRE, (p[2] - ground[2]) / PIXELS_PER_METRE]);
    // Each foot's height over the ground under it (slopes).
    let over = world.map(|p| g.ground_z(p[0], p[1]).map_or(0.0, |z| (p[2] - z) / PIXELS_PER_METRE));
    let mm = |x: f32| (x * 1000.0).round() / 1000.0;
    let over_m = [over[0], over[1]].map(mm);
    // The toes: where they are and their height over the ground (heel lifts, toe-off).
    let toe_m = toes.map(|p| {
        let h = g.ground_z(p[0], p[1]).map_or(0.0, |z| (p[2] - z) / PIXELS_PER_METRE);
        [mm(p[0] / PIXELS_PER_METRE), mm(p[1] / PIXELS_PER_METRE), mm(h)]
    });
    let clip = inst.clip.and_then(|c| model.clips.get(c)).map(|c| c.name.clone());
    let r = |v: Vec3| v.map(|x| (x * 1000.0).round() / 1000.0);
    Ok((
        json!({"tick": tick, "clip": clip, "t": inst.time, "posed": shown.is_some(), "ground_m": r([ground[0] / PIXELS_PER_METRE, ground[1] / PIXELS_PER_METRE, 0.0]),
               "foot_l": r(feet[0]), "foot_r": r(feet[1]), "hand_l": r(feet[2]), "hand_r": r(feet[3]),
               "over_m": over_m, "toe_m": toe_m}),
        feet,
    ))
}

/// Foot (or hand) sliding over a walk: per effector, the horizontal distance it moves
/// between samples while planted (within 3 cm of its lowest height).
pub fn sliding(feet: &[[Vec3; 2]], names: [&str; 2]) -> Value {
    let mut out = Vec::new();
    for k in 0..2 {
        let low = feet.iter().map(|f| f[k][2]).fold(f32::MAX, f32::min);
        let planted = |f: &[Vec3; 2]| f[k][2] <= low + 0.03;
        let (mut slide, mut n) = (0.0f32, 0);
        for w in feet.windows(2) {
            if planted(&w[0]) && planted(&w[1]) {
                slide += ((w[1][k][0] - w[0][k][0]).powi(2) + (w[1][k][1] - w[0][k][1]).powi(2)).sqrt();
                n += 1;
            }
        }
        out.push(json!({"foot": names[k], "slide_m": (slide * 1000.0).round() / 1000.0, "planted_steps": n}));
    }
    out.into()
}

/// A sheet in progress: cells row by row (unit × clip × time), a column per view.
pub struct Sheet {
    path: String,
    v: Value,
    rows: Vec<(u32, Value)>,
    views: Vec<String>,
    cell: [u32; 2],
    next: usize,
    staged: bool,
    tries: u32,
    image: Vec<u8>,
}

/// `sheet`: the rows and columns of the request, ready to render.
pub fn sheet(v: &Value) -> Result<Task, String> {
    let path = v.get("path").and_then(Value::as_str).ok_or("sheet: missing \"path\"")?.to_owned();
    let units = ids(v, "ids");
    if units.is_empty() {
        return Err("sheet: missing \"ids\"".into());
    }
    let views: Vec<String> = match v.get("views").and_then(Value::as_array) {
        Some(a) => a.iter().filter_map(Value::as_str).map(str::to_owned).collect(),
        None => vec!["front".into(), "side".into(), "three_quarter".into()],
    };
    for view in &views {
        view_orbit(view, 0)?;
    }
    let clips = v.get("clips").and_then(Value::as_array).cloned().unwrap_or_else(|| vec![json!({})]);
    let mut rows = Vec::new();
    for &id in &units {
        for c in &clips {
            let clip = c.get("clip").cloned();
            let mut times: Vec<(String, Value)> = Vec::new();
            for key in ["frame", "t", "phase"] {
                let many = c.get(key).or_else(|| c.get(format!("{key}s").as_str()));
                match many {
                    Some(Value::Array(a)) => times.extend(a.iter().map(|x| (key.to_owned(), x.clone()))),
                    Some(x) => times.push((key.to_owned(), x.clone())),
                    None => {}
                }
            }
            if times.is_empty() {
                times.push(("t".into(), json!(0.0)));
            }
            for (key, x) in times {
                let mut spec = json!({ key: x });
                if let Some(cl) = &clip {
                    spec["clip"] = cl.clone();
                }
                rows.push((id, spec));
            }
        }
    }
    let cell = match v.get("cell").and_then(Value::as_array).map(Vec::as_slice) {
        Some([w, h]) => [w.as_u64().unwrap_or(320) as u32, h.as_u64().unwrap_or(480) as u32],
        _ => [320, 480],
    };
    let (w, h) = (cell[0] * views.len() as u32, (cell[1] + LABEL) * rows.len() as u32);
    if u64::from(w) * u64::from(h) > 64 << 20 {
        return Err("sheet: too large".into());
    }
    Ok(Task::Sheet(Box::new(Sheet {
        path,
        v: v.clone(),
        rows,
        views,
        cell,
        next: 0,
        staged: false,
        tries: 0,
        image: vec![255; (w * h * 4) as usize],
    })))
}

fn sheet_step(game: &mut Game, mut s: Box<Sheet>, reply: &mut serde_json::Map<String, Value>) -> Result<Next, String> {
    let units: Vec<u32> = s.rows.iter().map(|r| r.0).collect();
    if !drawn(game, &units)? {
        if s.tries >= MODEL_TRIES {
            return Err(format!("sheet: unit(s) {units:?} not drawn as models"));
        }
        s.tries += 1;
        return Ok(Next::Prime(Task::Sheet(s)));
    }
    let window = ingame(game)?.window_size_px();
    let [cw, ch] = [s.cell[0].min(window[0] as u32), s.cell[1].min(window[1] as u32)];
    let cols = s.views.len();
    let width = s.cell[0] * cols as u32;
    if s.staged {
        let (row, col) = (s.next / cols, s.next % cols);
        let crop = [(window[0] as u32 - cw) / 2, (window[1] as u32 - ch) / 2, cw, ch];
        let ([gw, gh], rgba) = shot::capture(game, Some(crop))?;
        let (x0, y0) = (col as u32 * s.cell[0], row as u32 * (s.cell[1] + LABEL) + LABEL);
        for y in 0..gh {
            let src = (y * gw * 4) as usize;
            let dst = (((y0 + y) * width + x0) * 4) as usize;
            s.image[dst..dst + (gw * 4) as usize].copy_from_slice(&rgba[src..src + (gw * 4) as usize]);
        }
        let (id, spec) = &s.rows[row];
        let what = match (spec.get("frame"), spec.get("t"), spec.get("phase")) {
            (Some(f), ..) => format!("F{f}"),
            (_, Some(t), _) => format!("T{t}"),
            (.., Some(p)) => format!("P{p}"),
            _ => String::new(),
        };
        let clip = spec.get("clip").and_then(Value::as_str).unwrap_or("");
        let label = format!("{id} {clip} {what} {}", s.views[col]);
        text(&mut s.image, width, x0 + 4, y0 - LABEL + 4, &label);
        s.next += 1;
        s.staged = false;
    }
    if s.next == s.rows.len() * cols {
        let h = (s.cell[1] + LABEL) * s.rows.len() as u32;
        shot::write_png(&s.path, [width, h], &s.image)?;
        reply.insert("path".into(), s.path.clone().into());
        reply.insert("size".into(), json!([width, h]));
        reply.insert("cells".into(), s.next.into());
        return Ok(Next::Done);
    }
    // Stage the next cell: hold the pose, frame it in the cell, draw it next pass.
    let (row, col) = (s.next / cols, s.next % cols);
    let (id, spec) = s.rows[row].clone();
    let g = ingame(game)?;
    let (model, inst) = g.posed(id).ok_or("sheet: not drawn")?;
    let (clip, time) = clip_time(model, inst.clip, &spec)?;
    g.hold_pose(id, Some((clip, time)));
    g.studio_hold = true;
    let mut cam = json!({"frame": [id], "view": s.views[col]});
    for k in ["part", "margin", "zoom", "pitch_deg"] {
        if let Some(x) = s.v.get(k) {
            cam[k] = x.clone();
        }
    }
    camera(game, &cam, Some([cw as f32, ch as f32]))?;
    s.staged = true;
    Ok(Next::Frame(Task::Sheet(s)))
}

/// A 5×7 font (columns, low bit at the top) for the sheet's labels: space, `-./`, digits,
/// `:=`, `A`–`Z`, `_`.
const GLYPHS: &str = " -./0123456789:=ABCDEFGHIJKLMNOPQRSTUVWXYZ_";
const FONT: [[u8; 5]; 43] = [
    [0x00, 0x00, 0x00, 0x00, 0x00],
    [0x08, 0x08, 0x08, 0x08, 0x08],
    [0x00, 0x60, 0x60, 0x00, 0x00],
    [0x20, 0x10, 0x08, 0x04, 0x02],
    [0x3E, 0x51, 0x49, 0x45, 0x3E],
    [0x00, 0x42, 0x7F, 0x40, 0x00],
    [0x42, 0x61, 0x51, 0x49, 0x46],
    [0x21, 0x41, 0x45, 0x4B, 0x31],
    [0x18, 0x14, 0x12, 0x7F, 0x10],
    [0x27, 0x45, 0x45, 0x45, 0x39],
    [0x3C, 0x4A, 0x49, 0x49, 0x30],
    [0x01, 0x71, 0x09, 0x05, 0x03],
    [0x36, 0x49, 0x49, 0x49, 0x36],
    [0x06, 0x49, 0x49, 0x29, 0x1E],
    [0x00, 0x36, 0x36, 0x00, 0x00],
    [0x14, 0x14, 0x14, 0x14, 0x14],
    [0x7E, 0x11, 0x11, 0x11, 0x7E],
    [0x7F, 0x49, 0x49, 0x49, 0x36],
    [0x3E, 0x41, 0x41, 0x41, 0x22],
    [0x7F, 0x41, 0x41, 0x22, 0x1C],
    [0x7F, 0x49, 0x49, 0x49, 0x41],
    [0x7F, 0x09, 0x09, 0x09, 0x01],
    [0x3E, 0x41, 0x49, 0x49, 0x7A],
    [0x7F, 0x08, 0x08, 0x08, 0x7F],
    [0x00, 0x41, 0x7F, 0x41, 0x00],
    [0x20, 0x40, 0x41, 0x3F, 0x01],
    [0x7F, 0x08, 0x14, 0x22, 0x41],
    [0x7F, 0x40, 0x40, 0x40, 0x40],
    [0x7F, 0x02, 0x0C, 0x02, 0x7F],
    [0x7F, 0x04, 0x08, 0x10, 0x7F],
    [0x3E, 0x41, 0x41, 0x41, 0x3E],
    [0x7F, 0x09, 0x09, 0x09, 0x06],
    [0x3E, 0x41, 0x51, 0x21, 0x5E],
    [0x7F, 0x09, 0x19, 0x29, 0x46],
    [0x46, 0x49, 0x49, 0x49, 0x31],
    [0x01, 0x01, 0x7F, 0x01, 0x01],
    [0x3F, 0x40, 0x40, 0x40, 0x3F],
    [0x1F, 0x20, 0x40, 0x20, 0x1F],
    [0x3F, 0x40, 0x38, 0x40, 0x3F],
    [0x63, 0x14, 0x08, 0x14, 0x63],
    [0x07, 0x08, 0x70, 0x08, 0x07],
    [0x61, 0x51, 0x49, 0x45, 0x43],
    [0x40, 0x40, 0x40, 0x40, 0x40],
];

/// Writes `label` (upper case; unknown characters as blanks) in black at twice the font size.
fn text(image: &mut [u8], width: u32, x: u32, y: u32, label: &str) {
    for (i, c) in label.to_uppercase().chars().enumerate() {
        let Some(g) = GLYPHS.find(c) else { continue };
        for (col, bits) in FONT[g].iter().enumerate() {
            for row in 0..7 {
                if bits >> row & 1 == 0 {
                    continue;
                }
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let px = x + i as u32 * 12 + col as u32 * 2 + dx;
                    let py = y + row * 2 + dy;
                    if px >= width {
                        continue;
                    }
                    let at = ((py * width + px) * 4) as usize;
                    if let Some(p) = image.get_mut(at..at + 3) {
                        p.fill(0);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Projects with the camera the framing sets (`InGame::place_camera`).
    fn project(f: Framing, orbit: Orbit, size: [f32; 2], p: Vec3) -> Vec3 {
        let camera = Camera {
            x: f.target[0] - size[0] / f.scale / 2.0,
            y: f.target[1] / 2.0 - size[1] / f.scale / 2.0,
            scale: f.scale,
        };
        View3d { camera, size, orbit: Some(orbit) }.project(p)
    }

    fn man(at: [f32; 2]) -> Vec<Vec3> {
        // A 1.8 m man's box with a musket reaching 0.6 m ahead (world pixels).
        let mut v = Vec::new();
        for x in [-9.0, 9.0] {
            for y in [-6.0, 18.0] {
                for z in [0.0, 54.0] {
                    v.push([at[0] + x, at[1] + y, z]);
                }
            }
        }
        v
    }

    #[test]
    fn framing_fits_all_points_inside_the_margin() {
        let size = [1600.0, 900.0];
        for (view, dir) in [("front", 0u8), ("side", 64), ("three_quarter", 200), ("top", 32), ("back", 128)] {
            let orbit = view_orbit(view, dir).unwrap();
            let pts = man([7680.0, 7680.0]);
            let f = frame(&pts, orbit, size, 0.1).unwrap();
            let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
            for &p in &pts {
                let q = project(f, orbit, size, p);
                for i in 0..2 {
                    lo[i] = lo[i].min(q[i]);
                    hi[i] = hi[i].max(q[i]);
                    assert!(q[i] >= size[i] * 0.1 - 0.5 && q[i] <= size[i] * 0.9 + 0.5, "{view}: {q:?}");
                }
            }
            // Centred, and touching the margin on the tighter axis.
            for i in 0..2 {
                assert!(((lo[i] + hi[i]) / 2.0 - size[i] / 2.0).abs() < 0.5, "{view}: {lo:?} {hi:?}");
            }
            let fill = ((hi[0] - lo[0]) / (0.8 * size[0])).max((hi[1] - lo[1]) / (0.8 * size[1]));
            assert!((fill - 1.0).abs() < 1e-3, "{view}: {fill}");
        }
    }

    #[test]
    fn named_views_look_at_the_unit_from_the_right_side() {
        // Facing east (dir 0): the front view looks west at him, the back view east.
        let f = view_orbit("front", 0).unwrap();
        let fwd =
            View3d { camera: Camera { x: 0.0, y: 0.0, scale: 1.0 }, size: [1.0; 2], orbit: Some(f) }.basis().forward;
        assert!(fwd[0] < -0.9, "{fwd:?}");
        let b = view_orbit("back", 0).unwrap();
        let fwd =
            View3d { camera: Camera { x: 0.0, y: 0.0, scale: 1.0 }, size: [1.0; 2], orbit: Some(b) }.basis().forward;
        assert!(fwd[0] > 0.9, "{fwd:?}");
        // Facing south (dir 64): the front view looks north.
        let f = view_orbit("front", 64).unwrap();
        let fwd =
            View3d { camera: Camera { x: 0.0, y: 0.0, scale: 1.0 }, size: [1.0; 2], orbit: Some(f) }.basis().forward;
        assert!(fwd[1] < -0.9, "{fwd:?}");
        assert!(view_orbit("top", 0).unwrap().pitch > 1.3);
        assert!(view_orbit("below", 0).is_err());
        assert_eq!(Part::parse("hands"), Ok(Part::Hands));
        assert!(Part::parse("tail").is_err());
    }

    #[test]
    fn sliding_counts_planted_motion_only() {
        let feet = [
            [[0.0, 0.0, 0.0], [0.0, 0.0, 0.2]],
            [[0.01, 0.0, 0.0], [0.3, 0.0, 0.2]],
            [[0.03, 0.0, 0.01], [0.6, 0.0, 0.0]],
        ];
        let s = sliding(&feet, ["foot_l", "foot_r"]);
        assert!((s[0]["slide_m"].as_f64().unwrap() - 0.03).abs() < 1e-6, "{s}");
        assert_eq!(s[1]["planted_steps"], 0);
    }

    #[test]
    fn sheet_rows_and_labels() {
        let Task::Sheet(s) = sheet(&json!({"path": "x.png", "ids": [3], "views": ["front", "side"],
            "clips": [{"clip": "ATTACK1", "frames": [0, 10, 20]}], "cell": [100, 50]}))
        .unwrap() else {
            panic!()
        };
        assert_eq!(s.rows.len(), 3);
        assert_eq!(s.rows[1].1, json!({"clip": "ATTACK1", "frame": 10}));
        assert_eq!(s.image.len(), (200 * 3 * (50 + LABEL) * 4) as usize);
        let mut img = vec![255u8; 100 * 20 * 4];
        text(&mut img, 100, 0, 0, "a1");
        assert!(img.chunks(4).any(|p| p[0] == 0));
        assert!(sheet(&json!({"path": "x.png", "ids": [1], "views": ["under"]})).is_err());
    }
}
