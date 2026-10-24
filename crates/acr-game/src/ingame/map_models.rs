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

    /// Nothing placed, every sprite drawn.
    pub fn clear(&mut self) {
        self.placed.clear();
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
        let mut hidden = HashSet::new();
        let mut scales: HashMap<(SpriteGroup, u16), f32> = HashMap::new();
        for o in objects.iter().filter(|o| matches!(o.group, SpriteGroup::Trees | SpriteGroup::Stones)) {
            let Some(choice) = self.table.get(&(o.group, o.index)) else { continue };
            let Some(&(model, height)) = ready.get(&choice.model) else { continue };
            let scale = *scales.entry((o.group, o.index)).or_insert_with(|| {
                choice.scale.or_else(|| sprite_scale(sprites, o.group, o.index, height)).unwrap_or(1.0)
            });
            let ground = view3d::ground_point(o.x, o.y, terrain.height_at(o.x, o.y));
            let dir = if choice.sprite_turn { SPRITE_DIR } else { turn(o.x, o.y) };
            self.placed.push((model, view3d::model_matrix(ground, view3d::facing_yaw(dir), scale)));
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
    fn the_shipped_table_parses() {
        assert!(!parse(TABLE).is_empty());
    }
}
