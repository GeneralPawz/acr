//! Bodies cut, broken and torn (`docs/damage-model.md` §3, `docs/rendering-3d.md`).
//!
//! A [`Damage`] on a [`crate::model_renderer::ModelInstance`] cuts limbs at any level, bends
//! them at fractures, tears the trunk and takes the head, or keeps only a cut-off piece
//! ([`Keep::Piece`]). The shader does the cutting per vertex from `_PART` and `_LIMB`; this
//! module holds the rest-pose [`Rig`] the levels are measured on (limb chains from the
//! skeleton's bind pose and radius profiles), the packing of the per-instance data texels
//! and the placement of the stump caps.
//!
//! Levels are bytes as in `acr_sim::body`: 0 at the limb's root (shoulder, hip), 255 at its
//! tip; the segment boundaries are [`ELBOW`], [`WRIST`], [`KNEE`] and [`ANKLE`].

use crate::math::{self, Mat4, Vec3};
use crate::model::{Material, ModelData, Node, Primitive, Slot, Vertex};

pub const ELBOW: u8 = 110;
pub const WRIST: u8 = 205;
pub const KNEE: u8 = 120;
pub const ANKLE: u8 = 215;
/// A level that cuts nothing.
pub const WHOLE: u8 = 255;
/// Samples of a radius profile along the level (`acr_limb_radius`).
pub const RADIUS_SAMPLES: usize = 16;

/// The four limbs in the order of `acr_sim::body::Body::limbs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Limb {
    ArmL = 0,
    ArmR = 1,
    LegL = 2,
    LegR = 3,
}

impl Limb {
    pub const ALL: [Limb; 4] = [Limb::ArmL, Limb::ArmR, Limb::LegL, Limb::LegR];

    pub fn name(self) -> &'static str {
        ["arm_l", "arm_r", "leg_l", "leg_r"][self as usize]
    }

    pub fn from_name(s: &str) -> Option<Limb> {
        Limb::ALL.into_iter().find(|l| l.name() == s)
    }

    pub fn is_arm(self) -> bool {
        matches!(self, Limb::ArmL | Limb::ArmR)
    }

    /// The limb of a body part of `rig.PARTS` (2..9), `None` for the torso and head.
    pub fn of_part(part: u32) -> Option<Limb> {
        match part {
            2 | 3 => Some(Limb::ArmL),
            4 | 5 => Some(Limb::ArmR),
            6 | 7 => Some(Limb::LegL),
            8 | 9 => Some(Limb::LegR),
            _ => None,
        }
    }
}

/// A broken bone: everything past `level` turns by `angle` radians about `axis` (rest
/// pose, glTF metres), about the point of the limb at `level`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fracture {
    pub level: u8,
    pub axis: Vec3,
    pub angle: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LimbDamage {
    /// The limb ends at this level; [`WHOLE`] (255) cuts nothing.
    pub cut: u8,
    pub fracture: Option<Fracture>,
}

impl Default for LimbDamage {
    fn default() -> Self {
        Self { cut: WHOLE, fracture: None }
    }
}

/// Which part of the damaged body an instance draws.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Keep {
    /// The body without what was cut off.
    #[default]
    Body,
    /// Only the span `from..to` (levels) of `limb`: a severed piece (`to` < 255 when the
    /// piece itself was cut again further out).
    Piece { limb: Limb, from: u8, to: u8 },
    /// The upper half of a body torn in two at [`Damage::trunk_cut`]: trunk above the cut,
    /// arms (as cut) and head (unless gone). The `Body` instance keeps the lower half.
    Upper,
}

/// Most wound volumes per body ([`Damage::volumes`]).
pub const MAX_VOLUMES: usize = 8;

/// How the boundary of a [`Volume`] looks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum VolumeKind {
    /// Torn out by a ball or a shell fragment: a very ragged, fibrous boundary.
    #[default]
    Tear,
    /// Hacked by a blade: a cleaner, still uneven edge.
    Gash,
}

/// Tissue torn out of a body (#280, `docs/body-sim.md` §3): a capsule from `a` to `b` (a
/// sphere when they meet) of `radius`, in the rest pose (glTF metres). The shader discards
/// what lies inside, with a noisy boundary from `seed`, and shades the hollow as flesh.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Volume {
    pub a: Vec3,
    pub b: Vec3,
    pub radius: f32,
    /// 0..1: varies the noise of the boundary.
    pub seed: f32,
    pub kind: VolumeKind,
}

impl Volume {
    pub fn sphere(c: Vec3, radius: f32, seed: f32) -> Volume {
        Volume { a: c, b: c, radius, seed, kind: VolumeKind::Tear }
    }

