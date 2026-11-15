//! The clip audit (#509, `docs/testing.md` "The clip audit"): every clip of a built human
//! model measured offline, without the game or a GPU. Three checks per clip:
//!
//! - **reach** (strike clips): at the hit frame ([`HIT_FRAME`]) the distance of the weapon's
//!   point to the target the clip is for, from the clip alone and after the aiming layer
//!   (`strike_view::probe`);
//! - **body**: the shown weapon in the right hand, as a segment along its axis, against the
//!   holder's own trunk, head and legs (capsules on the rig's joints, radii of
//!   `skeleton_h`), every frame: the deepest penetration;
//! - **grip** (`grip_right`, `grip_left`): for a musket or a pole weapon, each hand's distance
//!   to the weapon's segment, every frame: the worst.
//!
//! Test code only (the runner is the test `clip_audit_run`, driven by `ACR_AUDIT_MODELS` and
//! `ACR_AUDIT_OUT`); nothing here runs in the game.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use acr_physics::gait::{Rig, Skin};
use acr_physics::math::closest_segment_params;
use acr_physics::{Vec3, skeleton_h};
use acr_render::model::{ModelData, Slot};
use acr_sim::combat::{StrikeHeight, StrikeKind};

use super::ragdoll_view::{Mapping, animated_skins, node_frame};
use super::strike_view::Weapon;
use super::strike_view::{
    BODY_RADIUS, HIDDEN, HIT_FRAME, NEAR_BODY, REACH, TURN_LANCE, TURN_SABRE, TURN_SABRE_L, clip_kind, is_finish, pick,
    probe, shown, target_point, weapons,
};

/// Reach fails when the point is further than this from the target with the layer (m): the
/// layer's own test bound (`the_layer_takes_up_a_residual`).
pub const REACH_MAX: f32 = 0.15;
/// The weapon's axis deeper than this inside a body capsule fails (m).
pub const BODY_MAX: f32 = 0.03;
/// A weapon goes through a body part when at least this much of it lies on either side (m).
pub const BEYOND: f32 = 0.15;
/// A hand further than this from the weapon is off it (m).
pub const GRIP_MAX: f32 = 0.12;
/// A weapon at least this long (m) is a pole weapon: two hands on foot.
pub const POLE: f32 = 1.5;
/// The sim's melee distance to a standing man (m, `docs/melee.md`: musketeers fence at
/// 1.67–1.73 m) and to a man on the ground (`clips.TARGET_DOWN`; riders 1.9, 1.7 with a sabre).
const STANDING: f32 = 1.67;
const DOWN: f32 = 2.0;
const DOWN_LANCE: f32 = 1.9;
const DOWN_SABRE: f32 = 1.7;
/// `finish.TARGET_FINISH`: right, forward, up of the attacker's ground point (m).
const FINISH: [f32; 3] = [0.05, 0.90, 0.20];

/// One measured row of the audit.
#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    pub clip: String,
    /// `reach`, `body`, `grip_right`, `grip_left`.
    pub check: &'static str,
    /// The worst frame (sprite frames).
    pub frame: usize,
    /// Metres; `None` when the check could not be measured (the note says why).
    pub value: Option<f32>,
    pub threshold: f32,
    /// `pass`, `fail`, `info` (measured, not judged) or `n/a`.
    pub result: &'static str,
    pub note: String,
}

/// The distance between the segments `a0..a1` and `b0..b1`.
pub fn segment_distance(a0: Vec3, a1: Vec3, b0: Vec3, b1: Vec3) -> f32 {
    let (s, t) = closest_segment_params(a0, a1 - a0, b0, b1 - b0);
    ((a0 + (a1 - a0) * s) - (b0 + (b1 - b0) * t)).length()
}

/// A body capsule: its name, ends and radius.
pub type Capsule = (&'static str, Vec3, Vec3, f32);

/// How deep the segment `a..b` lies inside the capsules (m, negative: clear of all by that
/// much) and the capsule it is deepest in.
pub fn penetration(a: Vec3, b: Vec3, capsules: &[Capsule]) -> (f32, &'static str) {
    capsules
        .iter()
        .map(|&(name, p, q, r)| (r - segment_distance(a, b, p, q), name))
        .fold((f32::NEG_INFINITY, ""), |best, c| if c.0 > best.0 { c } else { best })
}

/// How a weapon meets the body in one frame: the part it lies deepest or longest in, how
/// deep its axis gets (m, negative: clear by that much), the length of it inside the part
/// (its longest run) and the lengths of it before and beyond that run.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Crossing {
    pub part: &'static str,
    pub depth: f32,
    pub inside: f32,
    pub before: f32,
    pub after: f32,
    pub frame: usize,
    pub hand: &'static str,
}

impl Crossing {
    /// The weapon goes through the part: deeper than [`BODY_MAX`] and at least [`BEYOND`]
    /// of it on either side. Anything shallower or at one of its ends is a touch.
    pub fn through(&self) -> bool {
        self.depth > BODY_MAX && self.before >= BEYOND && self.after >= BEYOND
    }

    /// Worst first: through by the length inside, then by depth.
    fn rank(&self) -> (bool, f32) {
        (self.through(), if self.through() { self.inside } else { self.depth })
    }
}

