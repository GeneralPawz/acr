//! Physical locomotion over animation clips (#279, `docs/body-sim.md` §2): kinematic layers
//! on a pose given as skinning matrices per bone of skeleton H (`current · rest⁻¹`, world
//! space, column-major), as [`crate::Ragdoll::skin_matrix`] gives them.
//!
//! - [`Rig`]: the hierarchy and rest joints of a skeleton; [`Rig::rotate_about`] turns a bone
//!   with everything below it, [`Rig::two_bone_ik`] reaches a target with a limb.
//! - [`Contact`]: an end effector (foot, hand) planted on the ground while it carries and
//!   blended back to the clip when it lifts, so it never slides.
//! - [`Cycle`]: the support and progress of a looping clip measured from the clip itself
//!   (which limb carries when, how far the body moves over the planted limb). The clip is then
//!   played by the distance covered (phase lock), and the visible body advances only while a
//!   limb pushes or pulls.
//! - [`limp_warp`]: asymmetric timing of a limp.

use crate::math::{Quat, Vec3};
use crate::ragdoll::BoneDesc;

pub type Skin = [[f32; 4]; 4];

fn col(m: &Skin, c: usize) -> Vec3 {
    Vec3::new(m[c][0], m[c][1], m[c][2])
}

/// `m · p` for a point.
pub fn apply(m: &Skin, p: Vec3) -> Vec3 {
    col(m, 0) * p.x + col(m, 1) * p.y + col(m, 2) * p.z + col(m, 3)
}

/// The world rotation `q` about the point `pivot`, applied after `m`.
pub fn rotated(m: &Skin, pivot: Vec3, q: Quat) -> Skin {
    let mut out = *m;
    for (c, o) in out.iter_mut().take(3).enumerate() {
        let v = q.rotate(col(m, c));
        *o = [v.x, v.y, v.z, 0.0];
    }
    let t = pivot + q.rotate(col(m, 3) - pivot);
    out[3] = [t.x, t.y, t.z, 1.0];
    out
}

/// `m` moved by `d`.
pub fn translated(m: &Skin, d: Vec3) -> Skin {
    let mut out = *m;
    out[3][0] += d.x;
    out[3][1] += d.y;
    out[3][2] += d.z;
    out
}

/// The shortest rotation taking direction `a` to direction `b`.
pub fn rotation_between(a: Vec3, b: Vec3) -> Quat {
    let (a, b) = (a.normalize_or_zero(), b.normalize_or_zero());
    let axis = a.cross(b);
    let s = axis.length();
    if s < 1e-6 {
        return Quat::IDENTITY;
    }
    Quat::from_axis_angle(axis * (1.0 / s), s.atan2(a.dot(b)))
}

/// The hierarchy and rest joints of a skeleton.
pub struct Rig {
    pub names: Vec<&'static str>,
    pub parent: Vec<Option<usize>>,
    pub head: Vec<Vec3>,
    pub tail: Vec<Vec3>,
    /// Each bone's descendants (itself first).
    below: Vec<Vec<usize>>,
}

impl Rig {
    pub fn new(bones: &[BoneDesc]) -> Rig {
        let names: Vec<&'static str> = bones.iter().map(|b| b.name).collect();
        let parent: Vec<Option<usize>> =
            bones.iter().map(|b| b.parent.and_then(|p| names.iter().position(|n| *n == p))).collect();
        let below = (0..bones.len())
            .map(|b| {
                (0..bones.len())
                    .filter(|&c| {
                        let mut n = Some(c);
                        while let Some(i) = n {
                            if i == b {
                                return true;
                            }
                            n = parent[i];
                        }
                        false
                    })
                    .collect()
            })
            .collect();
        Rig {
            names,
            parent,
            head: bones.iter().map(|b| Vec3::from_array(b.head)).collect(),
            tail: bones.iter().map(|b| Vec3::from_array(b.tail)).collect(),
            below,
        }
    }

    pub fn bone(&self, name: &str) -> usize {
        self.names.iter().position(|n| *n == name).unwrap_or(0)
    }

    /// The world position of the bone's head (its joint) in the pose.
    pub fn joint(&self, skins: &[Skin], b: usize) -> Vec3 {
        apply(&skins[b], self.head[b])
    }

    /// The world position of the bone's tail.
    pub fn end(&self, skins: &[Skin], b: usize) -> Vec3 {
        apply(&skins[b], self.tail[b])
    }

