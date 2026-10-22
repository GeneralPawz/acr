//! Bitmap fonts of the original (`RLCFont`): text width, height and rasterization.
//!
//! Reverse engineered from `ShowString` (`0x483382`), `ShowCharUNICODE` (`0x483119`),
//! `GetRLCStrWidth` (`0x4834DC`), `GP_System::ShowGP` (`0x48BE47`) and the font set-up at
//! `0x4A54EB` in `dmcr.exe`; `docs/formats/fonts.md` has the evidence.
//!
//! A font is a sprite pack (`FONT*.GP`) with one frame per CP1252 byte, plus an optional
//! `.GPI` file with per-frame advance widths and left shifts, an optional second pack drawn
//! underneath as a shadow, and a colour table: glyph pixels are darkness levels
//! ([`Encoding::Shade4`] / [`Encoding::Shade8`]) that the original maps through one of seven
//! 32-row lookup tables (`<set>\*.GRD`). A few old fonts are `.RLC` picture lists instead.

use crate::gp::{Encoding, Frame, GpError, GpFile};
use crate::grd::BlendTable;
use crate::pal::Palette;
use crate::raster::{Image, pixel_color};
use crate::rlc::{RlcFile, RlcItem};

/// Number of colour tables a font can select ([`Font::color`]).
pub const COLOR_TABLE_COUNT: usize = 7;

/// Lookup table of each colour mode, loaded from the active palette set directory by
/// `0x483CA2` (`<set>\<file>`). Mode 0 is also used for shadows. Modes 3..=6 can be replaced
/// per screen by `LoadOptionalTable` (`0x4838EC`), e.g. with `2\dark.grd` or `1\white.grd`.
pub const COLOR_TABLE_FILES: [&str; COLOR_TABLE_COUNT] =
    ["AGEW_1D.GRD", "AGEW_1BR.GRD", "AGEW_1R.GRD", "ORANGE.GRD", "OPT2.GRD", "OPT3.GRD", "OPT4.GRD"];

/// Path of the colour table for `mode` in palette set `palette_set` (0, 1 or 2), e.g.
/// `1\AGEW_1R.GRD`. Set 2 has no `OPT2..4.GRD`; the original then keeps the table of the
/// set loaded before (see [`FontColors::load`]).
pub fn color_table_path(palette_set: u8, mode: usize) -> String {
    format!("{palette_set}\\{}", COLOR_TABLE_FILES[mode % COLOR_TABLE_COUNT])
}

/// Byte that starts a `0x7F` + 4 hex digit escape: an icon from `UNICODE.DAT` (only
/// `MainFont` and `rom10` have icons). Fonts without icons draw nothing for it.
pub const ESCAPE: u8 = 0x7F;
const ESCAPE_LEN: usize = 5;

/// Width of a space in a sprite font is the width of this frame (`0x482D86`).
const SPACE_WIDTH_FRAME: u8 = b'c';

/// Character [`Font::height`] measures; the original code mostly asks `GetRLCHeight` for 'W'.
pub const HEIGHT_CHAR: u8 = b'W';

/// One 32-row darkness table (`*.GRD`) together with its RGBA equivalent.
#[derive(Clone)]
pub struct ColorTable {
    table: BlendTable,
    /// Colour and opacity of each level over an RGBA background.
    levels: [([u8; 3], u8); 32],
}

impl ColorTable {
    /// `grd` must have at least 32 rows. Row 31 of every shipped font table is one colour
    /// (the text colour); row `l` blends it over the background. The opacity of each row is
    /// the least-squares fit over all 256 background colours.
    pub fn new(grd: &[u8], palette: &Palette) -> Option<Self> {
        let table = BlendTable::parse(grd.get(..32 * 256)?)?;
        let color = palette.rgb(table.lookup(31, 0));
        let levels = std::array::from_fn(|level| {
            let (mut num, mut den) = (0.0f64, 0.0f64);
            for below in 0..=255u8 {
                let (b, o) = (palette.rgb(below), palette.rgb(table.lookup(level as u8, below)));
                for k in 0..3 {
                    let x = f64::from(color[k]) - f64::from(b[k]);
                    num += x * (f64::from(o[k]) - f64::from(b[k]));
                    den += x * x;
                }
            }
            let alpha = if level == 0 || den == 0.0 { 0.0 } else { (num / den).clamp(0.0, 1.0) };
            (color, if level == 31 { 255 } else { (alpha * 255.0).round() as u8 })
        });
        Some(Self { table, levels })
    }

