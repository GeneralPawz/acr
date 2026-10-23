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
fn frozen_corpse_blocks_and_stays_static() {
    let mut world = World::new(config());
    let lying = Quat::from_axis_angle(Vec3::X, 1.5);
    let corpse = Ragdoll::spawn(&mut world, &skeleton_h(), 75.0, Vec3::new(0.0, 0.0, 0.2), lying);
    corpse.freeze(&mut world);
    assert!(corpse.is_asleep(&world));
    // Across the corpse, hips over its belly.
    let across = Quat::from_axis_angle(Vec3::Z, core::f32::consts::FRAC_PI_2) * lying;
    let falling = Ragdoll::spawn(&mut world, &skeleton_h(), 75.0, Vec3::new(-1.0, -0.9, 0.6), across);
    let before = world.body(corpse.body(0)).pos;
    for _ in 0..90 {
        world.step(&flat);
        assert!(corpse.is_asleep(&world), "a frozen corpse is static geometry: touching it does not wake it");
    }
    assert_eq!(world.body(corpse.body(0)).pos, before, "and it does not move");
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

/// A bear-sized quadruped rig as an animal glTF gives it (names as in the WildMesh bear).
fn quadruped() -> Vec<acr_physics::RigBone> {
    use acr_physics::RigBone;
    let mut v: Vec<RigBone> = Vec::new();
    let mut add = |name: &'static str, parent: Option<usize>, head: [f32; 3], tail: [f32; 3], radius: f32| {
        v.push(RigBone { name, parent, head, tail, radius });
        v.len() - 1
    };
    let pelvis = add("RigPelvis", None, [0.0, 0.45, 0.85], [0.0, 0.25, 0.9], 0.28);
    let spine = add("RigSpine1", Some(pelvis), [0.0, 0.25, 0.9], [0.0, 0.0, 0.92], 0.3);
    let chest = add("RigChest", Some(spine), [0.0, 0.0, 0.92], [0.0, -0.35, 0.9], 0.32);
    let neck = add("RigNeck1", Some(chest), [0.0, -0.35, 0.9], [0.0, -0.6, 0.95], 0.16);
    add("RigHead", Some(neck), [0.0, -0.6, 0.95], [0.0, -0.9, 0.9], 0.13);
    add("RigTail1", Some(pelvis), [0.0, 0.45, 0.85], [0.0, 0.55, 0.75], 0.05);
    for (side, x) in [("L", 0.18f32), ("R", -0.18)] {
        let names: [&'static str; 7] = if side == "L" {
            ["RigLFLegCollarbone", "RigLFLeg1", "RigLFLeg2", "RigLFLegAnkle", "RigLBLeg1", "RigLBLeg2", "RigLBLegAnkle"]
        } else {
            ["RigRFLegCollarbone", "RigRFLeg1", "RigRFLeg2", "RigRFLegAnkle", "RigRBLeg1", "RigRBLeg2", "RigRBLegAnkle"]
        };
        let c = add(names[0], Some(chest), [x * 0.5, -0.2, 0.9], [x, -0.25, 0.75], 0.1);
        let f1 = add(names[1], Some(c), [x, -0.25, 0.75], [x, -0.22, 0.42], 0.09);
        let f2 = add(names[2], Some(f1), [x, -0.22, 0.42], [x, -0.25, 0.1], 0.07);
        add(names[3], Some(f2), [x, -0.25, 0.1], [x, -0.37, 0.03], 0.05);
        let b1 = add(names[4], Some(pelvis), [x, 0.4, 0.8], [x, 0.3, 0.45], 0.11);
        let b2 = add(names[5], Some(b1), [x, 0.3, 0.45], [x, 0.4, 0.1], 0.07);
        add(names[6], Some(b2), [x, 0.4, 0.1], [x, 0.28, 0.03], 0.05);
    }
    v
}

#[test]
fn generic_rig_kinds_regions_and_masses() {
    use acr_physics::{BoneKind, Region, bone_kinds, skeleton_generic};
    let rig = quadruped();
    let kinds = bone_kinds(&rig);
    let kind = |n: &str| kinds[rig.iter().position(|b| b.name == n).unwrap()];
    assert_eq!(kind("RigPelvis"), BoneKind::Root);
    assert_eq!(kind("RigSpine1"), BoneKind::Spine);
    assert_eq!(kind("RigChest"), BoneKind::Spine);
    assert_eq!(kind("RigNeck1"), BoneKind::Neck);
    assert_eq!(kind("RigHead"), BoneKind::Head);
    assert_eq!(kind("RigTail1"), BoneKind::Tail);
    assert_eq!(kind("RigLFLegCollarbone"), BoneKind::Girdle);
    assert_eq!(kind("RigLFLeg1"), BoneKind::Upper);
    assert_eq!(kind("RigLFLeg2"), BoneKind::Middle);
    assert_eq!(kind("RigLFLegAnkle"), BoneKind::Foot);
    assert_eq!(kind("RigRBLeg1"), BoneKind::Upper);
    assert_eq!(kind("RigRBLeg2"), BoneKind::Middle);
    let (desc, mass) = skeleton_generic(&rig);
    let region = |n: &str| desc.iter().find(|b| b.name == n).unwrap().region;
    assert_eq!(region("RigLFLeg2"), Region::ArmL);
    assert_eq!(region("RigRFLeg1"), Region::ArmR);
    assert_eq!(region("RigLBLeg2"), Region::LegL);
    assert_eq!(region("RigRBLegAnkle"), Region::LegR);
    assert_eq!(region("RigHead"), Region::Head);
    // A bear of a few hundred kilograms, most of it in the trunk.
    assert!((150.0..600.0).contains(&mass), "{mass}");
    let sum: f32 = desc.iter().map(|b| b.mass_fraction).sum();
    assert!((sum - 1.0).abs() < 1e-4);
    let trunk: f32 = desc.iter().filter(|b| b.region == Region::Torso).map(|b| b.mass_fraction).sum();
    assert!(trunk > 0.5, "{trunk}");
}

