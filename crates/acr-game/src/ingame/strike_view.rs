//! Aimed strikes (`docs/melee.md`, Animation): an action layer over the attack clips, render
//! side only. A man (on foot or mounted) who attacks a unit plays the strike clip for the
//! blow (`strike_clip`: by the sim's strike kind and height), bends his trunk so that his
//! weapon points at the victim's body, steps in when the victim is out of reach and puts his
//! weapon arm onto the target by two-bone IK, so the point meets him: a standing man's belly,
//! a rider's chest, a crawling or lying man on the ground. On foot he also sinks into his
//! knees to reach the ground.
//!
//! - The weapon's axis is read from the clip: the musket's `socket.muzzle` from the hand's
//!   socket, else the held weapon's own axis (the first node under `socket.hand_r`); its
//!   point is the muzzle, or [`REACH`] along the axis from the hand.
//! - The bend is the rotation taking that point onto the target as seen from the lower spine,
//!   shared between the lower spine and the chest and clamped ([`MAX_BEND`]); it fades in
//!   and out with the attack stance.
//! - The height of the blow is the sim's (`HitRecord::strike` of this attacker's latest blow
//!   at this victim), else what the renderer sees of the victim.

use std::collections::HashMap;

use acr_physics::gait::{self, Rig, Skin};
use acr_physics::{Quat, Vec3, skeleton_h};
use acr_render::model::ModelData;
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
/// How fast the layer fades in and out (1/s).
const FADE: f32 = 8.0;
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
    clip_names(&u.anim.name(), strike.map(|s| s.kind), height).iter().find_map(|n| data.clip(n))
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

/// The weapon nodes of a model: the right hand's socket, the node giving the weapon's axis
/// (the muzzle, or the held weapon) and whether that is a muzzle (its distance is the reach).
#[derive(Clone, Copy, Debug)]
struct Weapon {
    socket: usize,
    tip: Option<usize>,
    muzzle: bool,
}

/// The weapon of a model, found by node names (`*socket.hand_r`, `*socket.muzzle`).
fn weapon(data: &ModelData) -> Option<Weapon> {
    let socket = data.nodes.iter().position(|n| n.name.ends_with("socket.hand_r"))?;
    let under = |i: usize| {
        let mut n = data.nodes[i].parent;
        while let Some(p) = n {
            if p == socket {
                return true;
            }
            n = data.nodes[p].parent;
        }
        false
    };
    let muzzle = (0..data.nodes.len()).find(|&i| data.nodes[i].name.ends_with("socket.muzzle") && under(i));
    let held = || {
        (0..data.nodes.len()).find(|&i| data.nodes[i].parent == Some(socket) && !data.nodes[i].name.contains("socket."))
    };
    Some(Weapon { socket, tip: muzzle.or_else(held), muzzle: muzzle.is_some() })
}

