//! 3D models and the free camera in the game (#163, #164, docs/rendering-3d.md): which
//! objects are drawn as models (`models=` setting), their clip, facing and decay, the 3D
//! pass, and the free camera's keys (`camera=free`). The simulation is not touched: all of
//! it is read from the units the sprites are drawn from.

use std::collections::{HashMap, HashSet};

use acr_render::damage::{Damage, Keep, Limb};
use acr_render::fog_world::WorldFogRenderer;
use acr_render::model::{self, ModelData};

use acr_render::model_renderer::{BloodPool, DepthTarget, Drape, ModelInstance, ModelRenderer, Wounds};
use acr_render::objects_view::ObjectsView;
use acr_render::scene::{Placement, definition_name};
use acr_render::terrain::{Camera, TerrainData, TerrainRenderer};
use acr_render::view3d::{self, Orbit, PIXELS_PER_METRE, View3d};
use acr_render::water::Disturbances;
use acr_render::weather::Weather;
use acr_sim::World;
use acr_sim::world::{AnimKind, EntityId, Unit};
use winit::keyboard::{Key, NamedKey};

use super::debris::UnitModel;
use super::piles;
use crate::ui::settings::{CameraMode, Gore, Models, Settings};

/// The free camera's fog renderer and this frame's fog grid (`acr_sim::fog::Fog::cells`).
pub type WorldFog<'a> = (&'a mut WorldFogRenderer, Option<(i32, &'a [u16])>);

/// The fog grid moved from its projection onto the ground (#309): the sim stamps and the
/// original draws it at `(x, y − 2·h)`, the screen point of the ground, so cell `(cx, cy)`
/// here (centred on world `(128·(cx − 3), 128·(cy − 3))`) takes the grid's value at that
/// point's projection, interpolated along `y`. The free camera then looks the fog up by the
/// world `x`, `y` alone: a model or tree is as dark as the ground it stands on, from any side.
fn ground_fog(out: &mut Vec<u16>, width: i32, cells: &[u16], terrain: &TerrainData) {
    out.clear();
    if width <= 0 {
        return;
    }
    let rows = cells.len() as i32 / width;
    let at = |cx: i32, cy: i32| f32::from(cells[(cy.clamp(0, rows - 1) * width + cx) as usize]);
    for cy in 0..rows {
        for cx in 0..width {
            let (x, y) = (128 * (cx - 3), 128 * (cy - 3));
            let q = (y - 2 * terrain.height_at(x, y)) as f32 / 128.0 + 3.0;
            let (row, t) = (q.floor() as i32, q - q.floor());
            out.push((at(cx, row) + (at(cx, row + 1) - at(cx, row)) * t) as u16);
        }
    }
}

/// Where models are looked for: `<game folder>\ACR\Models\<name>.glb` (normalised game path),
/// `<name>` the `.MD` `NAME` in lower case (`EngM18` → `engm18.glb`).
pub const MODEL_DIR: &str = "ACR/MODELS/";
/// How far a shell burst throws the remains lying around it (world pixels, 4 m).
const BLAST_RADIUS: f32 = 120.0;
/// A cannonball grazing the ground (hit-locations rule, the earth it throws up,
/// `acr_sim::shrapnel::GRAZE`) throws the remains within 2.75 m: at most this many pieces
/// and corpses, at this share of a shell's force.
const GRAZE_RADIUS: f32 = 82.0;
const GRAZE_PIECES: usize = 10;
const GRAZE_CORPSES: usize = 4;
const GRAZE_FORCE: f32 = 0.65;

/// A blast throwing the remains around it: a shell's burst or a ball's graze.
#[derive(Debug, Clone, Copy)]
struct Blast {
    at: [f32; 2],
    radius: f32,
    pieces: usize,
    corpses: usize,
    force: f32,
}

impl Blast {
    fn shell(at: [f32; 2]) -> Self {
        Self {
            at,
            radius: BLAST_RADIUS,
            pieces: super::debris::BLAST_CAP,
            corpses: super::ragdoll_view::BLAST_CAP,
            force: 1.0,
        }
    }

    /// A mortar's smaller shell ([`super::particle_view::MORTAR_SCALE`] of the reach, the
    /// pieces and the corpses thrown).
    fn mortar(at: [f32; 2]) -> Self {
        let k = super::particle_view::MORTAR_SCALE;
        let n = |v: usize| ((v as f32 * k).round() as usize).max(1);
        let s = Self::shell(at);
        Self { radius: s.radius * k, pieces: n(s.pieces), corpses: n(s.corpses), ..s }
    }

    fn graze(at: [f32; 2]) -> Self {
        Self { at, radius: GRAZE_RADIUS, pieces: GRAZE_PIECES, corpses: GRAZE_CORPSES, force: GRAZE_FORCE }
    }
}
/// How long 3D corpses lie by default (minutes of game time, setting `corpse_minutes`).
const CORPSE_MINUTES: u64 = 5;
/// The death counter above which a sprite corpse turns half transparent (0x506ed5).
const CORPSE_FADE: u16 = 100;
/// Turn and tilt speed of the free camera's keys (radians per second).
const TURN_RATE: f32 = 1.6;
const TILT_RATE: f32 = 0.8;

/// Held gear of a unit model (`docs/assets-humans.md`: separate nodes on sockets).
#[derive(Debug, Clone, Default)]
struct Gear {
    /// The model without the musket, rammer and hat.
    unarmed: Option<usize>,
    /// The musket and hat alone, to lie on the ground.
    dropped: Vec<usize>,
    /// Per dropped item: its node in the unit model and how it lies.
    nodes: Vec<(usize, super::debris::Lying)>,
}

/// The `gore.glb` pack (#262): gibs, guts and the stump cap.
const GORE: &str = "GORE";
/// Culling of map objects drawn as models (`update_models`): the highest ground plus a tall
/// crown (true world pixels), and the margin for crowns and shadows reaching into the view.
const MAP_VIEW_TOP: f32 = 2000.0;
const MAP_VIEW_MARGIN: f32 = 600.0;
/// The wind (docs/rendering-3d.md "Wind"): world direction (unit; from the west-north-west),
/// strength (1: a fresh breeze, ~0.13 m of lean and rock at a 5 m crown) and the period the
/// shader's clock wraps at (seconds; f32 keeps its sines smooth well below it).
const WIND_DIR: [f32; 2] = [0.94, 0.34];
const WIND_STRENGTH: f32 = 1.0;
/// How fast smoke drifts in a wind of [`WIND_STRENGTH`] (world px a second: 1.8 m/s).
const SMOKE_DRIFT: f32 = 1.8 * PIXELS_PER_METRE;
const WIND_PERIOD: f32 = 3600.0;

/// The rain's strength and the lightning flash of `weather` at wind clock `clock`.
fn storm_at(weather: Weather, clock: f32) -> (f32, f32) {
    match weather {
        Weather::Rain => (acr_render::weather::rain_intensity(clock), acr_render::weather::flash(clock)),
        _ => (1.0, 0.0),
    }
}

/// Held gear a crawler, a disarmed or a killed man no longer carries: long weapons, a
/// rider's carbine and sabre, the hat and a peasant's work tools (each only while his clip
/// shows it, `ModelData::states`).
fn held_gear(node: &str) -> bool {
    let n = node.rsplit('.').next().unwrap_or(node);
    ["musket", "rammer", "pike", "gun", "sabre", "blade", "tricorn", "hat", "cap", "axe", "pick", "sickle", "hammer"]
        .contains(&n)
}

/// The pick clip of a peasant model (docs/families/civil.md).
const STONE_CLIP: &str = "ATTACK1_STONE";

/// The clip for a man breaking stone: the original plays one work animation for wood and
/// stone (`TAKERESSTAGES ... 2`, `#ATTACK1`; `gather.rs` `Attack(work - 1)`), the model
/// swings the pick in [`STONE_CLIP`] while his load (`carry_type`, set as the work starts) is
/// stone or gold, and holds it between blows in `PSTAND1_STONE`, `PATTACK1_STONE` and
/// `UATTACK1_STONE`. `None` keeps the `.MD` clip (wood, or a model without the variant).
fn stone_clip(model: &ModelData, world: &World, u: &Unit) -> Option<usize> {
    let name = match u.anim {
        AnimKind::Attack(1) => STONE_CLIP,
        AnimKind::PStand(1) => "PSTAND1_STONE",
        AnimKind::PAttack(1) => "PATTACK1_STONE",
        AnimKind::UAttack(1) => "UATTACK1_STONE",
        _ => return None,
    };
    let take = world.catalog.get(u.kind).economy.take_res.get(usize::from(u.carry_type?)).copied().flatten()?;
    let stone = matches!(take.credited, acr_sim::economy::STONE | acr_sim::economy::GOLD);
    if take.work == 2 && stone { model.clip(name) } else { None }
}

/// The death clip `clip` of `model` played with the tool `held` (his last clip alive) showed
/// still in the hand: `<clip>_<TOOL>` (`DEATH_AXE`, `DEATHLIE1_PICK`), where the model has it
/// (docs/families/civil.md).
fn held_death_clip(model: &ModelData, clip: usize, held: Option<usize>) -> Option<usize> {
    let held = model.clips.get(held?)?;
    let name = &model.clips.get(clip)?.name;
    model.states.iter().filter(|(_, names)| names.contains(&held.name)).find_map(|&(n, _)| {
        let tool = model.nodes[n].name.rsplit('.').next()?;
        model.clip(&format!("{name}_{}", tool.to_uppercase()))
    })
}

/// Whether gear node `n` of `model` is in his hands in `clip`: gear without clip states
/// always, a tool only in the clips its `acr_state` names.
fn holds(model: &ModelData, n: usize, clip: Option<usize>) -> bool {
    model
        .node_states(n)
        .is_none_or(|names| clip.and_then(|c| model.clips.get(c)).is_some_and(|c| names.contains(&c.name)))
}

/// Clearance of dropped gear from the body's axis (hips to head) of its dead owner (m).
const GEAR_CLEAR_M: f32 = 0.3;

/// Where a killed man's held gear lies once he is down (#206): per visible item its model,
/// its origin on the ground (world x, y) and its yaw. Each item starts where it is in his
/// landing pose (the clip's, or under a ragdoll its socket's bone from `bone`) and turned as
/// it is; a long one (musket, carbine, sabre) turns to lie alongside the body, a little off
/// by a hash of (unit, item), and everything is pushed out of the body to lie beside him.
fn laid_gear(
    model: &ModelData,
    gear: &Gear,
    inst: &ModelInstance,
    bone: &dyn Fn(&str) -> Option<[f32; 3]>,
    id: u32,
    tick: u64,
    held: Option<usize>,
) -> Vec<(usize, [f32; 2], f32)> {
    use acr_render::math;
    let globals = model.globals(inst.clip, inst.time);
    let world = |n: usize| math::mul(&inst.transform, &globals[n]);
    let node = |name: &str| {
        let rider = format!("rider.{name}");
        model.nodes.iter().position(|n| n.name == name).or_else(|| model.nodes.iter().position(|n| n.name == rider))
    };
    let short = |name: &str| name.rsplit('.').next().unwrap_or(name).to_owned();
    // A ragdoll's bone, else the clip's (world pixels).
    let point = |name: &str| -> Option<[f32; 2]> {
        if let Some(p) = bone(name) {
            return Some([p[0], p[1]]);
        }
        let p = math::transform_point(&world(node(name)?), [0.0; 3]);
        Some([p[0], p[1]])
    };
    let (Some(hips), Some(head)) = (point("hips"), point("head")) else { return Vec::new() };
    let body = [head[0] - hips[0], head[1] - hips[1]];
    let len = (body[0] * body[0] + body[1] * body[1]).sqrt().max(1e-3);
    let axis = [body[0] / len, body[1] / len];
    let ragdoll = bone("hips").is_some();
    let mut out = Vec::new();
    for (k, (&model_index, &(n, lying))) in gear.dropped.iter().zip(&gear.nodes).enumerate() {
        let m = world(n);
        // A hidden item (scaled to nothing by its clip) does not fall; a tool falls if he held
        // it in his last clip alive (`held`), from where the death clip has his hand.
        let x = [m[0][0], m[0][1], m[0][2]];
        let tool = model.node_states(n).is_some();
        if if tool { !holds(model, n, held) } else { math::dot(x, x) < 1e-6 } {
            continue;
        }
        let mut rng = super::splatter::Rng::new(tick, id ^ (k as u32).wrapping_mul(0x9e37) ^ 0x6ea2);
        // Its direction as it was (the clip's), a hashed one under a ragdoll or when upright.
        let o = math::transform_point(&m, [0.0; 3]);
        let tip = math::transform_point(&m, lying.axis);
        let mut d = [tip[0] - o[0], tip[1] - o[1]];
        let full = math::dot(math::sub(tip, o), math::sub(tip, o)).sqrt();
        let dl = (d[0] * d[0] + d[1] * d[1]).sqrt();
        if ragdoll || dl < 0.3 * full || full < 1e-6 {
            let a = rng.next() * std::f32::consts::TAU;
            d = [a.cos(), a.sin()];
        } else {
            d = [d[0] / dl, d[1] / dl];
        }
        // Its middle where it was: under a ragdoll, the bone its socket hangs on.
        let socket = model.nodes[n].parent.and_then(|p| model.nodes[p].parent).map(|p| short(&model.nodes[p].name));
        let held = socket.filter(|_| ragdoll).and_then(|s| bone(&s));
        let mut c = match held {
            Some(p) => [p[0], p[1]],
            None => [o[0] + d[0] * lying.mid * PIXELS_PER_METRE, o[1] + d[1] * lying.mid * PIXELS_PER_METRE],
        };
        let long = lying.half > 0.3;
        let mut sin = 1.0;
        if long {
            // Alongside the body, up to 0.35 rad off.
            let sign = if d[0] * axis[0] + d[1] * axis[1] < 0.0 { -1.0 } else { 1.0 };
            let turn = (rng.next() - 0.5) * 0.7;
            let (s, co) = turn.sin_cos();
            let a = [axis[0] * sign, axis[1] * sign];
            d = [a[0] * co - a[1] * s, a[0] * s + a[1] * co];
            sin = s.abs();
        }
        // Out of the body: from the nearest point of the hips-head segment, sideways.
        let t = (((c[0] - hips[0]) * axis[0] + (c[1] - hips[1]) * axis[1]) / len).clamp(0.0, 1.0);
        let q = [hips[0] + body[0] * t, hips[1] + body[1] * t];
        let (mut nx, mut ny) = (c[0] - q[0], c[1] - q[1]);
        let off = (nx * nx + ny * ny).sqrt();
        if off < 1e-3 {
            let side = if rng.next() < 0.5 { -1.0 } else { 1.0 };
            (nx, ny) = (-axis[1] * side, axis[0] * side);
        } else {
            (nx, ny) = (nx / off, ny / off);
        }
        let reach = if long { lying.half * sin } else { lying.half };
        let clear = (GEAR_CLEAR_M + reach + 0.1 * rng.next()) * PIXELS_PER_METRE;
        if off < clear {
            c = [q[0] + nx * clear, q[1] + ny * clear];
        }
        // glTF x lies along `d` (x turns to world (cos yaw, -sin yaw)).
        let yaw = (-d[1]).atan2(d[0]);
        let mid = lying.mid * PIXELS_PER_METRE;
        out.push((model_index, [c[0] - d[0] * mid, c[1] - d[1] * mid], yaw));
    }
    out
}

/// Gear that lies on the ground once dropped (the rammer stays in the musket).
fn lies_on_ground(node: &str) -> bool {
    held_gear(node) && !node.ends_with("rammer")
}

#[derive(Default)]
struct HeldKeys {
    left: bool,
    right: bool,
    up: bool,
    down: bool,
}

/// Building pieces prepared on a worker: the model, its centre and radius (#303).
#[cfg(not(target_arch = "wasm32"))]
type ChunkList = Vec<(acr_render::model_renderer::PreparedModel, acr_render::math::Vec3, f32)>;

/// Milliseconds a frame for uploading prepared building pieces (#303).
#[cfg(not(target_arch = "wasm32"))]
const UPLOAD_MS: f32 = 3.0;

/// A building's pieces on the GPU: the model index, its centre and radius (#303).
type Pieces = Vec<(usize, acr_render::math::Vec3, f32)>;

pub struct ThreeD {
    /// Severed pieces, gibs and dropped gear (#262 S5).
    debris: super::debris::Debris,
    /// The parts of `gore.glb` (gibs, guts) by node name.
    gibs: super::debris::GibModels,
    /// Per unit model: the model without held gear (crawlers, the disarmed) and the gear
    /// that then lies on the ground.
    gear: HashMap<usize, Gear>,
    /// Blood trails of the bleeding (render only, #254, #262).
    trail: super::trail::Trails,
    /// Wounds where the hits struck, on the living and their corpses (#277).
    wounds: super::living_wounds::LivingWounds,
    /// Charring forced by the driver's `burn` (#207), by unit id; over `fire`'s.
    pub(crate) burns: super::gore::Burns,
    /// The last tick drops were added for (frames redraw a tick several times).
    trail_tick: u64,
    /// The tick of the last update: going back means another game, the debris is dropped.
    debris_tick: u64,
    /// `battlefield_marks=keep`: the marks that stay for the whole battle.
    marks_keep: bool,
    kept: super::kept_marks::KeptMarks,
    /// `decay_stain=on`: the fluids of decaying corpses stain the ground (`seep.rs`).
    stain: bool,
    seeps: super::seep::Seeps,
    models: Models,
    gore: Gore,
    /// Sun shadows of the models (`shadows` setting; [`set_shadows_override`] wins).
    shadows: bool,
    /// `lod=on|off` (docs/rendering-3d.md, "Levels of detail").
    lod: bool,
    /// `material_detail=on|off` (docs/rendering-3d.md, "Material detail"; `ACR_MATERIAL_DETAIL`
    /// wins, native only).
    material_detail: bool,
    /// `wind=on|off` (docs/rendering-3d.md, "Wind"); the driver's `props` request wins.
    wind: bool,
    wind_override: Option<bool>,
    /// The wind's clock: frame seconds summed (wraps at [`WIND_PERIOD`]).
    wind_clock: f32,
    /// Trees and foliage placed by the driver (`props`, render only): model `NAME` and
    /// ground points (world pixels). Map trees are still sprites.
    props: Vec<(String, [i32; 2])>,
    /// The map's trees and stones drawn as models (`map_models=on|off`, docs/rendering-3d.md
    /// "Map trees and stones").
    map_models: super::map_models::MapModels,
    /// Rest heights (metres) of the models placed for map objects, by model index.
    map_heights: HashMap<usize, f32>,
    /// The world rectangle `[x0, y0, x1, y1]` this frame can show map objects in (set by
    /// `update_models`; `None`: all).
    pub(super) map_view: Option<[f32; 4]>,
    /// The water surface (`water=detailed`, docs/water.md); `ACR_WATER` wins.
    water_detailed: bool,
    water: Option<acr_render::water::WaterRenderer>,
    /// Ships riding the waves, their wakes, and shot falling into the water (render only).
    water_view: super::water_view::WaterView,
    /// Earth, dust, mud and water thrown into the air (docs/ground.md phase 3).
    particles: super::particle_view::ParticleView,
    /// Shell fires and burning men (#256): flames, smoke, scorch; the burn amount.
    fire: super::fire_view::FireView,
    /// The rounds in flight and the arrows stuck in the ground (#308).
    missiles: super::missile_view::Missiles,
    particle_renderer: Option<acr_render::particles::ParticleRenderer>,
    /// Grass tufts, flowers, ferns, litter and pebbles on the detailed ground
    /// (`ground_cover=on|off`, docs/ground.md "Phase 5: ground cover"); `ACR_GROUND_COVER`
    /// wins. `cover_view` is this frame's view, set before `update`.
    ground_cover: bool,
    cover: acr_render::ground_cover::GroundCover,
    cover_renderer: Option<acr_render::ground_cover::GroundCoverRenderer>,
    pub(super) cover_view: Option<View3d>,
    /// The frame shows a flat backdrop instead of the ground (`InGame::backdrop`: the blank
    /// field's `white` and `greenscreen`): no cover there, whatever the tile class says.
    pub(super) backdrop: bool,
    /// The driver's `ground_cover` (`blank`, `studio`) over the setting and the backdrop.
    pub(super) cover_forced: Option<bool>,
    /// Rain, snow or fog (#224, `weather` setting; the driver's `weather` overrides it).
    weather: Weather,
    weather_override: Option<Weather>,
    weather_renderer: Option<acr_render::weather::WeatherRenderer>,
    /// Where balls fell into the water this frame (a splash each).
    shots: Vec<[f32; 2]>,
    /// How long 3D corpses lie (ticks, `corpse_minutes`).
    corpse_ticks: u64,
    /// Shell bursts up to this tick have thrown the remains around them, and this frame's
    /// new bursts (world pixels) for the ragdolls.
    last_blast: u64,
    last_graze: u64,
    blasts: Vec<Blast>,
    free: bool,
    pub orbit: Orbit,
    keys: HeldKeys,
    /// Model files by `.MD` `NAME` (upper case) → game path.
    files: HashMap<String, String>,
    /// Requested model files: game path (normalised) → `NAME`.
    requested: HashMap<String, String>,
    /// Parsed models waiting for the GPU.
    parsed: Vec<(String, ModelData)>,
    /// Uploaded models by `NAME`; `None`: the file could not be used.
    loaded: HashMap<String, Option<usize>>,
    renderer: Option<ModelRenderer>,
    depth: Option<DepthTarget>,
    /// The fog grid on the ground for the free camera's world fog ([`ground_fog`]).
    fog_cells: Vec<u16>,
    instances: Vec<ModelInstance>,
    /// Blood pools under this frame's corpses and splatter behind hit units.
    pools: Vec<BloodPool>,
    /// Units drawn as models this frame (their sprites are left out).
    shown: HashSet<EntityId>,
    /// Building models with a ruin state group (#303): the ruin's model by the standing one.
    ruins: HashMap<usize, usize>,
    /// Map trees and rocks with struck state groups (rule `ball_terrain`), by the standing model.
    struck_parts: HashMap<usize, StruckParts>,
    /// Building models with construction stage or section groups (#302), by the finished one.
    states: HashMap<usize, BuildingStates>,
    /// Fire anchors of building models (#303, `ruin_view`), by model.
    anchors: HashMap<usize, super::ruin_view::Anchors>,
    /// Burning and fallen buildings this frame, for their fire, smoke and dust.
    burning: Vec<super::ruin_view::Burning>,
    /// The pieces that break off falling building models (#303, `ruin_view::chunks`): model,
    /// centre and radius of each, by the building's model; made on its first collapse.
    chunks: HashMap<usize, Pieces>,
    /// Building models whose pieces `sync_gpu` makes next.
    /// Building models whose pieces are wanted, with the seconds since the death of a fallen
    /// one (`None`: heavily damaged, wanted ahead).
    chunks_wanted: HashMap<usize, Option<f32>>,
    /// The broken pieces being cut and prepared on a worker (native).
    #[cfg(not(target_arch = "wasm32"))]
    chunk_jobs: HashMap<usize, std::sync::mpsc::Receiver<ChunkList>>,
    /// A finished job's pieces going onto the GPU, one a frame: the building model, those
    /// still to upload and those done.
    #[cfg(not(target_arch = "wasm32"))]
    chunk_uploads: Option<(usize, ChunkList, Pieces)>,
    /// Buildings falling in as models this frame (#303): map x, y, the reach of their
    /// `DESTRUCT` explosions (pixels) and their type, for [`Self::veils`].
    veils: Vec<(i32, i32, i32, acr_sim::TypeId)>,
    /// Hit damage on buildings (#304): scars by the building's instance index this frame,
    /// and the stuck arrows' model.
    building_hits: super::building_hits::BuildingHits,
    scars: HashMap<usize, acr_render::model_renderer::Scars>,
    arrow_model: Option<usize>,
    /// The rubble knocked out of holes: stone blocks and boards (`procedural_rubble`).
    rubble_models: Option<[usize; 2]>,
    /// Fire arrows burning on buildings this frame (world pixels, strength).
    arrow_fires: Vec<([f32; 3], f32)>,
    /// Frames and time since the last performance line in the log.
    frames: u32,
    since: Option<web_time::Instant>,
    /// Time spent stacking corpses (µs) and stackings since the last log line.
    pile_us: u128,
    pile_runs: u32,
    /// The rests of the corpses on piles, stacked again only when they change (#230).
    piles: piles::Piles,
    /// The terrain under each corpse's footprint samples, by unit id, with the ground point
    /// it was sampled at (sampling every corpse every frame costs more than the stacking).
    pile_ground: HashMap<u32, ([f32; 2], [f32; 8])>,
    /// `ACR_3D_COMPARE` (development): compare models with their sprites.
    compare: Compare,
    started: web_time::Instant,
    /// Physics ragdolls of the hit and the dying (#278), made at the first frame with models.
    ragdolls: Option<super::ragdoll_view::Ragdolls>,
    /// Gun crews: losses, their corpses and uniforms (#287).
    crews: super::crew_view::Crews,
    /// Animals standing over the men they maul (#276).
    maulers: super::maul_view::Maulers,
    /// The coat skirts of the men near the camera (runtime cloth, docs/clothing.md).
    skirts: super::cloth_view::Skirts,
    /// Carried gear lagging behind the men near the camera (docs/assets-humans.md, "Lag").
    carried: super::gear_view::Gear,
    /// Riders' horses turned for a blow, eased (`strike_view::Turns`).
    turns: super::strike_view::Turns,
    /// Per living man whose model shows tools by clip (`ModelData::states`): his clip this
    /// frame, so a killed or crawling man drops the tool he held (unit id).
    tool_clips: HashMap<u32, Option<usize>>,
    /// Seconds of the last frame and the world point at the window centre.
    frame_seconds: f32,
    pub focus: [f32; 2],
    /// The detailed ground (docs/ground.md): `ground=detailed`, its texture files (game
    /// paths in `acr_render::ground::texture_files` order), their contents as they come in
    /// and the decoded set waiting for the renderer.
    pub ground_detailed: bool,
    /// The detailed ground's state draws craters and furrows: no flat dirt decals then.
    pub ground_marks: bool,
    ground_files: Vec<Option<String>>,
    ground_data: Vec<Option<Vec<u8>>>,
    ground_requested: bool,
    ground_ready: Option<acr_render::ground::MaterialImages>,
    /// The driver's studio (#299): clip and time held per unit (`pose`), and this frame's
    /// instance index of each unit drawn as a model.
    pub held: HashMap<u32, (usize, f32)>,
    drawn: HashMap<u32, usize>,
    /// This frame's view: `update` leaves out the plain living well off it ([`off_view`]).
    cull_view: Option<View3d>,
}

/// `ACR_3D_COMPARE=overlay`: the sprites of units with a model are drawn half transparent
/// over the model; `toggle`: every 2 seconds sprites and models swap (pause the game to
/// compare one moment).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Compare {
    None,
    Overlay,
    Toggle,
}

