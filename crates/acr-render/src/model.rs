//! 3D models from glTF 2.0 binaries (`.glb`): meshes, materials, skins and animation clips,
//! read into plain data the [`crate::model_renderer::ModelRenderer`] uploads.
//!
//! What is read (see `docs/rendering-3d.md`):
//!
//! - every mesh of the default scene, static (bound to its node) or skinned; both end up
//!   skinned to a **palette** of matrices ([`Slot`]): a static mesh is weighted 1 to its
//!   node's slot, a skinned one to its joints' slots, so one vertex format and one shader
//!   serve all;
//! - `POSITION`, `NORMAL`, `TEXCOORD_0`, `COLOR_0`, `JOINTS_0`/`WEIGHTS_0`, indices
//!   (triangle lists only), the custom `_PART`, `_LIMB` and `_GORE` (x) for cutting
//!   ([`crate::damage`]); meshes named `*.caps` (the old stump caps) and the hair and beard
//!   variants `*.hair_*` / `*.beard_*` (#283, not chosen per soldier yet) are skipped;
//! - the morph targets `deflate` and `lie_flat` (by `extras.targetNames`, else the first
//!   two), positions only; a model without `lie_flat` (skeleton H) carries `emaciated`
//!   (hunger, #207) in the second slot instead ([`ModelData::emaciated`]); the face and body targets `face_*`/`body_*` (#283), kept on the
//!   CPU for [`ModelData::bake_looks`]; `_FACE`, and the hair and beard variant meshes
//!   `*.hair_*`/`*.beard_*` (marked in `body.w`, see `crate::looks`);
//! - materials: base colour factor and texture (PNG), `extras.acr_decay` (the decay layer of
//!   `docs/assets-humans.md`). With `acr_decay`, `COLOR_0` is the decay mask, otherwise the
//!   vertex colour; the normal map and the metallic-roughness factors and texture
//!   ([`Surface`], #234; tangents are derived in the shader, `TANGENT` is not read);
//! - animations: translation, rotation and scale channels (step, linear; cubic splines use
//!   their key values linearly), sampled on the CPU into the palette ([`ModelData::pose`]).
//!
//! Not read: compressed meshes (`EXT_meshopt_compression`, Draco), KTX2/JPEG textures,
//! cameras, lights, morph target normals.

use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};
use gltf::animation::util::ReadOutputs;

use crate::math::{self, Mat4};

/// Frames per second of the clips: clip time = sprite animation frame / `fps`. The assets
/// key their clips on sprite frames at Blender's default 24 fps; a scene root's
/// `extras.acr_fps` overrides it.
pub const CLIP_FPS: f32 = 24.0;

/// Names of the two morph targets the decay drives (`docs/assets-humans.md`).
pub const MORPH_NAMES: [&str; 2] = ["deflate", "lie_flat"];
/// The hunger morph (#183, #207): loaded into the second morph slot of models without
/// `lie_flat` (the vertex slots are full on WebGL2), see [`ModelData::emaciated`].
pub const EMACIATED: &str = "emaciated";

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    /// `COLOR_0` (RGBA, 1 when absent): the decay mask or the vertex colour.
    pub color: [f32; 4],
    /// Palette slots and their weights.
    pub joints: [u32; 4],
    pub weights: [f32; 4],
    /// Position offsets of the morph targets [`MORPH_NAMES`] (the second: [`EMACIATED`] on
    /// models with [`ModelData::emaciated`]).
    pub morph: [[f32; 3]; 2],
    /// Damage coordinates (`docs/damage-model.md`): `_PART` (body part 0..9 of `rig.PARTS`,
    /// −1 none), `_LIMB` (level 0..1 along the limb, −1 trunk or unknown; derived from the
    /// rest pose when the asset lacks it), `_GORE.x` (ragged-edge noise, 0.5 without), and
    /// the hair or beard variant the vertex belongs to (#283: index into
    /// [`crate::looks::VARIANT_NODES`] + 1; 0 always drawn).
    pub body: [f32; 4],
    /// `_FACE` (#283, flesh only): beard, wrinkle, pock zones and the face front, 0 without.
    pub face: [u8; 4],
    /// `_WIND` (trees and foliage, docs/rendering-3d.md "Wind"): trunk bend (0 at the root,
    /// 1 at the crown top), branch weight, phase (0..1, per clump), leaf flutter; 0 without.
    pub wind: [u8; 4],
}

/// One entry of a model's matrix palette.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Slot {
    /// A node's global transform (static meshes bound to the node).
    Node(usize),
    /// A skin joint: the joint node's global transform times its inverse bind matrix.
    Joint { node: usize, inverse_bind: Mat4 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub name: String,
    pub parent: Option<usize>,
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
}

/// The decay layer of a material (`extras.acr_decay`): windows of the decay parameter
/// 0..1 for discolouring and dissolving.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DecayLayer {
    pub discolour: [f32; 2],
    pub tint: [f32; 3],
    pub tint_strength: f32,
    /// `None`: never dissolves (wood, metal).
    pub dissolve: Option<[f32; 2]>,
    pub edge: f32,
    pub edge_rgb: [f32; 3],
    /// The layer lies inside the body until the flesh is gone (`layer: "bone"`, or
    /// `inner: true`: an animal's meat under its fur): it is not drawn for models that do
    /// not decay.
    pub inner: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Material {
    pub name: String,
    /// Linear RGBA.
    pub base_color: [f32; 4],
    /// Index into [`ModelData::images`].
    pub texture: Option<usize>,
    /// The texture's sampler asks for nearest filtering (projected sprite colours).
    pub nearest: bool,
    /// `alphaMode: MASK` cutoff.
    pub alpha_cutoff: Option<f32>,
    pub decay: Option<DecayLayer>,
    /// A stump cap (`docs/damage-model.md`): wet, ragged outline from `uv`.
    pub stump: bool,
    /// Drawn as torn tissue (#280): the inside of a wound, bone, or a cloth shred.
    pub tissue: Tissue,
    /// `extras.acr_tint` (#283): tinted per soldier with his skin tone or hair colour.
    pub tint: Tint,
    /// `extras.acr_face_tiles` (#306): the texture is a grid of this many whole head tiles
    /// (`looks::tile_grid` per side), the UVs on the first; each soldier shows the one
    /// [`crate::looks::face_tile`] picks. 1: a plain texture.
    pub face_tiles: u8,
    /// How the surface takes the light (`extras.acr_sheen`, `acr_gloss`; docs/clothing.md).
    pub finish: Finish,
    /// The glTF normal and metallic-roughness maps and factors (#234).
    pub surface: Surface,
}

/// A material's glTF surface detail (#234, `model.wgsl` `bumped`, `specular`): the
/// tangent-space `normalTexture` and its scale, the metallic and roughness factors and their
/// texture (G roughness, B metal). Drawn with the `material_detail` setting
/// (docs/rendering-3d.md "Material detail").
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Surface {
    /// Index into [`ModelData::images`] (linear, not sRGB).
    pub normal: Option<usize>,
    pub normal_scale: f32,
    /// Index into [`ModelData::images`] (linear, not sRGB).
    pub metal_rough: Option<usize>,
    pub metallic: f32,
    pub roughness: f32,
}

impl Default for Surface {
    /// No maps, a rough dielectric: the faintest highlight.
    fn default() -> Self {
        Self { normal: None, normal_scale: 1.0, metal_rough: None, metallic: 0.0, roughness: 1.0 }
    }
}

/// A material's response beyond Lambert (`model.wgsl`, `cloth`): cloth fibres catch the light
/// at grazing angles (`sheen`: wool's soft pale rim), blacked leather and brass a highlight
/// (`gloss`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Finish {
    pub sheen: f32,
    pub gloss: f32,
    /// Depth bias towards the camera (m) by the clothing layer (`extras.acr_layer`, else the
    /// material key's [`crate::layers::layer_rank`]): a millimetre of an inner layer crossing
    /// the one above it under skinning does not show through (docs/clothing.md, "Layers").
    pub layer: f32,
    /// The least width (screen px) the material's thin parts keep at any zoom (0: none;
    /// `model.wgsl` `vs_main`): vertices nearer the model's z axis than half of it are pushed
    /// out to it, so a thin shaft never falls below a pixel and flickers away when zoomed out.
    pub thin: f32,
}

/// Which per-soldier colour a material takes (`extras.acr_tint`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Tint {
    #[default]
    None = 0,
    Skin = 1,
    Hair = 2,
    /// A gun crew's coat (#287): the instance's uniform colour ([`ModelData::crew`]).
    Coat = 3,
    /// A gun crew's waistcoat and facings.
    Facings = 4,
}

/// How a material of the gore inserts and gibs is shaded (`model.wgsl`, `look.y`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Tissue {
    #[default]
    None = 0,
    /// The inner wall of a wound cavity: only its far side drawn, wet meat darkening
    /// towards the deep centre.
    Cavity = 1,
    /// Bone in a wound: wet, glossy.
    Bone = 2,
    /// A shred of uniform: tinted with the uniform colour, crumpled.
    Cloth = 3,
}

/// An RGBA8 picture (sRGB encoded, as stored).
#[derive(Debug, Clone, PartialEq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Primitive {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub material: usize,
    /// Which of the morph targets [`MORPH_NAMES`] the primitive has (the second: [`EMACIATED`]
    /// on models with [`ModelData::emaciated`]).
    pub morphs: [bool; 2],
    /// The face and body targets (#283) it has: index into [`crate::looks::LOOK_MORPHS`]
    /// and the position offsets, baked into variants on the CPU.
    pub look_morphs: Vec<(usize, Vec<[f32; 3]>)>,
    /// Its authored level of detail (docs/rendering-3d.md, "Authored levels"): `Some(k ≥ 1)`
    /// a level the asset carries (node `extras.acr_lod` = k, or a node name ending `.lod<k>`),
    /// `Some(0)` the master those levels replace (named by their `extras.acr_lod_of`, or the
    /// node `<name>` of a `<name>.lod<k>`), `None` drawn at every level (the renderer's own
    /// index levels, [`crate::lod::levels`]).
    pub lod: Option<u8>,
}

