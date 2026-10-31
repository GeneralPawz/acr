//! The touch top bar (`top_bar=touch`, docs/ui.md "Touch top bar"): two finger-sized buttons
//! instead of the resource bar and the "M" plaque, Menu at the top left and Resources at the
//! top right with a compact panel under it. Only the layout lives here; [`super::Hud`] draws
//! and hit-tests it.
//!
//! The interface is laid out in canvas pixels, which the window scales by
//! [`crate::canvas::Canvas::scale`]. A button must be at least [`MIN_DP`] density-independent
//! pixels, so the layout takes the canvas pixels one dp covers (`scale_factor / canvas scale`).

use crate::ui::draw::Rect;

/// The smallest button edge in density-independent pixels (the touch target guideline; about
/// 9 mm on a phone).
pub const MIN_DP: f32 = 48.0;
/// The smallest button edge in canvas pixels, so the label fits at a low density.
const MIN_SIDE: i32 = 32;
/// Space between a button and the screen edge, in dp.
const MARGIN_DP: f32 = 4.0;
/// The resource panel's inner border and its row height, in canvas pixels / dp.
pub const PANEL_PAD: i32 = 6;
const ROW_DP: f32 = 24.0;
const MIN_ROW: i32 = 28;
const MIN_PANEL_WIDTH: i32 = 150;
const PANEL_WIDTH_DP: f32 = 80.0;
/// The six resources, one row each.
pub const ROWS: usize = 6;

/// Where the buttons and the panel are, in canvas pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    pub menu: Rect,
    pub resources: Rect,
    /// The compact panel under the Resources button (drawn only while open).
    pub panel: Rect,
    /// Height of a panel row.
    pub row: i32,
}

fn px(dp: f32, density: f32) -> i32 {
    (dp * density).ceil() as i32
}

impl Layout {
    /// The layout for a screen of `screen` canvas pixels where one dp covers `density` canvas
    /// pixels. A density that is not a positive number counts as 1.
    pub fn new(screen: [i32; 2], density: f32) -> Self {
        let density = if density.is_finite() && density > 0.0 { density } else { 1.0 };
        let side = px(MIN_DP, density).max(MIN_SIDE);
        let width = side * 3 / 2;
        let margin = px(MARGIN_DP, density);
        let menu = Rect::new(margin, margin, width, side);
        let resources = Rect::new(screen[0] - margin - width, margin, width, side);
        let row = px(ROW_DP, density).max(MIN_ROW);
        let panel_width = px(PANEL_WIDTH_DP, density).max(MIN_PANEL_WIDTH);
        let panel = Rect::new(
            resources.x1 - panel_width,
            resources.y1 + margin,
            panel_width,
            row * ROWS as i32 + 2 * PANEL_PAD,
        );
        Self { menu, resources, panel, row }
    }

    /// Row `i` of the panel (the icons' order of the classic bar).
    pub fn row_rect(&self, i: usize) -> Rect {
        Rect::new(
            self.panel.x0 + PANEL_PAD,
            self.panel.y0 + PANEL_PAD + self.row * i as i32,
            self.panel.width() - 2 * PANEL_PAD,
            self.row,
        )
    }

    /// Whether `p` is on a button, or on the panel while it is `open`.
    pub fn covers(&self, p: [f32; 2], open: bool) -> bool {
        self.menu.contains(p) || self.resources.contains(p) || (open && self.panel.contains(p))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buttons_are_at_least_48_dp_at_any_density() {
        for density in [0.3, 0.5, 1.0, 1.0 / 0.9375, 1.9, 3.5 / 1.875] {
            let l = Layout::new([1690, 768], density);
            for r in [l.menu, l.resources] {
                assert!(r.height() as f32 >= MIN_DP * density - 0.001, "{density}: {r:?}");
                assert!(r.width() >= r.height(), "{density}: {r:?}");
                assert!(r.height() >= MIN_SIDE);
            }
        }
    }

    /// A phone (1440 x 3168 at scale 3.5, played landscape) and a desktop window.
    #[test]
    fn button_rects_at_a_few_window_sizes() {
        // 3168 x 1440 physical: canvas scale 1.875, screen 1690 x 768, a dp is 3.5 / 1.875.
        let phone = Layout::new([1690, 768], 3.5 / 1.875);
        assert_eq!(phone.menu, Rect::new(8, 8, 135, 90));
        assert_eq!(phone.resources, Rect::new(1690 - 8 - 135, 8, 135, 90));
        assert_eq!(phone.menu.height() as f32 * 1.875, 168.75);
        // The same screen at scale 1 (a laptop window of 1584 x 720, scale 0.9375).
        let window = Layout::new([1690, 768], 1.0 / 0.9375);
        assert_eq!(window.menu, Rect::new(5, 5, 78, 52));
        assert_eq!(window.resources.x1, 1690 - 5);
        // The classic 1024 x 768 screen at scale 1.
        let classic = Layout::new([1024, 768], 1.0);
        assert_eq!(classic.menu, Rect::new(4, 4, 72, 48));
        assert_eq!(classic.resources, Rect::new(1024 - 4 - 72, 4, 72, 48));
        // A bad density falls back to 1.
        assert_eq!(Layout::new([1024, 768], f32::NAN), classic);
        assert_eq!(Layout::new([1024, 768], 0.0), classic);
    }

    #[test]
    fn panel_sits_under_the_resources_button() {
        let l = Layout::new([1024, 768], 1.0);
        assert_eq!(l.panel.x1, l.resources.x1);
        assert_eq!(l.panel.y0, l.resources.y1 + 4);
        assert_eq!(l.panel.width(), MIN_PANEL_WIDTH);
        assert_eq!(l.row, MIN_ROW);
        assert_eq!(l.row_rect(0), Rect::new(l.panel.x0 + PANEL_PAD, l.panel.y0 + PANEL_PAD, 138, 28));
        assert_eq!(l.row_rect(5).y1 + PANEL_PAD, l.panel.y1);
    }

    #[test]
    fn hit_testing_follows_the_open_panel() {
        let l = Layout::new([1024, 768], 1.0);
        let inside = |r: Rect| [(r.x0 + 1) as f32, (r.y0 + 1) as f32];
        assert!(l.covers(inside(l.menu), false));
        assert!(l.covers(inside(l.resources), false));
        assert!(!l.covers(inside(l.panel), false));
        assert!(l.covers(inside(l.panel), true));
        // The area the classic bar covered is map now: between the buttons, and under them.
        assert!(!l.covers([512.0, 10.0], true));
        assert!(!l.covers([l.menu.x1 as f32, 10.0], true));
        assert!(!l.covers([10.0, l.menu.y1 as f32], true));
        // Edges: the right and bottom edges are exclusive.
        assert!(!l.covers([l.menu.x1 as f32, l.menu.y0 as f32], false));
        assert!(l.covers([(l.menu.x1 - 1) as f32, (l.menu.y1 - 1) as f32], false));
    }
}