    /// Distance of `p` from the capsule's axis segment.
    pub fn distance(&self, p: Vec3) -> f32 {
        let ab = math::sub(self.b, self.a);
        let t = (math::dot(math::sub(p, self.a), ab) / math::dot(ab, ab).max(1e-9)).clamp(0.0, 1.0);
        let d = math::sub(p, math::add(self.a, math::scale(ab, t)));
        math::dot(d, d).sqrt()
    }

    /// Cubic metres taken out (ignoring overlap and the body's outline).
    pub fn size(&self) -> f32 {
        let ab = math::sub(self.b, self.a);
        let r = self.radius;
        std::f32::consts::PI * r * r * (math::dot(ab, ab).sqrt() + 4.0 / 3.0 * r)
    }

    /// The two data texels (`model.wgsl`, `holes`): [a, radius], [b, kind + seed].
    pub fn gpu(&self) -> [[f32; 4]; 2] {
        let kind = match self.kind {
            VolumeKind::Tear => 0.0,
            VolumeKind::Gash => 1.0,
        };
        [
            [self.a[0], self.a[1], self.a[2], self.radius],
            [self.b[0], self.b[1], self.b[2], kind + self.seed.clamp(0.0, 0.999)],
        ]
    }
}

/// The damage drawn on one instance. The default draws the body whole.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Damage {
    /// arm_l, arm_r, leg_l, leg_r.
    pub limbs: [LimbDamage; 4],
    /// Torn in two through the trunk at this level (0 at the hips, 255 at the neck).
    pub trunk_cut: Option<u8>,
    pub head_gone: bool,
    pub belly_open: bool,
    pub keep: Keep,
    /// Tissue torn out (#280): the first `volumes_len`.
    pub volumes: [Volume; MAX_VOLUMES],
    pub volumes_len: u8,
}

impl Damage {
    /// Draws the body whole: no data texels beyond the wounds are needed.
    pub fn is_none(&self) -> bool {
        *self == Damage::default()
    }

    pub fn limb(&self, l: Limb) -> &LimbDamage {
        &self.limbs[l as usize]
    }

    pub fn limb_mut(&mut self, l: Limb) -> &mut LimbDamage {
        &mut self.limbs[l as usize]
    }

    /// The wound volumes.
    pub fn volumes(&self) -> &[Volume] {
        &self.volumes[..usize::from(self.volumes_len).min(MAX_VOLUMES)]
    }

    /// Adds a wound volume; when full, the smallest gives way to a larger one.
    pub fn push_volume(&mut self, v: Volume) {
        let n = usize::from(self.volumes_len);
        if n < MAX_VOLUMES {
            self.volumes[n] = v;
            self.volumes_len += 1;
        } else if let Some((i, small)) =
            self.volumes.iter().enumerate().min_by(|a, b| a.1.radius.total_cmp(&b.1.radius))
            && small.radius < v.radius
        {
            self.volumes[i] = v;
        }
    }

    /// Limbs whose whole cross-section a volume takes come off there: the cut moves into
    /// the volume (hidden by its ragged boundary), so the stump and the severed piece end
    /// in torn flesh rather than a flat cap. A torso volume through the shoulder takes the
    /// arm at the root.
    pub fn sever(&mut self, rig: &Rig) {
        const STEPS: usize = 64;
        for l in Limb::ALL {
            let chain = &rig.limbs[l as usize];
            let vols = self.volumes();
            let covered = |i: usize| {
                let lf = i as f32 / STEPS as f32;
                let (p, _, _) = chain.at(lf);
                let r = chain.radius_at(lf) * 0.8;
                vols.iter().any(|v| v.distance(p) + r < v.radius)
            };
            if let Some(first) = (0..=STEPS).find(|&i| covered(i)) {
                let last = (first..=STEPS).take_while(|&i| covered(i)).last().unwrap_or(first);
                let mid = ((first + last) as f32 / 2.0 / STEPS as f32 * 255.0).round() as u8;
                let cut = &mut self.limb_mut(l).cut;
                *cut = (*cut).min(mid.min(WHOLE - 1));
            }
        }
    }

    /// The instance that draws the piece cut off `limb` (from its cut to the tip).
    pub fn piece(&self, limb: Limb) -> Option<Damage> {
        let cut = self.limb(limb).cut;
        (cut < WHOLE).then_some(Damage { keep: Keep::Piece { limb, from: cut, to: WHOLE }, ..*self })
    }

