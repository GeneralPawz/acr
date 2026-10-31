//! The objects of a map (trees, stones, units, buildings) placed on the terrain in the
//! original's draw order.
//!
//! The original collects the visible objects every frame in a z-buffer of screen rows
//! (0x49C3E1: bucket = sort y + 256) and draws, after the terrain: first the flat objects
//! (0x49C249: stones, holes, ...; and ground parts of buildings), then the bucket rows from
//! top to bottom. Sprites of `TREELIST` and parts of complex objects are sorted by the
//! object's ground position `y / 2` *without* its height (0x4D0930); the frame is drawn at
//! `(x − DX, y / 2 − height − DY)`. See `docs/formats/terrain.md`.

use std::collections::{HashMap, HashSet};

use acr_formats::defs::UnitDef;
use acr_formats::m3d::{M3dMap, SpriteGroup};
use acr_formats::sprite_list::SpriteList;

use crate::terrain::{Camera, TerrainData};
use crate::view3d::View3d;

/// One frame of a sprite pack, by game path and frame index.
pub type FrameKey = (String, usize);

/// The zoomed-out view's version of a sprite pack (0x4870ee): every pack loaded from the
/// root folder has a quarter-size twin `L_Mode\<name>` with the same frames; the trees'
/// pack is `L_Mode\Der` (0x5fde23). Packs in folders have none.
pub fn zoom_pack(path: &str) -> Option<String> {
    let upper = acr_formats::normalize_path(path);
    if upper == "TREES.GP" {
        return Some("L_MODE\\DER.GP".to_owned());
    }
    (!upper.contains('/')).then(|| format!("L_MODE\\{upper}"))
}

/// How an object is ordered against the others.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Layer {
    /// Drawn flat on the ground before all sorted objects.
    Ground,
    /// Sorted by the ground point's screen row.
    Sorted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneSprite {
    pub frame: FrameKey,
    pub layer: Layer,
    /// Ground point in world pixels; the sort row is `y / 2`.
    pub x: i32,
    pub y: i32,
    /// Frame origin relative to the ground point's screen position (height included).
    pub dx: i32,
    pub dy: i32,
    /// Sort row: the ground point's `y / 2`, plus a building part's sort line.
    pub sort: i32,
    pub player: u8,
    /// Drawn half transparent (`ShowGPTransparent`: buildings in the transparency mode,
    /// corpses).
    pub transparent: bool,
    /// Drawn mirrored around the ground point (the east half of a unit's directions); `dx`
    /// is then already the mirrored offset.
    pub mirrored: bool,
    /// Darkness levels lighten (`WHITE` weapons).
    pub white: bool,
}

impl SceneSprite {
    fn key(&self) -> (Layer, i32) {
        (self.layer, self.sort)
    }
}

/// A unit or building type as the map shows it: the frames of its idle animation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitLook {
    /// Frame, offset, layer and sort row offset of each part.
    pub parts: Vec<(FrameKey, (i32, i32), Layer, i32)>,
    /// The frames are drawn mirrored around the ground point (the offsets are already
    /// mirrored).
    pub mirrored: bool,
}

/// The stored direction of an animation with `dirs` stored directions that a unit facing
/// `dir` (`RealDir`, 0 = east, 64 = south) shows, and whether it is drawn mirrored, as the
/// drawing code picks them (0x5062a3):
///
/// - `FULLROTATION` types (cannons): `k = (dir + 128/dirs − 64) / (256/dirs)`, never
///   mirrored (0x50663f).
/// - An odd count (9 for most units): with `n = 2·dirs − 2` (16), the index
///   `i = ((dir + 64 + 255/(2n)) & 255)·n >> 8`; below `dirs − 1` the frame is
///   `k = dirs − 1 − i` drawn mirrored, otherwise `k = i − (dirs − 1)` (0x506cd5).
/// - An even count: `n = 2·dirs`, `i = ((dir + 64) & 255)·n >> 8`; below `dirs`, `k =
///   dirs − 1 − i` mirrored, otherwise `k = i − dirs`.
///
/// Stored direction 0 faces south, the stored half turns over west to north; the east half
/// is its mirror image.
pub fn sprite_direction(dir: u8, dirs: usize, full_rotation: bool) -> (usize, bool) {
    let r = dirs.max(1) as i32;
    let dir = i32::from(dir);
    if full_rotation {
        let per = (256 / r).max(1);
        let v = (dir + (per >> 1) - 64).rem_euclid(256);
        return (((v / per) as usize).min(dirs.max(1) - 1), false);
    }
    let (n, c, below) = if r & 1 != 0 {
        let n = (2 * r - 2).max(1);
        (n, 255 / (2 * n), r - 1)
    } else {
        (2 * r, 0, r)
    };
    let i = (((dir + 64 + c) & 255) * n) >> 8;
    if i < below { ((r - 1 - i) as usize, true) } else { ((i - below) as usize, false) }
}

/// The `TOP` sort line of `LINESORT`.
const TOP: i32 = 10_000;

/// How `LINESORT` orders one part of a building animation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PartSort {
    Ground,
    /// A line in frame pixels; objects below it are drawn later.
    Line(i32, i32, i32, i32),
}

/// The pack and frame range of a unit's idle animation, to be completed with the
/// pack's frame count by [`unit_look`]. Buildings use `#STANDLO` (the finished
/// building), units `#STAND`.
pub fn idle_animation(def: &UnitDef) -> Option<&acr_formats::defs::md::Animation> {
    let names: &[&str] =
        if def.is_building() { &["#STANDLO", "#STAND", "#STANDHI"] } else { &["#STAND", "#PSTAND", "#MOTION_L"] };
    names.iter().find_map(|n| def.animation(n)).or_else(|| def.animations.first())
}

/// The sprite packs an object type needs: a building's idle, `#WORK`, construction
/// (`#BUILDLO_k`) and ruin animations and its sections (`ADDSECTION`), every animation of a
/// unit (the
/// simulation plays walking, fighting and dying, see `acr_sim::world::AnimKind`).
pub fn unit_packs(def: &UnitDef) -> Vec<String> {
    let anims: Vec<&acr_formats::defs::md::Animation> = if def.is_building() {
        let stages = def.animations.iter().filter(|a| {
            let n = a.name.to_ascii_uppercase();
            n.starts_with("#BUILDLO") || n.starts_with("#DEATHLIE") || n == "#WORK"
        });
        idle_animation(def).into_iter().chain(stages).collect()
    } else {
        def.animations.iter().collect()
    };
    let sections = def.sections().into_iter().map(|s| s.sprite);
    let mut packs: Vec<String> = anims
        .iter()
        .flat_map(|a| a.frame_refs())
        .map(|r| r.sprite)
        .chain(sections)
        .filter_map(|i| def.sprite(i))
        .map(|s| s.gp_path())
        .collect();
    packs.sort();
    packs.dedup();
    packs
}

/// The frames of `def`'s idle animation facing `direction` (0..=255, as stored in maps).
///
/// A building's animation lists its parts, all drawn; `LINESORT` marks `GROUND` parts.
/// A unit shows the first frame. Frame `f` of stored direction `d` is `f · dirs + d`, with
/// `d` from [`sprite_direction`] (see `docs/simulation.md`, "Directions and sprites").
///
/// A building on a map is finished with no section built: its sections show state 0, its
/// `#WORK` animation its first frame ([`work_part`]).
pub fn unit_look(def: &UnitDef, direction: u8, frames_in: impl Fn(&str) -> Option<usize>) -> Option<UnitLook> {
    let mut look = animation_look(def, idle_animation(def)?, direction, 0, def.is_building().then_some(0), &frames_in)?;
    look.parts.extend(work_part(def, 0, &frames_in));
    Some(look)
}

