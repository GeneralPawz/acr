//! Physical locomotion of the drawn men (#279, `docs/body-sim.md` §2): kinematic layers over
//! the walking and crawling clips, render side only.
//!
//! - The clip is played by the distance the man covered (phase lock), and his visible body
//!   advances only while the carrying limb pushes or pulls ([`gait::Cycle`], measured from
//!   the clip); it leads or lags the sim position by less than one loop.
//! - Feet (walking) and hands (crawling) are planted on the terrain while they carry and held
//!   there by two-bone IK, so nothing slides; the hips sink where a leg cannot reach (slopes)
//!   and the feet tilt with the ground.
//! - A limp ([`acr_sim::body::Body::leg`]): a short, quick step on the bad leg, held stiff in
//!   the swing with the hip hiked, the trunk leaning over the good leg; the hand on the side of
//!   a leg wound (or the right hand for a chest or belly wound) holds the wound. In a heavy
//!   limp the bad foot drags along the ground, toe down. A lost foot hobbles on the stump.
//! - A broken ankle or a joint put out ([`acr_sim::body::FractureKind`] on a leg that still
//!   carries): he hops on the good leg with the hurt foot held up behind him.
//! - Crawling: legs that no longer carry (or their stumps) are passive chains dragged behind
//!   the hips, knees and feet resting on the terrain and following each pull; a man with
//!   one working leg pushes with it in rhythm with the hands' pulls.

use std::collections::HashMap;

use acr_physics::gait::{self, Contact, Cycle, Follow, Rig, Skin};
use acr_physics::{Vec3, skeleton_h};
use acr_render::model::ModelData;
use acr_render::terrain::TerrainData;
use acr_render::view3d;
use acr_sim::body::{ANKLE, Body, FractureKind, LimbId};

use super::ragdoll_view::{Drawn, Mapping, animated_skins, ground_height, physics_of, repose};

/// Men nearest the window centre that get the gait layer.
const MEN: usize = 48;

/// The effectors of a clip: feet for walking, hands for crawling.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Walk,
    Crawl,
}

struct Measured {
    kind: Kind,
    cycle: Cycle,
    /// The clips of the loop and their durations: one looping clip, or the sim's two halves
    /// of a step cycle (`MOTION_L` then `MOTION_R`, the left and the right step), each taking
    /// half of the loop's samples.
    halves: [(usize, f32); 2],
    /// Height of each effector's joint over the ground when it carries (m).
    lift: [f32; 2],
    /// Walking: lowest height of each toe joint over the ground while its foot carries (m).
    toe: [f32; 2],
    /// The loop's length in time (s).
    duration: f32,
}

impl Measured {
    /// The clip and its time at loop phase `phase` (0..1).
    fn at(&self, phase: f32) -> (usize, f32) {
        if self.halves[0].0 == self.halves[1].0 {
            return (self.halves[0].0, phase * self.halves[0].1);
        }
        let h = (phase.rem_euclid(1.0) * 2.0).min(1.999_999);
        let (clip, duration) = self.halves[h as usize];
        (clip, h.fract() * duration)
    }

    /// The loop phase where the sim's clip `clip` is at `time` (a fresh start, or a man back
    /// in view or among the nearest: his feet pick up where the sim's step is).
    fn phase_of(&self, clip: usize, time: f32) -> f32 {
        let (half, (_, duration)) = if self.halves[1].0 == clip { (1, self.halves[1]) } else { (0, self.halves[0]) };
        let f = (time / duration.max(1e-3)).clamp(0.0, 0.999);
        if self.halves[0].0 == self.halves[1].0 { f } else { (half as f32 + f) * 0.5 }
    }
}

/// The two halves of the sim's step cycle a walking clip belongs to: `MOTION_L<x>` and
/// `MOTION_R<x>` (`LB`/`RB` backwards) with the same suffix, the left step first; both
/// clips must exist. The original swaps them at every step, so they are one loop.
fn step_halves(data: &ModelData, clip: usize) -> Option<[usize; 2]> {
    let name = &data.clips.get(clip)?.name;
    let rest = name.strip_prefix("MOTION_")?;
    let (left, right) = match rest.as_bytes().first()? {
        b'L' => (name.clone(), format!("MOTION_R{}", &rest[1..])),
        b'R' => (format!("MOTION_L{}", &rest[1..]), name.clone()),
        _ => return None,
    };
    let find = |n: &str| data.clips.iter().position(|c| c.name == n);
    Some([find(&left)?, find(&right)?])
}

