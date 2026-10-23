//! The mouse cursor as the system's (hardware) cursor (native): the game's cursor frames
//! (`CURSOR_00.GP`, the menus' `CURSOR_01.GP`, animated) are turned into window cursors,
//! so the pointer moves at the display's rate however slowly the game draws
//! (docs/rendering-3d.md, "Input latency"). `ACR_SOFT_CURSOR` draws it with the frame as
//! before; so does the test driver (its screenshots show the drawn cursor) and the browser.

use std::collections::HashMap;

use acr_formats::raster::Image;
use winit::event_loop::ActiveEventLoop;
use winit::window::{CustomCursor, Window};

use crate::ui::{Ui, draw::Sprite};

/// A cursor image: the sprite, its offset from the mouse and the window scale (bits).
type Key = (Sprite, [i32; 2], u32);

#[derive(Default)]
pub struct HardwareCursor {
    off: bool,
    cache: HashMap<Key, CustomCursor>,
    current: Option<Key>,
    /// The system cursor is visible (the menus hide it while the game draws its own).
    shown: bool,
}

impl HardwareCursor {
    pub fn new() -> Self {
        Self { off: std::env::var_os("ACR_SOFT_CURSOR").is_some(), ..Default::default() }
    }

    /// Someone hid the system cursor (`set_cursor_visible(false)`): show it again next frame.
    pub fn hidden(&mut self) {
        self.shown = false;
    }

    /// Sets the window's cursor to the one the interface would draw at `scale` (window
    /// pixels per canvas pixel), or lets the interface draw it while its image loads.
    pub fn update(&mut self, ui: &mut Ui, window: &Window, event_loop: &ActiveEventLoop, scale: f32) {
        if self.off {
            return;
        }
        let Some(draw) = ui.cursor_draw() else { return };
        let key: Key = (draw.sprite.clone(), [draw.x, draw.y], scale.to_bits());
        if self.shown && self.current.as_ref() == Some(&key) {
            return;
        }
        if !self.cache.contains_key(&key) {
            let Some((image, offset)) = ui.cursor_image(&draw) else {
                // Not loaded yet: drawn by the interface meanwhile.
                ui.set_hardware_cursor(false);
                window.set_cursor_visible(false);
                self.shown = false;
                return;
            };
            let (rgba, w, h) = scaled(&image, scale);
            let hx = ((-offset[0]) as f32 * scale).round().clamp(0.0, (w - 1) as f32) as u16;
            let hy = ((-offset[1]) as f32 * scale).round().clamp(0.0, (h - 1) as f32) as u16;
            match CustomCursor::from_rgba(rgba, w as u16, h as u16, hx, hy) {
                Ok(source) => {
                    self.cache.insert(key.clone(), event_loop.create_custom_cursor(source));
                }
                Err(e) => {
                    log::warn!("hardware cursor: {e}; drawing it instead");
                    self.off = true;
                    ui.set_hardware_cursor(false);
                    return;
                }
            }
        }
        window.set_cursor(self.cache[&key].clone());
        window.set_cursor_visible(true);
        ui.set_hardware_cursor(true);
        self.current = Some(key);
        self.shown = true;
    }
}

/// `image` scaled by `scale` with bilinear filtering (as the interface's sampler draws it),
/// at least 1 × 1 and at most 256 × 256 pixels.
fn scaled(image: &Image, scale: f32) -> (Vec<u8>, u32, u32) {
    let (sw, sh) = (image.width.max(1), image.height.max(1));
    let w = ((sw as f32 * scale).round() as u32).clamp(1, 256);
    let h = ((sh as f32 * scale).round() as u32).clamp(1, 256);
    if (w, h) == (sw, sh) && image.rgba.len() == (sw * sh * 4) as usize {
        return (image.rgba.clone(), w, h);
    }
    let px = |x: i32, y: i32| -> [f32; 4] {
        // Clamped to the edge, as the interface's sampler.
        let (x, y) = (x.clamp(0, sw as i32 - 1) as u32, y.clamp(0, sh as i32 - 1) as u32);
        let i = ((y * sw + x) * 4) as usize;
        let p = &image.rgba[i..i + 4];
        // Premultiplied while filtering, so transparent pixels add no colour.
        let a = f32::from(p[3]) / 255.0;
        [f32::from(p[0]) * a, f32::from(p[1]) * a, f32::from(p[2]) * a, a]
    };
    let mut out = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let fx = (x as f32 + 0.5) * sw as f32 / w as f32 - 0.5;
            let fy = (y as f32 + 0.5) * sh as f32 / h as f32 - 0.5;
            let (x0, y0) = (fx.floor() as i32, fy.floor() as i32);
            let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
            let mut c = [0.0f32; 4];
            for (dx, dy, wgt) in
                [(0, 0, (1.0 - tx) * (1.0 - ty)), (1, 0, tx * (1.0 - ty)), (0, 1, (1.0 - tx) * ty), (1, 1, tx * ty)]
            {
                let p = px(x0 + dx, y0 + dy);
                for k in 0..4 {
                    c[k] += p[k] * wgt;
                }
            }
            let i = ((y * w + x) * 4) as usize;
            let a = c[3];
            for k in 0..3 {
                out[i + k] = if a > 0.0 { (c[k] / a).round().clamp(0.0, 255.0) as u8 } else { 0 };
            }
            out[i + 3] = (a * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
    (out, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scales_and_keeps_opaque_colour() {
        let image = Image { width: 2, height: 2, rgba: [255, 0, 0, 255].repeat(4) };
        let (rgba, w, h) = scaled(&image, 2.0);
        assert_eq!((w, h), (4, 4));
        assert_eq!(&rgba[..4], &[255, 0, 0, 255]);
        let (same, ..) = scaled(&image, 1.0);
        assert_eq!(same, image.rgba);
    }
}
