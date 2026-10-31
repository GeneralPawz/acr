//! The army frame and the plane arithmetic the controller uses: integer only, the engine's
//! tables ([`acr_sim::trig`], cosine and sine scaled by 256).
//!
//! An army's frame has `f` along its facing and `s` to its right-hand side, in world pixels
//! from the army's origin: the inverse of the frame `ingame/editor/presets.rs` lays armies
//! out in.

use acr_sim::trig::{cos, sin};

/// A world offset from an army's origin as `(f, s)` for an army facing `dir`.
pub fn to_army(dir: u8, dx: i32, dy: i32) -> (i32, i32) {
    let (c, s) = (cos(dir), sin(dir));
    ((dx * c + dy * s) >> 8, (-dx * s + dy * c) >> 8)
}

/// The world offset of the army-frame point `(f, s)` for an army facing `dir`.
pub fn to_world(dir: u8, f: i32, s: i32) -> (i32, i32) {
    let (c, sn) = (cos(dir), sin(dir));
    ((f * c - s * sn) >> 8, (f * sn + s * c) >> 8)
}

/// The integer square root (floor) of `v`; negative values give 0.
pub fn isqrt(v: i64) -> i64 {
    if v <= 0 {
        return 0;
    }
    let mut x = 1i64 << ((64 - v.leading_zeros()).div_ceil(2));
    loop {
        let y = (x + v / x) >> 1;
        if y >= x {
            return x;
        }
        x = y;
    }
}

/// The Euclidean distance of two world points, pixels (rounded down).
pub fn dist(a: (i32, i32), b: (i32, i32)) -> i32 {
    let (dx, dy) = (i64::from(a.0 - b.0), i64::from(a.1 - b.1));
    isqrt(dx * dx + dy * dy) as i32
}
