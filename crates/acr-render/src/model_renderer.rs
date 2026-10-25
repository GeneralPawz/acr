//! Draws [`ModelData`] models on the GPU: skinned in the vertex shader from a float texture
//! of palettes (WebGL2-friendly), instanced per model type, depth-tested against the
//! terrain and each other in a pass shared with the sprites ([`DepthTarget`]).
//!
//! One frame: [`ModelRenderer::prepare`] samples each distinct pose once (instances of the
//! same model, clip and frame share it), uploads palettes and instances; then, in the pass
//! [`DepthTarget::begin`] opens, the terrain writes its depth
//! (`TerrainRenderer::render_depth`), [`ModelRenderer::render`] draws the models (marking
//! their pixels in the stencil), [`DepthTarget::reset_ground`] moves everything else back to
//! the far plane, and the sprites draw with a depth test against the models only.
//!
//! Corpses carry [`Wounds`] (blood, dirt and burns by the cause of death) and leave
//! [`BloodPool`]s: flat noisy decals on the ground, drawn before the models.

use std::collections::HashMap;
use std::ops::Range;

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::damage::{self, Damage};
use crate::lod;
use crate::looks;
use crate::math::{self, Mat4};
use crate::mipgen;
use crate::model::{ModelData, Vertex};
use crate::shadow::{self, ShadowMap};
use crate::tex_cache;
use crate::view3d::View3d;

/// Depth and stencil of the 3D pass (supported everywhere, WebGL2 included).
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24PlusStencil8;
/// Texels per row of the bone texture.
const BONE_WIDTH: u32 = 1024;
/// Direction to the light in world coordinates (east, south, up): the sun of the shadows.
const LIGHT: [f32; 3] = shadow::LIGHT;
/// Share of unlit colour.
const AMBIENT: f32 = 0.5;

/// One model to draw.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelInstance {
    /// Index returned by [`ModelRenderer::add_model`].
    pub model: usize,
    /// Clip index ([`ModelData::clip`]); `None` is the bind pose.
    pub clip: Option<usize>,
    /// Clip time in seconds.
    pub time: f32,
    /// Model (glTF metres) → world ([`crate::view3d::model_matrix`]).
    pub transform: Mat4,
    /// Decay 0..1 (`docs/assets-humans.md`).
    pub decay: f32,
    /// Weights of the morph targets `deflate`, `lie_flat` and `emaciated` (hunger, #207;
    /// drawn only by models whose second slot holds it, [`ModelData::emaciated`]).
    pub morphs: [f32; 3],
    /// Every other pixel only (the original's half transparent corpses).
    pub transparent: bool,
    pub player: u8,
    /// Wounds, blood, dirt and burns of a corpse (all zero for the living).
    pub wounds: Wounds,
    /// Blood sprayed onto it from someone hit in front of it (troops and buildings).
    pub sprayed: Sprayed,
    /// Cuts, fractures, a torn trunk, or the piece of a body this instance draws
    /// (`docs/damage-model.md`); the default draws it whole.
    pub damage: Damage,
    /// Individual look (#283, `crate::looks`): the seed of [`looks::choose`] (the unit id).
    /// Ignored by models without looks; `None` shows the model's base look.
    pub look: Option<u32>,
    /// A gun crew's uniform (#287): the colours (linear, as the model's palette) of the coat
    /// and of the waistcoat and facings; `None` keeps the model's own.
    pub uniform: Option<[[f32; 3]; 2]>,
    /// A corpse bent over the bodies or ground under it (#229); `None` keeps it straight.
    pub drape: Option<Drape>,
    /// A building under construction (#302): drawn up to a height; `None` draws it whole.
    pub rise: Option<Rise>,
}

/// A building rising under construction (#302, `model.wgsl` `rising`): only what lies below
/// `height` (metres above the model's ground point, glTF y after the node transforms) is
/// drawn, the cut line stepped like courses of logs or stones, and a band of `band` metres
/// under it shows a scaffold of poles and planks over fresh timber.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Rise {
    pub height: f32,
    pub band: f32,
}

/// A lying body's bend (#229, `model.wgsl`): the model's up (glTF +y, metres) is raised by
/// `heights` at four stations along it (glTF z, `first` and then every `spacing` metres),
/// linear between them and held beyond the ends. Applied after the palette, so not to
/// physics-posed bodies (their palette cancels the instance transform).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Drape {
    pub heights: [f32; 4],
    pub first: f32,
    pub spacing: f32,
}

/// Blood sprayed onto a model from a hit nearby: speckles around a point of its rest pose
/// (glTF metres), fresh red drying to brown.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Sprayed {
    pub point: [f32; 3],
    /// Radius of the speckled area (metres).
    pub radius: f32,
    /// 0..1.
    pub amount: f32,
    /// 0 fresh .. 1 dried.
    pub age: f32,
}

/// Blood, dirt and burns on a body (deaths by cause, `docs/rendering-3d.md`). Wounds sit in
/// the model's rest pose (glTF metres, +y up, facing +z), so they stay on the body whatever
/// pose it lies in.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Wounds {
    /// Direction to the blow in the model's frame (glTF x, z); zero: from the front.
    pub from: [f32; 2],
    /// Blood on the body, 0..1.
    pub blood: f32,
    /// Dirt, 0..1.
    pub dirt: f32,
    /// Charring, 0..1.
    pub burn: f32,
    /// Number of wounds (0..6): the first faces the blow; with two the second is the exit.
    pub count: u8,
    /// Wound radius in metres.
    pub radius: f32,
    /// Varies the places of the further wounds per corpse.
    pub seed: f32,
    /// Wounds placed where the hits struck (#277), on the living and the dead: the first
    /// `marks_len` of `marks`.
    pub marks: [Mark; MAX_MARKS],
    pub marks_len: u8,
}

/// Most wounds placed per body ([`Wounds::marks`]).
pub const MAX_MARKS: usize = 8;

/// One wound where a hit struck (#277), in the rest pose (glTF metres): a hole, blood soaking
/// the cloth around it and running down from it.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Mark {
    /// Centre on the body's surface (rest pose).
    pub point: [f32; 3],
    /// Radius of the hole (metres).
    pub radius: f32,
    /// A ragged exit wound: torn flesh around a larger hole.
    pub ragged: bool,
    /// How far the blood has run down from it (metres).
    pub run: f32,
    /// How far the cloth around it is soaked, 0..1.
    pub soak: f32,
    /// 0 fresh .. 1 dried.
    pub age: f32,
    pub seed: f32,
}

impl Mark {
    fn gpu(&self) -> [[f32; 4]; 2] {
        let r = if self.ragged { -self.radius } else { self.radius };
        [
            [self.point[0], self.point[1], self.point[2], r],
            [self.run, self.soak.clamp(0.0, 1.0), self.age.clamp(0.0, 1.0), self.seed],
        ]
    }
}

impl Wounds {
    fn gpu(&self) -> [[f32; 4]; 2] {
        [
            [self.from[0], self.from[1], self.blood, self.dirt],
            [self.burn, f32::from(self.count.min(6)), self.radius, self.seed],
        ]
    }
}

/// Most scars kept per building ([`Scars::list`]).
pub const MAX_SCARS: usize = 32;

/// What a round left on a building's wall or roof (#304, `model.wgsl` `scarred`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScarKind {
    /// A cannonball's hole: dark and ragged, a splintered rim on wood, cracked and chipped on
    /// stone and plaster.
    Hole = 0,
    /// A musket ball's or grapeshot's pock.
    Pock = 1,
    /// Soot of a shell burst, climbing up the wall, with a chipped crater in the middle.
    Scorch = 2,
    /// Where an arrow went in (the arrow itself is a model of its own).
    Puncture = 3,
}

/// One scar on a building (#304), in the model's frame after its node transforms (glTF
/// metres, the pose at time 0): the shader projects it onto the surface along `normal`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scar {
    pub point: [f32; 3],
    /// The struck surface's outward normal (unit).
    pub normal: [f32; 3],
    /// Radius (metres).
    pub radius: f32,
    pub kind: ScarKind,
    /// 0..1, varies the outline.
    pub seed: f32,
}

/// A building's hit damage (#304): its scars, and the cracks and soot over the whole model
/// that grow with the life it lost.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Scars {
    pub list: Vec<Scar>,
    /// Cracks all over, 0..1.
    pub batter: f32,
    /// Soot all over, 0..1.
    pub soot: f32,
}

/// Scars per group of [`Scars`] on the GPU: a fragment tests the groups' spheres first and
/// reads the scars of the near ones only.
pub const SCAR_GROUP: usize = 8;

impl Scars {
    /// A header texel (count, batter, soot, groups), a sphere per group of [`SCAR_GROUP`]
    /// (centre, radius: all its scars can reach), then two texels per scar: [point, radius],
    /// [normal, kind + seed · 0.9]. The groups are neighbours: ordered by the bearing around
    /// the model's middle, then by height.
    fn texels(&self, out: &mut Vec<[f32; 4]>) {
        let mut list: Vec<Scar> = self.list[..self.list.len().min(MAX_SCARS)].to_vec();
        let bearing = |s: &Scar| (s.point[2].atan2(s.point[0]) * 4.0).round() as i32;
        list.sort_by(|a, b| bearing(a).cmp(&bearing(b)).then(a.point[1].total_cmp(&b.point[1])));
        let groups = list.len().div_ceil(SCAR_GROUP);
        out.push([list.len() as f32, self.batter.clamp(0.0, 1.0), self.soot.clamp(0.0, 1.0), groups as f32]);
        for g in list.chunks(SCAR_GROUP) {
            let k = 1.0 / g.len() as f32;
            let c = g.iter().fold([0.0; 3], |c, s| math::add(c, math::scale(s.point, k)));
            // How far the shader draws a scar: a hole's cracks 3.2 radii, the others 1.4.
            let reach = |s: &Scar| s.radius * if s.kind == ScarKind::Hole { 3.2 } else { 1.4 };
            let r = g.iter().map(|s| math::dot(math::sub(s.point, c), math::sub(s.point, c)).sqrt() + reach(s));
            out.push([c[0], c[1], c[2], r.fold(0.0, f32::max)]);
        }
        for s in &list {
            out.push([s.point[0], s.point[1], s.point[2], s.radius]);
            out.push([s.normal[0], s.normal[1], s.normal[2], s.kind as u8 as f32 + s.seed.clamp(0.0, 1.0) * 0.9]);
        }
    }
}

/// A pool of blood on the ground under a corpse, or (`splatter`) a droplet sprayed onto the
/// ground behind a hit unit.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct BloodPool {
    /// Centre on the ground (world coordinates).
    pub centre: [f32; 3],
    /// Radius (world pixels).
    pub radius: f32,
    /// Direction the pool runs out to (world x, y; unit or zero) and how far (0: round, 1:
    /// twice as long).
    pub along: [f32; 2],
    pub stretch: f32,
    /// 0 fresh red .. 1 dried and soaked in.
    pub age: f32,
    /// 1 visible .. 0 gone.
    pub fade: f32,
    /// Varies the outline per pool.
    pub seed: f32,
    /// A droplet: a crisp round head with a tail running out along `along`, instead of a
    /// noisy pool.
    pub splatter: bool,
    /// Not blood but churned earth where a round struck the ground.
    pub dirt: Dirt,
    /// The ground's slope under the decal (world z per world x and per world y), so it
    /// lies on the slope instead of sinking into it.
    pub tilt: [f32; 2],
    /// A piece of a smear (a body dragged through blood): a ribbon from `centre` running
    /// `along` instead of a pool. `radius` is its half width where it ends.
    pub smear: Option<Smear>,
    /// A decay stain's state ([`Dirt::Seep`]).
    pub seep: Seep,
}

/// How far a decay stain ([`Dirt::Seep`]) has come: `age` dries it.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Seep {
    /// 0 the first leak at the head .. 1 the whole island under head and torso.
    pub growth: f32,
    /// The grass dead around it: 0 none (or no grass) .. 1 a full ring.
    pub ring: f32,
}

/// One segment of a smeared blood trail: consecutive segments of one unit join into a
/// ribbon with ragged edges and streaks along the drag (`blood.wgsl`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Smear {
    /// Length from `centre` along `along` (world pixels).
    pub length: f32,
    /// Half width where it starts, relative to `radius` (where it ends), so the ribbon
    /// widens and narrows smoothly from one segment to the next.
    pub start_width: f32,
    /// How much blood: 1 a heavy, dark smear (he was slow) .. 0 a thin, broken one.
    pub density: f32,
}