/// Individual looks of a model with the face and body extras (#283, `crate::looks`).
#[derive(Debug, Clone, PartialEq)]
pub struct Looks {
    pub style: crate::looks::Style,
    /// Morph weights of the baked variants ([`crate::looks::variant_weights`]).
    pub variants: Vec<[f32; 12]>,
    /// Eye height in the rest pose (m): `extras.acr_face_frame[2]`.
    pub eye: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Property {
    Translation,
    Rotation,
    Scale,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Channel {
    pub node: usize,
    pub property: Property,
    pub step: bool,
    pub times: Vec<f32>,
    /// xyz (translation, scale) or xyzw (rotation).
    pub values: Vec<[f32; 4]>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Clip {
    pub name: String,
    pub duration: f32,
    pub channels: Vec<Channel>,
}

/// A loaded model.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelData {
    pub nodes: Vec<Node>,
    /// Nodes with parents before children.
    order: Vec<usize>,
    pub slots: Vec<Slot>,
    pub primitives: Vec<Primitive>,
    pub materials: Vec<Material>,
    /// Decoded pictures; a deferred one ([`Self::encoded`]) has its size but no pixels.
    pub images: Vec<Image>,
    /// Per image: the encoded file of a picture left undecoded at load because only finer
    /// authored levels than the coarsest use it ([`Self::material_levels`]); the renderer
    /// decodes it once a model first draws at such a level (`tex_cache.rs`).
    pub encoded: Vec<Option<std::sync::Arc<[u8]>>>,
    pub clips: Vec<Clip>,
    /// Clip time per sprite frame: [`CLIP_FPS`] or the root's `extras.acr_fps`.
    pub fps: f32,
    /// `extras.acr_type` of the scene root: the `.MD` `NAME` the model stands for.
    pub type_name: Option<String>,
    /// `extras.acr_clock = "world"` of the scene root: clips run on the game clock, not on the
    /// sprite frame (ships: a sprite of one frame per direction still rocks; looping while
    /// alive, from the death on while sinking).
    pub world_clock: bool,
    /// `extras.acr_wind` of the scene root (trees and foliage, docs/rendering-3d.md "Wind"):
    /// the species' scale of the trunk sway, the branch swing and the leaf flutter, and its
    /// tempo; `[1, 1, 1, 1]` without.
    pub wind: [f32; 4],
    /// `extras.acr_machine` of the scene root: a ship or a gun, not a body (no blood pools,
    /// no corpse piles).
    pub machine: bool,
    /// Radius around the origin (world pixels at scale 1) that holds the rest pose, for
    /// culling.
    pub radius: f32,
    /// Limb chains and radius profiles for cutting (skeleton H models only).
    pub rig: Option<crate::damage::Rig>,
    /// A rig guessed from an animal's skeleton (root `extras.acr_animal`,
    /// [`crate::damage::Rig::beast`]): where wound marks and torn volumes go; never cut.
    pub beast_rig: Option<crate::damage::Rig>,
    /// Per-soldier faces, bodies, hair and beards (root `extras.acr_face_morphs`); `None`
    /// draws every soldier alike, as before.
    pub looks: Option<Looks>,
    /// The crew of a gun (#287, `acr_assets/guncrew.py`); `None` for every other model.
    pub crew: Option<Crew>,
    /// The second morph slot holds `emaciated` (#207), not `lie_flat`: no primitive has
    /// `lie_flat` and one has `emaciated`. The instance's weight for it is
    /// `ModelInstance::morphs[2]`.
    pub emaciated: bool,
}

/// The men of a gun model (#287): the bones `c0.`..`c3.` and what hangs on them, so a fallen
/// man can be hidden from the gun and drawn alone as his own corpse. The field gun has four
/// (`ArtPus`), the mortar three (`ArtMrt`, #301).
#[derive(Debug, Clone, PartialEq)]
pub struct Crew {
    /// Per palette slot: the man (0..4) it belongs to, `None` for the gun.
    pub slots: Vec<Option<u8>>,
    /// The order the men fall in ([`Crew::fall_order`]), one entry per man in the model.
    pub fall: Vec<u8>,
    /// Per man: where his hips are in his `STAND` pose (model space, on the ground).
    pub stations: Vec<[f32; 3]>,
    /// Per man: where his hips are once he lies dead (the end of `DEATH`, else `DEATHLIE1`),
    /// on the ground: where his corpse bleeds.
    pub lying: Vec<[f32; 3]>,
    /// The base colours (`extras.acr_base_rgb`) of the coat and the waistcoat that a
    /// uniform replaces, their luminance kept as shading ([`Tint::Coat`], [`Tint::Facings`]).
    pub base: [[f32; 3]; 2],
}

/// The most men a gun crew has (the field gun's four); the gun's bit in a shown mask.
pub const CREW: usize = 4;

impl Crew {
    /// The men in the model.
    pub fn men(&self) -> usize {
        self.fall.len()
    }

    /// The order men at `stations` fall in: the rearmost (least glTF z, the gun facing +z)
    /// stays longest, as the field gun's layer at the trail and the mortar's firer behind
    /// the bed serve the vent and can fire; the others fall from the highest number down
    /// (the field gun: firer, loader, rammer, layer = 3, 1, 0, 2; the mortar: 1, 0, 2).
    pub fn fall_order(stations: &[[f32; 3]]) -> Vec<u8> {
        let last = (0..stations.len()).min_by(|&a, &b| stations[a][2].total_cmp(&stations[b][2]));
        let mut order: Vec<u8> = (0..stations.len()).rev().filter(|&m| Some(m) != last).map(|m| m as u8).collect();
        order.extend(last.map(|m| m as u8));
        order
    }

    /// The pose `palette` with the men not in `shown` (a bit per man; the gun is bit 4)
    /// moved to one point: their triangles collapse and are not drawn.
    pub fn mask(&self, palette: &mut Palette, shown: u8) {
        for (rows, s) in palette.iter_mut().zip(&self.slots) {
            let bit = s.map_or(1 << CREW, |m| 1 << m);
            if shown & bit == 0 {
                *rows = [[0.0; 4]; 3];
            }
        }
    }
}

/// The affine rows (3 × vec4) of each palette slot.
pub type Palette = Vec<[[f32; 4]; 3]>;

/// The authored levels of detail of a glb's nodes ([`Primitive::lod`]).
struct AuthoredLods {
    masters: std::collections::HashSet<String>,
}

impl AuthoredLods {
    /// A level node's own level: `extras.acr_lod`, else a name ending `.lod<digit>`.
    fn level(node: &gltf::Node<'_>) -> Option<u8> {
        let extra = extras_json(node.extras()).and_then(|e| e.get("acr_lod").and_then(serde_json::Value::as_u64));
        let named = || {
            let (_, k) = node.name()?.rsplit_once(".lod")?;
            (k.len() == 1).then(|| k.parse::<u8>().ok()).flatten()
        };
        extra.map(|k| k.min(u64::from(u8::MAX)) as u8).or_else(named).filter(|&k| k >= 1)
    }

    fn new(nodes: &[gltf::Node<'_>]) -> Self {
        let mut masters = std::collections::HashSet::new();
        for n in nodes.iter().filter(|n| Self::level(n).is_some()) {
            let of =
                extras_json(n.extras()).and_then(|e| e.get("acr_lod_of").and_then(|v| v.as_str()).map(str::to_owned));
            if let Some(of) = of.or_else(|| n.name().and_then(|s| s.rsplit_once(".lod")).map(|(of, _)| of.to_owned())) {
                masters.insert(of);
            }
        }
        Self { masters }
    }

    fn of(&self, node: &gltf::Node<'_>) -> Option<u8> {
        Self::level(node).or_else(|| node.name().filter(|n| self.masters.contains(*n)).map(|_| 0))
    }
}

fn extras_json(raw: &gltf::json::Extras) -> Option<serde_json::Value> {
    raw.as_ref().and_then(|r| serde_json::from_str(r.get()).ok())
}

fn floats<const N: usize>(v: Option<&serde_json::Value>) -> Option<[f32; N]> {
    let a = v?.as_array()?;
    if a.len() != N {
        return None;
    }
    let mut out = [0.0; N];
    for (o, x) in out.iter_mut().zip(a) {
        *o = x.as_f64()? as f32;
    }
    Some(out)
}

fn decay_layer(extras: &serde_json::Value) -> Option<DecayLayer> {
    let d = extras.get("acr_decay")?;
    Some(DecayLayer {
        discolour: floats(d.get("discolour")).unwrap_or([0.0, 1.0]),
        tint: floats(d.get("tint")).unwrap_or([0.5; 3]),
        tint_strength: d.get("tint_strength").and_then(serde_json::Value::as_f64).unwrap_or(0.0) as f32,
        dissolve: floats(d.get("dissolve")),
        edge: d.get("edge").and_then(serde_json::Value::as_f64).unwrap_or(0.0) as f32,
        edge_rgb: floats(d.get("edge_rgb")).unwrap_or([0.0; 3]),
        inner: d.get("layer").and_then(serde_json::Value::as_str) == Some("bone")
            || d.get("inner").and_then(serde_json::Value::as_bool) == Some(true),
    })
}

/// A custom (`_`-prefixed) vertex attribute with `N` components as floats, whatever its
/// component type: float, or integers (normalised to 0..1 / -1..1 when the accessor says
/// so, as `assets/blender/lib/acr_assets/gltf_pack.py` writes `_GORE`, `_FACE`, `_WIND`).
fn custom_f32<'a, 's, const N: usize, F>(a: &gltf::Accessor<'a>, get: F) -> Option<Vec<[f32; N]>>
where
    F: Clone + Fn(gltf::Buffer<'a>) -> Option<&'s [u8]>,
{
    use gltf::accessor::{DataType, Dimensions, Iter};
    let n = match a.dimensions() {
        Dimensions::Scalar => 1,
        Dimensions::Vec4 => 4,
        _ => return None,
    };
    if n != N {
        return None;
    }
    let (scale, min) = match (a.normalized(), a.data_type()) {
        (true, DataType::U8) => (255.0, 0.0),
        (true, DataType::U16) => (65535.0, 0.0),
        (true, DataType::I8) => (127.0, -1.0),
        (true, DataType::I16) => (32767.0, -1.0),
        _ => (1.0, f32::MIN),
    };
    let f = move |x: f32| (x / scale).max(min);
    macro_rules! read {
        ($t:ty) => {
            if N == 1 {
                Iter::<$t>::new(a.clone(), get)?.map(|x| std::array::from_fn(|_| f(x as f32))).collect()
            } else {
                Iter::<[$t; 4]>::new(a.clone(), get)?.map(|e| std::array::from_fn(|k| f(e[k] as f32))).collect()
            }
        };
    }
    Some(match a.data_type() {
        DataType::F32 => read!(f32),
        DataType::U8 => read!(u8),
        DataType::U16 => read!(u16),
        DataType::I8 => read!(i8),
        DataType::I16 => read!(i16),
        DataType::U32 => read!(u32),
    })
}

/// A `VEC3` attribute as floats, whatever its component type: float, or the normalised and
/// plain integers `KHR_mesh_quantization` allows for positions, normals and morph targets.
fn vec3_f32<'a, 's, F>(a: &gltf::Accessor<'a>, get: F) -> Option<Vec<[f32; 3]>>
where
    F: Clone + Fn(gltf::Buffer<'a>) -> Option<&'s [u8]>,
{
    use gltf::accessor::{DataType, Dimensions, Iter};
    if a.dimensions() != Dimensions::Vec3 {
        return None;
    }
    let (scale, min) = match (a.normalized(), a.data_type()) {
        (true, DataType::U8) => (255.0, 0.0),
        (true, DataType::U16) => (65535.0, 0.0),
        (true, DataType::I8) => (127.0, -1.0),
        (true, DataType::I16) => (32767.0, -1.0),
        _ => (1.0, f32::MIN),
    };
    let f = move |x: f32| (x / scale).max(min);
    macro_rules! read {
        ($t:ty) => {
            Iter::<[$t; 3]>::new(a.clone(), get)?.map(|e| e.map(|x| f(x as f32))).collect()
        };
    }
    Some(match a.data_type() {
        DataType::F32 => Iter::<[f32; 3]>::new(a.clone(), get)?.collect(),
        DataType::U8 => read!(u8),
        DataType::U16 => read!(u16),
        DataType::I8 => read!(i8),
        DataType::I16 => read!(i16),
        DataType::U32 => return None,
    })
}

/// `extras.acr_dequant = [s, ox, oy, oz]`: the dequantisation `gltf_pack.py` folded into a
/// mesh's positions (`p = o + s q`), a skin's inverse bind matrices or a node's transform.
fn dequant(raw: &gltf::json::Extras) -> Option<[f32; 4]> {
    floats(extras_json(raw)?.get("acr_dequant"))
}

/// The glTF document, accepting `KHR_mesh_quantization` (the `gltf` crate does not list it,
/// but every reader here takes integer positions, normals and morph targets).
fn parse_gltf(bytes: &[u8]) -> Result<gltf::Gltf, String> {
    const QUANT: &str = "KHR_mesh_quantization";
    let (json, blob) = if bytes.starts_with(b"glTF") {
        let glb = gltf::Glb::from_slice(bytes).map_err(|e| format!("glTF: {e}"))?;
        (glb.json.into_owned(), glb.bin.map(std::borrow::Cow::into_owned))
    } else {
        (bytes.to_vec(), None)
    };
    let mut root = gltf::json::Root::from_slice(&json).map_err(|e| format!("glTF: {e}"))?;
    root.extensions_required.retain(|e| e != QUANT);
    let document = gltf::Document::from_json(root).map_err(|e| format!("glTF: {e}"))?;
    Ok(gltf::Gltf { document, blob })
}

/// The level of each material in a model with authored levels: `Some(k)` when only
/// primitives of level `k`, finer than the coarsest the model carries, use it; `None` for the
/// rest (the coarsest level's, those of primitives without a level, unused ones), and for all
/// of a model without authored levels.
pub fn material_levels(primitives: &[Primitive], materials: usize) -> Vec<Option<u8>> {
    let coarsest = primitives.iter().filter_map(|p| p.lod).max().unwrap_or(0);
    let mut out: Vec<Option<Option<u8>>> = vec![None; materials];
    for p in primitives {
        let Some(slot) = out.get_mut(p.material) else { continue };
        let own = p.lod.filter(|&l| l < coarsest);
        *slot = Some(match *slot {
            None => own,
            Some(prev) if prev == own => own,
            Some(_) => None,
        });
    }
    out.into_iter().map(Option::flatten).collect()
}

/// A PNG's size from its header, without decoding it.
fn png_size(bytes: &[u8]) -> Option<(u32, u32)> {
    const SIGNATURE: &[u8] = &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    if bytes.get(..8)? != SIGNATURE || bytes.get(12..16)? != b"IHDR" {
        return None;
    }
    let be = |at: usize| Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?));
    Some((be(16)?, be(20)?))
}

/// A JPEG's size from its frame header, without decoding it.
fn jpeg_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.get(..2)? != [0xFF, 0xD8] {
        return None;
    }
    let mut at = 2;
    loop {
        let (&[0xFF, marker], rest) = bytes.get(at..)?.split_first_chunk::<2>()? else { return None };
        let len = usize::from(u16::from_be_bytes(*rest.first_chunk::<2>()?));
        // SOF0..SOF15 without DHT (C4), JPG (C8) and DAC (CC): precision, height, width.
        if (0xC0..=0xCF).contains(&marker) && ![0xC4, 0xC8, 0xCC].contains(&marker) {
            let h = u16::from_be_bytes(*rest.get(3..)?.first_chunk::<2>()?);
            let w = u16::from_be_bytes(*rest.get(5..)?.first_chunk::<2>()?);
            return Some((u32::from(w), u32::from(h)));
        }
        at += 2 + len;
    }
}

/// A PNG or JPEG file (told apart by its signature) as RGBA8.
pub fn decode_image(bytes: &[u8]) -> Result<Image, String> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) { decode_png(bytes) } else { decode_jpeg(bytes) }
}

fn decode_png(bytes: &[u8]) -> Result<Image, String> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0; reader.output_buffer_size().ok_or("PNG too large")?];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    let pixels = &buf[..info.buffer_size()];
    let rgba = match info.color_type {
        png::ColorType::Rgba => pixels.to_vec(),
        png::ColorType::Rgb => pixels.as_chunks::<3>().0.iter().flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => {
            pixels.as_chunks::<2>().0.iter().flat_map(|p| [p[0], p[0], p[0], p[1]]).collect()
        }
        png::ColorType::Grayscale => pixels.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return Err("indexed PNG not expanded".into()),
    };
    Ok(Image { width: info.width, height: info.height, rgba })
}

