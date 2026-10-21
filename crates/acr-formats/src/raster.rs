//! Reference CPU rasterizer for decoded sprites.
//!
//! Used by tools and tests to turn a [`Frame`] into RGBA pixels. The real renderer
//! does the same on the GPU with the same constants.
//!
//! The original draws into an 8-bit frame buffer and blends through palette lookup tables
//! (`*.GRD`, see [`crate::grd`]). Over an RGBA canvas those become the alpha values below,
//! fitted from the tables with `acr grd-fit`.

use crate::gp::{Encoding, Frame};
use crate::pal::Palette;

/// First palette index of the player color ramps; each player owns 4 consecutive shades.
pub const PLAYER_RAMP_BASE: u8 = 0xD0;
pub const PLAYER_COUNT: u8 = 8;

/// Opacity of the sprite colour for [`Encoding::Raw`] 2, 3 and 4. The original maps
/// `(below, sprite)` through `AGEW_TR4.GRD` (a 75 %/25 % mix of row and column colour) for
/// 2, through `AGEW_TR.GRD` (50 %) for 3 and through `AGEW_TR4.GRD` with swapped operands for 4.
pub const TRANSLUCENT_ALPHA: [u8; 3] = [63, 128, 192];

/// Opacity of black for each darkness level of [`Encoding::Shade4`] / [`Encoding::Shade8`]:
/// the rows of `AGEW_1D.GRD` fitted against `AGEW_1.PAL` (palette set 0; sets 1 and 2 agree
/// within 4/255).
pub const SHADE_ALPHA: [u8; 32] = [
    0, 1, 12, 23, 33, 41, 50, 58, 65, 78, 86, 93, 102, 109, 117, 126, 133, 142, 149, 158, 166, 173, 182, 189, 197, 205,
    213, 220, 226, 232, 240, 255,
];

/// [`Encoding::Shadow`] darkens with row 12 of `AGEW_1D.GRD` unless a sprite pack selects
/// another shadow table.
pub const SHADOW_LEVEL: u8 = 12;

/// Colour and opacity of one pixel of a layer, or `None` if the layer draws nothing.
pub fn pixel_color(encoding: Encoding, value: u8, palette: &Palette, player: u8) -> Option<([u8; 3], u8)> {
    Some(match encoding {
        Encoding::DictLz | Encoding::SelfLz(_) => (palette.rgb(value), 255),
        Encoding::Raw(level) => (palette.rgb(value), TRANSLUCENT_ALPHA[usize::from(level - 2)]),
        Encoding::PlayerColor => (palette.rgb(PLAYER_RAMP_BASE + 4 * (player % PLAYER_COUNT) + (value & 3)), 255),
        Encoding::Shade4 | Encoding::Shade8 => ([0, 0, 0], SHADE_ALPHA[usize::from(value.min(31))]),
        Encoding::Shadow => ([0, 0, 0], SHADE_ALPHA[usize::from(SHADOW_LEVEL)]),
        Encoding::Empty | Encoding::Unknown(_) => return None,
    })
}

/// An RGBA image with straight (non-premultiplied) alpha.
#[derive(Debug, Clone)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Image {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height, rgba: vec![0; (width * height * 4) as usize] }
    }

    fn blend(&mut self, x: i32, y: i32, [r, g, b]: [u8; 3], alpha: u8) {
        if x < 0 || y < 0 || x as u32 >= self.width || y as u32 >= self.height {
            return;
        }
        let i = ((y as u32 * self.width + x as u32) * 4) as usize;
        let px = &mut self.rgba[i..i + 4];
        let a = u16::from(alpha);
        let mix = |dst: u8, src: u8| ((u16::from(src) * a + u16::from(dst) * (255 - a)) / 255) as u8;
        px.copy_from_slice(&[mix(px[0], r), mix(px[1], g), mix(px[2], b), px[3].max(alpha)]);
    }
}

pub struct RasterOptions<'a> {
    pub palette: &'a Palette,
    pub player: u8,
    /// Encoding codes ([`Encoding::to_byte`]) to draw; `None` draws every layer.
    pub only: Option<&'a [u8]>,
}

/// Draws `frame` into `image` with the frame origin at (`ox`, `oy`).
pub fn draw_frame(image: &mut Image, frame: &Frame, ox: i32, oy: i32, opt: &RasterOptions) {
    for layer in &frame.layers {
        if opt.only.is_some_and(|only| !only.contains(&layer.encoding.to_byte())) {
            continue;
        }
        let mut pixels = layer.pixels.iter();
        for run in &layer.runs {
            let y = oy + i32::from(layer.y) + i32::from(run.y);
            for i in 0..run.len {
                let x = ox + i32::from(layer.x) + i32::from(run.x) + i32::from(i);
                let v = pixels.next().copied().unwrap_or(0);
                let Some((color, alpha)) = pixel_color(layer.encoding, v, opt.palette, opt.player) else { continue };
                image.blend(x, y, color, alpha);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blend_strengths() {
        let palette = Palette(std::array::from_fn(|i| [i as u8; 3]));
        let alpha = |encoding, value| pixel_color(encoding, value, &palette, 0).map(|(_, a)| a);
        assert_eq!(alpha(Encoding::Raw(2), 9), Some(63));
        assert_eq!(alpha(Encoding::Raw(4), 9), Some(192));
        assert_eq!(alpha(Encoding::Shade8, 31), Some(255));
        assert_eq!(alpha(Encoding::Shade4, 0), Some(0));
        assert_eq!(alpha(Encoding::Shadow, 0), Some(102));
        assert_eq!(alpha(Encoding::Empty, 0), None);
        assert_eq!(pixel_color(Encoding::PlayerColor, 3, &palette, 1), Some(([0xD7; 3], 255)));
    }
}
