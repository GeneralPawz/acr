//! The touch top bar (`top_bar=touch`, docs/ui.md "Touch top bar"): the classic bar's own
//! pieces as plaques at the top right, from the right: a short bit of the resource bar with
//! the gold icon (under which a stack of resource slots opens), the "M" plaque, a pause plaque
//! and a small escape plaque (a bare bit of the bar, with a glyph of the game's font). Only
//! the layout lives here; [`super::Hud`] draws and hit-tests it.
//!
//! The plaques are cut from `Interf2\ResPanel` and drawn at the sprite's own size (the canvas
//! scales them: a whole-number multiple of the original, never blurred). The touch target is
//! larger than the plaque: the hit rects reach [`MIN_DP`] density-independent pixels, so the
//! layout takes the canvas pixels one dp covers (`scale_factor / canvas scale`).

use crate::ui::draw::Rect;

/// The smallest hit rect edge in density-independent pixels (about 7.5 mm on a phone).
pub const MIN_DP: f32 = 40.0;
/// Space between a plaque and the screen edge, in dp; between the two plaques in pixels.
const MARGIN_DP: f32 = 4.0;
const GAP: i32 = 4;
/// Where the plaques start in `ResPanel` frames 0 and 2: the left end cap's post (x = 11).
pub const CAP_X: i32 = 11;
/// The sprite's rows: row 0 is empty, the body is [`BODY_H`] rows, the shadow below it
/// [`SHADOW_H`].
pub const BODY_H: i32 = 35;
pub const SHADOW_H: i32 = 6;
/// The "M" plaque (frame 2, x 11..58).
pub const M_W: i32 = 47;
/// An end cap of the bar: its post and a little of the bar beside it. The left cap is frame 0
/// at 11..27, the right one at [`RIGHT_CAP_X`]..707.
pub const CAP_W: i32 = 16;
pub const RIGHT_CAP_X: i32 = 691;
/// Empty bar between two icons (frame 0, x 62..138), the filler of a slot.
pub const FILL_X: i32 = 62;
pub const FILL_W: i32 = 76;
/// The Resources plaque: cap, the gold icon with [`ICON_PAD`] either side, cap.
pub const ICON_PAD: i32 = 8;
pub const ICON_W: i32 = 32;
const RES_W: i32 = CAP_W * 2 + ICON_PAD * 2 + ICON_W;
/// The bare plaques: cap, filler of this width, cap.
pub const PAUSE_MID: i32 = 24;
pub const ESCAPE_MID: i32 = 12;
const PAUSE_W: i32 = CAP_W * 2 + PAUSE_MID;
const ESCAPE_W: i32 = CAP_W * 2 + ESCAPE_MID;
/// A row of the panel: cap, one classic slot (padding, icon, room for the value), cap.
pub const ROW_MID: i32 = 104;
pub const ROW_W: i32 = CAP_W * 2 + ROW_MID;
/// The six resources, one row each; a row's body is [`BODY_H`] high and they touch.
pub const ROWS: usize = 6;

/// Where the plaques, their touch targets and the panel are, in canvas pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    /// The "M" plaque's body, and the touch target around it.
    pub menu_draw: Rect,
    pub menu: Rect,
    /// The Resources plaque's body, and the touch target around it.
    pub resources_draw: Rect,
    pub resources: Rect,
    /// The pause plaque (left of "M"): pauses, or resumes while paused.
    pub pause_draw: Rect,
    pub pause: Rect,
    /// The escape plaque (left of pause): sends Escape.
    pub escape_draw: Rect,
    pub escape: Rect,
    /// The stack of slots under the Resources plaque (hit-tested only while open).
    pub panel: Rect,
    /// Gap between the screen edge and a plaque.
    pub margin: i32,
}

fn px(dp: f32, density: f32) -> i32 {
    (dp * density).ceil() as i32
}

impl Layout {
    /// The layout for a screen of `screen` canvas pixels where one dp covers `density` canvas
    /// pixels. A density that is not a positive number counts as 1.
    pub fn new(screen: [i32; 2], density: f32) -> Self {
        let density = if density.is_finite() && density > 0.0 { density } else { 1.0 };
        let margin = px(MARGIN_DP, density);
        let min = px(MIN_DP, density);
        let resources_draw = Rect::new(screen[0] - margin - RES_W, margin, RES_W, BODY_H);
        let menu_draw = Rect::new(resources_draw.x0 - GAP - M_W, margin, M_W, BODY_H);
        // The targets run to the top edge and, for Resources, the right edge; they meet halfway
        // between the plaques.
        let y1 = resources_draw.y1.max(min);
        let resources = Rect { x0: (resources_draw.x0 - GAP / 2).min(screen[0] - min), y0: 0, x1: screen[0], y1 };
        let menu = Rect { x0: menu_draw.x0.min(resources.x0 - min), y0: 0, x1: resources.x0, y1 };
        // Pause and escape: their targets (at least `min` wide) tile the space left of "M" and
        // each plaque sits at the right end of its target, so a finger never lands on two.
        let pause = Rect { x0: menu.x0 - min.max(PAUSE_W + GAP), y0: 0, x1: menu.x0, y1 };
        let escape = Rect { x0: pause.x0 - min.max(ESCAPE_W + GAP), y0: 0, x1: pause.x0, y1 };
        let pause_draw = Rect::new(pause.x1 - GAP - PAUSE_W, margin, PAUSE_W, BODY_H);
        let escape_draw = Rect::new(escape.x1 - GAP - ESCAPE_W, margin, ESCAPE_W, BODY_H);
        let panel = Rect::new(resources_draw.x1 - ROW_W, resources_draw.y1 + margin, ROW_W, BODY_H * ROWS as i32);
        Self { menu_draw, menu, resources_draw, resources, pause_draw, pause, escape_draw, escape, panel, margin }
    }