/// The worst [`Crossing`] of the segment `a..b` with the body parts of `capsules` (capsules
/// of one name are one part). The weapon is sampled every 2 cm.
pub fn crossing(a: Vec3, b: Vec3, capsules: &[Capsule]) -> Crossing {
    const STEP: f32 = 0.02;
    let len = (b - a).length();
    let n = (len / STEP).ceil().max(1.0) as usize;
    let mut best: Option<Crossing> = None;
    let mut names: Vec<&'static str> = capsules.iter().map(|c| c.0).collect();
    names.dedup();
    for part in names {
        let of: Vec<&Capsule> = capsules.iter().filter(|c| c.0 == part).collect();
        let depth = of.iter().map(|&&(_, p, q, r)| r - segment_distance(a, b, p, q)).fold(f32::NEG_INFINITY, f32::max);
        let mut c = Crossing { part, depth, inside: 0.0, before: 0.0, after: 0.0, frame: 0, hand: "" };
        if depth > 0.0 {
            // The longest run of samples inside the part.
            let inside = |i: usize| {
                let x = a + (b - a) * (i as f32 / n as f32);
                of.iter().any(|&&(_, p, q, r)| segment_distance(x, x, p, q) < r)
            };
            let (mut run, mut longest): (Option<usize>, Option<(usize, usize)>) = (None, None);
            for i in 0..=n + 1 {
                match (i <= n && inside(i), run) {
                    (true, None) => run = Some(i),
                    (false, Some(s)) => {
                        if longest.is_none_or(|(s0, e0)| i - 1 - s > e0 - s0) {
                            longest = Some((s, i - 1));
                        }
                        run = None;
                    }
                    _ => {}
                }
            }
            if let Some((s, e)) = longest {
                let at = |i: usize| len * i as f32 / n as f32;
                (c.inside, c.before, c.after) = ((at(e) - at(s)).max(STEP), at(s), len - at(e));
            }
        }
        if best.is_none_or(|w| c.rank() > w.rank()) {
            best = Some(c);
        }
    }
    best.unwrap_or(Crossing {
        part: "",
        depth: f32::NEG_INFINITY,
        inside: 0.0,
        before: 0.0,
        after: 0.0,
        frame: 0,
        hand: "",
    })
}

/// The weapons in the left hand: the items under `socket.hand_l` with a point
/// (`socket.tip`: a pikeman's `pike_l`), not bows (not struck with) nor ramrods.
fn left_weapons(data: &ModelData) -> Vec<Weapon> {
    let nodes = &data.nodes;
    let under = |mut n: usize, of: usize| loop {
        match nodes[n].parent {
            Some(p) if p == of => return true,
            Some(p) => n = p,
            None => return false,
        }
    };
    let Some(socket) = nodes.iter().position(|n| n.name.ends_with("socket.hand_l")) else { return Vec::new() };
    (0..nodes.len())
        .filter(|&i| nodes[i].name.ends_with("socket.tip") && under(i, socket) && !nodes[i].name.contains("bow"))
        .map(|tip| Weapon { socket, tip: Some(tip), muzzle: false, point: true })
        .collect()
}

/// The holder's body in a pose: the trunk from the hips to the chest, the head and the legs
/// (not the arms: the weapon's own arm and hand hold it).
pub fn body(rig: &Rig, skins: &[Skin]) -> Vec<Capsule> {
    const PARTS: [(&str, &str); 9] = [
        ("hips", "trunk"),
        ("spine", "trunk"),
        ("spine1", "trunk"),
        ("spine2", "trunk"),
        ("head", "head"),
        ("thigh_l", "left upper leg"),
        ("thigh_r", "right upper leg"),
        ("shin_l", "left lower leg"),
        ("shin_r", "right lower leg"),
    ];
    let bones = skeleton_h();
    PARTS
        .iter()
        .map(|&(bone, part)| {
            let b = rig.bone(bone);
            (part, rig.joint(skins, b), rig.end(skins, b), bones[b].radius)
        })
        .collect()
}

