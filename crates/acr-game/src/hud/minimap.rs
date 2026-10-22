//! The minimap in the lower right corner (built by 0x44c230, drawn by 0x4c0a40).
//!
//! One minimap pixel covers `1 << s` map cells (64 × 64 world pixels on normal maps), so
//! the map is always 240 × 240 pixels. In the large mode (the default, key M toggles) the
//! whole map is shown; the small mode shows at most 120 × 120 pixels around the camera.

use acr_formats::{M3dMap, m3d::SpriteGroup};
use acr_render::terrain::{GroundAssets, vertex_light};

use crate::ui::draw::Rect;

/// Side of the whole-map image: `240 << s` cells, one pixel per `1 << s` cells.
pub const SIDE: i32 = 240;
/// Side of the fog mask: one entry per 2 × 2 block of minimap pixels.
pub const FOG_SIDE: i32 = SIDE / 2;
/// Largest shown side in the small and the large mode (0x4c0a40).
const SMALL: i32 = 120;
const LARGE: i32 = 480;

/// Palette indices of the player colours on the minimap (table 0x644738).
pub const PLAYER_COLORS: [u8; 8] = [208, 212, 216, 220, 224, 228, 232, 173];
/// Units selected by the local player and the camera rectangle.
pub const WHITE: u8 = 0xff;
/// Map objects other than trees (0x44c59c).
const OBJECT: u8 = 0x4e;

/// The terrain image of a map (palette indices, `SIDE × SIDE`).
pub struct Minimap {
    pub pixels: Vec<u8>,
}

impl Minimap {
    /// The terrain colours (texture and light per sampled vertex), water by depth and
    /// brightness, then non-tree map objects as green dots.
    pub fn new(map: &M3dMap, assets: &GroundAssets) -> Option<Self> {
        let shift = u32::from(map.size?.shift);
        let heights = map.heights.as_ref()?;
        let textures = map.textures.as_ref()?;
        let side = SIDE as usize;
        let mut pixels = vec![0u8; side * side];
        let stride = heights.width;
        for y in 0..side {
            for x in 0..side {
                let v = (y << shift) * stride + (x << shift);
                if v >= heights.cells.len() || v >= textures.cells.len() {
                    continue;
                }
                let light = vertex_light(&heights.cells, stride, v);
                pixels[y * side + x] = assets.minimap_color(textures.cells[v], light);
            }
        }
        if let Some(water) = &map.water {
            let cells = (SIDE as usize) << shift;
            for y in (0..cells.min(water.depth.height)).step_by(2) {
                for x in (0..cells.min(water.depth.width)).step_by(2) {
                    let i = y * water.depth.width + x;
                    if water.depth.cells.get(i).is_none_or(|&d| d <= 0x80) {
                        continue;
                    }
                    let b = water.brightness.cells.get(i).copied().unwrap_or(0);
                    let color = match b {
                        0x59.. => 0xb7,
                        0x49..=0x58 => 0xb6,
                        0x29..=0x48 => 0xb5,
                        0x09..=0x28 => 0xb4,
                        _ => 0xb3,
                    };
                    let (px, py) = (x >> shift, y >> shift);
                    if px < side && py < side {
                        pixels[py * side + px] = color;
                    }
                }
            }
        }
        for s in map.sprites.iter().filter(|s| !matches!(s.group, SpriteGroup::Trees)) {
            let (px, py) = (s.x >> (shift + 5), s.y >> (shift + 5));
            if (0..SIDE).contains(&px) && (0..SIDE).contains(&py) {
                pixels[py as usize * side + px as usize] = OBJECT;
            }
        }
        Some(Self { pixels })
    }
}

/// Where the minimap is and which part of the map it shows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    /// Shown part of the minimap on the canvas.
    pub rect: Rect,
    /// First shown minimap pixel (`offx`, `offy`, 0xff300c/0xff3014).
    pub offset: [i32; 2],
}

impl Placement {
    /// `mmx = RealLx − w − 17`, `mmy = RealLy − h − 16` (0x4c0bec); in the small mode the
    /// shown window follows the camera (`camera` and `view` in minimap pixels).
    pub fn new(large: bool, screen: [i32; 2], camera: [f32; 2], view: [f32; 2]) -> Self {
        let max = if large { LARGE } else { SMALL };
        let (w, h) = (SIDE.min(max), SIDE.min(max));
        let rect = Rect::new(screen[0] - w - 17, screen[1] - h - 16, w, h);
        // offx = (camX + viewW/2 − w) >> s with camX, viewW in cells (0x4c0b3e).
        let off = |cam: f32, view: f32, size: i32| {
            let o = (cam + view / 2.0 - size as f32 / 2.0).floor() as i32;
            if o + size >= SIDE { SIDE - size - 1 } else { o.max(0) }
        };
        let offset = if w < SIDE { [off(camera[0], view[0], w), off(camera[1], view[1], h)] } else { [0, 0] };
        Self { rect, offset }
    }