#[derive(Default)]
struct Man {
    clip: Option<usize>,
    vis: Option<Vec3>,
    /// The visible position's way over the sim's last tick: from, to, the time since that
    /// tick and the tick's length (s), so it moves evenly between the sim's steps.
    from: Vec3,
    to: Vec3,
    since: f32,
    span: f32,
    /// Ground speed (m/s, smoothed).
    speed: f32,
    dir: Vec3,
    follow: Follow,
    contacts: [Contact; 2],
    /// A walking foot's ball (toe joint, world) held while its heel rises in the stance: the
    /// foot rolls over it instead of the ankle staying down and the toes digging in.
    balls: [Option<Vec3>; 2],
    sink: f32,
    seen: bool,
    /// Crawling: each dragged leg's knee and foot (world), once placed.
    legs: [Option<[Vec3; 2]>; 2],
    /// Crawling: the pushing foot's plant.
    feet: [Contact; 2],
}

/// Height of a dragged knee's and foot's joint over the ground (m): the kneecap and the
/// instep lie on it.
const KNEE_REST: f32 = 0.06;
const FOOT_REST: f32 = 0.07;
/// How fast a dragged limb that has lost the ground falls back onto it (m/s).
const LEG_FALL: f32 = 1.5;
/// Share of the hop loop in the air.
const HOP_FLIGHT: f32 = 0.3;
/// Stride against ground speed: the clip's stride scaled by (speed / clip speed)^`STRIDE_EXP`
/// within `STRIDE_SOFT`, so mostly the cadence follows the speed; the stride stretches further
/// (within `STRIDE`) only where the cadence would leave `CADENCE` times the clip's.
const STRIDE_EXP: f32 = 0.25;
const STRIDE_SOFT: [f32; 2] = [0.9, 1.15];
const STRIDE: [f32; 2] = [0.75, 1.3];
const CADENCE: [f32; 2] = [0.5, 1.8];
/// How high a lifting foot clears the ground at most in its first 0.12 s (m).
const CLEAR: f32 = 0.05;
/// How far a swinging foot's toes clear the ground over their stance height (m).
const TOE_CLEAR: f32 = 0.04;

/// The gait layer of the 3D view.
pub struct Gaits {
    rig: Rig,
    clips: HashMap<(usize, usize), Option<Measured>>,
    men: HashMap<u32, Man>,
    /// Planted contacts checked, their summed and largest miss (m), for the log.
    misses: std::cell::Cell<(u32, f32, f32)>,
}

impl Default for Gaits {
    fn default() -> Self {
        Gaits { rig: Rig::new(&skeleton_h()), clips: HashMap::new(), men: HashMap::new(), misses: Default::default() }
    }
}

fn kind_of(name: &str) -> Option<Kind> {
    if name.starts_with("MOTION") {
        Some(Kind::Walk)
    } else if name.starts_with("CRAWL") {
        Some(Kind::Crawl)
    } else {
        None
    }
}

impl Gaits {
    fn effectors(&self, kind: Kind) -> [[usize; 3]; 2] {
        let b = |n: &str| self.rig.bone(n);
        match kind {
            Kind::Walk => [[b("thigh_l"), b("shin_l"), b("foot_l")], [b("thigh_r"), b("shin_r"), b("foot_r")]],
            Kind::Crawl => {
                [[b("upperarm_l"), b("forearm_l"), b("hand_l")], [b("upperarm_r"), b("forearm_r"), b("hand_r")]]
            }
        }
    }