/// The frames of animation `anim` of `def` at animation frame `step`, facing `direction`.
/// Buildings show all parts of the animation; units show the part at `step`. A finished
/// building passes its section states (`sections`, OB+0xaa) to add its [`section_parts`].
pub fn animation_look(
    def: &UnitDef,
    anim: &acr_formats::defs::md::Animation,
    direction: u8,
    step: usize,
    sections: Option<u16>,
    frames_in: impl Fn(&str) -> Option<usize>,
) -> Option<UnitLook> {
    let refs = anim.frame_refs();
    let dirs = anim.directions.max(1) as usize;
    let (d, mirrored) = sprite_direction(direction, dirs, def.get("FULLROTATION").is_some());
    let sorts = part_sorts(def, &anim.name);
    // A building's picture is placed at its position + `LOCATION x y`, the corner of the
    // picture rectangle (the placement preview 0x4bdd8e draws it so; the lock cells and
    // `RECTANGLE` are centred on the position too).
    let location = def
        .get("LOCATION")
        .filter(|_| def.is_building())
        .map_or((0, 0), |c| (c.int(0).unwrap_or(0), c.int(1).unwrap_or(0)));
    let parts: Vec<_> = if def.is_building() {
        refs.iter().enumerate().collect()
    } else if refs.is_empty() {
        Vec::new()
    } else {
        vec![(0, &refs[step % refs.len()])]
    };
    let parts = parts
        .into_iter()
        .filter_map(|(i, r)| {
            let set = def.sprite(r.sprite)?;
            let pack = set.gp_path();
            // The frames are direction-interleaved: frame `f` of stored direction `d` is
            // `f·dirs + d` (0x506f13).
            let frame = r.frame.max(0) as usize * dirs + d;
            if frame >= frames_in(&pack)? {
                return None;
            }
            // TODO: the original compares objects against the line (0x49C96A); the line's
            // middle row is used as the sort row here.
            let (layer, row) = match sorts.get(i) {
                Some(PartSort::Ground) => (Layer::Ground, 0),
                Some(&PartSort::Line(_, y1, _, y2)) => (Layer::Sorted, location.1 + (y1 + y2) / 2),
                None => (Layer::Sorted, 0),
            };
            // A mirrored frame is placed at `x − dx` (0x506f5f).
            let dx = set.offset.0 + location.0;
            Some(((pack, frame), (if mirrored { -dx } else { dx }, set.offset.1 + location.1), layer, row))
        })
        .chain(sections.into_iter().flat_map(|stages| section_parts(def, stages, &frames_in)))
        .collect::<Vec<_>>();
    (!parts.is_empty()).then_some(UnitLook { parts, mirrored })
}

/// The frame of a finished building's `#WORK` animation at `step` (wrapping), drawn on top of
/// its `#STANDLO` parts: the mills' turning wheels, the coal mine's hoist. A finished
/// building's current animation (OB+0x10d) is `#WORK` (set with `#STANDLO` and `#STANDHI`
/// when it is done, 0x523924, and at creation, 0x554b8e); the object draw (0x50927a) shows
/// its frame OB+0xec (back to 0 at the frame count) at the picture origin plus the
/// frame's offset, sorted at the ground point plus `ANMSH` (NM+0x2890, 16 by default)
/// and the frame's y offset, and steps the counter by one while the game runs
/// (`0x668f22`, 0x509480). The `#STANDLO` pictures leave holes for it.
pub fn work_part(
    def: &UnitDef,
    step: usize,
    frames_in: impl Fn(&str) -> Option<usize>,
) -> Option<(FrameKey, (i32, i32), Layer, i32)> {
    if !def.is_building() {
        return None;
    }
    let refs = def.animation("#WORK")?.frame_refs();
    let r = refs.get(step % refs.len().max(1))?;
    let set = def.sprite(r.sprite)?;
    let pack = set.gp_path();
    let frame = r.frame.max(0) as usize;
    if frame >= frames_in(&pack)? {
        return None;
    }
    let location = def.get("LOCATION").map_or((0, 0), |c| (c.int(0).unwrap_or(0), c.int(1).unwrap_or(0)));
    // `ANMSH` is not in the keyword table: its value stays among the ignored tokens.
    let shift = def
        .get("ANMSH")
        .and_then(|c| c.int(0).or_else(|| c.ignored.first().and_then(acr_formats::defs::Value::as_int)))
        .unwrap_or(16);
    let offset = (set.offset.0 + location.0, set.offset.1 + location.1);
    Some(((pack, frame), offset, Layer::Sorted, shift + set.offset.1))
}

/// The number of frames of a building's `#WORK` animation (its counter wraps there).
pub fn work_frames(def: &UnitDef) -> Option<usize> {
    def.is_building().then(|| def.animation("#WORK")).flatten().map(|a| a.frame_refs().len()).filter(|&n| n > 0)
}

/// The frames a finished building's sections (`ADDSECTION`) add in the section states
/// `stages` (OB+0xaa, 3 bits per section), as 0x5099a8 in the object draw 0x5062a3 (and
/// 0x5fdb93 zoomed out) adds them after the `#STANDLO` parts: per declared section the
/// frame of its state 0..=3 unless it is `-1`, drawn at the picture origin and sorted at
/// the origin plus the state's point (0x49C3E1 with sort point `origin + (x, y)`).
pub fn section_parts(
    def: &UnitDef,
    stages: u16,
    frames_in: impl Fn(&str) -> Option<usize>,
) -> Vec<(FrameKey, (i32, i32), Layer, i32)> {
    let location = def.get("LOCATION").map_or((0, 0), |c| (c.int(0).unwrap_or(0), c.int(1).unwrap_or(0)));
    def.sections()
        .iter()
        .filter_map(|s| {
            let (frame, (_, sy)) = s.shown(s.state_in(stages))?;
            let set = def.sprite(s.sprite)?;
            let pack = set.gp_path();
            let frame = usize::from(frame);
            (frame < frames_in(&pack)?).then(|| {
                ((pack, frame), (set.offset.0 + location.0, set.offset.1 + location.1), Layer::Sorted, location.1 + sy)
            })
        })
        .collect()
}

/// Per part of animation `name`, its `LINESORT` entry (0x4f9c70): `LINE x1 y1 x2 y2`,
/// `POINT x y` (stored as the line `x y x y`), `GROUND` (the line at −10000) or `TOP` (the
/// line at 10000, after everything around it).
fn part_sorts(def: &UnitDef, name: &str) -> Vec<PartSort> {
    let Some(cmd) = def.all("LINESORT").find(|c| c.word(0).is_some_and(|w| w.eq_ignore_ascii_case(name))) else {
        return Vec::new();
    };
    let values: Vec<&acr_formats::defs::Value> = cmd.args.iter().chain(&cmd.ignored).skip(1).collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < values.len() {
        match values[i].as_word() {
            Some(w) if w.eq_ignore_ascii_case("GROUND") => {
                out.push(PartSort::Ground);
                i += 1;
            }
            Some(w) if w.eq_ignore_ascii_case("LINE") => {
                let n = |k: usize| values.get(i + k).and_then(|v| v.as_int()).unwrap_or(0);
                out.push(PartSort::Line(n(1), n(2), n(3), n(4)));
                i += 5;
            }
            Some(w) if w.eq_ignore_ascii_case("POINT") => {
                let n = |k: usize| values.get(i + k).and_then(|v| v.as_int()).unwrap_or(0);
                out.push(PartSort::Line(n(1), n(2), n(1), n(2)));
                i += 3;
            }
            Some(w) if w.eq_ignore_ascii_case("TOP") => {
                out.push(PartSort::Line(TOP, TOP, TOP, TOP));
                i += 1;
            }
            _ => i += 1,
        }
    }
    out
}

/// The look of a unit type by map id and direction.
pub type LookFn<'a> = dyn Fn(&str, u8) -> Option<UnitLook> + 'a;

/// The placed objects of a map, in draw order.
#[derive(Debug, Default)]
pub struct Scene {
    /// Ground objects first, then sorted ones, each by `y / 2`; stable within equal keys.
    sprites: Vec<SceneSprite>,
    /// Index of the first sorted sprite.
    sorted_from: usize,
    /// Objects without a known look, by kind.
    pub missing: HashMap<String, usize>,
    /// Frames of map objects drawn as models instead (docs/rendering-3d.md "Map trees and
    /// stones"): left out of [`Scene::visible`].
    hidden: HashSet<FrameKey>,
}