    /// Turns bone `b` and everything below it by the world rotation `q` about `pivot`.
    pub fn rotate_about(&self, skins: &mut [Skin], b: usize, pivot: Vec3, q: Quat) {
        for &c in &self.below[b] {
            skins[c] = rotated(&skins[c], pivot, q);
        }
    }

    /// Moves bone `b` and everything below it by `d`.
    pub fn translate(&self, skins: &mut [Skin], b: usize, d: Vec3) {
        for &c in &self.below[b] {
            skins[c] = translated(&skins[c], d);
        }
    }

    /// Two-bone IK: turns `upper` (about its joint) and `lower` (about its joint) so that
    /// the joint of `end` reaches `target` as nearly as the limb's length allows. The bend
    /// stays in the plane the limb already bends in (`hint` when it is straight).
    pub fn two_bone_ik(&self, skins: &mut [Skin], upper: usize, lower: usize, end: usize, target: Vec3, hint: Vec3) {
        let a = self.joint(skins, upper);
        let b = self.joint(skins, lower);
        let c = self.joint(skins, end);
        let (l1, l2) = ((b - a).length(), (c - b).length());
        if l1 < 1e-4 || l2 < 1e-4 {
            return;
        }
        let d = (target - a).length().clamp((l1 - l2).abs() + 1e-3, l1 + l2 - 1e-3);
        let (u, v) = ((b - a) * (1.0 / l1), (c - b) * (1.0 / l2));
        let mut n = u.cross(v);
        if n.length() < 1e-4 {
            n = hint;
        }
        let n = n.normalize_or_zero();
        // Bend at the middle joint: the angle between the two bones.
        let now = u.dot(v).clamp(-1.0, 1.0).acos();
        let want = ((d * d - l1 * l1 - l2 * l2) / (2.0 * l1 * l2)).clamp(-1.0, 1.0).acos();
        self.rotate_about(skins, lower, b, Quat::from_axis_angle(n, want - now));
        // Then swing the whole limb onto the target.
        let c = self.joint(skins, end);
        self.rotate_about(skins, upper, a, rotation_between(c - a, target - a));
    }
}

/// An end effector held on the ground while it carries.
#[derive(Clone, Copy, Debug, Default)]
pub struct Contact {
    /// Where it is planted (world), while planted.
    pub planted: Option<Vec3>,
    /// Blend from the last planted point back to the clip after lifting (1 → 0).
    pub release: f32,
    pub last: Vec3,
}

impl Contact {
    /// The target of the effector this frame: the planted point while it `carries`, else the
    /// clip's point `anim` (blended out of the last plant over 0.12 s). `ground` is the
    /// ground height under a point.
    pub fn update(&mut self, carries: bool, anim: Vec3, lift: f32, ground: &dyn Fn(Vec3) -> f32, dt: f32) -> Vec3 {
        if carries {
            let p = *self.planted.get_or_insert_with(|| Vec3::new(anim.x, anim.y, ground(anim) + lift));
            self.release = 1.0;
            self.last = p;
            return p;
        }
        self.planted = None;
        self.release = (self.release - dt / 0.12).max(0.0);
        let k = self.release * self.release * (3.0 - 2.0 * self.release);
        let on = Vec3::new(anim.x, anim.y, anim.z.max(ground(anim) + lift * 0.5));
        on * (1.0 - k) + self.last * k
    }
}

/// How far a carrying ankle rises over its lowest before a run's foot rolls over its ball (m).
pub const HEEL_OFF: f32 = 0.005;

/// Samples of a looping clip.
pub const SAMPLES: usize = 48;

/// What a looping clip does with its contacts, measured from the clip.
#[derive(Clone, Debug)]
pub struct Cycle {
    /// The body's forward travel over one loop (m), from the carrying limbs.
    pub length: f32,
    /// Which effector carries at each sample (index into the effectors measured, or none).
    pub support: [Option<u8>; SAMPLES],
    /// The share of the loop's travel done by each sample (0 → 1, monotone).
    pub progress: [f32; SAMPLES + 1],
}