impl Compare {
    fn from_env() -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        match std::env::var("ACR_3D_COMPARE").as_deref() {
            Ok("overlay") => return Self::Overlay,
            Ok("toggle") => return Self::Toggle,
            _ => {}
        }
        Self::None
    }
}

impl Default for ThreeD {
    fn default() -> Self {
        Self {
            debris: Default::default(),
            gibs: Default::default(),
            gear: HashMap::new(),
            models: Models::Off,
            gore: Gore::Full,
            shadows: true,
            lod: true,
            material_detail: true,
            wind: true,
            wind_override: None,
            wind_clock: 0.0,
            props: Vec::new(),
            map_models: super::map_models::MapModels::new(),
            map_heights: HashMap::new(),
            map_view: None,
            water_detailed: true,
            water: None,
            water_view: Default::default(),
            particles: Default::default(),
            fire: Default::default(),
            missiles: Default::default(),
            particle_renderer: None,
            ground_cover: true,
            cover: Default::default(),
            cover_renderer: None,
            cover_view: None,
            backdrop: false,
            cover_forced: None,
            weather: Weather::Off,
            weather_override: None,
            weather_renderer: None,
            shots: Vec::new(),
            last_blast: 0,
            last_graze: 0,
            blasts: Vec::new(),
            corpse_ticks: CORPSE_MINUTES * 60 * acr_sim::ORIGINAL_TICKS_PER_SECOND as u64,
            stain: true,
            seeps: Default::default(),
            free: false,
            orbit: Orbit::ORIGINAL,
            keys: HeldKeys::default(),
            files: HashMap::new(),
            requested: HashMap::new(),
            parsed: Vec::new(),
            loaded: HashMap::new(),
            renderer: None,
            depth: None,
            fog_cells: Vec::new(),
            instances: Vec::new(),
            pools: Vec::new(),
            trail: super::trail::Trails::default(),
            wounds: Default::default(),
            burns: Default::default(),
            trail_tick: u64::MAX,
            debris_tick: 0,
            marks_keep: false,
            kept: Default::default(),
            shown: HashSet::new(),
            ruins: HashMap::new(),
            struck_parts: HashMap::new(),
            states: HashMap::new(),
            anchors: HashMap::new(),
            burning: Vec::new(),
            chunks: HashMap::new(),
            chunks_wanted: HashMap::new(),
            #[cfg(not(target_arch = "wasm32"))]
            chunk_jobs: HashMap::new(),
            #[cfg(not(target_arch = "wasm32"))]
            chunk_uploads: None,
            veils: Vec::new(),
            building_hits: Default::default(),
            scars: HashMap::new(),
            arrow_model: None,
            rubble_models: None,
            arrow_fires: Vec::new(),
            frames: 0,
            since: None,
            pile_us: 0,
            pile_runs: 0,
            piles: piles::Piles::default(),
            pile_ground: HashMap::new(),
            compare: Compare::from_env(),
            started: web_time::Instant::now(),
            ragdolls: None,
            crews: Default::default(),
            maulers: Default::default(),
            skirts: Default::default(),
            carried: Default::default(),
            turns: Default::default(),
            tool_clips: HashMap::new(),
            frame_seconds: 0.0,
            focus: [0.0; 2],
            ground_detailed: true,
            ground_marks: false,
            ground_files: Vec::new(),
            ground_data: Vec::new(),
            ground_requested: false,
            ground_ready: None,
            held: HashMap::new(),
            drawn: HashMap::new(),
            cull_view: None,
        }
    }
}

impl ThreeD {
    /// `decay_stain=on|off` (`apply` sets it from the settings).
    pub fn set_decay_stain(&mut self, on: bool) {
        self.stain = on;
        if !on {
            self.seeps.clear();
        }
    }

    pub fn apply(&mut self, settings: &Settings) {
        self.models = settings.models;
        self.gore = settings.gore;
        self.shadows = settings.shadows;
        self.lod = settings.lod;
        self.material_detail = settings.material_detail;
        self.wind = settings.wind;
        self.weather = settings.weather;
        self.map_models.on = settings.map_models;
        self.water_detailed = settings.water_detailed;
        self.corpse_ticks = u64::from(settings.corpse_minutes) * 60 * acr_sim::ORIGINAL_TICKS_PER_SECOND as u64;
        self.free = settings.camera == CameraMode::Free;
        self.ground_detailed = settings.ground_detailed;
        self.ground_cover = settings.ground_cover;
        if self.marks_keep && !settings.marks_keep {
            // Back to fading: what was kept goes, the world's recent marks show again.
            self.kept.clear();
        }
        self.marks_keep = settings.marks_keep;
        self.set_decay_stain(settings.decay_stain());
        model::set_decay_cap(if settings.corpse_decay_half { model::HALF_DECAY } else { 1.0 });
        log::info!(
            "3d: models {:?}, camera {:?}, gore {:?}, wind {}, map models {}",
            settings.models,
            settings.camera,
            settings.gore,
            settings.wind,
            settings.map_models
        );
    }

    /// Switches to the free camera with this turn and tilt (the driver's `camera`). The
    /// tilt may go below the keys' limit, down to a near side view.
    pub fn set_free(&mut self, orbit: Orbit) {
        self.free = true;
        self.orbit = Orbit {
            yaw: orbit.yaw.rem_euclid(std::f32::consts::TAU),
            pitch: orbit.pitch.clamp(0.05, Orbit::MAX_PITCH),
        };
    }

    /// The free camera on or off for this battle (the modern controls' Ctrl + Space, C); true
    /// if it is on now. The turn and tilt stay from the last time; the view keeps its centre
    /// either way, since the free camera turns about the ground point there.
    pub fn toggle_free(&mut self) -> bool {
        self.free = !self.free;
        self.release_keys();
        self.free
    }

    /// The model and this frame's instance of a unit drawn as a model (the studio, #299).
    pub fn posed(&self, unit: u32) -> Option<(&acr_render::model::ModelData, &ModelInstance)> {
        let inst = self.instances.get(*self.drawn.get(&unit)?)?;
        Some((self.renderer.as_ref()?.model(inst.model)?, inst))
    }

    /// The palette the gait or a ragdoll gave a unit's model this frame (still waiting for the
    /// draw, as in the headless driver), else the one it was last drawn with (the studio's
    /// foot measurements, #279).
    pub fn shown_pose(&self, unit: u32) -> Option<&acr_render::model::Palette> {
        let i = *self.drawn.get(&unit)?;
        self.ragdolls.as_ref().and_then(|r| r.poses.get(&i)).or_else(|| self.renderer.as_ref()?.shown_pose(i))
    }

    /// Finds the model files among the game's paths.
    pub fn start(&mut self, paths: &[String]) {
        for path in paths {
            let norm = acr_formats::normalize_path(path);
            if let Some(file) = norm.strip_prefix(MODEL_DIR).filter(|f| f.ends_with(".GLB") && !f.contains('/')) {
                self.files.insert(file.trim_end_matches(".GLB").to_owned(), path.clone());
            }
        }
        let wanted = acr_render::ground::texture_files();
        self.ground_files = wanted
            .iter()
            .map(|w| {
                let w = acr_formats::normalize_path(w);
                paths.iter().find(|p| acr_formats::normalize_path(p) == w).cloned()
            })
            .collect();
        self.ground_data = vec![None; wanted.len()];
        if !self.files.is_empty() {
            let mut names: Vec<&String> = self.files.keys().collect();
            names.sort();
            log::info!("3d: models found for {names:?}");
        }
    }

    /// The 3D pass replaces the plain sprite pass.
    pub fn active(&self) -> bool {
        self.models != Models::Off || self.free
    }

    /// The free camera's view of this frame, `None` with the fixed camera.
    pub fn free_view(&self, camera: Camera, size: [f32; 2]) -> Option<View3d> {
        self.free.then_some(View3d { camera, size, orbit: Some(self.orbit) })
    }