    /// The minimap pixel under a canvas point, if the point is on the minimap.
    pub fn pixel_at(&self, [x, y]: [f32; 2]) -> Option<[f32; 2]> {
        // The engine tests strictly inside the rectangle (0x4c5c9a).
        let r = self.rect;
        if x <= r.x0 as f32 || y <= r.y0 as f32 || x >= r.x1 as f32 || y >= r.y1 as f32 {
            return None;
        }
        Some([x - r.x0 as f32 + self.offset[0] as f32, y - r.y0 as f32 + self.offset[1] as f32])
    }
}

/// A unit dot: minimap pixel and palette colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dot {
    pub x: i32,
    pub y: i32,
    pub color: u8,
}

/// The shown part of the minimap with the unit dots (2 × 2) and the camera rectangle, as
/// RGBA pixels of `placement.rect`'s size.
pub fn compose(
    map: &Minimap,
    placement: &Placement,
    dots: &[Dot],
    fog: Option<&[bool]>,
    camera: Option<Rect>,
    palette: &acr_formats::Palette,
) -> acr_formats::raster::Image {
    let (w, h) = (placement.rect.width(), placement.rect.height());
    let mut idx = vec![0u8; (w * h) as usize];
    let [ox, oy] = placement.offset;
    for y in 0..h {
        for x in 0..w {
            let (sx, sy) = (x + ox, y + oy);
            if (0..SIDE).contains(&sx) && (0..SIDE).contains(&sy) {
                idx[(y * w + x) as usize] = map.pixels[(sy * SIDE + sx) as usize];
            }
        }
    }
    let put = |idx: &mut [u8], x: i32, y: i32, c: u8| {
        if (0..w).contains(&x) && (0..h).contains(&y) {
            idx[(y * w + x) as usize] = c;
        }
    };
    for d in dots {
        let (x, y) = (d.x - ox, d.y - oy);
        for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            put(&mut idx, x + dx, y + dy, d.color);
        }
    }
    // Fog (0x485c24, after the dots, so it hides them): 2 × 2 blocks in palette colour 0.
    if let Some(fog) = fog {
        for y in 0..h {
            for x in 0..w {
                let (bx, by) = ((x + ox) >> 1, (y + oy) >> 1);
                if (0..FOG_SIDE).contains(&bx) && (0..FOG_SIDE).contains(&by) && fog[(by * FOG_SIDE + bx) as usize] {
                    idx[(y * w + x) as usize] = 0;
                }
            }
        }
    }
    if let Some(r) = camera {
        let (x0, y0, x1, y1) = (r.x0 - ox, r.y0 - oy, r.x1 - 1 - ox, r.y1 - 1 - oy);
        for x in x0..=x1 {
            put(&mut idx, x, y0, WHITE);
            put(&mut idx, x, y1, WHITE);
        }
        for y in y0..=y1 {
            put(&mut idx, x0, y, WHITE);
            put(&mut idx, x1, y, WHITE);
        }
    }
    let mut rgba = Vec::with_capacity(idx.len() * 4);
    for &i in &idx {
        let [r, g, b] = palette.rgb(i);
        rgba.extend([r, g, b, 255]);
    }
    acr_formats::raster::Image { width: w as u32, height: h as u32, rgba }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placement_at_1024x768() {
        let p = Placement::new(true, [1024, 768], [0.0, 0.0], [16.0, 24.0]);
        assert_eq!(p.rect, Rect::new(767, 512, 240, 240));
        assert_eq!(p.offset, [0, 0]);
        assert_eq!(p.pixel_at([768.0, 513.0]), Some([1.0, 1.0]));
        assert_eq!(p.pixel_at([767.0, 600.0]), None);
        let small = Placement::new(false, [1024, 768], [100.0, 10.0], [16.0, 24.0]);
        assert_eq!(small.rect, Rect::new(887, 632, 120, 120));
        assert_eq!(small.offset, [48, 0]);
        let small = Placement::new(false, [1024, 768], [200.0, 200.0], [16.0, 24.0]);
        assert_eq!(small.offset, [119, 119]);
    }
}
