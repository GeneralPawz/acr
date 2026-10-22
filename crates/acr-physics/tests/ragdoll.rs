//! Behaviour of the XPBD ragdoll: falling and settling, holding a pose, reacting to an
//! impulse, and limits that hold.

use acr_physics::math::closest_segment_params;
use acr_physics::{Body, Config, JointLimit, Quat, Ragdoll, Tone, Vec3, World, skeleton_h};

const DEG: f32 = core::f32::consts::PI / 180.0;

/// The default solver settings; `ACR_PHYS_SUBSTEPS` and `ACR_PHYS_PASSES` override them to
/// measure the accuracy of cheaper settings with these same tests.
fn config() -> Config {
    let env = |k: &str| std::env::var(k).ok().and_then(|v| v.parse().ok());
    let d = Config::default();
    Config {
        substeps: env("ACR_PHYS_SUBSTEPS").unwrap_or(d.substeps),
        joint_iterations: env("ACR_PHYS_PASSES").unwrap_or(d.joint_iterations),
        ..d
    }
}

fn flat(_x: f32, _y: f32) -> f32 {
    0.0
}

/// Largest violation (rad, or m for the separation) of any joint limit right now.
fn worst_violation(world: &World) -> (f32, f32) {
    let (mut ang, mut sep) = (0.0f32, 0.0f32);
    for (j, joint) in world.joints().iter().enumerate() {
        let s = world.joint_state(j as u32);
        sep = sep.max(s.separation);
        match joint.limit {
            JointLimit::Hinge { lo, hi } => {
                ang = ang.max(lo - s.angle).max(s.angle - hi).max(s.twist);
            }
            JointLimit::Ball { cone, twist_lo, twist_hi, .. } => {
                ang = ang.max(s.angle - cone).max(twist_lo - s.twist).max(s.twist - twist_hi);
            }
        }
    }
    (ang, sep)
}

fn kinetic(world: &World) -> f32 {
    world.bodies().iter().map(|b| b.kinetic_energy()).sum()
}

#[test]
fn limp_ragdoll_falls_and_settles() {
    let mut world = World::new(config());
    let tilt = Quat::from_axis_angle(Vec3::new(1.0, 0.3, 0.0), 0.4);
    let man = Ragdoll::spawn(&mut world, &skeleton_h(), 75.0, Vec3::new(0.0, 0.0, 1.0), tilt);
    let e0 = world.energy();
    let (mut worst_ang, mut worst_sep, mut max_speed) = (0.0f32, 0.0f32, 0.0f32);
    let mut settled_ang = 0.0f32;
    let mut e_prev_second = e0;
    for step in 0..480 {
        world.step(&flat);
        let (a, s) = worst_violation(&world);
        worst_ang = worst_ang.max(a);
        worst_sep = worst_sep.max(s);
        if step >= 360 {
            settled_ang = settled_ang.max(a);
        }
        for b in world.bodies() {
            assert!(b.pos.is_finite() && b.vel.is_finite(), "exploded at step {step}");
            max_speed = max_speed.max(b.vel.length());
        }
        if step % 60 == 59 && step >= 119 {
            // Energy never grows (small numerical slack).
            let e = world.energy();
            assert!(e <= e_prev_second + 1.0, "energy grew at step {step}: {e_prev_second} -> {e}");
            e_prev_second = e;
        }
    }
    let e1 = world.energy();
    let ke = kinetic(&world);
    let com = man.center_of_mass(&world);
    let min_z = man.min_z(&world);
    println!(
        "limp drop: E {e0:.1} -> {e1:.1} J, KE {ke:.4} J, com z {:.3}, min z {min_z:.3}, worst limit {:.2} deg (settled {:.2}), \
         worst separation {:.4} m, max speed {max_speed:.2}",
        com.z,
        worst_ang / DEG,
        settled_ang / DEG,
        worst_sep
    );
    assert!(max_speed < 12.0, "no explosion: max speed {max_speed}");
    assert!(e1 < e0 - 300.0, "energy decays: {e0} -> {e1}");
    assert!(ke < 0.2, "settled: KE {ke}");
    assert!(com.z < 0.35, "lying on the ground: com z {}", com.z);
    assert!(min_z > -0.03, "not through the ground: {min_z}");
    // Feet-first landing from a metre forces the ankle cone for a frame or two while the
    // body's momentum is turned; at rest the limits hold exactly.
    assert!(worst_ang < 10.0 * DEG, "limits hold during the impact: {} deg", worst_ang / DEG);
    assert!(settled_ang < 1.0 * DEG, "limits hold at rest: {} deg", settled_ang / DEG);
    assert!(worst_sep < 0.02, "joints stay together: {worst_sep}");
}

