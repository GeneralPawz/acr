//! Wounds on the living (#277, `docs/body-sim.md` phase 0): every injury of a hit
//! (`HitRecord::injury`) leaves a mark where it struck: an entry wound, for balls a larger
//! ragged exit on the far side, blood soaking the uniform around it and running down from it,
//! growing over seconds and minutes. Render only: kept per unit id, placed in the body's rest
//! pose from a hash of the hit (the same hit always looks the same), carried over to the
//! corpse when he dies.

use std::collections::HashMap;

use acr_render::damage::{MAX_VOLUMES, Rig, Volume};

use super::tears::{self, Tear};
use acr_render::math::{self, Vec3};
use acr_render::model::ramp;
use acr_render::model_renderer::{MAX_MARKS, Mark, Sprayed};
use acr_render::view3d;
use acr_sim::body::{Cause, FractureKind, Outcome, Part};
use acr_sim::combat::{DeathCause, HitRecord};

use crate::ui::settings::Gore;

/// Ticks per second of the simulation.
const TPS: f32 = acr_sim::ORIGINAL_TICKS_PER_SECOND as f32;
/// A wound bleeds onto the ground for this long (seconds).
pub const BLEEDS: f32 = 120.0;

/// Where a wound sits.
#[derive(Debug, Clone, Copy, PartialEq)]
enum At {
    /// Limb (0..4) at level 0..1 from its root.
    Limb(usize, f32),
    /// Trunk at level 0 (hips) .. 1 (neck).
    Trunk(f32),
    Head,
}

/// One wound of a hit.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Wound {
    tick: u64,
    at: At,
    /// Outwards in the model's frame (glTF x, z): towards the attacker for an entry.
    side: [f32; 2],
    radius: f32,
    ragged: bool,
    /// How much it bleeds (scales soak and run), 0..1.
    bleed: f32,
    seed: f32,
}

#[derive(Default)]
pub struct LivingWounds {
    units: HashMap<u32, Vec<Wound>>,
    /// The hits that tear tissue out (#280), per unit.
    tears: HashMap<u32, Vec<Tear>>,
    /// Units drawn crawling or writhing, with the clip: their death holds that pose.
    lying: HashMap<u32, &'static str>,
    seen: Option<u64>,
}

/// 0..1 from a hit and a salt.
fn hash(tick: u64, unit: u32, salt: u64) -> f32 {
    let mut h = u64::from(unit).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ tick.wrapping_mul(0xbf58_476d_1ce4_e5b9)
        ^ salt.wrapping_mul(0x94d0_49bb_1331_11eb);
    h ^= h >> 31;
    h = h.wrapping_mul(0x94d0_49bb_1331_11eb);
    h ^= h >> 29;
    (h >> 40) as f32 / (1u64 << 24) as f32
}

/// Turns `v` (x, z) by `a` radians about the up axis.
fn turn(v: [f32; 2], a: f32) -> [f32; 2] {
    let (s, c) = a.sin_cos();
    [v[0] * c + v[1] * s, v[1] * c - v[0] * s]
}

