//! A glyph atlas of one original font: every byte 0x20..=0xFF rendered alone with
//! `Font::draw` (shadow included), packed into one PNG, plus the metrics to lay a string out
//! (`GameText.svelte`). Placing the glyph images one after another, each over the last, gives
//! the pixels of `ShowString`.
//!
//! Metrics JSON: `{"height": H, "glyphs": {"<char>": [x, y, w, h, left, top, advance]}}`;
//! `x, y, w, h` is the glyph in the PNG, `left, top` where its top-left pixel lies from the
//! pen position and `advance` how far the pen moves. The keys are the CP1252 characters of the
//! bytes, so the UI needs no code page. A space has `w = h = 0`.

use acr_formats::CodePage;
use acr_formats::raster::Image;

use super::assets::{AssetError, png};
use crate::ui::fonts::{FontId, Fonts};
use crate::ui::library::Library;

/// Atlas width in pixels; glyphs are packed in rows.
const WIDTH: u32 = 512;
/// The `ShowString` escape, which draws nothing.
const ESCAPE: u8 = 0x7f;

pub struct Atlas {
    pub png: Vec<u8>,
    pub json: String,
}

/// The atlas of `font` in palette `set`; `None` while the font's files load.
pub fn build(lib: &Library, fonts: &Fonts, font: FontId, set: u8) -> Result<Option<Atlas>, AssetError> {
    struct Glyph {
        key: String,
        image: Option<(Image, [i32; 2])>,
        advance: i32,
    }
    let mut glyphs = Vec::new();
    for byte in 0x20..=0xffu8 {
        if byte == ESCAPE {
            continue;
        }
        let Some(image) = fonts.rasterize_in(lib, font, &[byte], set) else { return Ok(None) };
        let key = CodePage::Cp1252.decode(&[byte]);
        if glyphs.iter().any(|g: &Glyph| g.key == key) {
            continue;
        }
        glyphs.push(Glyph { key, image, advance: fonts.char_width(lib, font, byte) });
    }
    // Shelf packing, one pixel apart.
    let (mut x, mut y, mut row) = (0u32, 0u32, 0u32);
    let mut placed = Vec::new();
    for g in &glyphs {
        let Some((image, _)) = &g.image else {
            placed.push([0, 0]);
            continue;
        };
        if x + image.width > WIDTH {
            (x, y, row) = (0, y + row + 1, 0);
        }
        placed.push([x, y]);
        x += image.width + 1;
        row = row.max(image.height);
    }
    let mut atlas = Image::new(WIDTH, y + row + 1);
    for (g, at) in glyphs.iter().zip(&placed) {
        let Some((image, _)) = &g.image else { continue };
        for line in 0..image.height {
            let src = (line * image.width * 4) as usize;
            let dst = (((at[1] + line) * WIDTH + at[0]) * 4) as usize;
            let len = (image.width * 4) as usize;
            atlas.rgba[dst..dst + len].copy_from_slice(&image.rgba[src..src + len]);
        }
    }
    let entries: serde_json::Map<String, serde_json::Value> = glyphs
        .iter()
        .zip(&placed)
        .map(|(g, at)| {
            let entry = match &g.image {
                Some((image, offset)) => {
                    serde_json::json!([at[0], at[1], image.width, image.height, offset[0], offset[1], g.advance])
                }
                None => serde_json::json!([0, 0, 0, 0, 0, 0, g.advance]),
            };
            (g.key.clone(), entry)
        })
        .collect();
    let json = serde_json::json!({ "height": fonts.height(lib, font), "glyphs": entries }).to_string();
    Ok(Some(Atlas { png: png(&atlas)?, json }))
}
