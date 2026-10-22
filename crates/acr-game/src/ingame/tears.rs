//! Tissue torn out by a hit (#280, `docs/body-sim.md` §3): the wound volumes an injury
//! leaves, placed in the body's rest pose. Render only, hashed from the hit, so the same
//! hit always tears the same way while two alike hits differ.
//!
//! - cannonball: a big capsule along the ball's track, radius by energy; through a shoulder
//!   it takes the arm, the shoulder and a chunk of chest, through a leg the leg and a ragged
//!   thigh, through the trunk (torn in two) a huge messy chunk;
//! - musket ball at close range: a ragged exit cavity, a shattered limb a partial volume
//!   beside the bone (it hangs by a thread, bent at the fracture);
//! - grapeshot: 2–4 smaller bites; a shell: several ragged fragments over the body;
//! - sabre, bayonet, axe: a gash, a thin deep elongated capsule;
//! - mauling (`Injury::cause`): claws rake 3–4 parallel gashes, jaws crush a core between two
//!   rows of tooth holes, antlers and horns punch a deep hole along the blow.

use acr_render::damage::{Limb, Rig, Volume, VolumeKind};
use acr_render::math::{self, Vec3};
use acr_sim::body::{Cause, Injury, Outcome, Part, Torn};
use acr_sim::combat::DeathCause;

/// One hit as the tears need it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tear {
    pub tick: u64,
    pub victim: u32,
    pub cause: DeathCause,
    pub injury: Injury,
    /// Towards the attacker in the model's frame (glTF x, z).
    pub side: [f32; 2],
}

/// 0..1 from the hit and a salt.
fn hash(t: &Tear, salt: u64) -> f32 {
    let mut h = u64::from(t.victim).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ t.tick.wrapping_mul(0xbf58_476d_1ce4_e5b9)
        ^ (salt + 0x7ea2).wrapping_mul(0x94d0_49bb_1331_11eb);
    h ^= h >> 31;
    h = h.wrapping_mul(0x94d0_49bb_1331_11eb);
    h ^= h >> 29;
    (h >> 40) as f32 / (1u64 << 24) as f32
}

fn norm(v: Vec3) -> Vec3 {
    let l = math::dot(v, v).sqrt();
    if l > 1e-6 { math::scale(v, 1.0 / l) } else { [0.0, 0.0, 1.0] }
}

fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// Where the injury struck: the axis point, the bone's direction and the body's radius there.
fn struck(rig: &Rig, part: Part, level: f32) -> (Vec3, Vec3, f32) {
    match part {
        Part::Limb(l) => {
            let chain = &rig.limbs[l as usize];
            let (p, d, _) = chain.at(level);
            (p, d, chain.radius_at(level).max(0.03))
        }
        // The sim counts the trunk from the shoulders, the renderer from the hips.
        Part::Chest | Part::Belly | Part::Pelvis => {
            let lf = (1.0 - level).clamp(0.0, 1.0);
            let (p, d, _) = rig.trunk.at(lf);
            (p, d, rig.trunk.radius_at(lf).max(0.08))
        }
        Part::Head => (math::add(rig.neck.0, [0.0, 0.11, 0.0]), [0.0, 1.0, 0.0], rig.head_radius),
    }
}

