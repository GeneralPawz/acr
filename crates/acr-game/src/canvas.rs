//! The original 1024x768 layout canvas, scaled into the window with letterboxing.

pub const WIDTH: f32 = 1024.0;
pub const HEIGHT: f32 = 768.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Canvas {
    pub scale: f32,
    /// Window position of the canvas origin.
    pub origin: [f32; 2],
}

impl Canvas {
    /// The largest 4:3 area centred in a window of `size` pixels.
    pub fn fit(size: [f32; 2]) -> Self {
        let scale = (size[0] / WIDTH).min(size[1] / HEIGHT);
        let origin = [((size[0] - WIDTH * scale) / 2.0).floor(), ((size[1] - HEIGHT * scale) / 2.0).floor()];
        Self { scale, origin }
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
}