    /// Colour and opacity of darkness `level` (levels below 0 draw nothing, above 31 clamp).
    pub fn level(&self, level: i32) -> ([u8; 3], u8) {
        if level <= 0 { ([0; 3], 0) } else { self.levels[level.min(31) as usize] }
    }

    /// The original 8-bit operation: the palette index that replaces `below`.
    pub fn remap(&self, level: i32, below: u8) -> u8 {
        if level <= 0 { below } else { self.table.lookup(level.min(31) as u8, below) }
    }
}

/// Palette and colour tables of the active palette set.
#[derive(Clone)]
pub struct FontColors {
    pub palette: Palette,
    tables: [Option<ColorTable>; COLOR_TABLE_COUNT],
}

impl FontColors {
    pub fn new(palette: Palette) -> Self {
        Self { palette, tables: Default::default() }
    }

    /// Loads all tables of `palette_set` with `read` (a game path such as `1\AGEW_1R.GRD` to
    /// its bytes). A table missing in that set falls back to set 0, then 1, like the
    /// original keeps the table of an earlier set when a file does not exist.
    pub fn load(palette: Palette, palette_set: u8, mut read: impl FnMut(&str) -> Option<Vec<u8>>) -> Self {
        let mut colors = Self::new(palette);
        for mode in 0..COLOR_TABLE_COUNT {
            for set in [palette_set, 0, 1] {
                if let Some(bytes) = read(&color_table_path(set, mode))
                    && colors.set_table(mode, &bytes)
                {
                    break;
                }
            }
        }
        colors
    }

    /// Replaces the table of `mode` (like `LoadOptionalTable`); false if `grd` is too short.
    pub fn set_table(&mut self, mode: usize, grd: &[u8]) -> bool {
        let table = ColorTable::new(grd, &self.palette);
        let ok = table.is_some();
        self.tables[mode % COLOR_TABLE_COUNT] = table;
        ok
    }

    /// The table `ShowGP` uses for colour mode `mode`: 1..=6 select their own table,
    /// everything else the darkness table of mode 0.
    pub fn table(&self, mode: u8) -> Option<&ColorTable> {
        let mode = usize::from(mode);
        self.tables[if mode < COLOR_TABLE_COUNT { mode } else { 0 }].as_ref()
    }
}

/// Glyphs of a sprite font: the decoded frames plus the `.GPI` metrics.
#[derive(Debug, Clone)]
pub struct GpGlyphs {
    frames: Vec<Frame>,
    /// `GP_Header::GetLx` / `GetLy`: the largest `x + w` / `y + h` of the frame's chunks.
    lx: Vec<i32>,
    ly: Vec<i32>,
    /// `.GPI`: per frame a signed shift to the left, then a signed advance width.
    shift: Option<Vec<i8>>,
    advance: Option<Vec<i8>>,
}

impl GpGlyphs {
    /// `gpi` is the `.GPI` file next to the pack, if there is one: `frame_count` signed
    /// bytes of left shifts followed by `frame_count` signed advance widths
    /// (`GP_System::LoadGP`, `0x487147`). A shorter file is ignored.
    pub fn new(gp: &GpFile, gpi: Option<&[u8]>) -> Result<Self, GpError> {
        let n = gp.frame_count();
        let frames = (0..n).map(|i| gp.decode_frame(i)).collect::<Result<Vec<_>, _>>()?;
        let extent = |f: &Frame, pos: fn(&crate::gp::Layer) -> i32| f.layers.iter().map(pos).fold(0, i32::max);
        let lx = frames.iter().map(|f| extent(f, |l| i32::from(l.x) + i32::from(l.width as i16))).collect();
        let ly = frames.iter().map(|f| extent(f, |l| i32::from(l.y) + i32::from(l.height as i16))).collect();
        let gpi = gpi.filter(|g| g.len() >= 2 * n);
        let signed = |bytes: &[u8]| bytes.iter().map(|&b| b as i8).collect::<Vec<_>>();
        Ok(Self { frames, lx, ly, shift: gpi.map(|g| signed(&g[..n])), advance: gpi.map(|g| signed(&g[n..2 * n])) })
    }

    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    pub fn frame(&self, index: usize) -> Option<&Frame> {
        self.frames.get(index)
    }

