//! Mauling in 3D (#276): an animal at a man it knocked down (`Body::down`) closes in and
//! stands over him, aligned to where his body lies: a bear across him with its forepaws on
//! his chest, a jaguar on top of him along his body with its jaws at his neck, an alligator
//! gripping a limb and rolling after each bite, elk and bison with their heads at his belly.
//! Render only: the simulation keeps the animal at its striking distance, so the placement
//! eases in and out over a third of a second. Nobody is ever down without the hit-locations
//! rule.

use std::collections::HashMap;

use acr_render::damage::Rig;
use acr_render::model_renderer::ModelInstance;
use acr_render::view3d::{self, PIXELS_PER_METRE};
use acr_sim::World;
use acr_sim::body::{Beast, LimbId, Part};
use acr_sim::world::{AnimKind, Unit};

/// How far (world pixels) an animal may stand from the man it closes in on.
const REACH: f32 = 5.0 * PIXELS_PER_METRE;
/// Blend towards the pinning pose and back (per second).
const BLEND_RATE: f32 = 3.0;
/// How high a bear's forepaws rest on a lying man's chest, and the height of a jaguar's
/// root on him (metres: its pouncing clip already lifts the body, so it is lowered to
/// lie on him).
const CHEST_TOP: f32 = 0.22;
const ON_TOP: f32 = -0.1;
/// How far a jaguar on top of a man leans forward (the tilt's slope, ~24°).
const JAGUAR_LEAN: f32 = 0.45;
/// An alligator's roll after a bite: how long (s) and how far (the tilt's slope, ~60°).
const ROLL_S: f32 = 1.4;
const ROLL: f32 = 1.7;

/// Where the man lies (world pixels): his chest, his hips and the limb an alligator grips.
#[derive(Debug, Clone, Copy)]
pub struct Lying {
    pub chest: [f32; 3],
    pub hips: [f32; 3],
    pub limb: [f32; 3],
}

#[derive(Debug, Clone, Copy)]
struct Pin {
    victim: u32,
    blend: f32,
    at: [f32; 3],
    yaw: f32,
    pitch: f32,
    roll: f32,
}

/// Animals standing over the men they maul.
#[derive(Default)]
pub struct Maulers {
    pins: HashMap<u32, Pin>,
}

/// The ragdoll bone of the limb a hit on `part` struck (the left shin otherwise).
pub fn limb_bone(part: Option<Part>) -> &'static str {
    match part {
        Some(Part::Limb(LimbId::LegR)) => "shin_r",
        Some(Part::Limb(LimbId::ArmL)) => "forearm_l",
        Some(Part::Limb(LimbId::ArmR)) => "forearm_r",
        _ => "shin_l",
    }
}

fn walking(u: &Unit) -> bool {
    matches!(u.anim, AnimKind::MotionL | AnimKind::MotionR | AnimKind::MotionLs(_) | AnimKind::MotionRs(_))
}

fn flat(a: [f32; 3], b: [f32; 3]) -> ([f32; 2], f32) {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let n = (dx * dx + dy * dy).sqrt();
    if n < 1e-3 { ([0.0, 1.0], 0.0) } else { ([dx / n, dy / n], n) }
}

/// The yaw of a model facing along the world direction `d` (`view3d::model_matrix`: glTF
/// +z goes to `(sin yaw, cos yaw)`).
fn yaw_of(d: [f32; 2]) -> f32 {
    d[0].atan2(d[1])
}

fn lerp_angle(a: f32, b: f32, t: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    let d = (b - a + std::f32::consts::PI).rem_euclid(tau) - std::f32::consts::PI;
    a + d * t
}