    pub fn orbit(&self) -> Option<Orbit> {
        self.free.then_some(self.orbit)
    }

    /// Whether `unit` is drawn as a model this frame.
    pub fn shows(&self, unit: EntityId) -> bool {
        self.compare != Compare::Overlay && self.models_phase() && self.shown.contains(&unit)
    }

    /// `ACR_3D_COMPARE=overlay`: the unit's sprite is drawn half transparent over its model.
    /// The rounds in flight are drawn as models (#308): their sprites are left out.
    pub fn draws_rounds(&self) -> bool {
        self.models != Models::Off && self.missiles.ready()
    }

    /// Whether `p` is a `DESTRUCT` explosion of a building falling in as a model (#303): its
    /// sprite is drawn thin and cut short, so the collapse shows. Sprite buildings and the
    /// original look (models off) keep the whole explosion.
    pub fn veils(&self, world: &World, p: &acr_sim::projectile::Projectile) -> bool {
        let (x, y) = (p.pos.0 >> 14, p.pos.1 >> 14);
        self.veils.iter().any(|&(vx, vy, r, kind)| {
            (x - vx).abs() <= r && (y - vy).abs() <= r && world.catalog.get(kind).destruct.ids.contains(&Some(p.weapon))
        })
    }

    pub fn ghost(&self, unit: EntityId) -> bool {
        self.compare == Compare::Overlay && self.shown.contains(&unit)
    }

    /// `ACR_3D_COMPARE=toggle`: whether the models are shown at the moment.
    fn models_phase(&self) -> bool {
        self.compare != Compare::Toggle || (self.started.elapsed().as_secs() / 2).is_multiple_of(2)
    }

    /// Asks for the detailed ground's textures once 3D mode shows it.
    fn request_ground(&mut self, request: &dyn Fn(&str)) {
        if self.ground_requested || !self.ground_detailed || !self.active() {
            return;
        }
        self.ground_requested = true;
        if self.ground_files.is_empty() || self.ground_files.iter().any(Option::is_none) {
            log::info!("3d: no textures for the detailed ground (ACR\\Models\\ground), the original stays");
            return;
        }
        for path in self.ground_files.iter().flatten() {
            request(path);
        }
    }

    /// The decoded ground textures, once (for `TerrainRenderer::set_materials`).
    pub fn take_ground(&mut self) -> Option<acr_render::ground::MaterialImages> {
        self.ground_ready.take()
    }

    /// Takes a model file this layer asked for.
    pub fn on_loaded(&mut self, path: &str, result: &Result<Vec<u8>, String>) -> bool {
        let norm = acr_formats::normalize_path(path);
        if let Some(i) =
            self.ground_files.iter().position(|f| f.as_deref().map(acr_formats::normalize_path) == Some(norm.clone()))
        {
            match result {
                Ok(bytes) => self.ground_data[i] = Some(bytes.clone()),
                Err(e) => log::warn!("3d: {path}: {e}"),
            }
            if self.ground_data.iter().all(Option::is_some) {
                let files: Vec<Vec<u8>> = self.ground_data.iter_mut().map(|d| d.take().unwrap_or_default()).collect();
                // The browser takes half the resolution (a quarter of the memory).
                let skip = usize::from(cfg!(target_arch = "wasm32"));
                let started = web_time::Instant::now();
                match acr_render::ground::MaterialImages::decode(&files, skip) {
                    Ok(images) => {
                        log::info!("3d: ground textures {0} x {0} decoded in {1:?}", images.size, started.elapsed());
                        self.ground_ready = Some(images);
                    }
                    Err(e) => log::warn!("3d: ground textures: {e}"),
                }
            }
            return true;
        }
        let Some(name) = self.requested.remove(&acr_formats::normalize_path(path)) else { return false };
        let started = web_time::Instant::now();
        match result.as_ref().map_err(String::clone).and_then(|b| ModelData::from_glb(b)) {
            Ok(model) => {
                log::info!(
                    "3d: {path}: parsed in {:?}, {} triangles, radius {:.1} m, {} clips {:?}",
                    started.elapsed(),
                    model.triangle_count(),
                    model.radius / PIXELS_PER_METRE,
                    model.clips.len(),
                    model.clips.iter().map(|c| c.name.as_str()).collect::<Vec<_>>()
                );
                self.parsed.push((name, model));
            }
            Err(e) => {
                log::warn!("3d: {path}: {e}");
                self.loaded.insert(name, None);
            }
        }
        true
    }

    pub fn sync_gpu(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) {
        if !self.active() {
            return;
        }
        let renderer = self.renderer.get_or_insert_with(|| ModelRenderer::new(device, queue, format));
        if self.arrow_model.is_none() {
            self.arrow_model = Some(renderer.add_model(device, queue, acr_render::damage::procedural_arrow()));
        }
        if self.rubble_models.is_none() {
            self.rubble_models = Some(
                [false, true]
                    .map(|wood| renderer.add_model(device, queue, acr_render::damage::procedural_rubble(wood))),
            );
        }
        self.depth.get_or_insert_with(|| DepthTarget::new(device, format));
        self.water.get_or_insert_with(|| acr_render::water::WaterRenderer::new(device, format));
        self.particle_renderer.get_or_insert_with(|| acr_render::particles::ParticleRenderer::new(device, format));
        self.weather_renderer.get_or_insert_with(|| acr_render::weather::WeatherRenderer::new(device, format));
        self.cover_renderer.get_or_insert_with(|| acr_render::ground_cover::GroundCoverRenderer::new(device, format));
        self.missiles.upload(renderer, device, queue);
        // The pieces of buildings that began to fall in (#303): broken ones cut on a worker as
        // soon as a building is heavily damaged (native), the plain ones at once where those
        // are not ready when it falls (and in the browser).
        for (index, dead) in std::mem::take(&mut self.chunks_wanted) {
            if self.chunks.contains_key(&index) {
                continue;
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                let started = self.chunk_jobs.contains_key(&index)
                    || self.chunk_uploads.as_ref().is_some_and(|(i, _, _)| *i == index);
                if !started && let Some(model) = renderer.model(index) {
                    let model = model.clone();
                    let (tx, rx) = std::sync::mpsc::channel();
                    std::thread::spawn(move || {
                        let started = web_time::Instant::now();
                        let pieces = super::ruin_view::chunks(&model, true);
                        let triangles: usize =
                            pieces.iter().flat_map(|c| &c.model.primitives).map(|p| p.indices.len() / 3).sum();
                        let list: ChunkList = pieces
                            .into_iter()
                            .map(|c| (ModelRenderer::prepare_model(c.model), c.centre, c.radius))
                            .collect();
                        log::info!(
                            "3d: {} broken building pieces, {triangles} triangles, on a worker in {:?}",
                            list.len(),
                            started.elapsed()
                        );
                        let _ = tx.send(list);
                    });
                    self.chunk_jobs.insert(index, rx);
                }
                // Wait for the worker until the first piece would break off.
                if dead.is_none_or(|t| t < super::ruin_view::BREAK) {
                    if dead.is_some() {
                        self.chunks_wanted.insert(index, dead);
                    }
                    continue;
                }
                self.chunk_jobs.remove(&index);
                if self.chunk_uploads.as_ref().is_some_and(|(i, _, _)| *i == index) {
                    self.chunk_uploads = None;
                }
            }
            if dead.is_none() {
                continue;
            }
            let Some(model) = renderer.model(index) else { continue };
            let started = web_time::Instant::now();
            let pieces = super::ruin_view::chunks(model, false);
            let list: Vec<_> =
                pieces.into_iter().map(|c| (renderer.add_model(device, queue, c.model), c.centre, c.radius)).collect();
            log::info!("3d: {} plain building pieces in {:?}", list.len(), started.elapsed());
            self.chunks.insert(index, list);
        }
        // A finished worker's pieces onto the GPU, as many a frame as fit in `UPLOAD_MS` (at
        // least one; a whole building's at once took up to 40 ms).
        #[cfg(not(target_arch = "wasm32"))]
        {
            if self.chunk_uploads.is_none() {
                let mut done = None;
                for (&index, rx) in &self.chunk_jobs {
                    match rx.try_recv() {
                        Ok(list) => done = Some((index, Some(list))),
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => done = Some((index, None)),
                        Err(std::sync::mpsc::TryRecvError::Empty) => continue,
                    }
                    break;
                }
                if let Some((index, list)) = done {
                    self.chunk_jobs.remove(&index);
                    if let Some(mut list) = list {
                        list.reverse();
                        self.chunk_uploads = Some((index, list, Vec::new()));
                    }
                }
            }
            if let Some((index, mut list, mut done)) = self.chunk_uploads.take() {
                let started = web_time::Instant::now();
                while started.elapsed().as_secs_f32() * 1000.0 < UPLOAD_MS
                    && let Some((m, centre, radius)) = list.pop()
                {
                    done.push((renderer.add_prepared(device, queue, m), centre, radius));
                }
                if list.is_empty() {
                    self.chunks.entry(index).or_insert_with(|| {
                        log::info!("3d: {} broken building pieces uploaded", done.len());
                        done
                    });
                } else {
                    self.chunk_uploads = Some((index, list, done));
                }
            }
        }
        for (name, model) in self.parsed.drain(..) {
            // Upload time per model: textures go up here (KTX2 ones transcoded first, #297).
            let started = web_time::Instant::now();
            if name == GORE {
                // The pack is split into its nodes; the torn cap replaces the procedural one
                // (one cap for all cuts: `stump_cap_clean` is not used yet).
                for node in model.mesh_nodes() {
                    let Some(mut part) = model.node_part(node) else { continue };
                    // Shreds of uniform take the victim's coat colour and crumple (#280).
                    for m in part.materials.iter_mut().filter(|m| m.name.contains("cloth")) {
                        m.tissue = acr_render::model::Tissue::Cloth;
                    }
                    if node == "stump_cap" {
                        renderer.set_cap(device, queue, part);
                    } else {
                        self.gibs.insert(node.to_owned(), renderer.add_model(device, queue, part));
                    }
                }
                log::info!("3d: {name}: uploaded in {:?}", started.elapsed());
                self.loaded.insert(name, Some(usize::MAX));
                continue;
            }
            let mut gear = Gear::default();
            // Men of skeleton H and riders (`rider.*` bones).
            if model.rig.is_some() || model.nodes.iter().any(|n| n.name == "rider.hips") {
                let unarmed = model.without_nodes(held_gear);
                if unarmed.primitives.len() < model.primitives.len() {
                    for node in model.mesh_nodes().into_iter().filter(|n| lies_on_ground(n)) {
                        if let Some(part) = model.node_part(node)
                            && let Some(n) = model.nodes.iter().position(|m| m.name == node)
                        {
                            let (part, lying) = super::debris::lying_flat(part);
                            gear.dropped.push(renderer.add_model(device, queue, part));
                            gear.nodes.push((n, lying));
                        }
                    }
                    gear.unarmed = Some(renderer.add_model(device, queue, unarmed));
                }
            }
            // A tree drawn standing: its felled and stump states, splinter scar and chip
            // marks are left out (`acr_state` groups, docs/assets.md "States" and "Struck map
            // objects"); a map model's struck states become models of their own.
            let map_model = self.map_models.names().contains(name.as_str());
            let mut struck_parts = Vec::new();
            if map_model {
                for key in STRUCK_GROUPS {
                    let part = model.only_groups(|n| n.contains(key));
                    struck_parts.push((!part.primitives.is_empty()).then_some(part));
                }
                // A rock's chip marks, one per ball heading (`<name>.chip.<k>`).
                for k in 0..super::map_models::CHIP_SIDES {
                    let end = format!(".chip.{k}");
                    let part = model.only_groups(|n| n.ends_with(&end));
                    struck_parts.push((!part.primitives.is_empty()).then_some(part));
                }
            }
            let model = if self.props.iter().any(|(n, _)| *n == name) || map_model {
                model.without_nodes(|n| STRUCK_GROUPS.iter().any(|g| n.contains(g)))
            } else {
                model
            };
            // A building with state groups (#303, docs/assets.md "States"): the finished
            // building draws without its stages and ruin, the ruin as a model of its own.
            let ruin = model
                .nodes
                .iter()
                .any(|n| state_group(&n.name) == Some("ruin"))
                .then(|| model.without_groups(|n| state_group(n).is_some_and(|s| s != "ruin")));
            // Construction stages (#302) as models of their own, each with the parts outside
            // every group; section states as overlays drawn over the finished building.
            let groups: Vec<String> =
                model.nodes.iter().filter_map(|n| state_group(&n.name)).map(str::to_owned).collect();
            let stages: Vec<Option<ModelData>> = (0..4)
                .map(|k| {
                    let g = format!("build{k}");
                    groups
                        .contains(&g)
                        .then(|| model.without_groups(|n| state_group(n).is_some_and(|s| s != g.as_str())))
                })
                .collect();
            let mut sections: Vec<((u8, u16), ModelData)> = Vec::new();
            for g in &groups {
                if let Some(key) = super::build_view::section_group(g)
                    && !sections.iter().any(|(k, _)| *k == key)
                {
                    sections.push((key, model.only_groups(|n| state_group(n) == Some(g.as_str()))));
                }
            }
            let model = if groups.is_empty() {
                model
            } else {
                model.without_groups(|n| state_group(n).is_some_and(|s| s != "finished"))
            };
            let index = renderer.add_model(device, queue, model);
            if struck_parts.iter().any(Option::is_some) {
                let mut ids = struck_parts.into_iter().map(|m| m.map(|m| renderer.add_model(device, queue, m)));
                let mut next = || ids.next().flatten();
                let (stump, felled, scar, chip) = (next(), next(), next(), next());
                let chips = std::array::from_fn(|_| next());
                let parts = StruckParts { stump, felled, scar, chip, chips };
                self.struck_parts.insert(index, parts);
            }
            if let Some(ruin) = ruin {
                let r = renderer.add_model(device, queue, ruin);
                self.ruins.insert(index, r);
            }
            if stages.iter().any(Option::is_some) || !sections.is_empty() {
                let mut states = BuildingStates::default();
                for (k, m) in stages.into_iter().enumerate() {
                    states.build[k] = m.map(|m| renderer.add_model(device, queue, m));
                }
                for (key, m) in sections {
                    states.sections.push((key.0, key.1, renderer.add_model(device, queue, m)));
                }
                self.states.insert(index, states);
            }
            self.gear.insert(index, gear);
            log::info!("3d: {name}: uploaded in {:?}", started.elapsed());
            self.loaded.insert(name, Some(index));
        }
    }

    /// A key while the game runs; true when the free camera took it: `,` and `.` turn,
    /// Page Up / Page Down tilt, End returns to the original angle.
    pub fn key(&mut self, key: &Key, pressed: bool) -> bool {
        if !self.free {
            return false;
        }
        match key {
            Key::Character(c) if c.as_str() == "," => self.keys.left = pressed,
            Key::Character(c) if c.as_str() == "." => self.keys.right = pressed,
            Key::Named(NamedKey::PageUp) => self.keys.up = pressed,
            Key::Named(NamedKey::PageDown) => self.keys.down = pressed,
            Key::Named(NamedKey::End) => {
                if pressed {
                    self.orbit = Orbit::ORIGINAL;
                }
            }
            _ => return false,
        }
        true
    }

    /// Turns and tilts the free camera by the held keys.
    pub fn tick(&mut self, seconds: f32) {
        self.frame_seconds = seconds;
        self.wind_clock = (self.wind_clock + seconds.clamp(0.0, 0.25)) % WIND_PERIOD;
        if !self.free {
            return;
        }
        let dt = seconds.min(0.25);
        let k = &self.keys;
        let turn = f32::from(u8::from(k.right)) - f32::from(u8::from(k.left));
        let tilt = f32::from(u8::from(k.up)) - f32::from(u8::from(k.down));
        if turn != 0.0 || tilt != 0.0 {
            self.orbit =
                Orbit { yaw: self.orbit.yaw + turn * TURN_RATE * dt, pitch: self.orbit.pitch + tilt * TILT_RATE * dt }
                    .clamped();
        }
    }

    pub fn release_keys(&mut self) {
        self.keys = HeldKeys::default();
    }