    /// `GP_System::GetGPWidth`: the `.GPI` advance, else the frame's `Lx`; 0 past the end.
    pub fn width(&self, frame: usize) -> i32 {
        match &self.advance {
            Some(a) => a.get(frame).map_or(0, |&w| i32::from(w)),
            None => self.lx.get(frame).copied().unwrap_or(0),
        }
    }

    /// `GP_System::GetGPHeight`: the frame's `Ly`; 0 past the end.
    pub fn height(&self, frame: usize) -> i32 {
        self.ly.get(frame).copied().unwrap_or(0)
    }

    /// `GP_System::GetGPShift`: `ShowGP` draws the frame this many pixels further left.
    pub fn shift(&self, frame: usize) -> i32 {
        self.shift.as_ref().and_then(|s| s.get(frame)).map_or(0, |&s| i32::from(s))
    }
}

#[derive(Debug, Clone)]
pub enum Glyphs {
    Gp(GpGlyphs),
    Rlc(RlcFile),
}

/// A second pack drawn under every glyph (`RLCFont::SetShadowGP`), always through the
/// colour table of mode 0.
#[derive(Debug, Clone)]
pub struct Shadow {
    pub glyphs: GpGlyphs,
    pub dx: i32,
    pub dy: i32,
}

/// An `RLCFont`: glyphs, colour mode (`SetColorTable(n)`; `SetWhiteColor` = 1,
/// `SetRedColor` = 2, `SetBlackColor` = 0) and optional shadow.
#[derive(Debug, Clone)]
pub struct Font {
    pub glyphs: Glyphs,
    pub color: u8,
    pub shadow: Option<Shadow>,
}

/// A rendered string. The image's top-left pixel is at `(x + left, y + top)` when the string
/// is drawn with `ShowString(x, y, ..)`.
#[derive(Debug, Clone)]
pub struct TextImage {
    pub image: Image,
    pub left: i32,
    pub top: i32,
    /// [`Font::text_width`] of the string: where the next string would start.
    pub advance: i32,
}

/// What one step of `ShowString` draws.
struct Step {
    byte: u8,
    /// Bytes consumed (5 for an escape).
    len: usize,
    advance: i32,
}

impl Font {
    pub fn new(glyphs: Glyphs) -> Self {
        Self { glyphs, color: 0, shadow: None }
    }

    pub fn with_color(mut self, mode: u8) -> Self {
        self.color = mode;
        self
    }

    pub fn with_shadow(mut self, glyphs: GpGlyphs, dx: i32, dy: i32) -> Self {
        self.shadow = Some(Shadow { glyphs, dx, dy });
        self
    }

    /// Advance of one byte (`0x482D86`): sprite fonts use the width of 'c' for a space.
    pub fn char_width(&self, c: u8) -> i32 {
        match &self.glyphs {
            Glyphs::Gp(g) => g.width(usize::from(if c == b' ' { SPACE_WIDTH_FRAME } else { c })),
            Glyphs::Rlc(r) => r.items.get(usize::from(c)).map_or(0, |i| i32::from(i.width)),
        }
    }

    /// `GetRLCHeight(font, c)`: the height of the glyph of `c`.
    pub fn glyph_height(&self, c: u8) -> i32 {
        match &self.glyphs {
            Glyphs::Gp(g) => g.height(usize::from(c)),
            Glyphs::Rlc(r) => r.items.get(usize::from(c)).map_or(0, |i| i32::from(i.height)),
        }
    }

    /// Line height as the original UI code measures it: `GetRLCHeight(font, 'W')`.
    pub fn height(&self) -> i32 {
        self.glyph_height(HEIGHT_CHAR)
    }