    /// Parses a debug spec (`ACR_DEBUG_DAMAGE`): comma separated `cut:<limb>:<level>`,
    /// `frac:<limb>:<level>:<degrees>`, `trunk:<level>`, `head`, `belly`,
    /// `vol:<ax>:<ay>:<az>:<bx>:<by>:<bz>:<radius>` (a wound volume), and one of
    /// `piece:<limb>` / `upper` for the kept part. `<limb>` is arm_l, arm_r, leg_l or leg_r.
    pub fn parse(spec: &str) -> Result<Damage, String> {
        let mut d = Damage::default();
        for tok in spec.split(',').map(str::trim).filter(|t| !t.is_empty()) {
            let f: Vec<&str> = tok.split(':').collect();
            let limb = |i: usize| f.get(i).and_then(|s| Limb::from_name(s)).ok_or(format!("bad limb in {tok}"));
            let num = |i: usize| f.get(i).and_then(|s| s.parse::<f32>().ok()).ok_or(format!("bad number in {tok}"));
            match f[0] {
                "cut" => d.limb_mut(limb(1)?).cut = num(2)? as u8,
                "frac" => {
                    let l = limb(1)?;
                    // Bent forwards (about x), the way a broken shin or forearm folds.
                    d.limb_mut(l).fracture =
                        Some(Fracture { level: num(2)? as u8, axis: [1.0, 0.0, 0.0], angle: num(3)?.to_radians() });
                }
                "trunk" => d.trunk_cut = Some(num(1)? as u8),
                "head" => d.head_gone = true,
                "belly" => d.belly_open = true,
                "piece" => {
                    let l = limb(1)?;
                    d.keep = Keep::Piece { limb: l, from: d.limb(l).cut, to: WHOLE };
                }
                "upper" => d.keep = Keep::Upper,
                // vol:ax:ay:az:bx:by:bz:radius (rest pose, metres)
                "vol" => {
                    let a = [num(1)?, num(2)?, num(3)?];
                    let b = [num(4)?, num(5)?, num(6)?];
                    d.push_volume(Volume { a, b, radius: num(7)?, seed: 0.37, kind: VolumeKind::Tear });
                }
                _ => return Err(format!("unknown damage token {tok}")),
            }
        }
        Ok(d)
    }
}

/// A level as the shader compares it (0..1); [`WHOLE`] → 2 (past every vertex).
pub fn level_f(level: u8) -> f32 {
    if level == WHOLE { 2.0 } else { f32::from(level) / 255.0 }
}

/// One limb in the rest pose (bind pose, glTF metres): points along the chain at levels,
/// the palette slot that moves each segment and the radius profile.
#[derive(Debug, Clone, PartialEq)]
pub struct Chain {
    /// (level 0..1, rest point, slot of the segment starting here).
    pub points: Vec<(f32, Vec3, u32)>,
    pub radius: [f32; RADIUS_SAMPLES],
}

impl Chain {
    /// The point, direction (towards the tip) and slot at `level` (0..1).
    pub fn at(&self, level: f32) -> (Vec3, Vec3, u32) {
        let n = self.points.len();
        let mut i = 0;
        while i + 2 < n && level > self.points[i + 1].0 {
            i += 1;
        }
        let (l0, p0, slot) = self.points[i];
        let (l1, p1, _) = self.points[i + 1];
        let t = ((level - l0) / (l1 - l0).max(1e-6)).clamp(0.0, 1.0);
        let d = math::sub(p1, p0);
        (math::add(p0, math::scale(d, t)), normalize(d), slot)
    }

    /// The level (0..1) of the point of the chain nearest to `p`.
    pub fn project(&self, p: Vec3) -> f32 {
        let mut best = (f32::MAX, 0.0);
        for w in self.points.windows(2) {
            let (l0, a, _) = w[0];
            let (l1, b, _) = w[1];
            let ab = math::sub(b, a);
            let t = (math::dot(math::sub(p, a), ab) / math::dot(ab, ab).max(1e-9)).clamp(0.0, 1.0);
            let q = math::add(a, math::scale(ab, t));
            let d = math::sub(p, q);
            let d2 = math::dot(d, d);
            if d2 < best.0 {
                best = (d2, l0 + (l1 - l0) * t);
            }
        }
        best.1
    }

    /// Distance of `p` from the chain's axis.
    fn distance(&self, p: Vec3) -> f32 {
        let (q, _, _) = self.at(self.project(p));
        let d = math::sub(p, q);
        math::dot(d, d).sqrt()
    }

    /// Outer radius at `level` (0..1), interpolated from the profile.
    pub fn radius_at(&self, level: f32) -> f32 {
        profile_at(&self.radius, level)
    }
}

fn profile_at(r: &[f32; RADIUS_SAMPLES], level: f32) -> f32 {
    let x = level.clamp(0.0, 1.0) * (RADIUS_SAMPLES - 1) as f32;
    let i = (x.floor() as usize).min(RADIUS_SAMPLES - 2);
    r[i] + (r[i + 1] - r[i]) * (x - i as f32)
}

fn normalize(v: Vec3) -> Vec3 {
    let l = math::dot(v, v).sqrt();
    if l > 1e-9 { math::scale(v, 1.0 / l) } else { [0.0, 1.0, 0.0] }
}