/// The wounds an injury leaves: (at, outwards, radius, ragged, bleed). `side` points at the
/// attacker.
fn wounds_of(hit: &HitRecord, side: [f32; 2]) -> Vec<(At, [f32; 2], f32, bool, f32)> {
    let Some(injury) = hit.injury else { return Vec::new() };
    let h = |salt| hash(hit.tick, hit.victim, salt);
    let level = f32::from(injury.level) / 255.0;
    let at = |jitter: f32| match injury.part {
        Part::Limb(l) => At::Limb(l as usize, (level + jitter).clamp(0.03, 0.97)),
        // The sim counts the trunk from the shoulders, the renderer from the hips.
        Part::Chest | Part::Belly | Part::Pelvis => At::Trunk((1.0 - level + jitter).clamp(0.05, 0.95)),
        Part::Head => At::Head,
    };
    let ball = matches!(hit.cause, DeathCause::Gunshot | DeathCause::Grapeshot);
    let entry = turn(side, (h(1) - 0.5) * 0.9);
    let exit = turn([-side[0], -side[1]], (h(2) - 0.5) * 1.2);
    let mut out = Vec::new();
    // Mauling: claws rake a row of torn gashes, jaws leave the bite on both sides, horns one
    // big torn hole.
    let mauled = !matches!(injury.outcome, Outcome::Off | Outcome::Torn(_));
    match injury.cause {
        Cause::Claw if mauled => {
            let n = 3 + usize::from(h(4) > 0.6);
            for k in 0..n {
                let off = (k as f32 - (n - 1) as f32 * 0.5) * 0.05;
                out.push((at(off), turn(entry, off * 2.5), 0.024, true, 1.0));
            }
        }
        Cause::Bite if mauled => {
            out.push((at(0.0), entry, 0.04, true, 1.0));
            out.push((at(0.02), turn([-entry[0], -entry[1]], (h(5) - 0.5) * 0.6), 0.034, true, 1.0));
        }
        Cause::Gore if mauled => out.push((at(0.0), entry, 0.05, true, 1.0)),
        _ => {}
    }
    if !out.is_empty() {
        if injury.outcome == Outcome::BellyOpen {
            out.push((at(0.0), [0.0, 1.0], 0.06, true, 1.0));
        }
        return out;
    }
    let mut hole = |r: f32, bleed: f32, ragged_entry: bool| {
        out.push((at(0.0), entry, r, ragged_entry, bleed));
        if ball {
            // The ball leaves a larger, torn hole where it comes out.
            out.push((at((h(3) - 0.5) * 0.08), exit, r * 2.1, true, bleed));
        }
    };
    match injury.outcome {
        Outcome::Wound => hole(0.028, 0.7, false),
        Outcome::DeepWound => hole(0.045, 0.9, !ball),
        Outcome::Fracture(FractureKind::Open) => hole(0.035, 1.0, true),
        Outcome::Fracture(_) => hole(0.03, 0.8, false),
        Outcome::Kill => hole(0.03, 1.0, false),
        Outcome::BellyOpen => out.push((at(0.0), [0.0, 1.0], 0.06, true, 1.0)),
        // The stump bleeds down from just above the cut (the cut itself is drawn by #262).
        Outcome::Off => {
            if let Part::Limb(l) = injury.part {
                out.push((At::Limb(l as usize, (level - 0.04).max(0.02)), entry, 0.03, true, 1.0));
            }
        }
        Outcome::Torn(_) => {}
    }
    out
}

impl LivingWounds {
    pub fn clear(&mut self) {
        self.units.clear();
        self.tears.clear();
        self.lying.clear();
        self.seen = None;
    }

