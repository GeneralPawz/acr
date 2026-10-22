//! Step cost of ragdolls: `cargo run -p acr-physics --release --example bench [-- SUBSTEPS
//! JOINT_PASSES]`.
//!
//! Measures fixed steps (1/60 s) of N skeleton-H ragdolls side by side on uneven ground:
//! the active ones are dropped limp (falling, then lying, for 4 s, sleeping disabled so they
//! stay active); with "half frozen" every second one is a frozen corpse. Reports
//! microseconds per active ragdoll step. This feeds the budget of docs/body-sim.md: active
//! ragdolls per frame = floor(frame budget / cost per active ragdoll step).

use std::time::Instant;

#[cfg(windows)]
mod cycles {
    unsafe extern "system" {
        fn GetCurrentThread() -> isize;
        fn QueryThreadCycleTime(thread: isize, cycles: *mut u64) -> i32;
    }
    /// CPU cycles spent by this thread: unlike wall time, not inflated by other processes.
    pub fn now() -> u64 {
        let mut c = 0u64;
        // SAFETY: the pseudo handle of the current thread is always valid; `c` is writable.
        unsafe { QueryThreadCycleTime(GetCurrentThread(), &mut c) };
        c
    }
}
#[cfg(not(windows))]
mod cycles {
    pub fn now() -> u64 {
        0
    }
}

use acr_physics::{Config, Quat, Ragdoll, Vec3, World, skeleton_h};

fn main() {
    let args: Vec<u32> = std::env::args().skip(1).filter_map(|a| a.parse().ok()).collect();
    let mut config = Config { sleep_energy: 0.0, ..Config::default() };
    if let Some(&s) = args.first() {
        config.substeps = s;
    }
    if let Some(&j) = args.get(1) {
        config.joint_iterations = j;
    }
    println!("{} substeps x {} joint passes", config.substeps, config.joint_iterations);
    for &(n, frozen) in &[(1usize, false), (16, false), (16, true), (64, true), (256, true)] {
        // Best of three: the dev machine is shared, the minimum is the least disturbed run.
        let (us, kc, active) = (0..3)
            .map(|_| run(config, n, frozen))
            .fold((f64::MAX, f64::MAX, 0), |b, r| (b.0.min(r.0), b.1.min(r.1), r.2));
        println!(
            "{n:4} ragdolls, {active:3} active{}: {us:6.1} us, {kc:6.1} kcycles per active ragdoll step",
            if frozen { " (half frozen)" } else { "" }
        );
    }
}

/// Microseconds and thousands of thread cycles per active ragdoll step, and the number of
/// active ragdolls.
fn run(config: Config, n: usize, half_frozen: bool) -> (f64, f64, usize) {
    let skeleton = skeleton_h();
    let ground = |x: f32, y: f32| 0.05 * (x * 1.3).sin() + 0.04 * (y * 0.9).cos();
    let mut world = World::new(config);
    let side = (n as f32).sqrt().ceil() as usize;
    let mut active = 0;
    for i in 0..n {
        let (x, y) = ((i % side) as f32 * 2.5, (i / side) as f32 * 2.5);
        let frozen = half_frozen && i % 2 == 1;
        let (pos, rot) = if frozen {
            (Vec3::new(x, y, 0.25), Quat::from_axis_angle(Vec3::X, 1.5))
        } else {
            (Vec3::new(x, y, 0.6), Quat::from_axis_angle(Vec3::new(1.0, 0.2 * i as f32, 0.0), 0.5))
        };
        let man = Ragdoll::spawn(&mut world, &skeleton, 75.0, pos, rot);
        if frozen {
            man.freeze(&mut world);
        } else {
            active += 1;
        }
    }
    let steps = 240;
    let (t, c) = (Instant::now(), cycles::now());
    for _ in 0..steps {
        world.step(&ground);
    }
    let per = (steps * active) as f64;
    (t.elapsed().as_secs_f64() * 1e6 / per, (cycles::now() - c) as f64 / 1e3 / per, active)
}
