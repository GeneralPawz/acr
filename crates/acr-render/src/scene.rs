//! The objects of a map (trees, stones, units, buildings) placed on the terrain in the
//! original's draw order.
//!
//! The original collects the visible objects every frame in a z-buffer of screen rows
//! (0x49C3E1: bucket = sort y + 256) and draws, after the terrain: first the flat objects
//! (0x49C249: stones, holes, ...; and ground parts of buildings), then the bucket rows from
//! top to bottom. Sprites of `TREELIST` and parts of complex objects are sorted by the
//! object's ground position `y / 2` *without* its height (0x4D0930); the frame is drawn at
//! `(x − DX, y / 2 − height − DY)`. See `docs/formats/terrain.md`.

use std::collections::HashMap;

use acr_formats::defs::UnitDef;
use acr_formats::m3d::{M3dMap, SpriteGroup};
use acr_formats::sprite_list::SpriteList;

use crate::terrain::{Camera, TerrainData};

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
    /// Drawn half transparent (`ShowGPTransparent`: buildings in the transparency mode).
    pub transparent: bool,
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
}

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

/// The sprite packs an object type needs: a building's idle, construction (`#BUILDLO_k`) and
/// ruin animations, every animation of a unit (the simulation plays walking, fighting and
/// dying, see `acr_sim::world::AnimKind`).
pub fn unit_packs(def: &UnitDef) -> Vec<String> {
    let anims: Vec<&acr_formats::defs::md::Animation> = if def.is_building() {
        let stages = def.animations.iter().filter(|a| {
            let n = a.name.to_ascii_uppercase();
            n.starts_with("#BUILDLO") || n.starts_with("#DEATHLIE")
        });
        idle_animation(def).into_iter().chain(stages).collect()
    } else {
        def.animations.iter().collect()
    };
    let mut packs: Vec<String> =
        anims.iter().flat_map(|a| a.frame_refs()).filter_map(|r| def.sprite(r.sprite)).map(|s| s.gp_path()).collect();
    packs.sort();
    packs.dedup();
    packs
}

/// The frames of `def`'s idle animation facing `direction` (0..=255, as stored in maps).
///
/// A building's animation lists its parts, all drawn; `LINESORT` marks `GROUND` parts.
/// A unit shows the first frame. Frame `f` of direction `d` is `d · N / dirs + f` in a pack
/// of `N` frames (see `docs/formats/definitions.md`). TODO: the direction order and the
/// mirroring of the 9 stored directions are not verified; directions are mapped evenly
/// and mirrored ones use their unmirrored twin.
pub fn unit_look(def: &UnitDef, direction: u8, frames_in: impl Fn(&str) -> Option<usize>) -> Option<UnitLook> {
    animation_look(def, idle_animation(def)?, direction, 0, frames_in)
}

/// The frames of animation `anim` of `def` at animation frame `step`, facing `direction`.
/// Buildings show all parts of the animation; units show the part at `step`.
pub fn animation_look(
    def: &UnitDef,
    anim: &acr_formats::defs::md::Animation,
    direction: u8,
    step: usize,
    frames_in: impl Fn(&str) -> Option<usize>,
) -> Option<UnitLook> {
    let refs = anim.frame_refs();
    let dirs = anim.directions.max(1) as usize;
    let d = if dirs >= 16 {
        usize::from(direction) * dirs / 256
    } else {
        // 9 stored directions cover half a circle; the other half is mirrored.
        let d16 = (usize::from(direction) + 8) / 16 % 16;
        let half = if d16 <= 8 { d16 } else { 16 - d16 };
        half * (dirs - 1) / 8
    };
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
            let per_dir = frames_in(&pack)? / dirs;
            let frame = d.min(dirs - 1) * per_dir + r.frame.max(0) as usize;
            // TODO: the original compares objects against the line (0x49C96A); the line's
            // middle row is used as the sort row here.
            let (layer, row) = match sorts.get(i) {
                Some(PartSort::Ground) => (Layer::Ground, 0),
                Some(&PartSort::Line(_, y1, _, y2)) => (Layer::Sorted, location.1 + (y1 + y2) / 2),
                None => (Layer::Sorted, 0),
            };
            Some(((pack, frame), (set.offset.0 + location.0, set.offset.1 + location.1), layer, row))
        })
        .collect::<Vec<_>>();
    (!parts.is_empty()).then_some(UnitLook { parts })
}