    /// Collects this frame's model instances from the simulation; requests model files of
    /// types that appear.
    pub fn update(&mut self, world: &World, terrain: &TerrainData, objects: &ObjectsView, request: &dyn Fn(&str)) {
        self.instances.clear();
        self.pools.clear();
        self.shown.clear();
        self.drawn.clear();
        self.request_ground(request);
        if self.models == Models::Off || !self.map_models.on {
            self.map_models.clear();
        }
        self.veils.clear();
        if self.models == Models::Off {
            return;
        }
        // The ragdoll steps of the last frame ran on a worker over the render: the world back
        // before anything below reads a body (`bone_world`).
        if let Some(r) = self.ragdolls.as_mut() {
            r.join();
        }
        let defs = objects.objects();
        let mut names: HashMap<acr_sim::TypeId, String> = HashMap::new();
        let mut ups: HashMap<acr_sim::TypeId, String> = HashMap::new();
        // Corpses drawn as models: instance index, sunk ground point, yaw, footprint.
        let mut corpses: Vec<(usize, [f32; 3], f32)> = Vec::new();
        let mut footprints: Vec<piles::Footprint> = Vec::new();
        let mut targets: Vec<super::splatter::Target> = Vec::new();
        let mut pieces: Vec<(usize, Damage)> = Vec::new();
        // Guts beside open bellies, drawn after the units (the indices above are per unit).
        let mut extra: Vec<ModelInstance> = Vec::new();
        // Models of the units drawn, for the pieces their hits throw.
        let mut unit_models: HashMap<u32, (UnitModel, usize)> = HashMap::new();
        // Rigged men drawn: unit index → instance index (ragdolls, #278).
        let mut rigged: Vec<(usize, usize)> = Vec::new();
        // Riders (bones prefixed `rider.`): only the strike layer poses them.
        let mut riders: Vec<(usize, usize)> = Vec::new();
        // Ships and guns drawn (`acr_machine`): their hits splash no blood.
        let mut machines: std::collections::HashSet<u32> = std::collections::HashSet::new();
        let now = world.tick.0;
        self.crews.begin(now);
        if self.debris_tick > now {
            // A loaded game or a restart: the debris belongs to another battle.
            self.debris.clear();
            self.wounds.clear();
            self.burns.clear();
            self.kept.clear();
            self.seeps.clear();
            self.missiles.clear();
        }
        self.debris_tick = now;
        self.burning.clear();
        self.request_gore(request);
        // The water's clock (as `render` passes to the surface) for ship motion and rings.
        let water_seconds = self.started.elapsed().as_secs_f32();
        self.water_view.begin(now, water_seconds);
        let shots = self.water_view.shots(world, terrain, water_seconds);
        self.shots.extend(shots);
        // Corpses the simulation removed (after about a minute, as the original) stay in 3D
        // until their own lifetime ends (`corpse_minutes`), so battlefields fill and piles
        // grow; their units keep position, facing and lying pose. An animal becomes a
        // carcass (meat) sprite when its death animation ends; its body stays in 3D, holding
        // the death's last pose, for as long as other corpses.
        let lingering = |u: &Unit| {
            (self.gore != Gore::Off
                && !u.alive
                && !u.building
                && (u.death >= 1000 || (u.death > 0 && world.catalog.get(u.kind).economy.meat.is_some()))
                && world.deaths.get(&u.id.0).is_some_and(|r| r.tick > 0 && now - r.tick < self.corpse_ticks))
                // A building the simulation removed at once (a type without `#DEATH`) stays
                // as a 3D ruin as long as a `#DEATHLIE1` would (#303).
                || (u.building
                    && !u.alive
                    && u.death > 0
                    && self.models == Models::All
                    && world.deaths.get(&u.id.0).is_some_and(|r| r.tick > 0 && now - r.tick < super::ruin_view::RUIN_TICKS))
        };
        // Wounds where this tick's hits struck (#277), kept for the drawn and their corpses.
        let facing = |v: u32| world.units.get(v as usize).filter(|u| u.id.0 == v && !u.building).map(|u| u.dir);
        let instances_scope = acr_sim::prof::Scope::new("3d.instances");
        self.wounds.take_hits(world.hits.iter(), now, self.gore, &facing);
        self.building_hits.take(world);
        self.scars.clear();
        self.arrow_fires.clear();
        let mut arrows: Vec<acr_render::math::Mat4> = Vec::new();
        // The section overlays of the building being drawn (#302).
        let mut sections: Vec<usize> = Vec::new();
        self.wounds.retain(&|v| world.units.get(v as usize).is_some_and(|u| u.id.0 == v && (u.alive || lingering(u))));
        self.burns.retain(|v| world.units.get(v as usize).is_some_and(|u| u.id.0 == v && (u.alive || lingering(u))));
        // Arrows stuck in bodies (#308) go with the corpse.
        self.missiles.body.retain(&|v| {
            self.gore != Gore::Off
                && world.units.get(v as usize).is_some_and(|u| u.id.0 == v && (u.alive || lingering(u)))
        });
        // The plain living (standing, walking, fighting; not crawling, not dropping his gear, no
        // gun or ship) well off this frame's view are left out: in a big map most men
        // (m2-par-loops). What their instances feed while off it is kept below.
        let cull = (self.held.is_empty() && cull_enabled()).then_some(self.cull_view).flatten();
        let off = |u: &Unit, model: &ModelData| {
            cull.is_some_and(|v| {
                u.alive
                    && u.death == 0
                    && !u.building
                    && !u.body.crawling()
                    && !u.body.dropped
                    && !model.machine
                    && model.crew.is_none()
                    && off_view(&v, terrain, u)
            })
        };
        // What is left of each man's and animal's body, his torn tissue (the costliest part of
        // an instance in a big fight), each on its own and in parallel on native
        // (`acr_render::par`); the loop below takes it by unit (and works it out itself for
        // anything this missed).
        let mut damaged: HashMap<u32, (usize, Option<(Damage, bool)>)> = {
            let mut jobs: Vec<(&Unit, usize, &ModelData)> = Vec::new();
            let renderer = self.renderer.as_ref();
            for u in world.units.iter().filter(|u| (u.alive || lingering(u)) && u.inside_of.is_none() && !u.building) {
                let name = names.entry(u.kind).or_insert_with(|| model_name(&self.files, world.catalog.get(u.kind)));
                if let Some(Some(index)) = self.loaded.get(name.as_str())
                    && let Some(model) = renderer.and_then(|r| r.model(*index))
                    && !off(u, model)
                {
                    jobs.push((u, *index, model));
                }
            }
            let (gore, wounds) = (self.gore, &self.wounds);
            acr_render::par::map(&jobs, |&(u, index, model)| {
                let record = world.deaths.get(&u.id.0).copied().unwrap_or_default();
                (u.id.0, (index, body_damage(gore, wounds, model, u, &record)))
            })
            .into_iter()
            .collect()
        };
        for u in world.units.iter().filter(|u| (u.alive || lingering(u)) && u.inside_of.is_none()) {
            // Buildings: under construction (#302), finished, burning down, falling in and as
            // ruins (#303).
            let fallen = u.building && u.death > 0 && u.stage >= world.max_stage(u);
            let constructing = u.building && u.alive && u.death == 0 && u.stage < world.max_stage(u);
            if u.building && (self.models != Models::All || !(finished(world, u) || fallen || constructing)) {
                continue;
            }
            let mut name = names.entry(u.kind).or_insert_with(|| model_name(&self.files, world.catalog.get(u.kind)));
            // Upgraded (#302): every section done draws the `<NAME>UP` model where there is one.
            if u.building
                && !constructing
                && super::build_view::upgraded(&world.catalog.get(u.kind).economy.sections, u.stages)
            {
                let up = ups.entry(u.kind).or_insert_with(|| format!("{name}UP"));
                if self.files.contains_key(up.as_str()) {
                    name = up;
                }
            }
            let Some(path) = self.files.get(name.as_str()) else { continue };
            let index = match self.loaded.get(name.as_str()) {
                Some(Some(i)) => *i,
                Some(None) => continue,
                None => {
                    let key = acr_formats::normalize_path(path);
                    if !self.requested.contains_key(&key) && !self.parsed.iter().any(|(n, _)| n == name) {
                        self.requested.insert(key, name.clone());
                        request(path);
                    }
                    continue;
                }
            };
            let Some(model) = self.renderer.as_ref().and_then(|r| r.model(index)) else { continue };
            if off(u, model) {
                // Off the view: still a model (no sprite at the edge), not crawling, and his
                // blood trail goes on.
                self.shown.insert(u.id);
                self.wounds.set_lying(u.id.0, None);
                self.turns.forget(u.id.0);
                if (u.body.bleeding() > 0 || self.wounds.bleeding(u.id.0, now))
                    && self.gore != Gore::Off
                    && now.is_multiple_of(super::trail::EVERY)
                    && now != self.trail_tick
                {
                    let (x, y) = (u.pos.x >> 4, u.pos.y >> 4);
                    self.trail.sample(u.id.0, unit_ground(terrain, x, y), false, now);
                }
                continue;
            }
            let removed_at = match defs.definition(&world.catalog.get(u.kind).id) {
                Some(d) if d.animation("#DEATHLIE2").is_some() => 2000,
                _ => 1200,
            };
            let record = world.deaths.get(&u.id.0).copied().unwrap_or_default();
            // 3D corpses age on their own clock: two counter steps per tick from the death
            // (as a lying corpse counts), removed after `corpse_minutes`.
            let (death, removed_at) = if u.death > 0 && !u.building && self.gore != Gore::Off && record.tick > 0 {
                let age = now.saturating_sub(record.tick);
                ((2 * age).min(u64::from(u16::MAX)) as u16, (2 * self.corpse_ticks).min(u64::from(u16::MAX)) as u16)
            } else {
                (u.death, removed_at)
            };
            let (x, y) = (u.pos.x >> 4, u.pos.y >> 4);
            let ground = unit_ground(terrain, x, y);
            let body = super::gore::Body { death, ..u.into() };
            let (mut wounds, mut pool) = super::gore::corpse(body, record, self.gore, removed_at, ground);
            // A building bleeds no pool and takes no wounds (its burn comes from `ruin_view`).
            if u.building {
                (wounds, pool) = (Default::default(), None);
            }
            // A big animal bleeds a bigger pool (by its girth against a man's).
            if let (Some(p), Some(rig)) = (pool.as_mut(), model.beast_rig.as_ref()) {
                p.radius *= (rig.trunk.radius_at(0.5) / 0.16).clamp(0.6, 2.5);
            }
            // Ships and guns leave no blood.
            if !model.machine
                && let Some(p) = pool
            {
                if self.marks_keep && u.death > 0 {
                    self.kept.corpse_pool(u.id.0, &p);
                }
                self.pools.push(p);
            }
            // The fluids of the decay soak into the ground under him (`decay_stain`).
            if self.stain && !model.machine && u.death > 0 && !u.building && self.gore != Gore::Off && record.tick > 0 {
                let size =
                    model.beast_rig.as_ref().map_or(1.0, |rig| (rig.trunk.radius_at(0.5) / 0.16).clamp(0.8, 1.8));
                let turf = f32::from(u8::from(super::particle_view::soil_at(terrain, None, ground[0], ground[1]).turf));
                let decay = model::corpse_decay(death, removed_at);
                let seed = (u.id.0 as f32 * 0.618_034).fract() * 97.0;
                if let Some(p) =
                    super::seep::stain(ground, view3d::facing_yaw(u.dir).sin_cos(), size, decay, turf, seed)
                {
                    self.seeps.corpse(u.id.0, p);
                }
            }
            // The wounds of his hits, alive and dead; they replace the corpse's guessed ones.
            if !u.building {
                (wounds.marks, wounds.marks_len) = self.wounds.marks(u.id.0, wound_rig(model), now);
                if wounds.marks_len > 0 {
                    wounds.count = 0;
                }
                // Charred where he burned, alive and dead (#207, `fire_view`); the cause chars a
                // corpse too.
                if self.gore != Gore::Off {
                    wounds.burn = wounds.burn.max(self.fire.burn(u.id.0)).max(self.burns.amount(u.id.0));
                }
            }
            let (mut inst, at) = instance(u, death, index, model, terrain, self.gore, removed_at, wounds);
            // Burning down, falling in, a ruin (#303): charred, its fires and smoke at its
            // anchors, sunk and squashed (or its ruin state) once it died.
            if u.building {
                let anchors = self.anchors.entry(index).or_insert_with(|| super::ruin_view::anchors(model));
                // Under construction its life follows the stages: no fires for that.
                let lost = if constructing { 0.0 } else { 1.0 - u.life.max(0) as f32 / u.max_life.max(1) as f32 };
                let dead = (u.death > 0)
                    .then(|| now.saturating_sub(record.tick) as f32 / acr_sim::ORIGINAL_TICKS_PER_SECOND as f32);
                if dead.is_none() && lost >= super::ruin_view::HEAVY_DAMAGE && !self.chunks.contains_key(&index) {
                    self.chunks_wanted.entry(index).or_insert(None);
                }
                let ruin = self.ruins.get(&index).copied();
                let look = super::ruin_view::look(u.id.0, ground, anchors, lost, dead, ruin.is_some());
                inst.transform = look.transform;
                inst.wounds.burn = look.burn;
                (inst.decay, inst.morphs, inst.transparent) = (0.0, [0.0; 3], false);
                if look.show_ruin
                    && let Some(r) = ruin
                {
                    inst.model = r;
                }
                let mut fire = look.fire;
                // Falling in: the `DESTRUCT` flash over it thins (`veils`) and pieces of its
                // roof and walls break off, tumble down and settle in the rubble.
                if let Some(t) = dead {
                    if t < super::ruin_view::COLLAPSE_SECONDS + 1.0 {
                        let reach = (fire.half[0].max(fire.half[1]) as i32).max(32) + 48;
                        self.veils.push((x, y, reach, u.kind));
                    }
                    match self.chunks.get(&index) {
                        None => {
                            self.chunks_wanted.insert(index, Some(t));
                        }
                        Some(list) => {
                            for (k, &(m, centre, radius)) in list.iter().enumerate() {
                                let Some((transform, landed)) =
                                    super::ruin_view::chunk_fall(ground, u.id.0, k, centre, radius, t)
                                else {
                                    continue;
                                };
                                if (0.0..super::ruin_view::LANDING_DUST).contains(&landed) {
                                    fire.landings.push([transform[3][0], transform[3][1], ground[2]]);
                                }
                                extra.push(ModelInstance {
                                    model: m,
                                    clip: None,
                                    time: 0.0,
                                    transform,
                                    rise: None,
                                    damage: Default::default(),
                                    // Less charred than the burnt-out shell: the broken
                                    // planks and thatch read against it.
                                    wounds: Wounds {
                                        burn: inst.wounds.burn.min(super::ruin_view::CHUNK_BURN),
                                        ..inst.wounds
                                    },
                                    ..inst
                                });
                            }
                        }
                    }
                }
                if fire.flames + fire.smoking + fire.dust > 0.0 || !fire.landings.is_empty() {
                    self.burning.push(fire);
                }
                let states = self.states.get(&index);
                if constructing {
                    // Its stage group, else the model rising with the progress (#302).
                    let k = match u.anim {
                        AnimKind::BuildLo(k) => super::build_view::stage_group(k),
                        _ => 0,
                    };
                    match states.and_then(|s| s.build[k]) {
                        Some(m) => inst.model = m,
                        None => {
                            let progress = u.stage.max(0) as f32 / world.max_stage(u).max(1) as f32;
                            inst.rise = Some(super::build_view::rise(progress, anchors.top));
                        }
                    }
                } else if !look.show_ruin
                    && let Some(states) = states
                {
                    // Its sections in their states (#302), drawn as it is.
                    for &(s, state, m) in &states.sections {
                        if acr_sim::upgrade::section_state(u.stages, s) == state {
                            sections.push(m);
                        }
                    }
                }
                // Holes, pocks, scorch and stuck arrows where rounds struck it (#304).
                if let Some(scars) =
                    self.building_hits.place(u, index, model, &inst, &look.sooted, &mut arrows, &mut self.arrow_fires)
                {
                    self.scars.insert(self.instances.len(), scars);
                }
            }
            // Starved: the `emaciated` morph (#207), on models that carry it.
            if u.death > 0 && self.gore != Gore::Off && record.cause == acr_sim::combat::DeathCause::Hunger {
                inst.morphs[2] = 1.0;
            }
            // The strike clip for the blow (`docs/melee.md`, Strike clips), when the model has one.
            let clip = super::strike_view::strike_clip(model, world, u);
            if let Some(c) = clip {
                inst.clip = Some(c);
                // A long `ATTACK` repeats the blow instead of standing between blows.
                inst.time = super::strike_view::strike_time(u.frame, u.anim_frames, &model.clips[c], model.fps);
            } else if let Some(c) = stone_clip(model, world, u) {
                inst.clip = Some(c);
            }
            if u.alive && u.death == 0 && !model.states.is_empty() && !u.body.crawling() {
                self.tool_clips.insert(u.id.0, inst.clip);
            }
            // A SLOWRECHARGE musketeer replays #ATTACK3 until his reload counter is down to one
            // play (`combat.rs`, the slow-recharge branch): he rams on that last play only and
            // holds the clip's first frame, the rammer in the bore, through the others (#255).
            if u.anim == AnimKind::Attack(3) && u.delay > u.anim_frames && world.catalog.get(u.kind).slow_recharge {
                inst.time = 0.0;
            }
            // A rider striking down turns his horse so the man lies at his side, eased in and
            // back.
            if u.alive && u.death == 0 && !u.building {
                let want = clip.and_then(|c| super::strike_view::strike_yaw(model, world, u, c));
                let facing = view3d::facing_yaw(u.dir);
                if let Some(yaw) = self.turns.ease(u.id.0, want, facing, self.frame_seconds) {
                    let t = inst.transform[3];
                    inst.transform = view3d::model_matrix([t[0], t[1], t[2]], yaw, 1.0);
                }
            } else {
                self.turns.forget(u.id.0);
            }
            if model.world_clock {
                // A finished building plays its `WORK` clip (mill sails, a water wheel: the
                // sprite's `#WORK` layer, 0x50927a) on the world clock.
                if u.building
                    && let Some(work) = model.clip("WORK")
                {
                    inst.clip = Some(work);
                }
                inst.time = world_time(model, inst.clip, u, now, record.tick);
            }
            if model.machine && !u.building {
                // A wreck neither rots nor sinks into the ground: its clips show the damage.
                // A gun's crew takes the blood sprayed onto it (#287).
                if model.crew.is_none() {
                    machines.insert(u.id.0);
                } else {
                    inst.uniform = self.crews.uniform(world, u);
                    if let Some((c, time)) = self.crews.work(model, u, now, self.frame_seconds) {
                        (inst.clip, inst.time) = (Some(c), time);
                    }
                }
                inst.decay = 0.0;
                inst.morphs = [0.0; 3];
                inst.wounds = Default::default();
                // A gun turns smoothly through its 16 sprite directions (crew_view.rs).
                let yaw = if model.crew.is_some() {
                    self.crews.gun_yaw(u, world.catalog.get(u.kind), self.frame_seconds)
                } else {
                    view3d::facing_yaw(u.dir)
                };
                inst.transform = self
                    .water_view
                    .ship(u, now, terrain, ground, yaw, model.radius, water_seconds)
                    .unwrap_or_else(|| view3d::model_matrix(ground, yaw, 1.0));
            }
            let living = u.alive && u.death == 0;
            let damage = match damaged.remove(&u.id.0) {
                Some((i, damage)) if i == index => damage,
                _ => body_damage(self.gore, &self.wounds, model, u, &record),
            };
            let Some((damage, belly_open)) = damage else {
                // Pulverized: nothing whole to draw, only gibs and mist (and no sprite).
                self.shown.insert(u.id);
                continue;
            };
            inst.damage = damage;
            if model.rig.is_some() {
                unit_models.insert(u.id.0, (UnitModel { model: index, player: u.owner }, index));
            }
            // Lost legs (#254, #262): he lies face down and drags himself along, head first.
            if living && !u.building {
                let lying = u.body.crawling().then(|| super::damage_view::crawl_clip(&u.body));
                self.wounds.set_lying(u.id.0, lying);
            }
            // Killed while crawling or writhing (#277): he never stands up to die, his last
            // crawling pose is held and sinks as a corpse.
            if !living
                && !u.building
                && let Some(name) = self.wounds.lying(u.id.0)
            {
                match model.clip(name) {
                    Some(clip) => {
                        inst.clip = Some(clip);
                        inst.time = record.tick as f32 / acr_sim::ORIGINAL_TICKS_PER_SECOND as f32;
                    }
                    None => {
                        inst.clip = model.clip_for_animation("DEATHLIE1");
                        inst.time = 0.0;
                        inst.morphs[1] = 1.0;
                    }
                }
            }
            if living && u.body.crawling() {
                match model.clip(super::damage_view::crawl_clip(&u.body)) {
                    Some(clip) => {
                        inst.clip = Some(clip);
                        inst.time = now as f32 / acr_sim::ORIGINAL_TICKS_PER_SECOND as f32;
                    }
                    None => {
                        inst.clip = model.clip_for_animation("DEATHLIE1");
                        inst.time = 0.0;
                        inst.morphs[1] = 1.0;
                    }
                }
            }
            // A crawler or a man who can no longer hold his musket lets it fall with his hat.
            if living
                && (u.body.crawling() || u.body.dropped)
                && let Some(gear) = self.gear.get(&index)
                && let Some(unarmed) = gear.unarmed
            {
                inst.model = unarmed;
                let held = self.tool_clips.remove(&u.id.0).flatten();
                let dropped: Vec<usize> = (gear.dropped.iter().zip(&gear.nodes))
                    .filter(|&(_, &(n, _))| holds(model, n, held))
                    .map(|(&m, _)| m)
                    .collect();
                self.debris.drop_gear(u.id.0, &dropped, u.owner, at, now);
            }
            // Killed (#206): his held gear stays in his hands and on his head while he falls
            // and lies beside him from the landing on (the death clip's end, or `#DEATHLIE`).
            if !living
                && !u.building
                && let Some(gear) = self.gear.get(&index)
                && let Some(unarmed) = gear.unarmed
            {
                // The tool he held stays in his hand until it is laid down (`DEATH_AXE`).
                if !self.debris.has_dropped(u.id.0)
                    && let Some(c) = inst.clip
                    && let Some(v) = held_death_clip(model, c, self.tool_clips.get(&u.id.0).copied().flatten())
                {
                    inst.clip = Some(v);
                }
                let landed = matches!(u.anim, AnimKind::DeathLie(_))
                    || inst.clip.and_then(|c| model.clips.get(c)).is_none_or(|c| inst.time >= c.duration);
                if self.debris.has_dropped(u.id.0) {
                    inst.model = unarmed;
                } else if landed && self.gore != Gore::Off {
                    let ragdolls = self.ragdolls.as_ref();
                    let bone = |name: &str| ragdolls.and_then(|r| r.bone_world(u.id.0, name));
                    let tick = if record.tick > 0 { record.tick } else { now };
                    let held = self.tool_clips.remove(&u.id.0).flatten();
                    let laid = laid_gear(model, gear, &inst, &bone, u.id.0, tick, held);
                    self.debris.lay_gear(u.id.0, &laid, u.owner, tick, now);
                    inst.model = unarmed;
                }
            }
            if belly_open && self.gore == Gore::Full {
                let yaw = view3d::facing_yaw(u.dir);
                super::debris::guts(u.id.0, at, yaw, inst.decay, u.owner, &self.gibs, &mut extra);
            }
            // The bleeding leave drops of blood as they go, crawlers a smear.
            if u.alive
                && u.death == 0
                && (u.body.bleeding() > 0 || self.wounds.bleeding(u.id.0, now))
                && self.gore != Gore::Off
                && now.is_multiple_of(super::trail::EVERY)
                && now != self.trail_tick
            {
                self.trail.sample(u.id.0, at, u.body.crawling(), now);
            }
            if u.death > 0 && !u.building && !model.machine {
                corpses.push((self.instances.len(), at, view3d::facing_yaw(u.dir)));
                let ground = |x: f32, y: f32| terrain.surface(x, y) / view3d::COS_E;
                let mut f = footprint(u, record.tick, at, &inst, model.radius);
                f.ground = match self.pile_ground.get(&u.id.0) {
                    Some(&(p, g)) if p == [at[0], at[1]] => g,
                    _ => {
                        let g = pile_ground(&f, [at[0], at[1]], &ground);
                        self.pile_ground.insert(u.id.0, ([at[0], at[1]], g));
                        g
                    }
                };
                footprints.push(f);
            }
            targets.push(super::splatter::Target {
                unit: u.id.0,
                pos: (at[0], at[1]),
                yaw: if u.building { 0.0 } else { view3d::facing_yaw(u.dir) },
                building: u.building,
                radius: model.radius,
            });
            // `ACR_DEBUG_DAMAGE` overrides the game's damage (a debug aid).
            if let Some(damage) = model.rig.as_ref().and_then(|_| debug_damage(u.id.0)) {
                inst.damage = damage;
                pieces.push((self.instances.len(), damage));
            }
            // Men of skeleton H and animals (their own ragdolls).
            if (model.rig.is_some() || super::ragdoll_view::animal_model(model)) && !u.building {
                rigged.push((u.id.0 as usize, self.instances.len()));
            } else if living && !u.building && model.nodes.iter().any(|n| n.name == "rider.hips") {
                riders.push((u.id.0 as usize, self.instances.len()));
            }
            if model.crew.is_some() && !u.building {
                self.crews.gun(u, model, self.instances.len(), &inst, now);
            }
            // The studio's held pose (#299) wins over the game's clip.
            if let Some(&(clip, time)) = self.held.get(&u.id.0) {
                (inst.clip, inst.time) = (Some(clip), time);
            }
            self.drawn.insert(u.id.0, self.instances.len());
            for m in sections.drain(..) {
                extra.push(ModelInstance { model: m, ..inst });
            }
            self.instances.push(inst);
            self.shown.insert(u.id);
        }
        // Rubble knocked out of the holes (#304), lying for the battle.
        let mut rubble = Vec::new();
        self.building_hits.pieces(self.frame_seconds, &mut rubble);
        if let Some(models) = self.rubble_models {
            for (transform, wood) in rubble {
                self.instances.push(ModelInstance {
                    model: models[usize::from(wood)],
                    clip: None,
                    time: 0.0,
                    transform,
                    decay: 0.0,
                    morphs: [0.0; 3],
                    transparent: false,
                    player: 0,
                    wounds: Default::default(),
                    sprayed: Default::default(),
                    damage: Default::default(),
                    look: None,
                    uniform: None,
                    drape: None,
                    rise: None,
                });
            }
        }
        if let Some(arrow) = self.arrow_model {
            for transform in arrows {
                self.instances.push(ModelInstance {
                    model: arrow,
                    clip: None,
                    time: 0.0,
                    transform,
                    decay: 0.0,
                    morphs: [0.0; 3],
                    transparent: false,
                    player: 0,
                    wounds: Default::default(),
                    sprayed: Default::default(),
                    damage: Default::default(),
                    look: None,
                    uniform: None,
                    drape: None,
                    rise: None,
                });
            }
        }
        drop(instances_scope);
        let spray_scope = acr_sim::prof::Scope::new("3d.spray_trail");
        // Blood sprayed onto troops and buildings behind hit units (#220).
        for (i, sprayed) in super::splatter::sprayed_targets(&world.hits, world.tick.0, self.gore, &targets) {
            if !machines.contains(&targets[i].unit) {
                self.instances[i].sprayed = sprayed;
            }
        }
        // Blood burst around his own newest wound where nothing else sprayed on him (#277).
        for (i, t) in targets.iter().enumerate() {
            if t.building || machines.contains(&t.unit) || self.instances[i].sprayed.amount > 0.0 {
                continue;
            }
            let rig = self.renderer.as_ref().and_then(|r| r.model(self.instances[i].model)).and_then(wound_rig);
            if let Some(s) = self.wounds.spurt(t.unit, rig, now) {
                self.instances[i].sprayed = s;
            }
        }
        // Blood trails of the bleeding (#254, #262): drops and smears that dry and fade.
        if now.is_multiple_of(super::trail::EVERY) {
            self.trail_tick = now;
        }
        self.trail.expire(now);
        self.trail.pools(now, &mut self.pools);
        drop(spray_scope);
        // Corpses that overlap lie on each other (#221).
        let piles_scope = acr_sim::prof::Scope::new("3d.piles");
        let start = web_time::Instant::now();
        if self.pile_ground.len() > 2 * footprints.len() + 64 {
            let ids: HashSet<u32> = footprints.iter().map(|f| f.order.1).collect();
            self.pile_ground.retain(|id, _| ids.contains(id));
        }
        let rests = self.piles.rests(&footprints);
        self.pile_us += start.elapsed().as_micros();
        self.pile_runs += 1;
        for ((&(i, at, yaw), rest), f) in corpses.iter().zip(rests).zip(&footprints) {
            if *rest != piles::Rest::default() {
                let ground = [at[0], at[1], at[2] + rest.lift];
                self.instances[i].transform = view3d::model_matrix_tilted(ground, yaw, 1.0, rest.pitch, rest.roll);
            }
            if rest.drape != [0.0; 4] {
                // The stations along the model's z (metres from its ground point).
                let ahead = (f.centre[0] - at[0]) * f.along[0] + (f.centre[1] - at[1]) * f.along[1];
                self.instances[i].drape = Some(Drape {
                    heights: rest.drape.map(|d| d / PIXELS_PER_METRE),
                    first: (ahead + piles::STATIONS[0] * f.half_length) / PIXELS_PER_METRE,
                    spacing: (piles::STATIONS[1] - piles::STATIONS[0]) * f.half_length / PIXELS_PER_METRE,
                });
            }
        }
        if self.pile_runs == 500 {
            let (full, alone) = self.piles.take_counts();
            log::info!(
                "3d: {} corpses piled in {} µs ({full} stacked, {alone} placed alone); debris {}; wounded {}",
                footprints.len(),
                self.pile_us / 500,
                self.debris.len(),
                self.wounds.len()
            );
            self.pile_us = 0;
            self.pile_runs = 0;
        }
        drop(piles_scope);
        // Animals stand over the men they maul (#276), before the ragdolls take the pose.
        {
            let renderer = self.renderer.as_ref();
            let ragdolls = self.ragdolls.as_ref();
            let ground = |x: f32, y: f32| surface_point(terrain, x, y);
            let rig = |m: usize| renderer.and_then(|r| r.model(m)).and_then(|d| d.beast_rig.as_ref());
            let lying = |v: u32, limb: &'static str| {
                let bone = |name: &str| ragdolls.and_then(|r| r.bone_world(v, name));
                // The chest two thirds of the way from the hips to the neck (the spine's body
                // lies close to the hips).
                if let (Some(neck), Some(hips)) = (bone("neck"), bone("hips")) {
                    let chest = [0, 1, 2].map(|i| hips[i] + 0.67 * (neck[i] - hips[i]));
                    return Some(super::maul_view::Lying { chest, hips, limb: bone(limb).unwrap_or(hips) });
                }
                // No ragdoll: he lies along his facing from where he stood.
                let u = world.units.get(v as usize).filter(|u| u.id.0 == v)?;
                let p = ground((u.pos.x >> 4) as f32, (u.pos.y >> 4) as f32 + view3d::UNIT_SOUTH);
                let (s, c) = view3d::facing_yaw(u.dir).sin_cos();
                let at = |m: f32| ground(p[0] + s * m * PIXELS_PER_METRE, p[1] + c * m * PIXELS_PER_METRE);
                Some(super::maul_view::Lying { chest: at(0.45), hips: p, limb: at(-0.6) })
            };
            self.maulers.place(world, self.frame_seconds, &rigged, &mut self.instances, &rig, &lying, &ground);
        }
        self.update_ragdolls(world, terrain, &rigged, &riders);
        // Physics-posed bodies lie as the physics put them: no drape (#229).
        if let Some(r) = self.ragdolls.as_ref() {
            for &i in r.poses.keys() {
                if let Some(inst) = self.instances.get_mut(i) {
                    inst.drape = None;
                }
            }
        }
        acr_sim::prof_scope!("3d.debris_decals");
        // Debris and decals well off this frame's view are left out, as the living are
        // (m2-perf6): nothing reads them but the draw.
        let far_view = (self.held.is_empty() && cull_enabled()).then_some(self.cull_view).flatten();
        // Appended last: the indices above refer to one instance per unit.
        for (i, damage) in pieces {
            let body = self.instances[i];
            self.instances.extend(debug_pieces(&body, damage));
        }
        self.instances.extend(extra);
        // The fallen of the gun crews, each a corpse of his own (#287).
        {
            let renderer = self.renderer.as_ref();
            acr_sim::prof_scope!("3d.crew_corpses");
            self.crews.corpses(
                |i| renderer.and_then(|r| r.model(i)),
                now,
                self.corpse_ticks,
                self.gore,
                &mut self.instances,
                &mut self.pools,
            );
        }
        let ground = |x: f32, y: f32| surface_point(terrain, x, y);
        // Severed limbs and halves thrown by the blow, gibs (#262 S5).
        let renderer = self.renderer.as_ref();
        let unit = |v: u32| {
            let &(m, index) = unit_models.get(&v)?;
            let model = renderer?.model(index)?;
            Some((m, model.rig.as_ref()?, super::debris::coat(model)))
        };
        let death = |v: u32| world.deaths.get(&v).copied();
        let wounds = &self.wounds;
        let tears =
            |h: &acr_sim::combat::HitRecord, rig: &acr_render::damage::Rig| wounds.hit_volumes(h.victim, h.tick, rig);
        {
            acr_sim::prof_scope!("3d.debris_hits");
            self.debris.take_hits(world.hits.iter(), now, self.gore, &unit, &death, &tears, &self.gibs, &ground);
        }
        if self.marks_keep {
            // Rubble stays: the newest pieces, gibs and gear, rotting down to bones.
            self.debris.keep_newest(super::debris::KEEP_CAP);
        } else {
            self.debris.expire(now, self.corpse_ticks);
        }
        // Shell bursts throw the remains lying around them with the mud (gibs, pieces and
        // gear here, the corpses in the ragdolls).
        if now < self.last_blast {
            self.last_blast = 0;
        }
        if now < self.last_graze {
            self.last_graze = 0;
        }
        self.blasts.clear();
        let near = |blasts: &[Blast], at: [f32; 2]| {
            blasts.iter().any(|b| (b.at[0] - at[0]).abs() + (b.at[1] - at[1]).abs() < 20.0)
        };
        for i in world
            .impacts
            .iter()
            .filter(|i| i.tick > self.last_blast && i.cause == acr_sim::combat::DeathCause::Explosion)
        {
            let at = [i.pos.0 as f32, i.pos.1 as f32];
            if !near(&self.blasts, at) {
                self.blasts.push(if i.mortar { Blast::mortar(at) } else { Blast::shell(at) });
            }
        }
        self.last_blast = world.impacts.back().map_or(self.last_blast, |i| i.tick.max(self.last_blast));
        // A cannonball's grazes: where it threw up earth (hit-locations rule only).
        for f in world.fragments.iter().filter(|f| f.tick > self.last_graze && f.kind == acr_sim::shrapnel::Kind::Earth)
        {
            let at = [f.from.0 as f32, f.from.1 as f32];
            if !near(&self.blasts, at) {
                self.blasts.push(Blast::graze(at));
            }
        }
        self.last_graze = world.fragments.back().map_or(self.last_graze, |f| f.tick.max(self.last_graze));
        if !self.blasts.is_empty() {
            acr_sim::prof_scope!("3d.blast_debris");
            for b in &self.blasts {
                let n = self.debris.blast(b.at, b.radius, b.pieces, b.force, now, &ground);
                log::debug!("3d: blast at {:?} threw {n} pieces", b.at);
            }
        }
        let turf = |x: f32, y: f32| f32::from(u8::from(super::particle_view::soil_at(terrain, None, x, y).turf));
        let stain: Option<&dyn Fn(f32, f32) -> f32> =
            if self.stain && self.gore != Gore::Off { Some(&turf) } else { None };
        {
            acr_sim::prof_scope!("3d.debris_draw");
            let shown = |x: f32, y: f32| !far_view.is_some_and(|v| off_view_at(&v, terrain, x, y));
            self.debris.draw(
                now,
                self.corpse_ticks,
                self.marks_keep,
                &ground,
                &shown,
                &mut self.instances,
                &mut self.pools,
                stain,
            );
        }
        {
            let height = |x: f32, y: f32| super::particle_view::ground_z(terrain, x, y);
            let sink = &mut self.particles.particles;
            self.missiles.draw(world, &height, self.corpse_ticks, self.frame_seconds, &mut self.instances, sink);
        }
        // Arrows and spears stuck in the men they hit (#308): on the bone, through clips,
        // ragdolls and corpses; not with the original fade (gore=off).
        if self.gore != Gore::Off {
            self.missiles.stick(world);
        }
        {
            let renderer = self.renderer.as_ref();
            let model = |i: usize| renderer.and_then(|r| r.model(i));
            let mut poses = vec![&self.crews.poses];
            if let Some(r) = self.ragdolls.as_ref() {
                poses.push(&r.poses);
            }
            self.missiles.draw_body(&self.drawn, &model, &poses, &mut self.instances);
        }
        if self.marks_keep {
            acr_sim::prof_scope!("3d.keep_marks");
            self.keep_marks(world, &machines, &ground);
        }
        let (hits_until, impacts_until) = (self.kept.hits_until, self.kept.impacts_until);
        {
            acr_sim::prof_scope!("3d.splatter");
            // The spray of a hit well off the view lands well off it too (a few metres).
            let near = |(x, y): (i32, i32)| !far_view.is_some_and(|v| off_view_at(&v, terrain, x as f32, y as f32));
            for hit in world.hits.iter().filter(|h| !machines.contains(&h.victim) && h.tick > hits_until && near(h.pos))
            {
                super::splatter::splatter(hit, world.tick.0, self.gore, &ground, &mut self.pools);
                super::debris::mist(hit, world.tick.0, self.gore, &ground, &mut self.pools);
            }
            // Earth churned up where rounds struck the ground (#226).
            for impact in world.impacts.iter().filter(|i| !self.ground_marks && i.tick > impacts_until && near(i.pos)) {
                super::splatter::impact(impact, world.tick.0, &ground, &mut self.pools);
            }
        }
        // The decay stains, of corpses lying and gone (`decay_stain`).
        if self.stain {
            acr_sim::prof_scope!("3d.seeps");
            let kept = &mut self.kept;
            if let Some(r) = self.ragdolls.as_ref() {
                self.seeps.place(&|v| Some((r.bone_world(v, "hips")?, r.bone_world(v, "neck")?)));
            }
            let mut keep = |p| kept.keep_stain(p, &ground);
            let keep: Option<&mut dyn FnMut(acr_render::model_renderer::BloodPool)> =
                if self.marks_keep { Some(&mut keep) } else { None };
            self.seeps.end(now, keep, &mut self.pools);
            if now.is_multiple_of(500) && !self.seeps.is_empty() {
                log::info!("3d: {} decay stains", self.seeps.len());
            }
        }
        self.draw_props(terrain, request);
        {
            acr_sim::prof_scope!("3d.map_objects");
            self.draw_map_objects(world, terrain, objects, request);
        }
        // Every decal lies on the slope under it rather than flat at one height (#226).
        {
            acr_sim::prof_scope!("3d.lay_decals");
            if let Some(v) = far_view {
                self.pools.retain(|p| !off_view_at(&v, terrain, p.centre[0], p.centre[1]));
            }
            super::splatter::lay_on_ground_par(&mut self.pools, &ground);
        }
        // Rain dilutes the fresh blood: wider and darker (#226).
        let wet = if self.weather() == Weather::Rain { 1.0 } else { 0.0 };
        super::splatter::soak(&mut self.pools, wet);
        // The kept marks were laid when they were kept.
        self.kept.draw(&mut self.pools);
        // The decay stains lie under all other decals (drawn first, blended over by blood).
        if self.stain || self.marks_keep {
            self.pools.sort_by_key(|p| p.dirt != acr_render::model_renderer::Dirt::Seep);
        }
        // This frame's ragdoll steps run on a worker while the frame renders.
        if let Some(r) = self.ragdolls.as_mut() {
            r.dispatch(terrain);
        }
    }

    /// `battlefield_marks=keep`: copies the settled decals of hits and impacts, and the pools
    /// of corpses gone this frame, into the kept marks (`kept_marks.rs`).
    fn keep_marks(
        &mut self,
        world: &World,
        machines: &std::collections::HashSet<u32>,
        ground: &dyn Fn(f32, f32) -> [f32; 3],
    ) {
        acr_sim::prof_scope!("3d.kept_marks");
        let now = world.tick.0;
        let mut fresh = Vec::new();
        let settled = |tick: u64| tick + super::kept_marks::SETTLED <= now;
        let since = self.kept.hits_until;
        for hit in world.hits.iter().filter(|h| h.tick > since && settled(h.tick)) {
            if !machines.contains(&hit.victim) {
                let at = hit.tick + super::kept_marks::SETTLED;
                super::splatter::splatter(hit, at, self.gore, ground, &mut fresh);
                super::debris::mist(hit, at, self.gore, ground, &mut fresh);
            }
            self.kept.hits_until = hit.tick;
        }
        let since = self.kept.impacts_until;
        for impact in world.impacts.iter().filter(|i| i.tick > since && settled(i.tick)) {
            if !self.ground_marks {
                super::splatter::impact(impact, impact.tick + super::kept_marks::SETTLED, ground, &mut fresh);
            }
            self.kept.impacts_until = impact.tick;
        }
        super::splatter::lay_on_ground(&mut fresh, ground);
        for p in fresh {
            self.kept.keep(p);
        }
        self.kept.end_corpses(ground);
        if now.is_multiple_of(500) && !self.kept.is_empty() {
            log::info!(
                "3d: {} kept marks ({} merged), {} debris",
                self.kept.len(),
                self.kept.merged,
                self.debris.len()
            );
        }
    }

    /// Particles of this frame (after [`Self::update`]): impacts and the footfalls of the
    /// ground marks throw earth, mud, dust and water; landed clods mark the ground state.
    pub fn update_particles(
        &mut self,
        world: &World,
        terrain: &TerrainData,
        marks: Option<&mut super::ground_marks::GroundMarks>,
    ) {
        let mut marks = marks;
        let mut footfalls = marks.as_mut().map(|m| std::mem::take(&mut m.footfalls)).unwrap_or_default();
        if self.models != Models::Off {
            // Smoke drifts with the wind that sways the trees (#303).
            let weather = self.weather_override.unwrap_or(self.weather);
            let drift = if self.wind_override.unwrap_or(self.wind) {
                SMOKE_DRIFT * WIND_STRENGTH * weather.wind_scale()
            } else {
                0.0
            };
            self.particles.particles.wind = [WIND_DIR[0] * drift, WIND_DIR[1] * drift];
            let water_seconds = self.started.elapsed().as_secs_f32();
            let water_view = &mut self.water_view;
            let mut ring = |at: [f32; 2]| water_view.ring(at, water_seconds);
            self.particles.update(
                world,
                self.gore == Gore::Full,
                terrain,
                marks.as_deref().map(|m| &m.state),
                &footfalls,
                self.ground_marks,
                water_detailed(self.water_detailed),
                self.frame_seconds,
                &self.shots,
                &mut ring,
            );
            let rain = self.weather() == Weather::Rain;
            self.fire.update(
                world,
                terrain,
                &mut self.particles.particles,
                marks.as_deref_mut().map(|m| &mut m.state),
                self.frame_seconds,
                rain,
            );
            self.fire.buildings(
                &self.burning,
                &mut self.particles.particles,
                marks.as_deref_mut().map(|m| &mut m.state),
                self.frame_seconds,
                rain,
            );
            self.fire.arrow_fires(&self.arrow_fires, &mut self.particles.particles, self.frame_seconds, rain);
        }
        self.shots.clear();
        self.update_cover(terrain, marks.as_deref().map(|m| &m.state));
        if let Some(m) = marks {
            self.particles.stamp_landings(&mut m.state);
            // Keep the list's capacity.
            footfalls.clear();
            m.footfalls = footfalls;
        }
    }

    /// Whether the ground cover draws: `ACR_GROUND_COVER`, else the driver's `ground_cover`,
    /// else the setting on a real ground (none under a flat backdrop); and the detailed ground
    /// with its marks, not zoomed out (`cover_view`) and no snow.
    fn cover_on(&self) -> bool {
        ground_cover_override().or(self.cover_forced).unwrap_or(self.ground_cover && !self.backdrop)
            && self.ground_marks
            && self.cover_view.is_some()
            && self.weather() != Weather::Snow
    }

    /// Places the ground cover around this frame's view (`ground_cover.rs`).
    fn update_cover(&mut self, terrain: &TerrainData, state: Option<&acr_render::ground_state::GroundState>) {
        let class = |x: f32, y: f32| acr_render::ground::class_of(terrain.tile_at(x as i32, y as i32));
        let height = |x: f32, y: f32| super::particle_view::ground_z(terrain, x, y);
        self.place_cover(&class, &height, state);
    }

    /// [`Self::update_cover`] over any ground: `class` and `height` at a world point.
    fn place_cover(
        &mut self,
        class: &dyn Fn(f32, f32) -> u8,
        height: &dyn Fn(f32, f32) -> f32,
        state: Option<&acr_render::ground_state::GroundState>,
    ) {
        let view = match self.cover_view {
            Some(v) if self.cover_on() && acr_render::ground_cover::zoom_fade(v.camera.scale) > 0.0 => v,
            _ => {
                self.cover.clear();
                return;
            }
        };
        acr_sim::prof_scope!("3d.ground_cover");
        let seconds = self.started.elapsed().as_secs_f32();
        self.cover.update(&view, seconds, class, height, state);
    }

    /// Body physics of the hit and the dying (#278): ragdolls pose their instances.
    fn update_ragdolls(
        &mut self,
        world: &World,
        terrain: &TerrainData,
        rigged: &[(usize, usize)],
        riders: &[(usize, usize)],
    ) {
        let Some(renderer) = self.renderer.as_ref() else { return };
        let ragdolls = self.ragdolls.get_or_insert_with(Default::default);
        let drawn_of = |list: &[(usize, usize)]| {
            let mut drawn = Vec::with_capacity(list.len());
            for &(u, i) in list {
                let (Some(unit), inst) = (world.units.get(u), &self.instances[i]) else { continue };
                let Some(data) = renderer.model(inst.model) else { continue };
                drawn.push(super::ragdoll_view::Drawn {
                    unit,
                    index: i,
                    model: inst.model,
                    data,
                    clip: inst.clip,
                    time: inst.time,
                    transform: inst.transform,
                });
            }
            drawn
        };
        let (drawn, mounted) = (drawn_of(rigged), drawn_of(riders));
        let hits: Vec<_> = world.hits.iter().collect();
        let killed = |v: u32| world.units.get(v as usize).is_none_or(|u| u.id.0 != v || !u.alive || u.death > 0);
        {
            acr_sim::prof_scope!("3d.ragdolls");
            for b in &self.blasts {
                acr_sim::prof_scope!("3d.blast_corpses");
                let n = ragdolls.blast(b.at, b.radius, b.corpses, b.force, world.tick.0, &drawn, &killed, terrain);
                log::info!("3d: blast at {:?} threw {n} corpses", b.at);
            }
            self.blasts.clear();
            ragdolls.update(world.tick.0, self.frame_seconds, &hits, &drawn, &mounted, self.focus, terrain, &killed);
        }
        for &i in ragdolls.poses.keys() {
            // The physics pose replaces the lying pose's flattening.
            self.instances[i].morphs[1] = 0.0;
        }
        // The skirts swing over the clip, the gait and the ragdoll (after the flattening:
        // a man on his clip keeps his morphs).
        if std::env::var_os("ACR_NO_CLOTH").is_none() {
            acr_sim::prof_scope!("3d.cloth");
            self.skirts.update(self.frame_seconds, &drawn, self.focus, terrain, &mut ragdolls.poses);
        }
        // Knapsacks, quivers and the slung musket lag and bob on the living (`ACR_NO_GEAR` off).
        if std::env::var_os("ACR_NO_GEAR").is_none() {
            acr_sim::prof_scope!("3d.gear");
            self.carried.update(self.frame_seconds, &[&drawn, &mounted], self.focus, &mut ragdolls.poses);
        }
    }

    /// The driver's trees and foliage (`props`): replaces the placed ones; `wind` overrides
    /// the setting (`None`: the setting again).
    pub fn set_props(&mut self, name: &str, at: Vec<[i32; 2]>, wind: Option<bool>) {
        let name = name.to_uppercase();
        self.props = at.into_iter().map(|p| (name.clone(), p)).collect();
        self.wind_override = wind;
    }

    /// Appends the props' instances (standing, at time 0; each turned by a hash of its place).
    fn draw_props(&mut self, terrain: &TerrainData, request: &dyn Fn(&str)) {
        for (name, [x, y]) in &self.props {
            let Some(path) = self.files.get(name.as_str()) else { continue };
            let index = match self.loaded.get(name.as_str()) {
                Some(Some(i)) => *i,
                Some(None) => continue,
                None => {
                    let key = acr_formats::normalize_path(path);
                    if !self.requested.contains_key(&key) && !self.parsed.iter().any(|(n, _)| n == name) {
                        self.requested.insert(key, name.clone());
                        request(path);
                    }
                    continue;
                }
            };
            let Some(model) = self.renderer.as_ref().and_then(|r| r.model(index)) else { continue };
            let ground = view3d::ground_point(*x, *y, terrain.height_at(*x, *y));
            let turn = (x.wrapping_mul(73_856_093) ^ y.wrapping_mul(19_349_663)).rem_euclid(256) as u8;
            self.instances.push(ModelInstance {
                model: index,
                clip: model.clip_for_animation("#STAND"),
                time: 0.0,
                transform: view3d::model_matrix(ground, view3d::facing_yaw(turn), 1.0),
                decay: 0.0,
                morphs: [0.0; 3],
                transparent: false,
                player: 0,
                wounds: Default::default(),
                sprayed: Default::default(),
                damage: Default::default(),
                look: None,
                uniform: None,
                drape: None,
                rise: None,
            });
        }
    }

    /// The map's trees and stones whose sprite type has a model (`map_models`): standing, at
    /// time 0, turned by a hash of the place and scaled to the sprite's height.
    fn draw_map_objects(
        &mut self,
        world: &World,
        terrain: &TerrainData,
        objects: &ObjectsView,
        request: &dyn Fn(&str),
    ) {
        if !self.map_models.on || world.objects.is_empty() {
            return;
        }
        let mut ready: HashMap<String, (usize, f32)> = HashMap::new();
        let names: Vec<String> = self.map_models.names().into_iter().map(str::to_owned).collect();
        for name in names {
            let Some(path) = self.files.get(name.as_str()) else { continue };
            match self.loaded.get(name.as_str()) {
                Some(Some(i)) => {
                    let Some(model) = self.renderer.as_ref().and_then(|r| r.model(*i)) else { continue };
                    let height = *self.map_heights.entry(*i).or_insert_with(|| model.rest_height());
                    ready.insert(name, (*i, height));
                }
                Some(None) => {}
                None => {
                    let key = acr_formats::normalize_path(path);
                    if !self.requested.contains_key(&key) && !self.parsed.iter().any(|(n, _)| *n == name) {
                        self.requested.insert(key, name.clone());
                        request(path);
                    }
                }
            }
        }
        self.map_models.place(&world.objects, &ready, objects.objects(), objects.ready(), terrain);
        let Some(renderer) = self.renderer.as_ref() else { return };
        let [x0, y0, x1, y1] = self.map_view.unwrap_or([f32::MIN, f32::MIN, f32::MAX, f32::MAX]);
        let mut clips: HashMap<usize, Option<usize>> = HashMap::new();
        let now = world.tick.0 as f32;
        for (n, &(index, transform)) in self.map_models.placed.iter().enumerate() {
            let [x, y] = [transform[3][0], transform[3][1]];
            if x < x0 || x > x1 || y < y0 || y > y1 {
                continue;
            }
            // Felled trees, splintered trunks, chipped rocks and knocked stones (rule
            // `ball_terrain`): the model's struck states where it has them (docs/assets.md
            // "Struck map objects"), else the standing model tipped over or moved.
            let struck = self.map_models.objs.get(n).and_then(|k| world.struck.get(&(*k as u32)));
            let parts = self.struck_parts.get(&index).copied().unwrap_or_default();
            let mut shown: [Option<(usize, acr_render::math::Mat4)>; 2] = [Some((index, transform)), None];
            match struck {
                Some(&acr_sim::ball_terrain::Struck::Felled { dir, tick })
                    if parts.stump.is_some() && parts.felled.is_some() =>
                {
                    shown[0] = parts.stump.map(|s| (s, transform));
                    // Its reach: the standing model's radius (the parts keep it; world pixels at
                    // scale 1) at its scale (the matrix maps metres to world pixels).
                    let stretch = (transform[0][0].powi(2) + transform[0][1].powi(2) + transform[0][2].powi(2)).sqrt()
                        / acr_render::view3d::PIXELS_PER_METRE;
                    let length = renderer.model(index).map_or(0.0, |m| m.radius) * stretch;
                    shown[1] = parts.felled.map(|f| {
                        let m = super::map_models::felled_transform(transform, dir, tick, now, length, |x, y| {
                            terrain.height_at(x, y)
                        });
                        (f, m)
                    });
                }
                Some(&acr_sim::ball_terrain::Struck::Splintered { dir, .. }) => {
                    shown[1] = parts.scar.map(|s| (s, super::map_models::facing(transform, dir)));
                }
                Some(&acr_sim::ball_terrain::Struck::Chipped { dir, .. }) => {
                    // The marks built where a ball of this heading meets the rock (turning one
                    // would lift it off a rock that is not round); older models have one set,
                    // on their -X face.
                    let side = super::map_models::chip_side(transform, dir);
                    shown[1] = parts.chips[side].or(parts.chip).map(|c| (c, transform));
                }
                Some(_) => {
                    let k = self.map_models.objs[n];
                    let t = super::map_models::struck_transform(transform, k, &world.struck, now, |x, y| {
                        terrain.height_at(x, y)
                    });
                    shown[0] = Some((index, t));
                }
                None => {}
            }
            for (index, transform) in shown.into_iter().flatten() {
                let clip = match clips.get(&index) {
                    Some(c) => *c,
                    None => {
                        let Some(model) = renderer.model(index) else { continue };
                        *clips.entry(index).or_insert(model.clip_for_animation("#STAND"))
                    }
                };
                self.instances.push(ModelInstance {
                    model: index,
                    clip,
                    time: 0.0,
                    transform,
                    decay: 0.0,
                    morphs: [0.0; 3],
                    transparent: false,
                    player: 0,
                    wounds: Default::default(),
                    sprayed: Default::default(),
                    damage: Default::default(),
                    look: None,
                    uniform: None,
                    drape: None,
                    rise: None,
                });
            }
        }
    }

    /// The model textures' GPU memory (`drive` `textures`).
    pub fn texture_stats(&self) -> Option<acr_render::model_renderer::TextureStats> {
        self.renderer.as_ref().map(ModelRenderer::texture_stats)
    }

    /// `map_models` on or off (the setting, or the driver).
    pub fn set_map_models(&mut self, on: bool) {
        self.map_models.on = on;
    }

    /// The sprite frames now drawn as models, once they changed (for
    /// [`ObjectsView::set_hidden`]).
    pub fn take_map_hidden(&mut self) -> Option<HashSet<acr_render::scene::FrameKey>> {
        std::mem::take(&mut self.map_models.hidden_changed).then(|| self.map_models.hidden.clone())
    }

    /// Asks for `gore.glb` (gibs, guts, the stump cap) once gore is shown.
    fn request_gore(&mut self, request: &dyn Fn(&str)) {
        if self.gore == Gore::Off || self.loaded.contains_key(GORE) || self.parsed.iter().any(|(n, _)| n == GORE) {
            return;
        }
        let Some(path) = self.files.get(GORE) else { return };
        let key = acr_formats::normalize_path(path);
        if let std::collections::hash_map::Entry::Vacant(e) = self.requested.entry(key) {
            e.insert(GORE.to_owned());
            request(path);
        }
    }

    /// Draws the frame's 3D pass over the terrain already in `view`: the terrain's depth,
    /// the models, then the sprites depth-tested against the models.
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        size: [f32; 2],
        camera: &Camera,
        ground: Option<&TerrainRenderer>,
        terrain: Option<&TerrainData>,
        objects: &mut ObjectsView,
        world_fog: Option<WorldFog<'_>>,
    ) {
        let models_phase = self.models_phase();
        // The free camera's fog of war on the world (#309): the fixed camera keeps the
        // original's screen overlay.
        let world_fog = world_fog.and_then(|(f, grid)| {
            f.clear();
            grid.filter(|_| self.free).map(|g| (f, g))
        });
        let (Some(renderer), Some(depth), Some(terrain)) = (&mut self.renderer, &mut self.depth, terrain) else {
            objects.render(device, queue, encoder, view, size, camera);
            return;
        };
        let view3d = View3d { camera: *camera, size, orbit: self.free.then_some(self.orbit) };
        let world_fog = world_fog.map(|(f, (width, cells))| {
            acr_sim::prof_scope!("render.world_fog");
            ground_fog(&mut self.fog_cells, width, cells, terrain);
            (f, (width, &self.fog_cells[..]))
        });
        let shown = if models_phase { &self.instances[..] } else { &[] };
        if models_phase {
            // Ragdolls and the gun crews' masked poses (#287).
            let mut poses = self.crews.poses.clone();
            if let Some(r) = self.ragdolls.as_mut() {
                poses.extend(std::mem::take(&mut r.poses));
            }
            renderer.set_custom_poses(poses);
            renderer.set_scars(self.scars.clone());
        }
        renderer.shadows = shadows_override().unwrap_or(self.shadows);
        renderer.lods = self.lod;
        renderer.material_detail = material_detail_override().unwrap_or(self.material_detail);
        let weather = self.weather_override.unwrap_or(self.weather);
        let storm = storm_at(weather, self.wind_clock);
        let look = weather.look();
        let strength = if self.wind_override.unwrap_or(self.wind) { WIND_STRENGTH * weather.wind_scale() } else { 0.0 };
        renderer.wind = [WIND_DIR[0], WIND_DIR[1], strength, self.wind_clock];
        renderer.weather = [look.model(), look.model_fog()];
        if let Some(ground) = ground {
            ground.set_weather(queue, look.model());
        }
        {
            acr_sim::prof_scope!("render.models_prepare");
            renderer.prepare(device, queue, &view3d, shown, self.gore == Gore::Full);
            let [staging, layout, group, upload] = renderer.prepare_ns;
            acr_sim::prof::add("render.mp_staging", staging);
            acr_sim::prof::add("render.mp_layout", layout);
            acr_sim::prof::add("render.mp_group_draws", group);
            acr_sim::prof::add("render.mp_upload", upload);
        }
        renderer.prepare_pools(device, queue, if models_phase { &self.pools[..] } else { &[] });
        let sprites = {
            let sprite_fog = world_fog.as_ref().map(|(f, grid)| {
                let (f, grid): (&WorldFogRenderer, _) = (f, *grid);
                move |x: i32, y: i32| f.black_at(grid, x as f32, y as f32)
            });
            let fog = sprite_fog.as_ref().map(|f| f as &dyn Fn(i32, i32) -> f32);
            let on_top = self.compare == Compare::Overlay;
            objects.prepare(device, queue, size, camera, Some(Placement { view: &view3d, terrain, on_top, fog }))
        };
        let water = match self.water.as_mut() {
            Some(w) if !objects.zoomed && (self.free || water_detailed(self.water_detailed)) => {
                let seconds = self.started.elapsed().as_secs_f32();
                let detailed = water_detailed(self.water_detailed);
                let none = Disturbances::default();
                let disturbances = if detailed { &self.water_view.disturbances } else { &none };
                w.prepare(device, queue, terrain, &view3d, seconds, detailed, disturbances).then_some(&*w)
            }
            _ => None,
        };
        if let Some(particles) = self.particle_renderer.as_mut() {
            particles.prepare(queue, &view3d, &mut self.particles.particles);
        }
        if let Some(c) = self.cover_renderer.as_mut() {
            let on = self.cover_view.is_some() && !objects.zoomed;
            c.prepare(queue, &view3d, on.then_some(&mut self.cover), renderer.wind);
        }
        if let Some(w) = self.weather_renderer.as_mut() {
            w.set_storm(storm.0, storm.1);
            w.prepare(queue, &view3d, weather, self.started.elapsed().as_secs_f32(), WIND_DIR);
        }
        use acr_render::gpu_timer::mark_encoder;
        // The GPU timer (`gputime`) splits the 3D pass at each segment: timestamps inside a
        // pass are not reliable on tiling GPUs. Otherwise it is one pass, as always.
        let split = acr_render::gpu_timer::active();
        mark_encoder(encoder, "3d prepare");
        renderer.render_shadows(encoder);
        mark_encoder(encoder, "shadow map");
        let mut pass = depth.begin(device, encoder, view, size, split || world_fog.is_some(), false);
        macro_rules! segment {
            ($label:expr) => {
                if split {
                    drop(pass);
                    mark_encoder(encoder, $label);
                    pass = depth.begin(device, encoder, view, size, true, true);
                }
            };
        }
        if let Some(ground) = ground {
            ground.render_depth(&mut pass);
        }
        segment!("terrain depth");
        renderer.render(&mut pass);
        segment!("models");
        if let Some(ground) = ground {
            ground.render_shadows(device, &mut pass, renderer.shadow());
        }
        segment!("ground shadows");
        if let Some(c) = &self.cover_renderer {
            c.render(&mut pass);
        }
        segment!("ground cover");
        if let Some(water) = water {
            water.render(&mut pass);
        }
        segment!("water");
        if let Some(particles) = &self.particle_renderer {
            particles.render(&mut pass);
        }
        segment!("particles");
        if let Some(w) = &self.weather_renderer {
            w.render_mist(&mut pass);
        }
        segment!("weather mist");
        if let Some((fog, grid)) = world_fog {
            // It reads the depth of the ground and the models, so between passes and before
            // the ground reset; sprites drawn after it take their shade in `objects`.
            drop(pass);
            if let Some(d) = depth.depth_view() {
                fog.prepare(device, queue, &view3d, Some(grid), d);
                fog.render(encoder, view);
            }
            mark_encoder(encoder, "world fog");
            pass = depth.begin(device, encoder, view, size, split, true);
        }
        depth.reset_ground(&mut pass);
        if sprites {
            objects.draw(&mut pass, true);
        }
        segment!("sprites");
        if let Some(w) = &self.weather_renderer {
            w.render(&mut pass);
        }
        drop(pass);
        mark_encoder(encoder, "weather");
        self.frames += 1;
        let now = web_time::Instant::now();
        let since = *self.since.get_or_insert(now);
        let elapsed = (now - since).as_secs_f32();
        // The 5 s line goes with the `ACR_PROFILE` block; the recorder's 1 Hz `perf:` line
        // carries the same counts (docs/testing.md "Performance recorder").
        if elapsed >= 5.0 && acr_sim::prof::enabled() {
            log::info!(
                "3d: {:.1} fps, {} models drawn ({} poses) of {}, {}k triangles ({} draws), {}k cast ({} draws), camera {:?}",
                self.frames as f32 / elapsed,
                renderer.drawn,
                renderer.pose_count,
                self.instances.len(),
                renderer.triangles[0] / 1000,
                renderer.calls[0],
                renderer.triangles[1] / 1000,
                renderer.calls[1],
                view3d.orbit
            );
        }
        if elapsed >= 5.0 {
            self.frames = 0;
            self.since = Some(now);
        }
    }

    /// The last frame's 3D counts for the performance recorder (`crate::perf`): cheap
    /// (counters kept anyway, and one pass over the draws and ragdolls).
    pub fn perf_counts(&self, c: &mut crate::perf::Counts) {
        c.instances = self.instances.len() as u32;
        c.particles = self.particles.particles.live() as u32;
        c.decals = self.pools.len() as u32;
        c.ragdolls_awake = self.ragdolls.as_ref().map_or(0, |r| r.awake() as u32);
        c.free_camera = self.free;
        c.models_phase = self.models_phase();
        if let Some(r) = &self.renderer {
            c.drawn = r.drawn as u32;
            c.poses = r.pose_count as u32;
            c.triangles = r.triangles[0].min(u64::from(u32::MAX)) as u32;
            c.draws = r.calls[0] as u32;
            c.shadow_triangles = r.triangles[1].min(u64::from(u32::MAX)) as u32;
            c.shadow_draws = r.calls[1] as u32;
            c.lod = r.lod_counts();
            c.shadows = r.shadows;
        }
    }
}