impl Scene {
    /// Places the map's objects. `lists` gives the sprite list of each object group,
    /// `looks` the look of a unit type by map id and direction.
    pub fn build(
        map: &M3dMap,
        terrain: &TerrainData,
        lists: &HashMap<SpriteGroup, SpriteList>,
        looks: Option<&LookFn<'_>>,
    ) -> Self {
        let mut missing = HashMap::new();
        let mut sprites = Vec::with_capacity(map.sprites.len() + map.units.len());
        for s in &map.sprites {
            let (Some(list), Some(entry)) =
                (lists.get(&s.group), lists.get(&s.group).and_then(|l| l.entries.get(usize::from(s.index))))
            else {
                *missing.entry(format!("{:?}", s.group)).or_insert(0) += 1;
                continue;
            };
            let layer = if s.group == SpriteGroup::Trees { Layer::Sorted } else { Layer::Ground };
            let h = terrain.height_at(s.x, s.y);
            sprites.push(SceneSprite {
                frame: (list.gp_path(), usize::from(s.index)),
                layer,
                x: s.x,
                y: s.y,
                dx: -entry.dx,
                dy: -entry.dy - h,
                sort: s.y >> 1,
                player: 0,
                transparent: false,
                mirrored: false,
                white: false,
            });
        }
        for u in map.units.iter().filter(|_| looks.is_some()) {
            let looks = looks.expect("filtered");
            let (x, y) = (u.x >> 4, u.y >> 4);
            let Some(look) = looks(&u.name, u.direction) else {
                *missing.entry(u.name.clone()).or_insert(0) += 1;
                continue;
            };
            let h = terrain.height_at(x, y);
            for (frame, (ox, oy), layer, row) in look.parts {
                let sort = (y >> 1) + row;
                sprites.push(SceneSprite {
                    frame,
                    layer,
                    x,
                    y,
                    dx: ox,
                    dy: oy - h,
                    sort,
                    player: u.nation,
                    transparent: false,
                    mirrored: look.mirrored,
                    white: false,
                });
            }
        }
        sprites.sort_by_key(SceneSprite::key);
        let sorted_from = sprites.iter().position(|s| s.layer == Layer::Sorted).unwrap_or(sprites.len());
        Self { sprites, sorted_from, missing, hidden: HashSet::new() }
    }

    /// Leaves the static sprites of these frames out from now on (map objects drawn as
    /// models); an empty set draws every sprite again.
    pub fn set_hidden(&mut self, frames: HashSet<FrameKey>) {
        self.hidden = frames;
    }

    pub fn sprites(&self) -> &[SceneSprite] {
        &self.sprites
    }

    /// Every frame the scene uses, without duplicates.
    pub fn frames(&self) -> Vec<FrameKey> {
        let mut keys: Vec<FrameKey> = self.sprites.iter().map(|s| s.frame.clone()).collect();
        keys.sort();
        keys.dedup();
        keys
    }

    /// Sprites that may be visible through `camera` on an output of `size` pixels, in draw
    /// order, with their frame origin in output pixels.
    pub fn visible(
        &self,
        camera: &Camera,
        size: [f32; 2],
        margin: f32,
    ) -> impl Iterator<Item = (&SceneSprite, [f32; 2])> {
        let view = [size[0] / camera.scale, size[1] / camera.scale];
        let (x0, x1) = (camera.x - margin, camera.x + view[0] + margin);
        // Heights lift objects up to ~900 pixels, frames reach ~300 pixels below their origin.
        let (y0, y1) = ((camera.y - margin) * 2.0, (camera.y + view[1] + margin + 1000.0) * 2.0);
        let (ground, sorted) = self.sprites.split_at(self.sorted_from);
        let rows = |part: &'_ [SceneSprite]| {
            let lo = part.partition_point(|s| s.sort * 2 < y0 as i32);
            let hi = part.partition_point(|s| s.sort * 2 < y1 as i32 + 2000);
            lo..hi
        };
        let (g, s) = (rows(ground), rows(sorted));
        let hidden = &self.hidden;
        ground[g].iter().chain(&sorted[s]).filter_map(move |s| {
            if (s.x as f32) < x0 || (s.x as f32) > x1 || (!hidden.is_empty() && hidden.contains(&s.frame)) {
                return None;
            }
            let [px, py] = camera.project(s.x, s.y, 0);
            let at = [px + s.dx as f32 * camera.scale, py + s.dy as f32 * camera.scale];
            (at[1] < size[1] + margin * camera.scale && at[1] > -margin * 4.0 * camera.scale).then_some((s, at))
        })
    }
}

/// Strips the nation suffix of a map unit id: `EngM18(EN)` → `ENGM18`, the `NAME` of its
/// `.MD` definition.
pub fn definition_name(unit_id: &str) -> String {
    unit_id.split('(').next().unwrap_or(unit_id).trim().to_ascii_uppercase()
}

/// Collects what a map's objects need (sprite lists, unit definitions, sprite packs),
/// independent of how files are read: feed every requested file to [`MapObjects::add_file`]
/// and request the paths it returns.
#[derive(Default)]
pub struct MapObjects {
    lists: HashMap<SpriteGroup, SpriteList>,
    /// `COMPLEX.LST` (fields): its entries name animations of `WEAPON.ADS`.
    complex: Option<SpriteList>,
    /// `.MD` files still to arrive, once `NMLIST.TXT` is read.
    defs_pending: Option<usize>,
    /// Unit definitions by upper-case `NAME`.
    defs: HashMap<String, UnitDef>,
    /// Upper-case object id → upper-case `.MD` `NAME`, from the nations' `[MEMBERS]` where
    /// they differ from the id's own name (`SpnCoa(FR)` is `SpnRudCo`), see
    /// [`MapObjects::set_names`].
    names: HashMap<String, String>,
    /// Requested sprite packs; `None` while loading.
    packs: HashMap<String, Option<acr_formats::GpFile>>,
    /// Packs that could not be read; not requested again (zoomed-out twins are often
    /// missing).
    missing: std::collections::HashSet<String>,
    /// `WEAPON.ADS`, the animations of projectiles, smoke and fires; `None` until loaded.
    effects: Option<UnitDef>,
    /// `WEAPON.ADS` was requested and has not arrived yet.
    effects_pending: bool,
    /// Older run-length packs (`*.RLC`, see [`rlc_frames`]) by path; `None` while loading.
    rlc: HashMap<String, Option<Vec<acr_formats::Frame>>>,
}

/// The frames of an `RLC` pack. Effects whose pixels are all below 32 (`PU3`, `WEXP`) are
/// brightness maps that the engine draws through a 32-row table (`AGEW_1W.GRD` for `WHITE`
/// weapons, 0x4903a2 -> 0x482545); they become darkness levels, the others palette pixels.
pub fn rlc_frames(file: &acr_formats::rlc::RlcFile) -> Vec<acr_formats::Frame> {
    use acr_formats::gp::{Layer, Run};
    let levels = file.items.iter().flat_map(|i| &i.runs).flat_map(|r| &r.pixels).all(|&v| v < 32);
    file.items
        .iter()
        .map(|item| {
            let runs: Vec<Run> = item.runs.iter().map(|r| Run { x: r.x, y: r.y, len: r.pixels.len() as u16 }).collect();
            let pixels: Vec<u8> = item.runs.iter().flat_map(|r| r.pixels.iter().copied()).collect();
            let encoding = if levels { acr_formats::Encoding::Shade8 } else { acr_formats::Encoding::DictLz };
            let layer = Layer {
                encoding,
                x: 0,
                y: 0,
                width: item.width.max(0) as u16,
                height: item.height.max(0) as u16,
                runs,
                pixels,
            };
            acr_formats::Frame { layers: vec![layer] }
        })
        .collect()
}

impl MapObjects {
    /// The animations of `WEAPON.NDS` (projectiles, smoke, explosions, fires).
    pub const EFFECTS: &'static str = "WEAPON.ADS";

    /// Sprite lists of the object groups drawn so far. TODO: `COMPLEX.LST` (fields) and
    /// `ANIMATIONS.LST` (river animations) use a different format and are not drawn yet.
    pub const LISTS: [(SpriteGroup, &'static str); 3] = [
        (SpriteGroup::Trees, "TREELIST.LST"),
        (SpriteGroup::Stones, "STONLIST.LST"),
        (SpriteGroup::Holes, "HOLELIST.LST"),
    ];
    pub const NAME_LIST: &'static str = "NMLIST.TXT";
    /// The fields' list; kept apart from [`MapObjects::LISTS`]: it has no pack of its own.
    pub const COMPLEX_LIST: &'static str = "COMPLEX.LST";

    /// The files to request first.
    pub fn files() -> Vec<&'static str> {
        Self::LISTS.iter().map(|(_, p)| *p).chain([Self::NAME_LIST, Self::COMPLEX_LIST]).collect()
    }