/// The volumes of an animal's attack (claws, jaws, horns) at `p` on a bone along `bone`
/// with the body's radius `radius` there; `facing` points out towards the animal, `track`
/// along the blow.
fn mauled(t: &Tear, p: Vec3, bone: Vec3, radius: f32, facing: Vec3, track: Vec3) -> Vec<Volume> {
    let injury = t.injury;
    let h = |k: u64| hash(t, k);
    let jitter = |k: u64, s: f32| (h(k) - 0.5) * 2.0 * s;
    let energy = f32::from(injury.energy) / 100.0;
    let tear = |a: Vec3, b: Vec3, r: f32, k: u64| Volume { a, b, radius: r, seed: h(60 + k), kind: VolumeKind::Tear };
    let surface = |d: f32| math::add(p, math::scale(facing, d));
    let limb = matches!(injury.part, Part::Limb(_));
    let mut v = Vec::new();
    if injury.outcome == Outcome::Off {
        // Torn off: ragged flesh where the limb was ripped away.
        v.push(tear(p, p, radius * (1.6 + 0.3 * h(50)), 0));
    }
    match injury.cause {
        Cause::Claw => {
            // Raking claws: parallel gashes across the part, slanted, a claw's width apart,
            // cutting 2–4 cm deep.
            let across = norm(cross(bone, facing));
            let axis = norm(math::add(across, math::scale(bone, jitter(51, 0.7))));
            let spread = norm(cross(facing, axis));
            let n = 3 + u64::from(h(52) > 0.55);
            let half = 0.06 + 0.05 * h(53) + 0.04 * energy;
            let r = 0.011 + 0.008 * energy;
            for k in 0..n {
                let off = (k as f32 - (n - 1) as f32 * 0.5) * (0.04 + 0.01 * h(54));
                let c = math::add(surface(radius - 0.3 * r), math::scale(spread, off));
                // Each claw a little longer or shorter, a little off line.
                let c = math::add(c, math::scale(axis, jitter(55 + k, 0.02)));
                let l = half * (0.8 + 0.35 * h(60 + k));
                v.push(Volume {
                    a: math::sub(c, math::scale(axis, l)),
                    b: math::add(c, math::scale(axis, l)),
                    radius: r * (0.85 + 0.3 * h(65 + k)),
                    seed: h(70 + k),
                    kind: VolumeKind::Gash,
                });
            }
        }
        Cause::Bite => {
            // The crushed core under the jaws, a shattered bone in it.
            let broken = matches!(injury.outcome, Outcome::Fracture(_));
            let core = if broken { radius * (0.9 + 0.15 * h(56)) } else { 0.03 + 0.03 * energy };
            let at = if broken { surface(radius * 0.35) } else { surface(radius * 0.85) };
            v.push(tear(at, at, core, 1));
            // Two rows of tooth holes: around a limb on its near and far side, into the trunk
            // or head above and below the bite.
            let (u, w) = if limb { (facing, norm(bone)) } else { (bone, norm(cross(bone, facing))) };
            for (row, sign) in [(0u64, 1.0f32), (1, -1.0)] {
                let base = if limb {
                    math::add(p, math::scale(u, sign * radius * 0.95))
                } else {
                    math::add(surface(radius * 0.92), math::scale(u, sign * (0.05 + 0.02 * energy)))
                };
                for k in 0..3u64 {
                    let c = math::add(base, math::scale(w, (k as f32 - 1.0) * 0.032 + jitter(80 + 3 * row + k, 0.006)));
                    v.push(tear(
                        c,
                        math::add(c, math::scale(u, -sign * 0.03)),
                        0.011 + 0.006 * energy,
                        2 + 3 * row + k,
                    ));
                }
            }
        }
        Cause::Gore => {
            // Horn or antler: in at the surface, deep along the blow.
            let depth = radius * (0.9 + 0.6 * energy);
            let a = surface(radius * 1.05);
            let b = math::add(a, math::scale(track, depth));
            v.push(tear(a, b, 0.03 + 0.02 * energy, 1));
            v.push(tear(a, a, 0.045 + 0.02 * h(57), 2));
        }
        _ => {}
    }
    v
}