/// Baseline or progressive JPEG (`gltf_pack.py` writes opaque base colours so, #199) as RGBA
/// with alpha 255. `zune-jpeg` is pure Rust and builds for wasm.
fn decode_jpeg(bytes: &[u8]) -> Result<Image, String> {
    use zune_jpeg::zune_core::{bytestream::ZCursor, colorspace::ColorSpace, options::DecoderOptions};
    let options = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGBA);
    let mut decoder = zune_jpeg::JpegDecoder::new_with_options(ZCursor::new(bytes), options);
    let rgba = decoder.decode().map_err(|e| format!("JPEG: {e:?}"))?;
    let (w, h) = decoder.dimensions().ok_or("JPEG: no size")?;
    if rgba.len() != w * h * 4 {
        return Err("JPEG: unexpected output size".into());
    }
    Ok(Image { width: w as u32, height: h as u32, rgba })
}

impl ModelData {
    /// Reads a `.glb` file (or a `.gltf` whose buffers are all embedded in the binary chunk).
    pub fn from_glb(bytes: &[u8]) -> Result<Self, String> {
        let gltf = parse_gltf(bytes)?;
        let doc = &gltf.document;
        let blob = gltf.blob.as_deref();
        let buffers: Vec<Option<&[u8]>> = doc
            .buffers()
            .map(|b| match b.source() {
                gltf::buffer::Source::Bin => blob,
                gltf::buffer::Source::Uri(_) => None,
            })
            .collect();
        let get = |b: gltf::Buffer<'_>| buffers.get(b.index()).copied().flatten();

        // Nodes and their parents.
        let mut nodes: Vec<Node> = doc
            .nodes()
            .map(|n| {
                let (mut t, r, mut s) = n.transform().decomposed();
                if let Some([q, o @ ..]) = dequant(n.extras()) {
                    // node' = node * Tr(o) * Scale(q): S = S' / q, t = t' - R (S o).
                    s = s.map(|x| x / q);
                    let v = math::quat_rotate(r, [o[0] * s[0], o[1] * s[1], o[2] * s[2]]);
                    t = [t[0] - v[0], t[1] - v[1], t[2] - v[2]];
                }
                Node { name: n.name().unwrap_or("").to_owned(), parent: None, translation: t, rotation: r, scale: s }
            })
            .collect();
        for n in doc.nodes() {
            for c in n.children() {
                nodes[c.index()].parent = Some(n.index());
            }
        }
        let mut order = Vec::with_capacity(nodes.len());
        let mut stack: Vec<usize> = (0..nodes.len()).filter(|&i| nodes[i].parent.is_none()).rev().collect();
        while let Some(i) = stack.pop() {
            order.push(i);
            stack.extend(doc.nodes().nth(i).into_iter().flat_map(|n| n.children().map(|c| c.index())));
        }

        let scene = doc.default_scene().or_else(|| doc.scenes().next()).ok_or("glTF: no scene")?;
        let root_extras: Vec<serde_json::Value> = scene.nodes().filter_map(|n| extras_json(n.extras())).collect();
        let type_name = root_extras.iter().find_map(|e| e.get("acr_type").and_then(|v| v.as_str()).map(str::to_owned));
        let fps = root_extras
            .iter()
            .find_map(|e| e.get("acr_fps").and_then(serde_json::Value::as_f64))
            .map_or(CLIP_FPS, |f| f as f32);
        let machine =
            root_extras.iter().any(|e| e.get("acr_machine").and_then(serde_json::Value::as_bool) == Some(true));
        let world_clock = root_extras.iter().any(|e| e.get("acr_clock").and_then(|v| v.as_str()) == Some("world"));
        let wind = root_extras
            .iter()
            .find_map(|e| {
                let k: Vec<f32> =
                    e.get("acr_wind")?.as_array()?.iter().filter_map(|v| v.as_f64().map(|f| f as f32)).collect();
                <[f32; 4]>::try_from(k).ok()
            })
            .unwrap_or([1.0; 4]);

        // Encoded first: the pictures only finer authored levels use stay so (below).
        let raw = doc
            .images()
            .map(|img| match img.source() {
                gltf::image::Source::View { view, mime_type } => {
                    let data = get(view.buffer()).ok_or("glTF: image buffer missing")?;
                    let bytes =
                        data.get(view.offset()..view.offset() + view.length()).ok_or("glTF: image out of range")?;
                    match mime_type {
                        "image/png" | "image/jpeg" => Ok(bytes),
                        _ => Err(format!("glTF: {mime_type} textures are not supported (use PNG or JPEG)")),
                    }
                }
                gltf::image::Source::Uri { .. } => Err("glTF: external images are not supported".to_owned()),
            })
            .collect::<Result<Vec<_>, _>>()?;

        let mut materials: Vec<Material> = doc
            .materials()
            .map(|m| {
                let pbr = m.pbr_metallic_roughness();
                let texture = pbr.base_color_texture().map(|t| t.texture());
                let nearest = texture
                    .as_ref()
                    .is_some_and(|t| t.sampler().mag_filter() == Some(gltf::texture::MagFilter::Nearest));
                Material {
                    name: m.name().unwrap_or("").to_owned(),
                    base_color: pbr.base_color_factor(),
                    texture: texture.map(|t| t.source().index()),
                    nearest,
                    alpha_cutoff: (m.alpha_mode() == gltf::material::AlphaMode::Mask)
                        .then(|| m.alpha_cutoff().unwrap_or(0.5)),
                    decay: extras_json(m.extras()).as_ref().and_then(decay_layer),
                    stump: false,
                    tissue: Tissue::None,
                    tint: match extras_json(m.extras())
                        .as_ref()
                        .and_then(|e| e.get("acr_tint")?.as_str().map(str::to_owned))
                    {
                        Some(t) if t == "skin" => Tint::Skin,
                        Some(t) if t == "hair" => Tint::Hair,
                        Some(t) if t == "coat" => Tint::Coat,
                        Some(t) if t == "facings" => Tint::Facings,
                        _ => Tint::None,
                    },
                    face_tiles: extras_json(m.extras())
                        .as_ref()
                        .and_then(|e| e.get("acr_face_tiles")?.as_u64())
                        .map_or(1, |n| n.clamp(1, u64::from(crate::looks::FACE_TILES_MAX)) as u8),
                    finish: extras_json(m.extras()).as_ref().map_or_else(Finish::default, |e| Finish {
                        sheen: e.get("acr_sheen").and_then(serde_json::Value::as_f64).unwrap_or(0.0) as f32,
                        gloss: e.get("acr_gloss").and_then(serde_json::Value::as_f64).unwrap_or(0.0) as f32,
                        layer: crate::layers::LAYER_BIAS
                            * e.get("acr_layer").and_then(serde_json::Value::as_f64).map_or_else(
                                || {
                                    crate::layers::layer_rank(crate::layers::material_key(m.name().unwrap_or("")))
                                        .map_or(0.0, f32::from)
                                },
                                |x| x as f32,
                            ),
                        thin: 0.0,
                    }),
                    surface: Surface {
                        normal: m.normal_texture().map(|t| t.texture().source().index()),
                        normal_scale: m.normal_texture().map_or(1.0, |t| t.scale()),
                        metal_rough: pbr.metallic_roughness_texture().map(|t| t.texture().source().index()),
                        metallic: pbr.metallic_factor(),
                        roughness: pbr.roughness_factor(),
                    },
                }
            })
            .collect();
        // Primitives without a material take a grey default.
        let default_material = materials.len();
        materials.push(Material {
            name: "default".into(),
            base_color: [0.5, 0.5, 0.5, 1.0],
            texture: None,
            nearest: false,
            alpha_cutoff: None,
            decay: None,
            stump: false,
            tissue: Tissue::None,
            tint: Tint::None,
            face_tiles: 1,
            finish: Finish::default(),
            surface: Surface::default(),
        });

        let mut slots: Vec<Slot> = Vec::new();
        let mut node_slot: HashMap<usize, u32> = HashMap::new();
        let mut joint_slot: HashMap<(usize, usize), u32> = HashMap::new();
        let mut primitives = Vec::new();
        let mut skinned_prims: Vec<bool> = Vec::new();
        let mut has_limb = false;
        let (mut has_lie_flat, mut has_emaciated) = (false, false);
        // Primitives whose second slot holds `emaciated`.
        let mut lean_prims = Vec::new();
        let mut scene_nodes: Vec<gltf::Node<'_>> = Vec::new();
        let mut stack: Vec<gltf::Node<'_>> = scene.nodes().collect();
        while let Some(n) = stack.pop() {
            stack.extend(n.children());
            scene_nodes.push(n);
        }
        scene_nodes.sort_by_key(gltf::Node::index);
        let lods = AuthoredLods::new(&scene_nodes);
        for node in &scene_nodes {
            let Some(mesh) = node.mesh() else { continue };
            // Slots this node's vertices refer to: its joints, or the node itself.
            let joints: Vec<u32> = match node.skin() {
                Some(skin) => {
                    let mut ibms: Vec<Mat4> =
                        skin.reader(&get).read_inverse_bind_matrices().map(|it| it.collect()).unwrap_or_default();
                    if let Some([q, o @ ..]) = dequant(skin.extras()) {
                        // IBM' = IBM * Tr(o) * Scale(q), columns first.
                        for m in &mut ibms {
                            for c in m.iter_mut().take(3) {
                                *c = c.map(|x| x / q);
                            }
                            m[3] = std::array::from_fn(|r| m[3][r] - o[0] * m[0][r] - o[1] * m[1][r] - o[2] * m[2][r]);
                        }
                    }
                    skin.joints()
                        .enumerate()
                        .map(|(j, joint)| {
                            *joint_slot.entry((skin.index(), j)).or_insert_with(|| {
                                slots.push(Slot::Joint {
                                    node: joint.index(),
                                    inverse_bind: ibms.get(j).copied().unwrap_or(math::IDENTITY),
                                });
                                slots.len() as u32 - 1
                            })
                        })
                        .collect()
                }
                None => vec![*node_slot.entry(node.index()).or_insert_with(|| {
                    slots.push(Slot::Node(node.index()));
                    slots.len() as u32 - 1
                })],
            };
            let skinned = node.skin().is_some();
            let names: Vec<String> = extras_json(mesh.extras())
                .and_then(|e| e.get("targetNames").cloned())
                .and_then(|v| serde_json::from_value(v).ok())
                .unwrap_or_default();
            // The old joint-only stump caps: replaced by placed caps (`damage.rs`).
            if mesh.name().is_some_and(|n| n.ends_with(".caps")) {
                continue;
            }
            // A hair or beard variant (#283): drawn only for soldiers whose look shows it.
            let variant = mesh
                .name()
                .and_then(|n| n.rsplit('.').next())
                .and_then(|s| crate::looks::VARIANT_NODES.iter().position(|v| *v == s))
                .map_or(0.0, |i| i as f32 + 1.0);
            for prim in mesh.primitives() {
                if prim.mode() != gltf::mesh::Mode::Triangles {
                    continue;
                }
                let reader = prim.reader(&get);
                let Some(positions) = prim.get(&gltf::Semantic::Positions).and_then(|a| vec3_f32(&a, get)) else {
                    continue;
                };
                let (q, o) = match dequant(mesh.extras()) {
                    Some([q, o @ ..]) => (q, o),
                    None => (1.0, [0.0; 3]),
                };
                let mut vertices: Vec<Vertex> = positions
                    .into_iter()
                    .map(|p| Vertex {
                        position: std::array::from_fn(|k| o[k] + q * p[k]),
                        normal: [0.0, 1.0, 0.0],
                        uv: [0.0; 2],
                        color: [1.0; 4],
                        joints: [joints[0], 0, 0, 0],
                        weights: [1.0, 0.0, 0.0, 0.0],
                        morph: [[0.0; 3]; 2],
                        body: [-1.0, -1.0, 0.5, variant],
                        face: [0; 4],
                        wind: [0; 4],
                    })
                    .collect();
                if let Some(a) = prim.get(&gltf::Semantic::Normals) {
                    let unit = a.data_type() != gltf::accessor::DataType::F32;
                    for (v, n) in vertices.iter_mut().zip(vec3_f32(&a, get).unwrap_or_default()) {
                        // Quantised normals are a little off unit length.
                        v.normal = if unit { math::normalize(n) } else { n };
                    }
                }
                if let Some(it) = reader.read_tex_coords(0) {
                    vertices.iter_mut().zip(it.into_f32()).for_each(|(v, t)| v.uv = t);
                }
                if let Some(it) = reader.read_colors(0) {
                    vertices.iter_mut().zip(it.into_rgba_f32()).for_each(|(v, c)| v.color = c);
                }
                let custom = |name: &str| prim.get(&gltf::Semantic::Extras(name.into()));
                if let Some(it) = custom("PART").and_then(|a| custom_f32::<1, _>(&a, get)) {
                    vertices.iter_mut().zip(it).for_each(|(v, x)| v.body[0] = x[0]);
                }
                if let Some(it) = custom("LIMB").and_then(|a| custom_f32::<1, _>(&a, get)) {
                    vertices.iter_mut().zip(it).for_each(|(v, x)| v.body[1] = x[0]);
                    has_limb = true;
                }
                if let Some(it) = custom("GORE").and_then(|a| custom_f32::<4, _>(&a, get)) {
                    vertices.iter_mut().zip(it).for_each(|(v, x)| v.body[2] = x[0]);
                }
                if let Some(it) = custom("FACE").and_then(|a| custom_f32::<4, _>(&a, get)) {
                    vertices
                        .iter_mut()
                        .zip(it)
                        .for_each(|(v, x)| v.face = x.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8));
                }
                if let Some(it) = custom("WIND").and_then(|a| custom_f32::<4, _>(&a, get)) {
                    vertices
                        .iter_mut()
                        .zip(it)
                        .for_each(|(v, x)| v.wind = x.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8));
                }
                if skinned && let (Some(js), Some(ws)) = (reader.read_joints(0), reader.read_weights(0)) {
                    for ((v, j), w) in vertices.iter_mut().zip(js.into_u16()).zip(ws.into_f32()) {
                        v.joints = j.map(|j| joints.get(usize::from(j)).copied().unwrap_or(joints[0]));
                        let sum: f32 = w.iter().sum();
                        v.weights = if sum > 0.0 { w.map(|x| x / sum) } else { [1.0, 0.0, 0.0, 0.0] };
                    }
                }
                let mut morphs = [false; 2];
                let mut look_morphs = Vec::new();
                // `emaciated` takes the second slot when the mesh has no `lie_flat`.
                let lean = !names.iter().any(|n| n == MORPH_NAMES[1]);
                for (t, target) in prim.morph_targets().enumerate() {
                    let positions = target
                        .positions()
                        .and_then(|a| vec3_f32(&a, get))
                        .map(|d| d.into_iter().map(move |d| d.map(|x| q * x)));
                    let name = names.get(t).map(String::as_str);
                    if let Some(l) = name.and_then(crate::looks::look_morph) {
                        if let Some(positions) = positions {
                            look_morphs.push((l, positions.collect()));
                        }
                        continue;
                    }
                    let k = match name {
                        Some(EMACIATED) if lean => Some(1),
                        Some(n) => MORPH_NAMES.iter().position(|m| *m == n),
                        None => (t < 2).then_some(t),
                    };
                    has_lie_flat |= k == Some(1) && name != Some(EMACIATED);
                    has_emaciated |= k == Some(1) && name == Some(EMACIATED);
                    if let (Some(k), Some(positions)) = (k, positions) {
                        morphs[k] = true;
                        vertices.iter_mut().zip(positions).for_each(|(v, d)| v.morph[k] = d);
                    }
                }
                let indices: Vec<u32> = match reader.read_indices() {
                    Some(i) => i.into_u32().collect(),
                    None => (0..vertices.len() as u32).collect(),
                };
                if indices.iter().any(|&i| i as usize >= vertices.len()) {
                    return Err("glTF: index out of range".into());
                }
                let material = prim.material().index().unwrap_or(default_material);
                skinned_prims.push(skinned);
                if lean && morphs[1] {
                    lean_prims.push(primitives.len());
                }
                primitives.push(Primitive { vertices, indices, material, morphs, look_morphs, lod: lods.of(node) });
            }
        }

        let clips = doc
            .animations()
            .map(|a| {
                let mut duration: f32 = 0.0;
                let channels = a
                    .channels()
                    .filter_map(|c| {
                        let property = match c.target().property() {
                            gltf::animation::Property::Translation => Property::Translation,
                            gltf::animation::Property::Rotation => Property::Rotation,
                            gltf::animation::Property::Scale => Property::Scale,
                            gltf::animation::Property::MorphTargetWeights => return None,
                        };
                        let reader = c.reader(&get);
                        let times: Vec<f32> = reader.read_inputs()?.collect();
                        let mut values: Vec<[f32; 4]> = match reader.read_outputs()? {
                            ReadOutputs::Translations(it) => it.map(|v| [v[0], v[1], v[2], 0.0]).collect(),
                            ReadOutputs::Scales(it) => it.map(|v| [v[0], v[1], v[2], 0.0]).collect(),
                            ReadOutputs::Rotations(it) => it.into_f32().collect(),
                            ReadOutputs::MorphTargetWeights(_) => return None,
                        };
                        let interpolation = c.sampler().interpolation();
                        if interpolation == gltf::animation::Interpolation::CubicSpline {
                            // (in-tangent, value, out-tangent) per key: keep the values.
                            values = values.as_chunks::<3>().0.iter().map(|k| k[1]).collect();
                        }
                        if times.is_empty() || values.len() != times.len() {
                            return None;
                        }
                        duration = duration.max(*times.last().unwrap_or(&0.0));
                        Some(Channel {
                            node: c.target().node().index(),
                            property,
                            step: interpolation == gltf::animation::Interpolation::Step,
                            times,
                            values,
                        })
                    })
                    .collect();
                Clip { name: a.name().unwrap_or("").to_owned(), duration, channels }
            })
            .collect();

        let mut rig = crate::damage::Rig::from_skeleton(&nodes, &slots);
        if let Some(rig) = &mut rig {
            if !has_limb {
                // `_LIMB` missing (assets before #262): project the rest positions of the
                // skinned limb vertices onto the chains, as the bake does.
                for (p, _) in primitives.iter_mut().zip(&skinned_prims).filter(|(_, s)| **s) {
                    for v in &mut p.vertices {
                        if let Some(l) = crate::damage::Limb::of_part(v.body[0].round().max(0.0) as u32) {
                            v.body[1] = rig.limbs[l as usize].project(v.position);
                        }
                    }
                }
            }
            rig.measure_radii(primitives.iter().zip(&skinned_prims).filter(|(_, s)| **s).flat_map(|(p, _)| {
                p.vertices
                    .iter()
                    .filter(|v| v.body[0] >= 0.0)
                    .map(|v| (v.position, v.body[0].round() as u32, v.body[1]))
            }));
            if let Some(r) = root_extras.iter().find_map(|e| e.get("acr_limb_radius")) {
                rig.read_radii(r);
            }
        }
        let animal = root_extras.iter().any(|e| e.get("acr_animal").and_then(serde_json::Value::as_bool) == Some(true));
        let beast_rig = (rig.is_none() && animal)
            .then(|| {
                let vertices = primitives.iter().zip(&skinned_prims).filter(|(_, s)| **s).flat_map(|(p, _)| {
                    p.vertices.iter().map(|v| {
                        let k = (0..4).max_by(|&a, &b| v.weights[a].total_cmp(&v.weights[b])).unwrap_or(0);
                        (v.position, v.joints[k])
                    })
                });
                crate::damage::Rig::beast(&nodes, &slots, vertices)
            })
            .flatten();
        if beast_rig.is_some() {
            // An animal's skin is flesh (`_PART` 0): the far wall seen through a wound volume
            // is shaded as tissue. It is never cut (no limb parts, no `ModelData::rig`).
            for (p, _) in primitives.iter_mut().zip(&skinned_prims).filter(|(_, s)| **s) {
                for v in p.vertices.iter_mut().filter(|v| v.body[0] < 0.0) {
                    v.body[0] = 0.0;
                }
            }
        }
        // Individual looks (#283): the style from the odds (civilians wear their hair loose).
        let looks = root_extras.iter().find(|e| e.get("acr_face_morphs").is_some()).map(|e| Looks {
            style: if e.pointer("/acr_look_odds/hair/loose").is_some() {
                crate::looks::Style::Civilian
            } else {
                crate::looks::Style::Regular
            },
            variants: (0..crate::looks::VARIANTS).map(crate::looks::variant_weights).collect(),
            eye: floats::<3>(e.get("acr_face_frame")).map_or(1.61, |f| f[2]),
        });
        // A model with both: the slot is `lie_flat`'s, the lean primitives lose `emaciated`.
        let emaciated = has_emaciated && !has_lie_flat;
        if has_emaciated && has_lie_flat {
            for &i in &lean_prims {
                let p: &mut Primitive = &mut primitives[i];
                p.morphs[1] = false;
                p.vertices.iter_mut().for_each(|v| v.morph[1] = [0.0; 3]);
            }
        }
        // Pictures only finer authored levels than the coarsest use: size only, the file kept.
        let levels = material_levels(&primitives, materials.len());
        let mut deferred = vec![!raw.is_empty(); raw.len()];
        let mut used = vec![false; raw.len()];
        for (m, level) in materials.iter().zip(&levels) {
            for i in [m.texture, m.surface.normal, m.surface.metal_rough].into_iter().flatten() {
                if let Some(d) = deferred.get_mut(i) {
                    *d &= level.is_some();
                    used[i] = true;
                }
            }
        }
        let mut images = Vec::with_capacity(raw.len());
        let mut encoded = Vec::with_capacity(raw.len());
        for (k, bytes) in raw.iter().enumerate() {
            let size = (deferred[k] && used[k]).then(|| png_size(bytes).or_else(|| jpeg_size(bytes))).flatten();
            match size {
                Some((width, height)) => {
                    images.push(Image { width, height, rgba: Vec::new() });
                    encoded.push(Some(std::sync::Arc::<[u8]>::from(*bytes)));
                }
                None => {
                    images.push(decode_image(bytes)?);
                    encoded.push(None);
                }
            }
        }
        let mut model = Self {
            nodes,
            order,
            slots,
            primitives,
            materials,
            images,
            encoded,
            clips,
            fps,
            type_name,
            world_clock,
            wind,
            machine,
            radius: 0.0,
            rig,
            beast_rig,
            looks,
            crew: None,
            emaciated,
        };
        model.radius = model.rest_radius();
        if machine {
            model.crew = model.find_crew(doc);
        }
        Ok(model)
    }

    /// The clip called `name` (case-insensitive).
    pub fn clip(&self, name: &str) -> Option<usize> {
        self.clips.iter().position(|c| c.name.eq_ignore_ascii_case(name))
    }

    /// The top of the rest pose above the model's origin, in metres (glTF +y).
    pub fn rest_height(&self) -> f32 {
        let palette = self.pose(None, 0.0);
        let mut top: f32 = 0.0;
        for p in &self.primitives {
            for v in &p.vertices {
                let rows = &palette[v.joints[0] as usize];
                let y =
                    rows[1][0] * v.position[0] + rows[1][1] * v.position[1] + rows[1][2] * v.position[2] + rows[1][3];
                top = top.max(y);
            }
        }
        top
    }

    fn rest_radius(&self) -> f32 {
        let palette = self.pose(None, 0.0);
        let mut r: f32 = 0.0;
        for p in &self.primitives {
            for v in &p.vertices {
                let rows = &palette[v.joints[0] as usize];
                let q: [f32; 3] = std::array::from_fn(|i| {
                    rows[i][0] * v.position[0] + rows[i][1] * v.position[1] + rows[i][2] * v.position[2] + rows[i][3]
                });
                r = r.max(math::dot(q, q).sqrt());
            }
        }
        r * crate::view3d::PIXELS_PER_METRE
    }

    /// A static model of one primitive and material (the procedural stump cap).
    pub fn single(primitive: Primitive, material: Material) -> Self {
        let node = Node {
            name: "root".into(),
            parent: None,
            translation: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0; 3],
        };
        let mut model = Self {
            nodes: vec![node],
            order: vec![0],
            slots: vec![Slot::Node(0)],
            primitives: vec![primitive],
            materials: vec![material],
            images: Vec::new(),
            encoded: Vec::new(),
            clips: Vec::new(),
            fps: CLIP_FPS,
            type_name: None,
            world_clock: false,
            wind: [1.0; 4],
            machine: false,
            radius: 0.0,
            rig: None,
            beast_rig: None,
            looks: None,
            crew: None,
            emaciated: false,
        };
        model.radius = model.rest_radius();
        model
    }

    /// The crew of a gun: slots under the nodes `c0.`..`c3.` (bones, and gear on their
    /// sockets); its coat and waistcoat materials (`<name>.crewN.coat.*`,
    /// `.waistcoat.*`) take the uniform tints. `None` without such nodes.
    fn find_crew(&mut self, doc: &gltf::Document) -> Option<Crew> {
        let man = |name: &str| -> Option<u8> {
            let b = name.as_bytes();
            (b.len() > 2 && b[0] == b'c' && b[2] == b'.' && (b'0'..b'0' + CREW as u8).contains(&b[1]))
                .then(|| b[1] - b'0')
        };
        let of_node = |mut n: usize| -> Option<u8> {
            loop {
                if let Some(m) = man(&self.nodes[n].name) {
                    return Some(m);
                }
                n = self.nodes[n].parent?;
            }
        };
        let slots: Vec<Option<u8>> = self
            .slots
            .iter()
            .map(|s| match *s {
                Slot::Node(n) | Slot::Joint { node: n, .. } => of_node(n),
            })
            .collect();
        let men = slots.iter().flatten().map(|&m| usize::from(m) + 1).max()?;
        // Each man's hips, put on the ground, in a pose of the gun.
        let hips: Vec<Option<usize>> = (0..men)
            .map(|m| {
                let named = |end: &str| self.nodes.iter().position(|n| n.name == format!("c{m}.{end}"));
                named("hips").or_else(|| named("root"))
            })
            .collect();
        let at = |clip: Option<usize>, time: f32| -> Vec<[f32; 3]> {
            let g = self.globals(clip, time);
            hips.iter().map(|h| h.map_or([0.0; 3], |h| [g[h][3][0], 0.0, g[h][3][2]])).collect()
        };
        let stations = at(self.clip("STAND"), 0.0);
        let lying = match (self.clip("DEATH"), self.clip("DEATHLIE1")) {
            (Some(d), _) => at(Some(d), self.clips[d].duration),
            (None, Some(l)) => at(Some(l), 0.0),
            (None, None) => stations.clone(),
        };
        let fall = Crew::fall_order(&stations);
        let mut base = [[1.0; 3]; 2];
        for (m, json) in self.materials.iter_mut().zip(doc.materials()) {
            if m.tint != Tint::None || !m.name.contains(".crew") {
                continue;
            }
            let k = if m.name.contains(".waistcoat.") {
                1
            } else if m.name.contains(".coat.") {
                0
            } else {
                continue;
            };
            m.tint = if k == 0 { Tint::Coat } else { Tint::Facings };
            if let Some(rgb) = extras_json(json.extras()).as_ref().and_then(|e| floats::<3>(e.get("acr_base_rgb"))) {
                base[k] = rgb;
            }
        }
        Some(Crew { slots, fall, stations, lying, base })
    }

    /// Local transforms of all nodes: the rest pose with `clip` applied at `time` seconds.
    fn locals(&self, clip: Option<usize>, time: f32) -> Vec<Mat4> {
        let mut trs: Vec<([f32; 3], [f32; 4], [f32; 3])> =
            self.nodes.iter().map(|n| (n.translation, n.rotation, n.scale)).collect();
        if let Some(clip) = clip.and_then(|c| self.clips.get(c)) {
            for ch in &clip.channels {
                let Some(node) = trs.get_mut(ch.node) else { continue };
                let v = sample(ch, time);
                match ch.property {
                    Property::Translation => node.0 = [v[0], v[1], v[2]],
                    Property::Rotation => node.1 = v,
                    Property::Scale => node.2 = [v[0], v[1], v[2]],
                }
            }
        }
        trs.iter().map(|&(t, r, s)| math::from_trs(t, r, s)).collect()
    }

    /// Global transforms of all nodes for `clip` at `time`.
    pub fn globals(&self, clip: Option<usize>, time: f32) -> Vec<Mat4> {
        let locals = self.locals(clip, time);
        let mut globals = vec![math::IDENTITY; self.nodes.len()];
        for &i in &self.order {
            globals[i] = match self.nodes[i].parent {
                Some(p) => math::mul(&globals[p], &locals[i]),
                None => locals[i],
            };
        }
        globals
    }

    /// The palette for `clip` at `time` seconds (bind pose for `None`), as affine rows.
    pub fn pose(&self, clip: Option<usize>, time: f32) -> Palette {
        let globals = self.globals(clip, time);
        self.slots
            .iter()
            .map(|s| match *s {
                Slot::Node(n) => math::affine_rows(&globals[n]),
                Slot::Joint { node, inverse_bind } => math::affine_rows(&math::mul(&globals[node], &inverse_bind)),
            })
            .collect()
    }

    /// The clip for the `.MD` animation `anim` (`#STAND`, `#MOTION_L1`, `#DEATHLIE2`, ...):
    /// the clip of that name (without `#`), else without its variant digits (`MOTION_L`),
    /// else the family's first (`DEATHLIE1` for any `DEATHLIE*`), else `STAND`, else the first
    /// clip; `None` (bind pose) for a model without clips.
    pub fn clip_for_animation(&self, anim: &str) -> Option<usize> {
        let name = anim.trim_start_matches('#');
        let base = name.trim_end_matches(|c: char| c.is_ascii_digit());
        self.clip(name)
            .or_else(|| self.clip(base))
            .or_else(|| self.clip(&format!("{base}1")))
            .or_else(|| self.clip("STAND"))
            .or_else(|| (!self.clips.is_empty()).then_some(0))
    }

    pub fn triangle_count(&self) -> usize {
        self.primitives.iter().map(|p| p.indices.len() / 3).sum()
    }

    /// The node a primitive's mesh hangs on, for unskinned meshes (`None` when skinned).
    fn primitive_node(&self, p: &Primitive) -> Option<usize> {
        let slot = p.vertices.first()?.joints[0] as usize;
        match self.slots.get(slot)? {
            Slot::Node(n) => Some(*n),
            Slot::Joint { .. } => None,
        }
    }

    /// The meshes on the node called `name` alone, the node moved to the origin and freed of
    /// its parent (keeping its own rotation and scale): one gib of a pack (`gore.glb`) or a
    /// piece of held gear (the musket) to place on its own. `None` without such a mesh.
    pub fn node_part(&self, name: &str) -> Option<ModelData> {
        let node = self.nodes.iter().position(|n| n.name == name)?;
        let primitives: Vec<Primitive> =
            self.primitives.iter().filter(|p| self.primitive_node(p) == Some(node)).cloned().collect();
        if primitives.is_empty() {
            return None;
        }
        let mut model = Self { primitives, clips: Vec::new(), rig: None, beast_rig: None, ..self.clone() };
        let n = &mut model.nodes[node];
        n.parent = None;
        n.translation = [0.0; 3];
        model.radius = model.rest_radius();
        Some(model)
    }

    /// Names of the nodes that carry unskinned meshes (gibs of a pack, held gear).
    pub fn mesh_nodes(&self) -> Vec<&str> {
        let mut out: Vec<&str> = self
            .primitives
            .iter()
            .filter_map(|p| self.primitive_node(p))
            .map(|n| self.nodes[n].name.as_str())
            .collect();
        out.dedup();
        out
    }

    /// The face and body variants of a model with looks (#283), baked on the CPU: the
    /// vertices of the primitives the look morphs touch, in [`Self::primitives`] order, with
    /// [`Looks::variants`]`[k]` mixed into the positions, one list per variant. `None`
    /// without looks or look morphs.
    pub fn bake_looks(&self) -> Option<Vec<Vec<Vertex>>> {
        let looks = self.looks.as_ref()?;
        let touched: Vec<&Primitive> = self.primitives.iter().filter(|p| !p.look_morphs.is_empty()).collect();
        if touched.is_empty() {
            return None;
        }
        Some(
            looks
                .variants
                .iter()
                .map(|w| {
                    let mut out = Vec::with_capacity(touched.iter().map(|p| p.vertices.len()).sum());
                    for p in &touched {
                        let start = out.len();
                        out.extend_from_slice(&p.vertices);
                        for (t, deltas) in &p.look_morphs {
                            let k = w[*t];
                            if k == 0.0 {
                                continue;
                            }
                            for (v, d) in out[start..].iter_mut().zip(deltas) {
                                v.position.iter_mut().zip(d).for_each(|(p, d)| *p += k * d);
                            }
                        }
                    }
                    out
                })
                .collect(),
        )
    }

    /// The corpse rots by the shader's procedural fallback (`model.wgsl`, "Procedural
    /// decay"): a skinned body without any `acr_decay` mask (the animals), not a machine.
    /// Their corpses darken, discolour, collapse, sink and dissolve by noise from the rest
    /// position; the sink is applied in the shader after the palette, so it also holds for
    /// poses from the body physics.
    pub fn decays_procedurally(&self) -> bool {
        !self.machine
            && self.materials.iter().all(|m| m.decay.is_none())
            && self.slots.iter().any(|s| matches!(s, Slot::Joint { .. }))
    }

    /// The model without the meshes on nodes whose name `drop` accepts (held gear left out
    /// for crawlers: the musket and hat lie on the ground instead).
    pub fn without_nodes(&self, drop: impl Fn(&str) -> bool) -> ModelData {
        let primitives = self
            .primitives
            .iter()
            .filter(|p| !self.primitive_node(p).is_some_and(|n| drop(&self.nodes[n].name)))
            .cloned()
            .collect();
        Self { primitives, ..self.clone() }
    }

    /// The model without the meshes on nodes that `drop` accepts or that hang below one it
    /// accepts (the state groups of a building: `<name>.ruin`, `<name>.build0`, ...).
    pub fn without_groups(&self, drop: impl Fn(&str) -> bool) -> ModelData {
        let dropped = |mut n: usize| loop {
            if drop(&self.nodes[n].name) {
                return true;
            }
            match self.nodes[n].parent {
                Some(p) => n = p,
                None => return false,
            }
        };
        let primitives =
            self.primitives.iter().filter(|p| !self.primitive_node(p).is_some_and(dropped)).cloned().collect();
        Self { primitives, ..self.clone() }
    }

    /// The model with only the meshes on nodes that `keep` accepts or that hang below one it
    /// accepts (one state group of a building: a construction stage, a section state).
    pub fn only_groups(&self, keep: impl Fn(&str) -> bool) -> ModelData {
        let kept = |mut n: usize| loop {
            if keep(&self.nodes[n].name) {
                return true;
            }
            match self.nodes[n].parent {
                Some(p) => n = p,
                None => return false,
            }
        };
        let primitives = self.primitives.iter().filter(|p| self.primitive_node(p).is_some_and(kept)).cloned().collect();
        Self { primitives, ..self.clone() }
    }
}

