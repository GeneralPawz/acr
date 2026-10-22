//! Step cost of ragdolls (native): `cargo run -p acr-physics --release --example bench`.
//!
//! Measures one fixed step (1/60 s, default substeps and joint iterations) of N limp
//! skeleton-H ragdolls falling onto uneven ground and lying there, side by side (no contact
//! between them), and reports microseconds per ragdoll step. This feeds the test-driven
//! budget of docs/body-sim.md: bodies per frame = frame budget / cost per ragdoll.

use std::time::Instant;

use acr_physics::{Config, Quat, Ragdoll, Tone, Vec3, World, skeleton_h};

fn main() {
    let skeleton = skeleton_h();
    let flat = std::env::args().any(|a| a == "--flat");
    let ground = move |x: f32, y: f32| if flat { 0.0 } else { 0.05 * (x * 1.3).sin() + 0.04 * (y * 0.9).cos() };
    for &(n, tone) in &[(1usize, false), (16, false), (16, true), (64, false)] {
        let mut world = World::new(Config::default());
        let men: Vec<Ragdoll> = (0..n)
            .map(|i| {
                let pos = Vec3::new((i % 8) as f32 * 2.5, (i / 8) as f32 * 2.5, 0.6);
                let tilt = Quat::from_axis_angle(Vec3::new(1.0, 0.2 * i as f32, 0.0), 0.5);
                Ragdoll::spawn(&mut world, &skeleton, 75.0, pos, tilt)
            })
            .collect();
        if tone {
            for m in &men {
                m.set_tone_all(&mut world, Tone::FULL.scaled(0.3));
            }
        }
        let steps = (6000 / n).max(120);
        // Falling (first second) and lying (the rest) both count.
        let t = Instant::now();
        for _ in 0..steps {
            world.step(&ground);
        }
        let us = t.elapsed().as_secs_f64() * 1e6 / (steps * n) as f64;
        println!(
            "{n:3} ragdolls{}: {us:7.1} us per ragdoll step ({steps} steps, {} substeps x {} joint passes)",
            if tone { " (tone 0.3)" } else { "" },
            world.config.substeps,
            world.config.joint_iterations
        );
    }
}