/// The volumes `t` tears out of a body with `rig`.
pub fn volumes(t: &Tear, rig: &Rig) -> Vec<Volume> {
    let injury = t.injury;
    let h = |k: u64| hash(t, k);
    let jitter = |k: u64, s: f32| (h(k) - 0.5) * 2.0 * s;
    let level = f32::from(injury.level) / 255.0;
    let energy = f32::from(injury.energy) / 100.0;
    let (p, bone, radius) = struck(rig, injury.part, level);
    let out_dir = norm([t.side[0], 0.0, t.side[1]]);
    // Square to the bone, towards the attacker.
    let facing = {
        let o = math::sub(out_dir, math::scale(bone, math::dot(out_dir, bone)));
        if math::dot(o, o) > 1e-4 { norm(o) } else { out_dir }
    };
    let track = norm([-t.side[0] + jitter(1, 0.25), jitter(2, 0.3), -t.side[1] + jitter(3, 0.25)]);
    let tear = |a: Vec3, b: Vec3, r: f32, k: u64| Volume { a, b, radius: r, seed: h(40 + k), kind: VolumeKind::Tear };
    let sphere = |c: Vec3, r: f32, k: u64| tear(c, c, r, k);
    let along = |c: Vec3, half: f32| (math::sub(c, math::scale(track, half)), math::add(c, math::scale(track, half)));
    if matches!(injury.cause, Cause::Claw | Cause::Bite | Cause::Gore) {
        if matches!(injury.outcome, Outcome::Torn(_)) {
            return Vec::new();
        }
        return mauled(t, p, bone, radius, facing, track);
    }
    let mut v = Vec::new();
    let off = matches!(injury.outcome, Outcome::Off);
    match t.cause {
        DeathCause::Cannonball => {
            if matches!(injury.outcome, Outcome::Torn(Torn::Pulverized | Torn::Headless)) {
                return v;
            }
            let r = (0.075 + 0.09 * energy) * (0.85 + 0.3 * h(4));
            match injury.part {
                Part::Limb(l) => {
                    let limb = Limb::ALL[l as usize];
                    let root = rig.limbs[l as usize].at(0.0).0;
                    if limb.is_arm() && level < 0.5 && energy > 0.3 {
                        // Through the shoulder: arm, shoulder and a chunk of chest.
                        let inward = norm([-root[0], 0.0, 0.0]);
                        let c = math::add(root, [inward[0] * 0.03, -0.02 + jitter(5, 0.03), jitter(6, 0.03)]);
                        let (a, b) = along(c, 0.3);
                        v.push(tear(a, b, r * 1.3, 0));
                        let chest = math::add(c, [inward[0] * (0.1 + 0.05 * h(7)), -0.1 - 0.06 * h(8), 0.0]);
                        v.push(sphere(chest, r * (0.7 + 0.25 * h(9)), 1));
                        v.push(sphere(math::add(p, math::scale(facing, radius * 0.5)), r * 0.5, 2));
                    } else {
                        let (a, b) = along(p, 0.3);
                        v.push(tear(a, b, r.max(radius * 1.6), 0));
                        // The ragged thigh or upper arm above it.
                        let up = rig.limbs[l as usize].at((level - 0.18 - 0.15 * h(10)).max(0.04));
                        let c = math::add(up.0, math::scale(facing, radius * (0.3 + 0.4 * h(11))));
                        v.push(sphere(c, r * (0.5 + 0.25 * h(12)), 1));
                    }
                }
                Part::Head => v.push(sphere(p, r, 0)),
                _ => {
                    let torn = matches!(injury.outcome, Outcome::Torn(Torn::InTwo { .. }));
                    let r = if torn { 0.17 + 0.07 * energy + 0.04 * h(13) } else { r * 1.1 };
                    let (a, b) = along(math::add(p, [jitter(14, 0.05), 0.0, 0.0]), 0.3);
                    v.push(tear(a, b, r, 0));
                    for k in 0..2u64 {
                        let c = math::add(
                            p,
                            [jitter(15 + k, 0.18), jitter(17 + k, 0.16), out_dir[2] * radius * jitter(19 + k, 0.8)],
                        );
                        v.push(sphere(c, r * (0.45 + 0.3 * h(21 + k)), 1 + k));
                    }
                }
            }
        }
        DeathCause::Gunshot => {
            let close = injury.energy >= acr_sim::body::BALL_CLOSE;
            // Out on the far side, along the ball's track (square to the bone).
            let exit = norm(math::sub(track, math::scale(bone, 0.7 * math::dot(track, bone))));
            // Fist-sized at close range: the soft ball flattens and tears its way out.
            let r_exit = if close { 0.05 + 0.025 * h(4) } else { 0.03 + 0.015 * h(4) };
            if close || matches!(injury.outcome, Outcome::Fracture(_) | Outcome::BellyOpen) {
                // A cavity where the flattened ball comes out.
                let c = math::add(p, math::scale(exit, radius * (0.8 + 0.2 * h(5))));
                let (a, b) = (c, math::add(c, math::scale(exit, -r_exit * 0.8)));
                v.push(tear(a, b, r_exit, 0));
            }
            if let (Part::Limb(_), Outcome::Fracture(_)) = (injury.part, injury.outcome) {
                // Shattered: most of the limb's cross-section gone, a thread left.
                let c = math::add(p, math::scale(exit, radius * 0.55));
                v.push(sphere(c, radius * (1.05 + 0.1 * h(6)), 1));
            }
            if off {
                v.push(sphere(p, radius * (1.6 + 0.3 * h(7)), 2));
            }
        }
        DeathCause::Grapeshot => {
            let n = 2 + (h(4) * 3.0) as u64;
            for k in 0..n {
                let c = math::add(
                    math::add(p, math::scale(facing, radius * (0.3 + 0.35 * h(5 + k)))),
                    [jitter(10 + k, 0.1), jitter(20 + k, 0.12), 0.0],
                );
                v.push(sphere(c, 0.045 + 0.035 * h(30 + k) + 0.02 * energy, k));
            }
            if off {
                v.push(sphere(p, radius * 1.6, 7));
            }
        }
        DeathCause::Explosion => {
            let n = 3 + (h(4) * 3.0) as u64;
            for k in 0..n {
                // Fragments over the side towards the burst.
                let at = match (h(5 + k) * 4.0) as u32 {
                    0 => rig.trunk.at(0.3 + 0.6 * h(10 + k)),
                    1 => rig.limbs[(h(15 + k) * 4.0) as usize % 4].at(0.1 + 0.8 * h(20 + k)),
                    _ => (p, bone, 0),
                };
                let c = math::add(
                    at.0,
                    [jitter(25 + k, 0.08), jitter(30 + k, 0.08), out_dir[2] * 0.08 + jitter(35 + k, 0.03)],
                );
                let c = math::add(c, [out_dir[0] * 0.06, 0.0, 0.0]);
                v.push(sphere(c, 0.035 + 0.05 * h(40 + k) * (0.5 + energy), k));
            }
            if off {
                v.push(sphere(p, radius * 1.6, 7));
            }
        }
        DeathCause::Melee | DeathCause::Thrown => {
            if off {
                // A clean blow: the planar cut (ragged in the shader) stays.
                return v;
            }
            // A gash: thin, deep and long, across the bone, slanted.
            let across = norm(cross(bone, facing));
            let slant = jitter(4, 0.6);
            let axis = norm(math::add(across, math::scale(bone, slant)));
            let half = 0.06 + 0.06 * h(5) + 0.03 * energy;
            let depth = if t.cause == DeathCause::Thrown { 0.45 } else { 0.6 };
            let c = math::add(p, math::scale(facing, radius * (depth + 0.15 * h(6))));
            let r = 0.018 + 0.012 * h(7);
            v.push(Volume {
                a: math::sub(c, math::scale(axis, half)),
                b: math::add(c, math::scale(axis, half)),
                radius: r,
                seed: h(8),
                kind: VolumeKind::Gash,
            });
        }
        _ => {}
    }
    v
}