impl ThreeD {
    /// How charred unit `index` is (0..1): grows while he burns, stays on the corpse of the
    /// burned (`fire_view.rs`). For the model's burn tint (#207).
    #[allow(dead_code)]
    pub fn burn(&self, index: u32) -> f32 {
        self.fire.burn(index)
    }

    /// The rain's strength and the lightning flash now (`acr_render::weather`), both on the
    /// wind clock, as `weather_sound` hears them.
    pub fn storm(&self) -> (f32, f32) {
        storm_at(self.weather(), self.wind_clock)
    }

    /// The wind clock (seconds, wraps at [`WIND_PERIOD`]) the trees sway and the storm runs
    /// on.
    pub fn wind_clock(&self) -> f32 {
        self.wind_clock
    }

    /// The trees' wind at world point `at` now: its strength (0 with `wind=off`) times the
    /// gust there, as `model.wgsl` sways them, in units of [`WIND_STRENGTH`].
    pub fn wind_at(&self, at: [f32; 2]) -> f32 {
        if !self.wind_override.unwrap_or(self.wind) {
            return 0.0;
        }
        let strength = self.weather().wind_scale();
        strength * acr_render::weather::gust(at, WIND_DIR, self.wind_clock)
    }

    /// The weather drawn: the driver's override, else the setting.
    pub fn weather(&self) -> Weather {
        self.weather_override.unwrap_or(self.weather)
    }