    /// Row `i` of the panel (the icons' order of the classic bar).
    pub fn row_rect(&self, i: usize) -> Rect {
        Rect::new(self.panel.x0, self.panel.y0 + BODY_H * i as i32, ROW_W, BODY_H)
    }

    /// Whether `p` is on a plaque's target, or on the panel while it is `open`.
    pub fn covers(&self, p: [f32; 2], open: bool) -> bool {
        self.menu.contains(p)
            || self.pause.contains(p)
            || self.escape.contains(p)
            || self.resources.contains(p)
            || (open && self.panel.contains(p))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_are_at_least_40_dp_and_hold_the_plaque() {
        for density in [0.3, 0.5, 1.0, 1.0 / 0.9375, 1.9, 3.5 / 1.875] {
            let l = Layout::new([1690, 768], density);
            let min = (MIN_DP * density).ceil() as i32;
            for (hit, draw) in [
                (l.menu, l.menu_draw),
                (l.resources, l.resources_draw),
                (l.pause, l.pause_draw),
                (l.escape, l.escape_draw),
            ] {
                assert!(hit.height() >= min && hit.width() >= min, "{density}: {hit:?}");
                assert!(hit.x0 <= draw.x0 && hit.x1 >= draw.x1 && hit.y0 <= draw.y0 && hit.y1 >= draw.y1);
            }
            assert!(l.menu.x1 <= l.resources.x0, "{density}: the targets overlap");
            assert!(l.pause.x1 <= l.menu.x0 && l.escape.x1 <= l.pause.x0, "{density}: the targets overlap");
            assert!(l.escape.x0 >= 0 && l.escape_draw.x0 >= 0);
        }
    }

    /// A phone (1440 x 3168 at scale 3.5, played landscape) and a desktop window.
    #[test]
    fn rects_at_a_few_window_sizes() {
        // 3168 x 1440 physical: canvas scale 1.875, screen 1690 x 768, a dp is 3.5 / 1.875.
        let phone = Layout::new([1690, 768], 3.5 / 1.875);
        assert_eq!(phone.resources_draw, Rect::new(1602, 8, 80, 35));
        assert_eq!(phone.menu_draw, Rect::new(1551, 8, 47, 35));
        assert_eq!(phone.resources, Rect::new(1600, 0, 90, 75));
        assert_eq!(phone.menu, Rect::new(1525, 0, 75, 75));
        assert_eq!(phone.pause_draw, Rect::new(1465, 8, 56, 35));
        assert_eq!(phone.pause, Rect::new(1450, 0, 75, 75));
        assert_eq!(phone.escape_draw, Rect::new(1402, 8, 44, 35));
        assert_eq!(phone.escape, Rect::new(1375, 0, 75, 75));
        // The plaque is 35 canvas pixels (66 physical, 19 dp), the target 75 (140, 40 dp).
        assert!(phone.resources.height() as f32 * 1.875 / 3.5 >= MIN_DP);
        // The same screen at scale 1 (a laptop window of 1584 x 720, scale 0.9375).
        let window = Layout::new([1690, 768], 1.0 / 0.9375);
        assert_eq!(window.resources_draw.x1, 1690 - 5);
        assert_eq!(window.resources.y1, 43);
        // The classic 1024 x 768 screen at scale 1.
        let classic = Layout::new([1024, 768], 1.0);
        assert_eq!(classic.resources_draw, Rect::new(940, 4, 80, 35));
        assert_eq!(classic.resources, Rect::new(938, 0, 86, 40));
        assert_eq!(classic.menu, Rect::new(889, 0, 49, 40));
        assert_eq!(classic.pause, Rect::new(829, 0, 60, 40));
        assert_eq!(classic.escape, Rect::new(781, 0, 48, 40));
        // A bad density falls back to 1.
        assert_eq!(Layout::new([1024, 768], f32::NAN), classic);
        assert_eq!(Layout::new([1024, 768], 0.0), classic);
    }

    #[test]
    fn panel_sits_under_the_resources_plaque() {
        let l = Layout::new([1024, 768], 1.0);
        assert_eq!(l.panel.x1, l.resources_draw.x1);
        assert_eq!(l.panel.y0, l.resources_draw.y1 + l.margin);
        assert_eq!(l.panel.width(), ROW_W);
        assert_eq!(l.row_rect(0), Rect::new(l.panel.x0, l.panel.y0, ROW_W, BODY_H));
        assert_eq!(l.row_rect(5).y1, l.panel.y1);
    }

    #[test]
    fn hit_testing_follows_the_open_panel() {
        let l = Layout::new([1024, 768], 1.0);
        let inside = |r: Rect| [(r.x0 + 1) as f32, (r.y0 + 1) as f32];
        assert!(l.covers(inside(l.menu), false));
        assert!(l.covers(inside(l.resources), false));
        assert!(l.covers(inside(l.pause), false));
        assert!(l.covers(inside(l.escape), false));
        assert!(!l.covers(inside(l.panel), false));
        assert!(l.covers(inside(l.panel), true));
        // The area the classic bar covered is map now: left of the plaques, and under them.
        assert!(!l.covers([512.0, 10.0], true));
        assert!(!l.covers([(l.escape.x0 - 1) as f32, 10.0], true));
        assert!(!l.covers([l.menu.x0 as f32 + 5.0, l.menu.y1 as f32], true));
        // The targets reach the top and the right edge of the screen.
        assert!(l.covers([1023.0, 0.0], false));
        assert!(!l.covers([l.menu.x1 as f32, l.menu.y1 as f32], false));
    }
}
