//! Fixed-point coordinates and directions, as the original engine stores them.
//!
//! Positions are `i32` in 1/16 world pixel (the unit of `UNI3` coordinates and of the
//! engine's object positions); directions are a byte, 256 steps per full turn.

/// Sub-pixel steps per world pixel.
pub const SUBPIXEL: i32 = 16;

/// A point in 1/16 world pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Pos {
    pub x: i32,
    pub y: i32,
}

impl Pos {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    /// From world pixels.
    pub const fn from_pixels(x: i32, y: i32) -> Self {
        Self { x: x * SUBPIXEL, y: y * SUBPIXEL }
    }

    /// World pixels, rounded towards negative infinity.
    pub const fn pixels(self) -> (i32, i32) {
        (self.x.div_euclid(SUBPIXEL), self.y.div_euclid(SUBPIXEL))
    }

    /// Cell of a grid with `cell` world pixels per side.
    pub const fn cell(self, cell: i32) -> (i32, i32) {
        let (x, y) = self.pixels();
        (x.div_euclid(cell), y.div_euclid(cell))
    }
}
