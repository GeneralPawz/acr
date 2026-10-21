//! A ready-to-use layer for a map's objects: feeds [`MapObjects`] the files it asks for,
//! builds the map's [`Scene`] and draws its visible part over the terrain. Used by the
//! game and the viewer.

use acr_formats::{M3dMap, Palette};

use crate::SpriteRenderer;
use crate::scene::{MapObjects, Scene, SceneAtlas};
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
        }
    }

    pub fn start(&self, load: Loader<'_>) {
        for path in MapObjects::files() {
            load(path);
        }
        load(PALETTE);
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
        ours
    }

    /// Whether `path` is a file this layer asked for.
    pub fn wants(&self, path: &str) -> bool {
        path == PALETTE || self.assets.wants(path)
    }

    pub fn on_failed(&mut self, path: &str) {
        self.assets.file_failed(path);
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
        let scene = self.assets.scene(map, terrain);
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
        if let (Some(scene), None) = (&self.scene, &self.atlas) {
            self.atlas = Some(self.assets.upload(renderer, device, queue, scene.frames()));
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
        let (Some(scene), Some(atlas), Some(renderer), true) =
            (&self.scene, &self.atlas, &mut self.renderer, self.shown)
        else {
            return;
        };
        let draws = atlas.draws(scene, &[], camera, size);
        renderer.prepare(device, queue, &atlas.atlas, size, &draws);
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
        renderer.render(&mut pass, &atlas.atlas);
    }
}