/// The earth thrown up where a round strikes the ground (a [`BloodPool`] decal in dirt
/// colours).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Dirt {
    /// Blood.
    #[default]
    None,
    /// A cannonball's or shell's crater: a dark pit in a ring of thrown earth.
    Crater,
    /// A musket ball's, arrow's or grapeshot's scuff: a small dusty smudge.
    Scuff,
    /// Not earth thrown up but the fluids of a decaying body soaked into it
    /// (`decay_stain`): dark and greasy, drying brown, the grass dead around it.
    Seep,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuPool {
    centre: [f32; 4],
    shape: [f32; 4],
    look: [f32; 4],
    tilt: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    view_proj: Mat4,
    light: [f32; 4],
    forward: [f32; 4],
    params: [f32; 4],
    /// Wind (docs/rendering-3d.md "Wind"): world direction x, y (unit), strength (0: still),
    /// time (s).
    wind: [f32; 4],
    /// Weather (`weather.rs`): wet, snow, ground mist, its height; the fog colour (linear).
    weather: [f32; 4],
    fog: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct MaterialUniform {
    base: [f32; 4],
    tint: [f32; 4],
    windows: [f32; 4],
    edge: [f32; 4],
    flags: [f32; 4],
    /// x: stump cap (wet, ragged outline), y: tissue kind, z: per-soldier tint, w: the
    /// model rots by the procedural fallback ([`crate::model::ModelData::decays_procedurally`]).
    look: [f32; 4],
    /// x: sheen at grazing angles, y: gloss ([`crate::model::Finish`]).
    cloth: [f32; 4],
    /// The model's wind scales: trunk, branch, flutter, tempo ([`crate::model::ModelData::wind`]).
    wind: [f32; 4],
    /// Surface detail (#234, [`crate::model::Surface`]): metallic, roughness, normal map
    /// scale (0: no normal map), 1 with a metallic-roughness texture.
    pbr: [f32; 4],
    /// x: the least screen width (px) of the thin parts ([`crate::model::Finish::thin`]).
    thin: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuInstance {
    rows: [[f32; 4]; 3],
    params: [f32; 4],
    /// First texel of the pose, first texel of the instance data (`model.wgsl`), player, 0.
    extra: [u32; 4],
}

/// The data texels of an instance in the bone texture: wounds, spray and (when damaged)
/// the damage block of [`damage::pack`]; a building's [`Scars`] last.
fn instance_texels(
    inst: &ModelInstance,
    rig: Option<&damage::Rig>,
    look: &[[f32; 4]; 3],
    scars: Option<&Scars>,
    out: &mut Vec<[f32; 4]>,
) {
    let [w0, w1] = inst.wounds.gpu();
    let s = &inst.sprayed;
    let damaged = !inst.damage.is_none();
    let marks = &inst.wounds.marks[..usize::from(inst.wounds.marks_len).min(MAX_MARKS)];
    out.extend([
        w0,
        w1,
        [s.point[0], s.point[1], s.point[2], s.radius],
        // Bit 0: damaged; bit 1: draped (the drape follows the look); bit 2: scarred (the
        // scars follow the drape and the rise); bit 3: rising (after the drape).
        [
            s.amount.clamp(0.0, 1.0),
            s.age.clamp(0.0, 1.0),
            f32::from(
                u8::from(damaged)
                    | (u8::from(inst.drape.is_some()) << 1)
                    | (u8::from(scars.is_some()) << 2)
                    | (u8::from(inst.rise.is_some()) << 3),
            ),
            marks.len() as f32,
        ],
    ]);
    if damaged {
        out.extend(damage::pack(&inst.damage, rig));
    }
    // The marks follow the damage block (`model.wgsl`, `marked`).
    for m in marks {
        out.extend(m.gpu());
    }
    // The look follows the marks (`model.wgsl`, `look_texel`).
    out.extend_from_slice(look);
    // The drape follows the look (`model.wgsl`, `vs_main`).
    if let Some(d) = inst.drape {
        out.extend([d.heights, [d.first, d.spacing, 0.0, 0.0]]);
    }
    // A building's rise follows the drape (`model.wgsl`, `vs_main`).
    if let Some(r) = inst.rise {
        out.push([r.height, r.band.max(0.01), 0.0, 0.0]);
    }
    // A building's scars follow the drape (`model.wgsl`, `scarred`).
    if let Some(s) = scars {
        s.texels(out);
    }
}

/// The look texels of an instance (`model.wgsl`, `look_texel`): skin tint (linear) and
/// stubble; hair tint and age; pocks, scar side (-1, 0, 1), eye height (rest pose, m),
/// 1 + the face tile (#306, of `face_tiles`). Neutral without a look.
fn look_texels(look: Option<&looks::Look>, eye: f32, seed: u32, face_tiles: u8) -> [[f32; 4]; 3] {
    let Some(l) = look else { return [[1.0, 1.0, 1.0, 0.0], [1.0, 1.0, 1.0, 0.0], [0.0; 4]] };
    let (skin, hair) = looks::tints(l);
    let side = if !l.scar {
        0.0
    } else if seed & 1 == 0 {
        1.0
    } else {
        -1.0
    };
    [
        [skin[0], skin[1], skin[2], l.stubble],
        [hair[0], hair[1], hair[2], l.morphs[7]],
        [l.pocks, side, eye, 1.0 + f32::from(looks::face_tile(seed, face_tiles))],
    ]
}

/// One draw: the instances `range` of a model at a level of detail.
struct Draw {
    model: usize,
    range: Range<u32>,
    /// The inner layers show (decay opens the skin, or a wound): drawn too.
    decaying: bool,
    /// The instances can drop no fragment ([`Entry::clean`]).
    clean: bool,
    /// The look variant's vertex buffer.
    variant: Option<usize>,
    lod: usize,
    /// The parts drawn: all, or only those the look variants touch (`Some(true)`, one draw
    /// per variant) or leave (`Some(false)`, one draw across the variants).
    touched: Option<bool>,
}

/// An instance prepared, before the draws are grouped.
/// An instance culled and staged on its own (`prepare`), before its palette and data find
/// their places.
struct Staged {
    /// Its index in the frame's list (`usize::MAX` for caps and inserts).
    i: usize,
    /// As drawn (scaled to its look's height).
    inst: ModelInstance,
    /// Its instance texels (`instance_texels`).
    data: Vec<[f32; 4]>,
    entry: Entry,
}

struct Entry {
    gpu: GpuInstance,
    model: usize,
    decaying: bool,
    /// Intact, not half transparent, not dissolving: its parts in materials that drop
    /// nothing either ([`GpuModel::discards`]) draw without `discard` (`fs_clean`), so the
    /// tiler keeps its early depth test for them.
    clean: bool,
    /// Depth in the window (0..1): the instances of a draw go front to back.
    depth: f32,
    variant: Option<usize>,
    /// Within the window (else only a shadow caster).
    on_screen: bool,
    /// Damaged other than by wound volumes alone ([`Damage::splits_parts`]): the drawn
    /// list and the shadow casters keep such an instance at [`lod::DAMAGED_UP_TO`], as the
    /// far meshes' welded triangles span body parts; volumes alone draw from the far meshes
    /// (m2-perf: a battlefield of shot corpses otherwise drew whole).
    split: bool,
    origin: [f32; 3],
    /// Culling radius (world pixels).
    radius: f32,
}

struct GpuPrimitive {
    /// The index ranges of the levels of detail ([`lod`]), the full mesh first.
    lods: [Range<u32>; lod::LEVELS + 1],
    /// Moved by the face and body morphs: drawn from the look variant's vertex buffer.
    touched: bool,
    material: usize,
    /// Hidden inside the body until it decays (the bone layer): drawn for decaying
    /// instances only, a third of a human's triangles.
    inner: bool,
}

/// One material's far mesh ([`GpuModel::far`]).
struct FarPart {
    range: Range<u32>,
    material: usize,
    inner: bool,
}

struct GpuModel {
    data: ModelData,
    /// The far levels (from [`lod::FAR_FROM`], and the shadow list): one index range per
    /// material instead of one per primitive, the coarsest simplified as one shape
    /// ([`lod::far_mesh`]); empty for the near levels.
    far: [Vec<FarPart>; lod::LEVELS + 1],
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    primitives: Vec<GpuPrimitive>,
    /// Per material: always its own pictures (white for one not loaded); a level whose
    /// pictures are not all in is not drawn ([`Self::level_ready`], docs/rendering-3d.md
    /// "Texture streaming").
    materials: Vec<wgpu::BindGroup>,
    uniforms: Vec<wgpu::Buffer>,
    /// Per material: its authored level when finer than the coarsest ([`model::material_levels`]).
    levels: Vec<Option<u8>>,
    /// Per image: the texture, `None` while a streamed one is not loaded.
    views: Vec<Option<wgpu::TextureView>>,
    /// Per image: sRGB colours (not a normal or roughness map); built with mips.
    srgb: Vec<bool>,
    mips: Vec<bool>,
    /// Per material: it can drop fragments whatever the instance (an alpha cut-out, a stump
    /// cap's outline, a wound cavity's near wall).
    discards: Vec<bool>,
    /// The decay from which a layer dissolves (`gore=full`): the outer layers (the inner
    /// ones show from then), and any layer (the instance drops fragments from then).
    opens_at: f32,
    dissolves_at: f32,
    /// The baked face and body variants (#283, [`ModelData::bake_looks`]): the touched
    /// primitives' vertices, which come first in [`Self::vertices`].
    variants: Vec<wgpu::Buffer>,
    /// Eye height of the rest pose (m, `extras.acr_face_frame`).
    eye: f32,
    /// Head tiles of the face texture (#306, [`crate::model::Material::face_tiles`]); 1: none to pick.
    face_tiles: u8,
    /// The glb carries its own levels ([`lod::authored`]): chosen by [`lod::AUTHORED_SIZES`].
    authored: bool,
}

impl GpuModel {
    /// The bind group drawn for `material`: always its own. Another level's pictures belong
    /// to another UV layout (`hdbody.build_lods` packs each level's own atlases).
    fn group(&self, material: usize) -> &wgpu::BindGroup {
        &self.materials[material]
    }

    /// All pictures `material` samples are in (a streamed one loaded, or none streamed).
    fn material_ready(&self, material: usize) -> bool {
        self.images_of(material)
            .all(|i| self.views[i].is_some() || self.data.encoded.get(i).is_none_or(Option::is_none))
    }

    /// Every material drawn at `lod` has its own pictures in.
    fn level_ready(&self, lod: usize) -> bool {
        self.parts(lod, true).all(|(_, k, _)| self.material_ready(k))
    }

    /// The images `material` samples.
    fn images_of(&self, material: usize) -> impl Iterator<Item = usize> + '_ {
        let m = &self.data.materials[material];
        [m.texture, m.surface.normal, m.surface.metal_rough].into_iter().flatten().filter(|&i| i < self.views.len())
    }

    /// What a draw at `lod` draws: (index range, material, from the look variant's vertices).
    fn parts(&self, lod: usize, decaying: bool) -> impl Iterator<Item = (Range<u32>, usize, bool)> + '_ {
        let merged = !self.far[lod].is_empty();
        let near = self
            .primitives
            .iter()
            .filter(move |p| !merged && (decaying || !p.inner) && !p.lods[lod].is_empty())
            .map(move |p| (p.lods[lod].clone(), p.material, p.touched));
        let far =
            self.far[lod].iter().filter(move |f| decaying || !f.inner).map(|f| (f.range.clone(), f.material, false));
        near.chain(far)
    }

    /// The parts of [`Self::parts`] a draw takes ([`Draw::touched`]).
    fn draw_parts(&self, d: &Draw) -> impl Iterator<Item = (Range<u32>, usize, bool)> + '_ {
        let only = d.touched;
        self.parts(d.lod, d.decaying).filter(move |&(.., t)| only.is_none_or(|o| o == t))
    }
}

/// Splits the runs of draws that differ only in the look variant (sorted so, adjacent
/// instances): the parts no variant touches become one draw across the run, only the touched
/// ones stay a draw per variant. A soldier's face and body variants move a few of his ~23
/// primitives, so a crowd of mixed looks costs a draw per material plus a few per variant
/// instead of all of them per variant.
fn split_variants(draws: Vec<Draw>) -> Vec<Draw> {
    let mut out = Vec::with_capacity(draws.len());
    let mut rest = draws.as_slice();
    while let Some(first) = rest.first() {
        let n = rest
            .iter()
            .take_while(|d| {
                d.model == first.model && d.decaying == first.decaying && d.clean == first.clean && d.lod == first.lod
            })
            .count();
        let (run, tail) = rest.split_at(n);
        rest = tail;
        // Contiguous instances by construction ([`ModelRenderer::group_draws`]).
        if run.len() < 2 || run.windows(2).any(|w| w[0].range.end != w[1].range.start) {
            out.extend(run.iter().map(|d| Draw { range: d.range.clone(), ..*d }));
            continue;
        }
        out.push(Draw {
            range: first.range.start..run[run.len() - 1].range.end,
            variant: None,
            touched: Some(false),
            ..*first
        });
        out.extend(run.iter().map(|d| Draw { range: d.range.clone(), touched: Some(true), ..*d }));
    }
    out
}

