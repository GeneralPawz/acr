//! Layout canvases scaled into the window: the original 1024x768 menu canvas with
//! letterboxing, and the in-game screen canvas that covers the whole window.

use crate::ui::draw::Rect;

pub const WIDTH: f32 = 1024.0;
pub const HEIGHT: f32 = 768.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Canvas {
    pub scale: f32,
    /// Window position of the canvas origin.
    pub origin: [f32; 2],
    /// The canvas area that is drawn; draws outside it are cut off.
    pub clip: Rect,
}

impl Canvas {
    /// A canvas without bounds (the map, window pixels).
    pub fn new(scale: f32, origin: [f32; 2]) -> Self {
        Self { scale, origin, clip: Rect::ALL }
    }

    /// The largest 4:3 area centred in a window of `size` pixels.
    pub fn fit(size: [f32; 2]) -> Self {
        let scale = (size[0] / WIDTH).min(size[1] / HEIGHT);
        let origin = [((size[0] - WIDTH * scale) / 2.0).floor(), ((size[1] - HEIGHT * scale) / 2.0).floor()];
        Self { scale, origin, clip: Rect::SCREEN }
    }

    /// The in-game screen (`RealLx` × `RealLy` of the original): the whole window, at the
    /// scale of [`Canvas::fit`] so the interface keeps its size. One side is 1024 or 768
    /// canvas pixels, the other at least as long; see [`screen_size`].
    pub fn screen(size: [f32; 2]) -> Self {
        let scale = (size[0] / WIDTH).min(size[1] / HEIGHT);
        let [w, h] = screen_size(size);
        Self { scale, origin: [0.0, 0.0], clip: Rect::new(0, 0, w, h) }
    }

    /// The size of the drawn area in canvas pixels.
    pub fn size(self) -> [i32; 2] {
        [self.clip.width(), self.clip.height()]
    }

    /// Converts a canvas rectangle `[x, y, w, h]` to window pixels.
    pub fn to_window(self, rect: [f32; 4]) -> [f32; 4] {
        [
            self.origin[0] + rect[0] * self.scale,
            self.origin[1] + rect[1] * self.scale,
            rect[2] * self.scale,
            rect[3] * self.scale,
        ]
    }

    /// Converts a window position to canvas coordinates.
    pub fn to_virtual(self, p: [f32; 2]) -> [f32; 2] {
        [(p[0] - self.origin[0]) / self.scale, (p[1] - self.origin[1]) / self.scale]
    }
}

/// The in-game screen size for a window of `size` pixels: the window divided by the
/// scale of [`Canvas::fit`], at least 1024 × 768.
pub fn screen_size(size: [f32; 2]) -> [i32; 2] {
    let scale = (size[0] / WIDTH).min(size[1] / HEIGHT);
    if scale.is_nan() || scale <= 0.0 {
        return [WIDTH as i32, HEIGHT as i32];
    }
    [((size[0] / scale).round() as i32).max(WIDTH as i32), ((size[1] / scale).round() as i32).max(HEIGHT as i32)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letterboxes_wide_windows() {
        let c = Canvas::fit([5120.0, 1440.0]);
        assert_eq!(c.scale, 1.875);
        assert_eq!(c.origin, [1600.0, 0.0]);
        assert_eq!(c.to_window([0.0, 0.0, WIDTH, HEIGHT]), [1600.0, 0.0, 1920.0, 1440.0]);
        assert_eq!(c.to_virtual([1600.0 + 1.875 * 512.0, 720.0]), [512.0, 384.0]);
    }

    #[test]
    fn screen_covers_the_window() {
        let c = Canvas::screen([1024.0, 768.0]);
        assert_eq!((c.scale, c.origin, c.size()), (1.0, [0.0, 0.0], [1024, 768]));
        let c = Canvas::screen([2048.0, 1536.0]);
        assert_eq!((c.scale, c.size()), (2.0, [1024, 768]));
        // Wide: 768 canvas pixels high, as wide as the window.
        let c = Canvas::screen([5120.0, 1440.0]);
        assert_eq!((c.scale, c.origin, c.size()), (1.875, [0.0, 0.0], [2731, 768]));
        assert_eq!(c.to_virtual([1875.0, 1440.0]), [1000.0, 768.0]);
        // Tall: 1024 wide.
        assert_eq!(Canvas::screen([1341.0, 1382.0]).size(), [1024, 1055]);
        assert_eq!(screen_size([1870.0, 1272.0]), [1129, 768]);
        assert_eq!(screen_size([800.0, 600.0]), [1024, 768]);
        assert_eq!(screen_size([0.0, 0.0]), [1024, 768]);
    }
}