/// The decay of a corpse (0 fresh .. 1 gone) from its death counter (`Unit::death`, +1 per
/// tick in `#DEATH`, +2 lying) and the counter at which it is removed (1200, or 2000 for types
/// with `#DEATHLIE2`): it starts where the original turns the corpse half transparent
/// (`docs/assets-humans.md`, "Decay").
pub fn corpse_decay(death: u16, removed_at: u16) -> f32 {
    ((f32::from(death) - 100.0) / (f32::from(removed_at) - 100.0).max(1.0)).clamp(0.0, decay_cap())
}

/// Where decay stops with `corpse_decay=half` (settings): the flesh half rotted (its
/// dissolve runs 0.20–0.52), hair and cloth going, the bones not yet bleached (0.50 on) and
/// the corpse not sunk (0.78 on).
pub const HALF_DECAY: f32 = 0.45;

/// The highest decay a corpse reaches (bits of an `f32`; 1.0 by default).
static DECAY_CAP: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0x3f80_0000);

/// The highest decay a corpse reaches: 1 (bones), or [`HALF_DECAY`] with `corpse_decay=half`.
pub fn decay_cap() -> f32 {
    f32::from_bits(DECAY_CAP.load(std::sync::atomic::Ordering::Relaxed))
}

/// Sets [`decay_cap`] (the `corpse_decay` setting), clamped to 0..=1.
pub fn set_decay_cap(cap: f32) {
    DECAY_CAP.store(cap.clamp(0.0, 1.0).to_bits(), std::sync::atomic::Ordering::Relaxed);
}