    /// Takes the new hits up to `now`; `facing` gives the victim's `RealDir`.
    pub fn take_hits<'a>(
        &mut self,
        hits: impl Iterator<Item = &'a HitRecord>,
        now: u64,
        gore: Gore,
        facing: &dyn Fn(u32) -> Option<u8>,
    ) {
        if self.seen.is_some_and(|s| s > now) {
            self.clear();
        }
        // The newest hit taken: a step records its hits under the tick it began with, so
        // they can carry the tick drawn before.
        let seen = self.seen;
        if gore == Gore::Off {
            self.seen = Some(now);
            return;
        }
        for hit in hits.filter(|h| seen.is_none_or(|s| h.tick > s) && h.tick <= now && !h.building) {
            self.seen = Some(self.seen.map_or(hit.tick, |s| s.max(hit.tick)));
            let Some(dir) = facing(hit.victim) else { continue };
            // The direction to the attacker in the model's frame (see `gore::corpse`).
            let (s, c) = view3d::facing_yaw(dir).sin_cos();
            let (vx, vy) = (-hit.dir.0 as f32, -hit.dir.1 as f32);
            let len = (vx * vx + vy * vy).sqrt();
            let side = if len > 0.0 { [(vx * c - vy * s) / len, (vx * s + vy * c) / len] } else { [0.0, 1.0] };
            if let Some(injury) = hit.injury {
                let tear = Tear { tick: hit.tick, victim: hit.victim, cause: hit.cause, injury, side };
                let tears = self.tears.entry(hit.victim).or_default();
                tears.push(tear);
                if tears.len() > MAX_VOLUMES {
                    tears.drain(..tears.len() - MAX_VOLUMES);
                }
            }
            let list = self.units.entry(hit.victim).or_default();
            for (k, (at, side, radius, ragged, bleed)) in wounds_of(hit, side).into_iter().enumerate() {
                let seed = hash(hit.tick, hit.victim, 10 + k as u64) * 97.0;
                list.push(Wound { tick: hit.tick, at, side, radius, ragged, bleed, seed });
            }
            // The newest stay when there are too many.
            if list.len() > MAX_MARKS {
                list.drain(..list.len() - MAX_MARKS);
            }
        }
    }

    /// Units with wounds.
    pub fn len(&self) -> usize {
        self.units.len()
    }

    /// Keeps only the units `keep` says are still drawn.
    pub fn retain(&mut self, keep: &dyn Fn(u32) -> bool) {
        self.units.retain(|&u, _| keep(u));
        self.tears.retain(|&u, _| keep(u));
        self.lying.retain(|&u, _| keep(u));
    }

    /// The tissue torn out of `unit` by all his hits (#280), on `rig`.
    pub fn volumes(&self, unit: u32, rig: &Rig) -> Vec<Volume> {
        // Newest first: a man mauled again and again shows his latest wounds (the volumes
        // are limited; bigger old ones still push out smaller new ones).
        self.tears.get(&unit).map_or_else(Vec::new, |l| l.iter().rev().flat_map(|t| tears::volumes(t, rig)).collect())
    }

    /// … by his hit at `tick` only.
    pub fn hit_volumes(&self, unit: u32, tick: u64, rig: &Rig) -> Vec<Volume> {
        self.tears.get(&unit).map_or_else(Vec::new, |l| {
            l.iter().filter(|t| t.tick == tick).flat_map(|t| tears::volumes(t, rig)).collect()
        })
    }

    /// Notes the clip a living unit is drawn lying in (`None`: on his feet).
    pub fn set_lying(&mut self, unit: u32, clip: Option<&'static str>) {
        match clip {
            Some(c) => {
                self.lying.insert(unit, c);
            }
            None => {
                self.lying.remove(&unit);
            }
        }
    }

    /// The clip he was lying in when last drawn alive.
    pub fn lying(&self, unit: u32) -> Option<&'static str> {
        self.lying.get(&unit).copied()
    }

    /// A wound of his still bleeds onto the ground.
    pub fn bleeding(&self, unit: u32, now: u64) -> bool {
        self.units.get(&unit).is_some_and(|l| l.iter().any(|w| (now.saturating_sub(w.tick) as f32) < BLEEDS * TPS))
    }

    /// The marks of `unit` at `now`, placed on `rig` (a rough human without one).
    pub fn marks(&self, unit: u32, rig: Option<&Rig>, now: u64) -> ([Mark; MAX_MARKS], u8) {
        let mut out = [Mark::default(); MAX_MARKS];
        let Some(list) = self.units.get(&unit) else { return (out, 0) };
        for (m, w) in out.iter_mut().zip(list) {
            let since = now.saturating_sub(w.tick) as f32 / TPS;
            *m = Mark {
                point: place(w.at, w.side, rig),
                radius: w.radius,
                ragged: w.ragged,
                // Soaks within seconds, then spreads for minutes; runs down to the knees.
                run: w.bleed * (0.06 + 0.55 * ramp(since, 0.0, 180.0).sqrt()),
                soak: w.bleed * (0.25 + 0.75 * ramp(since, 0.0, 240.0).sqrt()),
                age: ramp(since, 300.0, 1800.0),
                seed: w.seed,
            };
        }
        (out, list.len().min(MAX_MARKS) as u8)
    }

    /// Blood sprayed around his newest wound: a burst at the hit that stays as speckles and
    /// dries.
    pub fn spurt(&self, unit: u32, rig: Option<&Rig>, now: u64) -> Option<Sprayed> {
        let w = self.units.get(&unit)?.last()?;
        let since = now.saturating_sub(w.tick) as f32 / TPS;
        Some(Sprayed {
            point: place(w.at, w.side, rig),
            radius: 0.18 + 0.12 * w.bleed,
            amount: 0.6 + 0.35 * (1.0 - ramp(since, 0.5, 4.0)),
            age: ramp(since, 0.0, 600.0),
        })
    }
}