    /// The driver's `weather` request (`None`: back to the setting).
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub fn set_weather_override(&mut self, weather: Option<Weather>) {
        self.weather_override = weather;
    }

    /// True when this frame's 3D pass draws the water surface in place of the terrain's
    /// depth tint (`water=detailed`, models on and their renderer up, not zoomed out).
    pub fn draws_water(&self, zoomed: bool) -> bool {
        self.active() && self.renderer.is_some() && !zoomed && water_detailed(self.water_detailed)
    }
}

/// `water=detailed` unless `ACR_WATER=original` (native) says otherwise, or the reverse.
fn water_detailed(setting: bool) -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    match std::env::var("ACR_WATER").as_deref() {
        Ok("original") => return false,
        Ok("detailed") => return true,
        _ => {}
    }
    setting
}

/// Shadows forced on (2) or off (1) for tests, over the setting: `ACR_SHADOWS=on|off` or
/// the driver's `{"cmd": "shadows", "on": false}`; 0 follows the setting.
static SHADOWS_OVERRIDE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(u8::MAX);

/// Forces shadows on or off (`None`: the setting decides).
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub fn set_shadows_override(on: Option<bool>) {
    let v = match on {
        None => 0,
        Some(false) => 1,
        Some(true) => 2,
    };
    SHADOWS_OVERRIDE.store(v, std::sync::atomic::Ordering::Relaxed);
}