/// The skeleton H measures of a model: its limb chains and trunk (rest pose).
#[derive(Debug, Clone, PartialEq)]
pub struct Rig {
    pub limbs: [Chain; 4],
    /// Hips to neck: level 0..1 along it, the trunk's radius profile by that level.
    pub trunk: Chain,
    /// Neck joint and the head's slot (the cap where the head is gone).
    pub neck: (Vec3, u32),
}

/// Inverse of the affine part of `m` applied to the origin: where a joint with inverse
/// bind matrix `m` sits in the bind pose.
fn joint_position(m: &Mat4) -> Vec3 {
    // m · p + t = 0  →  p = −A⁻¹ t (Cramer).
    let a = |r: usize, c: usize| m[c][r];
    let t = [-m[3][0], -m[3][1], -m[3][2]];
    let det = a(0, 0) * (a(1, 1) * a(2, 2) - a(1, 2) * a(2, 1)) - a(0, 1) * (a(1, 0) * a(2, 2) - a(1, 2) * a(2, 0))
        + a(0, 2) * (a(1, 0) * a(2, 1) - a(1, 1) * a(2, 0));
    if det.abs() < 1e-12 {
        return [0.0; 3];
    }
    let col = |k: usize| -> f32 {
        let e = |r: usize, c: usize| if c == k { t[r] } else { a(r, c) };
        e(0, 0) * (e(1, 1) * e(2, 2) - e(1, 2) * e(2, 1)) - e(0, 1) * (e(1, 0) * e(2, 2) - e(1, 2) * e(2, 0))
            + e(0, 2) * (e(1, 0) * e(2, 1) - e(1, 1) * e(2, 0))
    };
    [col(0) / det, col(1) / det, col(2) / det]
}

impl Rig {
    /// From the skin joints named as in `rig.py` (`upperarm_l`, `forearm_l`, `hand_l`,
    /// `thigh_l`, `shin_l`, `foot_l`, `toe_l`, `hips`, `neck`, `head`); `None` without them.
    pub fn from_skeleton(nodes: &[Node], slots: &[Slot]) -> Option<Rig> {
        let joint = |name: &str| -> Option<(Vec3, u32)> {
            slots.iter().enumerate().find_map(|(i, s)| match s {
                Slot::Joint { node, inverse_bind } if nodes.get(*node).is_some_and(|n| n.name == name) => {
                    Some((joint_position(inverse_bind), i as u32))
                }
                _ => None,
            })
        };
        let arm = |s: &str| -> Option<Chain> {
            let (sh, s0) = joint(&format!("upperarm_{s}"))?;
            let (el, s1) = joint(&format!("forearm_{s}"))?;
            let (wr, s2) = joint(&format!("hand_{s}"))?;
            let tip = math::add(wr, math::scale(normalize(math::sub(wr, el)), 0.17));
            let points = vec![(0.0, sh, s0), (level_f(ELBOW), el, s1), (level_f(WRIST), wr, s2), (1.0, tip, s2)];
            Some(Chain { points, radius: [0.05; RADIUS_SAMPLES] })
        };
        let leg = |s: &str| -> Option<Chain> {
            let (hip, s0) = joint(&format!("thigh_{s}"))?;
            let (kn, s1) = joint(&format!("shin_{s}"))?;
            let (an, s2) = joint(&format!("foot_{s}"))?;
            let mut points = vec![(0.0, hip, s0), (level_f(KNEE), kn, s1), (level_f(ANKLE), an, s2)];
            match joint(&format!("toe_{s}")) {
                Some((toe, s3)) => {
                    let tip = math::add(toe, math::scale(normalize(math::sub(toe, an)), 0.07));
                    let (a, b) = (math::sub(toe, an), math::sub(tip, toe));
                    let (la, lb) = (math::dot(a, a).sqrt(), math::dot(b, b).sqrt());
                    let at = level_f(ANKLE) + (1.0 - level_f(ANKLE)) * la / (la + lb).max(1e-6);
                    points.push((at, toe, s3));
                    points.push((1.0, tip, s3));
                }
                None => points.push((1.0, math::add(an, [0.0, -0.08, 0.15]), s2)),
            }
            Some(Chain { points, radius: [0.07; RADIUS_SAMPLES] })
        };
        let (hips, sh) = joint("hips")?;
        let (neck, sn) = joint("neck")?;
        let spine = joint("spine1").or(joint("spine")).map_or(sh, |j| j.1);
        let head = joint("head").map_or(sn, |j| j.1);
        let trunk = Chain {
            points: vec![(0.0, hips, sh), (0.5, math::scale(math::add(hips, neck), 0.5), spine), (1.0, neck, spine)],
            radius: [0.16; RADIUS_SAMPLES],
        };
        Some(Rig { limbs: [arm("l")?, arm("r")?, leg("l")?, leg("r")?], trunk, neck: (neck, head) })
    }

