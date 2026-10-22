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
//!   a leg wound (or the right hand for a chest or belly wound) holds the wound. A lost foot
//!   hobbles on the stump.

use std::collections::HashMap;

use acr_physics::gait::{self, Contact, Cycle, Follow, Rig, Skin};
use acr_physics::{Vec3, skeleton_h};
use acr_render::model::ModelData;
use acr_render::terrain::TerrainData;
use acr_render::view3d;
use acr_sim::body::{Body, LimbId};

use super::ragdoll_view::{Drawn, Mapping, animated_skins, ground_height, palette_of, physics_of};

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
    duration: f32,
    /// Height of each effector's joint over the ground when it carries (m).
    lift: [f32; 2],
}

#[derive(Default)]
struct Man {
    clip: Option<usize>,
    vis: Option<Vec3>,
    dir: Vec3,
    follow: Follow,
    contacts: [Contact; 2],
    sink: f32,
    seen: bool,
}

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

    /// Measures a clip once: its loop of support and travel in the body's frame.
    fn measure(&self, map: &Mapping, data: &ModelData, clip: usize) -> Option<Measured> {
        let c = data.clips.get(clip)?;
        let kind = kind_of(&c.name)?;
        let eff = self.effectors(kind);
        let at_origin = view3d::model_matrix([0.0, 0.0, 0.0], 0.0, 1.0);
        let points: Vec<Vec<Vec3>> = (0..gait::SAMPLES)
            .map(|i| {
                let skins = animated_skins(
                    map,
                    &data.pose(Some(clip), c.duration * i as f32 / gait::SAMPLES as f32),
                    &at_origin,
                );
                eff.iter().map(|e| self.rig.joint(&skins, e[2])).collect()
            })
            .collect();
        let low = points.iter().flatten().map(|p| p.z).fold(f32::MAX, f32::min);
        let cycle = Cycle::measure(&points, low + 0.05);
        let lift = std::array::from_fn(|e| points.iter().map(|p| p[e].z).fold(f32::MAX, f32::min));
        log::info!("3d: gait clip {} ({kind:?}): {:.2} m per loop of {:.2} s", c.name, cycle.length, c.duration);
        (cycle.length > 0.05 && c.duration > 0.0).then_some(Measured { kind, cycle, duration: c.duration, lift })
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
            let Some(Some(map)) = maps.get(&d.model) else { continue };
            if !self.clips.contains_key(&(d.model, clip)) {
                let m = self.measure(map, d.data, clip);
                self.clips.insert((d.model, clip), m);
            }
            let Some(Some(clipm)) = self.clips.get(&(d.model, clip)) else {
                self.men.remove(&d.unit.id.0);
                continue;
            };
            let mut taken = self.men.remove(&d.unit.id.0).unwrap_or_default();
            let man = &mut taken;
            man.seen = true;
            if man.clip != Some(clip) {
                *man = Man { clip: Some(clip), seen: true, ..Man::default() };
            }
            // The sim position, smoothed over its ticks; the distance covered along the way.
            let sim = physics_of([d.transform[3][0], d.transform[3][1], d.transform[3][2]]);
            let vis = match man.vis {
                Some(v) if (sim - v).length() < 2.0 => v + (sim - v) * (1.0 - (-dt * 10.0).exp()),
                _ => sim,
            };
            let step = Vec3::new(vis.x - man.vis.unwrap_or(vis).x, vis.y - man.vis.unwrap_or(vis).y, 0.0);
            if step.length() > 1e-5 {
                man.dir = (man.dir * 0.8 + step.normalize_or_zero() * 0.2).normalize_or_zero();
            }
            man.follow.dist += step.length();
            man.vis = Some(vis);
            let body = &d.unit.body;
            let limp = Limp::of(body, clipm.kind);
            let (mut phase, lead) = man.follow.place(&clipm.cycle, clipm.cycle.length);
            if let Some(l) = limp {
                phase = gait::limp_warp(&clipm.cycle, l.bad, l.severity, phase);
            }
            let pose = d.data.pose(Some(clip), phase * clipm.duration);
            let mut skins = animated_skins(map, &pose, &d.transform);
            // The visible body: where the pushes and pulls brought it.
            let shift = (vis - sim) + man.dir * lead;
            self.rig.translate(&mut skins, 0, Vec3::new(shift.x, shift.y, 0.0));
            self.pose(&mut skins, clipm, man, phase, limp, body, &ground, dt);
            poses.insert(d.index, palette_of(map, &skins, &d.transform));
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
        // Contacts: planted where they carry.
        let mut targets = [Vec3::ZERO; 2];
        let mut need: f32 = 0.0;
        for (e, chain) in eff.iter().enumerate() {
            let anim = rig.joint(skins, chain[2]);
            let mut lift = clipm.lift[e].max(0.02);
            if clipm.kind == Kind::Walk && stump(body, e) {
                lift = 0.03;
            }
            let carries = support == Some(e as u8);
            let t = man.contacts[e].update(carries, anim, lift, ground, dt);
            targets[e] = t;
            if clipm.kind == Kind::Walk {
                let a = rig.joint(skins, chain[0]);
                let reach = (rig.joint(skins, chain[1]) - a).length() + (anim - rig.joint(skins, chain[1])).length();
                need = need.max((t - a).length() - reach * 0.995);
            }
        }
        // Hips lower where a leg cannot reach (downhill, a long planted step).
        if clipm.kind == Kind::Walk {
            let want = need.clamp(0.0, 0.25);
            man.sink += (want - man.sink) * (1.0 - (-dt * 12.0).exp()).max(if want > man.sink { 0.5 } else { 0.0 });
            rig.translate(skins, hips, Vec3::new(0.0, 0.0, -man.sink));
        }
        for (e, chain) in eff.iter().enumerate() {
            let side = if e == 0 { Vec3::X } else { Vec3::X * -1.0 };
            let hint = if clipm.kind == Kind::Walk { fwd.cross(up) * -1.0 } else { side };
            rig.two_bone_ik(skins, chain[0], chain[1], chain[2], targets[e], hint);
            if man.contacts[e].planted.is_some() {
                let miss = (rig.joint(skins, chain[2]) - targets[e]).length();
                let (n, sum, max) = self.misses.get();
                let (n, sum, max) = (n + 1, sum + miss, max.max(miss));
                if n == 100 {
                    log::info!(
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

/// A limp: the bad effector and how bad it is (0..1).
#[derive(Clone, Copy, Debug)]
struct Limp {
    bad: u8,
    severity: f32,
}

impl Limp {
    fn of(body: &Body, kind: Kind) -> Option<Limp> {
        if kind != Kind::Walk {
            return None;
        }
        let (l, r) = (body.leg(LimbId::LegL), body.leg(LimbId::LegR));
        let (bad, f) = if l <= r { (0u8, l) } else { (1, r) };
        (f < 100 && f > 0).then(|| Limp { bad, severity: (100 - f) as f32 / 100.0 })
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