    /// Takes a loaded file (any file; others are ignored) and returns further files to
    /// request.
    pub fn add_file(&mut self, path: &str, bytes: &[u8]) -> Vec<String> {
        let mut more = Vec::new();
        let upper = acr_formats::normalize_path(path);
        if path == Self::COMPLEX_LIST {
            self.complex = SpriteList::parse(&String::from_utf8_lossy(bytes));
        } else if let Some((group, _)) = Self::LISTS.iter().find(|(_, p)| *p == path) {
            if let Some(list) = SpriteList::parse(&String::from_utf8_lossy(bytes)) {
                more.extend(self.want_pack(&list.gp_path()));
                self.lists.insert(*group, list);
            }
        } else if path == Self::EFFECTS && self.effects_pending {
            self.effects_pending = false;
            let def = UnitDef::parse(bytes);
            // A pack is a `.GP` or an older `.RLC` (cannon balls, some explosions); both
            // are asked for, the missing one is skipped.
            let packs: Vec<String> = def.sprites.iter().map(|s| s.gp_path()).collect();
            for gp in packs {
                more.extend(self.want_pack(&gp));
                let rlc = format!("{}.RLC", gp.trim_end_matches(".GP"));
                if !self.rlc.contains_key(&rlc) && !self.missing.contains(&rlc) {
                    self.rlc.insert(rlc.clone(), None);
                    more.push(rlc);
                }
            }
            self.effects = Some(def);
        } else if let Some(slot) = self.rlc.get_mut(path) {
            match acr_formats::rlc::RlcFile::parse(bytes) {
                Ok(f) => *slot = Some(rlc_frames(&f)),
                Err(e) => {
                    log::warn!("{path}: {e}");
                    self.rlc.remove(path);
                    self.missing.insert(path.to_owned());
                }
            }
        } else if path == Self::NAME_LIST {
            let names = acr_formats::defs::name_list(bytes);
            self.defs_pending = Some(names.len());
            more.extend(names.into_iter().map(|n| format!("{}.MD", n.to_ascii_uppercase())));
        } else if let Some(slot) = self.packs.get_mut(path) {
            match acr_formats::GpFile::parse(bytes.to_vec()) {
                Ok(gp) => *slot = Some(gp),
                Err(e) => {
                    log::warn!("{path}: {e}");
                    self.packs.remove(path);
                    self.missing.insert(path.to_owned());
                }
            }
        } else if let Some(n) = self.defs_pending.filter(|_| upper.ends_with(".MD")) {
            let def = UnitDef::parse(bytes);
            if let Some(name) = def.name() {
                self.defs.insert(name.to_ascii_uppercase(), def);
            }
            self.defs_pending = Some(n.saturating_sub(1));
        }
        more
    }

    /// Whether `path` is a file this collection requested.
    pub fn wants(&self, path: &str) -> bool {
        Self::LISTS.iter().any(|(_, p)| *p == path)
            || path == Self::NAME_LIST
            || path == Self::COMPLEX_LIST
            || (path == Self::EFFECTS && self.effects_pending)
            || self.packs.contains_key(path)
            || self.rlc.contains_key(path)
            || (self.defs_pending.is_some_and(|n| n > 0) && acr_formats::normalize_path(path).ends_with(".MD"))
    }

    /// A requested file could not be read: stop waiting for it.
    pub fn file_failed(&mut self, path: &str) {
        if self.packs.remove(path).is_some() || self.rlc.remove(path).is_some() {
            self.missing.insert(path.to_owned());
        }
        if path == Self::EFFECTS {
            self.effects_pending = false;
        }
        if let Some(n) = self.defs_pending.filter(|_| acr_formats::normalize_path(path).ends_with(".MD")) {
            self.defs_pending = Some(n.saturating_sub(1));
        }
    }

    /// Requests a pack and its zoomed-out twin ([`zoom_pack`]); returns the new paths.
    fn want_pack(&mut self, path: &str) -> Vec<String> {
        let mut out = Vec::new();
        for p in std::iter::once(path.to_owned()).chain(zoom_pack(path)) {
            if !self.packs.contains_key(&p) && !self.missing.contains(&p) {
                self.packs.insert(p.clone(), None);
                out.push(p);
            }
        }
        out
    }

    /// The definition of a map unit id such as `EngM18(EN)`: by its nation's `[MEMBERS]`
    /// entry when one was given ([`MapObjects::set_names`]), else by the id's name.
    pub fn definition(&self, unit_id: &str) -> Option<&UnitDef> {
        let named = self.names.get(&unit_id.to_ascii_uppercase()).and_then(|n| self.defs.get(n));
        named.or_else(|| self.defs.get(&definition_name(unit_id)))
    }