    /// Fills the radius profiles from the vertices (rest pose): the outer radius of each
    /// sample's band, smoothed, gaps filled from the neighbours. `vertices` are
    /// (position, part, level) of the skinned meshes.
    pub fn measure_radii(&mut self, vertices: impl Iterator<Item = (Vec3, u32, f32)>) {
        let mut max = [[0.0f32; RADIUS_SAMPLES]; 5];
        for (p, part, level) in vertices {
            let (chain, k, level) = match Limb::of_part(part) {
                Some(l) if level >= 0.0 => (&self.limbs[l as usize], l as usize, level),
                _ if part == 0 => (&self.trunk, 4, self.trunk.project(p)),
                _ => continue,
            };
            let i = ((level * (RADIUS_SAMPLES - 1) as f32).round() as usize).min(RADIUS_SAMPLES - 1);
            max[k][i] = max[k][i].max(chain.distance(p));
        }
        for (k, m) in max.iter().enumerate() {
            let chain = if k < 4 { &mut self.limbs[k] } else { &mut self.trunk };
            if m.iter().all(|&r| r <= 0.0) {
                continue;
            }
            let mut r = *m;
            for (i, ri) in r.iter_mut().enumerate() {
                if *ri <= 0.0 {
                    let prev = (0..i).rev().find(|&j| m[j] > 0.0).map(|j| m[j]);
                    let next = (i + 1..RADIUS_SAMPLES).find(|&j| m[j] > 0.0).map(|j| m[j]);
                    *ri = match (prev, next) {
                        (Some(a), Some(b)) => 0.5 * (a + b),
                        (Some(a), None) | (None, Some(a)) => a,
                        (None, None) => chain.radius[i],
                    };
                }
            }
            chain.radius = std::array::from_fn(|i| {
                let a = r[i.saturating_sub(1)];
                let b = r[(i + 1).min(RADIUS_SAMPLES - 1)];
                (0.25 * a + 0.5 * r[i] + 0.25 * b).clamp(0.015, 0.4)
            });
        }
    }

    /// Takes the profiles of `extras.acr_limb_radius` (`{"arm_l": [16 radii], ...}` or four
    /// arrays in limb order).
    pub fn read_radii(&mut self, extras: &serde_json::Value) {
        let sample = |v: &serde_json::Value| -> Option<[f32; RADIUS_SAMPLES]> {
            let a = v.as_array()?;
            (a.len() == RADIUS_SAMPLES).then(|| std::array::from_fn(|i| a[i].as_f64().unwrap_or(0.05) as f32))
        };
        for l in Limb::ALL {
            let v = extras.get(l.name()).or_else(|| extras.get(l as usize));
            if let Some(r) = v.and_then(sample) {
                self.limbs[l as usize].radius = r;
            }
        }
    }
}

/// Number of data texels of an instance: the wounds block, plus the damage block when
/// damaged (`model.wgsl`).
pub const WOUND_TEXELS: u32 = 4;
pub const DAMAGE_TEXELS: u32 = 12 + 2 * MAX_VOLUMES as u32;

/// The damage block of the instance data (`model.wgsl`, `damage_*`):
/// - 0: cut levels of the four limbs (0..1, 2 whole);
/// - 1: fracture levels (2: none);
/// - 2..5: per limb the fracture's pivot (rest pose) and angle;
/// - 6..9: per limb the fracture's axis;
/// - 10: trunk cut height (rest y; 1e6 none), head gone, belly open, keep (−1 body, limb
///   0..3 piece, 4 upper);
/// - 11: piece from, piece to (0..1), belly height (rest y), number of volumes;
/// - 12..: two texels per wound volume ([`Volume::gpu`]).
pub fn pack(d: &Damage, rig: Option<&Rig>) -> [[f32; 4]; DAMAGE_TEXELS as usize] {
    let mut t = [[0.0f32; 4]; DAMAGE_TEXELS as usize];
    for (i, l) in d.limbs.iter().enumerate() {
        t[0][i] = level_f(l.cut);
        t[1][i] = 2.0;
        if let (Some(f), Some(rig)) = (l.fracture, rig) {
            let (pivot, _, _) = rig.limbs[i].at(level_f(f.level));
            t[1][i] = level_f(f.level);
            t[2 + i] = [pivot[0], pivot[1], pivot[2], f.angle];
            let a = normalize(f.axis);
            t[6 + i] = [a[0], a[1], a[2], 0.0];
        }
    }
    let trunk_y = |level: u8| rig.map_or(1.0, |r| r.trunk.at(f32::from(level) / 255.0).0[1]);
    t[10] = [
        d.trunk_cut.map_or(1e6, trunk_y),
        f32::from(u8::from(d.head_gone)),
        f32::from(u8::from(d.belly_open)),
        match d.keep {
            Keep::Body => -1.0,
            Keep::Piece { limb, .. } => limb as u8 as f32,
            Keep::Upper => 4.0,
        },
    ];
    let (from, to) = match d.keep {
        Keep::Piece { from, to, .. } => (f32::from(from) / 255.0, level_f(to)),
        _ => (0.0, 2.0),
    };
    t[11] = [from, to, trunk_y(70), d.volumes().len() as f32];
    for (i, v) in d.volumes().iter().enumerate() {
        let [a, b] = v.gpu();
        t[12 + 2 * i] = a;
        t[13 + 2 * i] = b;
    }
    t
}