    /// `GetRLCStrWidth`: the sum of the advances, up to the first NUL byte. A `0x7F` escape
    /// counts as 0 (no font in the table has icons).
    pub fn text_width(&self, text: &[u8]) -> i32 {
        let text = until_nul(text);
        let mut i = 0;
        let mut width = 0;
        while i < text.len() {
            let (advance, len) = if text[i] == ESCAPE { (0, ESCAPE_LEN) } else { (self.char_width(text[i]), 1) };
            width += advance;
            i += len;
        }
        width
    }

    /// The steps of `ShowString`. Sprite fonts treat `0x7F` + 4 hex digits as one icon
    /// (nothing drawn here); RLC fonts draw item `0x7F` like any other byte.
    fn steps(&self, text: &[u8]) -> Vec<Step> {
        let text = until_nul(text);
        let mut steps = Vec::with_capacity(text.len());
        let mut i = 0;
        while i < text.len() {
            let byte = text[i];
            let step = match (&self.glyphs, byte) {
                (Glyphs::Gp(_), ESCAPE) => Step { byte, len: ESCAPE_LEN, advance: 0 },
                _ => Step { byte, len: 1, advance: self.char_width(byte) },
            };
            i += step.len;
            steps.push(step);
        }
        steps
    }

    /// Renders `text` like `ShowString` into straight-alpha RGBA on a transparent
    /// background: per glyph first its shadow (colour mode 0), then the glyph through the
    /// font's colour table. Composited over any background this equals the original's
    /// 8-bit table lookups up to the table quantization.
    pub fn draw(&self, text: &[u8], colors: &FontColors) -> TextImage {
        self.draw_faded(text, colors, 0)
    }

    /// Like [`Font::draw`] with the original's fade-out (`0x7C3428`): colour modes 1..=4 read
    /// `fade` rows lower in their table, so `fade >= 31` makes the text invisible.
    pub fn draw_faded(&self, text: &[u8], colors: &FontColors, fade: i32) -> TextImage {
        let mut canvas = Canvas::default();
        let mut pen = 0;
        for step in self.steps(text) {
            if step.len == 1 {
                self.draw_glyph(&mut canvas, pen, step.byte, colors, fade);
            }
            pen += step.advance;
        }
        let (image, left, top) = canvas.finish();
        TextImage { image, left, top, advance: pen }
    }

    fn draw_glyph(&self, canvas: &mut Canvas, x: i32, c: u8, colors: &FontColors, fade: i32) {
        let frame = usize::from(c);
        match &self.glyphs {
            Glyphs::Gp(glyphs) => {
                if let Some(s) = &self.shadow {
                    draw_gp(canvas, &s.glyphs, frame, x + s.dx, s.dy, colors, 0, 0);
                }
                let fade = if (1..=4).contains(&self.color) { fade } else { 0 };
                draw_gp(canvas, glyphs, frame, x, 0, colors, self.color, fade);
            }
            Glyphs::Rlc(rlc) => {
                if let Some(item) = rlc.items.get(frame) {
                    draw_rlc(canvas, item, x, &colors.palette);
                }
            }
        }
    }
}

fn until_nul(text: &[u8]) -> &[u8] {
    text.iter().position(|&b| b == 0).map_or(text, |end| &text[..end])
}

#[allow(clippy::too_many_arguments)]
fn draw_gp(
    canvas: &mut Canvas,
    glyphs: &GpGlyphs,
    frame: usize,
    x: i32,
    y: i32,
    colors: &FontColors,
    mode: u8,
    fade: i32,
) {
    let Some(f) = glyphs.frame(frame) else { return };
    let x = x - glyphs.shift(frame);
    let table = colors.table(mode);
    for layer in &f.layers {
        let mut pixels = layer.pixels.iter();
        for run in &layer.runs {
            let py = y + i32::from(layer.y) + i32::from(run.y);
            for i in 0..run.len {
                let px = x + i32::from(layer.x) + i32::from(run.x) + i32::from(i);
                let v = pixels.next().copied().unwrap_or(0);
                let color = match layer.encoding {
                    Encoding::Shade4 | Encoding::Shade8 => table.map(|t| t.level(i32::from(v) - fade)),
                    other => pixel_color(other, v, &colors.palette, 0),
                };
                if let Some((rgb, alpha)) = color {
                    canvas.blend(px, py, rgb, alpha);
                }
            }
        }
    }
}