/// Memory and time of the look variants baked by [`ModelRenderer::add_model`].
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LookStats {
    pub variants: usize,
    pub vertices_per_variant: usize,
    pub bytes: usize,
}

/// GPU memory of the model textures ([`ModelRenderer::texture_stats`]).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TextureStats {
    /// Loaded with their models (with mips).
    pub loaded_bytes: u64,
    /// Streamed pictures of finer authored levels now resident.
    pub streamed_bytes: u64,
    pub pending: usize,
    pub loads: u64,
    pub evictions: u64,
}

/// A streamed picture going up ([`ModelRenderer::uploads`]): its full size in strips, then
/// the mips on the GPU.
struct Upload {
    key: tex_cache::Key,
    pixels: tex_cache::Pixels,
    /// None for transcoded KTX2 pictures (all levels go up at once, `ktx2::Blocks::upload`).
    texture: Option<wgpu::Texture>,
    srgb: bool,
    /// The next row (RGBA) or strip (browser bitmaps).
    next: u32,
}

impl Upload {
    /// Writes rows up to `budget` bytes (at least one); true once the full size is up.
    fn write(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, budget: &mut u64, srgb: bool) -> bool {
        let Some(texture) = &self.texture else {
            if let tex_cache::Pixels::Blocks(blocks) = &self.pixels {
                *budget = budget.saturating_sub(blocks.bytes());
                self.texture = Some(blocks.upload(device, queue, "model texture (streamed KTX2)", srgb));
            }
            return true;
        };
        match &self.pixels {
            tex_cache::Pixels::Blocks(_) => true,
            tex_cache::Pixels::Rgba(img) => {
                let row_bytes = u64::from(img.width) * 4;
                while self.next < img.height && *budget > 0 {
                    let rows = ((*budget / row_bytes) as u32).clamp(1, img.height - self.next);
                    let from = (u64::from(self.next) * row_bytes) as usize;
                    queue.write_texture(
                        wgpu::TexelCopyTextureInfo {
                            texture,
                            mip_level: 0,
                            origin: wgpu::Origin3d { x: 0, y: self.next, z: 0 },
                            aspect: wgpu::TextureAspect::All,
                        },
                        &img.rgba[from..from + (u64::from(rows) * row_bytes) as usize],
                        wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(img.width * 4),
                            rows_per_image: None,
                        },
                        wgpu::Extent3d { width: img.width, height: rows, depth_or_array_layers: 1 },
                    );
                    *budget = budget.saturating_sub(u64::from(rows) * row_bytes);
                    self.next += rows;
                }
                self.next >= img.height
            }
            #[cfg(target_arch = "wasm32")]
            tex_cache::Pixels::Bitmaps { width, strips, .. } => {
                while let Some((y, strip)) = strips.get(self.next as usize) {
                    if *budget == 0 {
                        break;
                    }
                    let rows = strip.height();
                    queue.copy_external_image_to_texture(
                        &wgpu::CopyExternalImageSourceInfo {
                            source: wgpu::ExternalImageSource::ImageBitmap(strip.clone()),
                            origin: wgpu::Origin2d::ZERO,
                            flip_y: false,
                        },
                        wgpu::CopyExternalImageDestInfo {
                            texture,
                            mip_level: 0,
                            origin: wgpu::Origin3d { x: 0, y: *y, z: 0 },
                            aspect: wgpu::TextureAspect::All,
                            color_space: wgpu::PredefinedColorSpace::Srgb,
                            premultiplied_alpha: false,
                        },
                        wgpu::Extent3d { width: *width, height: rows, depth_or_array_layers: 1 },
                    );
                    strip.close();
                    *budget = budget.saturating_sub(u64::from(*width) * u64::from(rows) * 4);
                    self.next += 1;
                }
                self.next as usize >= strips.len()
            }
        }
    }
}

/// An empty model texture of `width` x `height`, with a full mip chain when `mips`
/// ([`mipgen::MipGen`] fills it from level 0).
fn model_texture(device: &wgpu::Device, label: &str, width: u32, height: u32, srgb: bool, mips: bool) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: if mips { mipgen::mip_levels(width, height) } else { 1 },
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: if srgb { wgpu::TextureFormat::Rgba8UnormSrgb } else { wgpu::TextureFormat::Rgba8Unorm },
        // RENDER_ATTACHMENT: the mip passes, and the browser's bitmap copies.
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    })
}

/// A texture from a decoded picture, its mips made on the GPU when `mips`.
fn upload_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    mipgen: &mut mipgen::MipGen,
    image: &crate::model::Image,
    srgb: bool,
    mips: bool,
) -> wgpu::TextureView {
    let texture = model_texture(device, "model texture", image.width, image.height, srgb, mips);
    queue.write_texture(
        texture.as_image_copy(),
        &image.rgba,
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(image.width * 4), rows_per_image: None },
        wgpu::Extent3d { width: image.width, height: image.height, depth_or_array_layers: 1 },
    );
    mipgen.generate(device, queue, &texture);
    texture.create_view(&Default::default())
}

/// The level a model wanted at `wanted` draws at: the first from it, coarser up to `last`,
/// whose materials all have their own pictures in (`ready`), so a mesh never samples pictures
/// made for another level's UVs while the finer ones stream; `wanted` when none is.
fn shown_level(wanted: usize, last: usize, ready: impl Fn(usize) -> bool) -> usize {
    (wanted..=last.max(wanted)).find(|&l| ready(l)).unwrap_or(wanted)
}

/// A model with its CPU work done ([`ModelRenderer::prepare_model`]), ready to upload.
pub struct PreparedModel {
    data: ModelData,
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
    primitives: Vec<GpuPrimitive>,
    far: [Vec<FarPart>; lod::LEVELS + 1],
    baked: Vec<Vec<Vertex>>,
    have: Vec<u8>,
    authored: bool,
}

pub struct ModelRenderer {
    pipeline: wgpu::RenderPipeline,
    /// The cheaper fragments of the far levels (from [`lod::FAR_FROM`]).
    far_pipeline: wgpu::RenderPipeline,
    /// Both without `discard`, for the parts that drop nothing ([`Entry::clean`]).
    clean_pipeline: wgpu::RenderPipeline,
    far_clean_pipeline: wgpu::RenderPipeline,
    shadow_clean_pipeline: wgpu::RenderPipeline,
    pool_pipeline: wgpu::RenderPipeline,
    pools: wgpu::Buffer,
    pool_count: u32,
    globals: wgpu::Buffer,
    globals_layout: wgpu::BindGroupLayout,
    globals_group: wgpu::BindGroup,
    material_layout: wgpu::BindGroupLayout,
    bones: wgpu::Texture,
    instances: wgpu::Buffer,
    linear: wgpu::Sampler,
    nearest: wgpu::Sampler,
    white: wgpu::TextureView,
    /// The streamed pictures of authored levels ([`tex_cache`]).
    textures: tex_cache::TexCache,
    decoder: tex_cache::Decoder,
    /// Decoded pictures going up in strips of [`tex_cache::UPLOAD_PER_FRAME`] bytes a frame.
    uploads: std::collections::VecDeque<Upload>,
    /// The mip levels of every model texture.
    mipgen: mipgen::MipGen,
    /// GPU bytes of the model textures loaded with their models (not streamed).
    eager_bytes: u64,
    /// KTX2 pictures already transcoded and uploaded, by file (its `Arc`, kept so the address
    /// stays unique), sRGB and mips: the parts a model is split into (unarmed body, dropped
    /// gear, building states) share the file and take the texture instead of transcoding
    /// it again.
    ktx2_views: Vec<(std::sync::Arc<[u8]>, bool, bool, wgpu::TextureView)>,
    models: Vec<GpuModel>,
    /// The model placed at every cut ([`damage::procedural_cap`] until [`Self::set_cap`]).
    cap: usize,
    /// The inside of wounds (#280): cavity, rib, bone end ([`damage::InsertKind`] order).
    inserts: [usize; 3],
    /// Buildings' hit damage (#304) for the next `prepare`, by instance index.
    scars: HashMap<usize, Scars>,
    /// Small models always drawn whole ([`Self::add_small_model`]): their far levels would
    /// simplify thin parts away.
    small: Vec<usize>,
    /// Prepared for the next `render`: the instances on the screen.
    draws: Vec<Draw>,
    /// Prepared for the next `render_shadows`: the casters within the shadow map, coarser.
    shadow_draws: Vec<Draw>,
    /// The level of detail each model was drawn at last (hysteresis, [`lod::choose`]).
    lod_last: Vec<Option<usize>>,
    /// Levels of detail by projected size (docs/rendering-3d.md, "Levels of detail"), on by
    /// default (the game's `lod=on|off`). `ACR_LOD` overrides it (native only): `off`, `on`,
    /// or `<level>` forcing one level.
    pub lods: bool,
    pub lod_override: Option<bool>,
    pub lod_force: Option<usize>,
    /// Triangles drawn by the last `prepare`: models, shadow casters.
    pub triangles: [u64; 2],
    /// Draw calls of the last `prepare`: models, shadow casters.
    pub calls: [usize; 2],
    /// Wall time of the last `prepare`'s parts (ns): the per-instance work (looks, staging,
    /// poses, caps), the palette layout, [`Self::group_draws`], the uploads. Zero in the
    /// browser (no clock there).
    pub prepare_ns: [u64; 4],
    /// The look variants of the last model added.
    pub look_stats: LookStats,
    /// The target does not encode sRGB: the shader does.
    encode_srgb: bool,
    /// Palettes uploaded by the last `prepare`.
    pub pose_count: usize,
    /// Instances drawn by the last `prepare`.
    pub drawn: usize,
    /// Palettes given from outside (a physics ragdoll) for instances by their index in the
    /// next `prepare`'s slice, in place of their clip (`set_custom_poses`).
    custom: HashMap<usize, crate::model::Palette>,
    /// The custom palettes the last `prepare` drew (the studio's measurements, #279).
    shown_custom: HashMap<usize, crate::model::Palette>,
    /// Sun shadows (`shadow.rs`): on unless switched off (the `shadows` setting).
    pub shadows: bool,
    /// Normal maps and the highlight from the glTF roughness and metallic (#234): on unless
    /// switched off (the `material_detail` setting).
    pub material_detail: bool,
    /// Wind for the `_WIND` weights of trees and foliage (docs/rendering-3d.md "Wind"): world
    /// direction x, y (unit), strength (0: still, the `wind=off` setting), time in seconds.
    pub wind: [f32; 4],
    /// The weather's look of the models (`weather::Look::model`, `model_fog`); zero: none.
    pub weather: [[f32; 4]; 2],
    shadow: ShadowMap,
    /// The models into the shadow map: `vs_main` through the light's globals, `fs_shadow`.
    shadow_pipeline: wgpu::RenderPipeline,
    shadow_globals: wgpu::Buffer,
    shadow_globals_group: wgpu::BindGroup,
    /// Profiling switches (`ACR_MODEL_DEBUG`, native only, [`debug_flags`]).
    debug: u32,
    debug_file: bool,
}

/// `ACR_MODEL_DEBUG` (a comma list; native only, for profiling the models pass by kind):
/// `living`, `dead`, `dead_volumes`, `dead_plain` leave out those instances; `inner` draws no
/// inner decay layers; `caps` no caps and wound inserts; `gore` no blood, wounds, marks or
/// spray; `holes` no wound volumes; `looks` no look variants.
pub mod debug {
    pub const LIVING: u32 = 1;
    pub const DEAD: u32 = 2;
    pub const DEAD_VOLUMES: u32 = 4;
    pub const DEAD_PLAIN: u32 = 8;
    pub const INNER: u32 = 16;
    pub const CAPS: u32 = 32;
    pub const GORE: u32 = 64;
    pub const HOLES: u32 = 128;
    pub const LOOKS: u32 = 256;
    /// The state before m2-perf3: inner layers from the first decay, no clean pipeline.
    pub const OLD_INNER: u32 = 512;
    pub const NO_CLEAN: u32 = 1024;
}

fn debug_flags() -> u32 {
    let names = [
        "living",
        "dead",
        "dead_volumes",
        "dead_plain",
        "inner",
        "caps",
        "gore",
        "holes",
        "looks",
        "old_inner",
        "no_clean",
    ];
    // `ACR_MODEL_DEBUG_FILE`: the list read from a file every frame (switched while running).
    let from_file = std::env::var("ACR_MODEL_DEBUG_FILE").ok().and_then(|f| std::fs::read_to_string(f).ok());
    from_file
        .ok_or(())
        .or_else(|_| std::env::var("ACR_MODEL_DEBUG").map_err(|_| ()))
        .map(|v| v.split(',').filter_map(|w| names.iter().position(|n| *n == w.trim())).fold(0, |f, k| f | (1 << k)))
        .unwrap_or(0)
}

