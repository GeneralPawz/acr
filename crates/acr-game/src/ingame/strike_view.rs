//! Aimed strikes (`docs/melee.md`, Animation): an action layer over the attack clips, render
//! side only. A man (on foot or mounted) who attacks a unit plays the strike clip for the
//! blow (`strike_clip`: by the sim's strike kind and height), bends his trunk so that his
//! weapon points at the victim's body, steps in when the victim is out of reach and puts his
//! weapon arm onto the target by two-bone IK, so the point meets him: a standing man's belly,
//! a rider's chest, a crawling or lying man on the ground. On foot he also sinks into his
//! knees to reach the ground.
//!
//! - The weapon's axis is read from the clip: the musket's `socket.muzzle` or a held
//!   weapon's `socket.tip` (a lance's point, a drawn sabre's) from the hand's socket, its
//!   distance the reach; else the held weapon's own axis (the first node under
//!   `socket.hand_r`) with [`REACH`].
//! - The bend is the rotation taking that point onto the target as seen from the lower spine,
//!   shared between the lower spine and the chest and clamped ([`MAX_BEND`]); it fades in
//!   and out with the attack stance.
//! - The height of the blow is the sim's (`HitRecord::strike` of this attacker's latest blow
//!   at this victim), else what the renderer sees of the victim.

use std::collections::HashMap;

use acr_physics::gait::{self, Rig, Skin};
use acr_physics::{Quat, Vec3, skeleton_h};
use acr_render::math::Mat4;
use acr_render::model::{ModelData, Node};
use acr_sim::combat::{HitRecord, Strike, StrikeHeight, StrikeKind};
use acr_sim::world::{AnimKind, Unit, World};

use super::ragdoll_view::{Drawn, Mapping, animated_skins, node_frame, physics_of, repose};

/// Men nearest the window centre that get the layer.
const MEN: usize = 64;
/// How far the weapon's point lies along its axis from the hand (m) when the model has no
/// muzzle: about a sabre's or a club's length.
pub const REACH: f32 = 0.9;
/// The largest bend of the trunk (radians, ~75°).
pub const MAX_BEND: f32 = 1.3;
/// The largest bend of a rider's trunk (radians, ~46°): he leans out of the saddle, the arm
/// and the weapon's turn do the rest.
pub const MAX_BEND_MOUNTED: f32 = 0.8;
/// Share of the bend in the lower spine; the chest takes the rest.
const LOWER: f32 = 0.55;
/// How fast the layer fades in and out (1/s); on a strike clip it comes in faster, so the
/// first blow does not land at a part weight (m2-melee6: it was ~0.55).
const FADE: f32 = 8.0;
const FADE_STRIKE: f32 = 30.0;
/// The deepest crouch towards the ground (m).
const CROUCH: f32 = 0.25;
/// The longest step in towards a victim out of reach (m).
const STEP: f32 = 0.5;
/// How far the weapon arm may stretch (share of its length).
const STRETCH: f32 = 0.97;
/// How far from `pos - dir` the attacker of a hit may stand (world px) to be taken for it.
const NEAR: i32 = 48;
/// The sprite frame of a strike clip on which the blow lands (`poses.STRIKE_HIT_FRAME`).
pub const HIT_FRAME: f32 = 2.0;
/// The arm IK's weight at the wind-up (the clip's first and last frames) and during the
/// recovery; it is full from the hit frame for two frames.
const ARM_WINDUP: f32 = 0.35;
const ARM_RECOVER: f32 = 0.15;
/// How far short of a standing victim the point stays outside the blow (m): the bayonets
/// cross in front of the men instead of running them through.
const GUARD: f32 = 0.45;
/// The largest turn of the weapon by the hand (radians, ~30°) and how fast the turn
/// follows (1/s).
const MAX_TURN: f32 = 0.52;
const TURN_RATE: f32 = 14.0;
/// On a strike clip the clip sets the weapon's angle; the layer only takes up the residual:
/// the trunk bends at most `RESIDUAL_BEND` (~14°) and the hand turns at most
/// `RESIDUAL_TURN` (~10°), so it can never flatten the clip's blow (m2-melee5).
const RESIDUAL_BEND: f32 = 0.25;
const RESIDUAL_TURN: f32 = 0.17;
/// A standing man's body as a capsule about his axis (m): the point stops at its surface.
pub const BODY_RADIUS: f32 = 0.17;
/// How far the horse turns from the victim at a blow at a man on the ground (radians, from
/// the facing towards the rider's right): the lance's point passes the horse's right
/// shoulder (~25°), the sabre cuts down beside the horse (~75°).
pub const TURN_LANCE: f32 = 0.44;
pub const TURN_SABRE: f32 = 1.31;
/// The cut down left (`ATTACK_CUT_DOWN_L`, across the neck): the man ~40 deg left of the
/// facing (a signed turn: negative is left).
pub const TURN_SABRE_L: f32 = -0.70;
/// How much shorter (radians) the turn to the left must be before the left cut is chosen
/// (both cuts reach since m2-fam-cavalry: the shorter turn wins).
const LEFT_SAVING: f32 = 0.0;
/// How fast the horse's turn for a blow eases in and back (1/s).
const TURN_EASE: f32 = 6.0;
/// The point at a man on the ground aims at the near part of his body: this far from his
/// ground point towards the attacker (m; a man lying is ~1.7 m long). m2-melee6: 0.75 (was
/// 0.5): a man 1.7 m off is cut ~0.95 m from the rider's middle, as far as a seated man's
/// arm and sabre reach down to the ground.
pub const NEAR_BODY: f32 = 0.75;
/// The arm IK's weight on a finishing clip: the clip stands him over the man and drives the
/// blow down; the layer only corrects a little.
const FINISH_ARM: f32 = 0.5;
/// How fast the arm weight follows the clip phase (1/s): sprite frames step, the arm eases.
const ARM_RATE: f32 = 20.0;

/// The target point's height over the victim's ground point (m) for a blow at `h`.
pub fn metres(h: StrikeHeight) -> f32 {
    match h {
        StrikeHeight::High => 1.65,
        StrikeHeight::Mid => 1.15,
        StrikeHeight::Low => 0.7,
        StrikeHeight::Down => 0.2,
    }
}

/// The height of a blow at `victim`: the sim's (`strike`) when it reported one, else from
/// what the renderer sees of him: on the ground (crawling, dying, dead) `Down`, mounted
/// `High`, else `Mid`. A man on the ground is always struck `Down`, whatever the last blow.
pub fn strike_height(victim: &Unit, strike: Option<Strike>) -> StrikeHeight {
    let down = !victim.alive || victim.death > 0 || victim.body.crawling();
    if down {
        return StrikeHeight::Down;
    }
    match strike {
        Some(s) if s.height != StrikeHeight::Down => s.height,
        _ if victim.body.mounted => StrikeHeight::High,
        _ => StrikeHeight::Mid,
    }
}

/// The strike of `attacker`'s latest blow at `victim` in `hits` (newest last): a hit on
/// `victim` with a strike, struck from where `attacker` stands (the hit's `pos - dir`).
pub fn last_strike<'a>(
    hits: impl DoubleEndedIterator<Item = &'a HitRecord>,
    attacker: &Unit,
    victim: u32,
) -> Option<Strike> {
    let (ax, ay) = (attacker.pos.x >> 4, attacker.pos.y >> 4);
    hits.rev()
        .filter(|h| h.victim == victim && !h.building)
        .find(|h| {
            let (x, y) = (h.pos.0 - h.dir.0, h.pos.1 - h.dir.1);
            (x - ax).abs() <= NEAR && (y - ay).abs() <= NEAR
        })
        .and_then(|h| h.strike)
}