fn draw_rlc(canvas: &mut Canvas, item: &RlcItem, x: i32, palette: &Palette) {
    for run in &item.runs {
        for (i, &index) in run.pixels.iter().enumerate() {
            canvas.blend(x + i32::from(run.x) + i as i32, i32::from(run.y), palette.rgb(index), 255);
        }
    }
}

/// Premultiplied-alpha accumulator that grows to fit everything drawn.
#[derive(Default)]
struct Canvas {
    /// Pixel position -> premultiplied (r, g, b, a) in 0..=1.
    pixels: std::collections::BTreeMap<(i32, i32), [f32; 4]>,
}

impl Canvas {
    fn blend(&mut self, x: i32, y: i32, rgb: [u8; 3], alpha: u8) {
        if alpha == 0 {
            return;
        }
        let a = f32::from(alpha) / 255.0;
        let p = self.pixels.entry((y, x)).or_insert([0.0; 4]);
        for k in 0..3 {
            p[k] = f32::from(rgb[k]) / 255.0 * a + p[k] * (1.0 - a);
        }
        p[3] = a + p[3] * (1.0 - a);
    }

    fn finish(self) -> (Image, i32, i32) {
        let Some((x0, y0, x1, y1)) = self.pixels.keys().fold(None, |b: Option<(i32, i32, i32, i32)>, &(y, x)| {
            Some(b.map_or((x, y, x, y), |(a, b, c, d)| (a.min(x), b.min(y), c.max(x), d.max(y))))
        }) else {
            return (Image::new(0, 0), 0, 0);
        };
        let mut image = Image::new((x1 - x0 + 1) as u32, (y1 - y0 + 1) as u32);
        for ((y, x), p) in self.pixels {
            let i = (((y - y0) as u32 * image.width + (x - x0) as u32) * 4) as usize;
            let straight = |c: f32| if p[3] > 0.0 { (c / p[3] * 255.0).round().clamp(0.0, 255.0) as u8 } else { 0 };
            image.rgba[i..i + 4].copy_from_slice(&[
                straight(p[0]),
                straight(p[1]),
                straight(p[2]),
                (p[3] * 255.0).round() as u8,
            ]);
        }
        (image, x0, y0)
    }
}

/// One of the original's global fonts (set up at `0x4A54EB`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NamedFont {
    /// The exported variable name, e.g. `fonG60w`.
    pub name: &'static str,
    /// Sprite pack name as passed to `PreLoadGPImage`; the files are `<gp>.GP` / `<gp>.GPI`.
    pub gp: &'static str,
    /// Colour mode (`SetColorTable`).
    pub color: u8,
    /// Shadow pack and offset (`SetShadowGP`).
    pub shadow: Option<(&'static str, i32, i32)>,
}

impl NamedFont {
    pub fn gp_path(&self) -> String {
        format!("{}.GP", self.gp.to_ascii_uppercase())
    }

    pub fn gpi_path(&self) -> String {
        format!("{}.GPI", self.gp.to_ascii_uppercase())
    }

    pub fn shadow_paths(&self) -> Option<(String, String)> {
        self.shadow.map(|(s, ..)| (format!("{}.GP", s.to_ascii_uppercase()), format!("{}.GPI", s.to_ascii_uppercase())))
    }

    /// Builds the font from its files: `read` maps a game path (see [`Self::gp_path`]) to
    /// its bytes; a missing `.GPI` is fine.
    pub fn load(&self, mut read: impl FnMut(&str) -> Option<Vec<u8>>) -> Result<Font, GpError> {
        let mut glyphs = |gp: String, gpi: String| {
            let data = read(&gp).ok_or(GpError { offset: 0, reason: "font sprite pack not found" })?;
            GpGlyphs::new(&GpFile::parse(data)?, read(&gpi).as_deref())
        };
        let mut font = Font::new(Glyphs::Gp(glyphs(self.gp_path(), self.gpi_path())?)).with_color(self.color);
        if let (Some((gp, gpi)), Some((_, dx, dy))) = (self.shadow_paths(), self.shadow) {
            font = font.with_shadow(glyphs(gp, gpi)?, dx, dy);
        }
        Ok(font)
    }
}

