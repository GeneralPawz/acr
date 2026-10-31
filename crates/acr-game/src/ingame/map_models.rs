//! The map's trees and stones (`TREELIST`, `STONLIST` objects) drawn as 3D models where
//! `assets/nature/map_objects.json` names one; the others keep their sprite
//! (docs/rendering-3d.md "Map trees and stones"). Render only: the places come from
//! `World::objects`, each turn from a hash of the place (or the sprite's own facing), never
//! from the simulation's RNG.

use std::collections::{HashMap, HashSet};

use acr_formats::m3d::SpriteGroup;
use acr_render::math::Mat4;
use acr_render::scene::{FrameKey, MapObjects};
use acr_render::terrain::TerrainData;
use acr_render::view3d::{self, COS_E, PIXELS_PER_METRE};
use acr_sim::world::MapObject;

/// The table: `{"TREELIST": {"104": "broadleaf", "10-14": {"model": "pine", "scale": 1.2},
/// "452": {"model": "map_t452", "turn": "sprite"}}, "STONLIST": {...}}`; keys starting with
/// `_` are comments.
const TABLE: &str = include_str!("../../../../assets/nature/map_objects.json");

/// The scale of a model fitted to its sprite stays within these bounds (a frame whose box
/// is off, a model built at another size).
const FIT: std::ops::RangeInclusive<f32> = 0.25..=4.0;

/// The model drawn for one sprite type.
#[derive(Debug, Clone, PartialEq)]
pub struct Choice {
    /// Upper-case model `NAME` (`ACR\Models\<name>.glb`).
    pub model: String,
    /// The model's scale; `None`: its rest height fitted to the sprite's height.
    pub scale: Option<f32>,
    /// `"turn": "sprite"`: a model built from and fitted to this very sprite (`map_t<N>`,
    /// `acr_assets.nature._map_fit`) keeps the sprite's facing, `RealDir` 64 (yaw 0, the
    /// view the fit used), and its built size (scale 1 unless `scale` says otherwise);
    /// otherwise turned by [`turn`], the hash of the place.
    pub sprite_turn: bool,
}

/// The facing of a model fitted to its sprite: the reference view's `RealDir`
/// (`acr model-ref` `list_ref`: one view, `stored_direction` 64).
pub const SPRITE_DIR: u8 = 64;

/// Sprite type → model, from the JSON table.
pub fn parse(text: &str) -> HashMap<(SpriteGroup, u16), Choice> {
    let mut out = HashMap::new();
    let Ok(serde_json::Value::Object(root)) = serde_json::from_str::<serde_json::Value>(text) else {
        log::warn!("map_objects.json: not a JSON object");
        return out;
    };
    for (list, entries) in &root {
        let group = match list.to_ascii_uppercase().as_str() {
            "TREELIST" => SpriteGroup::Trees,
            "STONLIST" => SpriteGroup::Stones,
            _ => continue,
        };
        let Some(entries) = entries.as_object() else { continue };
        for (key, value) in entries.iter().filter(|(k, _)| !k.starts_with('_')) {
            let (from, to) = match key.split_once('-') {
                Some((a, b)) => (a.trim().parse::<u16>(), b.trim().parse::<u16>()),
                None => (key.trim().parse::<u16>(), key.trim().parse::<u16>()),
            };
            let (Ok(from), Ok(to)) = (from, to) else {
                log::warn!("map_objects.json: {list} key {key:?} is no index or range");
                continue;
            };
            let choice = match value {
                serde_json::Value::String(m) => {
                    Choice { model: m.to_ascii_uppercase(), scale: None, sprite_turn: false }
                }
                serde_json::Value::Object(o) => {
                    let Some(m) = o.get("model").and_then(serde_json::Value::as_str) else { continue };
                    let sprite_turn = match o.get("turn") {
                        None => false,
                        Some(serde_json::Value::String(t)) if t == "sprite" => true,
                        Some(t) => {
                            log::warn!("map_objects.json: {list} {key}: turn {t} is not \"sprite\"");
                            false
                        }
                    };
                    let scale = o.get("scale").and_then(serde_json::Value::as_f64).map(|s| s as f32);
                    let scale = scale.or(sprite_turn.then_some(1.0));
                    Choice { model: m.to_ascii_uppercase(), scale, sprite_turn }
                }
                _ => continue,
            };
            for index in from..=to {
                out.insert((group, index), choice.clone());
            }
        }
    }
    out
}