#[test]
fn full_tone_holds_pose_with_root_fixed() {
    let mut world = World::new(config());
    let man = Ragdoll::spawn(&mut world, &skeleton_h(), 75.0, Vec3::new(0.0, 0.0, 0.3), Quat::IDENTITY);
    world.set_pinned(man.body(0), true);
    man.set_tone_all(&mut world, Tone::FULL);
    // Standing pose with bent elbows and a slightly bent knee.
    for (name, angle) in [("forearm_l", 60.0), ("forearm_r", 60.0), ("shin_l", 20.0)] {
        let b = man.bone_index(name).unwrap();
        let axis = man.joint_axis(&world, b);
        man.set_target(&mut world, b, Quat::from_axis_angle(axis, angle * DEG));
    }
    for _ in 0..180 {
        world.step(&flat);
    }
    let mut worst = 0.0f32;
    for b in 1..man.len() {
        let s = world.joint_state(man.joint(b).unwrap());
        worst = worst.max(s.drive_error);
        assert!(s.drive_error < 5.0 * DEG, "{} off its target by {} deg", man.bone_name(b), s.drive_error / DEG);
    }
    let elbow = world.joint_state(man.joint(man.bone_index("forearm_l").unwrap()).unwrap());
    println!("pose held: worst drive error {:.2} deg, elbow {:.1} deg", worst / DEG, elbow.angle / DEG);
    assert!((elbow.angle - 60.0 * DEG).abs() < 3.0 * DEG);
    assert!(kinetic(&world) < 0.05);
}

#[test]
fn impulse_at_right_shoulder_spins_and_throws_back() {
    let mut world = World::new(config());
    let man = Ragdoll::spawn(&mut world, &skeleton_h(), 75.0, Vec3::new(0.0, 0.0, 0.005), Quat::IDENTITY);
    man.set_tone_all(&mut world, Tone::FULL);
    for _ in 0..6 {
        world.step(&flat);
    }
    // Back of the right shoulder pushed backwards (+Y): a cannonball-class hit.
    let chest = man.body(man.bone_index("spine2").unwrap());
    let shoulder = Vec3::new(-0.17, 0.0, 1.43);
    world.apply_impulse(chest, shoulder, Vec3::new(0.0, 60.0, 0.0));
    let w = world.body(chest).omega;
    println!("omega after the hit: {w:?}");
    assert!(w.z < 0.0, "spins clockwise seen from above (right shoulder back)");
    assert!(w.x < 0.0, "tips backwards");
    man.set_tone_all(&mut world, Tone::FULL.scaled(0.1));
    for _ in 0..240 {
        world.step(&flat);
    }
    let head = world.body(man.body(man.bone_index("head").unwrap())).pos;
    let com = man.center_of_mass(&world);
    println!("after the fall: head {head:?}, com {com:?}");
    assert!(head.y > 0.6, "fell backwards: head {head:?}");
    assert!(head.z < 0.5, "is down: head {head:?}");
}