impl Maulers {
    /// Places the animals among `drawn` (unit index, instance index) that maul a man on the
    /// ground. `rig`: the beast rig of an instance's model; `lying`: where a man lies (his
    /// ragdoll), with the limb bone to grip; `ground`: the ground point under world `(x, y)`.
    #[allow(clippy::too_many_arguments)]
    pub fn place<'a>(
        &mut self,
        world: &World,
        seconds: f32,
        drawn: &[(usize, usize)],
        instances: &mut [ModelInstance],
        rig: &dyn Fn(usize) -> Option<&'a Rig>,
        lying: &dyn Fn(u32, &'static str) -> Option<Lying>,
        ground: &dyn Fn(f32, f32) -> [f32; 3],
    ) {
        let now = world.tick.0;
        let mut seen = Vec::new();
        for &(ui, ii) in drawn {
            let Some(u) = world.units.get(ui) else { continue };
            if !u.alive || u.death > 0 || u.building {
                continue;
            }
            let Some(beast) = Beast::of(&world.catalog.get(u.kind).md_name) else { continue };
            if beast == Beast::Other {
                continue;
            }
            let Some(rig) = rig(instances[ii].model) else { continue };
            let own = [instances[ii].transform[3][0], instances[ii].transform[3][1], instances[ii].transform[3][2]];
            let near = |v: &Unit| {
                let p = ground((v.pos.x >> 4) as f32, (v.pos.y >> 4) as f32 + view3d::UNIT_SOUTH);
                flat(own, p).1 < REACH
            };
            let unit = |id: u32| world.units.get(id as usize).filter(|v| v.id.0 == id && !v.building);
            // His target on the ground, or the man it held down (now dead) while it stays.
            let victim = u
                .target()
                .and_then(|t| unit(t.0))
                .filter(|v| v.alive && v.death == 0 && v.body.downed() && near(v))
                .map(|v| v.id.0)
                .or_else(|| {
                    let pin = self.pins.get(&u.id.0)?;
                    let v = unit(pin.victim)?;
                    let held = v.body.downed() || !v.alive || v.death > 0;
                    let free = u.target().is_none_or(|t| t.0 == pin.victim);
                    (held && free && !walking(u) && near(v)).then_some(pin.victim)
                });
            let pin = self.pins.get(&u.id.0).copied();
            let target = victim.and_then(|v| {
                let last = world.hits.iter().rev().find(|h| h.victim == v).and_then(|h| h.injury);
                let body = lying(v, limb_bone(last.map(|i| i.part)))?;
                // Rolling after a bite (an alligator's death roll).
                let bitten = world
                    .hits
                    .iter()
                    .rev()
                    .find(|h| h.victim == v && h.injury.is_some_and(|i| i.cause == acr_sim::body::Cause::Bite))
                    .map(|h| now.saturating_sub(h.tick) as f32 / acr_sim::ORIGINAL_TICKS_PER_SECOND as f32);
                let pin = stand_over(beast, rig, own, body, bitten, ground, v);
                if !self.pins.contains_key(&u.id.0) {
                    log::info!(
                        "3d: {beast:?} {} pins unit {v}: chest {:?}, hips {:?}, from {own:?} to {:?}, yaw {:.2}",
                        u.id.0,
                        body.chest,
                        body.hips,
                        pin.at,
                        pin.yaw
                    );
                }
                Some(pin)
            });
            let blend = pin.map_or(0.0, |p| p.blend);
            let blend = if target.is_some() {
                (blend + BLEND_RATE * seconds).min(1.0)
            } else {
                (blend - BLEND_RATE * seconds).max(0.0)
            };
            let Some(goal) = target.or(pin) else { continue };
            if blend <= 0.0 {
                self.pins.remove(&u.id.0);
                continue;
            }
            let goal = Pin { blend, ..goal };
            self.pins.insert(u.id.0, goal);
            seen.push(u.id.0);
            let t = blend * blend * (3.0 - 2.0 * blend);
            let own_yaw = view3d::facing_yaw(u.dir);
            let at = [0, 1, 2].map(|k| own[k] + (goal.at[k] - own[k]) * t);
            let yaw = lerp_angle(own_yaw, goal.yaw, t);
            instances[ii].transform = view3d::model_matrix_tilted(at, yaw, 1.0, goal.pitch * t, goal.roll * t);
        }
        self.pins.retain(|k, _| seen.contains(k));
    }
}

