//! A ready-to-use layer for a map's objects: feeds [`MapObjects`] the files it asks for,
//! builds the map's [`Scene`] and draws its visible part over the terrain. Used by the
//! game and the viewer.

use acr_formats::{M3dMap, Palette};

use crate::SpriteRenderer;
use crate::scene::{MapObjects, Placement, Scene, SceneAtlas, SceneSprite};
use crate::terrain::{Camera, TerrainData};

/// Requests a game file; the result must come back through [`ObjectsView::on_loaded`] or
/// [`ObjectsView::on_failed`].
pub type Loader<'a> = &'a dyn Fn(&str);

/// Map sprites are drawn with the game palette (set 0).
const PALETTE: &str = r"0\AGEW_1.PAL";

pub struct ObjectsView {
    assets: MapObjects,
    palette: Option<Palette>,
    /// Waiting for definitions or packs before this map's scene can be built.
    waiting: bool,
    scene: Option<Scene>,
    atlas: Option<SceneAtlas>,
    renderer: Option<SpriteRenderer>,
    pub shown: bool,
    /// Units and buildings are drawn from the caller's dynamic sprites (the simulation)
    /// instead of the map's placements.
    dynamic_units: bool,
    /// Sprites to merge into the scene this frame, see [`ObjectsView::set_dynamic`].
    dynamic: Vec<SceneSprite>,
    /// Packs requested by [`ObjectsView::want_units`] that have not arrived yet.
    late_packs: Vec<String>,
    /// A late pack arrived: its frames are added to the atlas.
    grow: bool,
    /// The zoomed-out view: sprites come from their `L_Mode` twins.
    pub zoomed: bool,
}

impl Default for ObjectsView {
    fn default() -> Self {
        Self::new()
    }
}

impl ObjectsView {
    pub fn new() -> Self {
        Self {
            assets: MapObjects::default(),
            palette: None,
            waiting: false,
            scene: None,
            atlas: None,
            renderer: None,
            shown: true,
            dynamic_units: false,
            dynamic: Vec::new(),
            late_packs: Vec::new(),
            grow: false,
            zoomed: false,
        }
    }

    /// Draw units and buildings from the dynamic sprites passed to [`ObjectsView::render`].
    pub fn with_dynamic_units(mut self) -> Self {
        self.dynamic_units = true;
        self
    }

    /// Sprites (e.g. the simulation's units) drawn with the scene from now on, merged into
    /// the original draw order. Their frames must be among the uploaded ones.
    pub fn set_dynamic(&mut self, sprites: Vec<SceneSprite>) {
        self.dynamic = sprites;
    }