/// The turn of an object at `(x, y)`, 0..256 (the props' hash, docs/rendering-3d.md "Wind").
pub fn turn(x: i32, y: i32) -> u8 {
    (x.wrapping_mul(73_856_093) ^ y.wrapping_mul(19_349_663)).rem_euclid(256) as u8
}

/// The scale that makes a model of rest height `height_m` as tall as the sprite of type
/// `index`: the frame's opaque top above its ground point (`DY − min y`, screen pixels, i.e.
/// true height × cos 30°).
pub fn sprite_scale(objects: &MapObjects, group: SpriteGroup, index: u16, height_m: f32) -> Option<f32> {
    let list = objects.list(group)?;
    let entry = list.entries.get(usize::from(index))?;
    let (_, top, _, _) = objects.frame_bounds(&(list.gp_path(), usize::from(index)))?;
    let tall = (entry.dy - top) as f32 / COS_E;
    (height_m > 0.0 && tall > 0.0).then(|| (tall / (height_m * PIXELS_PER_METRE)).clamp(*FIT.start(), *FIT.end()))
}

/// Objects, models ready, sprites ready, the first object's place.
type PlaceKey = (usize, usize, bool, Option<(i32, i32)>);

/// The placed models of the current map and the sprite frames they replace.
#[derive(Default)]
pub struct MapModels {
    /// `map_models=on|off`.
    pub on: bool,
    pub table: HashMap<(SpriteGroup, u16), Choice>,
    /// Model index and transform per drawn object.
    pub placed: Vec<(usize, Mat4)>,
    /// The `World::objects` index of each [`MapModels::placed`] entry (for
    /// `World::struck`, [`struck_transform`]).
    pub objs: Vec<usize>,
    /// What [`MapModels::placed`] was built for: objects, models ready, sprites ready,
    /// the first object's place.
    key: Option<PlaceKey>,
    /// Frames of the replaced sprites; [`MapModels::hidden_changed`] once they change.
    pub hidden: HashSet<FrameKey>,
    pub hidden_changed: bool,
}

impl MapModels {
    pub fn new() -> Self {
        Self { on: true, table: parse(TABLE), ..Default::default() }
    }

    /// The model names the table uses.
    pub fn names(&self) -> HashSet<&str> {
        self.table.values().map(|c| c.model.as_str()).collect()
    }

    /// Places the models again at the next [`MapModels::place`] (the ground under them changed).
    pub fn invalidate(&mut self) {
        self.key = None;
    }

    /// Nothing placed, every sprite drawn.
    pub fn clear(&mut self) {
        self.placed.clear();
        self.objs.clear();
        self.key = None;
        if !self.hidden.is_empty() {
            self.hidden.clear();
            self.hidden_changed = true;
        }
    }

