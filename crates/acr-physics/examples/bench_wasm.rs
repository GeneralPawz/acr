//! The bench for wasm32, as a cdylib timed from JavaScript (no clock in
//! wasm32-unknown-unknown):
//!
//! ```text
//! cargo build -p acr-physics --release --example bench_wasm --target wasm32-unknown-unknown
//! node crates/acr-physics/examples/bench_wasm.mjs <target dir>/wasm32-unknown-unknown/release/examples/bench_wasm.wasm
//! ```
//!
//! `setup(n, half_frozen)` builds the scene of `bench.rs`, `run(steps)` steps it and
//! returns the number of active ragdolls.

use std::cell::RefCell;

use acr_physics::{Config, Quat, Ragdoll, Vec3, World, skeleton_h};

thread_local! {
    static SCENE: RefCell<Option<(World, u32)>> = const { RefCell::new(None) };
}

fn ground(x: f32, y: f32) -> f32 {
    0.05 * (x * 1.3).sin() + 0.04 * (y * 0.9).cos()
}

#[unsafe(no_mangle)]
pub extern "C" fn setup(n: u32, half_frozen: u32) {
    let skeleton = skeleton_h();
    let mut world = World::new(Config { sleep_energy: 0.0, ..Config::default() });
    let side = (n as f32).sqrt().ceil() as u32;
    let mut active = 0;
    for i in 0..n {
        let (x, y) = ((i % side) as f32 * 2.5, (i / side) as f32 * 2.5);
        let frozen = half_frozen != 0 && i % 2 == 1;
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
    SCENE.with(|s| *s.borrow_mut() = Some((world, active)));
}

#[unsafe(no_mangle)]
pub extern "C" fn run(steps: u32) -> u32 {
    SCENE.with(|s| {
        let mut s = s.borrow_mut();
        let (world, active) = s.as_mut().expect("setup first");
        for _ in 0..steps {
            world.step(&ground);
        }
        *active
    })
}