/// `ACR_GROUND_COVER=on|off` (native only) over the `ground_cover` setting, for tests.
fn ground_cover_override() -> Option<bool> {
    static ENV: std::sync::OnceLock<Option<bool>> = std::sync::OnceLock::new();
    *ENV.get_or_init(|| {
        let env = if cfg!(target_arch = "wasm32") { None } else { std::env::var("ACR_GROUND_COVER").ok() };
        match env.as_deref() {
            Some("off" | "0") => Some(false),
            Some("on" | "1") => Some(true),
            _ => None,
        }
    })
}

fn shadows_override() -> Option<bool> {
    use std::sync::atomic::Ordering;
    if SHADOWS_OVERRIDE.load(Ordering::Relaxed) == u8::MAX {
        // First use: the environment (native only; the browser has none).
        let env = if cfg!(target_arch = "wasm32") { None } else { std::env::var("ACR_SHADOWS").ok() };
        set_shadows_override(match env.as_deref() {
            Some("off" | "0") => Some(false),
            Some("on" | "1") => Some(true),
            _ => None,
        });
    }
    match SHADOWS_OVERRIDE.load(Ordering::Relaxed) {
        1 => Some(false),
        2 => Some(true),
        _ => None,
    }
}

/// Material detail forced on (2) or off (1) for tests, over the setting:
/// `ACR_MATERIAL_DETAIL=on|off` or the driver's `{"cmd": "material_detail", "on": false}`; 0
/// follows the setting.
static MATERIAL_DETAIL_OVERRIDE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(u8::MAX);

/// Forces the material detail on or off (`None`: the setting decides).
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub fn set_material_detail_override(on: Option<bool>) {
    let v = match on {
        None => 0,
        Some(false) => 1,
        Some(true) => 2,
    };
    MATERIAL_DETAIL_OVERRIDE.store(v, std::sync::atomic::Ordering::Relaxed);
}

fn material_detail_override() -> Option<bool> {
    use std::sync::atomic::Ordering;
    if MATERIAL_DETAIL_OVERRIDE.load(Ordering::Relaxed) == u8::MAX {
        let env = if cfg!(target_arch = "wasm32") { None } else { std::env::var("ACR_MATERIAL_DETAIL").ok() };
        set_material_detail_override(match env.as_deref() {
            Some("off" | "0") => Some(false),
            Some("on" | "1") => Some(true),
            _ => None,
        });
    }
    match MATERIAL_DETAIL_OVERRIDE.load(Ordering::Relaxed) {
        1 => Some(false),
        2 => Some(true),
        _ => None,
    }
}