/// What a strike clip is aimed at.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Aim {
    /// The target (physics, the attacker at the origin facing -y), its height and its name.
    At(Vec3, StrikeHeight, &'static str),
    /// Not a reach check, and why.
    Not(&'static str),
}

/// The target of the clip `name` for a man on foot or `mounted` whose weapon's point lies
/// `reach` m from the hand, `gun` when it is a musket (`socket.muzzle`) or holds an arrow:
/// the targets of `docs/melee.md` (`strike_view::target_point`, `clips.TARGET_DOWN`,
/// `finish.TARGET_FINISH`). `None`: not a strike clip.
pub fn aim_of(name: &str, mounted: bool, reach: f32, gun: bool) -> Option<Aim> {
    let rest = name.strip_prefix("ATTACK")?;
    let suffix = rest.trim_start_matches(|c: char| c.is_ascii_digit());
    let tokens: Vec<&str> = suffix.split('_').filter(|t| !t.is_empty()).collect();
    if !suffix.is_empty() && !suffix.starts_with('_') {
        return None;
    }
    let at = |right: f32, fwd: f32, up: f32| Vec3::new(-right, -fwd, up);
    match clip_kind(name) {
        Some(StrikeKind::Shot) => return Some(Aim::Not("a shot")),
        Some(StrikeKind::Smash) if !is_finish(name) => {
            return Some(Aim::Not("a butt or club stroke: the point is not the striking end"));
        }
        _ => {}
    }
    if tokens.is_empty() {
        if gun && !rest.is_empty() {
            return Some(Aim::Not("a shot (ATTACK<n> of a gun or a bow)"));
        }
        if gun && reach < HIDDEN {
            return Some(Aim::Not("a shot"));
        }
    }
    let me = Vec3::ZERO;
    let standing =
        |h: StrikeHeight, what: &'static str| Aim::At(target_point(at(0.0, STANDING, 0.0), me, h, false, 0.0), h, what);
    Some(if tokens.contains(&"FINISH") {
        Aim::At(at(FINISH[0], FINISH[1], FINISH[2]), StrikeHeight::Down, "finishing target (finish.TARGET_FINISH)")
    } else if tokens.contains(&"DOWN") {
        if mounted {
            // The horse turns the man to the rider's side (`strike_yaw`).
            let (dist, turn, near, what) = if reach >= POLE {
                (DOWN_LANCE, TURN_LANCE, 0.0, "man on the ground 1.9 m off, 25 deg right (lance)")
            } else if tokens.last() == Some(&"L") {
                (DOWN_SABRE, TURN_SABRE_L, NEAR_BODY, "near part of a man on the ground 1.7 m off, 40 deg left")
            } else {
                (DOWN_SABRE, TURN_SABRE, NEAR_BODY, "near part of a man on the ground 1.7 m off, 75 deg right")
            };
            let ground = at(turn.sin() * dist, turn.cos() * dist, 0.0);
            Aim::At(target_point(ground, me, StrikeHeight::Down, false, near), StrikeHeight::Down, what)
        } else {
            Aim::At(
                target_point(at(0.0, DOWN, 0.0), me, StrikeHeight::Down, false, 0.0),
                StrikeHeight::Down,
                "man on the ground 2.0 m ahead (clips.TARGET_DOWN)",
            )
        }
    } else if tokens.contains(&"HIGH") {
        standing(StrikeHeight::High, "standing man 1.67 m ahead, high (1.65 m)")
    } else if tokens.contains(&"LOW") {
        standing(StrikeHeight::Low, "standing man 1.67 m ahead, low (0.7 m)")
    } else {
        standing(StrikeHeight::Mid, "standing man 1.67 m ahead, mid (1.15 m)")
    })
}

/// Clips in which a musket or a pole weapon is held in both hands throughout by design: the
/// melee strike clips and the ready stances. Not the shots (`gun`: `ATTACK<n>`, `FIRE_*`,
/// `_SHOT_`: a hand reloads), nor the changes into and out of the stance (`PATTACK`,
/// `UATTACK`: they start or end in the carry).
pub fn both_hands(name: &str, gun: bool) -> bool {
    if name.starts_with("PSTAND") {
        return true;
    }
    // A plain `ATTACK<n>`: a shot, a throw or work, by the `.MD`.
    if name.strip_prefix("ATTACK").is_some_and(|r| !r.is_empty() && !r.contains('_')) {
        return false;
    }
    matches!(aim_of(name, false, REACH, gun), Some(Aim::At(..)))
        || (name.starts_with("ATTACK_SMASH") && !name.contains("_SHOT"))
}

/// Unskinned mesh vertices (a sample of at most ~1500) under node `socket` that show in
/// `clip`, as (node, local position).
fn weapon_vertices(data: &ModelData, socket: usize, clip: &str) -> Vec<(usize, Vec3)> {
    let under = |mut n: usize| loop {
        if n == socket {
            return true;
        }
        match data.nodes[n].parent {
            Some(p) => n = p,
            None => return false,
        }
    };
    let mut out = Vec::new();
    for p in &data.primitives {
        let Some(v) = p.vertices.first() else { continue };
        let Some(&Slot::Node(n)) = data.slots.get(v.joints[0] as usize) else { continue };
        if !under(n) || data.node_states(n).is_some_and(|s| !s.iter().any(|c| c == clip)) {
            continue;
        }
        let step = (p.vertices.len() / 500).max(1);
        out.extend(p.vertices.iter().step_by(step).map(|v| (n, Vec3::from_array(v.position))));
    }
    out
}

/// Audits every clip of `data`. `Err`: why the model is skipped.
pub fn audit(data: &ModelData) -> Result<Vec<Finding>, String> {
    if data.clips.is_empty() {
        return Err("no clips".into());
    }
    let rider = Mapping::with_prefix(data, "rider.");
    let mounted = rider.is_some();
    let Some(map) = rider.or_else(|| Mapping::with_prefix(data, "")).filter(Mapping::human) else {
        return Err("no skeleton-H rig".into());
    };
    let found = weapons(&data.nodes);
    let left = left_weapons(data);
    if found.is_empty() && left.is_empty() {
        return Err("no weapon socket in a hand".into());
    }
    let rig = Rig::new(&skeleton_h());
    let (hand_r, hand_l) = (rig.bone("hand_r"), rig.bone("hand_l"));
    let transform = acr_render::view3d::model_matrix([0.0; 3], 0.0, 1.0);
    let mut out = Vec::new();
    for (ci, clip) in data.clips.iter().enumerate() {
        let frames = (clip.duration * data.fps).round() as usize + 1;
        // The worst frame of the body check and of each hand's grip.
        let mut worst: Option<Crossing> = None;
        let mut through_frames = 0;
        let mut grips: [Option<(f32, usize)>; 2] = [None, None];
        // The mesh's extent along the weapon's axis from the hand, per socket.
        let mut extents: BTreeMap<usize, Option<(f32, f32)>> = BTreeMap::new();
        let (mut held_frames, mut two_handed, mut measured_mesh) = (0, false, false);
        for f in 0..frames {
            let time = f as f32 / data.fps;
            let globals = data.globals(Some(ci), time);
            let right = pick(&found, &data.nodes, &globals).filter(|w| shown(w, &data.nodes, &globals));
            let held: Vec<(Weapon, &'static str)> = right
                .map(|w| (w, "right"))
                .into_iter()
                .chain(left.iter().filter(|w| shown(w, &data.nodes, &globals)).map(|w| (*w, "left")))
                .collect();
            let mut posed: Option<(Vec<Skin>, Vec<Capsule>)> = None;
            let mut any = false;
            let mut crossed = false;
            for (wpn, side) in held {
                let (hand_at, socket_axis) = node_frame(&globals, wpn.socket, Vec3::Y, &transform);
                let tip = wpn.tip.map(|t| node_frame(&globals, t, Vec3::Y, &transform));
                let (dir, ahead) = match tip {
                    Some((p, _)) if wpn.point && (p - hand_at).length() < HIDDEN => continue,
                    Some((p, _)) if wpn.point => ((p - hand_at).normalize_or_zero(), (p - hand_at).length()),
                    Some((_, axis)) => (axis.normalize_or_zero(), REACH),
                    None => (socket_axis.normalize_or_zero(), REACH),
                };
                if dir == Vec3::ZERO {
                    continue;
                }
                any = true;
                let extent = *extents.entry(wpn.socket).or_insert_with(|| {
                    let verts = weapon_vertices(data, wpn.socket, &clip.name);
                    let mut span: Option<(f32, f32)> = None;
                    for (n, v) in verts {
                        let (o, x) = node_frame(&globals, n, Vec3::X, &transform);
                        let (_, y) = node_frame(&globals, n, Vec3::Y, &transform);
                        let (_, z) = node_frame(&globals, n, Vec3::Z, &transform);
                        let t = (o + x * v.x + y * v.y + z * v.z - hand_at).dot(dir);
                        span = Some(span.map_or((t, t), |(lo, hi)| (lo.min(t), hi.max(t))));
                    }
                    span
                });
                measured_mesh |= extent.is_some();
                let (lo, hi) = extent.map_or((0.0, ahead), |(lo, hi)| (lo.min(0.0), hi.max(ahead)));
                let (a, b) = (hand_at + dir * lo, hand_at + dir * hi);
                let (skins, capsules) = posed.get_or_insert_with(|| {
                    let skins = animated_skins(&map, &data.pose(Some(ci), time), &transform);
                    let capsules = body(&rig, &skins);
                    (skins, capsules)
                });
                let mut c = crossing(a, b, capsules);
                (c.frame, c.hand) = (f, side);
                crossed |= c.through();
                if worst.is_none_or(|w| c.rank() > w.rank()) {
                    worst = Some(c);
                }
                if wpn.muzzle || (!mounted && hi - lo >= POLE) {
                    two_handed = true;
                    for (k, hand) in [hand_r, hand_l].into_iter().enumerate() {
                        let palm = (rig.joint(skins, hand) + rig.end(skins, hand)) * 0.5;
                        let off = segment_distance(palm, palm, a, b);
                        if grips[k].is_none_or(|g| off > g.0) {
                            grips[k] = Some((off, f));
                        }
                    }
                }
            }
            held_frames += usize::from(any);
            through_frames += usize::from(crossed);
        }
        let extent_note =
            if measured_mesh { "weapon mesh extent" } else { "hand to point only (no mesh under the socket)" };
        // Reach.
        let globals = data.globals(Some(ci), HIT_FRAME / data.fps);
        let at_hit = pick(&found, &data.nodes, &globals);
        let reach = at_hit.and_then(|w| {
            let hand = node_frame(&globals, w.socket, Vec3::Y, &transform).0;
            w.tip.filter(|_| w.point).map(|t| (node_frame(&globals, t, Vec3::Y, &transform).0 - hand).length())
        });
        let gun = at_hit.is_some_and(|w| w.muzzle || w.tip.is_some_and(|t| data.nodes[t].name.contains("arrow")));
        match aim_of(&clip.name, mounted, reach.unwrap_or(REACH), gun) {
            None | Some(Aim::Not(_)) => {}
            Some(Aim::At(target, height, what)) => {
                let finding = |value, result, note: String| Finding {
                    clip: clip.name.clone(),
                    check: "reach",
                    frame: HIT_FRAME as usize,
                    value,
                    threshold: REACH_MAX,
                    result,
                    note,
                };
                // Targets the audit can only assume: off the target is reported, not failed.
                let plain = clip.name.strip_prefix("ATTACK").is_some_and(|r| !r.is_empty() && !r.contains('_'));
                let unsure = if plain {
                    Some("ATTACK<n>: melee or ranged (or work) is the .MD's choice, not known offline")
                } else if mounted && height != StrikeHeight::Down {
                    Some("a rider's standing target is assumed straight ahead")
                } else {
                    None
                };
                let measured = reach.filter(|r| *r >= HIDDEN).and_then(|_| probe(data, ci, HIT_FRAME, target, height));
                out.push(match measured {
                    Some((alone, layered, _)) => {
                        let (a, l) = ((alone - target).length(), (layered - target).length());
                        // Short of the target (+) or past it (-), along the line from the man.
                        let flat = |v: Vec3| Vec3::new(v.x, v.y, 0.0).length();
                        let short = flat(target) - flat(alone);
                        finding(
                            Some(l),
                            match (l > REACH_MAX, unsure) {
                                (false, _) => "pass",
                                (true, None) => "fail",
                                (true, Some(_)) => "info",
                            },
                            format!(
                                "clip alone {a:.2} m ({} {:.2} m); {what}; point {:.2} m from the hand{}",
                                if short >= 0.0 { "short by" } else { "past by" },
                                short.abs(),
                                reach.unwrap_or(0.0),
                                unsure.map_or(String::new(), |u| format!("; not judged: {u}")),
                            ),
                        )
                    }
                    None => finding(None, "n/a", format!("no shown weapon point (socket.tip / socket.muzzle); {what}")),
                });
            }
        }
        // Body and grip.
        let row = |check, frame, value, threshold, result, note: String| Finding {
            clip: clip.name.clone(),
            check,
            frame,
            value,
            threshold,
            result,
            note,
        };
        match worst {
            Some(c) => {
                let shown = format!("{extent_note}; weapon shown in {held_frames} of {frames} frames");
                let hand = c.hand;
                let (value, result, note) = if c.through() {
                    (
                        c.inside,
                        "fail",
                        format!(
                            "through the {}: {:.2} m of the {hand}-hand weapon inside, {:.2} m deep, {:.2} m and {:.2} m of it beyond; in {through_frames} frames; {shown}",
                            c.part, c.inside, c.depth, c.before, c.after
                        ),
                    )
                } else if c.depth > 0.0 {
                    (
                        c.depth,
                        "info",
                        format!(
                            "touch: the {hand}-hand weapon dips {:.2} m into the {} ({:.2} m of it inside, {:.2} m and {:.2} m beyond); {shown}",
                            c.depth, c.part, c.inside, c.before, c.after
                        ),
                    )
                } else {
                    (c.depth, "pass", format!("clear of the {} by {:.2} m; {shown}", c.part, -c.depth))
                };
                out.push(row("body", c.frame, Some(value), BODY_MAX, result, note));
            }
            None => out.push(row("body", 0, None, BODY_MAX, "n/a", "no weapon shown in a hand".into())),
        }
        if two_handed {
            for (k, check) in ["grip_right", "grip_left"].into_iter().enumerate() {
                if let Some((off, f)) = grips[k] {
                    let judged = both_hands(&clip.name, gun);
                    let result = match (judged, off > GRIP_MAX) {
                        (false, _) => "info",
                        (true, true) => "fail",
                        (true, false) => "pass",
                    };
                    let hand = if k == 0 { "right" } else { "left" };
                    let note = match (judged, off > GRIP_MAX) {
                        (_, false) => format!("{hand} hand on the weapon"),
                        (true, true) => format!("{hand} hand off the weapon"),
                        (false, true) => {
                            format!("{hand} hand off the weapon (not a strike or ready clip: may be by design)")
                        }
                    };
                    out.push(row(check, f, Some(off), GRIP_MAX, result, note));
                }
            }
        }
    }
    out.extend(gear(data, &transform));
    Ok(out)
}

/// A gear part moved more than this against the node it hangs on (m) is loose.
pub const GEAR_MAX: f32 = 0.01;

/// Loose gear: every part that is its own node with an unskinned mesh (a held weapon, a
/// hat, a tool) against the node it hangs on, per clip: the most its distance to that node
/// differs from the rest pose (it should be rigid: 0), and whether it is hidden (scaled
/// away) while that node is shown. The note has how far the part moves against its body
/// bone (a socket animated on the hand: by design in a reload or a carry).
pub fn gear(data: &ModelData, transform: &acr_render::math::Mat4) -> Vec<Finding> {
    let nodes = &data.nodes;
    let mut parts: Vec<usize> = data
        .primitives
        .iter()
        .filter_map(|p| match data.slots.get(p.vertices.first()?.joints[0] as usize)? {
            Slot::Node(n) => Some(*n),
            Slot::Joint { .. } => None,
        })
        .collect();
    parts.sort_unstable();
    parts.dedup();
    let at = |g: &[acr_render::math::Mat4], n: usize| node_frame(g, n, Vec3::Y, transform);
    // The nearest ancestor that is a bone (not a socket, not another part).
    let bone = |n: usize| {
        let mut b = nodes[n].parent;
        while let Some(i) = b {
            if !nodes[i].name.contains("socket") && !parts.contains(&i) {
                break;
            }
            b = nodes[i].parent;
        }
        b
    };
    let rest = data.globals(None, 0.0);
    let mut out = Vec::new();
    for (ci, clip) in data.clips.iter().enumerate() {
        let frames = (clip.duration * data.fps).round() as usize + 1;
        // Per part: (worst change against the owner, its frame, worst against the bone,
        // frames hidden while the owner shows, frames the owner is hidden).
        let mut found: Vec<(f32, usize, f32, usize, usize)> = vec![(0.0, 0, 0.0, 0, 0); parts.len()];
        for f in 0..frames {
            let globals = data.globals(Some(ci), f as f32 / data.fps);
            for (k, &n) in parts.iter().enumerate() {
                let Some(owner) = nodes[n].parent else { continue };
                let scale = |g: &[acr_render::math::Mat4], i: usize| at(g, i).1.length();
                let (s0, s) = (scale(&rest, owner), scale(&globals, owner));
                if s0 < 1e-6 || s / s0 < 0.5 {
                    found[k].4 += 1;
                    continue;
                }
                if scale(&globals, n) / s < 0.5 * scale(&rest, n) / s0 {
                    found[k].3 += 1;
                    continue;
                }
                let gap = |g: &[acr_render::math::Mat4], of: usize| (at(g, n).0 - at(g, of).0).length();
                let moved = (gap(&globals, owner) * s0 / s - gap(&rest, owner)).abs();
                if moved > found[k].0 {
                    (found[k].0, found[k].1) = (moved, f);
                }
                if let Some(b) = bone(n) {
                    found[k].2 = found[k].2.max((gap(&globals, b) - gap(&rest, b)).abs());
                }
            }
        }
        for (k, &n) in parts.iter().enumerate() {
            let Some(owner) = nodes[n].parent else { continue };
            if data.node_states(n).is_some_and(|s| !s.contains(&clip.name)) {
                continue;
            }
            let (moved, frame, on_bone, hidden, owner_hidden) = found[k];
            let mut note = format!("{} on {}", nodes[n].name, nodes[owner].name);
            if on_bone > GEAR_MAX {
                let b = bone(n).map_or("", |b| nodes[b].name.as_str());
                let _ = write!(note, "; moves {on_bone:.2} m against the bone {b} (the socket is animated)");
            }
            if hidden > 0 {
                let _ = write!(note, "; hidden in {hidden} of {frames} frames while {} is shown", nodes[owner].name);
            }
            if owner_hidden > 0 {
                let _ = write!(note, "; {} hidden in {owner_hidden} of {frames} frames", nodes[owner].name);
            }
            out.push(Finding {
                clip: clip.name.clone(),
                check: "gear",
                frame,
                value: Some(moved),
                threshold: GEAR_MAX,
                result: if moved > GEAR_MAX {
                    "fail"
                } else if hidden > 0 && hidden < frames {
                    "info"
                } else {
                    "pass"
                },
                note,
            });
        }
    }
    out
}

/// The rows of `audit.csv`.
pub fn csv(rows: &[(String, Finding)]) -> String {
    let mut s = String::from("model,clip,check,worst_frame,value_m,threshold_m,result,note\n");
    for (model, f) in rows {
        let value = f.value.map_or(String::new(), |v| format!("{v:.3}"));
        let _ = writeln!(
            s,
            "{model},{},{},{},{value},{:.2},{},\"{}\"",
            f.clip,
            f.check,
            f.frame,
            f.threshold,
            f.result,
            f.note.replace('"', "'")
        );
    }
    s
}

/// `summary.md`: the thresholds, the counts, the 40 worst rows per check, the failing clips
/// per model and the skipped models.
pub fn summary(rows: &[(String, Finding)], skipped: &[(String, String)], source: &str, verified: &str) -> String {
    let mut s = String::new();
    let models: std::collections::BTreeSet<&str> = rows.iter().map(|(m, _)| m.as_str()).collect();
    let clips: std::collections::BTreeSet<(&str, &str)> =
        rows.iter().map(|(m, f)| (m.as_str(), f.clip.as_str())).collect();
    let _ = writeln!(s, "# Clip audit (#509)\n");
    if !verified.is_empty() {
        let _ = writeln!(s, "{}\n", verified.trim_end());
    }
    let _ = writeln!(
        s,
        "Models: `{source}`. Measured offline by `clip_audit_run` (docs/testing.md, \"The clip audit\").\n"
    );
    let _ = writeln!(
        s,
        "{} models audited, {} clips, {} rows; {} models skipped.\n",
        models.len(),
        clips.len(),
        rows.len(),
        skipped.len()
    );
    let _ = writeln!(s, "## Checks and thresholds\n");
    let _ = writeln!(s, "| Check | Value | Fails above |\n|---|---|---|");
    let _ = writeln!(
        s,
        "| `reach` | weapon point to the clip's target at frame {HIT_FRAME:.0}, with the aiming layer (the note has the clip alone) | {REACH_MAX} m |"
    );
    let _ = writeln!(
        s,
        "| `body` | the held weapon's axis against the holder's trunk, head and legs over all frames. `fail` = **through**: deeper than {BODY_MAX} m and at least {BEYOND} m of weapon on either side; the value is the length of weapon inside the part. `info` = **touch**: it dips in near one end or shallowly; the value is the depth. `pass`: clear (negative: by that much) | through |\n| `gear` | per part that is its own node (held weapon, hat, tool): the most its distance to the node it hangs on differs from the rest pose; `info`: hidden in some frames while that node is shown | {GEAR_MAX} m |"
    );
    let _ = writeln!(
        s,
        "| `grip_right`, `grip_left` | furthest the hand (middle of the hand bone) is from the weapon over all frames; muskets and pole weapons (>= {POLE} m, on foot) only; judged in the melee strike clips and PSTAND*, `info` elsewhere (carry, shots with their reload, PATTACK/UATTACK) | {GRIP_MAX} m |\n"
    );
    let _ = writeln!(
        s,
        "Limits: the weapons checked are the right hand's (`socket.hand_r`, `socket.blade_r`, `socket.club_r`) and pointed ones in the left (`socket.hand_l` with a `socket.tip`; no bows, no ramrods); reach is measured for the right hand only; `gear` sees only parts that are their own node (skinned gear such as scabbards and a bayonet modelled into the musket's mesh cannot come loose offline); the weapon is a line (no thickness) and the body is capsules of the ragdoll's radii (trunk {BODY_RADIUS:.2} m is the strike target's, the capsules use 0.12–0.13 m), so cloth and gear are not counted; reach targets are the doc's standard ones, straight ahead (a rider's standing target too: an assumption, the doc gives none); a plain `ATTACK<n>` of a weapon without a muzzle or an arrow is measured as melee but not judged (`info`), and so is a rider's blow at a standing man; `body` and `grip` measure the clip alone, without the aiming layer.\n"
    );
    let _ = writeln!(s, "## Counts\n\n| Check | Rows | Fail | Pass | Info | n/a |\n|---|---|---|---|---|---|");
    let checks = ["reach", "body", "grip_right", "grip_left", "gear"];
    for c in checks {
        let n = |r: &str| rows.iter().filter(|(_, f)| f.check == c && (r.is_empty() || f.result == r)).count();
        let _ = writeln!(s, "| `{c}` | {} | {} | {} | {} | {} |", n(""), n("fail"), n("pass"), n("info"), n("n/a"));
    }
    for (title, group) in
        [("reach", &checks[..1]), ("body (through)", &checks[1..2]), ("grip", &checks[2..4]), ("gear", &checks[4..])]
    {
        let mut worst: Vec<&(String, Finding)> =
            rows.iter().filter(|(_, f)| group.contains(&f.check) && f.value.is_some() && f.result != "info").collect();
        worst.sort_by(|a, b| b.1.value.partial_cmp(&a.1.value).unwrap_or(std::cmp::Ordering::Equal));
        let _ = writeln!(
            s,
            "\n## Worst 40: {title}\n\n| Model | Clip | Check | Frame | Value (m) | Result | Note |\n|---|---|---|---|---|---|---|"
        );
        for (m, f) in worst.into_iter().take(40) {
            let _ = writeln!(
                s,
                "| {m} | `{}` | {} | {} | {:.3} | {} | {} |",
                f.clip,
                f.check,
                f.frame,
                f.value.unwrap_or(0.0),
                f.result,
                f.note
            );
        }
    }
    let _ = writeln!(
        s,
        "\n## Failing clips per model\n\n| Model | reach | body | grip | gear | Clips with a failure |\n|---|---|---|---|---|---|"
    );
    let mut per: Vec<(&str, [usize; 4], usize)> = models
        .iter()
        .map(|&m| {
            let fails: Vec<&Finding> =
                rows.iter().filter(|(n, f)| n == m && f.result == "fail").map(|(_, f)| f).collect();
            let n = |p: &str| fails.iter().filter(|f| f.check.starts_with(p)).count();
            let clips: std::collections::BTreeSet<&str> = fails.iter().map(|f| f.clip.as_str()).collect();
            (m, [n("reach"), n("body"), n("grip"), n("gear")], clips.len())
        })
        .collect();
    per.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(b.0)));
    for (m, n, clips) in per {
        let _ = writeln!(s, "| {m} | {} | {} | {} | {} | {clips} |", n[0], n[1], n[2], n[3]);
    }
    let _ = writeln!(s, "\n## Skipped models\n");
    let mut why: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (m, w) in skipped {
        why.entry(w.as_str()).or_default().push(m.as_str());
    }
    for (w, list) in why {
        let _ = writeln!(s, "- **{w}** ({}): {}", list.len(), list.join(", "));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_weapon_through_a_capsule_is_measured() {
        let trunk: Capsule = ("trunk", Vec3::new(0.0, 0.0, 1.0), Vec3::new(0.0, 0.0, 1.4), 0.12);
        // A pole through the middle of the trunk: as deep as the radius.
        let (deep, part) = penetration(Vec3::new(-1.0, 0.0, 1.2), Vec3::new(1.0, 0.0, 1.2), &[trunk]);
        assert!((deep - 0.12).abs() < 1e-5 && part == "trunk", "{deep} {part}");
        // Passing 0.2 m in front of it: clear by 0.08.
        let (clear, _) = penetration(Vec3::new(-1.0, -0.2, 1.2), Vec3::new(1.0, -0.2, 1.2), &[trunk]);
        assert!((clear + 0.08).abs() < 1e-5, "{clear}");
        // Ending 0.5 m short of it.
        let (short, _) = penetration(Vec3::new(-1.0, 0.0, 1.2), Vec3::new(-0.5, 0.0, 1.2), &[trunk]);
        assert!((short + 0.38).abs() < 1e-5, "{short}");
        // Through: 0.24 m of it inside, 0.88 m on either side. Ending inside it: a touch.
        let c = crossing(Vec3::new(-1.0, 0.0, 1.2), Vec3::new(1.0, 0.0, 1.2), &[trunk]);
        assert!(c.through() && (c.inside - 0.24).abs() < 0.05 && (c.before - 0.88).abs() < 0.05, "{c:?}");
        let butt = crossing(Vec3::new(-0.05, 0.0, 1.2), Vec3::new(1.0, 0.0, 1.2), &[trunk]);
        assert!(!butt.through() && butt.depth > 0.05, "{butt:?}");
        // Grazing 0.02 m deep along the side: a touch.
        let graze = crossing(Vec3::new(-1.0, -0.10, 1.2), Vec3::new(1.0, -0.10, 1.2), &[trunk]);
        assert!(!graze.through() && graze.depth > 0.0, "{graze:?}");
        // A hand 0.3 m beyond the weapon's end is off it.
        let p = Vec3::new(1.3, 0.0, 0.0);
        assert!((segment_distance(p, p, Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0)) - 0.3).abs() < 1e-5);
    }

    #[test]
    fn clips_name_their_targets() {
        let at = |a: Option<Aim>| match a {
            Some(Aim::At(p, h, _)) => (-p.x, -p.y, p.z, h),
            other => panic!("{other:?}"),
        };
        let near = |a: (f32, f32, f32, StrikeHeight), b: (f32, f32, f32)| {
            (a.0 - b.0).abs() < 0.01 && (a.1 - b.1).abs() < 0.01 && (a.2 - b.2).abs() < 0.01
        };
        // The doc's table: body surface at 1.67 m, 0/1.50/1.15.
        assert!(near(at(aim_of("ATTACK_THRUST_MID", false, 1.4, true)), (0.0, 1.50, 1.15)));
        assert!(near(at(aim_of("ATTACK", false, 1.4, true)), (0.0, 1.50, 1.15)));
        assert!(near(at(aim_of("ATTACK_DOWN", false, 2.0, false)), (0.0, 2.0, 0.2)));
        assert!(near(at(aim_of("ATTACK_CUT_FINISH", false, 0.9, false)), (0.05, 0.9, 0.2)));
        // The doc's lance: a man at 1.9 m, 25 deg right: 0.81/1.73/0.20.
        assert!(near(at(aim_of("ATTACK_THRUST_DOWN", true, 2.5, false)), (0.80, 1.72, 0.2)));
        let left = at(aim_of("ATTACK_CUT_DOWN_L", true, 0.9, false));
        assert!(left.0 < -0.5 && left.3 == StrikeHeight::Down, "{left:?}");
        assert!(matches!(aim_of("ATTACK1", false, 1.4, true), Some(Aim::Not(_))));
        assert!(matches!(aim_of("ATTACK_SHOT_DOWN", false, 1.4, true), Some(Aim::Not(_))));
        assert!(matches!(aim_of("ATTACK_SMASH_MID", false, 1.4, true), Some(Aim::Not(_))));
        assert!(matches!(aim_of("ATTACK_SMASH_FINISH", false, 0.5, false), Some(Aim::At(..))));
        assert_eq!(aim_of("FIRE_AIMED", false, 1.4, true), None);
        assert_eq!(aim_of("PATTACK", false, 1.4, true), None);
        assert!(both_hands("ATTACK_THRUST_MID", true) && both_hands("PSTAND1", true) && both_hands("ATTACK", true));
        assert!(!both_hands("ATTACK1", true) && !both_hands("FIRE_HIP", true) && !both_hands("UATTACK", false));
        assert!(!both_hands("ATTACK1", false));
        assert!(!both_hands("STAND", false) && !both_hands("ATTACK_SHOT_DOWN", true));
    }

    /// The audit over a folder of built models (`ACR_AUDIT_MODELS`), written to
    /// `ACR_AUDIT_OUT` (`audit.csv`, `summary.md`); skipped without the variables.
    /// `ACR_AUDIT_ONLY` (comma separated model names) narrows it.
    #[test]
    fn clip_audit_run() {
        let (Some(dir), Some(out)) = (std::env::var_os("ACR_AUDIT_MODELS"), std::env::var_os("ACR_AUDIT_OUT")) else {
            return;
        };
        let (dir, out) = (std::path::PathBuf::from(dir), std::path::PathBuf::from(out));
        let only = std::env::var("ACR_AUDIT_ONLY").unwrap_or_default();
        let only: Vec<&str> = only.split(',').filter(|s| !s.is_empty()).collect();
        let mut files: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| Some(e.ok()?.path()))
            .filter(|p| p.extension().is_some_and(|e| e == "glb"))
            .collect();
        files.sort();
        let (mut rows, mut skipped) = (Vec::new(), Vec::new());
        for path in files {
            let name = path.file_stem().unwrap().to_string_lossy().to_string();
            if !only.is_empty() && !only.contains(&name.as_str()) {
                continue;
            }
            let found = std::fs::read(&path)
                .map_err(|e| e.to_string())
                .and_then(|bytes| ModelData::from_glb(&bytes).map_err(|e| format!("does not load: {e}")))
                .and_then(|data| audit(&data));
            match found {
                Ok(list) => rows.extend(list.into_iter().map(|f| (name.clone(), f))),
                Err(why) => skipped.push((name, why)),
            }
        }
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(out.join("audit.csv"), csv(&rows)).unwrap();
        std::fs::write(
            out.join("summary.md"),
            summary(
                &rows,
                &skipped,
                &dir.to_string_lossy(),
                &std::fs::read_to_string(out.join("verified.md")).unwrap_or_default(),
            ),
        )
        .unwrap();
        println!("AUDIT {} rows, {} skipped, written to {}", rows.len(), skipped.len(), out.display());
    }
}