    /// The `.MD` names of object ids (the nations' `[MEMBERS]`, e.g. from the simulation's
    /// catalog). Without them, ids whose name differs from their definition's (the Spanish
    /// coal, iron and stone mines `SpnCoa`, `SpnIro`, `SpnStn`) have no picture.
    pub fn set_names<'a>(&mut self, names: impl IntoIterator<Item = (&'a str, &'a str)>) {
        for (id, md) in names {
            self.names.insert(id.to_ascii_uppercase(), md.to_ascii_uppercase());
        }
    }

    /// A loaded sprite pack.
    pub fn pack(&self, path: &str) -> Option<&acr_formats::GpFile> {
        self.packs.get(path).and_then(Option::as_ref)
    }

    /// Frames of a loaded `.GP` or `.RLC` pack.
    fn frame_count(&self, path: &str) -> Option<usize> {
        self.pack(path)
            .map(acr_formats::GpFile::frame_count)
            .or_else(|| self.rlc.get(path).and_then(Option::as_ref).map(Vec::len))
    }

    fn decode_one(&self, path: &str, frame: usize) -> Option<acr_formats::Frame> {
        match self.pack(path) {
            Some(g) => g.decode_frame(frame).ok(),
            None => self.rlc.get(path)?.as_ref()?.get(frame).cloned(),
        }
    }

    /// Requests `WEAPON.ADS` and, once it is there, its packs; returns the paths to load.
    pub fn want_effects(&mut self) -> Vec<String> {
        if self.effects.is_some() || self.effects_pending {
            return Vec::new();
        }
        self.effects_pending = true;
        vec![Self::EFFECTS.to_owned()]
    }

    /// `WEAPON.ADS`, once loaded.
    pub fn effects(&self) -> Option<&UnitDef> {
        self.effects.as_ref()
    }

    /// The animation of a field type (`COMPLEX.LST` entry `index`, `KUK00`..`KUK05` by growth
    /// stage) and its step at `tick`: the drawing code (0x4d0a22) takes `tick / pause` modulo
    /// the animation's length.
    pub fn complex_step(&self, index: usize, tick: u64) -> Option<(&str, usize)> {
        let entry = self.complex.as_ref()?.entries.get(index)?;
        let anim = entry.animation.as_deref()?;
        let len = self.effects.as_ref()?.animation(anim)?.frame_refs().len();
        Some((anim, (tick / entry.pause.max(1) as u64) as usize % len.max(1)))
    }

    /// The picture of a field type at `tick`: its animation's frame and the `COMPLEX.LST`
    /// offset `(−DX, −DY)` of the ground point (0x4d0a74).
    pub fn complex_look(&self, index: usize, tick: u64) -> Option<(FrameKey, (i32, i32))> {
        let (anim, step) = self.complex_step(index, tick)?;
        let entry = &self.complex.as_ref()?.entries[index];
        let (frame, _) = self.effect_look(anim, step, 0)?;
        Some((frame, (-entry.dx, -entry.dy)))
    }

    /// Frame `step` (clamped to the last, as 0x557d98 does) of the effect animation `anim`
    /// (`#SHOTFOG`) in direction `dir` (0 when it has one), with its offset. An animation of
    /// `n` directions keeps its directions together: sprite `f` of direction `d` is
    /// `f * n + d` in the pack (0x557f09).
    pub fn effect_look(&self, anim: &str, step: usize, dir: usize) -> Option<(FrameKey, (i32, i32))> {
        let def = self.effects.as_ref()?;
        let a = def.animation(anim)?;
        let refs = a.frame_refs();
        let r = refs.get(step.min(refs.len().checked_sub(1)?))?;
        let set = def.sprite(r.sprite)?;
        let gp = set.gp_path();
        let pack = if self.pack(&gp).is_some() { gp } else { format!("{}.RLC", gp.trim_end_matches(".GP")) };
        let dirs = a.directions.max(1) as usize;
        let frame = r.frame.max(0) as usize * dirs + dir.min(dirs - 1);
        (frame < self.frame_count(&pack)?).then_some(((pack, frame), set.offset))
    }

    /// Requests the packs `map` needs; returns the paths to load. Call when the
    /// definitions are complete ([`MapObjects::definitions_ready`]).
    pub fn packs_for(&mut self, map: &M3dMap) -> Vec<String> {
        let mut names: Vec<&str> = map.units.iter().map(|u| u.name.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        let packs: Vec<String> = names.iter().filter_map(|n| self.definition(n)).flat_map(unit_packs).collect();
        packs.iter().flat_map(|p| self.want_pack(p)).collect()
    }

    /// Requests the packs of object types (map unit ids such as `EngSkl(EN)`) that appear
    /// during the game; returns the paths to load.
    pub fn packs_for_units<'a>(&mut self, ids: impl IntoIterator<Item = &'a str>) -> Vec<String> {
        let packs: Vec<String> = ids.into_iter().filter_map(|id| self.definition(id)).flat_map(unit_packs).collect();
        packs.iter().flat_map(|p| self.want_pack(p)).collect()
    }

    pub fn definitions_ready(&self) -> bool {
        self.defs_pending == Some(0) && self.lists.len() == Self::LISTS.len() && !self.effects_pending
    }

    /// Whether every requested pack has arrived.
    pub fn packs_ready(&self) -> bool {
        self.packs.values().all(Option::is_some) && self.rlc.values().all(Option::is_some)
    }

    /// The look of a unit type facing `direction`, from its definition and packs.
    pub fn unit_look(&self, unit_id: &str, direction: u8) -> Option<UnitLook> {
        unit_look(self.definition(unit_id)?, direction, |p| self.frame_count(p))
    }

    /// Places the objects of `map`; call when [`MapObjects::packs_ready`].
    pub fn scene(&self, map: &M3dMap, terrain: &TerrainData) -> Scene {
        Scene::build(map, terrain, &self.lists, Some(&|id, dir| self.unit_look(id, dir)))
    }

    /// Places only the static objects of `map` (trees, stones, ...); units and buildings
    /// come from the simulation as dynamic sprites.
    pub fn static_scene(&self, map: &M3dMap, terrain: &TerrainData) -> Scene {
        Scene::build(map, terrain, &self.lists, None)
    }

    /// The look of `unit_id` playing animation `anim` (e.g. `#MOTION_L`) at frame `step`,
    /// falling back to the idle animation; a finished building adds its sections in the
    /// states `sections` ([`section_parts`]).
    pub fn animation_look(
        &self,
        unit_id: &str,
        anim: &str,
        direction: u8,
        step: usize,
        sections: Option<u16>,
    ) -> Option<UnitLook> {
        let def = self.definition(unit_id)?;
        let animation = def.animation(anim).or_else(|| idle_animation(def))?;
        animation_look(def, animation, direction, step, sections, |p| self.frame_count(p))
    }

    /// The `#WORK` frame of a finished building of type `unit_id` at `step` ([`work_part`]).
    pub fn work_part(&self, unit_id: &str, step: usize) -> Option<(FrameKey, (i32, i32), Layer, i32)> {
        work_part(self.definition(unit_id)?, step, |p| self.frame_count(p))
    }

    /// The number of `#WORK` frames of a building type ([`work_frames`]).
    pub fn work_frames(&self, unit_id: &str) -> Option<usize> {
        work_frames(self.definition(unit_id)?)
    }

    /// An empty atlas; frames are added as they are drawn ([`SceneAtlas::draws_uploading`]).
    pub fn atlas(&self, renderer: &crate::SpriteRenderer, device: &wgpu::Device, queue: &wgpu::Queue) -> SceneAtlas {
        SceneAtlas { atlas: renderer.upload(device, queue, &[]), index: HashMap::new(), budget: ATLAS_PAGES }
    }

    /// Adds those of `frames` that `atlas` lacks and whose pack is loaded, without
    /// uploading the rest again.
    pub fn upload_more(
        &self,
        renderer: &crate::SpriteRenderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        atlas: &mut SceneAtlas,
        frames: Vec<FrameKey>,
    ) {
        let mut seen = HashSet::new();
        let mut keys = Vec::new();
        let mut decoded = Vec::new();
        for key in frames {
            if atlas.index.contains_key(&key) || !seen.insert(key.clone()) {
                continue;
            }
            if let Some(frame) = self.decode_one(&key.0, key.1) {
                keys.push(key);
                decoded.push(frame);
            }
        }
        if keys.is_empty() {
            return;
        }
        let base = atlas.atlas.frame_count();
        renderer.extend(device, queue, &mut atlas.atlas, &decoded);
        atlas.index.extend(keys.into_iter().enumerate().map(|(i, k)| (k, base + i)));
    }

    /// The sprite list of a map object group (`TREELIST.LST`, ...), once loaded.
    pub fn list(&self, group: SpriteGroup) -> Option<&SpriteList> {
        self.lists.get(&group)
    }

    /// The opaque box `(min_x, min_y, max_x, max_y)` of a loaded frame.
    pub fn frame_bounds(&self, (pack, frame): &FrameKey) -> Option<(i32, i32, i32, i32)> {
        self.decode_one(pack, *frame)?.bounds()
    }

    /// The `L_Mode` twin of a frame, if its pack is loaded and has the frame.
    fn zoom_twin(&self, (pack, frame): &FrameKey) -> Option<FrameKey> {
        let z = zoom_pack(pack)?;
        (*frame < self.pack(&z)?.frame_count()).then_some((z, *frame))
    }
}

/// Atlas pages (8 MB each) a [`SceneAtlas`] may fill before it starts over.
const ATLAS_PAGES: usize = 24;

/// The uploaded frames of a scene. Frames go in when they are first drawn: uploading every
/// frame of every unit pack up front took 334 pages of 8 MB on Miss7F03, more than the GPU
/// of a small machine could allocate (pages went missing: rectangles cut out of sprites).
/// Once the atlas fills its budget it starts over with the frames drawn at that moment.
pub struct SceneAtlas {
    pub atlas: crate::SpriteAtlas,
    index: HashMap<FrameKey, usize>,
    /// Pages after which the atlas starts over: [`ATLAS_PAGES`], or twice what the frames
    /// of one view took after the last start, so a large view does not start over again
    /// and again.
    budget: usize,
}

/// How sprites sit in the 3D pass ([`SceneAtlas::placed_draws`]): their depth against the
/// models and, with the free camera, their place on the screen.
#[derive(Clone, Copy)]
pub struct Placement<'a> {
    pub view: &'a View3d,
    pub terrain: &'a TerrainData,
    /// Every sprite in front of the models (comparing models with sprites).
    pub on_top: bool,
    /// The fog of war's share of black at a world ground point (`x`, `y`), for sprites under
    /// the free camera's world fog (#309).
    pub fog: Option<&'a dyn Fn(i32, i32) -> f32>,
}

impl Placement<'_> {
    /// Depth of a sprite: ground sprites (and those the zoomed-out view bakes into its ground)
    /// lie behind every model; the others stand at their sort row, `2 · sort` in world rows,
    /// like the original's row buckets.
    fn depth(&self, s: &SceneSprite, zoomed: bool, h: i32) -> f32 {
        if self.on_top {
            return 0.0;
        }
        if s.layer == Layer::Ground || (zoomed && baked_in_zoom(s)) {
            return 1.0;
        }
        self.view.sprite_depth([s.x as f32, (2 * s.sort) as f32, h as f32 / crate::view3d::COS_E])
    }
}

impl SceneAtlas {
    /// [`SceneAtlas::placed_draws`] after uploading the frames they need that are not in the
    /// atlas yet (in the zoomed-out view the `L_Mode` twins where `assets` has them).
    #[allow(clippy::too_many_arguments)]
    pub fn draws_uploading(
        &mut self,
        assets: &MapObjects,
        renderer: &crate::SpriteRenderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &Scene,
        dynamic: &[SceneSprite],
        camera: &Camera,
        size: [f32; 2],
        zoomed: bool,
        placement: Option<Placement<'_>>,
    ) -> Vec<crate::SpriteDraw> {
        let order = placed_order(scene, dynamic, camera, size, zoomed, placement);
        let missing = |index: &HashMap<FrameKey, usize>| -> Vec<FrameKey> {
            order
                .iter()
                .filter_map(|(s, ..)| match zoomed.then(|| assets.zoom_twin(&s.frame)).flatten() {
                    Some(twin) => (!index.contains_key(&twin)).then_some(twin),
                    None => (!index.contains_key(&s.frame)).then(|| s.frame.clone()),
                })
                .collect()
        };
        let mut new = missing(&self.index);
        if !new.is_empty() && self.atlas.page_count() >= self.budget {
            let budget = self.budget;
            *self = assets.atlas(renderer, device, queue);
            new = missing(&self.index);
            assets.upload_more(renderer, device, queue, self, new);
            self.budget = budget.max(2 * self.atlas.page_count());
            log::info!("sprite atlas started over: {} frames on {} pages", self.index.len(), self.atlas.page_count());
        } else if !new.is_empty() {
            assets.upload_more(renderer, device, queue, self, new);
        }
        self.draws_from(order, camera, zoomed, placement)
    }