    /// Places the mapped objects whose model is `ready` (name → model index, rest height in
    /// metres), when the objects, the models or the sprites changed since the last time.
    pub fn place(
        &mut self,
        objects: &[MapObject],
        ready: &HashMap<String, (usize, f32)>,
        sprites: &MapObjects,
        sprites_ready: bool,
        terrain: &TerrainData,
    ) {
        let key = Some((objects.len(), ready.len(), sprites_ready, objects.first().map(|o| (o.x, o.y))));
        if key == self.key {
            return;
        }
        self.key = key;
        self.placed.clear();
        self.objs.clear();
        let mut hidden = HashSet::new();
        let mut scales: HashMap<(SpriteGroup, u16), f32> = HashMap::new();
        for (k, o) in
            objects.iter().enumerate().filter(|(_, o)| matches!(o.group, SpriteGroup::Trees | SpriteGroup::Stones))
        {
            let Some(choice) = self.table.get(&(o.group, o.index)) else { continue };
            let Some(&(model, height)) = ready.get(&choice.model) else { continue };
            let scale = *scales.entry((o.group, o.index)).or_insert_with(|| {
                choice.scale.or_else(|| sprite_scale(sprites, o.group, o.index, height)).unwrap_or(1.0)
            });
            let ground = view3d::ground_point(o.x, o.y, terrain.height_at(o.x, o.y));
            let dir = if choice.sprite_turn { SPRITE_DIR } else { turn(o.x, o.y) };
            self.placed.push((model, view3d::model_matrix(ground, view3d::facing_yaw(dir), scale)));
            self.objs.push(k);
            if let Some(list) = sprites.list(o.group) {
                hidden.insert((list.gp_path(), usize::from(o.index)));
            }
        }
        if sprites_ready && ready.is_empty() {
            // Once per map: where the woods are and which tree types they use (for tests
            // and for filling the table).
            let mut cells: HashMap<(i32, i32), usize> = HashMap::new();
            let mut types: HashMap<u16, usize> = HashMap::new();
            for o in objects.iter().filter(|o| o.group == SpriteGroup::Trees) {
                *cells.entry((o.x >> 10, o.y >> 10)).or_default() += 1;
                *types.entry(o.index).or_default() += 1;
            }
            let densest = cells.iter().max_by_key(|(c, n)| (**n, std::cmp::Reverse(**c)));
            let mut types: Vec<(u16, usize)> = types.into_iter().collect();
            types.sort_by_key(|&(i, n)| (std::cmp::Reverse(n), i));
            types.truncate(16);
            log::info!(
                "3d: map trees: densest 1024 px cell {:?}, types by count {types:?}",
                densest.map(|((x, y), n)| ([x * 1024 + 512, y * 1024 + 512], n))
            );
        }
        if !self.placed.is_empty() {
            log::info!(
                "3d: {} map objects drawn as models ({} sprite types), scales {:?}",
                self.placed.len(),
                hidden.len(),
                scales.values().fold((f32::MAX, 0.0f32), |(a, b), &s| (a.min(s), b.max(s)))
            );
        }
        if hidden != self.hidden {
            self.hidden = hidden;
            self.hidden_changed = true;
        }
    }
}

/// Ticks a struck tree takes to fall (it speeds up as it goes, like a falling pole).
const FALL_TICKS: f32 = 24.0;
/// A felled tree lies at this angle from upright (its crown props it up a little).
const FALL_ANGLE: f32 = 1.48;
/// Ticks a knocked stone takes to slide and tumble to its new place.
const SLIDE_TICKS: f32 = 10.0;

/// `m` with its axes turned by `angle` about the world `axis` (unit), its place kept.
fn rotated(m: &Mat4, axis: [f32; 3], angle: f32) -> Mat4 {
    let (s, c) = angle.sin_cos();
    let turn = |v: [f32; 3]| {
        // Rodrigues: v cos + (a × v) sin + a (a · v)(1 − cos).
        let d = axis[0] * v[0] + axis[1] * v[1] + axis[2] * v[2];
        let x = [axis[1] * v[2] - axis[2] * v[1], axis[2] * v[0] - axis[0] * v[2], axis[0] * v[1] - axis[1] * v[0]];
        [0, 1, 2].map(|i| v[i] * c + x[i] * s + axis[i] * d * (1.0 - c))
    };
    let mut out = *m;
    for col in out.iter_mut().take(3) {
        let v = turn([col[0], col[1], col[2]]);
        col[..3].copy_from_slice(&v);
    }
    out
}

/// A sim direction (0 east, 64 south) as a world unit vector.
fn dir_vec(dir: u8) -> [f32; 2] {
    let a = f32::from(dir) * std::f32::consts::TAU / 256.0;
    [a.cos(), a.sin()]
}

/// `base` turned about the world's up axis so that the model's +X points along `dir`: the
/// struck states of a map model (stump, felled, scar, chip groups, docs/assets.md "Struck map
/// objects") are built for a ball flying +X.
pub fn facing(base: Mat4, dir: u8) -> Mat4 {
    let [dx, dy] = dir_vec(dir);
    let now = base[0][1].atan2(base[0][0]);
    rotated(&base, [0.0, 0.0, 1.0], dy.atan2(dx) - now)
}