/// The clip-name part of a strike kind (`docs/melee.md`, Strike clips).
pub fn kind_name(k: StrikeKind) -> &'static str {
    match k {
        StrikeKind::Thrust => "THRUST",
        StrikeKind::Cut => "CUT",
        StrikeKind::Smash => "SMASH",
        StrikeKind::Shot => "SHOT",
        StrikeKind::Bite => "BITE",
    }
}

/// The clip-name part of a strike height.
pub fn height_name(h: StrikeHeight) -> &'static str {
    match h {
        StrikeHeight::High => "HIGH",
        StrikeHeight::Mid => "MID",
        StrikeHeight::Low => "LOW",
        StrikeHeight::Down => "DOWN",
    }
}

/// The clip names to try for the attack animation `anim` (`ATTACK`, `ATTACK1`...) at a blow
/// of `kind` (when known) and `height`, best first: `ATTACK_THRUST_DOWN`, `ATTACK_DOWN`; a
/// blow high or low falls back to the kind's `_MID` clip.
pub fn clip_names(anim: &str, kind: Option<StrikeKind>, height: StrikeHeight) -> Vec<String> {
    let base = anim.trim_start_matches(['#', '@']);
    let mut out = Vec::with_capacity(3);
    if let Some(k) = kind {
        out.push(format!("{base}_{}_{}", kind_name(k), height_name(height)));
    }
    out.push(format!("{base}_{}", height_name(height)));
    // A standing man struck high or low: the kind's clip at the middle height (the layer
    // aims it up or down).
    if let Some(k) = kind.filter(|_| matches!(height, StrikeHeight::High | StrikeHeight::Low)) {
        out.push(format!("{base}_{}_MID", kind_name(k)));
    }
    out
}

/// The finishing clips to try for the attack animation `anim` at a man on the ground
/// (`docs/melee.md`, Finishing blows), best first: `ATTACK_<KIND>_FINISH`, `ATTACK_FINISH`.
/// None for a point-blank shot (`ATTACK_SHOT_DOWN`) or a bite.
pub fn finish_names(anim: &str, kind: Option<StrikeKind>) -> Vec<String> {
    let base = anim.trim_start_matches(['#', '@']);
    match kind {
        Some(StrikeKind::Shot | StrikeKind::Bite) => Vec::new(),
        Some(k) => vec![format!("{base}_{}_FINISH", kind_name(k)), format!("{base}_FINISH")],
        None => vec![format!("{base}_FINISH")],
    }
}

/// Whether a clip is a finishing blow (the man already stands over his victim).
pub fn is_finish(name: &str) -> bool {
    name.ends_with("_FINISH")
}

/// The strike clip for `u` when he plays an attack (`AnimKind::Attack`) at a unit and the
/// model has one for the blow (`clip_names`); `None` keeps the `.MD` animation's clip. The
/// kind comes from his latest blow at this victim (the first blow at him: by height only).
pub fn strike_clip(data: &ModelData, world: &World, u: &Unit) -> Option<usize> {
    if !matches!(u.anim, AnimKind::Attack(_)) || data.clips.is_empty() {
        return None;
    }
    let t = u.target()?;
    let v = world.units.get(t.0 as usize).filter(|v| v.id == t && !v.building)?;
    let strike = last_strike(world.hits.iter(), u, t.0);
    let height = strike_height(v, strike);
    // A man on the ground: the finishing blow, standing over him (not a point-blank shot).
    if height == StrikeHeight::Down
        && !is_rider(data)
        && let Some(c) = finish_names(&u.anim.name(), strike.map(|s| s.kind)).iter().find_map(|n| data.clip(n))
    {
        return Some(c);
    }
    let clip = clip_names(&u.anim.name(), strike.map(|s| s.kind), height).iter().find_map(|n| data.clip(n))?;
    // A rider cutting down: the side of the horse with the shorter turn.
    let name = &data.clips[clip].name;
    if (name.ends_with("_DOWN") || name.ends_with("_DOWN_R"))
        && down_turns(data, world, u).is_some_and(|(_, left)| left)
    {
        let base = u.anim.name();
        let base = base.trim_start_matches(['#', '@']);
        if let Some(l) = data.clip(&format!("{base}_CUT_DOWN_L")) {
            return Some(l);
        }
    }
    Some(clip)
}

/// Whether the model is a rider (it has the rider's bones).
fn is_rider(data: &ModelData) -> bool {
    data.nodes.iter().any(|n| n.name.ends_with("rider.hips"))
}

/// A sabre rider striking down at `u`'s victim: the yaw putting the man at the horse's right
/// ([`TURN_SABRE`]) or left ([`TURN_SABRE_L`], when the model has `_CUT_DOWN_L`), whichever
/// is the shorter turn from the unit's facing; and whether that is the left.
fn down_turns(data: &ModelData, world: &World, u: &Unit) -> Option<(f32, bool)> {
    if !is_rider(data) || !data.clips.iter().any(|c| c.name.ends_with("_CUT_DOWN_L")) {
        return None;
    }
    let (dx, dy) = victim_offset(world, u)?;
    let facing = acr_render::view3d::facing_yaw(u.dir);
    let (r, l) = (yaw_with_side(dx, dy, TURN_SABRE), yaw_with_side(dx, dy, TURN_SABRE_L));
    Some(if wrap(l - facing).abs() + LEFT_SAVING < wrap(r - facing).abs() { (l, true) } else { (r, false) })
}

