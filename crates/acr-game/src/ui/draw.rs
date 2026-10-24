//! What the menus draw, independent of the GPU: sprites placed on the 1024x768 canvas.
//!
//! Widgets emit [`Draw`] commands; [`Textures`] turns their sprites into GPU textures
//! (rasterized on the CPU once and cached) and the commands into [`ImageDraw`]s.

use std::collections::HashMap;

use acr_formats::raster::{Image, RasterOptions, draw_frame};
use acr_render::{ImageDraw, ImageRenderer, ImageTexture};

use super::fonts::{FontId, Fonts};
use super::library::Library;
use crate::canvas::Canvas;

/// Palette of every menu screen: they all call `LoadPalette("1\agew_1.pal")` and their
/// bitmaps carry the same colours.
pub const MENU_PALETTE: &str = "1\\AGEW_1.PAL";
/// Palette of the game screen; the in-game interface draws its texts and lines with it.
pub const GAME_PALETTE: &str = "0\\AGEW_1.PAL";

/// A rectangle on the canvas with exclusive right/bottom edges.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rect {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
}

impl Rect {
    /// The menu canvas (1024 × 768).
    pub const SCREEN: Rect = Rect { x0: 0, y0: 0, x1: 1024, y1: 768 };
    /// No bounds: draws are clipped by their canvas ([`Canvas::clip`]) only.
    pub const ALL: Rect = Rect { x0: i32::MIN / 2, y0: i32::MIN / 2, x1: i32::MAX / 2, y1: i32::MAX / 2 };

    pub fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x0: x, y0: y, x1: x + w, y1: y + h }
    }

    /// From the original's inclusive corners (`x`, `y`, `x1`, `y1` of `SimpleDialog`).
    pub fn inclusive(x: i32, y: i32, x1: i32, y1: i32) -> Self {
        Self { x0: x, y0: y, x1: x1 + 1, y1: y1 + 1 }
    }

    pub fn contains(&self, [x, y]: [f32; 2]) -> bool {
        x >= self.x0 as f32 && x < self.x1 as f32 && y >= self.y0 as f32 && y < self.y1 as f32
    }

    pub fn intersect(&self, o: &Rect) -> Rect {
        Rect { x0: self.x0.max(o.x0), y0: self.y0.max(o.y0), x1: self.x1.min(o.x1), y1: self.y1.min(o.y1) }
    }

    pub fn is_empty(&self) -> bool {
        self.x1 <= self.x0 || self.y1 <= self.y0
    }

    pub fn width(&self) -> i32 {
        self.x1 - self.x0
    }

    pub fn height(&self) -> i32 {
        self.y1 - self.y0
    }

    pub fn offset(&self, dx: i32, dy: i32) -> Rect {
        Rect { x0: self.x0 + dx, y0: self.y0 + dy, x1: self.x1 + dx, y1: self.y1 + dy }
    }
}