/// A point on the body's surface in the rest pose: on `at`, facing `side`.
fn place(at: At, side: [f32; 2], rig: Option<&Rig>) -> Vec3 {
    let out = [side[0], 0.0, side[1]];
    let on = |p: Vec3, dir: Vec3, r: f32| {
        // Outwards, square to the bone; along a lying bone (an animal's trunk hit from the
        // front or behind) over the top.
        let mut o = math::sub(out, math::scale(dir, math::dot(out, dir)));
        if math::dot(o, o) < 0.09 {
            let up = [0.0, 1.0, 0.0];
            o = math::add(o, math::sub(up, math::scale(dir, math::dot(up, dir))));
        }
        let l = math::dot(o, o).sqrt();
        let o = if l > 1e-3 { math::scale(o, 1.0 / l) } else { [0.0, 0.0, 1.0] };
        math::add(p, math::scale(o, r))
    };
    match (at, rig) {
        (At::Limb(l, level), Some(rig)) => {
            let chain = &rig.limbs[l];
            let (p, dir, _) = chain.at(level);
            on(p, dir, chain.radius_at(level).max(0.03))
        }
        (At::Trunk(level), Some(rig)) => {
            let (p, dir, _) = rig.trunk.at(level);
            on(p, dir, rig.trunk.radius_at(level).max(0.08))
        }
        (At::Head, Some(rig)) => on(math::add(rig.neck.0, [0.0, 0.11, 0.0]), [0.0, 1.0, 0.0], rig.head_radius),
        // A rough man of 1.75 m facing +z.
        (At::Limb(l, level), None) => {
            let x = if l % 2 == 0 { 0.2 } else { -0.2 };
            let y = if l < 2 { 1.4 - 0.6 * level } else { 0.95 - 0.9 * level };
            on([x, y, 0.0], [0.0, -1.0, 0.0], 0.05)
        }
        (At::Trunk(level), None) => on([0.0, 0.95 + 0.5 * level, 0.0], [0.0, 1.0, 0.0], 0.15),
        (At::Head, None) => on([0.0, 1.62, 0.0], [0.0, 1.0, 0.0], 0.09),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use acr_sim::body::{Injury, LimbId};

    fn hit(tick: u64, cause: DeathCause, part: Part, outcome: Outcome) -> HitRecord {
        HitRecord {
            tick,
            victim: 3,
            pos: (1000, 1000),
            // From the west: the attacker stands west of the victim.
            dir: (100, 0),
            cause,
            killed: false,
            building: false,
            region: None,
            injury: Some(Injury { part, level: 160, outcome, energy: 100, cause: acr_sim::body::Cause::Musket }),
        }
    }

    #[test]
    fn a_ball_leaves_an_entry_and_a_larger_ragged_exit_on_the_far_side() {
        let mut lw = LivingWounds::default();
        let h = hit(5, DeathCause::Gunshot, Part::Chest, Outcome::Wound);
        // Facing east (0): the attacker in the west is behind him (model −z).
        lw.take_hits([h].iter(), 5, Gore::Full, &|_| Some(0));
        let (m, n) = lw.marks(3, None, 5);
        assert_eq!(n, 2);
        assert!(!m[0].ragged && m[1].ragged && m[1].radius > m[0].radius * 1.5);
        assert!(m[0].point[2] < 0.0 && m[1].point[2] > 0.0, "{:?}", m);
        // The same hit again is not taken twice; the same hit elsewhere looks the same.
        lw.take_hits([h].iter(), 6, Gore::Full, &|_| Some(0));
        assert_eq!(lw.marks(3, None, 6).1, 2);
        let mut other = LivingWounds::default();
        other.take_hits([h].iter(), 5, Gore::Full, &|_| Some(0));
        assert_eq!(other.marks(3, None, 5), lw.marks(3, None, 5));
    }

    #[test]
    fn hits_recorded_under_the_tick_already_drawn_are_taken() {
        let mut lw = LivingWounds::default();
        lw.take_hits(std::iter::empty(), 7, Gore::Full, &|_| Some(0));
        let h = hit(7, DeathCause::Melee, Part::Chest, Outcome::Wound);
        lw.take_hits([h].iter(), 8, Gore::Full, &|_| Some(0));
        assert_eq!(lw.marks(3, None, 8).1, 1);
    }

    #[test]
    fn blood_soaks_and_runs_over_minutes_and_dries() {
        let mut lw = LivingWounds::default();
        let h = hit(0, DeathCause::Thrown, Part::Limb(LimbId::ArmL), Outcome::DeepWound);
        lw.take_hits([h].iter(), 0, Gore::Full, &|_| Some(64));
        let at = |s: u64| lw.marks(3, None, s * 10).0[0];
        let (a, b, c) = (at(1), at(60), at(2400));
        assert!(a.soak < b.soak && a.run < b.run && b.age < c.age && c.age == 1.0);
        assert!(lw.bleeding(3, 600) && !lw.bleeding(3, 3000));
        assert!(lw.spurt(3, None, 5).is_some());
    }

    #[test]
    fn a_mauled_man_shows_every_raking_and_bite() {
        let mut lw = LivingWounds::default();
        let mut claw = hit(1, DeathCause::Melee, Part::Chest, Outcome::DeepWound);
        claw.injury = claw.injury.map(|i| Injury { cause: Cause::Claw, ..i });
        lw.take_hits([claw].iter(), 1, Gore::Full, &|_| Some(0));
        let (m, n) = lw.marks(3, None, 1);
        assert!((3..=4).contains(&n), "{n}");
        assert!(m[..usize::from(n)].iter().all(|w| w.ragged));
        // A row along the body: the claws land at different heights.
        assert!((m[0].point[1] - m[usize::from(n) - 1].point[1]).abs() > 0.05);
        // Down and bitten again and again: the newest wounds always show.
        for t in 2..8 {
            let mut bite = hit(t, DeathCause::Melee, Part::Limb(LimbId::LegR), Outcome::Wound);
            bite.injury = bite.injury.map(|i| Injury { cause: Cause::Bite, ..i });
            lw.take_hits([bite].iter(), t, Gore::Full, &|_| Some(0));
        }
        assert_eq!(lw.marks(3, None, 8).1 as usize, MAX_MARKS);
    }

    #[test]
    fn gore_off_and_hits_without_injury_leave_nothing() {
        let mut lw = LivingWounds::default();
        let h = hit(1, DeathCause::Gunshot, Part::Head, Outcome::Kill);
        lw.take_hits([h].iter(), 1, Gore::Off, &|_| Some(0));
        assert_eq!(lw.marks(3, None, 1).1, 0);
        let plain = HitRecord { injury: None, tick: 2, ..h };
        lw.take_hits([plain].iter(), 2, Gore::Full, &|_| Some(0));
        assert_eq!(lw.marks(3, None, 2).1, 0);
    }
}