    /// The draws for the visible part of `scene` plus `dynamic` sprites (e.g. moving units
    /// from the simulation, any order), merged into the original's draw order.
    ///
    /// In the zoomed-out view (`zoomed`, 0x5fbac1) a sprite with a `L_Mode` twin is drawn
    /// from it at its own size, its offset divided by 4 (`>> 2`); others are scaled down.
    /// Stones, holes and then trees come first: the original bakes them into its cached
    /// ground picture (0x5fea06, [`baked_in_zoom`]), below every unit and building.
    pub fn draws(
        &self,
        scene: &Scene,
        dynamic: &[SceneSprite],
        camera: &Camera,
        size: [f32; 2],
        zoomed: bool,
    ) -> Vec<crate::SpriteDraw> {
        self.placed_draws(scene, dynamic, camera, size, zoomed, None)
    }

    /// [`SceneAtlas::draws`] for the 3D pass: each draw gets its depth, and with the free
    /// camera the sprites stand upright at their projected ground points, ordered far to near.
    pub fn placed_draws(
        &self,
        scene: &Scene,
        dynamic: &[SceneSprite],
        camera: &Camera,
        size: [f32; 2],
        zoomed: bool,
        placement: Option<Placement<'_>>,
    ) -> Vec<crate::SpriteDraw> {
        self.draws_from(placed_order(scene, dynamic, camera, size, zoomed, placement), camera, zoomed, placement)
    }

    fn draws_from(
        &self,
        placed: Vec<(&SceneSprite, [f32; 2], i32)>,
        camera: &Camera,
        zoomed: bool,
        placement: Option<Placement<'_>>,
    ) -> Vec<crate::SpriteDraw> {
        let mut out = Vec::with_capacity(placed.len());
        for (s, [ax, ay], h) in placed {
            let depth = placement.map_or(0.0, |p| p.depth(s, zoomed, h));
            let fog = placement.and_then(|p| p.fog).map_or(0.0, |f| f(s.x, s.y));
            let twin =
                zoomed.then(|| zoom_pack(&s.frame.0)).flatten().and_then(|p| self.index.get(&(p, s.frame.1)).copied());
            let (frame, x, y, scale) = if let Some(frame) = twin {
                (frame, ax + (s.dx >> 2) as f32, ay + ((s.dy + h) >> 2) as f32, 1.0)
            } else if let Some(&frame) = self.index.get(&s.frame) {
                (frame, ax + s.dx as f32 * camera.scale, ay + (s.dy + h) as f32 * camera.scale, camera.scale)
            } else {
                continue;
            };
            out.push(crate::SpriteDraw {
                frame,
                x: x.round(),
                y: y.round(),
                scale,
                player: s.player,
                transparent: s.transparent,
                mirrored: s.mirrored,
                white: s.white,
                depth,
                fog,
            });
        }
        out
    }
}

/// Per sprite in draw order: its ground point on the screen without the sprite offset, and
/// the ground height the offset `dy` already contains (0 when the anchor is the flat
/// projection `(x, y / 2)`).
fn placed_order<'a>(
    scene: &'a Scene,
    dynamic: &'a [SceneSprite],
    camera: &Camera,
    size: [f32; 2],
    zoomed: bool,
    placement: Option<Placement<'_>>,
) -> Vec<(&'a SceneSprite, [f32; 2], i32)> {
    match placement {
        Some(p) if p.view.is_free() => free_order(scene, dynamic, p, size, zoomed),
        _ => draw_order(scene, dynamic, camera, size, zoomed)
            .into_iter()
            .map(|(s, _)| (s, camera.project(s.x, s.y, 0), 0))
            .collect(),
    }
}

/// The free camera's sprites: every one whose ground point projects near the window, as
/// `(sprite, ground point on the screen, ground height)`, ground sprites first, then far to
/// near (the row order of the original no longer holds once the view turns).
fn free_order<'a>(
    scene: &'a Scene,
    dynamic: &'a [SceneSprite],
    placement: Placement<'_>,
    size: [f32; 2],
    zoomed: bool,
) -> Vec<(&'a SceneSprite, [f32; 2], i32)> {
    let view = placement.view;
    let margin = 1024.0 * view.camera.scale;
    // (sprite, ground point on the screen, ground height, (not flat, −depth))
    type Keyed<'s> = (&'s SceneSprite, [f32; 2], i32, (bool, f32));
    let mut out: Vec<Keyed<'_>> = scene
        .sprites
        .iter()
        .chain(dynamic)
        .filter_map(|s| {
            let h = placement.terrain.height_at(s.x, s.y);
            let p = view.project(crate::view3d::ground_point(s.x, s.y, h));
            let inside = p[0] > -margin && p[0] < size[0] + margin && p[1] > -margin && p[1] < size[1] + margin;
            let flat = s.layer == Layer::Ground || (zoomed && baked_in_zoom(s));
            inside.then_some((s, [p[0], p[1]], h, (!flat, -p[2])))
        })
        .collect();
    out.sort_by(|a, b| a.3.0.cmp(&b.3.0).then(a.3.1.total_cmp(&b.3.1)));
    out.into_iter().map(|(s, at, h, _)| (s, at, h)).collect()
}

/// The visible part of `scene` and the `dynamic` sprites in draw order, with their frame
/// origin in output pixels (see [`SceneAtlas::draws`]).
fn draw_order<'a>(
    scene: &'a Scene,
    dynamic: &'a [SceneSprite],
    camera: &Camera,
    size: [f32; 2],
    zoomed: bool,
) -> Vec<(&'a SceneSprite, [f32; 2])> {
    let mut extra: Vec<(&SceneSprite, [f32; 2])> = dynamic
        .iter()
        .map(|s| {
            let [px, py] = camera.project(s.x, s.y, 0);
            (s, [px + s.dx as f32 * camera.scale, py + s.dy as f32 * camera.scale])
        })
        .collect();
    extra.sort_by_key(|(s, _)| s.key());
    let key = SceneSprite::key;
    let mut extra = extra.into_iter().peekable();
    let mut out = Vec::new();
    if zoomed {
        out.extend(scene.visible(camera, size, 256.0).filter(|(s, _)| baked_in_zoom(s)));
    }
    for (s, at) in scene.visible(camera, size, 256.0).filter(|(s, _)| !(zoomed && baked_in_zoom(s))) {
        while let Some(e) = extra.next_if(|(e, _)| key(e) <= key(s)) {
            out.push(e);
        }
        out.push((s, at));
    }
    out.extend(extra);
    out
}

/// Whether the zoomed-out view draws `sprite` into its ground picture (0x5fdddd): the
/// objects of `STONLIST` and `HOLELIST` (flat, in list order) and `TREELIST` (sorted by
/// row), from the twins `L_Mode\Stones`, `L_Mode\Holes` and `L_Mode\Der`.
fn baked_in_zoom(sprite: &SceneSprite) -> bool {
    matches!(acr_formats::normalize_path(&sprite.frame.0).as_str(), "STONES.GP" | "HOLES.GP" | "TREES.GP")
}

#[cfg(test)]
mod tests {
    use super::*;

    const MD: &str = "NAME EngCen\nBUILDING\nUSERLC 0 EngCen SHADOW 0 0\n#STANDLO 1 3 0 4 0 5 0 6\n\
                      LINESORT #STANDLO GROUND LINE 119 559 580 458 LINE 121 559 204 602\n[END]\n";
    const UNIT: &str = "NAME AnmBiz\nUSERLC 0 AnmBizC SHADOW -160 -132\n#STAND 9 1 0 2\n[END]\n";

    fn sprite(layer: Layer, sort: i32) -> SceneSprite {
        SceneSprite {
            frame: ("A.GP".into(), 0),
            layer,
            x: 0,
            y: sort * 2,
            dx: 0,
            dy: 0,
            sort,
            player: 0,
            transparent: false,
            mirrored: false,
            white: false,
        }
    }