/// `ACR_DEBUG_DAMAGE` (native, `docs/rendering-3d.md`): forces damage on the 3D humans to
/// show the renderer without the game logic. A spec of [`Damage::parse`] for all, or `demo`
/// for a preset per unit (by id): cut arm, cut thigh, fractures, torn in two, headless with
/// the belly open, hand and foot off.
fn debug_damage(unit: u32) -> Option<Damage> {
    static SPEC: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    let spec = SPEC.get_or_init(|| std::env::var("ACR_DEBUG_DAMAGE").ok()).as_deref()?;
    const DEMO: [&str; 6] = [
        "cut:arm_l:160",
        "cut:leg_r:70",
        "frac:leg_l:170:55,frac:arm_r:150:-70",
        "trunk:110",
        "head,belly",
        "cut:arm_r:215,cut:leg_l:228",
    ];
    let spec = if spec == "demo" { DEMO[unit as usize % DEMO.len()] } else { spec };
    Damage::parse(spec).ok()
}

/// The severed pieces of a body under `ACR_DEBUG_DAMAGE`, laid beside it (game placement is
/// #262 S5).
fn debug_pieces(body: &ModelInstance, damage: Damage) -> Vec<ModelInstance> {
    let beside = |x: f32, z: f32| {
        let mut m = acr_render::math::IDENTITY;
        m[3] = [x, 0.0, z, 1.0];
        acr_render::math::mul(&body.transform, &m)
    };
    let mut out = Vec::new();
    if !matches!(damage.keep, Keep::Body) {
        return out;
    }
    for limb in Limb::ALL {
        if let Some(piece) = damage.piece(limb) {
            let x = if matches!(limb, Limb::ArmL | Limb::LegL) { 0.7 } else { -0.7 };
            out.push(ModelInstance { damage: piece, transform: beside(x, 0.5), ..*body });
        }
    }
    if damage.trunk_cut.is_some() {
        out.push(ModelInstance {
            damage: Damage { keep: Keep::Upper, ..damage },
            transform: beside(0.9, 0.0),
            ..*body
        });
    }
    out
}

/// Clip time of a model on the game clock (`acr_clock`, ships): alive, its clip loops, each
/// unit at its own phase; dead, the clip (sinking) runs from the death on and holds its end.
fn world_time(data: &ModelData, clip: Option<usize>, u: &Unit, now: u64, died: u64) -> f32 {
    let tps = acr_sim::ORIGINAL_TICKS_PER_SECOND as f32;
    if u.death > 0 || !u.alive {
        return now.saturating_sub(died) as f32 / tps;
    }
    let duration = clip.and_then(|c| data.clips.get(c)).map_or(0.0, |c| c.duration);
    if duration <= 0.0 {
        return 0.0;
    }
    let phase = (u.id.0 % 97) as f32 * 0.61;
    (now as f32 / tps + phase) % duration
}

/// The state group a node of a building model stands for (docs/assets.md "States"): the
/// name's last part when it is `finished`, `ruin`, `build<k>` or `section<s>_<k>`.
fn state_group(name: &str) -> Option<&str> {
    let last = name.rsplit('.').next()?;
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let section =
        last.strip_prefix("section").and_then(|s| s.split_once('_')).is_some_and(|(a, b)| digits(a) && digits(b));
    (name.contains('.')
        && (last == "finished" || last == "ruin" || last.strip_prefix("build").is_some_and(digits) || section))
        .then_some(last)
}

/// Node name parts of a map model's struck state groups (rule `ball_terrain`, docs/assets.md
/// "Struck map objects"), in [`StruckParts`] order: stump (and its break), the felled trunk and
/// crown (and its foot), the splinter scar, the chip marks.
const STRUCK_GROUPS: [&str; 4] = [".stump", ".felled", ".scar", ".chip"];

/// The struck states of a map model as models of their own.
#[derive(Clone, Copy, Default)]
struct StruckParts {
    stump: Option<usize>,
    felled: Option<usize>,
    scar: Option<usize>,
    /// All chip marks (models with one set only).
    chip: Option<usize>,
    /// The chip marks by ball heading ([`super::map_models::chip_side`]).
    chips: [Option<usize>; super::map_models::CHIP_SIDES],
}

/// The construction stage and section state groups of a building model (#302) as models of
/// their own: `build[k]` for `#BUILDLO_k`, sections as (section, state, model).
#[derive(Default)]
struct BuildingStates {
    build: [Option<usize>; 4],
    sections: Vec<(u8, u16, usize)>,
}

/// The model name of an object type: its `.MD` `NAME` where a model of that name exists
/// (the Spanish mines `SpnCoa`, `SpnIro`, `SpnStn` are `SpnRudCo`, `SpnRudIr`, `SpnRudSt`),
/// else its id's name.
fn model_name(files: &HashMap<String, String>, t: &acr_sim::ObjectType) -> String {
    let md = t.md_name.to_ascii_uppercase();
    if !md.is_empty() && files.contains_key(md.as_str()) { md } else { definition_name(&t.id) }
}

/// A building that stands finished (not under construction, not falling down).
fn finished(world: &World, u: &Unit) -> bool {
    u.stage >= world.max_stage(u) && !matches!(u.anim, AnimKind::BuildLo(_) | AnimKind::Death | AnimKind::DeathLie(_))
}

/// [`view3d::unit_ground_point`] on the drawn ground: the model stands at the height of the
/// terrain surface under its feet ([`TerrainData::surface`]), not the sim's rounded height of
/// its anchor, so its feet, the gait's planted feet and a corpse meet the ground drawn.
fn unit_ground(terrain: &TerrainData, x: i32, y: i32) -> [f32; 3] {
    let mut g = view3d::unit_ground_point(x, y, 0);
    g[2] = terrain.surface(g[0], g[1]) / view3d::COS_E;
    g
}

/// [`view3d::ground_point`] at the height of the drawn ground (world pixels, fractional).
fn surface_point(terrain: &TerrainData, x: f32, y: f32) -> [f32; 3] {
    let mut g = view3d::ground_point(x as i32, y as i32, 0);
    g[2] = terrain.surface(x, y) / view3d::COS_E;
    g
}

/// Whether `u` stands well off `view`: half a window beyond its edges (models reach, their
/// shadows fall and a scrolling view moves a frame on before they show).
fn off_view(view: &View3d, terrain: &TerrainData, u: &Unit) -> bool {
    let (x, y) = (u.pos.x >> 4, u.pos.y >> 4);
    off_projected(view, view.project(view3d::unit_ground_point(x, y, terrain.height_at(x, y))))
}

/// [`off_view`] for a point on the ground (world pixels): a decal or a piece of debris.
fn off_view_at(view: &View3d, terrain: &TerrainData, x: f32, y: f32) -> bool {
    let (xi, yi) = (x as i32, y as i32);
    off_projected(view, view.project(view3d::ground_point(xi, yi, terrain.height_at(xi, yi))))
}

fn off_projected(view: &View3d, p: [f32; 3]) -> bool {
    let [w, h] = view.size;
    let m = 0.5 * w.max(h) + 200.0 * view.camera.scale;
    p[0] < -m || p[0] > w + m || p[1] < -m || p[1] > h + m || !(0.0..=1.0).contains(&p[2])
}

/// Off-view culling of the living (`ACR_CULL=off` keeps every instance; native only).
fn cull_enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| cfg!(target_arch = "wasm32") || !matches!(std::env::var("ACR_CULL").as_deref(), Ok("off" | "0")))
}

/// What is left of `u`'s body (#262: cuts, fractures, a torn trunk, an open belly) with the
/// tissue torn out by his hits (#280; a limb whose whole cross-section is gone comes off
/// inside the hole), and whether his belly lies open; `None` when pulverized (nothing whole
/// to draw, only gibs and mist).
fn body_damage(
    gore: Gore,
    wounds: &super::living_wounds::LivingWounds,
    model: &ModelData,
    u: &Unit,
    record: &acr_sim::combat::DeathRecord,
) -> Option<(Damage, bool)> {
    let living = u.alive && u.death == 0;
    let mut damage = match (gore, model.rig.is_some(), living) {
        (Gore::Off, ..) | (_, false, _) => Damage::default(),
        (_, true, true) => super::damage_view::damage(&u.body, None)?,
        (_, true, false) => super::damage_view::damage(&record.body, record.torn)?,
    };
    let belly_open = damage.belly_open;
    if gore == Gore::Full
        && !u.building
        && let Some(rig) = model.rig.as_ref()
    {
        for v in wounds.volumes(u.id.0, rig) {
            damage.push_volume(v);
        }
        damage.sever(rig);
    } else if gore == Gore::Full
        && let Some(rig) = model.beast_rig.as_ref()
    {
        // An animal: torn but never cut.
        for v in wounds.volumes(u.id.0, rig) {
            damage.push_volume(v);
        }
    }
    Some((damage, belly_open))
}

/// The model instance of a unit: the clip of its sprite animation at its sprite frame, its
/// facing, and as a corpse its decay (or the original's fade with `gore=off`); and its
/// ground point (sunk with the decay).
#[allow(clippy::too_many_arguments)]
fn instance(
    u: &Unit,
    death: u16,
    model: usize,
    data: &ModelData,
    terrain: &TerrainData,
    gore: Gore,
    removed_at: u16,
    wounds: Wounds,
) -> (ModelInstance, [f32; 3]) {
    let anim = u.anim.name();
    let clip = data.clip_for_animation(&anim);
    let time = if u.building { 0.0 } else { u.frame.max(0) as f32 / data.fps };
    let (x, y) = (u.pos.x >> 4, u.pos.y >> 4);
    let lying = matches!(u.anim, AnimKind::DeathLie(_));
    let (decay, transparent) = match gore {
        Gore::Off => (0.0, u.death > CORPSE_FADE),
        Gore::Full | Gore::Reduced => (model::corpse_decay(death, removed_at), false),
    };
    let (morphs, sink) = model::decay_pose(decay, lying);
    // Units stand where their sprites draw their feet (#196), buildings on their anchor.
    let mut ground =
        if u.building { view3d::ground_point(x, y, terrain.height_at(x, y)) } else { unit_ground(terrain, x, y) };
    // A model without decay masks sinks in the shader (after the palette, so a physics
    // pose sinks too); the footprint still sinks here.
    let placed = ground;
    ground[2] -= sink * PIXELS_PER_METRE;
    let placed = if data.decays_procedurally() { placed } else { ground };
    // Buildings have one view, facing south.
    let yaw = if u.building { 0.0 } else { view3d::facing_yaw(u.dir) };
    let instance = ModelInstance {
        model,
        clip,
        time,
        transform: view3d::model_matrix(placed, yaw, 1.0),
        decay,
        morphs: [morphs[0], morphs[1], 0.0],
        transparent,
        player: u.owner,
        wounds,
        sprayed: Default::default(),
        damage: Default::default(),
        // Each soldier his own face, build, hair and height (#283).
        look: (!u.building).then_some(u.id.0),
        uniform: None,
        drape: None,
        rise: None,
    };
    (instance, ground)
}

/// A human's body lying on the ground (metres): length, width, thickness, and how far its
/// centre lies ahead of the ground point (towards the head; the chest is 0.55 m ahead,
/// `gore.rs`).
const BODY: [f32; 4] = [1.7, 0.5, 0.25, 0.1];
/// The rest-pose radius of a musketeer with his musket (`engm18.glb`, world pixels). Larger
/// models (horses) get a larger footprint, up to 1.8 times: a mounted model's radius grows
/// less than its lying body (2.6 m for `engd18.glb`, a horse lies ~2.4 m long).
const HUMAN_RADIUS: f32 = 2.2 * PIXELS_PER_METRE;

/// The terrain under the samples of `f` over the terrain under its ground point `at` (#229;
/// `terrain`: the ground's height at a point).
fn pile_ground(f: &piles::Footprint, at: [f32; 2], terrain: &dyn Fn(f32, f32) -> f32) -> [f32; 8] {
    let base = terrain(at[0], at[1]);
    let [s, c] = f.along;
    piles::SAMPLES.map(|(sa, sb)| {
        let (a, b) = (sa * f.half_length, sb * f.half_width);
        terrain(f.centre[0] + a * s - b * c, f.centre[1] + a * c + b * s) - base
    })
}

/// The footprint of a corpse for stacking (#221): thinner as it deflates and sinks. `died`
/// is the tick of death (`DeathRecord::tick`); the terrain under it is [`pile_ground`].
fn footprint(u: &Unit, died: u64, ground: [f32; 3], inst: &ModelInstance, radius: f32) -> piles::Footprint {
    let size = (1.0 + 2.5 * (radius / HUMAN_RADIUS - 1.0)).clamp(1.0, 1.8) * PIXELS_PER_METRE;
    let (s, c) = view3d::facing_yaw(u.dir).sin_cos();
    let [length, width, thick, ahead] = BODY.map(|v| v * size);
    // Deflating takes about two thirds of the body; the sink already lowered `ground`.
    let thickness = thick * (1.0 - 0.65 * inst.morphs[0]);
    piles::Footprint {
        centre: [ground[0] + s * ahead, ground[1] + c * ahead, ground[2]],
        along: [s, c],
        half_length: length / 2.0,
        half_width: width / 2.0,
        thickness,
        // Died earlier lies lower; the tick, unlike the death counter, stays put (#230).
        order: (u32::try_from(died).unwrap_or(u32::MAX), u.id.0),
        ground: [0.0; 8],
    }
}

impl super::InGame {
    /// With the free camera turned, scrolling (arrow keys, the window edge) moves along the
    /// screen instead of the world axes the game scrolls in.
    pub(super) fn turn_scrolling(&mut self, before: Camera, window: [f32; 2]) {
        let Some(view) = self.three_d.free_view(before, window) else { return };
        if self.camera.scale != before.scale {
            return;
        }
        let s = before.scale;
        let delta = [(self.camera.x - before.x) * s, (self.camera.y - before.y) * s];
        if delta != [0.0, 0.0] {
            self.camera = view.pan(delta);
        }
    }

    /// Turns the free camera and collects the models of this frame.
    pub(super) fn update_models(&mut self, seconds: f32, window: [f32; 2]) {
        self.three_d.tick(seconds);
        let (Some(world), Some(terrain), true) = (&self.world, &self.terrain, self.objects.ready()) else { return };
        // The world point at the window centre: ragdolls go to the hits nearest to it.
        let [w, h] = window;
        let c = self.camera;
        self.three_d.focus = [c.x + w / (2.0 * c.scale), 2.0 * (c.y + h / (2.0 * c.scale))];
        // Map objects whose ground point is up to `MAP_VIEW_MARGIN` beyond what the view shows of
        // ground up to `MAP_VIEW_TOP` high: tall crowns and long shadows reach in.
        let view = View3d { camera: c, size: window, orbit: self.three_d.orbit() };
        self.three_d.cover_view = (!self.interface.hud.zoom).then_some(view);
        self.three_d.cull_view = Some(view);
        self.three_d.backdrop = self.backdrop().is_some();
        let [x0, y0, x1, y1] = view.ground_bounds(-100.0, MAP_VIEW_TOP);
        self.three_d.map_view =
            Some([x0 - MAP_VIEW_MARGIN, y0 - MAP_VIEW_MARGIN, x1 + MAP_VIEW_MARGIN, y1 + MAP_VIEW_MARGIN]);
        let requests = &self.requests;
        {
            acr_sim::prof_scope!("3d.update");
            self.three_d.update(world, terrain, &self.objects, &|p| requests.borrow_mut().push(p.to_owned()));
        }
        if let Some(hidden) = self.three_d.take_map_hidden() {
            self.objects.set_hidden(hidden);
        }
        acr_sim::prof_scope!("3d.particles");
        self.three_d.update_particles(world, terrain, self.ground_marks.as_mut());
    }
}

/// Where wounds go on a model: its skeleton-H rig, else the rig guessed from an animal's
/// skeleton (marks and torn volumes only).
fn wound_rig(model: &acr_render::model::ModelData) -> Option<&acr_render::damage::Rig> {
    model.rig.as_ref().or(model.beast_rig.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::setup::Ground;

    /// The blank field's ground under the view at its centre, as `InGame` sets it up.
    fn blank_cover(ground: Ground, forced: Option<bool>) -> usize {
        let mut t = ThreeD { ground_marks: true, cover_forced: forced, ..Default::default() };
        let camera = Camera { x: 7680.0 - 800.0, y: 3840.0 - 450.0, scale: 1.0 };
        t.cover_view = Some(View3d { camera, size: [1600.0, 900.0], orbit: None });
        t.backdrop = crate::blank_map::backdrop(ground).is_some();
        let class = acr_render::ground::class_of(crate::blank_map::texture(ground));
        t.place_cover(&|_, _| class, &|_, _| 0.0, None);
        t.cover.instances().0.len()
    }

    #[test]
    fn no_ground_cover_on_a_backdrop() {
        if ground_cover_override().is_some() {
            return;
        }
        // Grass grows on the green field; white and greenscreen are that field under a flat
        // colour, so nothing grows there unless the driver asks for it.
        assert!(blank_cover(Ground::Grass, None) > 0);
        assert_eq!(blank_cover(Ground::White, None), 0);
        assert_eq!(blank_cover(Ground::Greenscreen, None), 0);
        assert!(blank_cover(Ground::White, Some(true)) > 0);
        assert_eq!(blank_cover(Ground::Grass, Some(false)), 0);
    }
}