/// Chip mark sets a rock model carries (`<name>.chip.<k>`, `acr_assets.nature.CHIP_SIDES`):
/// set k is where a ball flying at k / CHIP_SIDES of a turn from the model's +X towards
/// Blender's +Y meets the rock.
pub const CHIP_SIDES: usize = 8;

/// The chip mark set of a rock placed at `base` for a ball heading `dir`: the heading in the
/// model's own frame (glTF x = Blender x; glTF z = Blender -y), to the nearest set.
pub fn chip_side(base: Mat4, dir: u8) -> usize {
    let [dx, dy] = dir_vec(dir);
    let along = |c: usize| {
        let v = [base[c][0], base[c][1], base[c][2]];
        (dx * v[0] + dy * v[1]) / (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).max(1e-9)
    };
    let (bx, by) = (along(0), -along(2));
    let turns = by.atan2(bx) / std::f32::consts::TAU;
    ((turns * CHIP_SIDES as f32).round() as i64).rem_euclid(CHIP_SIDES as i64) as usize
}

/// The transform of a tree's felled state (`<name>.felled`, built lying [`FALL_ANGLE`] towards
/// +X about its ground point, crushed onto the ground there): turned to the fall direction,
/// pitched and rolled to the ground under it ([`lie_on_ground`]; `length` its reach in world
/// pixels, `height` the game's ground height) and, while it falls, turned back up by the part
/// of the fall still to come. The stump stays at `base`.
pub fn felled_transform(
    base: Mat4,
    dir: u8,
    tick: u64,
    now: f32,
    length: f32,
    height: impl Fn(i32, i32) -> i32,
) -> Mat4 {
    let t = ((now - tick as f32) / FALL_TICKS).clamp(0.0, 1.0);
    let [dx, dy] = dir_vec(dir);
    let (pitch, roll) = lie_on_ground(base, dir, length, height);
    let lying = rotated(&rotated(&facing(base, dir), [dx, dy, 0.0], roll), [-dy, dx, 0.0], pitch);
    rotated(&lying, [-dy, dx, 0.0], -FALL_ANGLE * (1.0 - t * t))
}

/// Pitch and roll (radians, about up × fall direction and about the fall direction) that lay
/// a felled tree of `length` world pixels from its foot at `base` onto the ground: a rigid
/// trunk rests on the highest ground under it, so the pitch is the steepest rise from the
/// foot to the ground along its length (a dip leaves it bridging, a slope down tips it down
/// to touch); the roll follows the ground across the crown, half the length out.
pub fn lie_on_ground(base: Mat4, dir: u8, length: f32, height: impl Fn(i32, i32) -> i32) -> (f32, f32) {
    let [dx, dy] = dir_vec(dir);
    let [x0, y0] = [base[3][0], base[3][1]];
    let z = |x: f32, y: f32| view3d::ground_point(x as i32, y as i32, height(x as i32, y as i32))[2];
    let foot = z(x0, y0);
    let rise = (1..=5)
        .map(|k| {
            let s = length * k as f32 / 5.0;
            ((z(x0 + dx * s, y0 + dy * s) - foot) / s).atan()
        })
        .fold(f32::MIN, f32::max);
    // Positive about up × dir turns the trunk down.
    let pitch = if length > 0.0 { -rise } else { 0.0 };
    let (s, w) = (0.5 * length, 0.15 * length);
    let [cx, cy] = [x0 + dx * s, y0 + dy * s];
    // A positive turn about dir lifts the side up × dir = (-dy, dx).
    let side = z(cx - dy * w, cy + dx * w);
    let other = z(cx + dy * w, cy - dx * w);
    let roll = if w > 0.0 { ((side - other) / (2.0 * w)).atan() } else { 0.0 };
    (pitch, roll)
}

