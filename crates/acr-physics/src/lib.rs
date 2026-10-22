//! XPBD active ragdolls for the body simulation (#276, phase 1 #278).
//!
//! Render side only: physics never feeds the lockstep sim, so it is `f32` and deterministic
//! only within a build (fixed step, no allocation in [`World::step`]). Runs the same on
//! native and wasm32; no dependencies.
//!
//! - [`World`]: rigid capsule [`Body`]s, [`Joint`]s (hinge with angle limits, ball with
//!   cone and twist limits), contact with a [`Ground`] height field (any
//!   `Fn(f32, f32) -> f32` is one) with friction, and capsule–capsule contacts. Solver:
//!   Müller et al. 2020, "Detailed Rigid Body Simulation with Extended Position Based
//!   Dynamics", one iteration per substep.
//! - Muscles: every joint drives towards a target rotation with a PD drive whose stiffness
//!   and damping ([`Tone`]) are set per joint or per [`Region`].
//! - [`Ragdoll`]: built from a skeleton description ([`BoneDesc`], [`skeleton_h`]): one
//!   capsule per bone, Dempster mass fractions, anatomical limits per [`JointSpec`]; reads
//!   back bone transforms and skinning matrices.
//!
//! Axes are those of skeleton H (Blender): metres, `+X` left, `-Y` forward, `+Z` up.
//!
//! ```
//! use acr_physics::{Config, Quat, Ragdoll, Tone, Vec3, World, skeleton_h};
//! let mut world = World::new(Config::default());
//! let man = Ragdoll::spawn(&mut world, &skeleton_h(), 75.0, Vec3::ZERO, Quat::IDENTITY);
//! man.set_tone_all(&mut world, Tone::FULL);
//! let ground = |_x: f32, _y: f32| 0.0;
//! world.step(&ground);
//! let skin = man.skin_matrix(&world, man.bone_index("head").unwrap());
//! assert!(skin[3][2].abs() < 0.1);
//! ```

pub mod math;
pub mod ragdoll;
pub mod world;

pub use math::{Quat, Vec3};
pub use ragdoll::{BoneDesc, JointSpec, Ragdoll, Region, skeleton_h};
pub use world::{Body, BodyId, Config, Ground, Joint, JointId, JointLimit, JointState, Tone, World};
