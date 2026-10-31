//! `hud_scale`: the HUD panels drawn larger about their screen corner, so their icons are
//! finger-sized on a phone. ACR's own: the original has one size.
//!
//! The panels are laid out as ever (docs/ui.md) and then scaled about the corner they hang
//! from: the command panel about the bottom-left corner (it grows up and right), the minimap
//! about the bottom-right (up and left), the side panels about the top-right and the army
//! list about its first icon. A [`Zoom`] maps both ways: a group's draws go to the window on
//! a [`Canvas`] [`Zoom::canvas`] that is `scale` times larger (the sprites are stretched like
//! on any canvas), and a press is mapped back with [`Zoom::to_layout`] to be tested against
//! the unscaled layout. The map and the top bar keep their size.

use crate::canvas::Canvas;
use crate::ui::draw::Rect;

/// Range of the `hud_scale` setting.
pub const MIN: f32 = 1.0;
pub const MAX: f32 = 2.5;

/// Size on the glass the Android default aims at for a command icon: about 6 mm, in dp (a dp
/// is 1/160 inch).
const ICON_MM: f32 = 6.0;
const DP_PER_MM: f32 = 160.0 / 25.4;

/// The panels that scale, by the corner they hang from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    /// Selected units, the command icons and the market (bottom-left).
    Commands,
    /// The army list (top-left, from its first icon).
    Army,
    /// The minimap (bottom-right).
    Minimap,
    /// The unit and game panels (top-right).
    Panel,
}

/// A scale about an anchor, both in layout (unscaled canvas) pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Zoom {
    pub anchor: [f32; 2],
    pub scale: f32,
}

impl Zoom {
    pub const NONE: Zoom = Zoom { anchor: [0.0, 0.0], scale: 1.0 };

    /// A canvas point of the scaled panel, in the unscaled layout.
    pub fn to_layout(self, p: [f32; 2]) -> [f32; 2] {
        if self.scale == 1.0 {
            return p;
        }
        [self.anchor[0] + (p[0] - self.anchor[0]) / self.scale, self.anchor[1] + (p[1] - self.anchor[1]) / self.scale]
    }

    /// A layout rectangle as it is drawn on the canvas, `[x, y, w, h]`.
    pub fn rect(self, r: Rect) -> [f32; 4] {
        let s = self.scale;
        [
            self.anchor[0] + (r.x0 as f32 - self.anchor[0]) * s,
            self.anchor[1] + (r.y0 as f32 - self.anchor[1]) * s,
            r.width() as f32 * s,
            r.height() as f32 * s,
        ]
    }

    /// The canvas to draw the group's layout coordinates on: `base` stretched about the
    /// anchor, cut off at the edge of the screen `base` covers.
    pub fn canvas(self, base: Canvas) -> Canvas {
        if self.scale == 1.0 {
            return base;
        }
        let s = self.scale;
        let origin = [
            base.origin[0] + self.anchor[0] * base.scale * (1.0 - s),
            base.origin[1] + self.anchor[1] * base.scale * (1.0 - s),
        ];
        // The screen in layout pixels, rounded outwards.
        let (lo, hi) = (self.to_layout([base.clip.x0 as f32, base.clip.y0 as f32]), {
            self.to_layout([base.clip.x1 as f32, base.clip.y1 as f32])
        });
        let clip = Rect {
            x0: lo[0].floor() as i32,
            y0: lo[1].floor() as i32,
            x1: hi[0].ceil() as i32,
            y1: hi[1].ceil() as i32,
        };
        Canvas { scale: base.scale * s, origin, clip }
    }
}

/// The scale the Android default aims at: a command icon of `icon` canvas pixels about 6 mm
/// wide when one dp covers `density` canvas pixels, within the setting's range.
pub fn phone_scale(density: f32, icon: i32) -> f32 {
    (ICON_MM * DP_PER_MM * density / icon as f32).clamp(MIN, MAX)
}

/// The scale in force: the setting, else the phone default on Android and 1 elsewhere.
pub fn resolve(setting: Option<f32>, density: f32, android: bool, icon: i32) -> f32 {
    match setting {
        Some(s) => s.clamp(MIN, MAX),
        None if android => phone_scale(density, icon),
        None => MIN,
    }
}