/// Something that can be turned into a texture.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Sprite {
    /// A whole bitmap file.
    Bitmap(String),
    /// One frame of a sprite pack, drawn with the menu palette; its origin is the draw position.
    Frame { gp: String, frame: usize },
    /// A string in one of the original fonts; the draw position is the top-left of the text.
    Text { font: FontId, text: Vec<u8> },
    /// A solid colour, stretched over `Draw::size`.
    Fill([u8; 4]),
    /// A frame of an in-game sprite pack, drawn with the palette the pack was made for
    /// ([`acr_formats::sprite_palette`]) in the colours of `player`.
    GameFrame { gp: String, frame: usize, player: u8 },
    /// A [`Sprite::GameFrame`] whose colours go through row `row` of a 256-byte colour table
    /// first (`ShowGPPal`): gray icons (`0\gray.set`), the building preview (`0\agew_1w.grd`,
    /// `0\agew_1r.grd`).
    GameFrameRemap { gp: String, frame: usize, player: u8, table: &'static str, row: u8 },
    /// A string with the colours of the in-game palette.
    GameText { font: FontId, text: Vec<u8> },
    /// A [`Sprite::GameText`] faded like the original's `0x7C3428` (0 = full, 31 = gone).
    GameTextFaded { font: FontId, text: Vec<u8>, fade: u8 },
    /// An image set with [`Textures::set_image`].
    Image(&'static str),
    /// A line from the draw position to the position plus `Draw::size` (canvas pixels, both
    /// ends drawn), one window pixel wide at any canvas scale and `drop` window pixels lower
    /// (a dark edge under a mark): crisp when the map is zoomed ([`Draw::line`]).
    Line { color: [u8; 4], drop: u8 },
}

/// One sprite placed on the canvas.
#[derive(Debug, Clone)]
pub struct Draw {
    pub sprite: Sprite,
    pub x: i32,
    pub y: i32,
    /// Only [`Sprite::Fill`] uses a size; others are drawn at their natural size.
    pub size: [i32; 2],
    pub clip: Rect,
    /// Opacity, 1 for opaque.
    pub alpha: f32,
}

impl Draw {
    pub fn bitmap(path: &str, x: i32, y: i32) -> Self {
        Self { sprite: Sprite::Bitmap(path.to_owned()), x, y, size: [0; 2], clip: Rect::ALL, alpha: 1.0 }
    }

    pub fn frame(gp: &str, frame: usize, x: i32, y: i32) -> Self {
        Self { sprite: Sprite::Frame { gp: gp.to_owned(), frame }, x, y, size: [0; 2], clip: Rect::ALL, alpha: 1.0 }
    }

    pub fn text(font: FontId, text: &[u8], x: i32, y: i32) -> Self {
        Self { sprite: Sprite::Text { font, text: text.to_vec() }, x, y, size: [0; 2], clip: Rect::ALL, alpha: 1.0 }
    }

    pub fn game_frame(gp: &str, frame: usize, x: i32, y: i32, player: u8) -> Self {
        let sprite = Sprite::GameFrame { gp: gp.to_owned(), frame, player };
        Self { sprite, x, y, size: [0; 2], clip: Rect::ALL, alpha: 1.0 }
    }

    /// [`Draw::game_frame`] with the colours remapped through row `row` of `table`.
    pub fn game_frame_remap(gp: &str, frame: usize, x: i32, y: i32, player: u8, table: &'static str, row: u8) -> Self {
        let sprite = Sprite::GameFrameRemap { gp: gp.to_owned(), frame, player, table, row };
        Self { sprite, x, y, size: [0; 2], clip: Rect::ALL, alpha: 1.0 }
    }

    pub fn game_text(font: FontId, text: &[u8], x: i32, y: i32) -> Self {
        Self { sprite: Sprite::GameText { font, text: text.to_vec() }, x, y, size: [0; 2], clip: Rect::ALL, alpha: 1.0 }
    }

    /// [`Draw::game_text`] faded by `fade` rows of the font's colour table.
    pub fn game_text_faded(font: FontId, text: &[u8], x: i32, y: i32, fade: u8) -> Self {
        let sprite = Sprite::GameTextFaded { font, text: text.to_vec(), fade };
        Self { sprite, x, y, size: [0; 2], clip: Rect::ALL, alpha: 1.0 }
    }

    /// An image set with [`Textures::set_image`], stretched over `rect`.
    pub fn image(name: &'static str, rect: Rect) -> Self {
        Self {
            sprite: Sprite::Image(name),
            x: rect.x0,
            y: rect.y0,
            size: [rect.width(), rect.height()],
            clip: Rect::ALL,
            alpha: 1.0,
        }
    }

    pub fn fill(color: [u8; 4], rect: Rect) -> Self {
        Self {
            sprite: Sprite::Fill(color),
            x: rect.x0,
            y: rect.y0,
            size: [rect.width(), rect.height()],
            clip: Rect::ALL,
            alpha: 1.0,
        }
    }

    /// A line from `a` to `b` (canvas pixels, both ends drawn), one window pixel wide at any
    /// scale ([`Sprite::Line`]).
    pub fn line(color: [u8; 4], a: (i32, i32), b: (i32, i32)) -> Self {
        Self {
            sprite: Sprite::Line { color, drop: 0 },
            x: a.0,
            y: a.1,
            size: [b.0 - a.0, b.1 - a.1],
            clip: Rect::ALL,
            alpha: 1.0,
        }
    }

    /// The same line `drop` window pixels lower.
    pub fn dropped(mut self, drop: u8) -> Self {
        if let Sprite::Line { drop: d, .. } = &mut self.sprite {
            *d = drop;
        }
        self
    }

    /// The same draw with the colours of the game screen (dialogs over the game).
    pub fn in_game(mut self) -> Self {
        self.sprite = match self.sprite {
            Sprite::Frame { gp, frame } => Sprite::GameFrame { gp, frame, player: 0 },
            Sprite::Text { font, text } => Sprite::GameText { font, text },
            other => other,
        };
        self
    }

    /// Without a clip of its own (only its canvas clips it).
    pub fn unclipped(mut self) -> Self {
        self.clip = Rect::ALL;
        self
    }

    pub fn with_alpha(mut self, alpha: f32) -> Self {
        self.alpha = alpha;
        self
    }

    pub fn clipped(mut self, clip: Rect) -> Self {
        self.clip = self.clip.intersect(&clip);
        self
    }

    /// The same draw moved by `dx`, `dy` with its clip.
    pub fn translated(mut self, dx: i32, dy: i32) -> Self {
        self.x += dx;
        self.y += dy;
        if self.clip != Rect::ALL {
            self.clip =
                Rect { x0: self.clip.x0 + dx, y0: self.clip.y0 + dy, x1: self.clip.x1 + dx, y1: self.clip.y1 + dy };
        }
        self
    }
}

/// A rasterized sprite: its texture and where its top-left pixel lies relative to the
/// draw position.
struct Raster {
    texture: ImageTexture,
    offset: [i32; 2],
}

/// GPU textures of rasterized sprites.
#[derive(Default)]
pub struct Textures {
    cache: HashMap<Sprite, Option<Raster>>,
    /// Images set by [`Textures::set_image`] and not uploaded yet.
    pending: HashMap<&'static str, Image>,
}

impl Textures {
    /// Forgets texts and bitmaps of the previous screen; sprite frames are kept.
    pub fn trim(&mut self) {
        self.cache.retain(|k, _| {
            matches!(
                k,
                Sprite::Frame { .. }
                    | Sprite::Fill(_)
                    | Sprite::Line { .. }
                    | Sprite::GameFrame { .. }
                    | Sprite::GameFrameRemap { .. }
                    | Sprite::Image(_)
            )
        });
    }

    /// Replaces the image drawn by [`Sprite::Image`]`(name)`; it is uploaded with the next draws.
    pub fn set_image(&mut self, name: &'static str, image: Image) {
        self.pending.insert(name, image);
    }

    /// Converts `draws` to window-space image draws, rasterizing sprites as needed.
    /// Sprites whose files are not loaded yet are skipped.
    #[allow(clippy::too_many_arguments)]
    pub fn resolve(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        images: &ImageRenderer,
        lib: &Library,
        fonts: &Fonts,
        canvas: &Canvas,
        draws: &[Draw],
        out: &mut Vec<ImageDraw>,
    ) {
        for (name, image) in self.pending.drain() {
            let texture = images.upload(device, queue, image.width, image.height, &image.rgba);
            self.cache.insert(Sprite::Image(name), Some(Raster { texture, offset: [0, 0] }));
        }
        for d in draws {
            if !self.cache.contains_key(&d.sprite) {
                let Some(raster) = rasterize(lib, fonts, &d.sprite) else { continue };
                let raster = raster.map(|(image, offset)| Raster {
                    texture: images.upload(device, queue, image.width, image.height, &image.rgba),
                    offset,
                });
                self.cache.insert(d.sprite.clone(), raster);
            }
            let Some(Some(raster)) = self.cache.get(&d.sprite) else { continue };
            if let Sprite::Line { drop, .. } = d.sprite {
                line_runs(canvas, d, f32::from(drop), |dst| {
                    out.push(ImageDraw {
                        texture: raster.texture.clone(),
                        dst,
                        src: [0.0, 0.0, 1.0, 1.0],
                        color: [1.0, 1.0, 1.0, d.alpha],
                    })
                });
                continue;
            }
            let (w, h) = (raster.texture.width as i32, raster.texture.height as i32);
            let size = if matches!(d.sprite, Sprite::Fill(_) | Sprite::Image(_)) { d.size } else { [w, h] };
            let dst = Rect::new(d.x + raster.offset[0], d.y + raster.offset[1], size[0], size[1]);
            let visible = dst.intersect(&d.clip).intersect(&canvas.clip);
            if visible.is_empty() {
                continue;
            }
            // Source pixels of the visible part.
            let (sx, sy) = (w as f32 / size[0] as f32, h as f32 / size[1] as f32);
            let src = [
                (visible.x0 - dst.x0) as f32 * sx,
                (visible.y0 - dst.y0) as f32 * sy,
                visible.width() as f32 * sx,
                visible.height() as f32 * sy,
            ];
            let window = canvas.to_window([
                visible.x0 as f32,
                visible.y0 as f32,
                visible.width() as f32,
                visible.height() as f32,
            ]);
            out.push(ImageDraw { texture: raster.texture.clone(), dst: window, src, color: [1.0, 1.0, 1.0, d.alpha] });
        }
    }
}

/// The window rectangles (`[x, y, w, h]`) of line `d` on `canvas`: its ends at the centres of
/// their canvas pixels, stepped in whole window pixels like `DrawLine` (0x4012ff) steps in
/// screen pixels, consecutive pixels of a row (column) joined, `drop` pixels lower. At scale 1
/// these are the pixels of the original's line; zoomed in the line stays one pixel wide.
fn line_runs(canvas: &Canvas, d: &Draw, drop: f32, mut run: impl FnMut([f32; 4])) {
    let clip = d.clip.intersect(&canvas.clip);
    if clip.is_empty() {
        return;
    }
    let window = |x: i32, y: i32| {
        let [wx, wy, ..] = canvas.to_window([x as f32 + 0.5, y as f32 + 0.5, 0.0, 0.0]);
        (wx.floor() as i64, wy.floor() as i64)
    };
    // The clip in window pixels (pixels whose centre lies inside).
    let [cx, cy, cw, ch] =
        canvas.to_window([clip.x0 as f32, clip.y0 as f32, clip.width() as f32, clip.height() as f32]);
    let (cx0, cy0) = ((cx - 0.5).ceil() as i64, (cy - 0.5).ceil() as i64);
    let (cx1, cy1) = ((cx + cw - 0.5).ceil() as i64, (cy + ch - 0.5).ceil() as i64);
    let (x0, y0) = window(d.x, d.y);
    let (x1, y1) = window(d.x + d.size[0], d.y + d.size[1]);
    if x0.max(x1) < cx0 || x0.min(x1) >= cx1 || y0.max(y1) < cy0 || y0.min(y1) >= cy1 {
        return;
    }
    let (dx, dy) = (x1 - x0, y1 - y0);
    let n = dx.abs().max(dy.abs());
    let at = |k: i64| {
        if n == 0 {
            return (x0, y0);
        }
        let round = |d: i64| (2 * d * k + n * d.signum()) / (2 * n);
        (x0 + round(dx), y0 + round(dy))
    };
    let horizontal = dx.abs() >= dy.abs();
    let mut emit = |a: (i64, i64), b: (i64, i64)| {
        let (rx0, ry0) = (a.0.min(b.0).max(cx0), a.1.min(b.1).max(cy0));
        let (rx1, ry1) = ((a.0.max(b.0) + 1).min(cx1), (a.1.max(b.1) + 1).min(cy1));
        if rx0 < rx1 && ry0 < ry1 {
            run([rx0 as f32, ry0 as f32 + drop, (rx1 - rx0) as f32, (ry1 - ry0) as f32]);
        }
    };
    let mut start = at(0);
    let mut last = start;
    for k in 1..=n {
        let p = at(k);
        if if horizontal { p.1 != start.1 } else { p.0 != start.0 } {
            emit(start, last);
            start = p;
        }
        last = p;
    }
    emit(start, last);
}

/// `None` while inputs are still loading, `Some(None)` if the sprite cannot be drawn.
type Rasterized = Option<Option<(Image, [i32; 2])>>;

pub(crate) fn rasterize(lib: &Library, fonts: &Fonts, sprite: &Sprite) -> Rasterized {
    match sprite {
        Sprite::Bitmap(path) => {
            if !lib.settled(path) {
                return None;
            }
            Some(lib.bitmap(path).map(|b| (Image { width: b.width, height: b.height, rgba: b.to_rgba() }, [0, 0])))
        }
        Sprite::Frame { gp, frame } => {
            if !lib.settled(gp) || !lib.settled(MENU_PALETTE) {
                return None;
            }
            let (Some(file), Some(palette)) = (lib.sprites(gp), lib.palette(MENU_PALETTE)) else { return Some(None) };
            Some(rasterize_frame(&file, *frame, &palette))
        }
        Sprite::Text { font, text } => fonts.rasterize(lib, *font, text),
        Sprite::Fill(color) | Sprite::Line { color, .. } => {
            Some(Some((Image { width: 1, height: 1, rgba: color.to_vec() }, [0, 0])))
        }
        Sprite::GameFrame { gp, frame, player } => {
            let palette_path = acr_formats::sprite_palette(gp);
            if !lib.settled(gp) || !lib.settled(palette_path) {
                return None;
            }
            let (Some(file), Some(palette)) = (lib.sprites(gp), lib.palette(palette_path)) else { return Some(None) };
            Some(rasterize_frame_as(&file, *frame, &palette, *player))
        }
        Sprite::GameFrameRemap { gp, frame, player, table, row } => {
            let palette_path = acr_formats::sprite_palette(gp);
            if !lib.settled(gp) || !lib.settled(palette_path) || !lib.settled(table) {
                return None;
            }
            let (Some(file), Some(palette), Some(table)) =
                (lib.sprites(gp), lib.palette(palette_path), lib.bytes(table))
            else {
                return Some(None);
            };
            let base = usize::from(*row) * 256;
            let remapped = acr_formats::Palette(std::array::from_fn(|i| {
                palette.rgb(table.get(base + i).copied().unwrap_or(i as u8))
            }));
            Some(rasterize_frame_as(&file, *frame, &remapped, *player))
        }
        Sprite::GameText { font, text } => fonts.rasterize_in(lib, *font, text, super::fonts::GAME_SET),
        Sprite::GameTextFaded { font, text, fade } => {
            fonts.rasterize_faded(lib, *font, text, super::fonts::GAME_SET, i32::from(*fade))
        }
        // Only uploaded images are drawn.
        Sprite::Image(_) => Some(None),
    }
}

/// A frame as an image covering its bounding box, plus the box's offset from the origin.
pub fn rasterize_frame(
    file: &acr_formats::GpFile,
    index: usize,
    palette: &acr_formats::Palette,
) -> Option<(Image, [i32; 2])> {
    rasterize_frame_as(file, index, palette, 0)
}

/// [`rasterize_frame`] in the colours of `player`.
pub fn rasterize_frame_as(
    file: &acr_formats::GpFile,
    index: usize,
    palette: &acr_formats::Palette,
    player: u8,
) -> Option<(Image, [i32; 2])> {
    if index >= file.frame_count() {
        return None;
    }
    let frame = file.decode_frame(index).ok()?;
    let (x0, y0, x1, y1) = frame.bounds()?;
    let mut image = Image::new((x1 - x0) as u32, (y1 - y0) as u32);
    draw_frame(&mut image, &frame, -x0, -y0, &RasterOptions { palette, player, only: None });
    Some((image, [x0, y0]))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The window pixels a line covers.
    fn pixels(canvas: &Canvas, d: &Draw) -> Vec<(i32, i32)> {
        let mut px = Vec::new();
        line_runs(canvas, d, 0.0, |[x, y, w, h]| {
            for i in 0..w as i32 {
                for j in 0..h as i32 {
                    px.push((x as i32 + i, y as i32 + j));
                }
            }
        });
        px.sort_unstable();
        px
    }

    #[test]
    fn lines_match_the_original_at_scale_1_and_stay_thin_zoomed() {
        // At scale 1 the pixels of `hud::line` (`DrawLine`), shifted by the origin.
        let canvas = Canvas::new(1.0, [-100.0, -50.0]);
        for (a, b) in [((0, 0), (12, -6)), ((100, 60), (88, 66)), ((5, 5), (5, 9)), ((3, 3), (3, 3))] {
            let mut fills = Vec::new();
            crate::hud::line(&mut fills, [255; 4], a, b);
            let mut want: Vec<(i32, i32)> = fills
                .iter()
                .flat_map(|f| {
                    (0..f.size[0]).flat_map(move |i| (0..f.size[1]).map(move |j| (f.x + i - 100, f.y + j - 50)))
                })
                .collect();
            want.sort_unstable();
            assert_eq!(pixels(&canvas, &Draw::line([255; 4], a, b)), want, "{a:?} {b:?}");
        }
        // Zoomed four times: four times as long, still one pixel per step.
        let canvas = Canvas::new(4.0, [0.0, 0.0]);
        let px = pixels(&canvas, &Draw::line([255; 4], (0, 0), (12, -6)));
        assert_eq!(px.len(), 49);
        assert_eq!((px[0], px[48]), ((2, 2), (50, -22)));
        // Off the canvas: nothing.
        let canvas = Canvas { clip: Rect::new(0, 0, 10, 10), ..Canvas::new(1.0, [0.0, 0.0]) };
        assert!(pixels(&canvas, &Draw::line([255; 4], (20, 20), (30, 25))).is_empty());
        assert_eq!(pixels(&canvas, &Draw::line([255; 4], (5, 5), (15, 5))).len(), 5);
    }

    #[test]
    fn rects() {
        let r = Rect::inclusive(10, 20, 19, 29);
        assert_eq!((r.width(), r.height()), (10, 10));
        assert!(r.contains([19.5, 29.5]));
        assert!(!r.contains([20.0, 25.0]));
        assert!(r.intersect(&Rect::new(30, 0, 5, 5)).is_empty());
        assert_eq!(r.offset(1, 2), Rect::inclusive(11, 22, 20, 31));
    }
}