const fn font(name: &'static str, gp: &'static str, color: u8, shadow: Option<(&'static str, i32, i32)>) -> NamedFont {
    NamedFont { name, gp, color, shadow }
}

/// The global fonts of `dmcr.exe` in the order of their set-up at `0x4A54EB`, then a few
/// local fonts of screens.
pub const FONTS: &[NamedFont] = &[
    font("fonG60y5", "FontG60", 6, None),
    font("fonG60w", "FontG60", 2, Some(("FontG60s", 0, 0))),
    font("fon40y1", "FontM40", 3, Some(("FontM40s", 0, 0))),
    font("fon40y5", "FontM40", 6, Some(("FontM40s", 0, 0))),
    font("fon30y1", "FontM30", 2, Some(("FontM30s", 0, 0))),
    font("fon30y3", "FontM30", 3, Some(("FontM30s", 0, 0))),
    font("fon30y5", "FontM30", 5, None),
    font("fon18w", "FontM18", 1, Some(("FontM18s", 1, 1))),
    font("fon18y3", "FontM18", 3, Some(("FontM18s", 1, 1))),
    font("fon18y5", "FontM18", 6, None),
    font("fon16y1", "FontM16", 1, Some(("FontM16s", 1, 1))),
    font("fon16y3", "FontM16", 3, Some(("FontM16s", 1, 1))),
    font("fon16y5", "FontM16", 6, None),
    font("fon13y2", "FontM13", 3, Some(("FontM13s", 0, 0))),
    font("BigWhiteFont", "FontM40", 1, None),
    font("BigYellowFont", "FontM40", 3, None),
    font("BigRedFont", "FontM40", 0, None),
    font("BigBlackFont", "FontM40", 1, Some(("FontM40s", 0, 0))),
    font("WhiteFont", "FontM18", 1, Some(("FontM18s", 1, 1))),
    font("YellowFont", "FontM18", 3, Some(("FontM18s", 1, 1))),
    font("RedFont", "FontM18", 2, Some(("FontM18s", 1, 1))),
    font("BlackFont", "FontM18", 6, None),
    font("SpecialWhiteFont", "FontM16", 1, Some(("FontM16s", 1, 1))),
    font("SpecialYellowFont", "FontM16", 3, Some(("FontM16s", 1, 1))),
    font("SpecialRedFont", "FontM16", 2, Some(("FontM16s", 1, 1))),
    font("SpecialBlackFont", "FontM16", 6, None),
    font("SmallWhiteFont", "FontM13", 1, None),
    font("SmallRedFont", "FontM13", 2, None),
    font("SmallYellowFont", "FontM13", 3, None),
    font("SmallBlackFont", "FontM13", 6, None),
    font("SmallWhiteFont1", "FontM13", 1, None),
    font("SmallRedFont1", "FontM13", 2, None),
    font("SmallBlackFont1", "FontM13", 6, None),
    font("SmallYellowFont1", "FontM13", 3, None),
    font("fn8", "FontM12", 1, None),
    font("fn10", "FontM12", 1, None),
    // Local fonts of screens, named here after their pack and colour: `AskMultilineQuestion`
    // (0x5ad540) builds `RLCFont(rom11)` objects, one with `SetRedColor`.
    font("rom11", "rom11", 0, None),
    font("rom11red", "rom11", 2, None),
];

/// The first global font built from the same pack, colour mode and shadow (intExplorer's
/// `#nfont` creates its own `RLCFont`s, which match global ones).
pub fn font_like(gp: &str, color: u8, shadow: Option<(&str, i32, i32)>) -> Option<&'static NamedFont> {
    FONTS.iter().find(|f| {
        f.gp.eq_ignore_ascii_case(gp)
            && f.color == color
            && match (f.shadow, shadow) {
                (None, None) => true,
                (Some((a, x, y)), Some((b, u, v))) => a.eq_ignore_ascii_case(b) && x == u && y == v,
                _ => false,
            }
    })
}