/// An angle wrapped into -pi..pi.
fn wrap(a: f32) -> f32 {
    (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

/// The world offset (pixels) from `u` to his victim; `None` without one or on the same spot.
fn victim_offset(world: &World, u: &Unit) -> Option<(f32, f32)> {
    let t = u.target()?;
    let v = world.units.get(t.0 as usize).filter(|v| v.id == t)?;
    let (dx, dy) = ((v.pos.x - u.pos.x) as f32, (v.pos.y - u.pos.y) as f32);
    (dx != 0.0 || dy != 0.0).then_some((dx, dy))
}

/// The yaw (radians, as `view3d::facing_yaw`) a rider's model takes while he strikes at a
/// man on the ground with a strike clip: turned from the victim so that he lies at the
/// rider's right side ([`TURN_LANCE`] with a lance, [`TURN_SABRE`] else) or, with
/// `_CUT_DOWN_L`, at his left ([`TURN_SABRE_L`]), where the clip's blow lands. `None` for
/// anyone else (the unit's own facing).
pub fn strike_yaw(data: &ModelData, world: &World, u: &Unit, clip: usize) -> Option<f32> {
    let name = &data.clips.get(clip)?.name;
    let left = name.ends_with("_DOWN_L");
    if !name.ends_with("_DOWN") && !name.ends_with("_DOWN_R") && !left {
        return None;
    }
    if !is_rider(data) {
        return None;
    }
    let (dx, dy) = victim_offset(world, u)?;
    // A lancer has no cutting clips.
    let lance = !data.clips.iter().any(|c| c.name.contains("_CUT_"));
    let turn = match (lance, left) {
        (true, _) => TURN_LANCE,
        (false, true) => TURN_SABRE_L,
        (false, false) => TURN_SABRE,
    };
    Some(yaw_with_side(dx, dy, turn))
}

/// The model yaw that puts the world direction `(dx, dy)` (world pixels, y down the screen)
/// `turn` radians to the right of the model's facing (negative: to the left).
pub fn yaw_with_side(dx: f32, dy: f32, turn: f32) -> f32 {
    // `model_matrix`: the facing (glTF +z) is world (sin yaw, cos yaw), the right side
    // (glTF -x) world (-cos yaw, sin yaw).
    let to = dx.atan2(dy);
    let right = |y: f32| (-y.cos() * dx + y.sin() * dy) * turn.signum();
    let t = turn.abs();
    if right(to - t) >= right(to + t) { to - t } else { to + t }
}

/// The horses' turns for a blow, eased in and back to the facing instead of snapping.
#[derive(Default)]
pub struct Turns {
    yaw: HashMap<u32, f32>,
}

impl Turns {
    /// The yaw for unit `id` this frame: towards `want` (a blow's `strike_yaw`) or back to
    /// `facing`; `None` once back at the facing (the unit's own yaw then).
    pub fn ease(&mut self, id: u32, want: Option<f32>, facing: f32, seconds: f32) -> Option<f32> {
        let now = self.yaw.get(&id).copied().or(want.map(|_| facing))?;
        let d = wrap(want.unwrap_or(facing) - now);
        if want.is_none() && d.abs() < 0.02 {
            self.yaw.remove(&id);
            return None;
        }
        let next = now + d * (1.0 - (-seconds.clamp(0.0, 0.1) * TURN_EASE).exp());
        self.yaw.insert(id, next);
        Some(next)
    }

    /// Forgets unit `id` (dead or gone: his own yaw).
    pub fn forget(&mut self, id: u32) {
        self.yaw.remove(&id);
    }
}

/// The weight of the arm IK at sprite frame `frame` of a strike clip of `frames` frames
/// (`poses.strike_keys`): it rises from the wind-up to full at the hit frame, holds two
/// frames, eases down for the recovery, stays low and rises again in the last fifth (the
/// wind-up for the next blow). Clips too short for that: full throughout.
pub fn arm_weight(frame: f32, frames: f32) -> f32 {
    let last = frames - 1.0;
    if last < HIT_FRAME + 4.0 {
        return 1.0;
    }
    let ease = |a: f32, b: f32, t: f32| {
        let t = t.clamp(0.0, 1.0);
        a + (b - a) * t * t * (3.0 - 2.0 * t)
    };
    let (hold, low, up) = (HIT_FRAME + 2.0, (0.35 * last).max(HIT_FRAME + 3.0), 0.8 * last);
    let f = frame.rem_euclid(frames);
    if f <= HIT_FRAME {
        ease(ARM_WINDUP, 1.0, f / HIT_FRAME)
    } else if f <= hold {
        1.0
    } else if f <= low {
        ease(1.0, ARM_RECOVER, (f - hold) / (low - hold))
    } else if f <= up {
        ARM_RECOVER
    } else {
        ease(ARM_RECOVER, ARM_WINDUP, (f - up) / (last - up))
    }
}

/// Whether the unit's animation is an attack stance (getting ready, standing ready, the strike
/// or the shot).
fn attacking(u: &Unit) -> bool {
    matches!(u.anim, AnimKind::Attack(_) | AnimKind::PStand(_) | AnimKind::PAttack(_))
}

/// The rotation taking `from` onto `to`, its angle clamped to `max` and scaled by `share`.
pub fn bend(from: Vec3, to: Vec3, max: f32, share: f32) -> Quat {
    let (a, b) = (from.normalize_or_zero(), to.normalize_or_zero());
    let axis = a.cross(b);
    let s = axis.length();
    if s < 1e-6 {
        return Quat::IDENTITY;
    }
    let angle = s.atan2(a.dot(b)).min(max) * share;
    Quat::from_axis_angle(axis * (1.0 / s), angle)
}

/// The point the weapon aims at on a victim whose ground point is `ground`, struck at
/// `height` by a man standing at `me` (physics): a man on the ground `near` m towards the
/// attacker from his ground point (a rider's sabre: the near part of his body, [`NEAR_BODY`];
/// a lance or a bayonet: his middle); a standing man at the surface of his body
/// ([`BODY_RADIUS`]), so the point stops there instead of running through him.
pub fn target_point(ground: Vec3, me: Vec3, height: StrikeHeight, mounted: bool, near: f32) -> Vec3 {
    let flat = Vec3::new(me.x - ground.x, me.y - ground.y, 0.0);
    let back = flat.normalize_or_zero();
    let off = match height {
        StrikeHeight::Down => near.min(flat.length() * 0.5),
        _ if mounted => 0.0,
        _ => BODY_RADIUS,
    };
    ground + back * off + Vec3::new(0.0, 0.0, metres(height))
}

/// The weapon nodes of a model: the right hand's socket, the node giving the weapon's axis
/// (the muzzle, a weapon's point or the held weapon), whether that is a muzzle (two-handed)
/// and whether it is a point (muzzle or `socket.tip`: its distance is the reach).
#[derive(Clone, Copy, Debug)]
struct Weapon {
    socket: usize,
    tip: Option<usize>,
    muzzle: bool,
    point: bool,
}

/// A point closer to the hand than this (m) is a hidden weapon (its socket scaled to 0 in
/// this clip: a dragoon's sabre while he holds the carbine); the layer then takes the
/// socket's axis and [`REACH`].
const HIDDEN: f32 = 0.2;

/// The right-hand weapon sockets, best first: the main weapon (`socket.hand_r`), a second
/// one drawn for melee (`socket.blade_r`: a dragoon's sabre, a musketeer's sidearm) and an
/// archer's club (`socket.club_r`). The clips key their scale: the visible one holds the
/// weapon in use.
const SOCKETS: [&str; 3] = ["socket.hand_r", "socket.blade_r", "socket.club_r"];

/// A socket whose scale is below this (relative to its parent) is hidden in the clip.
const SHOWN: f32 = 0.5;

/// The weapons of a model, one per right-hand socket of [`SOCKETS`] (in that order), found
/// by node names: `*socket.muzzle` under the socket, `*socket.tip` under the socket (for
/// `socket.hand_r` also elsewhere under the hand: a lance on the hand bone), else the held
/// item's first node.
fn weapons(nodes: &[Node]) -> Vec<Weapon> {
    let under = |i: usize, of: Option<usize>| {
        let mut n = nodes[i].parent;
        while let Some(p) = n {
            if Some(p) == of {
                return true;
            }
            n = nodes[p].parent;
        }
        false
    };
    let sockets: Vec<usize> = SOCKETS.iter().filter_map(|s| nodes.iter().position(|n| n.name.ends_with(s))).collect();
    // Under another weapon socket than `socket`.
    let elsewhere = |i: usize, socket: usize| sockets.iter().any(|&o| o != socket && under(i, Some(o)));
    let named = |s: &str, of: Option<usize>, socket: usize| {
        (0..nodes.len()).find(|&i| nodes[i].name.ends_with(s) && under(i, of) && !elsewhere(i, socket))
    };
    sockets
        .iter()
        .map(|&socket| {
            let hand = nodes[socket].parent;
            let muzzle = named("socket.muzzle", Some(socket), socket);
            let tip = named("socket.tip", Some(socket), socket).or_else(|| {
                if nodes[socket].name.ends_with(SOCKETS[0]) { named("socket.tip", hand, socket) } else { None }
            });
            let held =
                (0..nodes.len()).find(|&i| nodes[i].parent == Some(socket) && !nodes[i].name.contains("socket."));
            Weapon { socket, tip: muzzle.or(tip).or(held), muzzle: muzzle.is_some(), point: muzzle.or(tip).is_some() }
        })
        .collect()
}

/// The weapon in use in a pose (`globals`: the nodes' model matrices): the first of
/// `weapons` whose socket is shown (its scale against its parent's at least [`SHOWN`]),
/// else the first.
fn pick(weapons: &[Weapon], nodes: &[Node], globals: &[Mat4]) -> Option<Weapon> {
    let scale = |i: usize| {
        let m = &globals[i];
        Vec3::new(m[0][0], m[0][1], m[0][2]).length()
    };
    let shown = |w: &&Weapon| {
        let s = scale(w.socket);
        let p = nodes[w.socket].parent.map_or(1.0, scale);
        p > 1e-6 && s / p >= SHOWN
    };
    weapons.iter().find(shown).or(weapons.first()).copied()
}

/// The strike layer of the 3D view.
pub struct Strikes {
    rig: Rig,
    /// Riders' mappings (bones prefixed `rider.`), by model.
    riders: HashMap<usize, Option<Mapping>>,
    /// The weapon of each model.
    weapons: HashMap<usize, Vec<Weapon>>,
    /// Each man's weight of the layer (0..1).
    weight: HashMap<u32, f32>,
    /// Each man's arm weight (follows the clip phase, `arm_weight`) and the weapon's turn
    /// by the hand (a rotation vector, eased).
    arm: HashMap<u32, (f32, Vec3)>,
    logged: bool,
}

impl Default for Strikes {
    fn default() -> Self {
        Strikes {
            rig: Rig::new(&skeleton_h()),
            riders: HashMap::new(),
            weapons: HashMap::new(),
            weight: HashMap::new(),
            arm: HashMap::new(),
            logged: false,
        }
    }
}

impl Strikes {
    /// Bends the attacking men nearest `focus` towards their victims in `poses` (instance →
    /// palette), leaving out instances already posed (ragdolls, the gait). `hits` are the
    /// world's hit records (newest last).
    pub fn update(
        &mut self,
        seconds: f32,
        drawn: &[Drawn<'_>],
        hits: &[&HitRecord],
        maps: &HashMap<usize, Option<Mapping>>,
        focus: [f32; 2],
        poses: &mut HashMap<usize, acr_render::model::Palette>,
    ) {
        let dt = seconds.clamp(0.0, 0.1);
        let by_unit: HashMap<u32, &Drawn<'_>> = drawn.iter().map(|d| (d.unit.id.0, d)).collect();
        let mut near: Vec<(f32, &Drawn<'_>)> = drawn
            .iter()
            .filter(|d| d.unit.alive && d.unit.death == 0 && !d.unit.building && !poses.contains_key(&d.index))
            .filter(|d| attacking(d.unit) || self.weight.contains_key(&d.unit.id.0))
            .map(|d| {
                let (dx, dy) = (d.transform[3][0] - focus[0], d.transform[3][1] - focus[1]);
                (dx * dx + dy * dy, d)
            })
            .collect();
        near.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut seen = Vec::new();
        let debug = std::env::var_os("ACR_STRIKE_DEBUG").is_some();
        for (_, d) in near.into_iter().take(MEN) {
            let id = d.unit.id.0;
            let victim = d.unit.target().and_then(|t| by_unit.get(&t.0)).filter(|_| attacking(d.unit));
            let strike_clip = d.clip.and_then(|c| d.data.clips.get(c)).filter(|c| c.name.contains('_'));
            let w = self.weight.entry(id).or_insert(0.0);
            let want = if victim.is_some() { 1.0 } else { 0.0 };
            let fade = if strike_clip.is_some() && victim.is_some() { FADE_STRIKE } else { FADE };
            *w += (want - *w) * (1.0 - (-dt * fade).exp());
            let w = *w;
            if w < 0.01 && victim.is_none() {
                continue;
            }
            seen.push(id);
            // Keep aiming at the last victim while fading out: without one, nothing to bend to.
            let Some(v) = victim else { continue };
            // A rider: a model with the rider's bones prefixed `rider.`.
            let rider = self.riders.entry(d.model).or_insert_with(|| Mapping::with_prefix(d.data, "rider.")).as_ref();
            let mounted = rider.is_some();
            let map = rider.or_else(|| maps.get(&d.model).and_then(Option::as_ref).filter(|m| m.human()));
            let Some(map) = map else {
                if debug {
                    log::info!("3d: strike layer: unit {id} model {} has no mapping", d.model);
                }
                continue;
            };
            let globals = d.data.globals(d.clip, d.time);
            let found = self.weapons.entry(d.model).or_insert_with(|| weapons(&d.data.nodes));
            let Some(wpn) = pick(found, &d.data.nodes, &globals) else {
                if debug {
                    log::info!("3d: strike layer: unit {id} model {} has no weapon", d.model);
                }
                continue;
            };
            let palette = d.data.pose(d.clip, d.time);
            let before = animated_skins(map, &palette, &d.transform);
            let mut skins = before.clone();
            let ground = physics_of([v.transform[3][0], v.transform[3][1], v.transform[3][2]]);
            let strike = last_strike(hits.iter().copied(), d.unit, v.unit.id.0);
            let height = strike_height(v.unit, strike);
            let me = physics_of([d.transform[3][0], d.transform[3][1], d.transform[3][2]]);
            // The weapon as the clip holds it: from the hand's socket to the muzzle, else
            // along the held weapon.
            let (hand_at, socket_axis) = node_frame(&globals, wpn.socket, Vec3::Y, &d.transform);
            let point = wpn.tip.filter(|_| wpn.point).map(|t| node_frame(&globals, t, Vec3::Y, &d.transform).0);
            let (dir, reach) = match wpn.tip {
                Some(_) if point.is_some_and(|m| (m - hand_at).length() < HIDDEN) => {
                    (socket_axis.normalize_or_zero(), REACH)
                }
                Some(_) if wpn.point => {
                    let m = point.unwrap_or(hand_at);
                    ((m - hand_at).normalize_or_zero(), (m - hand_at).length())
                }
                Some(t) => (node_frame(&globals, t, Vec3::Y, &d.transform).1.normalize_or_zero(), REACH),
                None => (socket_axis.normalize_or_zero(), REACH),
            };
            let dir = if dir == Vec3::ZERO { Vec3::new(0.0, -1.0, 0.0) } else { dir };
            let near = if mounted && reach < 1.5 { NEAR_BODY } else { 0.0 };
            let target = target_point(ground, me, height, v.unit.body.mounted, near);
            let hb = self.rig.bone("hand_r");
            let grip = linear_t(&before[hb], hand_at - self.rig.joint(&before, hb));
            let held = Held { dir, reach, two_handed: wpn.muzzle, grip };
            // The arm follows the clip's phase when it is a strike clip (`strike_clip`), else
            // it stays on.
            let phase = strike_clip.map_or(1.0, |c| arm_weight(d.time * d.data.fps, c.duration * d.data.fps + 1.0));
            let (arm_w, turn) = self.arm.entry(id).or_insert((phase, Vec3::ZERO));
            *arm_w += (phase - *arm_w) * (1.0 - (-dt * ARM_RATE).exp());
            let how = How {
                w,
                arm: *arm_w,
                mounted,
                height,
                smash: strike.is_some_and(|s| s.kind == StrikeKind::Smash),
                rate: 1.0 - (-dt * TURN_RATE).exp(),
                residual: strike_clip.is_some(),
                finish: strike_clip.is_some_and(|c| is_finish(&c.name)),
                // Ready (PSTAND, or a model with strike clips outside them): the trunk only
                // turns to face the enemy (m2-melee6: the arm IK aimed an upright musket and
                // pointed it backwards over the shoulder).
                ready: strike_clip.is_none()
                    && (!matches!(d.unit.anim, AnimKind::Attack(_))
                        || d.data.clips.iter().any(|c| c.name.starts_with("ATTACK_"))),
            };
            *turn = aim(&self.rig, &mut skins, held, target, how, *turn);
            if !self.logged || debug {
                self.logged = true;
                log::info!(
                    "3d: strike layer: unit {id}{} at {} {height:?} ({strike:?}), clip {:?} phase {phase:.2} arm {:.2} w {w:.2}, weapon axis {dir:?} reach {reach:.2}, victim at {:?} m",
                    if mounted { " (mounted)" } else { "" },
                    v.unit.id.0,
                    strike_clip.map(|c| c.name.as_str()),
                    how.arm,
                    ground - me,
                );
            }
            poses.insert(d.index, repose(map, &palette, &before, &skins, &d.transform));
        }
        self.weight.retain(|k, _| seen.contains(k));
        self.arm.retain(|k, _| seen.contains(k));
    }
}

/// The weapon in the right hand as the clip holds it (physics): its axis from the hand, the
/// distance of its point and whether the left hand holds it too (a musket).
#[derive(Clone, Copy, Debug)]
pub struct Held {
    pub dir: Vec3,
    pub reach: f32,
    pub two_handed: bool,
    /// Where the weapon's hand socket sits from the rig's hand joint (in the hand's rest
    /// frame). The rig's joints are skeleton H's; a rider's model is bound elsewhere (seated,
    /// on the horse), so his rig hand lies ~0.45 m from the real one (m2-melee5): without
    /// this the layer corrected a phantom error and swung the lance off.
    pub grip: Vec3,
}

/// How `aim` poses a man: the layer's weight `w` (the attack stance), the arm IK's weight
/// `arm` (the clip phase, `arm_weight`), mounted, the blow's height, a butt smash (the arm
/// leaves the clip's swing mostly alone), and how far the weapon's turn follows this frame
/// (0..1).
#[derive(Clone, Copy, Debug)]
pub struct How {
    pub w: f32,
    pub arm: f32,
    pub mounted: bool,
    pub height: StrikeHeight,
    pub smash: bool,
    pub rate: f32,
    /// A strike clip: the clip sets the weapon's angle, the layer takes up the residual.
    pub residual: bool,
    /// Ready, no blow: the trunk only turns about the vertical to face the target; no
    /// step, no arm IK, no turn of the weapon.
    pub ready: bool,
    /// A finishing clip (`_FINISH`): he already stands over the man; no step in, no crouch,
    /// the arm IK at [`FINISH_ARM`] (small corrections only).
    pub finish: bool,
}

/// Bends the trunk of `skins` so that the weapon points at `target`; on foot, sinks into the
/// knees towards the ground and steps in when the target is out of reach; then puts the
/// weapon arm on the target by IK and turns the weapon onto it. Against a standing man the
/// point stays [`GUARD`] short of him but at the blow, and the step in comes with the blow.
/// `turn` is the weapon's turn by the hand last frame (a rotation vector); returns this
/// frame's (clamped to [`MAX_TURN`], eased towards the wanted one by `how.rate`).
pub fn aim(rig: &Rig, skins: &mut [Skin], weapon: Held, target: Vec3, how: How, turn: Vec3) -> Vec3 {
    let How { w, mounted, height, .. } = how;
    let b = |n: &str| rig.bone(n);
    if how.ready {
        // The chest's facing (rest -y in skeleton H) turned onto the target, flat.
        let flat = |v: Vec3| Vec3::new(v.x, v.y, 0.0);
        let (spine, chest) = (b("spine"), b("spine2"));
        for (bone, share) in [(spine, LOWER), (chest, 1.0)] {
            let pivot = rig.joint(skins, bone);
            let facing = flat(linear(&skins[chest], Vec3::new(0.0, -1.0, 0.0)));
            let max = MAX_BEND * if bone == spine { LOWER } else { 1.0 - LOWER };
            let q = bend(facing, flat(target - pivot), max, share * w);
            rig.rotate_about(skins, bone, pivot, q);
        }
        return turn * (1.0 - how.rate);
    }
    let arm_w = if how.smash {
        how.arm * 0.3
    } else if how.finish {
        how.arm * FINISH_ARM
    } else {
        how.arm
    } * w;
    let down = height == StrikeHeight::Down;
    let (hips, spine, chest, hand) = (b("hips"), b("spine"), b("spine2"), b("hand_r"));
    let (upper, fore) = (b("upperarm_r"), b("forearm_r"));
    let dir = weapon.dir.normalize_or_zero();
    // The weapon turns with the hand (the socket rides on it): its axis in the hand's rest
    // frame (skins are rotations from the rest pose).
    let local = linear_t(&skins[hand], dir);
    let grip = weapon.grip;
    let tip = |skins: &[Skin]| rig.joint(skins, hand) + linear(&skins[hand], grip + local * weapon.reach);
    let feet = [[b("thigh_l"), b("shin_l"), b("foot_l")], [b("thigh_r"), b("shin_r"), b("foot_r")]];
    let planted = feet.map(|l| rig.joint(skins, l[2]));
    // A standing man: the point stays short of him but at the blow (fencing distance).
    let target = if down || mounted {
        target
    } else {
        let from = rig.joint(skins, hips);
        let back = Vec3::new(from.x - target.x, from.y - target.y, 0.0).normalize_or_zero();
        target + back * GUARD * (1.0 - how.arm)
    };
    if !mounted && !how.finish {
        // Step in: the man moves towards the victim by what his arm and weapon lack; at a
        // standing man only with the blow.
        let shoulder = rig.joint(skins, upper);
        let arm =
            (rig.joint(skins, fore) - shoulder).length() + (rig.joint(skins, hand) - rig.joint(skins, fore)).length();
        let flat = Vec3::new(target.x - shoulder.x, target.y - shoulder.y, 0.0);
        let drop = (shoulder.z - CROUCH - target.z).max(0.0);
        let horiz = (arm * STRETCH + weapon.reach * 0.9).powi(2) - drop * drop;
        let can = if horiz > 0.0 { horiz.sqrt() } else { 0.0 };
        let lack = (flat.length() - can).clamp(0.0, STEP);
        let mut moved = flat.normalize_or_zero() * lack * if down { w } else { arm_w };
        if down {
            moved.z = -CROUCH * w;
        }
        if moved != Vec3::ZERO {
            rig.translate(skins, hips, moved);
            // The feet: the leading foot steps along, the other stays where the clip put it.
            let step = Vec3::new(moved.x, moved.y, 0.0);
            let fwd = (target - rig.joint(skins, hips)).normalize_or_zero();
            let lead = if (planted[0] - planted[1]).dot(step) >= 0.0 { 0 } else { 1 };
            for (k, (l, p)) in feet.iter().zip(planted).enumerate() {
                let at = if k == lead { p + step } else { p };
                rig.two_bone_ik(skins, l[0], l[1], l[2], at, fwd);
            }
        }
    }
    for (bone, share) in [(spine, LOWER), (chest, 1.0)] {
        let pivot = rig.joint(skins, bone);
        let most = if how.residual {
            RESIDUAL_BEND
        } else if mounted {
            MAX_BEND_MOUNTED
        } else {
            MAX_BEND
        };
        let max = most * if bone == spine { LOWER } else { 1.0 - LOWER };
        let q = bend(tip(skins) - pivot, target - pivot, max, share * w);
        rig.rotate_about(skins, bone, pivot, q);
    }
    // The weapon arm onto the target: the hand where the weapon's point meets him (the
    // weapon kept at its bent angle), weighted by the clip phase.
    let hand_at = rig.joint(skins, hand);
    let axis = linear(&skins[hand], local).normalize_or_zero();
    let want = hand_at + ((target - axis * weapon.reach - linear(&skins[hand], grip)) - hand_at) * arm_w;
    let hint = Vec3::new(0.0, 0.0, -1.0).cross(axis).normalize_or_zero();
    rig.two_bone_ik(skins, upper, fore, hand, want, hint);
    if how.residual {
        // The IK turns the forearm and with it the hand; on a strike clip the weapon keeps
        // the clip's angle (a long lance would swing far): the hand turned back onto it.
        let after = linear(&skins[hand], local).normalize_or_zero();
        let back = bend(after, axis, std::f32::consts::PI, 1.0);
        let at = rig.joint(skins, hand) + linear(&skins[hand], grip);
        rig.rotate_about(skins, hand, at, back);
    }
    // Then the weapon turned from the hand onto the target: clamped and eased over frames,
    // so the clip's swing shows and the point does not jump.
    let hand_at = rig.joint(skins, hand) + linear(&skins[hand], grip);
    let now = linear(&skins[hand], local);
    let max_turn = if how.residual { RESIDUAL_TURN } else { MAX_TURN };
    let wanted = if how.smash { Vec3::ZERO } else { bend(now, target - hand_at, max_turn, arm_w).to_rotvec() };
    let turn = turn + (wanted - turn) * how.rate;
    rig.rotate_about(skins, hand, hand_at, Quat::from_rotvec(turn));
    if weapon.two_handed {
        // The left hand on the stock, 0.45 m along the musket.
        let grip = hand_at + linear(&skins[hand], local).normalize_or_zero() * 0.45;
        let (l_up, l_fore, l_hand) = (b("upperarm_l"), b("forearm_l"), b("hand_l"));
        let now = rig.joint(skins, l_hand);
        rig.two_bone_ik(skins, l_up, l_fore, l_hand, now + (grip - now) * w, Vec3::new(0.0, 0.0, -1.0));
    }
    turn
}

/// The weapon's point of a model playing `clip` at sprite frame `frame`, standing at the
/// origin and facing physics -y (yaw 0): where the clip alone puts it and where it is after
/// the layer aims it at `target` (physics, from his ground point) at full weight (`arm` 1, the
/// turn converged). `None` without a weapon or a mapping. For offline checks of the clips
/// (`probe_clips`, `docs/melee.md`).
#[cfg(test)]
pub(super) fn probe(
    data: &ModelData,
    clip: usize,
    frame: f32,
    target: Vec3,
    height: StrikeHeight,
) -> Option<(Vec3, Vec3, Vec3)> {
    let rider = Mapping::with_prefix(data, "rider.");
    let mounted = rider.is_some();
    let map = rider.or_else(|| Mapping::with_prefix(data, ""))?;
    let transform = acr_render::view3d::model_matrix([0.0; 3], 0.0, 1.0);
    let time = frame / data.fps;
    let globals = data.globals(Some(clip), time);
    let wpn = pick(&weapons(&data.nodes), &data.nodes, &globals)?;
    let palette = data.pose(Some(clip), time);
    let mut skins = animated_skins(&map, &palette, &transform);
    let (hand_at, _) = node_frame(&globals, wpn.socket, Vec3::Y, &transform);
    let point = node_frame(&globals, wpn.tip.filter(|_| wpn.point)?, Vec3::Y, &transform).0;
    let rig = Rig::new(&skeleton_h());
    let hand = rig.bone("hand_r");
    let grip = linear_t(&skins[hand], hand_at - rig.joint(&skins, hand));
    let held = Held {
        dir: (point - hand_at).normalize_or_zero(),
        reach: (point - hand_at).length(),
        two_handed: wpn.muzzle,
        grip,
    };
    // The point rides on the hand bone: its offset in the hand's frame.
    let off = linear_t(&skins[hand], point - rig.joint(&skins, hand));
    let finish = data.clips.get(clip).is_some_and(|c| is_finish(&c.name));
    let how = How { w: 1.0, arm: 1.0, mounted, height, smash: false, rate: 1.0, residual: true, ready: false, finish };
    let mut turn = Vec3::ZERO;
    let start = skins.clone();
    for _ in 0..12 {
        skins = start.clone();
        turn = aim(&rig, &mut skins, held, target, how, turn);
    }
    Some((point, rig.joint(&skins, hand) + linear(&skins[hand], off), hand_at))
}

/// `m`'s rotation applied to `v`.
pub fn linear(m: &Skin, v: Vec3) -> Vec3 {
    gait::apply(m, v) - gait::apply(m, Vec3::ZERO)
}

/// `m`'s rotation undone on `v` (the transpose).
fn linear_t(m: &Skin, v: Vec3) -> Vec3 {
    let c = |i: usize| Vec3::new(m[i][0], m[i][1], m[i][2]);
    Vec3::new(c(0).dot(v), c(1).dot(v), c(2).dot(v))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rest(rig: &Rig) -> Vec<Skin> {
        vec![[[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]]; rig.names.len()]
    }

    /// Where the weapon's point is after `aim`.
    fn strike(mounted: bool, reach: f32, offset: Vec3) -> (Vec3, Vec3) {
        let rig = Rig::new(&skeleton_h());
        let mut skins = rest(&rig);
        let hand = rig.bone("hand_r");
        // The weapon level, forward (-y is forward in skeleton H).
        let dir = Vec3::new(0.0, -1.0, 0.0);
        let h = rig.joint(&skins, hand);
        let target = Vec3::new(h.x + offset.x, h.y + offset.y, offset.z);
        let held = Held { dir, reach, two_handed: !mounted, grip: Vec3::ZERO };
        let local = linear_t(&skins[hand], dir);
        let how = How {
            w: 1.0,
            arm: 1.0,
            mounted,
            height: StrikeHeight::Down,
            smash: false,
            rate: 1.0,
            residual: false,
            ready: false,
            finish: false,
        };
        aim(&rig, &mut skins, held, target, how, Vec3::ZERO);
        (rig.joint(&skins, hand) + linear(&skins[hand], local) * reach, target)
    }

    #[test]
    fn bend_is_clamped_and_shared() {
        let q = bend(Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.0, 0.0, -1.0), 0.5, 1.0);
        let v = q.rotate(Vec3::new(0.0, 1.0, 0.0));
        assert!((v.dot(Vec3::new(0.0, 1.0, 0.0)) - 0.5f32.cos()).abs() < 1e-4, "{v:?}");
        let half = bend(Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.0, 1.0, -1.0), 2.0, 0.5);
        let v = half.rotate(Vec3::new(0.0, 1.0, 0.0));
        assert!((v.z - (-(std::f32::consts::FRAC_PI_8).sin())).abs() < 1e-4, "{v:?}");
    }

    /// The bayonet's point meets a man on the ground, also one a step too far off.
    #[test]
    fn a_thrust_reaches_a_man_on_the_ground() {
        for ahead in [1.2, 1.9] {
            let (tip, target) = strike(false, 1.4, Vec3::new(0.0, -ahead, 0.2));
            assert!((tip - target).length() < 0.15, "{ahead}: tip {tip:?} target {target:?}");
        }
    }

    /// A rider's sabre reaches down to a man on the ground beside the horse (the rider's
    /// skeleton stands on the ground here: the target lies 1.0 m below his hand).
    #[test]
    fn a_rider_cuts_down() {
        let rig = Rig::new(&skeleton_h());
        let h = rig.joint(&rest(&rig), rig.bone("hand_r"));
        let (tip, target) = strike(true, 0.9, Vec3::new(0.4, -0.5, h.z - 1.0));
        assert!((tip - target).length() < 0.2, "tip {tip:?} target {target:?}");
    }

    /// A lance (its point ~2.3 m from the hand, `socket.tip`) reaches a man on the ground
    /// ahead of the horse (the target 1.0 m below the hand, 2 m out).
    #[test]
    fn a_lance_reaches_down() {
        let rig = Rig::new(&skeleton_h());
        let h = rig.joint(&rest(&rig), rig.bone("hand_r"));
        let (tip, target) = strike(true, 2.3, Vec3::new(0.2, -2.0, h.z - 1.0));
        assert!((tip - target).length() < 0.2, "tip {tip:?} target {target:?}");
    }

    /// The arm is full at the hit frame, low in the recovery, rising again for the wind-up.
    #[test]
    fn arm_weight_follows_the_blow() {
        let n = 20.0;
        assert_eq!(arm_weight(HIT_FRAME, n), 1.0);
        assert_eq!(arm_weight(HIT_FRAME + 2.0, n), 1.0);
        assert!((arm_weight(10.0, n) - ARM_RECOVER).abs() < 1e-6);
        assert!((arm_weight(0.0, n) - ARM_WINDUP).abs() < 1e-6);
        assert!((arm_weight(n - 1.0, n) - ARM_WINDUP).abs() < 1e-6);
        assert!(arm_weight(1.0, n) > ARM_WINDUP && arm_weight(1.0, n) < 1.0);
        assert_eq!(arm_weight(3.0, 4.0), 1.0);
    }

    #[test]
    fn clip_names_by_kind_and_height() {
        assert_eq!(
            clip_names("@ATTACK", Some(StrikeKind::Thrust), StrikeHeight::Down),
            ["ATTACK_THRUST_DOWN", "ATTACK_DOWN"]
        );
        assert_eq!(clip_names("ATTACK1", None, StrikeHeight::Mid), ["ATTACK1_MID"]);
        assert_eq!(
            clip_names("ATTACK", Some(StrikeKind::Thrust), StrikeHeight::Low),
            ["ATTACK_THRUST_LOW", "ATTACK_LOW", "ATTACK_THRUST_MID"]
        );
    }

    /// A man on the ground is finished by the kind's finishing clip; a point-blank shot keeps
    /// its own.
    #[test]
    fn finish_names_by_kind() {
        assert_eq!(finish_names("@ATTACK", Some(StrikeKind::Cut)), ["ATTACK_CUT_FINISH", "ATTACK_FINISH"]);
        assert_eq!(finish_names("ATTACK", None), ["ATTACK_FINISH"]);
        assert!(finish_names("ATTACK", Some(StrikeKind::Shot)).is_empty());
        assert!(is_finish("ATTACK_THRUST_FINISH") && !is_finish("ATTACK_THRUST_DOWN"));
    }

    /// The strike clips measured offline (`ACR_PROBE_MODELS`: a folder of built `.glb`s;
    /// skipped without it): the weapon's point at the hit frame, the clip alone and after
    /// the layer, against a target on the ground.
    #[test]
    fn probe_clips() {
        let Some(dir) = std::env::var_os("ACR_PROBE_MODELS") else { return };
        let dir = std::path::PathBuf::from(dir);
        let at = |v: &str| -> Vec3 {
            let n: Vec<f32> = v.split(',').map(|x| x.trim().parse().unwrap()).collect();
            Vec3::new(n[0], n[1], n[2])
        };
        let spec = std::env::var("ACR_PROBE").unwrap_or_default();
        for item in spec.split(';').filter(|s| !s.is_empty()) {
            // model:clip:frame:right,forward,up
            let f: Vec<&str> = item.split(':').collect();
            let bytes = std::fs::read(dir.join(format!("{}.glb", f[0]))).unwrap();
            let data = ModelData::from_glb(&bytes).unwrap();
            let clip = data.clip(f[1]).unwrap_or_else(|| panic!("no clip {}", f[1]));
            let frame: f32 = f[2].parse().unwrap();
            // right (-x), forward (-y), up.
            let r = at(f[3]);
            let target = Vec3::new(-r.x, -r.y, r.z);
            let height = if r.z < 0.6 { StrikeHeight::Down } else { StrikeHeight::Mid };
            let (clip_tip, layer_tip, hand_at) = probe(&data, clip, frame, target, height).unwrap();
            let rel = |p: Vec3| format!("right {:.2} fwd {:.2} up {:.2}", -p.x, -p.y, p.z);
            let d = clip_tip - hand_at;
            let k = if d.z.abs() > 1e-4 { (target.z - hand_at.z) / d.z } else { 0.0 };
            println!(
                "PROBE hand {} axis {} reach {:.2}, line at target height {}",
                rel(hand_at),
                rel(d.normalize_or_zero()),
                d.length(),
                rel(hand_at + d * k)
            );
            println!(
                "PROBE {} {} f{}: target {} | clip tip {} (off {:.2}) | layer tip {} (off {:.2})",
                f[0],
                f[1],
                frame,
                rel(target),
                rel(clip_tip),
                (clip_tip - target).length(),
                rel(layer_tip),
                (layer_tip - target).length()
            );
        }
    }

    /// The horse turns so that the victim lies `turn` to the rider's right.
    #[test]
    fn horse_turns_the_victim_to_the_right() {
        for (dx, dy) in [(60.0f32, 0.0f32), (0.0, -50.0), (-30.0, 40.0)] {
            let y = yaw_with_side(dx, dy, 0.44);
            let n = (dx * dx + dy * dy).sqrt();
            let f = (y.sin() * dx + y.cos() * dy) / n;
            let r = (-y.cos() * dx + y.sin() * dy) / n;
            assert!((f - 0.44f32.cos()).abs() < 1e-3 && (r - 0.44f32.sin()).abs() < 1e-3, "{dx},{dy}: {f} {r}");
        }
    }

    /// A signed turn puts the man to the left; the eased turn reaches the blow's yaw and
    /// returns to the facing.
    #[test]
    fn horse_turns_left_and_eases() {
        for (dx, dy) in [(60.0f32, 0.0f32), (0.0, -50.0), (-30.0, 40.0)] {
            let y = yaw_with_side(dx, dy, -0.7);
            let n = (dx * dx + dy * dy).sqrt();
            let r = (-y.cos() * dx + y.sin() * dy) / n;
            assert!((r + 0.7f32.sin()).abs() < 1e-3, "{dx},{dy}: {r}");
        }
        let mut t = Turns::default();
        assert_eq!(t.ease(1, None, 0.3, 0.05), None);
        let mut y = 0.0;
        for _ in 0..60 {
            y = t.ease(1, Some(1.2), 0.3, 0.05).unwrap();
        }
        assert!((y - 1.2).abs() < 0.01, "{y}");
        let first = t.ease(1, None, 0.3, 0.05).unwrap();
        assert!(first < 1.2 && first > 0.3, "{first}");
        let mut back = Some(first);
        for _ in 0..80 {
            back = t.ease(1, None, 0.3, 0.05);
        }
        assert_eq!(back, None);
    }

    /// Ready (no strike clip): the trunk turns to the target about the vertical only; the
    /// hand keeps its height (no arm IK).
    #[test]
    fn ready_only_turns_the_trunk() {
        let rig = Rig::new(&skeleton_h());
        let mut skins = rest(&rig);
        let hand = rig.bone("hand_r");
        let h0 = rig.joint(&skins, hand);
        let held = Held { dir: Vec3::new(0.0, 0.0, 1.0), reach: 1.4, two_handed: true, grip: Vec3::ZERO };
        let how = How {
            w: 1.0,
            arm: 1.0,
            mounted: false,
            height: StrikeHeight::Mid,
            smash: false,
            rate: 1.0,
            residual: false,
            ready: true,
            finish: false,
        };
        // The enemy to the side (skeleton H faces -y): a quarter turn, clamped to MAX_BEND.
        aim(&rig, &mut skins, held, Vec3::new(-1.7, 0.0, 1.15), how, Vec3::ZERO);
        let h1 = rig.joint(&skins, hand);
        assert!((h1.z - h0.z).abs() < 0.02, "hand height {h0:?} -> {h1:?}");
        let f = linear(&skins[rig.bone("spine2")], Vec3::new(0.0, -1.0, 0.0));
        assert!(f.z.abs() < 0.02 && f.x < -0.5, "chest facing {f:?}");
    }

    /// A clip that already holds the weapon close to the target: the layer takes up the
    /// residual and the point ends within 15 cm, for a lance and a sabre at a point on the
    /// ground, without flattening the weapon.
    #[test]
    fn the_layer_takes_up_a_residual() {
        let rig = Rig::new(&skeleton_h());
        let hand = rig.bone("hand_r");
        // The clip's arm: the hand bent forward in front of the right shoulder.
        let clip = || {
            let mut skins = rest(&rig);
            let (up, fore) = (rig.bone("upperarm_r"), rig.bone("forearm_r"));
            let at = rig.joint(&skins, up) + Vec3::new(0.0, -0.35, -0.25);
            rig.two_bone_ik(&mut skins, up, fore, hand, at, Vec3::new(0.0, 0.0, -1.0));
            skins
        };
        let h = rig.joint(&clip(), hand);
        for (reach, down) in [(2.5f32, 0.75f32), (0.9, 1.2)] {
            // The weapon forward and down at `down` radians; the target lies 0.2 m from where
            // the clip's point is.
            let dir = Vec3::new(0.0, -down.cos(), -down.sin());
            let clip_tip = h + dir * reach;
            let target = clip_tip + Vec3::new(0.1, -0.12, 0.12);
            let mut skins = clip();
            let held = Held { dir, reach, two_handed: false, grip: Vec3::ZERO };
            let local = linear_t(&skins[hand], dir);
            let how = How {
                w: 1.0,
                arm: 1.0,
                mounted: true,
                height: StrikeHeight::Down,
                smash: false,
                rate: 1.0,
                residual: true,
                ready: false,
                finish: false,
            };
            aim(&rig, &mut skins, held, target, how, Vec3::ZERO);
            let axis = linear(&skins[hand], local);
            let tip = rig.joint(&skins, hand) + axis * reach;
            assert!((tip - target).length() < 0.15, "{reach}: tip {tip:?} target {target:?}");
            let now_down = (-axis.z / axis.length()).asin();
            assert!(now_down > down - 0.26, "{reach}: flattened to {now_down} from {down}");
        }
    }

    /// The point at a standing man stops at his body's surface; at a man on the ground it
    /// aims at the near part of him.
    #[test]
    fn the_point_stops_at_the_body() {
        let me = Vec3::new(0.0, 0.0, 0.0);
        let at = target_point(Vec3::new(1.67, 0.0, 0.0), me, StrikeHeight::Mid, false, NEAR_BODY);
        assert!((at.x - (1.67 - BODY_RADIUS)).abs() < 1e-5 && (at.z - 1.15).abs() < 1e-5, "{at:?}");
        let down = target_point(Vec3::new(2.0, 0.0, 0.0), me, StrikeHeight::Down, false, NEAR_BODY);
        assert!((down.x - (2.0 - NEAR_BODY)).abs() < 1e-5, "{down:?}");
    }

    /// An archer with a club: the arrow on `socket.hand_r`, the club on `socket.club_r`;
    /// the visible socket's weapon is picked (the club in melee, the arrow else).
    #[test]
    fn the_shown_socket_holds_the_weapon() {
        let node = |name: &str, parent: Option<usize>| Node {
            name: name.into(),
            parent,
            translation: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0; 3],
        };
        let nodes = vec![
            node("hand_r", None),
            node("socket.hand_r", Some(0)),
            node("arrow", Some(1)),
            node("arrow.socket.tip", Some(2)),
            node("socket.club_r", Some(0)),
            node("war_club", Some(4)),
            node("war_club.socket.tip", Some(5)),
        ];
        let found = weapons(&nodes);
        assert_eq!(found.len(), 2);
        assert_eq!((found[0].socket, found[0].tip), (1, Some(3)));
        assert_eq!((found[1].socket, found[1].tip), (4, Some(6)));
        let scaled = |s: f32| [[s, 0.0, 0.0, 0.0], [0.0, s, 0.0, 0.0], [0.0, 0.0, s, 0.0], [0.0, 0.0, 0.0, 1.0]];
        let pose = |arrow: f32, club: f32| {
            vec![scaled(1.0), scaled(arrow), scaled(arrow), scaled(arrow), scaled(club), scaled(club), scaled(club)]
        };
        assert_eq!(pick(&found, &nodes, &pose(0.0, 1.0)).map(|w| w.socket), Some(4));
        assert_eq!(pick(&found, &nodes, &pose(1.0, 0.0)).map(|w| w.socket), Some(1));
        // Both hidden: the main socket.
        assert_eq!(pick(&found, &nodes, &pose(0.0, 0.0)).map(|w| w.socket), Some(1));
    }
}
