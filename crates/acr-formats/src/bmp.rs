//! Windows bitmaps (`*.BMP`): menu backgrounds, previews, portraits.
//!
//! The game ships uncompressed 8-bit images with their own palette and some 24-bit ones.
//! 8-bit images keep their indices so they can share the palette pipeline of sprites.

use crate::pal::Palette;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BmpError(pub &'static str);

impl std::fmt::Display for BmpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

impl std::error::Error for BmpError {}

#[derive(Debug, Clone)]
pub enum Pixels {
    /// Row-major palette indices, top row first.
    Indexed { palette: Box<Palette>, indices: Vec<u8> },
    /// Row-major RGB, top row first.
    Rgb(Vec<[u8; 3]>),
}

#[derive(Debug, Clone)]
pub struct Bitmap {
    pub width: u32,
    pub height: u32,
    pub pixels: Pixels,
}

impl Bitmap {
    pub fn parse(data: &[u8]) -> Result<Self, BmpError> {
        let u16_at = |i: usize| data.get(i..i + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
        let u32_at = |i: usize| data.get(i..i + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()));
        if data.get(..2) != Some(b"BM") {
            return Err(BmpError("missing BM signature"));
        }
        let truncated = BmpError("bitmap header truncated");
        let pixel_offset = u32_at(10).ok_or(truncated.clone())? as usize;
        let header_len = u32_at(14).ok_or(truncated.clone())? as usize;
        let width = u32_at(18).ok_or(truncated.clone())? as i32;
        let raw_height = u32_at(22).ok_or(truncated.clone())? as i32;
        let bits = u16_at(28).ok_or(truncated.clone())?;
        let compression = u32_at(30).ok_or(truncated.clone())?;
        if compression != 0 {
            return Err(BmpError("compressed bitmaps are not supported"));
        }
        if width <= 0 || raw_height == 0 {
            return Err(BmpError("invalid dimensions"));
        }
        // Positive height means rows are stored bottom-up.
        let bottom_up = raw_height > 0;
        let (width, height) = (width as usize, raw_height.unsigned_abs() as usize);
        let stride = (width * usize::from(bits)).div_ceil(32) * 4;
        let rows = data.get(pixel_offset..pixel_offset + stride * height).ok_or(BmpError("pixel data truncated"))?;
        let row = |y: usize| &rows[(if bottom_up { height - 1 - y } else { y }) * stride..][..stride];

        let pixels = match bits {
            8 => {
                let used = u32_at(46).ok_or(truncated)? as usize;
                let count = if used == 0 { 256 } else { used.min(256) };
                let table =
                    data.get(14 + header_len..14 + header_len + 4 * count).ok_or(BmpError("palette truncated"))?;
                let mut colors = [[0u8; 3]; 256];
                for (c, bgrx) in colors.iter_mut().zip(table.as_chunks::<4>().0) {
                    *c = [bgrx[2], bgrx[1], bgrx[0]];
                }
                let indices = (0..height).flat_map(|y| row(y)[..width].iter().copied()).collect();
                Pixels::Indexed { palette: Box::new(Palette(colors)), indices }
            }
            24 => Pixels::Rgb(
                (0..height)
                    .flat_map(|y| row(y)[..width * 3].as_chunks::<3>().0.iter().map(|p| [p[2], p[1], p[0]]))
                    .collect(),
            ),
            _ => return Err(BmpError("only 8-bit and 24-bit bitmaps are supported")),
        };
        Ok(Self { width: width as u32, height: height as u32, pixels })
    }

    /// RGBA pixels, top row first.
    pub fn to_rgba(&self) -> Vec<u8> {
        match &self.pixels {
            Pixels::Indexed { palette, indices } => {
                indices.iter().flat_map(|&i| palette.rgb(i).into_iter().chain([255])).collect()
            }
            Pixels::Rgb(rgb) => rgb.iter().flat_map(|p| p.iter().copied().chain([255])).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 3x2 8-bit bitmap, bottom-up, rows padded to 4 bytes.
    fn sample() -> Vec<u8> {
        let mut d = b"BM".to_vec();
        d.extend([0u8; 8]);
        d.extend((14u32 + 40 + 1024).to_le_bytes());
        d.extend(40u32.to_le_bytes());
        d.extend(3i32.to_le_bytes());
        d.extend(2i32.to_le_bytes());
        d.extend(1u16.to_le_bytes());
        d.extend(8u16.to_le_bytes());
        d.extend([0u8; 24]);
        for i in 0..=255u8 {
            d.extend([i, 0, 255 - i, 0]);
        }
        d.extend([4, 5, 6, 0]); // bottom row
        d.extend([1, 2, 3, 0]); // top row
        d
    }

    #[test]
    fn reads_bottom_up_indexed() {
        let b = Bitmap::parse(&sample()).unwrap();
        assert_eq!((b.width, b.height), (3, 2));
        let Pixels::Indexed { palette, indices } = &b.pixels else { panic!("expected indexed") };
        assert_eq!(indices, &[1, 2, 3, 4, 5, 6]);
        assert_eq!(palette.rgb(1), [254, 0, 1]);
        assert_eq!(&b.to_rgba()[..4], &[254, 0, 1, 255]);
    }

    #[test]
    fn rejects_non_bmp() {
        assert!(Bitmap::parse(b"GPAK").is_err());
    }
}
