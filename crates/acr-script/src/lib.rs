//! Mission and AI scripting.
//!
//! Every original mission (`Missions/**/Mission.dll`) and AI nation (`AI/*.dll`) is a
//! 32-bit x86 DLL that calls into the game through functions exported by `dmcr.exe`
//! (`CreateObject0`, `SetResource`, ...). The plan:
//!
//! 1. [`pe`]: read those DLLs (sections, imports, exports, relocations). Done.
//! 2. Run the original DLL code in an embedded x86 interpreter ([`x86`]) that routes
//!    imported calls to Rust ([`runtime`]), so all missions work unchanged on ARM and in
//!    the browser. The C runtime start-up runs on [`kernel32`] shims; the game API is a
//!    recording stub ([`dmcr`]) for now. See `docs/script-runtime.md`.
//! 3. Implement the imported API on top of `acr-sim`.
//! 4. Optionally port missions to native Rust one by one.

pub mod analysis;
pub mod dmcr;
pub mod heap;
pub mod kernel32;
pub mod loader;
pub mod memory;
pub mod pe;
pub mod runtime;
pub mod seh;
pub mod x86;
