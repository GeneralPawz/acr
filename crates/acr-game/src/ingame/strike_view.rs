//! Aimed strikes (`docs/melee.md`, Animation): an action layer over the attack clips, render
//! side only. A man (on foot or mounted) who attacks a unit bends his trunk so that his
//! weapon points at the victim's body: a standing man's belly, a rider's chest, a crawling or
//! lying man on the ground. On foot he also sinks into his knees to reach the ground (the feet
//! stay planted by leg IK).
//!
//! - The weapon's axis is the right-hand socket's (`socket.hand_r`: the musket, the sabre,
//!   the lance, the club), read from the clip; its point is [`REACH`] along it from the hand.
//! - The bend is the rotation taking that point onto the target as seen from the lower spine,
//!   shared between the lower spine and the chest and clamped ([`MAX_BEND`]); it fades in
//!   and out with the attack stance.
//! - The clip keeps its own motion (the lunge, the cut, the shot and the reload), so a thrust
//!   at a crawler is the clip's thrust, turned down onto him.

use std::collections::HashMap;

use acr_physics::gait::{self, Rig, Skin};
use acr_physics::{Quat, Vec3, skeleton_h};
use acr_render::model::ModelData;
use acr_sim::world::{AnimKind, Unit};

use super::ragdoll_view::{Drawn, Mapping, animated_skins, physics_of, repose, slot_direction};

/// Men nearest the window centre that get the layer.
const MEN: usize = 64;
/// How far the weapon's point lies along the socket from the hand (m): between a sabre's
/// (0.8) and a musket with its bayonet (1.3 from the right hand).
pub const REACH: f32 = 1.0;
/// The largest bend of the trunk (radians, ~75°).
pub const MAX_BEND: f32 = 1.3;
/// Share of the bend in the lower spine; the chest takes the rest.
const LOWER: f32 = 0.55;
/// How fast the layer fades in and out (1/s).
const FADE: f32 = 8.0;
/// The deepest crouch towards the ground (m).
const CROUCH: f32 = 0.2;

/// Where the blow goes. A copy of the sim's `StrikeHeight` (melee contract, not on main yet).
// TODO(m2-melee): use `acr_sim::combat::StrikeHeight` once `HitRecord::strike` lands, and
// prefer the height of the attacker's last strike (`strike_height` below) over the pose.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StrikeHeight {
    High,
    Mid,
    /// A kneeling man (only from the sim's strike: the renderer cannot see kneeling).
    #[allow(dead_code)]
    Low,
    /// At a man on the ground.
    Down,
}

impl StrikeHeight {
    /// The target point's height over the victim's ground point (m).
    pub fn metres(self) -> f32 {
        match self {
            StrikeHeight::High => 1.65,
            StrikeHeight::Mid => 1.15,
            StrikeHeight::Low => 0.7,
            StrikeHeight::Down => 0.2,
        }
    }
}