impl Cycle {
    /// Measures a clip: `points[i][e]` is effector `e`'s position at sample `i` in the
    /// body's frame (forward is `-y`, as skeleton H). The lowest effector within `reach` of
    /// the ground (`z` under `ground_z`) carries; the body travels as far as it is pushed back.
    /// With `flight` a clip with flights (samples no effector carries: a run's both feet
    /// up) travels evenly, as its body does: each sample by the median step of the samples
    /// that carry (the foot flat; not its landing, its roll onto the ball or the toe-off,
    /// which move the ankle slower than the body). A clip without flights measures as
    /// without `flight`.
    pub fn measure(points: &[Vec<Vec3>], ground_z: f32, flight: bool) -> Cycle {
        let n = points.len().min(SAMPLES);
        let mut support = [None; SAMPLES];
        for (i, s) in support.iter_mut().enumerate().take(n) {
            let lowest = points[i].iter().enumerate().min_by(|a, b| a.1.z.total_cmp(&b.1.z));
            *s = lowest.filter(|(_, p)| p.z < ground_z).map(|(e, _)| e as u8);
        }
        let mut step = [0.0f32; SAMPLES];
        for i in 0..n {
            if let Some(e) = support[i] {
                let (p, q) = (points[i][e as usize], points[(i + 1) % n][e as usize]);
                // Moving backwards (+y) under the body pushes it forwards.
                step[i] = (q.y - p.y).max(0.0);
            }
        }
        if flight && support[..n].iter().any(Option::is_some) && support[..n].contains(&None) {
            let mut carried: Vec<f32> = (0..n).filter(|&i| support[i].is_some()).map(|i| step[i]).collect();
            carried.sort_by(f32::total_cmp);
            let even = carried[carried.len() / 2];
            step[..n].fill(even);
        }
        let length: f32 = step.iter().sum();
        let mut progress = [0.0f32; SAMPLES + 1];
        for i in 0..SAMPLES {
            let s = if length > 1e-3 { step[i] / length } else { 1.0 / n.max(1) as f32 };
            progress[i + 1] = progress[i] + if i < n { s } else { 0.0 };
        }
        let total = progress[n].max(1e-6);
        for p in &mut progress {
            *p = (*p / total).min(1.0);
        }
        Cycle { length, support, progress }
    }

    /// The share of the loop's travel done at `phase` (0..1).
    pub fn progress_at(&self, phase: f32) -> f32 {
        let x = phase.rem_euclid(1.0) * SAMPLES as f32;
        let i = (x as usize).min(SAMPLES - 1);
        let f = x - i as f32;
        self.progress[i] * (1.0 - f) + self.progress[i + 1] * f
    }

    /// The effector carrying at `phase`.
    pub fn support_at(&self, phase: f32) -> Option<u8> {
        self.support[((phase.rem_euclid(1.0) * SAMPLES as f32) as usize).min(SAMPLES - 1)]
    }
}

/// Phase lock with a visible lag: the clip's phase follows the distance the sim covered,
/// and the visible body is placed where the clip's own pushes and pulls have brought it,
/// within one loop of the sim position.
#[derive(Clone, Copy, Debug, Default)]
pub struct Follow {
    /// Distance covered by the sim (m).
    pub dist: f32,
}

impl Follow {
    /// The clip phase (loops) and the visible body's lead over the sim position along the
    /// way (m; negative behind) for a loop of `length` metres.
    pub fn place(&self, cycle: &Cycle, length: f32) -> (f32, f32) {
        let loops = self.dist / length.max(0.05);
        let phase = loops.rem_euclid(1.0);
        let visible = (loops.floor() + cycle.progress_at(phase)) * length;
        (phase, visible - self.dist)
    }
}