/// Looks up a global font by its variable name (case-insensitive).
pub fn named_font(name: &str) -> Option<&'static NamedFont> {
    FONTS.iter().find(|f| f.name.eq_ignore_ascii_case(name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gp::{Layer, Run};

    fn shade_frame(x: i16, width: u16, level: u8) -> Frame {
        let runs = vec![Run { x: 0, y: 0, len: width }];
        let pixels = vec![level; usize::from(width)];
        Frame { layers: vec![Layer { encoding: Encoding::Shade8, x, y: 1, width, height: 1, runs, pixels }] }
    }

    /// Frame `i` is `i % 4 + 1` pixels wide.
    fn glyphs(gpi: bool) -> GpGlyphs {
        let frames: Vec<Frame> = (0..128).map(|i| shade_frame(0, i % 4 + 1, 31)).collect();
        let lx = frames.iter().map(|f| i32::from(f.layers[0].width)).collect();
        let ly = vec![2; frames.len()];
        let (shift, advance) = if gpi { (Some(vec![1; 128]), Some(vec![3; 128])) } else { (None, None) };
        GpGlyphs { frames, lx, ly, shift, advance }
    }

    fn colors() -> FontColors {
        let mut palette = Palette([[0; 3]; 256]);
        palette.0[1] = [255, 255, 255];
        palette.0[2] = [128, 128, 128];
        let mut colors = FontColors::new(palette);
        // Row r: index 1 (white) at r >= 16, index 2 (grey) for 0 < r < 16, identity at 0.
        let grd: Vec<u8> = (0..32 * 256)
            .map(|i| match i / 256 {
                0 => (i % 256) as u8,
                r if r >= 16 => 1,
                _ => 2,
            })
            .collect();
        assert!(colors.set_table(2, &grd));
        colors
    }

    #[test]
    fn widths() {
        let font = Font::new(Glyphs::Gp(glyphs(false)));
        // 'A' = 65 -> 2 px, space uses 'c' = 99 -> 4 px.
        assert_eq!(font.char_width(b'A'), 2);
        assert_eq!(font.char_width(b' '), 4);
        assert_eq!(font.text_width(b"A A\0AAAA"), 8);
        assert_eq!(font.text_width(b"A\x7F0041A"), 4);
        assert_eq!(font.height(), 2);
        let font = Font::new(Glyphs::Gp(glyphs(true)));
        assert_eq!(font.text_width(b"AB"), 6);
        assert_eq!(font.char_width(200), 0);
    }

    #[test]
    fn color_table_fit() {
        let colors = colors();
        let table = colors.table(2).unwrap();
        assert_eq!(table.level(31), ([255; 3], 255));
        assert_eq!(table.level(0).1, 0);
        assert_eq!(table.remap(0, 7), 7);
        assert_eq!(table.remap(20, 7), 1);
        // Mode 7 and up fall back to mode 0, which is not loaded here.
        assert!(colors.table(9).is_none());
    }

    #[test]
    fn draws_with_shift_and_shadow() {
        let colors = colors();
        let mut shadow = glyphs(false);
        shadow.frames = (0..128).map(|_| shade_frame(0, 1, 31)).collect();
        let font = Font::new(Glyphs::Gp(glyphs(true))).with_color(2).with_shadow(shadow, 1, 1);
        let text = font.draw(b"A", &colors);
        assert_eq!(text.advance, 3);
        // Glyph: 2 px at x = -1 (shift 1), y = 1. Shadow (mode 0 not loaded) draws nothing.
        assert_eq!((text.left, text.top, text.image.width, text.image.height), (-1, 1, 2, 1));
        assert_eq!(&text.image.rgba[..4], &[255, 255, 255, 255]);
        let faded = font.draw_faded(b"A", &colors, 40);
        assert_eq!(faded.image.width, 0);
    }

    #[test]
    fn named_fonts() {
        let f = named_font("FONG60W").unwrap();
        assert_eq!((f.gp_path(), f.color), ("FONTG60.GP".to_owned(), 2));
        assert_eq!(f.shadow_paths().unwrap().1, "FONTG60S.GPI");
        assert!(named_font("nope").is_none());
    }
}