/// Where `beast` stands over the man lying as `body`, coming from `own` (world pixels).
fn stand_over(
    beast: Beast,
    rig: &Rig,
    own: [f32; 3],
    body: Lying,
    bitten: Option<f32>,
    ground: &dyn Fn(f32, f32) -> [f32; 3],
    victim: u32,
) -> Pin {
    let k = PIXELS_PER_METRE;
    // The rig in glTF metres: +z ahead of the model's origin.
    let paws = 0.5 * (rig.limbs[0].at(1.0).0[2] + rig.limbs[1].at(1.0).0[2]);
    let hips = rig.trunk.at(0.0).0[2];
    let neck = rig.neck.0[2];
    let head = neck + 0.55 * (neck - hips).abs();
    let back = |p: [f32; 3], d: [f32; 2], by: f32| ground(p[0] - d[0] * by * k, p[1] - d[1] * by * k);
    let mut pin = Pin { victim, blend: 0.0, at: own, yaw: 0.0, pitch: 0.0, roll: 0.0 };
    match beast {
        // Forepaws on his chest: the front raised by the chest's height, the hind feet kept
        // on the ground.
        Beast::Bear => {
            // Across his body from its side of him, the paws below his chest.
            let (axis, _) = flat(body.hips, body.chest);
            let side = [axis[1], -axis[0]];
            let mine = (own[0] - body.chest[0]) * side[0] + (own[1] - body.chest[1]) * side[1];
            let d = if mine > 0.0 { [-side[0], -side[1]] } else { side };
            let paw_at = [0, 1, 2].map(|i| 0.7 * body.chest[i] + 0.3 * body.hips[i]);
            let at = back(paw_at, d, paws.max(0.2));
            let lift = CHEST_TOP * k * 0.5;
            pin.at = [at[0], at[1], at[2] + lift];
            pin.yaw = yaw_of(d);
            pin.pitch = lift / (paws.max(0.2) * k);
        }
        // On top of him along his body, its jaws at his neck.
        Beast::Jaguar => {
            let (d, _) = flat(body.hips, body.chest);
            let neck_at = [body.chest[0] + d[0] * 0.2 * k, body.chest[1] + d[1] * 0.2 * k, body.chest[2]];
            let at = back(neck_at, d, neck.max(0.2));
            pin.at = [at[0], at[1], at[2] + ON_TOP * k];
            pin.yaw = yaw_of(d);
            // Its rearing strike leans forward onto him.
            pin.pitch = -JAGUAR_LEAN;
        }
        // The jaws on a limb; a roll after each bite, dragging him back a little.
        Beast::Alligator => {
            let (d, _) = flat(own, body.limb);
            let roll = bitten.filter(|&s| s < ROLL_S).map_or(0.0, |s| (std::f32::consts::PI * s / ROLL_S).sin());
            let at = back(body.limb, d, head.max(0.3) + 0.25 * roll);
            pin.at = at;
            pin.yaw = yaw_of(d);
            pin.roll = ROLL * roll;
        }
        // The head lowered at his belly.
        Beast::Elk | Beast::Bison | Beast::Other => {
            let belly = [0, 1, 2].map(|i| 0.5 * (body.chest[i] + body.hips[i]));
            let (d, _) = flat(own, belly);
            pin.at = back(belly, d, (neck + 0.3 * (neck - hips).abs()).max(0.3));
            pin.yaw = yaw_of(d);
        }
    }
    pin
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yaw_faces_the_direction() {
        // Yaw 0 faces south (+y in the world).
        assert!(yaw_of([0.0, 1.0]).abs() < 1e-6);
        let (s, c) = yaw_of([1.0, 0.0]).sin_cos();
        assert!((s - 1.0).abs() < 1e-6 && c.abs() < 1e-6);
    }

    #[test]
    fn angles_blend_the_short_way() {
        let pi = std::f32::consts::PI;
        let a = lerp_angle(pi - 0.1, -pi + 0.1, 0.5);
        assert!((a.rem_euclid(2.0 * pi) - pi).abs() < 1e-4);
    }

    #[test]
    fn limbs_by_the_struck_part() {
        assert_eq!(limb_bone(Some(Part::Limb(LimbId::ArmR))), "forearm_r");
        assert_eq!(limb_bone(None), "shin_l");
    }
}