    #[test]
    fn zoom_packs_are_root_packs_in_l_mode() {
        assert_eq!(zoom_pack("EngKre.gp").as_deref(), Some("L_MODE\\ENGKRE.GP"));
        assert_eq!(zoom_pack("TREES.GP").as_deref(), Some("L_MODE\\DER.GP"));
        assert_eq!(zoom_pack("INTERFACE\\CURSGO.GP"), None);
        assert_eq!(zoom_pack("L_MODE\\STONES.GP"), None);
    }

    #[test]
    fn unit_ids_name_definitions() {
        assert_eq!(definition_name("EngM18(EN)"), "ENGM18");
        assert_eq!(definition_name("AnmBiz"), "ANMBIZ");
    }

    #[test]
    fn member_ids_find_differently_named_definitions() {
        let mut objects = MapObjects::default();
        let md = "NAME SpnRudCo
BUILDING
USERLC 0 SpnRudCo SHADOW 0 0
#STANDLO 1 2 0 2 0 3
[END]
";
        objects.defs.insert("SPNRUDCO".into(), UnitDef::parse(md.as_bytes()));
        assert!(objects.definition("SpnCoa(FR)").is_none());
        objects.set_names([("SpnCoa(FR)", "SpnRudCo")]);
        assert!(objects.definition("SpnCoa(FR)").is_some());
        assert!(objects.definition("SpnRudCo(SP)").is_some(), "the id's own name still works");
        assert_eq!(objects.packs_for_units(["SpnCoa(FR)"]), ["SPNRUDCO.GP", r"L_MODE\SPNRUDCO.GP"]);
    }

    /// ENGMEL.MD: the mill's picture leaves a hole for its turning wheel.
    const MILL: &str = "NAME EngMel
BUILDING
USERLC 0 EngMel SHADOW 0 0
USERLC 1 OEng SHADOW 0 0
                        SETANMPARAM -254 -285 1 0
LOCATION -254 -285 588 498
#STANDLO 1 2 0 4 0 5
                        #WORK 1 4 0 9 0 10 0 8 0 28
[END]
";

    #[test]
    fn finished_buildings_play_their_work_animation_on_top() {
        let def = UnitDef::parse(MILL.as_bytes());
        assert_eq!(work_frames(&def), Some(4));
        let at = |step| work_part(&def, step, |_| Some(30)).map(|(f, o, layer, row)| (f.1, o, layer, row));
        // At the picture origin, sorted 16 rows (`ANMSH`) below the ground point.
        assert_eq!(at(0), Some((9, (-254, -285), Layer::Sorted, 16)));
        assert_eq!(at(3).unwrap().0, 28);
        assert_eq!(at(5).unwrap().0, 10, "the counter wraps at the frame count");
        assert_eq!(work_part(&def, 3, |_| Some(28)), None, "frames beyond the pack draw nothing");
        // A map's mill and the placement preview show the first frame (0x4be467).
        let look = unit_look(&def, 0, |_| Some(30)).unwrap();
        assert_eq!(look.parts.iter().map(|p| p.0.1).collect::<Vec<_>>(), [4, 5, 9]);
        assert!(unit_packs(&def).contains(&"ENGMEL.GP".to_owned()));
        let shifted = UnitDef::parse(
            MILL.replace(
                "[END]",
                "ANMSH 40
[END]",
            )
            .as_bytes(),
        );
        assert_eq!(work_part(&shifted, 0, |_| Some(30)).unwrap().3, 40);
        // Units have no such layer.
        let unit = UnitDef::parse(
            UNIT.replace(
                "#STAND",
                "#WORK 1 1 0 2
#STAND",
            )
            .as_bytes(),
        );
        assert_eq!(work_part(&unit, 0, |_| Some(30)), None);
    }

    #[test]
    fn building_parts_follow_linesort() {
        let def = UnitDef::parse(MD.as_bytes());
        let look = unit_look(&def, 0, |_| Some(10)).unwrap();
        let parts: Vec<_> = look.parts.iter().map(|(f, _, layer, row)| (f.1, *layer, *row)).collect();
        assert_eq!(parts, vec![(4, Layer::Ground, 0), (5, Layer::Sorted, 508), (6, Layer::Sorted, 580)]);
    }

    #[test]
    fn building_pictures_start_at_location() {
        let md = MD.replace(
            "BUILDING
",
            "BUILDING
USERLC 1 EngCenB SHADOW 0 0
LOCATION -400 -300 900 700
#BUILDLO_0 1 1 1 0
",
        );
        let def = UnitDef::parse(md.as_bytes());
        let look = unit_look(&def, 0, |_| Some(10)).unwrap();
        assert!(look.parts.iter().all(|p| p.1 == (-400, -300)));
        // The sort lines are picture rows, so they move with the picture.
        assert_eq!(look.parts[1].3, 508 - 300);
        let packs = unit_packs(&def);
        assert_eq!(packs.len(), 2, "construction stages need their pack too: {packs:?}");
    }

    /// ENGKRE.MD: the wooden fort's front part is section 0's state-0 frame.
    const FORT: &str = "NAME EngKre\nBUILDING\nUSERLC 0 EngKre SHADOW 0 0\nLOCATION -496 -474 999 727\n\
                        #STANDLO 1 6 0 4 0 5 0 6 0 7 0 8 0 9\n#BUILDLO_0 1 1 0 0\n\
                        ADDSECTION 0 0 10000\n 10 296 513\n 11 296 513\n 12 296 513\n 13 296 513\n\
                        ADDSECTION 1 0 0\n -1 0 0\n 14 100 200\n 15 100 200\n -1 0 0\n[END]\n";

    #[test]
    fn finished_buildings_draw_their_sections() {
        let def = UnitDef::parse(FORT.as_bytes());
        let frames = |stages: Option<u16>| -> Vec<(usize, (i32, i32), i32)> {
            let anim = def.animation("#STANDLO").unwrap();
            let look = animation_look(&def, anim, 0, 0, stages, |_| Some(20)).unwrap();
            look.parts.iter().map(|(f, o, _, row)| (f.1, *o, *row)).collect()
        };
        let main: Vec<usize> = (4..=9).collect();
        let at = (-496, -474);
        // A map's fort: every section in state 0, section 1 shows nothing yet.
        let look = unit_look(&def, 0, |_| Some(20)).unwrap();
        let shown: Vec<usize> = look.parts.iter().map(|p| p.0.1).collect();
        assert_eq!(shown, [main.clone(), vec![10]].concat());
        assert_eq!(frames(Some(0))[6], (10, at, 513 - 474));
        // Section 0 being built, done, destroyed; section 1 done (bit 15 marks sectioned objects).
        let last = |stages| frames(Some(stages)).iter().skip(6).map(|p| p.0).collect::<Vec<_>>();
        assert_eq!(last(0x8000 | 1), [11]);
        assert_eq!(last(0x8000 | 2), [12]);
        assert_eq!(last(0x8000 | 3), [13]);
        assert_eq!(last(0x8000 | 2 | 2 << 3), [12, 15]);
        assert_eq!(frames(Some(2 << 3))[7], (15, at, 200 - 474));
        // Unknown states draw nothing; under construction or dying no sections at all.
        assert_eq!(last(4 | 4 << 3), Vec::<usize>::new());
        assert_eq!(frames(None).len(), 6);
        // A pack without the frame draws nothing.
        let anim = def.animation("#STANDLO").unwrap();
        assert_eq!(animation_look(&def, anim, 0, 0, Some(0), |_| Some(10)).unwrap().parts.len(), 6);
    }