/// A limp's timing: the clip phase for a uniform phase `u` when the loop's samples where
/// effector `bad` carries take `1 - 0.6 · severity` of their time (a short, quick step onto
/// the bad leg) and the rest take the remainder.
pub fn limp_warp(cycle: &Cycle, bad: u8, severity: f32, u: f32) -> f32 {
    let w = |i: usize| if cycle.support[i] == Some(bad) { 1.0 - 0.6 * severity.clamp(0.0, 1.0) } else { 1.0 };
    let total: f32 = (0..SAMPLES).map(w).sum();
    let mut target = u.rem_euclid(1.0) * total;
    for i in 0..SAMPLES {
        let wi = w(i);
        if target <= wi {
            return (i as f32 + target / wi) / SAMPLES as f32;
        }
        target -= wi;
    }
    1.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skeleton_h;

    fn rest() -> (Rig, Vec<Skin>) {
        let rig = Rig::new(&skeleton_h());
        let id = crate::math::mat4_from_rt(Vec3::ZERO, Quat::IDENTITY);
        let n = rig.names.len();
        (rig, vec![id; n])
    }

    #[test]
    fn leg_ik_reaches_a_target_and_keeps_the_bones() {
        let (rig, mut skins) = rest();
        let (t, s, f) = (rig.bone("thigh_l"), rig.bone("shin_l"), rig.bone("foot_l"));
        let knee = rig.joint(&skins, s);
        let len = (rig.joint(&skins, f) - knee).length();
        let target = Vec3::new(0.1, -0.2, 0.25);
        rig.two_bone_ik(&mut skins, t, s, f, target, Vec3::X);
        assert!((rig.joint(&skins, f) - target).length() < 1e-3, "{:?}", rig.joint(&skins, f));
        let knee = rig.joint(&skins, s);
        assert!(((rig.joint(&skins, f) - knee).length() - len).abs() < 1e-4);
        // The hip stays where it is.
        assert!((rig.joint(&skins, t) - rig.head[t]).length() < 1e-5);
    }

    #[test]
    fn a_planted_contact_does_not_slide() {
        let mut c = Contact::default();
        let g = |_: Vec3| 0.0;
        let a = c.update(true, Vec3::new(0.0, 0.0, 0.1), 0.08, &g, 0.016);
        let b = c.update(true, Vec3::new(0.0, -0.3, 0.1), 0.08, &g, 0.016);
        assert_eq!(a, b);
        let free = c.update(false, Vec3::new(0.0, -0.3, 0.3), 0.08, &g, 0.016);
        assert!(free.y < 0.0 && free.y > -0.3);
    }

    /// A clip whose two feet each carry half the loop, sliding back 0.4 m.
    fn walk() -> Cycle {
        let pts: Vec<Vec<Vec3>> = (0..SAMPLES)
            .map(|i| {
                let ph = i as f32 / SAMPLES as f32;
                let foot = |off: f32| {
                    let p = (ph + off).rem_euclid(1.0);
                    if p < 0.5 {
                        Vec3::new(0.0, -0.2 + 0.8 * p, 0.0)
                    } else {
                        Vec3::new(0.0, 0.2 - 0.8 * (p - 0.5), 0.1)
                    }
                };
                vec![foot(0.0), foot(0.5)]
            })
            .collect();
        Cycle::measure(&pts, 0.05, false)
    }

    #[test]
    fn cycle_measures_travel_and_support() {
        let c = walk();
        assert!((c.length - 0.8).abs() < 0.05, "{}", c.length);
        assert_eq!(c.support_at(0.1), Some(0));
        assert_eq!(c.support_at(0.6), Some(1));
        assert!((c.progress_at(0.5) - 0.5).abs() < 0.05);
    }

    #[test]
    fn a_flight_travels_at_the_stance_speed() {
        // Each foot carries 0.3 of the loop, sliding back 0.6 m a loop; both are up for the
        // 0.2 between (a run's flights): the body goes on at the stance speed.
        let pts: Vec<Vec<Vec3>> = (0..SAMPLES)
            .map(|i| {
                let ph = i as f32 / SAMPLES as f32;
                let foot = |off: f32| {
                    let p = (ph + off).rem_euclid(1.0);
                    if p < 0.3 {
                        Vec3::new(0.0, -0.3 + 2.0 * p, 0.0)
                    } else {
                        Vec3::new(0.0, 0.3 - 0.6 * (p - 0.3) / 0.7, 0.2)
                    }
                };
                vec![foot(0.0), foot(0.5)]
            })
            .collect();
        let c = Cycle::measure(&pts, 0.05, true);
        assert!((c.length - 2.0).abs() < 0.08, "{}", c.length);
        assert_eq!(c.support_at(0.4), None);
        assert!((c.progress_at(0.5) - 0.5).abs() < 0.03, "{}", c.progress_at(0.5));
        assert!(c.progress_at(0.4) > c.progress_at(0.31) + 0.05);
    }

    #[test]
    fn follow_stays_within_a_loop_and_limp_shortens_the_bad_stance() {
        let c = walk();
        for k in 0..100 {
            let f = Follow { dist: k as f32 * 0.037 };
            let (_, lead) = f.place(&c, c.length);
            assert!(lead.abs() <= c.length);
        }
        // Half the uniform time lands past the bad (0) stance, which is shortened.
        let mid = limp_warp(&c, 0, 1.0, 0.3);
        assert!(mid > 0.5, "{mid}");
        assert!((limp_warp(&c, 0, 0.0, 0.3) - 0.3).abs() < 1e-3);
    }
}