#[test]
fn hinge_limits_hold_under_large_impulses() {
    let mut world = World::new(Config { gravity: Vec3::ZERO, ..config() });
    let upper = world.add_body(Body::capsule(Vec3::new(0.0, 0.0, 1.0), Quat::IDENTITY, 0.04, 0.15, 2.0));
    let lower = world.add_body(Body::capsule(Vec3::new(0.0, 0.3, 1.0), Quat::IDENTITY, 0.04, 0.15, 1.2));
    world.set_pinned(upper, true);
    let (lo, hi) = (0.0, 120.0 * DEG);
    // Twist axis +Y (along the forearm), hinge axis +Z: flexion turns +Y towards -X.
    let frame = Quat::from_basis(Vec3::Y, -Vec3::X, Vec3::Z);
    let j = world.add_joint(upper, lower, Vec3::new(0.0, 0.15, 1.0), frame, JointLimit::Hinge { lo, hi });
    let (mut worst, mut worst_sep) = (0.0f32, 0.0f32);
    for (k, dir) in [-1.0f32, 1.0, -1.0, 1.0].iter().enumerate() {
        let p = world.body(lower).world_point(Vec3::new(0.0, 0.15, 0.0));
        world.apply_impulse(lower, p, Vec3::new(*dir * 8.0, 0.0, 3.0 * (k as f32 - 1.5)));
        for _ in 0..60 {
            world.step(&flat);
            let s = world.joint_state(j);
            worst = worst.max(lo - s.angle).max(s.angle - hi).max(s.twist);
            worst_sep = worst_sep.max(s.separation);
        }
    }
    println!("hinge: worst violation {:.2} deg, worst separation {worst_sep:.4} m", worst / DEG);
    assert!(worst_sep < 0.01, "separation {worst_sep}");
    // Spins of up to ~100 rad/s against the stops: a few degrees of overshoot at the peak.
    assert!(worst < 6.0 * DEG, "hinge limit violated by {} deg", worst / DEG);
}

#[test]
fn capsules_stack_instead_of_passing_through() {
    let mut world = World::new(config());
    // Capsule axes are local +Y: one lies along X, the other across it along Y.
    let along_x = Quat::from_axis_angle(Vec3::Z, core::f32::consts::FRAC_PI_2);
    let low = world.add_body(Body::capsule(Vec3::new(0.0, 0.0, 0.1), along_x, 0.1, 0.4, 20.0));
    let high = world.add_body(Body::capsule(Vec3::new(0.0, 0.0, 0.8), Quat::IDENTITY, 0.1, 0.4, 20.0));
    for _ in 0..180 {
        world.step(&flat);
    }
    let (a, b) = (world.body(low), world.body(high));
    let ((a0, a1), (b0, b1)) = (a.ends(), b.ends());
    let (s, t) = closest_segment_params(a0, a1 - a0, b0, b1 - b0);
    let dist = ((a0 + (a1 - a0) * s) - (b0 + (b1 - b0) * t)).length();
    println!("stack: low {:?}, high {:?}, distance {dist:.4}", a.pos, b.pos);
    assert!((a.pos.z - 0.1).abs() < 0.01, "the lower capsule lies on the ground: {:?}", a.pos);
    assert!(b.pos.z > 0.15, "the upper capsule is held up by the lower one: {:?}", b.pos);
    assert!(dist > 0.19, "no interpenetration: segment distance {dist}");
}

#[test]
fn deterministic_within_a_build() {
    let run = || {
        let mut world = World::new(config());
        let tilt = Quat::from_axis_angle(Vec3::Y, 0.3);
        let man = Ragdoll::spawn(&mut world, &skeleton_h(), 80.0, Vec3::new(0.0, 0.0, 0.5), tilt);
        for _ in 0..120 {
            world.step(&|x: f32, y: f32| 0.1 * (x * 2.0).sin() + 0.05 * y);
        }
        (0..man.len()).map(|b| man.skin_matrix(&world, b)).collect::<Vec<_>>()
    };
    assert_eq!(run(), run());
}