/// `model.wgsl` with the shadow receiver (`shadow.wgsl`) and casters (`model_shadow.wgsl`).
pub const MODEL_SHADER: &str =
    concat!(include_str!("model.wgsl"), include_str!("shadow.wgsl"), include_str!("model_shadow.wgsl"));

fn model_shader(device: &wgpu::Device) -> wgpu::ShaderModule {
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("model.wgsl"),
        source: wgpu::ShaderSource::Wgsl(MODEL_SHADER.into()),
    })
}

fn bone_texture(device: &wgpu::Device, rows: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("model bones"),
        size: wgpu::Extent3d { width: BONE_WIDTH, height: rows, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba32Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

/// `look.z` of a decal in `blood.wgsl`: 0 pool, 1 droplet, 2 crater, 3 scuff, 4 smear,
/// 5 decay stain.
fn pool_kind(p: &BloodPool) -> f32 {
    match p.dirt {
        Dirt::Crater => 2.0,
        Dirt::Scuff => 3.0,
        Dirt::Seep => 5.0,
        Dirt::None if p.smear.is_some() => 4.0,
        Dirt::None => f32::from(u8::from(p.splatter)),
    }
}

/// The GPU form of a decal: a smear's quad reaches `length` past its start (`stretch` in
/// pool radii), its start width and density ride in `tilt.z` and `look.w`; a decay stain's
/// dead grass and growth too.
fn gpu_pool(p: &BloodPool) -> GpuPool {
    let (stretch, start, density) = match p.smear {
        Some(s) => (s.length / (2.0 * p.radius), s.start_width, s.density),
        None if p.dirt == Dirt::Seep => (p.stretch, p.seep.ring, p.seep.growth),
        None => (p.stretch, 0.0, 0.0),
    };
    GpuPool {
        centre: [p.centre[0], p.centre[1], p.centre[2], p.radius],
        shape: [p.along[0], p.along[1], stretch, p.seed],
        look: [p.age, p.fade, pool_kind(p), density],
        tilt: [p.tilt[0], p.tilt[1], start, 0.0],
    }
}

fn pool_buffer(device: &wgpu::Device, count: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("blood pools"),
        size: count.max(1) * size_of::<GpuPool>() as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn instance_buffer(device: &wgpu::Device, count: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("model instances"),
        size: count.max(1) * size_of::<GpuInstance>() as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

impl ModelRenderer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, target: wgpu::TextureFormat) -> Self {
        crate::ktx2::set_target(crate::ktx2::Target::of(device.features()));
        let shader = model_shader(device);
        let globals_buffer = |label| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: size_of::<Globals>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let globals = globals_buffer("model globals");
        let shadow_globals = globals_buffer("model shadow globals");
        let shadow = ShadowMap::new(device);
        let both = wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT;
        let uniform = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: both,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let texture = |binding, visibility, filterable| wgpu::BindGroupLayoutEntry {
            binding,
            visibility,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("model globals"),
            entries: &[uniform(0), texture(1, both, false)],
        });
        let material_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("model material"),
            entries: &[
                uniform(0),
                texture(1, wgpu::ShaderStages::FRAGMENT, true),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                // The normal map and the metallic-roughness map (#234).
                texture(3, wgpu::ShaderStages::FRAGMENT, true),
                texture(4, wgpu::ShaderStages::FRAGMENT, true),
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("model"),
            bind_group_layouts: &[Some(&globals_layout), Some(&material_layout), Some(&shadow.layout)],
            immediate_size: 0,
        });
        // The shadow pass cannot bind the map it draws into: no group 2.
        let shadow_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("model shadow"),
            bind_group_layouts: &[Some(&globals_layout), Some(&material_layout)],
            immediate_size: 0,
        });
        // All 16 locations WebGL2 has: wounds, spray and damage live in the bone texture.
        let vertex_attributes = wgpu::vertex_attr_array![
            0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32x4,
            4 => Uint32x4, 5 => Float32x4, 6 => Float32x3, 7 => Float32x3, 8 => Float32x4,
            14 => Unorm8x4, 15 => Unorm8x4
        ];
        let instance_attributes = wgpu::vertex_attr_array![
            9 => Float32x4, 10 => Float32x4, 11 => Float32x4, 12 => Float32x4, 13 => Uint32x4
        ];
        let stencil_mark = wgpu::StencilFaceState {
            compare: wgpu::CompareFunction::Always,
            fail_op: wgpu::StencilOperation::Keep,
            depth_fail_op: wgpu::StencilOperation::Keep,
            pass_op: wgpu::StencilOperation::Replace,
        };
        let model_pipeline = |fragment: &str| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(fragment),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[
                        Some(wgpu::VertexBufferLayout {
                            array_stride: size_of::<Vertex>() as u64,
                            step_mode: wgpu::VertexStepMode::Vertex,
                            attributes: &vertex_attributes,
                        }),
                        Some(wgpu::VertexBufferLayout {
                            array_stride: size_of::<GpuInstance>() as u64,
                            step_mode: wgpu::VertexStepMode::Instance,
                            attributes: &instance_attributes,
                        }),
                    ],
                },
                // Clothes are thin shells seen from both sides (holes when they decay): no culling.
                primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::Less),
                    stencil: wgpu::StencilState {
                        front: stencil_mark,
                        back: stencil_mark,
                        read_mask: 0xff,
                        write_mask: 0xff,
                    },
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fragment),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: target,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let pipeline = model_pipeline("fs_main");
        let far_pipeline = model_pipeline("fs_far");
        let clean_pipeline = model_pipeline("fs_clean");
        let far_clean_pipeline = model_pipeline("fs_far_clean");
        let shadow_pipeline = |fragment: &str| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("model shadow"),
                layout: Some(&shadow_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[
                        Some(wgpu::VertexBufferLayout {
                            array_stride: size_of::<Vertex>() as u64,
                            step_mode: wgpu::VertexStepMode::Vertex,
                            attributes: &vertex_attributes,
                        }),
                        Some(wgpu::VertexBufferLayout {
                            array_stride: size_of::<GpuInstance>() as u64,
                            step_mode: wgpu::VertexStepMode::Instance,
                            attributes: &instance_attributes,
                        }),
                    ],
                },
                primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
                depth_stencil: Some(shadow::caster_depth()),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fragment),
                    compilation_options: Default::default(),
                    targets: &[],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let shadow_clean_pipeline = shadow_pipeline("fs_shadow_clean");
        let shadow_pipeline = shadow_pipeline("fs_shadow");
        // Blood pools: decals over the terrain's depth, blended, writing neither depth nor
        // stencil (the models and sprites cover them).
        let pool_shader = device.create_shader_module(wgpu::include_wgsl!("blood.wgsl"));
        let pool_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("blood pools"),
            bind_group_layouts: &[Some(&globals_layout)],
            immediate_size: 0,
        });
        let pool_attributes = wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4];
        let pool_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("blood pools"),
            layout: Some(&pool_layout),
            vertex: wgpu::VertexState {
                module: &pool_shader,
                entry_point: Some("vs_pool"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: size_of::<GpuPool>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &pool_attributes,
                })],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &pool_shader,
                entry_point: Some("fs_pool"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let bones = bone_texture(device, 1);
        let globals_group = Self::globals_group(device, &globals_layout, &globals, &bones);
        let shadow_globals_group = Self::globals_group(device, &globals_layout, &shadow_globals, &bones);
        let sampler = |filter| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("model texture"),
                address_mode_u: wgpu::AddressMode::Repeat,
                address_mode_v: wgpu::AddressMode::Repeat,
                mag_filter: filter,
                min_filter: filter,
                // Model textures carry mips (`mipgen`).
                mipmap_filter: if filter == wgpu::FilterMode::Linear {
                    wgpu::MipmapFilterMode::Linear
                } else {
                    wgpu::MipmapFilterMode::Nearest
                },
                ..Default::default()
            })
        };
        let white = device
            .create_texture_with_data(
                queue,
                &wgpu::TextureDescriptor {
                    label: Some("model white"),
                    size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                },
                wgpu::util::TextureDataOrder::LayerMajor,
                &[255; 4],
            )
            .create_view(&Default::default());
        let mut renderer = Self {
            pipeline,
            far_pipeline,
            clean_pipeline,
            far_clean_pipeline,
            shadow_clean_pipeline,
            pool_pipeline,
            pools: pool_buffer(device, 64),
            pool_count: 0,
            globals,
            globals_layout,
            globals_group,
            material_layout,
            bones,
            instances: instance_buffer(device, 256),
            linear: sampler(wgpu::FilterMode::Linear),
            nearest: sampler(wgpu::FilterMode::Nearest),
            white,
            textures: Default::default(),
            decoder: Default::default(),
            uploads: Default::default(),
            mipgen: mipgen::MipGen::new(device),
            eager_bytes: 0,
            ktx2_views: Vec::new(),
            models: Vec::new(),
            cap: 0,
            inserts: [0; 3],
            scars: HashMap::new(),
            small: Vec::new(),
            draws: Vec::new(),
            shadow_draws: Vec::new(),
            lod_last: Vec::new(),
            lods: true,
            lod_override: std::env::var("ACR_LOD").ok().map(|v| v != "off"),
            lod_force: std::env::var("ACR_LOD")
                .ok()
                .and_then(|v| v.parse::<usize>().ok())
                .map(|l| l.min(lod::LEVELS - 1)),
            triangles: [0; 2],
            calls: [0; 2],
            prepare_ns: [0; 4],
            look_stats: LookStats::default(),
            encode_srgb: !target.is_srgb(),
            pose_count: 0,
            drawn: 0,
            custom: HashMap::new(),
            shown_custom: HashMap::new(),
            shadows: true,
            material_detail: true,
            wind: [1.0, 0.0, 0.0, 0.0],
            weather: [[0.0; 4]; 2],
            shadow,
            shadow_pipeline,
            shadow_globals,
            shadow_globals_group,
            debug: debug_flags(),
            debug_file: std::env::var("ACR_MODEL_DEBUG_FILE").is_ok(),
        };
        renderer.cap = renderer.add_model(device, queue, damage::procedural_cap());
        renderer.inserts = [
            renderer.add_model(device, queue, damage::procedural_cavity()),
            renderer.add_model(device, queue, damage::procedural_rib()),
            renderer.add_model(device, queue, damage::procedural_bone()),
        ];
        renderer
    }

    /// Replaces the stump cap model (the modelled cap of `gore.glb`): a unit disc in xy
    /// facing +z, placed at every cut and scaled by the limb's radius.
    pub fn set_cap(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, data: ModelData) {
        let index = self.add_model(device, queue, data);
        self.models.swap(self.cap, index);
        self.models.pop();
    }

    fn globals_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        globals: &wgpu::Buffer,
        bones: &wgpu::Texture,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("model globals"),
            layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: globals.as_entire_binding() },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&bones.create_view(&Default::default())),
                },
            ],
        })
    }

    /// The far levels of a model whose primitives' levels are in `indices` (appended to it):
    /// per material (and inner layer), the primitives' lists of the level before
    /// [`lod::FAR_FROM`] joined and simplified as one ([`lod::far_mesh`]) for each far level,
    /// the shadow list [`lod::far_shadow`] of the last.
    ///
    /// Each list gets its own copy of the vertices it uses, appended to `vertices`: a joined
    /// list would otherwise span the whole model's vertices, and a tiler may shade every vertex
    /// between the lowest and the highest index of a draw (the shadow pass doubled so).
    fn far_meshes(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        primitives: &[GpuPrimitive],
    ) -> [Vec<FarPart>; lod::LEVELS + 1] {
        const { assert!(lod::LOOKS_UP_TO < lod::FAR_FROM && lod::FAR_FROM == lod::LEVELS - 2) };
        let mut groups: Vec<(usize, bool)> = Vec::new();
        for p in primitives {
            if !groups.contains(&(p.material, p.inner)) {
                groups.push((p.material, p.inner));
            }
        }
        let mut far: [Vec<FarPart>; lod::LEVELS + 1] = Default::default();
        for (material, inner) in groups {
            let members: Vec<&GpuPrimitive> =
                primitives.iter().filter(|p| p.material == material && p.inner == inner).collect();
            let full: usize = members.iter().map(|p| p.lods[0].len() / 3).sum();
            let near: Vec<u32> = members
                .iter()
                .flat_map(|p| {
                    let r = &p.lods[lod::FAR_FROM - 1];
                    indices[r.start as usize..r.end as usize].to_vec()
                })
                .collect();
            let middle = lod::far_mesh(vertices, &near, full, lod::FAR_FROM);
            let last = lod::far_mesh(vertices, &middle, full, lod::LEVELS - 1);
            let shadow = lod::far_shadow(vertices, &last, full);
            for (level, list) in [(lod::FAR_FROM, middle), (lod::LEVELS - 1, last), (lod::SHADOW, shadow)] {
                let start = indices.len() as u32;
                let mut copy: HashMap<u32, u32> = HashMap::new();
                for i in list {
                    let j = *copy.entry(i).or_insert_with(|| {
                        vertices.push(vertices[i as usize]);
                        vertices.len() as u32 - 1
                    });
                    indices.push(j);
                }
                far[level].push(FarPart { range: start..indices.len() as u32, material, inner });
            }
        }
        far
    }

    /// [`Self::add_model`] for a small model (missiles, #308) that is always drawn at its
    /// full level of detail.
    pub fn add_small_model(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, data: ModelData) -> usize {
        let m = self.add_model(device, queue, data);
        self.small.push(m);
        m
    }

    /// Uploads a model; returns its index for [`ModelInstance::model`].
    pub fn add_model(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, data: ModelData) -> usize {
        self.add_prepared(device, queue, Self::prepare_model(data))
    }

    /// The CPU work of [`Self::add_model`] (the looks baked, the levels of detail and far
    /// meshes built): without the device, so it can run on a worker thread (#303 building
    /// pieces); [`Self::add_prepared`] uploads the result.
    pub fn prepare_model(data: ModelData) -> PreparedModel {
        let mut vertices: Vec<Vertex> = Vec::new();
        let mut indices: Vec<u32> = Vec::new();
        // The look variants: the primitives they touch first, so a variant's buffer holds
        // just those and shares the index numbering.
        let baked = data.bake_looks().unwrap_or_default();
        let touched = |p: &crate::model::Primitive| !baked.is_empty() && !p.look_morphs.is_empty();
        let mut slots: Vec<Option<GpuPrimitive>> = (0..data.primitives.len()).map(|_| None).collect();
        // Authored levels (the HD body): the levels the glb carries, the master (0) first.
        let mut have: Vec<u8> = data.primitives.iter().filter_map(|p| p.lod).collect();
        have.sort_unstable();
        have.dedup();
        let authored = have.iter().any(|&l| l > 0);
        if authored && have.first() != Some(&0) {
            have.insert(0, 0);
        }
        for pass in [true, false] {
            for (k, p) in data.primitives.iter().enumerate().filter(|(_, p)| touched(p) == pass) {
                // Indices are offset here: WebGL2 has no base vertex.
                let base = vertices.len() as u32;
                vertices.extend_from_slice(&p.vertices);
                let levels = match p.lod.filter(|_| authored) {
                    Some(own) => lod::authored(&p.vertices, &p.indices, own.min(lod::LEVELS as u8 - 1), &have),
                    None => lod::levels(&p.vertices, &p.indices),
                };
                let lods = std::array::from_fn(|l| {
                    let start = indices.len() as u32;
                    indices.extend(levels[l].iter().map(|i| i + base));
                    start..indices.len() as u32
                });
                let inner = data.materials.get(p.material).and_then(|m| m.decay).is_some_and(|d| d.inner);
                slots[k] = Some(GpuPrimitive { lods, touched: pass, material: p.material, inner });
            }
        }
        let primitives: Vec<GpuPrimitive> = slots.into_iter().flatten().collect();
        // The authored levels are the far meshes themselves.
        let far =
            if authored { Default::default() } else { Self::far_meshes(&mut vertices, &mut indices, &primitives) };
        PreparedModel { data, vertices, indices, primitives, far, baked, have, authored }
    }

    /// Uploads a model [`Self::prepare_model`] built; answers its index.
    pub fn add_prepared(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, prepared: PreparedModel) -> usize {
        let PreparedModel { data, vertices, indices, primitives, far, baked, have, authored } = prepared;
        let variants: Vec<wgpu::Buffer> = baked
            .iter()
            .map(|v| {
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("model look variant"),
                    contents: bytemuck::cast_slice(v),
                    usage: wgpu::BufferUsages::VERTEX,
                })
            })
            .collect();
        let per = baked.first().map_or(0, Vec::len);
        self.look_stats = LookStats {
            variants: baked.len(),
            vertices_per_variant: per,
            bytes: baked.len() * per * size_of::<Vertex>(),
        };
        let eye = data.looks.as_ref().map_or(0.0, |l| l.eye);
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("model vertices"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("model indices"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        // Normal and metallic-roughness maps hold data, not colours: no sRGB decode.
        let srgb: Vec<bool> = (0..data.images.len())
            .map(|i| !data.materials.iter().any(|m| m.surface.normal == Some(i) || m.surface.metal_rough == Some(i)))
            .collect();
        // Mips for all but the projected sprite colours (nearest filtering keeps their texels).
        let mips: Vec<bool> =
            (0..data.images.len()).map(|i| !data.materials.iter().any(|m| m.nearest && m.texture == Some(i))).collect();
        // The pictures of finer authored levels stay encoded until drawn (`tex_cache`).
        // KTX2 pictures are transcoded for this device (`ktx2`), with the file's own mips.
        let mut gpu_bytes = vec![0; data.images.len()];
        let views: Vec<Option<wgpu::TextureView>> = data
            .images
            .iter()
            .enumerate()
            .map(|(i, img)| {
                if let Some(file) = data.compressed.get(i).and_then(Option::as_ref) {
                    let key = |(f, s, m, _): &&(std::sync::Arc<[u8]>, bool, bool, wgpu::TextureView)| {
                        std::sync::Arc::ptr_eq(f, file) && *s == srgb[i] && *m == mips[i]
                    };
                    if let Some((.., view)) = self.ktx2_views.iter().find(key) {
                        return Some(view.clone());
                    }
                    return match crate::ktx2::transcode(file, crate::ktx2::target(), mips[i]) {
                        Ok(blocks) => {
                            gpu_bytes[i] = blocks.bytes();
                            self.eager_bytes += gpu_bytes[i];
                            let texture = blocks.upload(device, queue, "model texture (KTX2)", srgb[i]);
                            let view = texture.create_view(&Default::default());
                            self.ktx2_views.push((file.clone(), srgb[i], mips[i], view.clone()));
                            Some(view)
                        }
                        Err(e) => {
                            log::warn!("model: image {i}: {e}");
                            None
                        }
                    };
                }
                if img.rgba.is_empty() && data.encoded.get(i).is_some_and(Option::is_some) {
                    let ktx2 = data.encoded[i].as_deref().is_some_and(crate::ktx2::is_ktx2);
                    gpu_bytes[i] = tex_cache::texture_bytes(img.width, img.height, mips[i])
                        / if ktx2 && crate::ktx2::target() != crate::ktx2::Target::Rgba { 4 } else { 1 };
                    return None;
                }
                gpu_bytes[i] = tex_cache::texture_bytes(img.width, img.height, mips[i]);
                self.eager_bytes += gpu_bytes[i];
                Some(upload_texture(device, queue, &mut self.mipgen, img, srgb[i], mips[i]))
            })
            .collect();
        let levels = crate::model::material_levels(&data.primitives, data.materials.len());
        let texture_bytes = |streamed: bool| -> u64 {
            (0..data.images.len())
                .filter(|&i| data.encoded.get(i).is_some_and(Option::is_some) == streamed)
                .map(|i| gpu_bytes[i])
                .sum()
        };
        log::log!(
            if authored { log::Level::Info } else { log::Level::Debug },
            "model: {} vertices, {} indices, {} textures {:.1} MB loaded, {:.1} MB streamed (with mips), {:.1} MB buffers{}",
            vertices.len(),
            indices.len(),
            data.images.len(),
            texture_bytes(false) as f64 / 1e6,
            texture_bytes(true) as f64 / 1e6,
            (vertices.len() * size_of::<Vertex>() + indices.len() * 4) as f64 / 1e6,
            if authored { format!(", authored levels {have:?}") } else { String::new() }
        );
        // `ACR_LOD_TINT` (native, a test aid): each authored level's materials tinted
        // (master white, then green, yellow, red).
        let tint_by_level = cfg!(not(target_arch = "wasm32")) && authored && std::env::var("ACR_LOD_TINT").is_ok();
        let level_of_material =
            |m: usize| data.primitives.iter().find(|p| p.material == m).and_then(|p| p.lod).map_or(0, usize::from);
        let procedural = data.decays_procedurally();
        let discards = data
            .materials
            .iter()
            .map(|m| m.alpha_cutoff.unwrap_or(0.0) > 0.0 || m.stump || m.tissue as u8 == 1)
            .collect();
        let dissolve_from = |inner: Option<bool>| {
            data.materials
                .iter()
                .filter_map(|m| m.decay)
                .filter(|d| inner.is_none_or(|i| d.inner == i))
                .filter_map(|d| d.dissolve.map(|w| w[0]))
                .fold(if procedural { 0.62 } else { 2.0 }, f32::min)
        };
        let (opens_at, dissolves_at) = (dissolve_from(Some(false)), dissolve_from(None));
        let uniforms = data
            .materials
            .iter()
            .enumerate()
            .map(|(k, m)| {
                let d = m.decay;
                const TINTS: [[f32; 4]; lod::LEVELS] =
                    [[1.0; 4], [0.35, 1.0, 0.35, 1.0], [1.0, 0.9, 0.2, 1.0], [1.0, 0.25, 0.2, 1.0]];
                let tint = if tint_by_level { TINTS[level_of_material(k).min(lod::LEVELS - 1)] } else { [1.0; 4] };
                let uniform = MaterialUniform {
                    base: std::array::from_fn(|c| m.base_color[c] * tint[c]),
                    tint: d.map_or([0.0; 4], |d| [d.tint[0], d.tint[1], d.tint[2], d.tint_strength]),
                    windows: d.map_or([0.0, 1.0, 2.0, 2.0], |d| {
                        let [s0, s1] = d.dissolve.unwrap_or([2.0, 2.0]);
                        [d.discolour[0], d.discolour[1], s0, s1]
                    }),
                    edge: d.map_or([0.0; 4], |d| [d.edge_rgb[0], d.edge_rgb[1], d.edge_rgb[2], d.edge]),
                    flags: [
                        if d.is_some() { 1.0 } else { 0.0 },
                        if d.is_some() { 0.0 } else { 1.0 },
                        m.alpha_cutoff.unwrap_or(0.0),
                        // The bone layer takes no blood, dirt or burns.
                        if d.is_some_and(|d| d.inner) || m.stump { 1.0 } else { 0.0 },
                    ],
                    look: [
                        if m.stump { 1.0 } else { 0.0 },
                        m.tissue as u8 as f32,
                        m.tint as u8 as f32,
                        if procedural { 1.0 } else { 0.0 },
                    ],
                    // w: tiles per side of a face texture with several heads (#306).
                    cloth: [
                        m.finish.sheen,
                        m.finish.gloss,
                        m.finish.layer,
                        if m.face_tiles > 1 { f32::from(looks::tile_grid(m.face_tiles)) } else { 0.0 },
                    ],
                    wind: data.wind,
                    pbr: [
                        m.surface.metallic,
                        m.surface.roughness,
                        if m.surface.normal.is_some_and(|t| t < views.len()) { m.surface.normal_scale } else { 0.0 },
                        if m.surface.metal_rough.is_some_and(|t| t < views.len()) { 1.0 } else { 0.0 },
                    ],
                    thin: [m.finish.thin, 0.0, 0.0, 0.0],
                };
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("model material"),
                    contents: bytemuck::bytes_of(&uniform),
                    usage: wgpu::BufferUsages::UNIFORM,
                })
            })
            .collect::<Vec<_>>();
        let face_tiles = data.materials.iter().map(|m| m.face_tiles).max().unwrap_or(1);
        self.models.push(GpuModel {
            data,
            vertices: vertex_buffer,
            indices: index_buffer,
            primitives,
            far,
            materials: Vec::new(),
            uniforms,
            levels,
            views,
            srgb,
            mips,
            discards,
            opens_at,
            dissolves_at,
            variants,
            eye,
            face_tiles,
            authored,
        });
        let index = self.models.len() - 1;
        let groups =
            (0..self.models[index].data.materials.len()).map(|k| self.material_group(device, index, k)).collect();
        self.models[index].materials = groups;
        index
    }

    /// The bind group of a model's material: its own pictures, white for one not loaded (a
    /// level with such a material is not drawn while it streams, [`GpuModel::level_ready`]).
    fn material_group(&self, device: &wgpu::Device, model: usize, k: usize) -> wgpu::BindGroup {
        let g = &self.models[model];
        let m = &g.data.materials[k];
        let view = |t: Option<usize>| t.and_then(|t| g.views.get(t)).and_then(Option::as_ref).unwrap_or(&self.white);
        let sampler = if m.nearest { &self.nearest } else { &self.linear };
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("model material"),
            layout: &self.material_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: g.uniforms[k].as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(view(m.texture)) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(sampler) },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(view(m.surface.normal)),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(view(m.surface.metal_rough)),
                },
            ],
        })
    }

    /// Asks for the streamed pictures of `model`'s materials at authored `level` (drawn this
    /// frame), marking the loaded ones used.
    fn want_textures(&mut self, model: usize, level: usize) {
        let g = &self.models[model];
        for k in (0..g.levels.len()).filter(|&k| g.levels[k].map(usize::from) == Some(level)) {
            for i in g.images_of(k) {
                let Some(bytes) = g.data.encoded.get(i).cloned().flatten() else { continue };
                if self.textures.want((model, i)) {
                    let (srgb, mips, target) = (g.srgb[i], g.mips[i], crate::ktx2::target());
                    self.decoder.push(tex_cache::Job { key: (model, i), bytes, srgb, mips, target });
                }
            }
        }
    }

    /// Uploads the pictures decoded since the last frame and drops the unused ones.
    fn pump_textures(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        self.textures.tick();
        for done in self.decoder.poll() {
            let (model, image) = done.key;
            match done.pixels {
                Ok(pixels) if model < self.models.len() => {
                    let (width, height) = pixels.size();
                    let texture = (!matches!(pixels, tex_cache::Pixels::Blocks(_))).then(|| {
                        model_texture(device, "model texture (streamed)", width, height, done.srgb, done.mips)
                    });
                    self.uploads.push_back(Upload { key: done.key, pixels, texture, srgb: done.srgb, next: 0 });
                }
                Ok(_) => {}
                Err(e) => {
                    log::warn!("textures: model {model} image {image}: {e}");
                    self.textures.failed(done.key);
                }
            }
        }
        // A few MB a frame: a 4 k picture with its mips in one go stalled a frame by 190 ms.
        let mut budget = tex_cache::UPLOAD_PER_FRAME;
        while budget > 0 {
            let Some(up) = self.uploads.front_mut() else { break };
            let srgb = up.srgb;
            if !up.write(device, queue, &mut budget, srgb) {
                continue;
            }
            let Some(up) = self.uploads.pop_front() else { break };
            let (model, image) = up.key;
            let (width, height) = up.pixels.size();
            let Some(texture) = up.texture else { continue };
            let bytes = match &up.pixels {
                tex_cache::Pixels::Blocks(blocks) => blocks.bytes(),
                _ => tex_cache::texture_bytes(width, height, texture.mip_level_count() > 1),
            };
            if model >= self.models.len() || !self.textures.loaded(up.key, bytes) {
                continue;
            }
            if !matches!(up.pixels, tex_cache::Pixels::Blocks(_)) {
                self.mipgen.generate(device, queue, &texture);
            }
            self.models[model].views[image] = Some(texture.create_view(&Default::default()));
            self.refresh_groups(device, model, image);
            log::info!("textures: model {model} image {image} in, {:.1} MB", bytes as f64 / 1e6);
        }
        for (model, image) in self.textures.evict() {
            if model < self.models.len() {
                self.models[model].views[image] = None;
                self.refresh_groups(device, model, image);
                log::info!("textures: model {model} image {image} dropped");
            }
        }
    }

    /// Rebuilds the groups of the materials that sample `image`.
    fn refresh_groups(&mut self, device: &wgpu::Device, model: usize, image: usize) {
        for k in 0..self.models[model].data.materials.len() {
            if self.models[model].images_of(k).any(|i| i == image) {
                let group = self.material_group(device, model, k);
                self.models[model].materials[k] = group;
            }
        }
    }

    /// GPU memory of the model textures and the streaming counts.
    pub fn texture_stats(&self) -> TextureStats {
        TextureStats {
            loaded_bytes: self.eager_bytes,
            streamed_bytes: self.textures.resident_bytes(),
            pending: self.textures.pending(),
            loads: self.textures.loads,
            evictions: self.textures.evictions,
        }
    }

    pub fn model(&self, index: usize) -> Option<&ModelData> {
        self.models.get(index).map(|m| &m.data)
    }

    /// Palettes for the next [`ModelRenderer::prepare`]: instance index → palette (the
    /// model's slots in order, as [`ModelData::pose`] gives them), drawn instead of the
    /// instance's clip. Used once.
    pub fn set_custom_poses(&mut self, poses: HashMap<usize, crate::model::Palette>) {
        self.custom = poses;
    }

    /// The custom palette instance `index` was drawn with by the last `prepare`, if any.
    pub fn shown_pose(&self, index: usize) -> Option<&crate::model::Palette> {
        self.shown_custom.get(&index)
    }

    /// Hit damage (#304) for the next [`ModelRenderer::prepare`]: instance index → its
    /// building's scars. Used once.
    pub fn set_scars(&mut self, scars: HashMap<usize, Scars>) {
        self.scars = scars;
    }

    /// Samples the poses, uploads palettes and instances for `instances` seen through
    /// `view`; `dissolve` lets decaying layers dissolve (the gore setting).
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &View3d,
        instances: &[ModelInstance],
        dissolve: bool,
    ) {
        self.pump_textures(device, queue);
        let mut clock = lap_start();
        let mut order: Vec<usize> = (0..instances.len()).filter(|&i| instances[i].model < self.models.len()).collect();
        // Each soldier's look (#283) and the baked variant nearest to it.
        let chosen: Vec<Option<(looks::Look, Option<usize>)>> = crate::par::map(instances, |inst| {
            let m = self.models.get(inst.model)?;
            let l = m.data.looks.as_ref()?;
            let look = looks::choose(inst.look?, l.style);
            let variant = (!m.variants.is_empty()).then(|| looks::nearest_variant(&l.variants, &look.morphs));
            Some((look, variant))
        });
        let variant_of = |i: usize| chosen.get(i).copied().flatten().and_then(|c| c.1);
        // By model, the decaying ones last (they also draw the inner layers), by look variant.
        order.sort_by_key(|&i| (instances[i].model, instances[i].decay > 0.0, variant_of(i)));
        let mut texels: Vec<[f32; 4]> = Vec::new();
        let mut poses: HashMap<(usize, Option<usize>, u32), u32> = HashMap::new();
        let mut entries: Vec<Entry> = Vec::with_capacity(order.len());
        let mut data: Vec<[f32; 4]> = Vec::new();
        let [w, h] = view.size;
        let cap_model = self.cap;
        let insert_models = self.inserts;
        let models = &self.models;
        let custom = std::mem::take(&mut self.custom);
        let scars = std::mem::take(&mut self.scars);
        let shadows_on = self.shadows;
        let mut casters: Vec<([f32; 3], f32)> = Vec::new();
        if self.debug_file {
            self.debug = debug_flags();
        }
        let dbg = self.debug;
        // Each instance on its own (in parallel on native, `par.rs`): culling, the look, its
        // instance texels and entry; the palette's place (`extra.x`), the data's (`extra.y`)
        // and the caps, which need the palette, follow in order in `finish`.
        let stage = |i: usize, inst: &ModelInstance| -> Option<Staged> {
            let tweaked;
            let inst = if dbg == 0 {
                inst
            } else {
                let dead =
                    inst.decay > 0.0 || inst.wounds.count > 0 || inst.wounds.blood > 0.0 || custom.contains_key(&i);
                let vol = inst.damage.volumes_len > 0;
                if (dbg & debug::LIVING != 0 && !dead && i != usize::MAX)
                    || (dbg & debug::DEAD != 0 && dead)
                    || (dbg & debug::DEAD_VOLUMES != 0 && dead && vol)
                    || (dbg & debug::DEAD_PLAIN != 0 && dead && !vol)
                    || (dbg & debug::CAPS != 0 && i == usize::MAX)
                {
                    return None;
                }
                let mut t = *inst;
                if dbg & debug::GORE != 0 {
                    t.wounds = Wounds::default();
                    t.sprayed = Sprayed::default();
                }
                if dbg & debug::HOLES != 0 {
                    t.damage.volumes_len = 0;
                }
                if dbg & debug::LOOKS != 0 {
                    t.look = None;
                }
                tweaked = t;
                &tweaked
            };
            let gpu_model = &models[inst.model];
            let model = &gpu_model.data;
            let look = chosen.get(i).copied().flatten().filter(|_| dbg & debug::LOOKS == 0);
            let variant = look.and_then(|c| c.1);
            // Height: a uniform scale about the ground point.
            let inst = match look {
                Some((l, _)) => {
                    let k = l.height_m / looks::BUILT_HEIGHT_M;
                    let scale = math::from_trs([0.0; 3], [0.0, 0.0, 0.0, 1.0], [k; 3]);
                    ModelInstance { transform: math::mul(&inst.transform, &scale), ..*inst }
                }
                None => *inst,
            };
            // Culling by the rest pose's radius (lying corpses reach about as far).
            let origin = [inst.transform[3][0], inst.transform[3][1], inst.transform[3][2]];
            let stretch = math::dot(
                [inst.transform[0][0], inst.transform[0][1], inst.transform[0][2]],
                [inst.transform[0][0], inst.transform[0][1], inst.transform[0][2]],
            )
            .sqrt()
                / crate::view3d::PIXELS_PER_METRE;
            // With shadows, models just off the screen still cast into it.
            let reach = if shadows_on { 2.5 } else { 1.5 };
            let r = (model.radius * stretch * reach + 8.0) * view.camera.scale;
            let p = view.project(origin);
            if p[0] < -r || p[0] > w + r || p[1] < -r || p[1] > h + r || !(0.0..=1.0).contains(&p[2]) {
                return None;
            }
            // Drawn in the 3D pass only when on the screen; the rest only cast.
            let rs = (model.radius * stretch * 1.5 + 8.0) * view.camera.scale;
            let on_screen = p[0] >= -rs && p[0] <= w + rs && p[1] >= -rs && p[1] <= h + rs;
            // The inner layers (bones, an animal's meat) of a body (a rig) only once the skin
            // opens, or where a wound may show them; before, they lie wholly inside the outer
            // layers. Gibs and other pieces without a rig show theirs (a bone chunk's bone).
            let intact = inst.damage.is_none();
            let dissolving = |at: f32| dissolve && at <= 1.0 && inst.decay > at;
            let enclosed = model.rig.is_some() && dbg & debug::OLD_INNER == 0;
            let decaying =
                inst.decay > 0.0 && (!enclosed || dissolving(gpu_model.opens_at) || !intact) && dbg & debug::INNER == 0;
            let clean = intact
                && !inst.transparent
                && inst.rise.is_none()
                && !dissolving(gpu_model.dissolves_at)
                && dbg & debug::NO_CLEAN == 0;
            let mut look_data =
                look_texels(look.as_ref().map(|c| &c.0), gpu_model.eye, inst.look.unwrap_or(0), gpu_model.face_tiles);
            // A crew's uniform rides on the look texels the crew does not use otherwise.
            if let (None, Some(u), Some(crew)) = (look, inst.uniform, model.crew.as_ref()) {
                // The colour and the base's luminance (`model.wgsl`: the cloth keeps its shading).
                for k in 0..2 {
                    let b = crew.base[k];
                    look_data[k] = [u[k][0], u[k][1], u[k][2], 0.2126 * b[0] + 0.7152 * b[1] + 0.0722 * b[2]];
                }
            }
            let mut data = Vec::new();
            instance_texels(&inst, model.rig.as_ref(), &look_data, scars.get(&i), &mut data);
            Some(Staged {
                i,
                data,
                entry: Entry {
                    gpu: GpuInstance {
                        rows: math::affine_rows(&inst.transform),
                        params: [
                            inst.decay,
                            inst.morphs[0],
                            inst.morphs[if model.emaciated { 2 } else { 1 }],
                            if inst.transparent { 1.0 } else { 0.0 },
                        ],
                        // The palette and the data (set in `finish`), the player, the hair
                        // and beard variants shown (none without a look).
                        extra: [0, 0, u32::from(inst.player), look.map_or(0, |c| c.0.node_mask())],
                    },
                    model: inst.model,
                    decaying,
                    clean,
                    depth: p[2],
                    variant,
                    on_screen,
                    split: inst.damage.splits_parts(),
                    origin,
                    radius: model.radius * stretch,
                },
                inst,
            })
        };
        let staged: Vec<Option<Staged>> = crate::par::map(&order, |&i| stage(i, &instances[i]));
        // The shared poses in the order they are first wanted (the palettes' layout below),
        // sampled in parallel.
        let mut wanted: Vec<(usize, Option<usize>, u32)> = Vec::new();
        for s in staged.iter().flatten().filter(|s| !custom.contains_key(&s.i)) {
            let key = (s.inst.model, s.inst.clip, s.inst.time.to_bits());
            if poses.insert(key, 0).is_none() {
                wanted.push(key);
            }
        }
        poses.clear();
        let sampled = crate::par::map(&wanted, |&(m, clip, time)| models[m].data.pose(clip, f32::from_bits(time)));
        let sampled_at: HashMap<(usize, Option<usize>, u32), usize> =
            wanted.iter().enumerate().map(|(k, &key)| (key, k)).collect();
        // Caps at the cuts, carried by the bone at the cut in this pose, and the inside of the
        // wounds: cavity walls, ribs and bone ends (#280); per instance in parallel.
        let caps_of = |s: &Staged| -> Vec<ModelInstance> {
            let inst = &s.inst;
            let model = &models[inst.model].data;
            let Some(rig) = model.rig.as_ref().filter(|_| !inst.damage.is_none()) else { return Vec::new() };
            let key = (inst.model, inst.clip, inst.time.to_bits());
            let Some(palette) = custom.get(&s.i).or_else(|| sampled_at.get(&key).map(|&k| &sampled[k])) else {
                return Vec::new();
            };
            let bone = |slot: u32| -> Option<Mat4> {
                let rows = palette.get(slot as usize)?;
                Some(std::array::from_fn(|c| [rows[0][c], rows[1][c], rows[2][c], if c == 3 { 1.0 } else { 0.0 }]))
            };
            let piece = |model: usize, bone: Mat4, rest: &Mat4| ModelInstance {
                model,
                clip: None,
                time: 0.0,
                transform: math::mul(&inst.transform, &math::mul(&bone, rest)),
                morphs: [0.0; 3],
                wounds: Default::default(),
                sprayed: Default::default(),
                damage: Damage::default(),
                look: None,
                ..*inst
            };
            let mut out = Vec::new();
            for cap in damage::caps(&inst.damage, rig) {
                if let Some(b) = bone(cap.slot) {
                    out.push(piece(cap_model, b, &cap.rest_matrix()));
                }
            }
            for ins in damage::inserts(&inst.damage, rig) {
                if let Some(b) = bone(ins.slot) {
                    out.push(piece(insert_models[ins.kind as usize], b, &ins.rest));
                }
            }
            out
        };
        let caps: Vec<ModelInstance> =
            crate::par::map(&staged, |s| s.as_ref().map_or_else(Vec::new, caps_of)).into_iter().flatten().collect();
        let staged_caps = crate::par::map(&caps, |inst| stage(usize::MAX, inst));
        self.prepare_ns[0] = lap(&mut clock);
        // The palettes in order, each shared pose once (sampled above in this order; the caps'
        // poses, after them, here), then the instance data.
        let mut next = 0;
        data.reserve(staged.iter().flatten().map(|s| s.data.len()).sum());
        for s in staged.into_iter().chain(staged_caps).flatten() {
            let (i, inst) = (s.i, &s.inst);
            casters.push((s.entry.origin, s.entry.radius));
            let key = (inst.model, inst.clip, inst.time.to_bits());
            let base = if let Some(palette) = custom.get(&i) {
                let base = texels.len() as u32;
                for rows in palette {
                    texels.extend_from_slice(rows);
                }
                base
            } else {
                *poses.entry(key).or_insert_with(|| {
                    let base = texels.len() as u32;
                    match sampled.get(next) {
                        Some(palette) => {
                            next += 1;
                            for rows in palette {
                                texels.extend_from_slice(rows);
                            }
                        }
                        None => {
                            for rows in models[inst.model].data.pose(inst.clip, inst.time) {
                                texels.extend_from_slice(&rows);
                            }
                        }
                    }
                    base
                })
            };
            let mut entry = s.entry;
            entry.gpu.extra[0] = base;
            entry.gpu.extra[1] = data.len() as u32;
            data.extend_from_slice(&s.data);
            entries.push(entry);
        }
        // The instance data follows the palettes; `extra.y` becomes absolute.
        let data_base = texels.len() as u32;
        texels.extend_from_slice(&data);
        entries.iter_mut().for_each(|e| e.gpu.extra[1] += data_base);
        self.pose_count = poses.len();
        // The sun's view of the models drawn, around the window centre.
        let focus = view.target();
        let reach = 1.1 * (w * w + h * h).sqrt() / view.camera.scale;
        let fit = if shadows_on { shadow::fit(casters.iter().copied(), focus, reach) } else { None };
        self.prepare_ns[1] = lap(&mut clock);
        let gpu = self.group_draws(&entries, view.camera.scale, fit.as_ref().map(|f| (f.texel, focus, reach)));
        self.prepare_ns[2] = lap(&mut clock);

        let rows = (texels.len() as u32).div_ceil(BONE_WIDTH).max(1);
        if rows > self.bones.height() {
            let rows = rows.next_power_of_two().min(device.limits().max_texture_dimension_2d);
            self.bones = bone_texture(device, rows);
            self.globals_group = Self::globals_group(device, &self.globals_layout, &self.globals, &self.bones);
            self.shadow_globals_group =
                Self::globals_group(device, &self.globals_layout, &self.shadow_globals, &self.bones);
        }
        let rows = rows.min(self.bones.height());
        texels.resize((rows * BONE_WIDTH) as usize, [0.0; 4]);
        queue.write_texture(
            self.bones.as_image_copy(),
            bytemuck::cast_slice(&texels),
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(BONE_WIDTH * 16), rows_per_image: None },
            wgpu::Extent3d { width: BONE_WIDTH, height: rows, depth_or_array_layers: 1 },
        );
        let needed = (gpu.len() * size_of::<GpuInstance>()) as u64;
        if needed > self.instances.size() {
            self.instances = instance_buffer(device, (gpu.len() as u64).next_power_of_two());
        }
        if !gpu.is_empty() {
            queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(&gpu));
        }
        self.prepare_ns[3] = lap(&mut clock);
        let len = math::dot(LIGHT, LIGHT).sqrt();
        let forward = view.basis().forward;
        let globals = Globals {
            view_proj: view.world_to_clip(),
            light: [LIGHT[0] / len, LIGHT[1] / len, LIGHT[2] / len, AMBIENT],
            // w: world px per screen px (thin parts' least width; 0 in the shadow map).
            forward: [forward[0], forward[1], forward[2], 1.0 / view.camera.scale.max(1e-3)],
            params: [
                if self.encode_srgb { 1.0 } else { 0.0 },
                if dissolve { 1.0 } else { 0.0 },
                BONE_WIDTH as f32,
                if self.material_detail { 1.0 } else { 0.0 },
            ],
            wind: self.wind,
            weather: self.weather[0],
            fog: self.weather[1],
        };
        queue.write_buffer(&self.globals, 0, bytemuck::bytes_of(&globals));
        self.shadow.upload(queue, fit.as_ref(), focus, reach);
        if let Some(fit) = &fit {
            let s = shadow::sun();
            let light = Globals { view_proj: fit.matrix, forward: [-s[0], -s[1], -s[2], 0.0], ..globals };
            queue.write_buffer(&self.shadow_globals, 0, bytemuck::bytes_of(&light));
        }
        self.shown_custom = custom;
    }

    /// The shadow map and the bind group receivers sample it through (terrain).
    pub fn shadow(&self) -> &ShadowMap {
        &self.shadow
    }

    /// Draws the prepared instances into the shadow map (before the 3D pass); nothing when
    /// shadows are off or nothing casts.
    pub fn render_shadows(&self, encoder: &mut wgpu::CommandEncoder) {
        if !self.shadow.active || self.shadow_draws.is_empty() {
            return;
        }
        let mut pass = self.shadow.begin(encoder);
        pass.set_bind_group(0, &self.shadow_globals_group, &[]);
        for (pipeline, clean) in [(&self.shadow_clean_pipeline, true), (&self.shadow_pipeline, false)] {
            if self.shadow_draws.iter().any(|d| self.has_parts(d, clean)) {
                pass.set_pipeline(pipeline);
                self.draw_instances(&mut pass, &self.shadow_draws, clean);
            }
        }
    }

    /// Uploads the blood pools of the next `render` (after [`Self::prepare`], which sets the
    /// view they are drawn through).
    pub fn prepare_pools(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, pools: &[BloodPool]) {
        let gpu: Vec<GpuPool> = pools.iter().filter(|p| p.fade > 0.0 && p.radius > 0.0).map(gpu_pool).collect();
        self.pool_count = gpu.len() as u32;
        if gpu.is_empty() {
            return;
        }
        let needed = (gpu.len() * size_of::<GpuPool>()) as u64;
        if needed > self.pools.size() {
            self.pools = pool_buffer(device, (gpu.len() as u64).next_power_of_two());
        }
        queue.write_buffer(&self.pools, 0, bytemuck::cast_slice(&gpu));
    }

    /// Draws the blood pools, then the prepared instances, into a pass opened by
    /// [`DepthTarget::begin`].
    pub fn render(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.pool_count > 0 {
            pass.set_pipeline(&self.pool_pipeline);
            pass.set_bind_group(0, &self.globals_group, &[]);
            pass.set_vertex_buffer(0, self.pools.slice(..));
            pass.draw(0..4, 0..self.pool_count);
        }
        if self.draws.is_empty() {
            return;
        }
        pass.set_bind_group(0, &self.globals_group, &[]);
        pass.set_bind_group(2, &self.shadow.group, &[]);
        pass.set_stencil_reference(1);
        // The draws are sorted by level: the near ones first, then the far ones' pipeline.
        // First every part that can drop nothing, without `discard` (the tiler's early
        // depth test stays on), then the rest.
        let split = self.draws.partition_point(|d| d.lod < lod::FAR_FROM);
        let (near, far) = self.draws.split_at(split);
        for (pipeline, draws, clean) in [
            (&self.clean_pipeline, near, true),
            (&self.far_clean_pipeline, far, true),
            (&self.pipeline, near, false),
            (&self.far_pipeline, far, false),
        ] {
            if draws.iter().any(|d| self.has_parts(d, clean)) {
                pass.set_pipeline(pipeline);
                self.draw_instances(pass, draws, clean);
            }
        }
    }

    /// Chooses the levels of detail and groups the prepared instances into the draws of the
    /// 3D pass (those on the screen) and of the shadow map (`shadow`: its texel size, focus
    /// and reach; the casters within the reach, coarser, the tiny ones skipped). Returns the
    /// instance buffer: the 3D pass's instances, then the casters'.
    fn group_draws(&mut self, entries: &[Entry], scale: f32, shadow: Option<(f32, [f32; 3], f32)>) -> Vec<GpuInstance> {
        let n = self.models.len();
        self.lod_last.resize(n, None);
        let mut size = vec![0.0f32; n];
        for e in entries {
            size[e.model] = size[e.model].max(2.0 * e.radius * scale);
        }
        // Caps and wound inserts sit on a body at its own level: always whole (they are small).
        let lods = self.lod_override.unwrap_or(self.lods);
        let whole = |m: usize| !lods || m == self.cap || self.inserts.contains(&m) || self.small.contains(&m);
        let level: Vec<usize> = (0..n)
            .map(|m| match self.lod_force {
                _ if whole(m) || size[m] == 0.0 => 0,
                Some(l) => l,
                None if self.models[m].authored => lod::choose_in(&lod::AUTHORED_SIZES, size[m], self.lod_last[m]),
                None => lod::choose(size[m], self.lod_last[m]),
            })
            .collect();
        for m in (0..n).filter(|&m| size[m] > 0.0) {
            if self.lod_last[m] != Some(level[m]) {
                log::debug!("lod: model {m} at {:.0} px: level {}", size[m], level[m]);
            }
            self.lod_last[m] = Some(level[m]);
        }
        // An authored level draws only once its own pictures are in; until then the next
        // coarser level that has them (its own mesh, UVs and pictures) stands in.
        let ready: Vec<[bool; lod::LEVELS + 1]> = (0..n)
            .map(|m| {
                let g = &self.models[m];
                std::array::from_fn(|l| !g.authored || size[m] == 0.0 || g.level_ready(l))
            })
            .collect();
        let shown = |m: usize, wanted: usize, last: usize| shown_level(wanted, last, |l| ready[m][l]);
        // (model, level) of the streamed pictures to ask for: the wanted level and every
        // coarser one down to the level shown in its place.
        let mut wants: Vec<(usize, usize)> = Vec::new();
        let mut gpu: Vec<GpuInstance> = Vec::with_capacity(entries.len() * 2);
        // (level, look variant, instance) → draws, appending the instances.
        let mut group = |mut list: Vec<(usize, Option<usize>, &Entry)>| -> Vec<Draw> {
            // The far levels last: they take the cheaper pipeline (`render`).
            // Within a draw front to back (`depth` in 0..1; the tiler's early depth test).
            list.sort_by_key(|&(l, v, e)| {
                (
                    l >= lod::FAR_FROM,
                    e.model,
                    e.decaying,
                    e.clean,
                    v,
                    l,
                    (e.depth.clamp(0.0, 1.0) * 16_777_215.0) as u32,
                )
            });
            let mut draws: Vec<Draw> = Vec::new();
            for (l, v, e) in list {
                let index = gpu.len() as u32;
                gpu.push(e.gpu);
                match draws.last_mut() {
                    Some(d)
                        if d.model == e.model
                            && d.decaying == e.decaying
                            && d.clean == e.clean
                            && d.variant == v
                            && d.lod == l =>
                    {
                        d.range.end = index + 1
                    }
                    _ => draws.push(Draw {
                        model: e.model,
                        range: index..index + 1,
                        decaying: e.decaying,
                        clean: e.clean,
                        variant: v,
                        lod: l,
                        touched: None,
                    }),
                }
            }
            split_variants(draws)
        };
        // Far off, the face and body variants are not worth a draw call each.
        let main: Vec<_> = entries
            .iter()
            .filter(|e| e.on_screen)
            .map(|e| {
                let wanted = if e.split { level[e.model].min(lod::DAMAGED_UP_TO) } else { level[e.model] };
                let at = shown(e.model, wanted, lod::LEVELS - 1);
                if self.models[e.model].authored {
                    wants.extend((wanted..=at).map(|l| (e.model, l)));
                }
                // A split body stays off the welded lists (white until its level is in).
                let l = if e.split { at.min(lod::DAMAGED_UP_TO) } else { at };
                (l, e.variant.filter(|_| l <= lod::LOOKS_UP_TO), e)
            })
            .collect();
        self.drawn = main.len();
        let draws = group(main);
        let mut casters = Vec::new();
        if let Some((texel, focus, reach)) = shadow {
            casters = entries
                .iter()
                .filter(|e| {
                    let d = math::sub(e.origin, focus);
                    // Outside the map, or under two texels across: nothing to see.
                    (d[0] * d[0] + d[1] * d[1]).sqrt() - e.radius <= reach && e.radius >= texel
                })
                // Coarser than seen; the shadow needs no face variant.
                .map(|e| {
                    let l = match level[e.model] + lod::SHADOW_STEP {
                        _ if whole(e.model) => 0,
                        // As drawn: only a body split into parts stays off the welded lists.
                        l if e.split => {
                            shown(e.model, l.min(lod::DAMAGED_UP_TO), lod::LEVELS - 1).min(lod::DAMAGED_UP_TO)
                        }
                        l if l >= lod::LEVELS - 1 => lod::SHADOW,
                        l => shown(e.model, l, lod::SHADOW),
                    };
                    (l, None, e)
                })
                .collect();
        }
        self.shadow_draws = group(casters);
        self.draws = draws;
        // The streamed pictures of the authored levels wanted and drawn (`tex_cache`).
        wants.sort_unstable();
        wants.dedup();
        for (model, level) in wants {
            self.want_textures(model, level);
        }
        self.triangles = [self.count_triangles(&self.draws), self.count_triangles(&self.shadow_draws)];
        let calls = |d: &[Draw]| -> usize { d.iter().map(|d| self.models[d.model].draw_parts(d).count()).sum() };
        self.calls = [calls(&self.draws), calls(&self.shadow_draws)];
        gpu
    }

    fn count_triangles(&self, draws: &[Draw]) -> u64 {
        draws
            .iter()
            .map(|d| {
                let per: u32 = self.models[d.model].draw_parts(d).map(|(r, ..)| r.len() as u32 / 3).sum();
                u64::from(per) * u64::from(d.range.end - d.range.start)
            })
            .sum()
    }

    /// A draw's part in `material` goes through the clean pipelines ([`Entry::clean`]).
    fn clean_part(&self, d: &Draw, material: usize) -> bool {
        d.clean && !self.models[d.model].discards[material]
    }

    fn has_parts(&self, d: &Draw, clean: bool) -> bool {
        self.models[d.model].draw_parts(d).any(|(_, material, _)| self.clean_part(d, material) == clean)
    }

    /// The prepared draws' parts that are `clean` or not ([`Self::clean_part`]), with the
    /// pipeline and group 0 bound.
    fn draw_instances(&self, pass: &mut wgpu::RenderPass<'_>, draws: &[Draw], clean: bool) {
        let stride = size_of::<GpuInstance>() as u64;
        for d in draws {
            if !self.has_parts(d, clean) {
                continue;
            }
            let range = &d.range;
            let m = &self.models[d.model];
            let look = d.variant.and_then(|v| m.variants.get(v));
            // The instances are bound from their first one: no base instance on WebGL2.
            pass.set_vertex_buffer(
                1,
                self.instances.slice(u64::from(range.start) * stride..u64::from(range.end) * stride),
            );
            pass.set_index_buffer(m.indices.slice(..), wgpu::IndexFormat::Uint32);
            let mut bound = None;
            for (indices, material, touched) in m.draw_parts(d) {
                if self.clean_part(d, material) != clean {
                    continue;
                }
                let from_look = touched && look.is_some();
                if bound != Some(from_look) {
                    pass.set_vertex_buffer(0, look.filter(|_| from_look).unwrap_or(&m.vertices).slice(..));
                    bound = Some(from_look);
                }
                pass.set_bind_group(1, m.group(material), &[]);
                pass.draw_indexed(indices, 0, 0..range.end - range.start);
            }
        }
    }
}

