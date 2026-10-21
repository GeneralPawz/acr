//! `font-png`: renders a string with one of the original fonts.

use std::path::Path;

use acr_formats::font::{Font, FontColors, Glyphs, GpGlyphs, named_font};
use acr_formats::raster::Image;
use acr_formats::rlc::RlcFile;
use acr_formats::{Bitmap, GpFile, Palette, Vfs};

use crate::Result;

struct Options {
    palette_set: u8,
    color: Option<u8>,
    background: Option<String>,
    at: (i32, i32),
    crop: Option<[u32; 4]>,
    zoom: u32,
}

impl Options {
    fn parse(args: &[&str]) -> Result<Self> {
        let mut o = Self { palette_set: 1, color: None, background: None, at: (0, 0), crop: None, zoom: 1 };
        let mut it = args.iter();
        while let Some(&flag) = it.next() {
            let value = *it.next().ok_or_else(|| format!("{flag} needs a value"))?;
            match flag {
                "--set" => o.palette_set = value.parse()?,
                "--color" => o.color = Some(value.parse()?),
                "--bg" => o.background = Some(value.to_owned()),
                "--at" => {
                    let (x, y) = value.split_once(',').ok_or("--at expects X,Y")?;
                    o.at = (x.parse()?, y.parse()?);
                }
                "--crop" => {
                    let v = value.split(',').map(str::parse).collect::<std::result::Result<Vec<u32>, _>>()?;
                    o.crop = Some(v.try_into().map_err(|_| "--crop expects X,Y,W,H")?);
                }
                "--zoom" => o.zoom = value.parse::<u32>()?.max(1),
                _ => return Err(format!("unknown option {flag}").into()),
            }
        }
        Ok(o)
    }
}

/// `font` is a global font name (`fonG60w`), a sprite pack name (`rom10`) or an `.RLC` file.
/// The text is taken as Latin-1/CP1252; `\xNN` escapes insert raw bytes.
pub fn font_png(dir: &Path, font: &str, text: &str, out: &Path, args: &[&str]) -> Result<()> {
    let o = Options::parse(args)?;
    let mut vfs = Vfs::open_game_dir(dir)?;
    let mut read = |path: &str| vfs.read(path).ok();
    let mut font = if let Some(named) = named_font(font) {
        named.load(&mut read)?
    } else if font.to_ascii_uppercase().ends_with(".RLC") {
        Font::new(Glyphs::Rlc(RlcFile::parse(&read(font).ok_or("RLC file not found")?)?))
    } else {
        let gp = GpFile::parse(read(&format!("{font}.GP")).ok_or("font not found")?)?;
        Font::new(Glyphs::Gp(GpGlyphs::new(&gp, read(&format!("{font}.GPI")).as_deref())?))
    };
    if let Some(color) = o.color {
        font.color = color;
    }
    let palette_path = format!("{}\\AGEW_1.PAL", o.palette_set);
    let palette = Palette::parse(&read(&palette_path).ok_or("palette not found")?).ok_or("palette too short")?;
    let colors = FontColors::load(palette, o.palette_set, &mut read);
    let bytes = encode(text);
    let rendered = font.draw(&bytes, &colors);
    eprintln!(
        "width {} height {} image {}x{} at ({}, {})",
        font.text_width(&bytes),
        font.height(),
        rendered.image.width,
        rendered.image.height,
        rendered.left,
        rendered.top
    );
    let image = match &o.background {
        None => rendered.image,
        Some(bmp) => {
            let bitmap = Bitmap::parse(&read(bmp).ok_or("background not found")?)?;
            let mut image = Image { width: bitmap.width, height: bitmap.height, rgba: bitmap.to_rgba() };
            composite(&mut image, &rendered.image, o.at.0 + rendered.left, o.at.1 + rendered.top);
            image
        }
    };
    let image = match o.crop {
        Some([x, y, w, h]) => {
            scale(&image, x, y, w.min(image.width.saturating_sub(x)), h.min(image.height.saturating_sub(y)), o.zoom)
        }
        None => scale(&image, 0, 0, image.width, image.height, o.zoom),
    };
    crate::write_png(out, &image)
}

/// Cuts out a rectangle and enlarges it `zoom` times (nearest neighbour).
fn scale(src: &Image, x0: u32, y0: u32, w: u32, h: u32, zoom: u32) -> Image {
    let mut out = Image::new(w * zoom, h * zoom);
    for y in 0..out.height {
        for x in 0..out.width {
            let s = (((y0 + y / zoom) * src.width + x0 + x / zoom) * 4) as usize;
            let d = ((y * out.width + x) * 4) as usize;
            out.rgba[d..d + 4].copy_from_slice(&src.rgba[s..s + 4]);
        }
    }
    out
}

fn encode(text: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' && chars.peek() == Some(&'x') {
            chars.next();
            let hex: String = chars.by_ref().take(2).collect();
            bytes.push(u8::from_str_radix(&hex, 16).unwrap_or(b'?'));
        } else {
            bytes.push(u8::try_from(u32::from(c)).unwrap_or(b'?'));
        }
    }
    bytes
}

fn composite(dst: &mut Image, src: &Image, x0: i32, y0: i32) {
    for y in 0..src.height as i32 {
        for x in 0..src.width as i32 {
            let (dx, dy) = (x0 + x, y0 + y);
            if dx < 0 || dy < 0 || dx >= dst.width as i32 || dy >= dst.height as i32 {
                continue;
            }
            let s = &src.rgba[((y as u32 * src.width + x as u32) * 4) as usize..][..4];
            let d = &mut dst.rgba[((dy as u32 * dst.width + dx as u32) * 4) as usize..][..4];
            let a = u32::from(s[3]);
            for k in 0..3 {
                d[k] = ((u32::from(s[k]) * a + u32::from(d[k]) * (255 - a) + 127) / 255) as u8;
            }
        }
    }
}