    /// Loads the packs of object types that appear during the game (produced units, placed
    /// buildings), given as map unit ids; their frames are added to the atlas as they arrive.
    pub fn want_units<'a>(&mut self, ids: impl IntoIterator<Item = &'a str>, load: Loader<'_>) {
        if !self.assets.definitions_ready() {
            return;
        }
        for pack in self.assets.packs_for_units(ids) {
            load(&pack);
            self.late_packs.push(pack);
        }
    }

    /// The loaded definitions and packs.
    pub fn objects(&self) -> &MapObjects {
        &self.assets
    }

    /// Whether the scene is built and units can be looked up.
    pub fn ready(&self) -> bool {
        self.scene.is_some()
    }

    pub fn start(&self, load: Loader<'_>) {
        for path in MapObjects::files() {
            load(path);
        }
        load(PALETTE);
    }

    /// Also loads the animations of projectiles, smoke and fires (`WEAPON.ADS`); call with
    /// [`ObjectsView::start`].
    pub fn start_effects(&mut self, load: Loader<'_>) {
        for path in self.assets.want_effects() {
            load(&path);
        }
    }

    /// Takes a loaded file; returns whether it was one this layer asked for.
    pub fn on_loaded(&mut self, path: &str, bytes: &[u8], load: Loader<'_>) -> bool {
        let ours = self.wants(path);
        if path == PALETTE {
            self.palette = Palette::parse(bytes);
        }
        for more in self.assets.add_file(path, bytes) {
            load(&more);
        }
        self.late_pack_done(path);
        ours
    }

    /// A late pack arrived or failed: its frames go into the atlas. Only the new frames are
    /// uploaded; rebuilding the whole atlas froze the game for most of a second whenever a
    /// building or unit type appeared for the first time.
    fn late_pack_done(&mut self, path: &str) {
        let before = self.late_packs.len();
        self.late_packs.retain(|p| p != path);
        if self.late_packs.len() < before {
            self.grow = true;
        }
    }

    /// Whether `path` is a file this layer asked for.
    pub fn wants(&self, path: &str) -> bool {
        path == PALETTE || self.assets.wants(path)
    }

    pub fn on_failed(&mut self, path: &str) {
        self.assets.file_failed(path);
        self.late_pack_done(path);
    }

    /// Forgets the scene of the previous map.
    pub fn reset(&mut self) {
        self.scene = None;
        self.atlas = None;
        self.waiting = true;
    }

    /// Builds the scene once the definitions and every pack the map needs are loaded.
    pub fn update(&mut self, map: &M3dMap, terrain: &TerrainData, load: Loader<'_>) {
        if !self.waiting || !self.assets.definitions_ready() {
            return;
        }
        for pack in self.assets.packs_for(map) {
            load(&pack);
        }
        if !self.assets.packs_ready() {
            return;
        }
        self.waiting = false;
        let scene =
            if self.dynamic_units { self.assets.static_scene(map, terrain) } else { self.assets.scene(map, terrain) };
        if !scene.missing.is_empty() {
            log::warn!("objects without a sprite: {:?}", scene.missing);
        }
        self.scene = Some(scene);
    }

    pub fn sync_gpu(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) {
        let renderer = self.renderer.get_or_insert_with(|| SpriteRenderer::new(device, format));
        if let Some(palette) = self.palette.take() {
            renderer.set_palette(queue, &palette);
        }
        let Some(scene) = &self.scene else { return };
        if self.atlas.is_some() && !self.grow {
            return;
        }
        let mut frames = self.assets.with_zoom_frames(scene.frames());
        if self.dynamic_units {
            frames.extend(self.assets.unit_frames());
            frames.sort();
            frames.dedup();
        }
        self.grow = false;
        match &mut self.atlas {
            Some(atlas) => self.assets.upload_more(renderer, device, queue, atlas, frames),
            None => self.atlas = Some(self.assets.upload(renderer, device, queue, frames)),
        }
    }

    pub fn count(&self) -> usize {
        self.scene.as_ref().map_or(0, |s| s.sprites().len())
    }

    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        size: [f32; 2],
        camera: &Camera,
    ) {
        if !self.prepare(device, queue, size, camera, None) {
            return;
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("map objects"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        self.draw(&mut pass, false);
    }

    /// Uploads this frame's sprites; with a `placement` they get their depth for the 3D pass
    /// (and their place under the free camera). Returns whether there is anything to draw.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        size: [f32; 2],
        camera: &Camera,
        placement: Option<Placement<'_>>,
    ) -> bool {
        let (Some(scene), Some(atlas), Some(renderer), true) =
            (&self.scene, &self.atlas, &mut self.renderer, self.shown)
        else {
            return false;
        };
        let draws = atlas.placed_draws(scene, &self.dynamic, camera, size, self.zoomed, placement);
        renderer.prepare(device, queue, &atlas.atlas, size, &draws);
        true
    }

    /// Draws what [`ObjectsView::prepare`] uploaded; `depth`: into the 3D pass.
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, depth: bool) {
        let (Some(atlas), Some(renderer), true) = (&self.atlas, &self.renderer, self.shown) else { return };
        if depth {
            renderer.render_depth(pass, &atlas.atlas);
        } else {
            renderer.render(pass, &atlas.atlas);
        }
    }
}