/// The strike layer of the 3D view.
pub struct Strikes {
    rig: Rig,
    /// Riders' mappings (bones prefixed `rider.`), by model.
    riders: HashMap<usize, Option<Mapping>>,
    /// The weapon of each model.
    weapons: HashMap<usize, Option<Weapon>>,
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
            let w = self.weight.entry(id).or_insert(0.0);
            let want = if victim.is_some() { 1.0 } else { 0.0 };
            *w += (want - *w) * (1.0 - (-dt * FADE).exp());
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
            let Some(wpn) = *self.weapons.entry(d.model).or_insert_with(|| weapon(d.data)) else {
                if debug {
                    log::info!("3d: strike layer: unit {id} model {} has no weapon", d.model);
                }
                continue;
            };
            let palette = d.data.pose(d.clip, d.time);
            let globals = d.data.globals(d.clip, d.time);
            let before = animated_skins(map, &palette, &d.transform);
            let mut skins = before.clone();
            let ground = physics_of([v.transform[3][0], v.transform[3][1], v.transform[3][2]]);
            let strike = last_strike(hits.iter().copied(), d.unit, v.unit.id.0);
            let height = strike_height(v.unit, strike);
            let target = ground + Vec3::new(0.0, 0.0, metres(height));
            // The weapon as the clip holds it: from the hand's socket to the muzzle, else
            // along the held weapon.
            let (hand_at, socket_axis) = node_frame(&globals, wpn.socket, Vec3::Y, &d.transform);
            let (dir, reach) = match wpn.tip {
                Some(t) if wpn.muzzle => {
                    let (m, _) = node_frame(&globals, t, Vec3::Y, &d.transform);
                    ((m - hand_at).normalize_or_zero(), (m - hand_at).length())
                }
                Some(t) => (node_frame(&globals, t, Vec3::Y, &d.transform).1.normalize_or_zero(), REACH),
                None => (socket_axis.normalize_or_zero(), REACH),
            };
            let dir = if dir == Vec3::ZERO { Vec3::new(0.0, -1.0, 0.0) } else { dir };
            let held = Held { dir, reach, two_handed: wpn.muzzle };
            // The arm follows the clip's phase when it is a strike clip (`strike_clip`), else
            // it stays on.
            let strike_clip = d.clip.and_then(|c| d.data.clips.get(c)).filter(|c| c.name.contains('_'));
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
            };
            *turn = aim(&self.rig, &mut skins, held, target, how, *turn);
            if !self.logged || debug {
                self.logged = true;
                log::info!(
                    "3d: strike layer: unit {id}{} at {} {height:?} ({strike:?}), clip {:?} phase {phase:.2} arm {:.2} w {w:.2}, weapon axis {dir:?} reach {reach:.2}",
                    if mounted { " (mounted)" } else { "" },
                    v.unit.id.0,
                    strike_clip.map(|c| c.name.as_str()),
                    how.arm,
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
}

/// Bends the trunk of `skins` so that the weapon points at `target`; on foot, sinks into the
/// knees towards the ground and steps in when the target is out of reach; then puts the
/// weapon arm on the target by IK and turns the weapon onto it. Against a standing man the
/// point stays [`GUARD`] short of him but at the blow, and the step in comes with the blow.
/// `turn` is the weapon's turn by the hand last frame (a rotation vector); returns this
/// frame's (clamped to [`MAX_TURN`], eased towards the wanted one by `how.rate`).
pub fn aim(rig: &Rig, skins: &mut [Skin], weapon: Held, target: Vec3, how: How, turn: Vec3) -> Vec3 {
    let How { w, mounted, height, .. } = how;
    let arm_w = if how.smash { how.arm * 0.3 } else { how.arm } * w;
    let down = height == StrikeHeight::Down;
    let b = |n: &str| rig.bone(n);
    let (hips, spine, chest, hand) = (b("hips"), b("spine"), b("spine2"), b("hand_r"));
    let (upper, fore) = (b("upperarm_r"), b("forearm_r"));
    let dir = weapon.dir.normalize_or_zero();
    // The weapon turns with the hand (the socket rides on it): its axis in the hand's rest
    // frame (skins are rotations from the rest pose).
    let local = linear_t(&skins[hand], dir);
    let tip = |skins: &[Skin]| rig.joint(skins, hand) + linear(&skins[hand], local) * weapon.reach;
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
    if !mounted {
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
        let most = if mounted { MAX_BEND_MOUNTED } else { MAX_BEND };
        let max = most * if bone == spine { LOWER } else { 1.0 - LOWER };
        let q = bend(tip(skins) - pivot, target - pivot, max, share * w);
        rig.rotate_about(skins, bone, pivot, q);
    }
    // The weapon arm onto the target: the hand where the weapon's point meets him (the
    // weapon kept at its bent angle), weighted by the clip phase.
    let hand_at = rig.joint(skins, hand);
    let axis = linear(&skins[hand], local).normalize_or_zero();
    let want = hand_at + ((target - axis * weapon.reach) - hand_at) * arm_w;
    let hint = Vec3::new(0.0, 0.0, -1.0).cross(axis).normalize_or_zero();
    rig.two_bone_ik(skins, upper, fore, hand, want, hint);
    // Then the weapon turned from the hand onto the target: clamped and eased over frames,
    // so the clip's swing shows and the point does not jump.
    let hand_at = rig.joint(skins, hand);
    let now = linear(&skins[hand], local);
    let wanted = if how.smash { Vec3::ZERO } else { bend(now, target - hand_at, MAX_TURN, arm_w).to_rotvec() };
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
        let held = Held { dir, reach, two_handed: !mounted };
        let local = linear_t(&skins[hand], dir);
        let how = How { w: 1.0, arm: 1.0, mounted, height: StrikeHeight::Down, smash: false, rate: 1.0 };
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
}