/// Per part of animation `name`, its `LINESORT` entry: `GROUND` or `LINE x1 y1 x2 y2`.
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
                });
            }
        }
        sprites.sort_by_key(SceneSprite::key);
        let sorted_from = sprites.iter().position(|s| s.layer == Layer::Sorted).unwrap_or(sprites.len());
        Self { sprites, sorted_from, missing }
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
        ground[g].iter().chain(&sorted[s]).filter_map(move |s| {
            if (s.x as f32) < x0 || (s.x as f32) > x1 {
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
    /// `.MD` files still to arrive, once `NMLIST.TXT` is read.
    defs_pending: Option<usize>,
    /// Unit definitions by upper-case `NAME`.
    defs: HashMap<String, UnitDef>,
    /// Requested sprite packs; `None` while loading.
    packs: HashMap<String, Option<acr_formats::GpFile>>,
    /// Packs that could not be read; not requested again (zoomed-out twins are often
    /// missing).
    missing: std::collections::HashSet<String>,
}

impl MapObjects {
    /// Sprite lists of the object groups drawn so far. TODO: `COMPLEX.LST` (fields) and
    /// `ANIMATIONS.LST` (river animations) use a different format and are not drawn yet.
    pub const LISTS: [(SpriteGroup, &'static str); 3] = [
        (SpriteGroup::Trees, "TREELIST.LST"),
        (SpriteGroup::Stones, "STONLIST.LST"),
        (SpriteGroup::Holes, "HOLELIST.LST"),
    ];
    pub const NAME_LIST: &'static str = "NMLIST.TXT";

    /// The files to request first.
    pub fn files() -> Vec<&'static str> {
        Self::LISTS.iter().map(|(_, p)| *p).chain([Self::NAME_LIST]).collect()
    }

    /// Takes a loaded file (any file; others are ignored) and returns further files to
    /// request.
    pub fn add_file(&mut self, path: &str, bytes: &[u8]) -> Vec<String> {
        let mut more = Vec::new();
        let upper = acr_formats::normalize_path(path);
        if let Some((group, _)) = Self::LISTS.iter().find(|(_, p)| *p == path) {
            if let Some(list) = SpriteList::parse(&String::from_utf8_lossy(bytes)) {
                more.extend(self.want_pack(&list.gp_path()));
                self.lists.insert(*group, list);
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
            || self.packs.contains_key(path)
            || (self.defs_pending.is_some_and(|n| n > 0) && acr_formats::normalize_path(path).ends_with(".MD"))
    }

    /// A requested file could not be read: stop waiting for it.
    pub fn file_failed(&mut self, path: &str) {
        if self.packs.remove(path).is_some() {
            self.missing.insert(path.to_owned());
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

    /// The definition of a map unit id such as `EngM18(EN)`.
    pub fn definition(&self, unit_id: &str) -> Option<&UnitDef> {
        self.defs.get(&definition_name(unit_id))
    }

    /// A loaded sprite pack.
    pub fn pack(&self, path: &str) -> Option<&acr_formats::GpFile> {
        self.packs.get(path).and_then(Option::as_ref)
    }

    /// Requests the packs `map` needs; returns the paths to load. Call when the
    /// definitions are complete ([`MapObjects::definitions_ready`]).
    pub fn packs_for(&mut self, map: &M3dMap) -> Vec<String> {
        let mut names: Vec<String> = map.units.iter().map(|u| definition_name(&u.name)).collect();
        names.sort_unstable();
        names.dedup();
        let packs: Vec<String> = names.iter().filter_map(|n| self.defs.get(n)).flat_map(unit_packs).collect();
        packs.iter().flat_map(|p| self.want_pack(p)).collect()
    }

    /// Requests the packs of object types (map unit ids such as `EngSkl(EN)`) that appear
    /// during the game; returns the paths to load.
    pub fn packs_for_units<'a>(&mut self, ids: impl IntoIterator<Item = &'a str>) -> Vec<String> {
        let packs: Vec<String> =
            ids.into_iter().filter_map(|id| self.defs.get(&definition_name(id))).flat_map(unit_packs).collect();
        packs.iter().flat_map(|p| self.want_pack(p)).collect()
    }

    pub fn definitions_ready(&self) -> bool {
        self.defs_pending == Some(0) && self.lists.len() == Self::LISTS.len()
    }

    /// Whether every requested pack has arrived.
    pub fn packs_ready(&self) -> bool {
        self.packs.values().all(Option::is_some)
    }

    /// The look of a unit type facing `direction`, from its definition and packs.
    pub fn unit_look(&self, unit_id: &str, direction: u8) -> Option<UnitLook> {
        unit_look(self.definition(unit_id)?, direction, |p| self.pack(p).map(acr_formats::GpFile::frame_count))
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
    /// falling back to the idle animation.
    pub fn animation_look(&self, unit_id: &str, anim: &str, direction: u8, step: usize) -> Option<UnitLook> {
        let def = self.definition(unit_id)?;
        let animation = def.animation(anim).or_else(|| idle_animation(def))?;
        animation_look(def, animation, direction, step, |p| self.pack(p).map(acr_formats::GpFile::frame_count))
    }

    /// `keys` plus the frames of their zoomed-out twins that are loaded.
    pub fn with_zoom_frames(&self, mut keys: Vec<FrameKey>) -> Vec<FrameKey> {
        let twins: Vec<FrameKey> = keys
            .iter()
            .filter_map(|(p, f)| {
                let z = zoom_pack(p)?;
                (*f < self.pack(&z)?.frame_count()).then_some((z, *f))
            })
            .collect();
        keys.extend(twins);
        keys.sort();
        keys.dedup();
        keys
    }

    /// Every frame of every pack loaded for units, for uploading next to a static scene.
    pub fn unit_frames(&self) -> Vec<FrameKey> {
        let lists: Vec<String> = self.lists.values().map(SpriteList::gp_path).collect();
        let mut frames: Vec<FrameKey> = self
            .packs
            .iter()
            .filter(|(path, _)| !lists.contains(path))
            .filter_map(|(path, gp)| Some((path, gp.as_ref()?.frame_count())))
            .flat_map(|(path, n)| (0..n).map(move |f| (path.clone(), f)))
            .collect();
        frames.sort();
        frames
    }

    /// Decodes `frames` (a [`Scene::frames`] list, plus any frames dynamic sprites will
    /// use) and uploads them for drawing.
    pub fn upload(
        &self,
        renderer: &crate::SpriteRenderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frames: Vec<FrameKey>,
    ) -> SceneAtlas {
        let decoded: Vec<acr_formats::Frame> = frames
            .iter()
            .map(|(pack, frame)| self.pack(pack).and_then(|g| g.decode_frame(*frame).ok()).unwrap_or_default())
            .collect();
        let atlas = renderer.upload(device, queue, &decoded);
        SceneAtlas { atlas, index: frames.into_iter().enumerate().map(|(i, k)| (k, i)).collect() }
    }
}

/// The uploaded frames of a scene.
pub struct SceneAtlas {
    pub atlas: crate::SpriteAtlas,
    index: HashMap<FrameKey, usize>,
}

impl SceneAtlas {
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
        let mut out = Vec::new();
        for (s, [x, y]) in draw_order(scene, dynamic, camera, size, zoomed) {
            let twin =
                zoomed.then(|| zoom_pack(&s.frame.0)).flatten().and_then(|p| self.index.get(&(p, s.frame.1)).copied());
            if let Some(frame) = twin {
                let [px, py] = camera.project(s.x, s.y, 0);
                out.push(crate::SpriteDraw {
                    frame,
                    x: (px + (s.dx >> 2) as f32).round(),
                    y: (py + (s.dy >> 2) as f32).round(),
                    scale: 1.0,
                    player: s.player,
                    transparent: s.transparent,
                });
            } else if let Some(&frame) = self.index.get(&s.frame) {
                out.push(crate::SpriteDraw {
                    frame,
                    x: x.round(),
                    y: y.round(),
                    scale: camera.scale,
                    player: s.player,
                    transparent: s.transparent,
                });
            }
        }
        out
    }
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

    #[test]
    fn units_pick_a_direction_block() {
        let def = UnitDef::parse(UNIT.as_bytes());
        // 9 directions of 4 frames: frame 2 of direction d is 4·d + 2.
        let frame = |dir| unit_look(&def, dir, |_| Some(36)).unwrap().parts[0].0.1;
        assert_eq!(frame(0), 2);
        assert_eq!(frame(128), 4 * 8 + 2);
        assert_eq!(unit_look(&def, 0, |_| Some(36)).unwrap().parts[0].1, (-160, -132));
    }

    #[test]
    fn dynamic_sprites_merge_into_draw_order() {
        let scene = Scene {
            sprites: vec![sprite(Layer::Ground, 5), sprite(Layer::Sorted, 10), sprite(Layer::Sorted, 30)],
            sorted_from: 1,
            missing: HashMap::new(),
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