#[test]
fn generic_quadruped_falls_on_its_side_and_settles() {
    let (desc, mass) = acr_physics::skeleton_generic(&quadruped());
    let mut world = World::new(config());
    let animal = Ragdoll::spawn(&mut world, &desc, mass, Vec3::new(0.0, 0.0, 0.05), Quat::IDENTITY);
    // Dead: limp, pushed from the side as by a shot.
    let chest = animal.bone_index("RigChest").unwrap();
    let c = world.body(animal.body(chest)).pos;
    world.apply_impulse(animal.body(chest), c + Vec3::new(0.3, 0.0, 0.2), Vec3::new(-0.6 * mass, 0.0, 0.0));
    let mut worst = 0.0f32;
    for _ in 0..600 {
        world.step(&flat);
        worst = worst.max(worst_violation(&world).1);
    }
    let (ang, _) = worst_violation(&world);
    assert!(ang < 6.0 * DEG, "limits at rest {:.1}°", ang / DEG);
    assert!(worst < 0.05, "separation {worst}");
    // Down on the ground: the trunk lies low and nothing is under the ground.
    assert!(world.body(animal.body(chest)).pos.z < 0.5, "{:?}", world.body(animal.body(chest)).pos);
    assert!(animal.min_z(&world) > -0.03, "{}", animal.min_z(&world));
    assert!(kinetic(&world) / mass < 0.01, "{}", kinetic(&world));
}

/// A melee blow (12 N·s, `ragdoll_view`) on any bone of a dying man standing: he falls in
/// place or backwards, his feet move at most about a step, and no body is whipped faster
/// than a falling man's (no launch), also when a hand or the head is struck.
#[test]
fn melee_blow_knocks_down_but_never_launches() {
    let skeleton = skeleton_h();
    // First no blow at all: where a man collapsing in place puts his feet.
    let mut collapse = 0.0;
    for (bone, j) in [(0, 0.0)].into_iter().chain((0..skeleton.len()).map(|b| (b, 12.0))) {
        let mut world = World::new(config());
        let man = Ragdoll::spawn(&mut world, &skeleton, 75.0, Vec3::new(0.0, 0.0, 0.005), Quat::IDENTITY);
        man.set_tone_all(&mut world, Tone::FULL);
        for _ in 0..6 {
            world.step(&flat);
        }
        let feet = |w: &World| {
            let p = |n: &str| w.body(man.body(man.bone_index(n).unwrap())).pos;
            (p("foot_l") + p("foot_r")) * 0.5
        };
        let f0 = feet(&world);
        let id = man.body(bone);
        let dir = Vec3::new(0.0, 1.0, 0.0);
        let point = world.body(id).pos - dir * (world.body(id).radius * 0.8);
        man.apply_blow(&mut world, bone, point, dir * j, 2.0);
        let mut top = 0.0f32;
        for s in 0..180 {
            // The tone runs out over 0.4 s, as for the dying.
            let k = (1.0 - s as f32 / 24.0).max(0.0);
            man.set_tone_all(&mut world, Tone::FULL.scaled(0.6 * k));
            world.step(&flat);
            for b in 0..man.len() {
                top = top.max(world.body(man.body(b)).vel.length());
            }
        }
        let f1 = feet(&world);
        let moved = Vec3::new(f1.x - f0.x, f1.y - f0.y, 0.0).length();
        let name = skeleton[bone].name;
        println!("{name} {j}: feet moved {moved:.2} m, top {top:.1} m/s");
        if j == 0.0 {
            collapse = moved;
        }
        assert!(moved < collapse + 0.6, "{name}: feet moved {moved:.2} m ({collapse:.2} m without the blow)");
        assert!(top < 6.0, "{name}: a body reached {top:.1} m/s");
    }
}

/// `Ragdoll::set_velocity` takes the spikes out of a ragdoll that hung on a pinned root.
#[test]
fn set_velocity_calms_every_body() {
    let mut world = World::new(config());
    let man = Ragdoll::spawn(&mut world, &skeleton_h(), 75.0, Vec3::new(0.0, 0.0, 0.005), Quat::IDENTITY);
    man.set_tone_all(&mut world, Tone::FULL);
    man.set_pinned_root(&mut world, true);
    // The root jumps 5 cm a frame (a running man's clip).
    for i in 0..30 {
        let b = world.body(man.body(0));
        let (p, q) = (b.pos + Vec3::new(0.05, 0.0, 0.0), b.rot);
        world.set_body_state(man.body(0), p, q, Vec3::ZERO, Vec3::ZERO);
        world.step(&flat);
        let _ = i;
    }
    man.set_pinned_root(&mut world, false);
    let v = Vec3::new(3.0, 0.0, 0.0);
    man.set_velocity(&mut world, v);
    for b in 0..man.len() {
        assert!((world.body(man.body(b)).vel - v).length() < 1e-4);
    }
}