/// `clamp((x − a) / (b − a), 0, 1)`.
pub fn ramp(x: f32, a: f32, b: f32) -> f32 {
    ((x - a) / (b - a).max(1e-6)).clamp(0.0, 1.0)
}

/// What a corpse's decay does to the whole model: the morph weights `deflate` and
/// `lie_flat` (1 once it lies) and how far it has sunk into the ground (metres).
pub fn decay_pose(decay: f32, lying: bool) -> ([f32; 2], f32) {
    ([ramp(decay, 0.18, 0.55), if lying { 1.0 } else { 0.0 }], ramp(decay, 0.78, 1.0) * 0.20)
}

/// A channel's value at `time` (clamped to its keys).
fn sample(ch: &Channel, time: f32) -> [f32; 4] {
    let n = ch.times.len();
    let i = ch.times.partition_point(|&t| t <= time);
    if i == 0 {
        return ch.values[0];
    }
    if i >= n || ch.step {
        return ch.values[i.min(n) - 1];
    }
    let (t0, t1) = (ch.times[i - 1], ch.times[i]);
    let f = if t1 > t0 { (time - t0) / (t1 - t0) } else { 0.0 };
    let (a, b) = (ch.values[i - 1], ch.values[i]);
    match ch.property {
        Property::Rotation => math::slerp(a, b, f),
        _ => std::array::from_fn(|k| a[k] + (b[k] - a[k]) * f),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Builds a `.glb` from a JSON document and its binary chunk.
    pub(crate) fn glb(json: &str, bin: &[u8]) -> Vec<u8> {
        let mut json = json.as_bytes().to_vec();
        while !json.len().is_multiple_of(4) {
            json.push(b' ');
        }
        let mut bin = bin.to_vec();
        while !bin.len().is_multiple_of(4) {
            bin.push(0);
        }
        let total = 12 + 8 + json.len() + 8 + bin.len();
        let mut out = Vec::new();
        out.extend(b"glTF");
        out.extend(2u32.to_le_bytes());
        out.extend((total as u32).to_le_bytes());
        out.extend((json.len() as u32).to_le_bytes());
        out.extend(b"JSON");
        out.extend(&json);
        out.extend((bin.len() as u32).to_le_bytes());
        out.extend(b"BIN\0");
        out.extend(&bin);
        out
    }

    fn f32s(v: &[f32]) -> Vec<u8> {
        v.iter().flat_map(|x| x.to_le_bytes()).collect()
    }

    /// A tiny skinned model: one triangle on two joints (`hips` at the origin, `arm` 1 m
    /// up), a static box-less "hat" quad on the arm, morph targets `lie_flat` and `deflate`
    /// (in that order), a decaying material and a clip `WAVE` that turns the arm 90° about
    /// +z over one second and moves the hips by a step.
    pub(crate) fn tiny_model() -> Vec<u8> {
        let mut bin = Vec::new();
        let mut views = Vec::new();
        let mut push = |bytes: Vec<u8>| {
            let at = bin.len();
            views.push(format!("{{\"buffer\":0,\"byteOffset\":{at},\"byteLength\":{}}}", bytes.len()));
            bin.extend(bytes);
            while !bin.len().is_multiple_of(4) {
                bin.push(0);
            }
        };
        // 0 positions, 1 normals, 2 colours (u16 normalised), 3 joints (u8), 4 weights,
        // 5 indices (u16), 6 morph lie_flat, 7 morph deflate, 8 inverse binds,
        // 9 hat positions, 10 times, 11 rotations, 12 step times, 13 translations.
        push(f32s(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 2.0, 0.0]));
        push(f32s(&[0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0]));
        push(
            [0u16, 0, 0, 65535, 32768, 0, 0, 65535, 65535, 65535, 0, 65535]
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect(),
        );
        push(vec![0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0]);
        push(f32s(&[1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0]));
        push([0u16, 1, 2, 0].iter().flat_map(|v| v.to_le_bytes()).collect());
        push(f32s(&[0.0, 0.0, 0.5, 0.0, 0.0, 0.5, 0.0, 0.0, 0.5]));
        push(f32s(&[0.0, -0.1, 0.0, 0.0, -0.1, 0.0, 0.0, -0.1, 0.0]));
        let identity = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];
        let mut arm_ibm = identity;
        arm_ibm[13] = -1.0;
        push(f32s(&[identity, arm_ibm].concat()));
        push(f32s(&[0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, 0.5, 0.0]));
        push(f32s(&[0.0, 1.0]));
        let h = std::f32::consts::FRAC_1_SQRT_2;
        push(f32s(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, h, h]));
        push(f32s(&[0.0, 0.5]));
        push(f32s(&[0.0, 0.0, 0.0, 0.0, 0.0, 3.0]));
        // 14 `_GORE` as normalised bytes, 15 `_PART` as plain bytes 4 apart (`gltf_pack.py`).
        push(vec![255, 0, 0, 0, 51, 0, 0, 0, 0, 0, 0, 0]);
        push(vec![3, 0, 0, 0, 5, 0, 0, 0, 7, 0, 0, 0]);
        views[15] = views[15].replace('}', ",\"byteStride\":4}");
        let acc = |view: usize, ctype: u32, count: usize, ty: &str, extra: &str| {
            format!("{{\"bufferView\":{view},\"componentType\":{ctype},\"count\":{count},\"type\":\"{ty}\"{extra}}}")
        };
        let accessors = [
            acc(0, 5126, 3, "VEC3", ",\"min\":[0,0,0],\"max\":[1,2,0]"),
            acc(1, 5126, 3, "VEC3", ""),
            acc(2, 5123, 3, "VEC4", ",\"normalized\":true"),
            acc(3, 5121, 3, "VEC4", ""),
            acc(4, 5126, 3, "VEC4", ""),
            acc(5, 5123, 3, "SCALAR", ""),
            acc(6, 5126, 3, "VEC3", ",\"min\":[0,0,0.5],\"max\":[0,0,0.5]"),
            acc(7, 5126, 3, "VEC3", ",\"min\":[0,-0.1,0],\"max\":[0,-0.1,0]"),
            acc(8, 5126, 2, "MAT4", ""),
            acc(9, 5126, 3, "VEC3", ",\"min\":[0,0,0],\"max\":[0.5,0.5,0]"),
            acc(10, 5126, 2, "SCALAR", ",\"min\":[0],\"max\":[1]"),
            acc(11, 5126, 2, "VEC4", ""),
            acc(12, 5126, 2, "SCALAR", ",\"min\":[0],\"max\":[0.5]"),
            acc(13, 5126, 2, "VEC3", ""),
            acc(14, 5121, 3, "VEC4", ",\"normalized\":true"),
            acc(15, 5121, 3, "SCALAR", ""),
        ];
        let json = format!(
            r#"{{"asset":{{"version":"2.0"}},"scene":0,
            "scenes":[{{"nodes":[0]}}],
            "nodes":[
              {{"name":"tiny","children":[1,3],"extras":{{"acr_type":"Tiny","acr_fps":10,"acr_clock":"world","acr_wind":[0.5,1,2,1.5]}}}},
              {{"name":"hips","children":[2]}},
              {{"name":"arm","translation":[0,1,0],"children":[4]}},
              {{"name":"tiny.body","mesh":0,"skin":0}},
              {{"name":"tiny.hat","mesh":1,"translation":[0,0.5,0]}}],
            "skins":[{{"joints":[1,2],"inverseBindMatrices":8}}],
            "meshes":[
              {{"name":"body","extras":{{"targetNames":["lie_flat","deflate"]}},"primitives":[{{"attributes":{{"POSITION":0,"NORMAL":1,"COLOR_0":2,"JOINTS_0":3,"WEIGHTS_0":4,"_GORE":14,"_PART":15}},"indices":5,"material":0,
                "targets":[{{"POSITION":6}},{{"POSITION":7}}]}}]}},
              {{"name":"hat","primitives":[{{"attributes":{{"POSITION":9}}}}]}}],
            "materials":[{{"name":"skin","pbrMetallicRoughness":{{"baseColorFactor":[0.5,0.25,0.125,1]}},
              "extras":{{"acr_decay":{{"layer":"flesh","discolour":[0.0,0.22],"tint":[0.47,0.45,0.33],"tint_strength":0.85,"dissolve":[0.2,0.52],"edge":0.07,"edge_rgb":[0.16,0.09,0.06]}}}}}}],
            "animations":[{{"name":"WAVE","channels":[{{"sampler":0,"target":{{"node":2,"path":"rotation"}}}},{{"sampler":1,"target":{{"node":1,"path":"translation"}}}}],
              "samplers":[{{"input":10,"output":11,"interpolation":"LINEAR"}},{{"input":12,"output":13,"interpolation":"STEP"}}]}}],
            "accessors":[{}],
            "bufferViews":[{}],
            "buffers":[{{"byteLength":{}}}]}}"#,
            accessors.join(","),
            views.join(","),
            bin.len()
        );
        glb(&json, &bin)
    }

    /// A scene of one empty node and one embedded image.
    fn image_model(mime: &str, bytes: &[u8]) -> Vec<u8> {
        let json = format!(
            r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"name":"n"}}],
            "images":[{{"bufferView":0,"mimeType":"{mime}"}}],
            "bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":{}}}],"buffers":[{{"byteLength":{}}}]}}"#,
            bytes.len(),
            bytes.len().next_multiple_of(4)
        );
        glb(&json, bytes)
    }

    /// A 16×8 RGB test card: a red ramp across, a green ramp down, blue constant.
    fn card() -> Vec<u8> {
        (0..8u32).flat_map(|y| (0..16u32).flat_map(move |x| [(x * 16) as u8, (y * 32) as u8, 160])).collect()
    }

    #[test]
    fn png_texture_round_trips() {
        let mut bytes = Vec::new();
        let mut enc = png::Encoder::new(&mut bytes, 16, 8);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&card()).unwrap();
        let data = ModelData::from_glb(&image_model("image/png", &bytes)).unwrap();
        let img = &data.images[0];
        assert_eq!((img.width, img.height), (16, 8));
        let want: Vec<u8> = card().as_chunks::<3>().0.iter().flat_map(|p| [p[0], p[1], p[2], 255]).collect();
        assert_eq!(img.rgba, want);
    }

    #[test]
    fn header_sizes_match_decoded() {
        let jpeg = jpeg_encoder_bytes(5, 3);
        assert_eq!(super::jpeg_size(&jpeg), Some((5, 3)));
        let img = super::decode_image(&jpeg).unwrap();
        assert_eq!((img.width, img.height), (5, 3));
        let mut png = Vec::new();
        {
            let mut e = png::Encoder::new(&mut png, 7, 2);
            e.set_color(png::ColorType::Rgba);
            e.write_header().unwrap().write_image_data(&[9; 7 * 2 * 4]).unwrap();
        }
        assert_eq!(super::png_size(&png), Some((7, 2)));
        assert_eq!(super::decode_image(&png).unwrap().width, 7);
        assert_eq!(super::jpeg_size(&png), None);
    }

    fn jpeg_encoder_bytes(w: u16, h: u16) -> Vec<u8> {
        let mut out = Vec::new();
        let e = jpeg_encoder::Encoder::new(&mut out, 90);
        e.encode(&vec![128; usize::from(w) * usize::from(h) * 3], w, h, jpeg_encoder::ColorType::Rgb).unwrap();
        out
    }

    #[test]
    fn jpeg_texture_round_trips() {
        let mut bytes = Vec::new();
        jpeg_encoder::Encoder::new(&mut bytes, 95).encode(&card(), 16, 8, jpeg_encoder::ColorType::Rgb).unwrap();
        let data = ModelData::from_glb(&image_model("image/jpeg", &bytes)).unwrap();
        let img = &data.images[0];
        assert_eq!((img.width, img.height), (16, 8));
        for (got, want) in img.rgba.as_chunks::<4>().0.iter().zip(card().as_chunks::<3>().0) {
            assert_eq!(got[3], 255);
            for c in 0..3 {
                assert!(got[c].abs_diff(want[c]) <= 12, "{got:?} vs {want:?}");
            }
        }
        assert!(ModelData::from_glb(&image_model("image/webp", &bytes)).is_err());
    }

    fn near(a: [f32; 3], b: [f32; 3]) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() < 1e-4)
    }

    fn apply(rows: &[[f32; 4]; 3], p: [f32; 3]) -> [f32; 3] {
        std::array::from_fn(|i| rows[i][0] * p[0] + rows[i][1] * p[1] + rows[i][2] * p[2] + rows[i][3])
    }

    #[test]
    fn loads_a_tiny_skinned_model() {
        let m = ModelData::from_glb(&tiny_model()).unwrap();
        assert_eq!(m.type_name.as_deref(), Some("Tiny"));
        assert_eq!(m.fps, 10.0);
        assert!(m.world_clock);
        assert_eq!(m.wind, [0.5, 1.0, 2.0, 1.5]);
        assert_eq!(m.primitives.len(), 2);
        assert_eq!(m.triangle_count(), 2);
        // Skinned triangle: two joint slots; the hat a node slot of its own.
        assert_eq!(m.slots.len(), 3);
        let body = &m.primitives[0];
        assert_eq!(body.vertices[2].joints[0], 1, "the top vertex hangs on the arm");
        assert_eq!(body.vertices[0].weights, [1.0, 0.0, 0.0, 0.0]);
        let c = body.vertices[1].color;
        assert!((c[0] - 0.5).abs() < 1e-4 && c[1] == 0.0 && c[3] == 1.0, "{c:?}");
        // Morph targets are found by name, whatever their order.
        assert_eq!(body.morphs, [true, true]);
        assert_eq!(body.vertices[0].morph[0], [0.0, -0.1, 0.0], "deflate");
        assert_eq!(body.vertices[0].morph[1], [0.0, 0.0, 0.5], "lie_flat");
        let hat = &m.primitives[1];
        assert_eq!(hat.vertices[0].joints[0], 2);
        assert_eq!(hat.morphs, [false, false]);
        let mat = &m.materials[body.material];
        assert_eq!(mat.base_color, [0.5, 0.25, 0.125, 1.0]);
        let decay = mat.decay.unwrap();
        assert_eq!(decay.dissolve, Some([0.2, 0.52]));
        assert_eq!(m.materials[hat.material].name, "default");
        assert_eq!(m.clip("wave"), Some(0));
        assert_eq!(m.clips[0].duration, 1.0);
        // The rest pose reaches 2 m (the top vertex).
        assert!((m.radius - 60.0).abs() < 1e-3, "{}", m.radius);
    }

    #[test]
    fn reads_quantised_custom_attributes() {
        let m = ModelData::from_glb(&tiny_model()).unwrap();
        let body: Vec<[f32; 4]> = m.primitives[0].vertices.iter().map(|v| v.body).collect();
        assert_eq!(body.iter().map(|b| b[0]).collect::<Vec<_>>(), [3.0, 5.0, 7.0]);
        assert!((body[0][2] - 1.0).abs() < 1e-6 && (body[1][2] - 0.2).abs() < 1e-6 && body[2][2] == 0.0);
    }

    #[test]
    fn samples_clips_into_the_palette() {
        let m = ModelData::from_glb(&tiny_model()).unwrap();
        let rest = m.pose(None, 0.0);
        // Bind pose: skinning leaves the vertices where they are.
        assert!(near(apply(&rest[1], [0.0, 2.0, 0.0]), [0.0, 2.0, 0.0]));
        // The hat sits 0.5 m above the arm joint (1 m).
        assert!(near(apply(&rest[2], [0.0, 0.0, 0.0]), [0.0, 1.5, 0.0]));
        // At the end of WAVE the arm is turned 90° about +z: (0, 2) → (−1, 1); the hips
        // moved 3 m south by the step key at 0.5 s.
        let end = m.pose(Some(0), 1.0);
        assert!(near(apply(&end[1], [0.0, 2.0, 0.0]), [-1.0, 1.0, 3.0]));
        assert!(near(apply(&end[0], [1.0, 0.0, 0.0]), [1.0, 0.0, 3.0]));
        // Half way: 45°, and the step has not happened before 0.5 s.
        let h = std::f32::consts::FRAC_1_SQRT_2;
        let mid = m.pose(Some(0), 0.49);
        let mid_rot = m.pose(Some(0), 0.5);
        assert!(near(apply(&mid[0], [0.0; 3]), [0.0; 3]));
        assert!(near(apply(&mid_rot[1], [0.0, 2.0, 0.0]), [-h, 1.0 + h, 3.0]));
        // Past the end the last key holds.
        assert!(near(apply(&m.pose(Some(0), 5.0)[1], [0.0, 2.0, 0.0]), [-1.0, 1.0, 3.0]));
        // The hat follows the arm: 0.5 m along the turned arm's +y.
        assert!(near(apply(&end[2], [0.0; 3]), [-0.5, 1.0, 3.0]));
    }

    /// `ACR_TEST_GLB=path/to/model.glb cargo test -p acr-render model -- --nocapture` loads a
    /// real model and samples every clip.
    #[test]
    fn loads_a_model_file_when_given() {
        let Ok(path) = std::env::var("ACR_TEST_GLB") else { return };
        let m = ModelData::from_glb(&std::fs::read(&path).unwrap()).unwrap();
        println!(
            "{path}: type {:?}, {} nodes, {} slots, {} primitives, {} triangles, {} materials ({} decaying), radius {:.1} px",
            m.type_name,
            m.nodes.len(),
            m.slots.len(),
            m.primitives.len(),
            m.triangle_count(),
            m.materials.len(),
            m.materials.iter().filter(|x| x.decay.is_some()).count(),
            m.radius
        );
        for (i, c) in m.clips.iter().enumerate() {
            let t = std::time::Instant::now();
            let p = m.pose(Some(i), c.duration * 0.5);
            println!("  clip {} {:.3} s, {} channels, pose in {:?}", c.name, c.duration, c.channels.len(), t.elapsed());
            assert_eq!(p.len(), m.slots.len());
        }
    }

    #[test]
    fn models_without_masks_decay_procedurally() {
        let mut m = ModelData::from_glb(&tiny_model()).unwrap();
        // A soldier with his decay masks rots by them.
        assert!(!m.decays_procedurally());
        // A skinned animal without masks takes the shader's fallback.
        m.materials[0].decay = None;
        assert!(m.decays_procedurally());
        // Ships and guns never rot.
        m.machine = true;
        assert!(!m.decays_procedurally());
        // Nor does a static model (a building).
        m.machine = false;
        m.slots.retain(|s| matches!(s, Slot::Node(_)));
        assert!(!m.decays_procedurally());
    }

    #[test]
    fn animations_pick_clips() {
        let mut m = ModelData::from_glb(&tiny_model()).unwrap();
        let named = |names: &[&str]| -> Vec<Clip> {
            names.iter().map(|n| Clip { name: (*n).into(), duration: 0.0, channels: Vec::new() }).collect()
        };
        m.clips = named(&["DEATH", "DEATHLIE1", "STAND", "MOTION_L"]);
        let pick = |m: &ModelData, a: &str| m.clip_for_animation(a).map(|i| m.clips[i].name.clone());
        assert_eq!(pick(&m, "#STAND").as_deref(), Some("STAND"));
        assert_eq!(pick(&m, "#DEATH").as_deref(), Some("DEATH"));
        assert_eq!(pick(&m, "#DEATHLIE3").as_deref(), Some("DEATHLIE1"));
        assert_eq!(pick(&m, "#MOTION_L2").as_deref(), Some("MOTION_L"));
        // Unknown animations stand rather than play the first clip (a dying walker).
        assert_eq!(pick(&m, "#ATTACK1").as_deref(), Some("STAND"));
        m.clips = named(&["WAVE"]);
        assert_eq!(pick(&m, "#ATTACK1").as_deref(), Some("WAVE"));
        m.clips.clear();
        assert_eq!(m.clip_for_animation("#STAND"), None);
    }

    #[test]
    fn corpses_decay_on_the_death_counter() {
        // docs/assets-humans.md: decay 0 up to 100, 0.15 at 265, 1 at the removal.
        assert_eq!(corpse_decay(0, 1200), 0.0);
        assert_eq!(corpse_decay(100, 1200), 0.0);
        assert!((corpse_decay(265, 1200) - 0.15).abs() < 1e-6);
        assert_eq!(corpse_decay(1200, 1200), 1.0);
        assert!((corpse_decay(1050, 2000) - 0.5).abs() < 1e-6);
        let (morphs, sink) = decay_pose(0.0, false);
        assert_eq!((morphs, sink), ([0.0, 0.0], 0.0));
        let (morphs, sink) = decay_pose(0.9, true);
        assert_eq!(morphs, [1.0, 1.0]);
        assert!((sink - 0.2 * (0.12 / 0.22)).abs() < 1e-6);
    }

    /// Load and bake time and memory of the look variants of a real model:
    /// `ACR_LOOKS_GLB=path cargo test -p acr-render --release -- --ignored measures_look_variants --nocapture`.
    #[test]
    #[ignore]
    fn measures_look_variants() {
        let Ok(path) = std::env::var("ACR_LOOKS_GLB") else { return };
        let bytes = std::fs::read(path).expect("glb");
        let t = std::time::Instant::now();
        let model = ModelData::from_glb(&bytes).expect("loads");
        let load = t.elapsed();
        let t = std::time::Instant::now();
        let baked = model.bake_looks().expect("looks");
        let bake = t.elapsed();
        let all: usize = model.primitives.iter().map(|p| p.vertices.len()).sum();
        let per = baked[0].len();
        println!(
            "load {load:?}, bake {bake:?}: {} variants x {per} of {all} vertices = {} KiB (base {} KiB)",
            baked.len(),
            baked.len() * per * size_of::<Vertex>() / 1024,
            all * size_of::<Vertex>() / 1024
        );
        assert!(model.looks.is_some());
    }

    #[test]
    fn rejects_broken_files() {
        assert!(ModelData::from_glb(b"not a model").is_err());
    }
}