/// The depth and stencil buffer of the 3D pass and the pipeline that resets the ground.
pub struct DepthTarget {
    size: (u32, u32),
    view: Option<wgpu::TextureView>,
    /// The depth aspect alone, for passes that read the depth (the free camera's fog).
    depth_only: Option<wgpu::TextureView>,
    reset: wgpu::RenderPipeline,
}

impl DepthTarget {
    pub fn new(device: &wgpu::Device, target: wgpu::TextureFormat) -> Self {
        let shader = model_shader(device);
        let keep_if_zero = wgpu::StencilFaceState {
            compare: wgpu::CompareFunction::Equal,
            fail_op: wgpu::StencilOperation::Keep,
            depth_fail_op: wgpu::StencilOperation::Keep,
            pass_op: wgpu::StencilOperation::Keep,
        };
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("depth reset"),
            bind_group_layouts: &[],
            immediate_size: 0,
        });
        let reset = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("depth reset"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_far"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: wgpu::StencilState { front: keep_if_zero, back: keep_if_zero, read_mask: 0xff, write_mask: 0 },
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_none"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target,
                    blend: None,
                    write_mask: wgpu::ColorWrites::empty(),
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        Self { size: (0, 0), view: None, depth_only: None, reset }
    }

    /// Opens the 3D pass over `color` (kept) with a cleared depth and stencil buffer of the
    /// output size. `resume` instead keeps the depth and stencil of the previous pass, which
    /// then must have been opened with `keep` (the GPU timer splits the pass so).
    #[allow(clippy::too_many_arguments)]
    pub fn begin<'e>(
        &mut self,
        device: &wgpu::Device,
        encoder: &'e mut wgpu::CommandEncoder,
        color: &wgpu::TextureView,
        size: [f32; 2],
        keep: bool,
        resume: bool,
    ) -> wgpu::RenderPass<'e> {
        let size = ((size[0] as u32).max(1), (size[1] as u32).max(1));
        if self.size != size || self.view.is_none() {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("depth"),
                size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            self.view = Some(texture.create_view(&Default::default()));
            self.depth_only = Some(texture.create_view(&wgpu::TextureViewDescriptor {
                aspect: wgpu::TextureAspect::DepthOnly,
                ..Default::default()
            }));
            self.size = size;
        }
        let depth = self.view.as_ref().expect("created above");
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("3d"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: Some(wgpu::Operations {
                    load: if resume { wgpu::LoadOp::Load } else { wgpu::LoadOp::Clear(1.0) },
                    store: if keep { wgpu::StoreOp::Store } else { wgpu::StoreOp::Discard },
                }),
                stencil_ops: Some(wgpu::Operations {
                    load: if resume { wgpu::LoadOp::Load } else { wgpu::LoadOp::Clear(0) },
                    store: if keep { wgpu::StoreOp::Store } else { wgpu::StoreOp::Discard },
                }),
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        })
    }

    /// The depth of the last pass (opened with `keep`), once [`DepthTarget::begin`] made one.
    pub fn depth_view(&self) -> Option<&wgpu::TextureView> {
        self.depth_only.as_ref()
    }

    /// Moves every pixel no model drew to the far plane: sprites then draw over the terrain
    /// (as in the original) but behind models nearer than them.
    pub fn reset_ground(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(&self.reset);
        pass.set_stencil_reference(0);
        pass.draw(0..3, 0..1);
    }
}