/// The height of a blow at `victim` from what the renderer sees of him: on the ground
/// (crawling, dying, dead) `Down`, mounted `High`, else `Mid`. `strike` is the sim's height
/// of the blow when it reports one (TODO(m2-melee)).
pub fn strike_height(victim: &Unit, strike: Option<StrikeHeight>) -> StrikeHeight {
    if let Some(h) = strike {
        return h;
    }
    if !victim.alive || victim.death > 0 || victim.body.crawling() {
        StrikeHeight::Down
    } else if victim.body.mounted {
        StrikeHeight::High
    } else {
        StrikeHeight::Mid
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

/// The strike layer of the 3D view.
pub struct Strikes {
    rig: Rig,
    /// Riders' mappings (bones prefixed `rider.`), by model.
    riders: HashMap<usize, Option<Mapping>>,
    /// The right-hand socket's slot, by model.
    sockets: HashMap<usize, Option<usize>>,
    /// Each man's weight of the layer (0..1).
    weight: HashMap<u32, f32>,
    logged: bool,
}

impl Default for Strikes {
    fn default() -> Self {
        Strikes {
            rig: Rig::new(&skeleton_h()),
            riders: HashMap::new(),
            sockets: HashMap::new(),
            weight: HashMap::new(),
            logged: false,
        }
    }
}

/// The slot carried by the node named `*socket.hand_r` (or the first below it).
fn socket_slot(data: &ModelData) -> Option<usize> {
    data.slots.iter().position(|s| {
        let mut n = Some(match *s {
            acr_render::model::Slot::Joint { node, .. } => node,
            acr_render::model::Slot::Node(n) => n,
        });
        while let Some(i) = n {
            if data.nodes[i].name.ends_with("socket.hand_r") {
                return true;
            }
            n = data.nodes[i].parent;
        }
        false
    })
}

impl Strikes {
    /// Bends the attacking men nearest `focus` towards their victims in `poses` (instance →
    /// palette), leaving out instances already posed (ragdolls, the gait).
    pub fn update(
        &mut self,
        seconds: f32,
        drawn: &[Drawn<'_>],
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
            let mounted = d.unit.body.mounted;
            let map = if mounted {
                self.riders.entry(d.model).or_insert_with(|| Mapping::with_prefix(d.data, "rider.")).as_ref()
            } else {
                maps.get(&d.model).and_then(Option::as_ref).filter(|m| m.human())
            };
            let Some(map) = map else { continue };
            let Some(slot) = *self.sockets.entry(d.model).or_insert_with(|| socket_slot(d.data)) else { continue };
            let palette = d.data.pose(d.clip, d.time);
            let before = animated_skins(map, &palette, &d.transform);
            let mut skins = before.clone();
            let ground = physics_of([v.transform[3][0], v.transform[3][1], v.transform[3][2]]);
            let height = strike_height(v.unit, None);
            let target = ground + Vec3::new(0.0, 0.0, height.metres());
            // The socket's axis as the clip poses it is not read reliably yet (it came out
            // upright for a levelled bayonet); the weapon is taken to point where the hips
            // face, level, which holds for thrusts and the end of a cut.
            let _ = (slot_direction(d.data, &palette, slot, &d.transform, Vec3::Z), slot);
            let hips = linear(&before[self.rig.bone("hips")], Vec3::new(0.0, -1.0, 0.0));
            let dir = Vec3::new(hips.x, hips.y, 0.0).normalize_or_zero();
            aim(&self.rig, &mut skins, dir, target, w, mounted, height);
            if !self.logged {
                self.logged = true;
                log::info!("3d: strike layer: unit {id} at {} {height:?}, weapon axis {dir:?}", v.unit.id.0);
            }
            poses.insert(d.index, repose(map, &palette, &before, &skins, &d.transform));
        }
        self.weight.retain(|k, _| seen.contains(k));
    }
}

/// Bends the trunk of `skins` so that the weapon (axis `dir` from the right hand) points at
/// `target`, with weight `w`; on foot, sinks into the knees towards the ground.
pub fn aim(rig: &Rig, skins: &mut [Skin], dir: Vec3, target: Vec3, w: f32, mounted: bool, height: StrikeHeight) {
    {
        let b = |n: &str| rig.bone(n);
        let (hips, spine, chest, hand) = (b("hips"), b("spine"), b("spine2"), b("hand_r"));
        let dir = dir.normalize_or_zero();
        if !mounted && height == StrikeHeight::Down {
            // Down into the knees; the feet stay where the clip put them.
            let feet = [[b("thigh_l"), b("shin_l"), b("foot_l")], [b("thigh_r"), b("shin_r"), b("foot_r")]];
            let at = feet.map(|l| rig.joint(skins, l[2]));
            rig.translate(skins, hips, Vec3::new(0.0, 0.0, -CROUCH * w));
            let fwd = (target - rig.joint(skins, hips)).normalize_or_zero();
            for (l, p) in feet.iter().zip(at) {
                rig.two_bone_ik(skins, l[0], l[1], l[2], p, fwd);
            }
        }
        // The weapon turns with the hand (the socket rides on it): its axis in the hand's rest
        // frame (skins are rotations from the rest pose).
        let local = linear_t(&skins[hand], dir);
        for (bone, share) in [(spine, LOWER), (chest, 1.0)] {
            let pivot = rig.joint(skins, bone);
            let tip = rig.joint(skins, hand) + linear(&skins[hand], local) * REACH;
            let max = MAX_BEND * if bone == spine { LOWER } else { 1.0 - LOWER };
            let q = bend(tip - pivot, target - pivot, max, share * w);
            rig.rotate_about(skins, bone, pivot, q);
        }
    }
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

    #[test]
    fn bend_is_clamped_and_shared() {
        let q = bend(Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.0, 0.0, -1.0), 0.5, 1.0);
        let v = q.rotate(Vec3::new(0.0, 1.0, 0.0));
        assert!((v.dot(Vec3::new(0.0, 1.0, 0.0)) - 0.5f32.cos()).abs() < 1e-4, "{v:?}");
        let half = bend(Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.0, 1.0, -1.0), 2.0, 0.5);
        let v = half.rotate(Vec3::new(0.0, 1.0, 0.0));
        assert!((v.z - (-(std::f32::consts::FRAC_PI_8).sin())).abs() < 1e-4, "{v:?}");
    }

    #[test]
    fn a_thrust_turns_down_onto_a_man_on_the_ground() {
        let rig = Rig::new(&skeleton_h());
        let mut skins: Vec<Skin> =
            vec![
                [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];
                rig.names.len()
            ];
        let hand = rig.bone("hand_r");
        // The weapon level, forward (-y is forward in skeleton H).
        let dir = Vec3::new(0.0, -1.0, 0.0);
        let h = rig.joint(&skins, hand);
        let target = Vec3::new(h.x, h.y - 1.2, 0.2);
        let before = h + dir * REACH;
        let miss_before = (before - target).length();
        aim(&rig, &mut skins, dir, target, 1.0, false, StrikeHeight::Down);
        let tip = rig.joint(&skins, hand) + linear(&skins[hand], dir) * REACH;
        let miss = (tip - target).length();
        assert!(miss < miss_before * 0.6, "tip {tip:?} target {target:?}: {miss} vs {miss_before}");
        assert!(tip.z < before.z - 0.3, "the point goes down: {tip:?}");
    }
}