    /// Measures a loop once (one clip, or both halves of a step cycle): its support and
    /// travel in the body's frame.
    fn measure(&self, map: &Mapping, data: &ModelData, key: [usize; 2]) -> Option<Measured> {
        let (a, b) = (data.clips.get(key[0])?, data.clips.get(key[1])?);
        let kind = kind_of(&a.name)?;
        let eff = self.effectors(kind);
        let halves = [(key[0], a.duration), (key[1], b.duration)];
        let duration = if key[0] == key[1] { a.duration } else { a.duration + b.duration };
        let probe =
            Measured { kind, cycle: Cycle::measure(&[], 0.0, false), halves, lift: [0.0; 2], toe: [0.0; 2], duration };
        let toes = [self.rig.bone("toe_l"), self.rig.bone("toe_r")];
        let mut toe_z: Vec<[f32; 2]> = Vec::new();
        let at_origin = view3d::model_matrix([0.0, 0.0, 0.0], 0.0, 1.0);
        let points: Vec<Vec<Vec3>> = (0..gait::SAMPLES)
            .map(|i| {
                let (clip, t) = probe.at(i as f32 / gait::SAMPLES as f32);
                let skins = animated_skins(map, &data.pose(Some(clip), t), &at_origin);
                toe_z.push(toes.map(|b| self.rig.joint(&skins, b).z));
                eff.iter().map(|e| self.rig.joint(&skins, e[2])).collect()
            })
            .collect();
        let low = points.iter().flatten().map(|p| p.z).fold(f32::MAX, f32::min);
        // A run's body travels evenly over its flights (`MOTION*`); a crawl's gaps between its
        // hands do not travel.
        let cycle = Cycle::measure(&points, low + 0.05, kind == Kind::Walk);
        let lift = std::array::from_fn(|e| points.iter().map(|p| p[e].z).fold(f32::MAX, f32::min));
        // The toe's lowest height while its foot stands.
        let toe = std::array::from_fn(|e| {
            let on = (0..toe_z.len()).filter(|&i| cycle.support[i] == Some(e as u8)).map(|i| toe_z[i][e]);
            on.fold(f32::MAX, f32::min).min(1.0)
        });
        let name = if key[0] == key[1] { a.name.clone() } else { format!("{}+{}", a.name, b.name) };
        log::info!(
            "3d: gait clip {name} ({kind:?}): {:.2} m per loop of {duration:.2} s, lift {lift:.3?}, toe {toe:.3?}",
            cycle.length
        );
        (cycle.length > 0.05 && a.duration > 0.0 && b.duration > 0.0).then_some(Measured { cycle, lift, toe, ..probe })
    }