/// The depth state of pipelines drawn in the 3D pass after the models: test, no writes.
pub fn depth_test_only() -> wgpu::DepthStencilState {
    wgpu::DepthStencilState {
        format: DEPTH_FORMAT,
        depth_write_enabled: Some(false),
        depth_compare: Some(wgpu::CompareFunction::LessEqual),
        stencil: Default::default(),
        bias: Default::default(),
    }
}

/// A clock for [`ModelRenderer::prepare_ns`]: none in the browser (`std::time::Instant`
/// panics there).
fn lap_start() -> Option<std::time::Instant> {
    #[cfg(not(target_arch = "wasm32"))]
    return Some(std::time::Instant::now());
    #[cfg(target_arch = "wasm32")]
    None
}

/// Nanoseconds since `clock`, which moves on to now.
fn lap(clock: &mut Option<std::time::Instant>) -> u64 {
    let now = lap_start();
    let ns = match (*clock, now) {
        (Some(a), Some(b)) => (b - a).as_nanos() as u64,
        _ => 0,
    };
    *clock = now;
    ns
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Primitive, Vertex};

    fn primitive(material: usize, lod: Option<u8>) -> Primitive {
        let vertices = (0..3)
            .map(|i| Vertex { position: [i as f32, (i * i) as f32, 0.0], ..<Vertex as bytemuck::Zeroable>::zeroed() })
            .collect();
        Primitive { vertices, indices: vec![0, 1, 2], material, morphs: [false; 2], look_morphs: Vec::new(), lod }
    }

    /// No draw samples a texture set made for another level's UVs: whichever streamed pictures
    /// are in, every material drawn at the level shown is its own level's (or shared by all
    /// levels) and has its pictures in. An HD man: master (0), levels 1 to 3 with their own
    /// atlases (materials 0..3: skin, 4..7: coat), a shared material 8 drawn at every level.
    #[test]
    fn draws_never_take_another_levels_textures() {
        let have = [0u8, 1, 2, 3];
        let mut prims: Vec<Primitive> =
            (0..4u8).flat_map(|l| [primitive(l.into(), Some(l)), primitive(4 + usize::from(l), Some(l))]).collect();
        prims.push(primitive(8, None));
        let levels = crate::model::material_levels(&prims, 9);
        assert_eq!(&levels[..4], &[Some(0), Some(1), Some(2), None], "the coarsest is always loaded");
        // Per level: the materials drawn ([`lod::authored`]; a primitive without level at all).
        let drawn: Vec<Vec<usize>> = (0..lod::LEVELS)
            .map(|l| {
                prims
                    .iter()
                    .filter(|p| {
                        p.lod.is_none_or(|own| !lod::authored(&p.vertices, &p.indices, own, &have)[l].is_empty())
                    })
                    .map(|p| p.material)
                    .collect()
            })
            .collect();
        // Every set of loaded streamed levels (bits 0..2), every wanted level.
        for loaded in 0u8..8 {
            let ready_material = |k: usize| levels[k].is_none_or(|l| loaded & (1 << l) != 0);
            let ready = |l: usize| drawn[l].iter().all(|&k| ready_material(k));
            for wanted in 0..lod::LEVELS {
                let at = shown_level(wanted, lod::LEVELS - 1, ready);
                assert!(at >= wanted, "never finer than wanted");
                for &k in &drawn[at] {
                    assert!(ready_material(k), "loaded {loaded:03b}, wanted {wanted}: material {k} at {at} not in");
                    assert!(
                        levels[k].is_none_or(|own| usize::from(own) == at),
                        "material {k} of level {:?} drawn at level {at}",
                        levels[k]
                    );
                }
                if ready(wanted) {
                    assert_eq!(at, wanted, "loaded {loaded:03b}: the wanted level once its pictures are in");
                }
            }
        }
    }
}