/// A stump cap in the rest pose: where (point), facing (unit normal, away from the kept
/// flesh), how large (radius) and which palette slot carries it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cap {
    pub point: Vec3,
    pub normal: Vec3,
    pub radius: f32,
    pub slot: u32,
}

impl Cap {
    /// The cap's model matrix in the model's rest space: unit disc in xy (normal +z) to
    /// `point`, `normal`, `radius`.
    pub fn rest_matrix(&self) -> Mat4 {
        let z = normalize(self.normal);
        let up = if z[1].abs() < 0.9 { [0.0, 1.0, 0.0] } else { [1.0, 0.0, 0.0] };
        let x = normalize(cross(up, z));
        let y = cross(z, x);
        let r = self.radius;
        let p = self.point;
        [
            [x[0] * r, x[1] * r, x[2] * r, 0.0],
            [y[0] * r, y[1] * r, y[2] * r, 0.0],
            [z[0] * r, z[1] * r, z[2] * r, 0.0],
            [p[0], p[1], p[2], 1.0],
        ]
    }
}

fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// The caps an instance with `d` shows: at every cut of the kept part, on its side.
pub fn caps(d: &Damage, rig: &Rig) -> Vec<Cap> {
    let mut out = Vec::new();
    let limb_cap = |l: Limb, level: u8, outward: bool| {
        let chain = &rig.limbs[l as usize];
        let lf = f32::from(level) / 255.0;
        let (point, dir, slot) = chain.at(lf);
        Cap { point, normal: if outward { dir } else { math::scale(dir, -1.0) }, radius: chain.radius_at(lf), slot }
    };
    let trunk_cap = |level: u8, up: bool| {
        let lf = f32::from(level) / 255.0;
        let (point, dir, slot) = rig.trunk.at(lf);
        Cap { point, normal: if up { dir } else { math::scale(dir, -1.0) }, radius: rig.trunk.radius_at(lf), slot }
    };
    let upper_attached = |l: Limb| l.is_arm();
    match d.keep {
        Keep::Body => {
            for l in Limb::ALL {
                let cut = d.limb(l).cut;
                if cut < WHOLE && !(d.trunk_cut.is_some() && upper_attached(l)) {
                    out.push(limb_cap(l, cut, true));
                }
            }
            if let Some(t) = d.trunk_cut {
                out.push(trunk_cap(t, true));
            } else if d.head_gone {
                let (p, slot) = rig.neck;
                let dir = rig.trunk.at(1.0).1;
                out.push(Cap { point: p, normal: dir, radius: rig.trunk.radius_at(1.0).min(0.07), slot });
            }
        }
        Keep::Piece { limb, from, to } => {
            out.push(limb_cap(limb, from, false));
            if to < WHOLE {
                out.push(limb_cap(limb, to, true));
            }
        }
        Keep::Upper => {
            if let Some(t) = d.trunk_cut {
                out.push(trunk_cap(t, false));
            }
            for l in Limb::ALL.into_iter().filter(|&l| upper_attached(l)) {
                let cut = d.limb(l).cut;
                if cut < WHOLE {
                    out.push(limb_cap(l, cut, true));
                }
            }
        }
    }
    // A cut inside a wound volume ends in its torn boundary, not a cap.
    out.retain(|c| !d.volumes().iter().any(|v| v.distance(c.point) < v.radius * 0.95));
    out
}