    /// Poses the walking and crawling men nearest `focus` into `poses` (instance → palette),
    /// leaving out instances already posed (ragdolls).
    pub fn update(
        &mut self,
        seconds: f32,
        drawn: &[Drawn<'_>],
        maps: &HashMap<usize, Option<Mapping>>,
        focus: [f32; 2],
        terrain: &TerrainData,
        poses: &mut HashMap<usize, acr_render::model::Palette>,
    ) {
        let dt = seconds.clamp(0.0, 0.1);
        for m in self.men.values_mut() {
            m.seen = false;
        }
        let mut near: Vec<(f32, &Drawn<'_>)> = drawn
            .iter()
            .filter(|d| d.unit.alive && d.unit.death == 0 && !d.unit.body.mounted && !poses.contains_key(&d.index))
            .map(|d| {
                let (dx, dy) = (d.transform[3][0] - focus[0], d.transform[3][1] - focus[1]);
                (dx * dx + dy * dy, d)
            })
            .collect();
        near.sort_by(|a, b| a.0.total_cmp(&b.0));
        let ground = |p: Vec3| ground_height(terrain, p.x, p.y);
        for (_, d) in near.into_iter().take(MEN) {
            let Some(clip) = d.clip else { continue };
            let Some(Some(map)) = maps.get(&d.model).filter(|m| m.as_ref().is_some_and(Mapping::human)) else {
                continue;
            };
            // MOTION_L and MOTION_R are one loop: the sim's swap at every step is no change.
            let key = step_halves(d.data, clip).unwrap_or([clip, clip]);
            let loop_clip = key[0];
            if !self.clips.contains_key(&(d.model, loop_clip)) {
                let m = self.measure(map, d.data, key);
                self.clips.insert((d.model, loop_clip), m);
            }
            let Some(Some(clipm)) = self.clips.get(&(d.model, loop_clip)) else {
                self.men.remove(&d.unit.id.0);
                continue;
            };
            let mut taken = self.men.remove(&d.unit.id.0).unwrap_or_default();
            let man = &mut taken;
            man.seen = true;
            if man.clip != Some(loop_clip) {
                // A crawl keeps its dragged legs (and its phase) over a change of its clip,
                // a hop its plants.
                let same = man
                    .clip
                    .and_then(|c| self.clips.get(&(d.model, c)))
                    .is_some_and(|m| m.as_ref().is_some_and(|m| m.kind == clipm.kind));
                let keep =
                    same && (clipm.kind == Kind::Crawl || Limp::of(&d.unit.body, clipm.kind).is_some_and(|l| l.hop));
                if keep {
                    man.clip = Some(loop_clip);
                } else {
                    *man = Man { clip: Some(loop_clip), seen: true, ..Man::default() };
                    // The feet pick up where the sim's step is (a man starting, or back in
                    // view or among the nearest), not at the loop's start.
                    man.follow.dist = clipm.phase_of(clip, d.time) * clipm.cycle.length;
                }
            }
            // The sim position, smoothed over its ticks; the distance covered along the way.
            // The sim moves in ticks: the visible body goes evenly from where it was to the new
            // sim position over one tick's time (one tick behind), at any frame rate.
            let sim = physics_of([d.transform[3][0], d.transform[3][1], d.transform[3][2]]);
            match man.vis {
                Some(v) if (sim - v).length() < 2.0 => {
                    if (sim - man.to).length() > 1e-6 {
                        man.span = man.since.clamp(dt.max(1e-3), 0.25);
                        (man.from, man.to, man.since) = (v, sim, 0.0);
                    }
                }
                _ => (man.from, man.to, man.span, man.since) = (sim, sim, 0.1, 0.1),
            }
            man.since += dt;
            let vis = man.from + (man.to - man.from) * (man.since / man.span.max(1e-3)).min(1.0);
            let step = Vec3::new(vis.x - man.vis.unwrap_or(vis).x, vis.y - man.vis.unwrap_or(vis).y, 0.0);
            if step.length() > 1e-5 {
                man.dir = (man.dir * 0.8 + step.normalize_or_zero() * 0.2).normalize_or_zero();
            }
            if dt > 0.0 {
                let k = 1.0 - (-dt / 0.3).exp();
                man.speed += (step.length() / dt - man.speed) * k;
            }
            // The cadence follows the ground speed; the stride stretches a little (walking).
            let hops = Limp::of(&d.unit.body, clipm.kind).is_some_and(|l| l.hop);
            let stride = if clipm.kind == Kind::Walk && !hops {
                stride_scale(man.speed, clipm.cycle.length / clipm.duration.max(1e-3))
            } else {
                1.0
            };
            man.follow.dist += step.length() / stride;
            man.vis = Some(vis);
            let body = &d.unit.body;
            let limp = Limp::of(body, clipm.kind);
            let (mut phase, lead) = man.follow.place(&clipm.cycle, clipm.cycle.length);
            if let Some(l) = limp.filter(|l| !l.hop) {
                phase = gait::limp_warp(&clipm.cycle, l.bad, l.severity, phase);
            }
            let (shown, t) = clipm.at(phase);
            let pose = d.data.pose(Some(shown), t);
            let before = animated_skins(map, &pose, &d.transform);
            let mut skins = before.clone();
            // The visible body: where the pushes and pulls brought it.
            let shift = (vis - sim) + man.dir * (lead * stride);
            self.rig.translate(&mut skins, 0, Vec3::new(shift.x, shift.y, 0.0));
            self.pose(&mut skins, clipm, man, phase, stride, limp, body, &ground, dt);
            // Each slot moves with its bone but keeps its own animation: a socket the clip
            // keys to scale 0 (a tool shown only while working) stays hidden, fingers keep
            // their grip.
            poses.insert(d.index, repose(map, &pose, &before, &skins, &d.transform));
            self.men.insert(d.unit.id.0, taken);
        }
        self.men.retain(|_, m| m.seen);
    }

    #[allow(clippy::too_many_arguments)]
    fn pose(
        &self,
        skins: &mut [Skin],
        clipm: &Measured,
        man: &mut Man,
        phase: f32,
        stride: f32,
        limp: Option<Limp>,
        body: &Body,
        ground: &dyn Fn(Vec3) -> f32,
        dt: f32,
    ) {
        let rig = &self.rig;
        let eff = self.effectors(clipm.kind);
        let (hips, spine) = (rig.bone("hips"), rig.bone("spine"));
        let support = clipm.cycle.support_at(phase);
        let fwd = if man.dir.length() > 0.5 { man.dir } else { Vec3::new(0.0, -1.0, 0.0) };
        let up = Vec3::Z;
        if let (Some(l), Kind::Walk) = (limp, clipm.kind) {
            let good = 1 - l.bad as usize;
            let bad_side = (rig.joint(skins, eff[l.bad as usize][0]) - rig.joint(skins, hips)).normalize_or_zero();
            // Hip hike while the bad leg swings, the trunk over the good leg while it carries.
            let swing = support == Some(good as u8);
            let hike = if swing { 0.14 * l.severity } else { 0.0 };
            let pivot = rig.joint(skins, hips);
            rig.rotate_about(skins, hips, pivot, gait::rotation_between(up, up - bad_side * hike));
            let lean = 0.12 * l.severity + if swing { 0.0 } else { 0.08 * l.severity };
            let pivot = rig.joint(skins, spine);
            rig.rotate_about(skins, spine, pivot, gait::rotation_between(up, up - bad_side * lean));
            // The stiff bad leg in its swing: less knee bend.
            if swing {
                let [t, s, f] = eff[l.bad as usize];
                let (a, c) = (rig.joint(skins, t), rig.joint(skins, f));
                let full = (rig.joint(skins, s) - a).length() + (c - rig.joint(skins, s)).length();
                let d = (c - a).length();
                let want = d + (full - d) * 0.7 * l.severity;
                rig.two_bone_ik(skins, t, s, f, a + (c - a).normalize_or_zero() * want, Vec3::X);
            }
        }
        // How much the bad foot drags (a limp worse than 40 %, fully from 80 %; two leg wounds
        // make 70 %, 0.75); a stump hobbles instead.
        let drag = match (limp, clipm.kind) {
            (Some(l), Kind::Walk) if !stump(body, l.bad as usize) && !l.hop => {
                ((l.severity - 0.4) / 0.4).clamp(0.0, 1.0)
            }
            _ => 0.0,
        };
        // A hop on the good leg: two hops per loop, the end of each in the air.
        let hop = limp.filter(|l| l.hop && clipm.kind == Kind::Walk).map(|l| {
            let h = (phase * 2.0).fract();
            (l.bad as usize, h, h >= 1.0 - HOP_FLIGHT)
        });
        // Contacts: planted where they carry.
        let mut targets = [Vec3::ZERO; 2];
        let mut need: f32 = 0.0;
        let pelvis = rig.joint(skins, hips);
        let toes = [rig.bone("toe_l"), rig.bone("toe_r")];
        for (e, chain) in eff.iter().enumerate() {
            let joint = rig.joint(skins, chain[2]);
            // The stride stretched along the way about the hips.
            let anim = joint + fwd * ((joint - pelvis).dot(fwd) * (stride - 1.0));
            let mut lift = clipm.lift[e].max(0.02);
            if clipm.kind == Kind::Walk && stump(body, e) {
                lift = 0.03;
            }
            let mut carries = support == Some(e as u8);
            let mut anim = anim;
            // The clip swings the foot over a level floor at the body's height: on a slope or a
            // bump it follows the ground under it instead (uphill it hit the rising ground at
            // full swing before its contact and scraped in; flat ground is unchanged).
            if clipm.kind == Kind::Walk {
                anim.z += ground(anim) - ground(pelvis);
            }
            if let Some((bad, _, flying)) = hop {
                // The good foot carries all but the flight and lands under its hip, a little
                // ahead; the hurt foot is placed after the IK.
                carries = e != bad && !flying;
                let a = rig.joint(skins, chain[0]);
                anim = Vec3::new(a.x, a.y, 0.0) + fwd * 0.18;
                anim.z = ground(anim) + lift + 0.06;
            }
            let mut t = man.contacts[e].update(carries, anim, lift, ground, dt);
            // Heel-off in a run (a clip with flights): once the clip lifts the carrying ankle,
            // the ball stays where it was and the ankle follows the clip's foot about it.
            if clipm.kind == Kind::Walk
                && carries
                && hop.is_none()
                && clipm.cycle.support.contains(&None)
                && anim.z - ground(anim) - lift > gait::HEEL_OFF
            {
                let stretch = |p: Vec3| p + fwd * ((p - pelvis).dot(fwd) * (stride - 1.0));
                let o = stretch(rig.joint(skins, toes[e])) - stretch(joint);
                let ball = *man.balls[e].get_or_insert(t + o);
                t = ball - o;
                man.contacts[e].last = t;
            } else {
                man.balls[e] = None;
            }
            // A walking foot that has just lifted clears the ground first (toe-off) instead of
            // scraping forwards on its toes.
            let release = man.contacts[e].release;
            if clipm.kind == Kind::Walk && !carries && release > 0.0 && hop.is_none() {
                let floor = ground(t) + lift;
                t.z = t.z.max(floor + CLEAR * (std::f32::consts::PI * (1.0 - release)).sin());
            }
            // A heavy limp drags the bad foot: in its swing it stays down on the ground,
            // scraping forwards instead of lifting (the hip hike gives it room).
            let dragged = drag > 0.0 && !carries && limp.is_some_and(|l| l.bad as usize == e);
            if dragged {
                let floor = ground(t) + lift;
                t.z = floor + (t.z - floor).max(0.0) * (1.0 - drag);
            }
            targets[e] = t;
            if clipm.kind == Kind::Walk && !dragged && hop.is_none_or(|(bad, _, _)| bad != e) {
                let a = rig.joint(skins, chain[0]);
                let reach = (rig.joint(skins, chain[1]) - a).length() + (joint - rig.joint(skins, chain[1])).length();
                need = need.max((t - a).length() - reach * 0.995);
            }
        }
        // Hips lower where a leg cannot reach (downhill, a long planted step).
        if clipm.kind == Kind::Walk {
            let want = need.clamp(0.0, 0.25);
            man.sink += (want - man.sink) * (1.0 - (-dt * 12.0).exp()).max(if want > man.sink { 0.5 } else { 0.0 });
            // The hop lifts the hips in its flight.
            let bob = hop.map_or(0.0, |(_, h, flying)| {
                if flying { 0.07 * (std::f32::consts::PI * (h - 1.0 + HOP_FLIGHT) / HOP_FLIGHT).sin() } else { 0.0 }
            });
            rig.translate(skins, hips, Vec3::new(0.0, 0.0, bob - man.sink));
        }
        for (e, chain) in eff.iter().enumerate() {
            let side = if e == 0 { Vec3::X } else { Vec3::X * -1.0 };
            let hint = if clipm.kind == Kind::Walk { fwd.cross(up) * -1.0 } else { side };
            // The foot keeps the clip's pitch in the world, not the shin's after the IK (the toes
            // sank into the ground late in each stance).
            let aim = rig.joint(skins, toes[e]) - rig.joint(skins, chain[2]);
            rig.two_bone_ik(skins, chain[0], chain[1], chain[2], targets[e], hint);
            if clipm.kind == Kind::Walk {
                let ankle = rig.joint(skins, chain[2]);
                rig.rotate_about(
                    skins,
                    chain[2],
                    ankle,
                    gait::rotation_between(rig.joint(skins, toes[e]) - ankle, aim),
                );
            }
            if man.contacts[e].planted.is_some() {
                let miss = (rig.joint(skins, chain[2]) - targets[e]).length();
                let (n, sum, max) = self.misses.get();
                let (n, sum, max) = (n + 1, sum + miss, max.max(miss));
                if n == 100 {
                    log::debug!(
                        "3d: gait contacts planted: miss {:.1} mm mean, {:.0} mm max",
                        sum / n as f32 * 1000.0,
                        max * 1000.0
                    );
                    self.misses.set((0, 0.0, 0.0));
                } else {
                    self.misses.set((n, sum, max));
                }
            }
            // Feet tilt with the ground.
            if clipm.kind == Kind::Walk && man.contacts[e].planted.is_some() {
                let p = targets[e];
                let h = |dx: f32, dy: f32| ground(Vec3::new(p.x + dx, p.y + dy, 0.0));
                let n = Vec3::new(h(-0.1, 0.0) - h(0.1, 0.0), h(0.0, -0.1) - h(0.0, 0.1), 0.2).normalize_or_zero();
                let pivot = rig.joint(skins, chain[2]);
                rig.rotate_about(skins, chain[2], pivot, gait::rotation_between(up, n));
            }
            // The toes stay out of the ground (a foot pitched down at toe-off, or on a slope) and
            // clear it in the swing: up to `TOE_CLEAR` as the foot rises from its stance height.
            if clipm.kind == Kind::Walk && man.balls[e].is_none() {
                let (ankle, tip) = (rig.joint(skins, chain[2]), rig.joint(skins, toes[e]));
                let mut risen = ((ankle.z - ground(ankle) - clipm.lift[e]) / 0.04).clamp(0.0, 1.0);
                if clipm.cycle.support.contains(&None) {
                    // A run's foot leaves its ball risen: its toes clear over the release, not at
                    // once.
                    risen *= 1.0 - man.contacts[e].release;
                }
                let floor = ground(tip) + clipm.toe[e] - 0.005 + TOE_CLEAR * risen;
                let d = tip - ankle;
                if tip.z < floor && d.length() > 1e-3 {
                    let flat = Vec3::new(d.x, d.y, 0.0).length().max(1e-3);
                    let z = (floor - ankle.z).clamp(-d.length() * 0.95, d.length() * 0.95);
                    let h = (d.length() * d.length() - z * z).sqrt();
                    let want = Vec3::new(d.x / flat * h, d.y / flat * h, z);
                    rig.rotate_about(skins, chain[2], ankle, gait::rotation_between(d, want));
                }
            }
            // The hurt foot held up behind, the knee forward.
            if hop.is_some_and(|(bad, _, _)| bad == e) {
                let a = rig.joint(skins, chain[0]);
                let (l1, l2) = chain_lengths(rig, skins, chain);
                let foot = a - up * (0.62 * (l1 + l2)) - fwd * (0.3 * (l1 + l2));
                let knee = knee_towards(a, foot, l1, l2, fwd);
                place_leg(rig, skins, chain, knee, foot);
            }
            // The dragged foot hangs toe down, its toe scraping behind.
            if drag > 0.0 && limp.is_some_and(|l| l.bad as usize == e) && man.contacts[e].planted.is_none() {
                let pivot = rig.joint(skins, chain[2]);
                let down = (fwd - up * (0.7 * drag)).normalize_or_zero();
                rig.rotate_about(skins, chain[2], pivot, gait::rotation_between(fwd, down));
            }
        }
        if clipm.kind == Kind::Crawl {
            self.crawl_legs(skins, clipm, phase, man, body, ground, dt, fwd);
        }
        // The hand holds the wound.
        if let Some((side, bone, along)) = held_wound(body) {
            let arm =
                if side == 0 { ["upperarm_l", "forearm_l", "hand_l"] } else { ["upperarm_r", "forearm_r", "hand_r"] };
            let b = rig.bone(bone);
            let (j, e) = (rig.joint(skins, b), rig.end(skins, b));
            let at = j + (e - j) * along + fwd * 0.11;
            let [u, f, h] = arm.map(|n| rig.bone(n));
            rig.two_bone_ik(skins, u, f, h, at, Vec3::Z);
        }
    }
}

impl Gaits {
    /// A crawling man's legs: each leg that no longer carries is a chain of hip, knee and
    /// foot dragged behind the hips (follow the leader: each point keeps its bone's length
    /// to the one before and otherwise stays where it lay), resting on the terrain; a leg that
    /// still works pushes while the hands pull and draws up, knee out, while they reach.
    #[allow(clippy::too_many_arguments)]
    fn crawl_legs(
        &self,
        skins: &mut [Skin],
        clipm: &Measured,
        phase: f32,
        man: &mut Man,
        body: &Body,
        ground: &dyn Fn(Vec3) -> f32,
        dt: f32,
        fwd: Vec3,
    ) {
        let rig = &self.rig;
        let up = Vec3::Z;
        let legs = self.effectors(Kind::Walk);
        let pelvis = rig.joint(skins, rig.bone("hips"));
        // The pull: the body travels faster than its mean over the loop.
        let rate = (clipm.cycle.progress_at(phase + 0.03) - clipm.cycle.progress_at(phase)).rem_euclid(1.0) / 0.03;
        let pulling = rate > 1.0;
        for (e, chain) in legs.iter().enumerate() {
            let hip = rig.joint(skins, chain[0]);
            let (knee_a, foot_a) = (rig.joint(skins, chain[1]), rig.joint(skins, chain[2]));
            let (l1, l2) = chain_lengths(rig, skins, chain);
            let limb = if e == 0 { LimbId::LegL } else { LimbId::LegR };
            if body.leg(limb) > 0 {
                // The push: planted while the hands pull, drawn up while they reach.
                let push = pulling;
                let side = {
                    let s = hip - pelvis;
                    Vec3::new(s.x, s.y, 0.0).normalize_or_zero()
                };
                let mut drawn = hip - fwd * (0.5 * (l1 + l2)) + side * (0.3 * (l1 + l2));
                drawn.z = ground(drawn) + FOOT_REST + 0.04;
                let foot = man.feet[e].update(push, drawn, FOOT_REST, ground, dt);
                let knee = knee_towards(hip, foot, l1, l2, side + up * 0.3);
                place_leg(rig, skins, chain, knee, foot);
                man.legs[e] = None;
                continue;
            }
            let [mut knee, mut foot] = man.legs[e].unwrap_or([knee_a, foot_a]);
            // Fall onto the ground, then keep the bones' lengths from the hip outwards.
            knee.z -= LEG_FALL * dt;
            knee = hip + norm_or(knee - hip, knee_a - hip) * l1;
            knee.z = knee.z.max(ground(knee) + KNEE_REST);
            foot.z -= LEG_FALL * dt;
            // A little stiffness: the knee straightens as it is pulled.
            let thigh = (knee - hip).normalize_or_zero();
            let dir = norm_or(norm_or(foot - knee, thigh) * 0.85 + thigh * 0.15, thigh);
            foot = knee + dir * l2;
            foot.z = foot.z.max(ground(foot) + FOOT_REST);
            man.legs[e] = Some([knee, foot]);
            place_leg(rig, skins, chain, knee, foot);
        }
    }
}

/// The stride's scale for ground speed `speed` on a clip made for `natural` (both m/s).
fn stride_scale(speed: f32, natural: f32) -> f32 {
    if speed < 0.05 || natural < 0.05 {
        return 1.0;
    }
    let r = speed / natural;
    let soft = r.powf(STRIDE_EXP).clamp(STRIDE_SOFT[0], STRIDE_SOFT[1]);
    soft.max(r / CADENCE[1]).min(r / CADENCE[0]).clamp(STRIDE[0], STRIDE[1])
}

/// `v` normalised, else `or` normalised.
fn norm_or(v: Vec3, or: Vec3) -> Vec3 {
    if v.length_sq() > 1e-10 { v.normalize_or_zero() } else { or.normalize_or_zero() }
}

/// The lengths of a chain's two bones in the pose.
fn chain_lengths(rig: &Rig, skins: &[Skin], chain: &[usize; 3]) -> (f32, f32) {
    let [a, b, c] = chain.map(|j| rig.joint(skins, j));
    ((b - a).length(), (c - b).length())
}

/// Where the knee (or elbow) of a chain from `a` with bones `l1`, `l2` lies when its end
/// reaches towards `c`, bent towards `pole`.
fn knee_towards(a: Vec3, c: Vec3, l1: f32, l2: f32, pole: Vec3) -> Vec3 {
    let to = c - a;
    let d = to.length().clamp((l1 - l2).abs() + 1e-3, l1 + l2 - 1e-3);
    let axis = norm_or(to, Vec3::Z * -1.0);
    let x = (d * d + l1 * l1 - l2 * l2) / (2.0 * d);
    let h = (l1 * l1 - x * x).max(0.0).sqrt();
    let perp = (pole - axis * pole.dot(axis)).normalize_or_zero();
    a + axis * x + perp * h
}

/// Turns a chain's upper bone so its middle joint lies towards `knee`, then its lower bone so
/// its end points at `foot`.
fn place_leg(rig: &Rig, skins: &mut [Skin], chain: &[usize; 3], knee: Vec3, foot: Vec3) {
    let a = rig.joint(skins, chain[0]);
    let b = rig.joint(skins, chain[1]);
    rig.rotate_about(skins, chain[0], a, gait::rotation_between(b - a, knee - a));
    let b = rig.joint(skins, chain[1]);
    let c = rig.joint(skins, chain[2]);
    rig.rotate_about(skins, chain[1], b, gait::rotation_between(c - b, foot - b));
}

/// A limp: the bad effector and how bad it is (0..1); `hop` when that leg is broken at the
/// ankle or has a joint out and he hops on the other.
#[derive(Clone, Copy, Debug)]
struct Limp {
    bad: u8,
    severity: f32,
    hop: bool,
}

impl Limp {
    fn of(body: &Body, kind: Kind) -> Option<Limp> {
        if kind != Kind::Walk {
            return None;
        }
        let (l, r) = (body.leg(LimbId::LegL), body.leg(LimbId::LegR));
        let (bad, f) = if l <= r { (0u8, l) } else { (1, r) };
        let limb = body.limb(if bad == 0 { LimbId::LegL } else { LimbId::LegR });
        let hop =
            !limb.severed() && limb.fracture.is_some_and(|b| b.level >= ANKLE || b.kind == FractureKind::Dislocated);
        (f < 100 && f > 0).then(|| Limp { bad, severity: (100 - f) as f32 / 100.0, hop })
    }
}

/// A leg (0 left, 1 right) that ends in a stump above the foot.
fn stump(body: &Body, e: usize) -> bool {
    let l = body.limb(if e == 0 { LimbId::LegL } else { LimbId::LegR });
    l.severed()
}

/// The hand that holds a wound: side, the bone held and where along it.
fn held_wound(body: &Body) -> Option<(usize, &'static str, f32)> {
    if body.limb(LimbId::LegL).wounds > 0 && body.arm(LimbId::ArmL) > 0 {
        return Some((0, "thigh_l", 0.45));
    }
    if body.limb(LimbId::LegR).wounds > 0 && body.arm(LimbId::ArmR) > 0 {
        return Some((1, "thigh_r", 0.45));
    }
    if (body.chest_wounds > 0 || body.belly_open) && body.arm(LimbId::ArmR) > 0 {
        return Some((1, if body.belly_open { "spine" } else { "spine2" }, 0.5));
    }
    None
}
