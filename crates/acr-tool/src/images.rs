//! Bitmap export and palette loading shared by the image commands.

use std::path::Path;

use acr_formats::bmp::Pixels;
use acr_formats::raster::Image;
use acr_formats::{Bitmap, Palette, Vfs};

use crate::Result;

/// Loads a palette from a `.PAL` file or from the colour table of an 8-bit `.BMP`.
pub fn load_palette(vfs: &mut Vfs, path: &str) -> Result<Palette> {
    let bytes = vfs.read(path)?;
    if path.to_ascii_uppercase().ends_with(".BMP") {
        return match Bitmap::parse(&bytes)?.pixels {
            Pixels::Indexed { palette, .. } => Ok(*palette),
            Pixels::Rgb(_) => Err(format!("{path} has no palette").into()),
        };
    }
    Ok(Palette::parse(&bytes).ok_or("palette too short")?)
}

pub fn bmp_png(dir: &Path, bmp: &str, out: &Path) -> Result<()> {
    let mut vfs = Vfs::open_game_dir(dir)?;
    let bitmap = Bitmap::parse(&vfs.read(bmp)?)?;
    let image = Image { width: bitmap.width, height: bitmap.height, rgba: bitmap.to_rgba() };
    crate::write_png(out, &image)
}