/// Cubic metres torn out (for the gibs).
pub fn size(v: &[Volume]) -> f32 {
    v.iter().map(Volume::size).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use acr_render::damage::{Chain, Damage, WHOLE};
    use acr_sim::body::LimbId;

    fn rig() -> Rig {
        let arm = |x: f32| Chain {
            points: vec![(0.0, [x, 1.4, 0.0], 1), (0.43, [x * 1.9, 1.15, 0.0], 2), (1.0, [x * 2.6, 0.9, 0.0], 3)],
            radius: [0.05; 16],
        };
        let leg = |x: f32| Chain {
            points: vec![(0.0, [x, 0.95, 0.0], 4), (0.47, [x, 0.5, 0.0], 5), (1.0, [x, 0.05, 0.05], 6)],
            radius: [0.07; 16],
        };
        let trunk = Chain { points: vec![(0.0, [0.0, 0.95, 0.0], 0), (1.0, [0.0, 1.45, 0.0], 0)], radius: [0.15; 16] };
        Rig { limbs: [arm(0.2), arm(-0.2), leg(0.1), leg(-0.1)], trunk, neck: ([0.0, 1.5, 0.0], 7), head_radius: 0.09 }
    }

    fn tear(cause: DeathCause, part: Part, level: u8, outcome: Outcome, energy: u8, tick: u64) -> Tear {
        Tear {
            tick,
            victim: 9,
            cause,
            injury: Injury { part, level, outcome, energy, cause: Cause::Musket },
            side: [0.0, 1.0],
        }
    }

    fn maul(by: Cause, part: Part, outcome: Outcome, tick: u64) -> Tear {
        let mut t = tear(DeathCause::Melee, part, 120, outcome, 90, tick);
        t.injury.cause = by;
        t
    }

    #[test]
    fn claws_rake_parallel_gashes() {
        let r = rig();
        for tick in 0..20 {
            let v = volumes(&maul(Cause::Claw, Part::Chest, Outcome::DeepWound, tick), &r);
            assert!((3..=4).contains(&v.len()), "{}", v.len());
            assert!(v.iter().all(|g| g.kind == VolumeKind::Gash && g.radius < 0.025));
            // Parallel, side by side a claw's width apart.
            let axis = |g: &Volume| norm(math::sub(g.b, g.a));
            let mid = |g: &Volume| math::scale(math::add(g.a, g.b), 0.5);
            for g in &v[1..] {
                assert!(math::dot(axis(g), axis(&v[0])).abs() > 0.999);
                let gap = math::sub(mid(g), mid(&v[0]));
                let len = math::dot(gap, gap).sqrt();
                assert!(len > 0.03 && len < 0.2, "{len}");
            }
            // On the surface, towards the animal (+z).
            assert!(v.iter().all(|g| mid(g)[2] > 0.1), "{v:?}");
        }
        // A forearm ripped off: a ragged tear where it went.
        let off = volumes(&maul(Cause::Claw, Part::Limb(LimbId::ArmR), Outcome::Off, 1), &r);
        assert!(off.iter().any(|g| g.kind == VolumeKind::Tear && g.radius > 0.07));
    }

    #[test]
    fn jaws_crush_and_horns_punch() {
        let r = rig();
        let broken = Outcome::Fracture(acr_sim::body::FractureKind::Closed);
        let bite = volumes(&maul(Cause::Bite, Part::Limb(LimbId::LegL), broken, 2), &r);
        assert_eq!(bite.len(), 7);
        assert!(bite.len() <= acr_render::damage::MAX_VOLUMES);
        // Tooth holes on both sides of the leg.
        assert!(bite.iter().any(|g| g.a[2] > 0.05) && bite.iter().any(|g| g.a[2] < -0.05));
        let gore = volumes(&maul(Cause::Gore, Part::Belly, Outcome::BellyOpen, 3), &r);
        assert_eq!(gore.len(), 2);
        let track = math::sub(gore[0].b, gore[0].a);
        assert!(math::dot(track, track).sqrt() > 0.12, "deep");
        assert!(volumes(&maul(Cause::Bite, Part::Chest, Outcome::Torn(Torn::Pulverized), 3), &r).is_empty());
    }

    #[test]
    fn a_ball_through_the_shoulder_takes_the_arm_and_chest() {
        let r = rig();
        let t = tear(DeathCause::Cannonball, Part::Limb(LimbId::ArmL), 40, Outcome::Off, 80, 3);
        let v = volumes(&t, &r);
        assert!(v.len() >= 2);
        let mut d = Damage::default();
        v.iter().for_each(|&x| d.push_volume(x));
        d.sever(&r);
        assert!(d.limb(Limb::ArmL).cut < 60, "{}", d.limb(Limb::ArmL).cut);
        assert_eq!(d.limb(Limb::ArmR).cut, WHOLE);
        // Two alike hits differ; the same hit is the same.
        let other = volumes(&Tear { tick: 4, ..t }, &r);
        assert_ne!(v, other);
        assert_eq!(v, volumes(&t, &r));
        assert!(size(&v) > 0.005);
    }

    #[test]
    fn blades_gash_and_shot_scatters() {
        let r = rig();
        let g = volumes(&tear(DeathCause::Melee, Part::Limb(LimbId::ArmR), 120, Outcome::DeepWound, 60, 1), &r);
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].kind, VolumeKind::Gash);
        assert!(g[0].radius < 0.04 && g[0].distance(g[0].a) == 0.0);
        let s = volumes(&tear(DeathCause::Grapeshot, Part::Chest, 100, Outcome::Wound, 60, 1), &r);
        assert!((2..=4).contains(&s.len()));
        let clean = volumes(&tear(DeathCause::Melee, Part::Limb(LimbId::ArmR), 120, Outcome::Off, 60, 1), &r);
        assert!(clean.is_empty());
        let far = volumes(&tear(DeathCause::Gunshot, Part::Chest, 100, Outcome::Wound, 30, 1), &r);
        assert!(far.is_empty(), "a spent ball leaves only its marks");
    }
}