/// The procedural stump cap (until `gore.glb` brings the modelled one): a unit disc in xy
/// facing +z, domed slightly, coloured by rings: skin rim, red flesh, white bone dot.
/// `uv` is the disc position (−1..1) for the ragged outline the shader cuts.
pub fn procedural_cap() -> ModelData {
    const RINGS: [(f32, [f32; 4]); 6] = [
        (0.0, [0.92, 0.88, 0.80, 1.0]),
        (0.16, [0.85, 0.80, 0.70, 1.0]),
        (0.24, [0.32, 0.02, 0.02, 1.0]),
        (0.7, [0.42, 0.04, 0.035, 1.0]),
        (0.86, [0.55, 0.30, 0.24, 1.0]),
        (1.0, [0.60, 0.42, 0.33, 1.0]),
    ];
    const SEGMENTS: usize = 24;
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for (ri, &(r, color)) in RINGS.iter().enumerate() {
        let n = if ri == 0 { 1 } else { SEGMENTS };
        for s in 0..n {
            let a = s as f32 / SEGMENTS as f32 * std::f32::consts::TAU;
            let (x, y) = (a.cos() * r, a.sin() * r);
            let z = 0.12 * (1.0 - r * r);
            vertices.push(Vertex {
                position: [x, y, z],
                normal: normalize([x * 0.3, y * 0.3, 1.0]),
                uv: [x, y],
                color,
                joints: [0; 4],
                weights: [1.0, 0.0, 0.0, 0.0],
                morph: [[0.0; 3]; 2],
                body: [-1.0, -1.0, 0.5, 0.0],
            });
        }
    }
    let ring = |ri: usize, s: usize| if ri == 0 { 0 } else { (1 + (ri - 1) * SEGMENTS + s % SEGMENTS) as u32 };
    for ri in 1..RINGS.len() {
        for s in 0..SEGMENTS {
            if ri == 1 {
                indices.extend([0, ring(1, s), ring(1, s + 1)]);
            } else {
                indices.extend([ring(ri - 1, s), ring(ri, s), ring(ri, s + 1)]);
                indices.extend([ring(ri - 1, s), ring(ri, s + 1), ring(ri - 1, s + 1)]);
            }
        }
    }
    let material = Material {
        name: "stump_cap".into(),
        base_color: [1.0; 4],
        texture: None,
        nearest: false,
        alpha_cutoff: None,
        decay: None,
        stump: true,
    };
    ModelData::single(Primitive { vertices, indices, material: 0, morphs: [false; 2] }, material)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chain() -> Chain {
        Chain {
            points: vec![
                (0.0, [0.0, 1.4, 0.0], 1),
                (level_f(ELBOW), [0.3, 1.4, 0.0], 2),
                (level_f(WRIST), [0.55, 1.4, 0.0], 3),
                (1.0, [0.72, 1.4, 0.0], 3),
            ],
            radius: std::array::from_fn(|i| 0.06 - 0.002 * i as f32),
        }
    }

    #[test]
    fn level_maps_to_a_plane_on_the_chain() {
        let c = chain();
        let (p, d, slot) = c.at(level_f(ELBOW));
        assert!((p[0] - 0.3).abs() < 1e-5 && slot == 1 || slot == 2, "{p:?} {slot}");
        // Halfway between elbow and wrist: on the forearm, facing along it.
        let mid = (f32::from(ELBOW) + f32::from(WRIST)) / 2.0 / 255.0;
        let (p, d2, slot) = c.at(mid);
        assert!((p[0] - 0.425).abs() < 1e-4, "{p:?}");
        assert_eq!(slot, 2);
        assert!((d2[0] - 1.0).abs() < 1e-5 && (d[0] - 1.0).abs() < 1e-5);
        // Projection is the inverse.
        assert!((c.project([0.425, 1.45, 0.02]) - mid).abs() < 1e-4);
        assert_eq!(c.project([-0.2, 1.4, 0.0]), 0.0);
        assert!((c.project([0.9, 1.4, 0.0]) - 1.0).abs() < 1e-6);
        // Radius profile interpolates.
        assert!((c.radius_at(0.0) - 0.06).abs() < 1e-6);
        assert!((c.radius_at(1.0) - 0.03).abs() < 1e-5);
    }

    #[test]
    fn joint_position_inverts_the_inverse_bind() {
        // Joint at (1, 2, 3) rotated 90° about z: IBM = inverse.
        let m = math::from_trs([1.0, 2.0, 3.0], [0.0, 0.0, 0.70710677, 0.70710677], [1.0; 3]);
        // Inverse of a rigid transform: Rᵀ, −Rᵀt.
        let r = |a: usize, b: usize| m[b][a];
        let inv: Mat4 = std::array::from_fn(|c| {
            std::array::from_fn(|a| match (c, a) {
                (3, 3) => 1.0,
                (_, 3) => 0.0,
                (3, a) => -(0..3).map(|k| r(k, a) * m[3][k]).sum::<f32>(),
                (c, a) => r(c, a),
            })
        });
        let p = joint_position(&inv);
        assert!((p[0] - 1.0).abs() < 1e-5 && (p[1] - 2.0).abs() < 1e-5 && (p[2] - 3.0).abs() < 1e-5, "{p:?}");
    }

    #[test]
    fn piece_span_and_parse() {
        let d = Damage::parse("cut:arm_l:150, frac:leg_r:60:40, head, piece:arm_l").unwrap();
        assert_eq!(d.limb(Limb::ArmL).cut, 150);
        assert_eq!(d.keep, Keep::Piece { limb: Limb::ArmL, from: 150, to: WHOLE });
        assert!(d.head_gone && !d.belly_open);
        let f = d.limb(Limb::LegR).fracture.unwrap();
        assert_eq!(f.level, 60);
        assert!((f.angle - 40f32.to_radians()).abs() < 1e-6);
        assert!(Damage::parse("cut:tail:3").is_err());
        let body = Damage { keep: Keep::Body, ..d };
        assert_eq!(body.piece(Limb::ArmL).unwrap().keep, d.keep);
        assert!(body.piece(Limb::ArmR).is_none());
        assert!(Damage::default().is_none() && !body.is_none());
    }

    #[test]
    fn packing() {
        let mut d = Damage::default();
        let t = pack(&d, None);
        assert_eq!(t[0], [2.0; 4]);
        assert_eq!(t[1], [2.0; 4]);
        assert_eq!(t[10][0], 1e6);
        assert_eq!(t[10][3], -1.0);
        d.limb_mut(Limb::LegL).cut = 51;
        d.keep = Keep::Piece { limb: Limb::LegL, from: 51, to: 200 };
        d.belly_open = true;
        let t = pack(&d, None);
        assert!((t[0][2] - 0.2).abs() < 1e-6);
        assert_eq!(t[10][2], 1.0);
        assert_eq!(t[10][3], 2.0);
        assert!((t[11][0] - 0.2).abs() < 1e-6 && (t[11][1] - 200.0 / 255.0).abs() < 1e-6);
        d.keep = Keep::Upper;
        assert_eq!(pack(&d, None)[10][3], 4.0);
    }

    #[test]
    fn caps_face_away_from_the_kept_flesh() {
        let c = chain();
        let trunk = Chain { points: vec![(0.0, [0.0, 1.0, 0.0], 0), (1.0, [0.0, 1.5, 0.0], 0)], radius: [0.15; 16] };
        let rig = Rig { limbs: [c.clone(), c.clone(), c.clone(), c], trunk, neck: ([0.0, 1.5, 0.0], 5) };
        let mut d = Damage::default();
        d.limb_mut(Limb::ArmL).cut = 150;
        let body = caps(&d, &rig);
        assert_eq!(body.len(), 1);
        assert!(body[0].normal[0] > 0.99);
        let piece = caps(&d.piece(Limb::ArmL).unwrap(), &rig);
        assert_eq!(piece.len(), 1);
        assert!(piece[0].normal[0] < -0.99);
        assert_eq!(piece[0].point, body[0].point);
        // Torn in two: the arm hangs on the upper half; the lower half has one trunk cap.
        d.trunk_cut = Some(128);
        let lower = caps(&d, &rig);
        assert_eq!(lower.len(), 1);
        assert!(lower[0].normal[1] > 0.99);
        let upper = caps(&Damage { keep: Keep::Upper, ..d }, &rig);
        assert_eq!(upper.len(), 2);
        let m = upper[0].rest_matrix();
        assert!((m[2][1] + 0.15).abs() < 1e-4, "disc normal scaled by the radius: {m:?}");
    }

    #[test]
    fn volumes_sever_limbs_and_hide_caps() {
        let c = chain();
        let trunk = Chain { points: vec![(0.0, [0.0, 1.0, 0.0], 0), (1.0, [0.0, 1.5, 0.0], 0)], radius: [0.15; 16] };
        let rig = Rig { limbs: [c.clone(), c.clone(), c.clone(), c], trunk, neck: ([0.0, 1.5, 0.0], 5) };
        let mut d = Damage::parse("vol:0.0:1.4:-0.3:0.0:1.4:0.3:0.12").unwrap();
        assert_eq!(d.volumes().len(), 1);
        d.sever(&rig);
        // The ball through the shoulder takes the arm at its root.
        let cut = d.limb(Limb::ArmL).cut;
        assert!(cut < 40, "{cut}");
        assert!(caps(&d, &rig).is_empty(), "the stump ends in the volume");
        assert!(caps(&d.piece(Limb::ArmL).unwrap(), &rig).is_empty());
        let t = pack(&d, None);
        assert_eq!(t[11][3], 1.0);
        assert_eq!(t[12][3], 0.12);
        // A small volume beside the forearm leaves it hanging on.
        let mut thread = Damage::default();
        thread.push_volume(Volume::sphere([0.42, 1.45, 0.0], 0.04, 0.5));
        thread.sever(&rig);
        assert_eq!(thread.limb(Limb::ArmL).cut, WHOLE);
        // Full: the smallest gives way.
        for i in 0..MAX_VOLUMES {
            thread.push_volume(Volume::sphere([0.0; 3], 0.05 + i as f32 * 0.01, 0.1));
        }
        assert_eq!(thread.volumes().len(), MAX_VOLUMES);
        assert!(thread.volumes().iter().all(|v| v.radius > 0.04));
    }

    #[test]
    fn procedural_cap_is_a_closed_fan() {
        let m = procedural_cap();
        let p = &m.primitives[0];
        assert!(p.indices.iter().all(|&i| (i as usize) < p.vertices.len()));
        assert_eq!(p.indices.len() % 3, 0);
        assert!(m.materials[0].stump);
    }
}
