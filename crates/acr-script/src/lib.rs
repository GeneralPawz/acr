//! Mission and AI scripting.
//!
//! Every original mission (`Missions/**/Mission.dll`) and AI nation (`AI/*.dll`) is a
//! 32-bit x86 DLL that calls into the game through functions exported by `dmcr.exe`
//! (`CreateObject0`, `SetResource`, ...). The plan:
//!
//! 1. [`pe`]: read those DLLs (sections, imports, exports, relocations). Done for imports
//!    and exports.
//! 2. Implement the imported API on top of `acr-sim`.
//! 3. Run the original DLL code in an embedded x86 interpreter that routes imported calls
//!    to step 2, so all missions work unchanged on ARM and in the browser.
//! 4. Optionally port missions to native Rust one by one.

pub mod pe;