/// A setting's value kept in range; `None` for what is no scale.
pub fn parse(value: &str) -> Option<f32> {
    value.trim().parse::<f32>().ok().filter(|s| s.is_finite() && *s > 0.0).map(|s| s.clamp(MIN, MAX))
}

/// `scale` capped so that a panel extending `extent` layout pixels from its anchor towards
/// the far side stays within `room`: `room / extent`, never below 1.
pub fn fit(scale: f32, room: f32, extent: f32) -> f32 {
    scale.min(room / extent).max(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scales_about_the_anchor_and_back() {
        let z = Zoom { anchor: [0.0, 768.0], scale: 2.0 };
        // The anchor stays; a point above and right of it moves twice as far.
        assert_eq!(z.rect(Rect::new(0, 768, 10, 0))[..2], [0.0, 768.0]);
        assert_eq!(z.rect(Rect::new(182, 630, 40, 40)), [364.0, 492.0, 80.0, 80.0]);
        assert_eq!(z.to_layout([364.0, 492.0]), [182.0, 630.0]);
        assert_eq!(Zoom::NONE.to_layout([5.0, 6.0]), [5.0, 6.0]);
    }

    #[test]
    fn the_zoomed_canvas_matches_the_layout() {
        let base = Canvas::screen([1584.0, 720.0]);
        let z = Zoom { anchor: [1690.0, 768.0], scale: 1.5 };
        let c = z.canvas(base);
        assert_eq!(c.scale, base.scale * 1.5);
        // A layout point lands where the canvas rectangle says.
        let r = Rect::new(1500, 600, 40, 40);
        let [x, y, w, h] = z.rect(r);
        let on_window = c.to_window([r.x0 as f32, r.y0 as f32, 40.0, 40.0]);
        let expect = base.to_window([x, y, w, h]);
        for (a, b) in on_window.iter().zip(expect) {
            assert!((a - b).abs() < 1e-3, "{on_window:?} {expect:?}");
        }
        // The anchor corner is fixed; the clip covers the screen and no more.
        assert!(
            (c.to_window([1690.0, 768.0, 0.0, 0.0])[0] - base.to_window([1690.0, 768.0, 0.0, 0.0])[0]).abs() < 1e-3
        );
        assert_eq!(Zoom::NONE.canvas(base), base);
        assert_eq!((c.clip.x0, c.clip.y0, c.clip.y1), (563, 256, 768));
    }

    #[test]
    fn the_phone_default_and_the_setting() {
        // 3168 x 1440 at scale factor 3.5: a dp is 1.867 canvas pixels; the icon 6 mm wide.
        let phone = phone_scale(3.5 / 1.875, 40);
        assert!((phone - 1.764).abs() < 0.01, "{phone}");
        let lower = phone_scale(1.5, 40);
        assert!(lower > 1.0 && lower < phone, "{lower}");
        assert_eq!(phone_scale(5.0, 40), MAX);
        assert_eq!(phone_scale(0.1, 40), MIN);
        // Unset: the phone default on Android, 1 on the desktop; a set value wins.
        assert_eq!(resolve(None, 3.5 / 1.875, true, 40), phone);
        assert_eq!(resolve(None, 3.5 / 1.875, false, 40), 1.0);
        assert_eq!(resolve(Some(1.8), 3.5 / 1.875, true, 40), 1.8);
        assert_eq!(resolve(Some(1.0), 3.5 / 1.875, true, 40), 1.0);
        assert_eq!(parse("1.8"), Some(1.8));
        assert_eq!(parse("9"), Some(MAX));
        assert_eq!(parse("0.2"), Some(MIN));
        assert_eq!((parse("-1"), parse("0"), parse("big"), parse("nan"), parse("inf")), (None, None, None, None, None));
        assert_eq!(fit(2.5, 768.0, 474.0), 768.0 / 474.0);
        assert_eq!(fit(1.0, 100.0, 474.0), 1.0);
    }
}