/// The transform of map object `k` as a ball left it (`World::struck`, rule `ball_terrain`;
/// render only), for models without struck states: a felled tree falls away from the impact
/// about its foot and then lies; a
/// knocked stone slides and tumbles to its new place; anything else stands as placed.
pub fn struck_transform(
    base: Mat4,
    k: usize,
    struck: &acr_sim::ball_terrain::StruckMap,
    now: f32,
    height: impl Fn(i32, i32) -> i32,
) -> Mat4 {
    use acr_sim::ball_terrain::Struck;
    match struck.get(&(k as u32)) {
        Some(&Struck::Felled { dir, tick }) => {
            let t = ((now - tick as f32) / FALL_TICKS).clamp(0.0, 1.0);
            let [dx, dy] = dir_vec(dir);
            // About the horizontal axis up × fall direction: the top swings towards `dir`.
            rotated(&base, [-dy, dx, 0.0], FALL_ANGLE * t * t)
        }
        Some(&Struck::Moved { dx, dy, from, tick }) => {
            let t = ((now - tick as f32) / SLIDE_TICKS).clamp(0.0, 1.0);
            let ease = 1.0 - (1.0 - t) * (1.0 - t);
            // `base` stands on the map place; `from` and `(dx, dy)` are offsets from it.
            let x = base[3][0] + from.0 as f32 + (dx - from.0) as f32 * ease;
            let y = base[3][1] + from.1 as f32 + (dy - from.1) as f32 * ease;
            let (mx, my) = ((dx - from.0) as f32, (dy - from.1) as f32);
            let len = (mx * mx + my * my).sqrt().max(1.0);
            // Tumbles about the axis across its way, a quarter turn per 20 px rolled.
            let mut m = rotated(&base, [-my / len, mx / len, 0.0], len * ease / 20.0 * std::f32::consts::FRAC_PI_2);
            let g = view3d::ground_point(x as i32, y as i32, height(x as i32, y as i32));
            m[3] = [g[0], g[1], g[2], 1.0];
            m
        }
        _ => base,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_takes_indices_ranges_and_scales() {
        let t = parse(
            r#"{"TREELIST": {"_note": "x", "3": "broadleaf", "10-12": {"model": "Pine", "scale": 1.5},
                "452": {"model": "map_t452", "turn": "sprite"}},
                "STONLIST": {"0": "rock"}, "HOLELIST": {"1": "hole"}}"#,
        );
        assert_eq!(t.len(), 6);
        assert_eq!(t[&(SpriteGroup::Trees, 3)], Choice { model: "BROADLEAF".into(), scale: None, sprite_turn: false });
        assert_eq!(t[&(SpriteGroup::Trees, 11)], Choice { model: "PINE".into(), scale: Some(1.5), sprite_turn: false });
        assert_eq!(
            t[&(SpriteGroup::Trees, 452)],
            Choice { model: "MAP_T452".into(), scale: Some(1.0), sprite_turn: true }
        );
        assert!(t.contains_key(&(SpriteGroup::Stones, 0)));
        assert!(!t.contains_key(&(SpriteGroup::Trees, 13)));
    }

    #[test]
    fn a_felled_tree_falls_towards_its_direction() {
        use acr_sim::ball_terrain::{Struck, StruckMap};
        let base = view3d::model_matrix([100.0, 100.0, 0.0], 0.0, 1.0);
        let mut struck = StruckMap::new();
        struck.insert(3, Struck::Felled { dir: 0, tick: 10 });
        let up = |m: &Mat4| [m[1][0], m[1][1], m[1][2]];
        assert_eq!(struck_transform(base, 3, &struck, 10.0, |_, _| 0), base);
        let lying = struck_transform(base, 3, &struck, 100.0, |_, _| 0);
        // The model's up axis now points east and nearly level.
        assert!(up(&lying)[0] > 0.9 * up(&base)[2] && up(&lying)[2].abs() < 0.2 * up(&base)[2], "{:?}", up(&lying));
        assert_eq!(lying[3], base[3]);
        assert_eq!(struck_transform(base, 4, &struck, 100.0, |_, _| 0), base);
    }

    #[test]
    fn felled_state_turns_to_the_fall_and_stands_up_at_the_start() {
        let base = view3d::model_matrix([100.0, 100.0, 0.0], 0.7, 1.0);
        let scale = (base[1][0].powi(2) + base[1][1].powi(2) + base[1][2].powi(2)).sqrt();
        for dir in [0u8, 64, 100, 200] {
            let [dx, dy] = dir_vec(dir);
            let f = facing(base, dir);
            let x = [f[0][0], f[0][1]];
            assert!((x[0] * dx + x[1] * dy) / scale > 0.999, "{dir}: {x:?}");
            assert_eq!(f[3], base[3]);
            // Lying at the end: the model is only turned about up.
            let end = felled_transform(base, dir, 10, 100.0, 200.0, |_, _| 0);
            for c in 0..3 {
                for r in 0..3 {
                    assert!((end[c][r] - f[c][r]).abs() < 1e-4 * scale);
                }
            }
            // At the strike the lying model is turned back upright: its up axis tips by
            // FALL_ANGLE away from the fall, its +X (the lying trunk) points nearly up.
            let start = felled_transform(base, dir, 10, 10.0, 200.0, |_, _| 0);
            let up = [start[1][0], start[1][1], start[1][2]];
            assert!((up[0] * dx + up[1] * dy) / scale < -0.99, "{dir}: {up:?}");
            assert!(start[0][2] / scale > 0.99, "{dir}: {:?}", start[0]);
        }
    }

    #[test]
    fn a_felled_tree_lies_along_the_slope_under_it() {
        let base = view3d::model_matrix([1000.0, 1000.0, 0.0], 0.0, 1.0);
        let len = 1000.0;
        // Ground rising 1 in 4 to the east (height in screen pixels: world z = h / COS_E).
        let ramp = |x: i32, _y: i32| ((x - 1000) as f32 * 0.25 * view3d::COS_E) as i32;
        // Felled east, uphill: the crown end rises with the ground; across it is level.
        let (pitch, roll) = lie_on_ground(base, 0, len, ramp);
        assert!((pitch + 0.25f32.atan()).abs() < 0.02, "{pitch}");
        assert!(roll.abs() < 0.02, "{roll}");
        let m = felled_transform(base, 0, 0, 100.0, len, ramp);
        // The lying model's +X (the trunk) climbs at the ramp's angle.
        let x = [m[0][0], m[0][1], m[0][2]];
        assert!((x[2] / x[0] - 0.25).abs() < 0.03, "{x:?}");
        // Felled north (dir 192, -y) across the same ramp: rolled, its up × dir side (east) up.
        let (pitch, roll) = lie_on_ground(base, 192, len, ramp);
        assert!(pitch.abs() < 0.02, "{pitch}");
        assert!((roll - 0.25f32.atan()).abs() < 0.02, "{roll}");
        // Downhill: tipped down to touch the ground.
        let (pitch, _) = lie_on_ground(base, 128, len, ramp);
        assert!((pitch - 0.25f32.atan()).abs() < 0.02, "{pitch}");
        // Flat ground: nothing.
        assert_eq!(lie_on_ground(base, 40, len, |_, _| 0), (0.0, 0.0));
    }

    #[test]
    fn a_chip_faces_the_ball() {
        // Unturned: Blender +X is world east, Blender +Y world north (-y).
        let base = view3d::model_matrix([500.0, 500.0, 0.0], 0.0, 0.7);
        assert_eq!(chip_side(base, 0), 0); // east
        assert_eq!(chip_side(base, 192), 2); // north: Blender +Y
        assert_eq!(chip_side(base, 128), 4); // west
        assert_eq!(chip_side(base, 64), 6); // south
        // A rock turned a quarter: the heading in its own frame turns back by as much.
        let turned = view3d::model_matrix([500.0, 500.0, 0.0], std::f32::consts::FRAC_PI_2, 1.0);
        let east = chip_side(turned, 0);
        let x = [turned[0][0], turned[0][1]];
        // Its +X points north or south; a ball flying east meets it from its side.
        assert!(x[0].abs() < 1e-3);
        assert!(east == 2 || east == 6, "{east}");
        assert_eq!(chip_side(turned, 128), (east + 4) % 8);
    }

    #[test]
    fn the_shipped_table_parses() {
        assert!(!parse(TABLE).is_empty());
    }
}
