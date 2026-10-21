//! Reference CPU rasterizer for decoded sprites.
//!
//! Used by tools and tests to turn a [`Frame`] into RGBA pixels. The real renderer
//! does the same on the GPU; keep both in sync when blending rules are confirmed.

use crate::gp::{Encoding, Frame};
use crate::pal::Palette;

/// First palette index of the player color ramps; each player owns 4 consecutive shades.
pub const PLAYER_RAMP_BASE: u8 = 0xD0;
pub const PLAYER_COUNT: u8 = 8;

/// An RGBA image with straight (non-premultiplied) alpha.
#[derive(Debug, Clone)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Image {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            rgba: vec![0; (width * height * 4) as usize],
        }
    }

    fn blend(&mut self, x: i32, y: i32, [r, g, b]: [u8; 3], alpha: u8) {
        if x < 0 || y < 0 || x as u32 >= self.width || y as u32 >= self.height {
            return;
        }
        let i = ((y as u32 * self.width + x as u32) * 4) as usize;
        let px = &mut self.rgba[i..i + 4];
        let a = u16::from(alpha);
        let mix =
            |dst: u8, src: u8| ((u16::from(src) * a + u16::from(dst) * (255 - a)) / 255) as u8;
        px.copy_from_slice(&[
            mix(px[0], r),
            mix(px[1], g),
            mix(px[2], b),
            px[3].max(alpha),
        ]);
    }
}

pub struct RasterOptions<'a> {
    pub palette: &'a Palette,
    pub player: u8,
    /// Raw encoding bytes to draw; `None` draws every layer.
    pub only: Option<&'a [u8]>,
}

/// Draws `frame` into `image` with the frame origin at (`ox`, `oy`).
pub fn draw_frame(image: &mut Image, frame: &Frame, ox: i32, oy: i32, opt: &RasterOptions) {
    for layer in &frame.layers {
        if opt
            .only
            .is_some_and(|only| !only.contains(&layer.encoding.to_byte()))
        {
            continue;
        }
        let mut pixels = layer.pixels.iter();
        for run in &layer.runs {
            let y = oy + i32::from(layer.y) + i32::from(run.y);
            for i in 0..run.len {
                let x = ox + i32::from(layer.x) + i32::from(run.x) + i32::from(i);
                let v = pixels.next().copied().unwrap_or(0);
                // Alpha values for edge and shadow layers are approximations until the
                // original blend tables (`*.XLT`) are decoded.
                let (color, alpha) = match layer.encoding {
                    Encoding::DictLz | Encoding::SelfLz(_) => (opt.palette.rgb(v), 255),
                    // Anti-aliased outline pixels.
                    Encoding::Raw(level) => (opt.palette.rgb(v), 64 * (level - 1)),
                    Encoding::PlayerColor => (
                        opt.palette
                            .rgb(PLAYER_RAMP_BASE + 4 * (opt.player % PLAYER_COUNT) + v),
                        255,
                    ),
                    // Soft shadow with 16 darkness levels.
                    Encoding::Packed4(_) => ([0, 0, 0], v * 10),
                    Encoding::Shadow => ([0, 0, 0], 128),
                    Encoding::Unknown(_) => ([255, 0, 255], 255),
                    Encoding::Empty => continue,
                };
                image.blend(x, y, color, alpha);
            }
        }
    }
}