    #[test]
    fn points_and_top_take_their_part() {
        // ENGKRE.MD: a `POINT` between the lines must not shift the later parts.
        let def = UnitDef::parse(FORT.replace("[END]", "LINESORT #STANDLO GROUND LINE 82 432 919 461 POINT 701 601              LINE 630 572 783 562 TOP LINE 356 682 544 700
[END]").as_bytes());
        let look = unit_look(&def, 0, |_| Some(20)).unwrap();
        let rows: Vec<(usize, Layer, i32)> = look.parts.iter().take(6).map(|p| (p.0.1, p.2, p.3 + 474)).collect();
        assert_eq!(
            rows,
            [
                (4, Layer::Ground, 474),
                (5, Layer::Sorted, 446),
                (6, Layer::Sorted, 601),
                (7, Layer::Sorted, 567),
                (8, Layer::Sorted, TOP),
                (9, Layer::Sorted, 691)
            ]
        );
    }

    #[test]
    fn sections_alone_make_a_building() {
        // INCDO3.MD: `#STANDLO 1 0`, the whole house is two sections.
        let def = UnitDef::parse(
            b"NAME IncDo3\nBUILDING\nUSERLC 0 IncDo3 SHADOW 0 0\n#STANDLO 1 0\n\
              ADDSECTION 0 0 10000 4 239 234 5 239 234 6 239 234 7 239 234\n\
              ADDSECTION 1 0 10000 8 199 307 9 199 307 10 199 307 11 199 307\n[END]\n",
        );
        let look = unit_look(&def, 0, |_| Some(12)).unwrap();
        let shown: Vec<(usize, i32)> = look.parts.iter().map(|p| (p.0.1, p.3)).collect();
        assert_eq!(shown, [(4, 234), (8, 307)]);
        assert_eq!(unit_packs(&def), ["INCDO3.GP"]);
    }

    #[test]
    fn units_pick_an_interleaved_direction() {
        let def = UnitDef::parse(UNIT.as_bytes());
        // 9 directions of 4 frames, interleaved: frame 2 of stored direction k is 2·9 + k.
        let look = |dir| unit_look(&def, dir, |_| Some(36)).unwrap();
        let frame = |dir| (look(dir).parts[0].0.1, look(dir).mirrored);
        assert_eq!(frame(64), (18, false), "south is stored direction 0");
        assert_eq!(frame(128), (22, false), "west is stored direction 4");
        assert_eq!(frame(0), (22, true), "east mirrors west");
        assert_eq!(frame(192), (26, true), "north is stored direction 8");
        assert_eq!(look(128).parts[0].1, (-160, -132));
        assert_eq!(look(0).parts[0].1, (160, -132), "a mirrored frame is placed at x − dx");
        // A frame beyond the pack is not drawn.
        assert!(unit_look(&def, 0, |_| Some(20)).is_none());
    }

    #[test]
    fn directions_follow_the_drawing_code() {
        // 9 stored directions: 16 sectors offset by 7, the east half mirrored.
        let nine: Vec<_> = (0..16).map(|s| sprite_direction((s * 16) as u8, 9, false)).collect();
        assert_eq!(nine[0], (4, true));
        assert_eq!(nine[4], (0, false));
        assert_eq!(nine[8], (4, false));
        assert_eq!(nine[12], (8, true));
        // RealDir 57 still shows the east-southeast frame; 58 is the next sector (64 − 7 + 1).
        assert_eq!(sprite_direction(56, 9, false), (1, true));
        assert_eq!(sprite_direction(57, 9, false), (0, false));
        // Every stored direction is reached and none lies outside.
        for d in 0..=255u8 {
            assert!(sprite_direction(d, 9, false).0 < 9);
            assert!(sprite_direction(d, 16, false).0 < 16);
            assert!(sprite_direction(d, 16, true).0 < 16);
            assert!(sprite_direction(d, 65, false).0 < 65);
        }
        // 16 stored directions: 32 sectors over a half circle, south is 0; west and east fall
        // between two stored directions (8, and 7 mirrored).
        assert_eq!(sprite_direction(64, 16, false), (0, false));
        assert_eq!(sprite_direction(128, 16, false), (8, false));
        assert_eq!(sprite_direction(0, 16, false), (7, true));
        // FULLROTATION: 16 directions around, south first, west at 4, never mirrored.
        assert_eq!(sprite_direction(64, 16, true), (0, false));
        assert_eq!(sprite_direction(128, 16, true), (4, false));
        assert_eq!(sprite_direction(0, 16, true), (12, false));
        assert_eq!(sprite_direction(55, 16, true), (15, false));
        // One direction (buildings) is always frame 0, unmirrored.
        assert_eq!(sprite_direction(200, 1, false), (0, false));
    }

    #[test]
    fn dynamic_sprites_merge_into_draw_order() {
        let mut scene = Scene {
            sprites: vec![sprite(Layer::Ground, 5), sprite(Layer::Sorted, 10), sprite(Layer::Sorted, 30)],
            sorted_from: 1,
            missing: HashMap::new(),
            hidden: HashSet::new(),
        };
        let mut extra = [sprite(Layer::Sorted, 20)];
        extra[0].frame.1 = 1;
        let order: Vec<i32> = {
            let camera = Camera { x: -100.0, y: -100.0, scale: 1.0 };
            let visible: Vec<_> = scene.visible(&camera, [800.0, 600.0], 256.0).map(|(s, _)| s.sort).collect();
            assert_eq!(visible, vec![5, 10, 30]);
            let mut merged: Vec<&SceneSprite> = scene.sprites().iter().collect();
            merged.push(&extra[0]);
            merged.sort_by_key(|s| s.key());
            merged.iter().map(|s| s.sort).collect()
        };
        assert_eq!(order, vec![5, 10, 20, 30]);
        // Frames drawn as models instead (map objects) are left out.
        scene.set_hidden(HashSet::from([("A.GP".to_owned(), 0)]));
        let camera = Camera { x: -100.0, y: -100.0, scale: 1.0 };
        assert_eq!(scene.visible(&camera, [800.0, 600.0], 256.0).count(), 0);
    }

    #[test]
    fn zoomed_view_draws_stones_and_trees_first() {
        let named = |pack: &str, layer, sort| SceneSprite { frame: (pack.into(), 0), ..sprite(layer, sort) };
        let scene = Scene {
            sprites: vec![
                named("STONES.GP", Layer::Ground, 5),
                named("ENGCEN.GP", Layer::Ground, 6),
                named("TREES.GP", Layer::Sorted, 10),
                named("ENGCEN.GP", Layer::Sorted, 12),
                named("TREES.GP", Layer::Sorted, 30),
            ],
            sorted_from: 2,
            missing: HashMap::new(),
            hidden: HashSet::new(),
        };
        let unit = [named("ENGM18.GP", Layer::Sorted, 20)];
        let camera = Camera { x: -100.0, y: -100.0, scale: 1.0 };
        let order = |zoomed| -> Vec<(String, i32)> {
            draw_order(&scene, &unit, &camera, [800.0, 600.0], zoomed)
                .iter()
                .map(|(s, _)| (s.frame.0.clone(), s.sort))
                .collect()
        };
        let names = |o: Vec<(String, i32)>| o.into_iter().map(|(n, s)| format!("{n}{s}")).collect::<Vec<_>>().join(" ");
        assert_eq!(names(order(false)), "STONES.GP5 ENGCEN.GP6 TREES.GP10 ENGCEN.GP12 ENGM18.GP20 TREES.GP30");
        assert_eq!(names(order(true)), "STONES.GP5 TREES.GP10 TREES.GP30 ENGCEN.GP6 ENGCEN.GP12 ENGM18.GP20");
    }
}

#[cfg(test)]
mod complex_tests {
    use super::*;

    const LST: &str = "COMPLEX 4\nKUK00 159 146 30 #KUK00 1 1 0 0\nKUK01 159 146 30 #KUK01 1 1 0 0\n\
                       KUK05 159 146 30 #KUK05 1 1 0 0\nKUD05 159 146 30 #KUK05 1 1 0 0\n";
    const ADS: &str = "USERLC 52 kuk NONE -159 -146\n#KUK00 1 1 52 0\n#KUK01 1 1 52 1\n#KUK05 1 1 52 5\n[END]\n";

    /// The picture of a field follows its growth stage: the type's `#KUKnn` animation names
    /// the frame, and a regrown field (`KUD05`) shows the ripe frame again.
    #[test]
    fn field_frames_follow_the_growth_stage() {
        let mut objects = MapObjects::default();
        objects.add_file(MapObjects::COMPLEX_LIST, LST.as_bytes());
        objects.effects = Some(UnitDef::parse(ADS.as_bytes()));
        let frame_of = |index: usize, tick: u64| {
            let (anim, step) = objects.complex_step(index, tick).unwrap();
            let refs = objects.effects().unwrap().animation(anim).unwrap().frame_refs();
            (anim.to_owned(), refs[step].sprite, refs[step].frame)
        };
        assert_eq!(frame_of(0, 0), ("#KUK00".to_owned(), 52, 0));
        assert_eq!(frame_of(1, 700), ("#KUK01".to_owned(), 52, 1));
        assert_eq!(frame_of(2, 5), ("#KUK05".to_owned(), 52, 5));
        assert_eq!(frame_of(3, 5), frame_of(2, 5));
        assert!(objects.complex_step(4, 0).is_none(), "past the list");
    }
}