#[test]
fn resting_ragdoll_sleeps_and_wakes_on_impulse() {
    let mut world = World::new(config());
    let tilt = Quat::from_axis_angle(Vec3::X, 1.2);
    let man = Ragdoll::spawn(&mut world, &skeleton_h(), 75.0, Vec3::new(0.0, 0.0, 0.4), tilt);
    let mut slept_at = None;
    for step in 0..600 {
        world.step(&flat);
        if slept_at.is_none() && man.is_asleep(&world) {
            slept_at = Some(step);
        }
    }
    let slept_at = slept_at.expect("falls asleep once at rest");
    println!("asleep after {:.2} s", slept_at as f32 / 60.0);
    assert_eq!(world.awake_bodies(), 0);
    let mut before = [[[0.0; 4]; 4]; 22];
    man.pose(&world, &mut before);
    for _ in 0..30 {
        world.step(&flat);
    }
    let mut after = [[[0.0; 4]; 4]; 22];
    man.pose(&world, &mut after);
    assert_eq!(before, after, "a sleeping body does not move");
    let chest = man.body(man.bone_index("spine2").unwrap());
    let p = world.body(chest).pos;
    world.apply_impulse(chest, p, Vec3::new(0.0, 0.0, 30.0));
    assert!(!man.is_asleep(&world), "an impulse wakes it");
    world.step(&flat);
    assert!(world.body(chest).pos.z > p.z, "and it moves");
}

#[test]
fn frozen_corpse_blocks_and_wakes_when_hit() {
    let mut world = World::new(config());
    let lying = Quat::from_axis_angle(Vec3::X, 1.5);
    let corpse = Ragdoll::spawn(&mut world, &skeleton_h(), 75.0, Vec3::new(0.0, 0.0, 0.2), lying);
    corpse.freeze(&mut world);
    assert!(corpse.is_asleep(&world));
    // Across the corpse, hips over its belly.
    let across = Quat::from_axis_angle(Vec3::Z, core::f32::consts::FRAC_PI_2) * lying;
    let falling = Ragdoll::spawn(&mut world, &skeleton_h(), 75.0, Vec3::new(-1.0, -0.9, 0.6), across);
    let mut woke = false;
    for _ in 0..90 {
        world.step(&flat);
        woke |= !corpse.is_asleep(&world);
    }
    assert!(woke, "the corpse wakes when the falling body touches it");
    let hips = world.body(falling.body(0)).pos;
    let under = world.body(corpse.body(corpse.bone_index("spine").unwrap())).pos;
    println!("fallen onto the corpse: hips {hips:?}, corpse's belly {under:?}");
    // Draped over him: the hips rest on his trunk (a lying man's hips are at about 0.12 m).
    assert!(hips.z > under.z + 0.1, "lies on the corpse, not through it: {hips:?} over {under:?}");
}

#[test]
fn placed_by_skin_matrices_reproduces_the_pose() {
    let mut world = World::new(config());
    let turn = Quat::from_axis_angle(Vec3::Z, 0.7);
    let src = Ragdoll::spawn(&mut world, &skeleton_h(), 75.0, Vec3::new(1.0, 2.0, 0.0), turn);
    let dst = Ragdoll::spawn(&mut world, &skeleton_h(), 75.0, Vec3::new(9.0, 0.0, 0.0), Quat::IDENTITY);
    for b in 0..src.len() {
        let m = src.skin_matrix(&world, b);
        dst.place(&mut world, b, &m, Vec3::ZERO);
    }
    for b in 0..src.len() {
        let (a, c) = (src.skin_matrix(&world, b), dst.skin_matrix(&world, b));
        for (x, y) in a.iter().flatten().zip(c.iter().flatten()) {
            assert!((x - y).abs() < 1e-4, "bone {b}: {a:?} vs {c:?}");
        }
    }
    dst.target_current_pose(&mut world);
    assert!(world.joint_state(dst.joint(5).unwrap()).drive_error < 1e-3);
}
