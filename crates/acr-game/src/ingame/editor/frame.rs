//! Where the editor's window stands and how big it is (ACR's own: the original's is fixed).
//! The window is laid out at the original's place and size, then moved by the frame's offset;
//! the frame only changes what depends on the size: the icon columns (one more per 44 pixels
//! of width), the visible height of the grid and the scroll bar, and the width of the extras
//! box. It never leaves the screen, together with the extras box under it, and is never
//! smaller than the original's window. The place is a UI setting (`editor_window` in
//! `settings.txt`), kept per session.

use super::{COLUMNS, PITCH, ROW, VISIBLE_H};
use crate::hud::SCREEN;
use crate::ui::draw::Rect;
use crate::ui::settings::EditorWindow;

/// The window's box: its top-left corner and its size as the original's `addBorder` counts it
/// (`RealLx − 269`, `InfY1`; 249 wide, 31 + 386 high, the last pixel included).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frame {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    /// Only the title row stands (F2, the "_" button).
    pub folded: bool,
}

/// The original's place and size.
pub const DEFAULT: Frame = Frame { x: SCREEN[0] - 259 - 10, y: 48, w: 249, h: 417, folded: false };
/// The tallest the extras box under the window gets (its rows at most, plus the frame).
const EXTRAS_MAX: i32 = 14 + 15 * ROW;
/// The title row's buttons, from the window's left edge: the grip to move it by, and "_".
const HANDLE_X: i32 = 124;
const MINIMISE_X: i32 = 150;
/// The corner grip's side.
const GRIP: i32 = 14;

/// What a press on the window's grips takes hold of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grab {
    /// The title row's handle: the whole window follows the cursor.
    Move,
    /// The corner grip: the size follows the cursor.
    Resize,
}

/// A grip held down: the button that took it, the cursor where it began and the frame then.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Drag {
    pub grab: Grab,
    /// Taken with the right button (a touch long press); else with the left.
    pub right: bool,
    from: (i32, i32),
    start: Frame,
}

impl Drag {
    pub fn new(grab: Grab, right: bool, from: (i32, i32), start: Frame) -> Self {
        Self { grab, right, from, start }
    }

    /// The frame with the cursor at `to`.
    pub fn at(&self, to: (i32, i32)) -> Frame {
        let (dx, dy) = (to.0 - self.from.0, to.1 - self.from.1);
        match self.grab {
            Grab::Move => Frame { x: self.start.x + dx, y: self.start.y + dy, ..self.start }.fitted(),
            // The corner stays where it is: the size is held by the screen from there.
            Grab::Resize => {
                let (s, max) = (self.start, Frame::max_size(self.start));
                Frame { w: (s.w + dx).min(max.0), h: (s.h + dy).min(max.1), ..s }.fitted()
            }
        }
    }
}

impl Frame {
    /// The frame of the setting, kept on the screen; the original's without one.
    pub fn from_setting(setting: Option<EditorWindow>) -> Self {
        setting.map_or(DEFAULT, |s| Self { x: s.x, y: s.y, w: s.w, h: s.h, folded: s.folded }.fitted())
    }

    pub fn setting(&self) -> Option<EditorWindow> {
        (*self != DEFAULT).then_some(EditorWindow { x: self.x, y: self.y, w: self.w, h: self.h, folded: self.folded })
    }

    /// At least the original's size, on the screen with the extras box under it (folded, only
    /// the title row has to fit).
    pub fn fitted(self) -> Self {
        let w = self.w.clamp(DEFAULT.w, SCREEN[0] - 1);
        let h = self.h.clamp(DEFAULT.h, SCREEN[1] - EXTRAS_MAX);
        let x = self.x.clamp(0, SCREEN[0] - 1 - w);
        let room = if self.folded { SCREEN[1] - ROW } else { SCREEN[1] - EXTRAS_MAX - h };
        Self { x, y: self.y.clamp(0, room), w, h, folded: self.folded }
    }

    /// The largest size from this frame's corner: to the screen's edge, extras box included.
    fn max_size(f: Frame) -> (i32, i32) {
        ((SCREEN[0] - 1 - f.x).max(DEFAULT.w), (SCREEN[1] - EXTRAS_MAX - f.y).max(DEFAULT.h))
    }

    /// Folded or not, kept on the screen.
    pub fn toggled(self) -> Self {
        Self { folded: !self.folded, ..self }.fitted()
    }

    /// The window's origin, the `DialogsSystem` corner of the original (0x5bb6c0).
    pub fn origin(&self) -> (i32, i32) {
        (self.x, self.y + 31)
    }

    /// How far the window stands from the original's place.
    pub fn shift(&self) -> (i32, i32) {
        (self.x - DEFAULT.x, self.y - DEFAULT.y)
    }

    /// The same size at the original's place: where the layout is built.
    pub fn at_default(&self) -> Self {
        Self { x: DEFAULT.x, y: DEFAULT.y, ..*self }
    }

    pub fn extra_w(&self) -> i32 {
        self.w - DEFAULT.w
    }

    pub fn extra_h(&self) -> i32 {
        self.h - DEFAULT.h
    }

    /// Icons to a row: five, and one more for each 44 pixels of width.
    pub fn columns(&self) -> usize {
        COLUMNS + (self.extra_w() / PITCH) as usize
    }

    /// The grid's visible height ([`VISIBLE_H`] in the original).
    pub fn visible_h(&self) -> i32 {
        VISIBLE_H + self.extra_h()
    }

    /// The right edge of the box (the last pixel).
    pub fn x1(&self) -> i32 {
        self.x + self.w
    }

    /// The bottom edge of the box (the last pixel).
    pub fn y1(&self) -> i32 {
        self.y + self.h
    }

    /// The clipped content (`addClipper(DS.x + 4, DS.y + 75, RealLx, DS.y + 386)`).
    pub fn content(&self) -> Rect {
        let (ox, oy) = self.origin();
        Rect { x0: ox + 4, y0: oy + 75, x1: self.x1(), y1: self.y1() }
    }

    /// The characters a line of the extras box holds: 44 in the original's width.
    pub fn chars(&self) -> usize {
        44 + (self.extra_w() / 6) as usize
    }

    /// The title row's grip, "_" button and the corner grip, in this frame.
    pub fn handle(&self) -> Rect {
        Rect::new(self.x + HANDLE_X, self.y + 1, 22, ROW)
    }

    pub fn minimise(&self) -> Rect {
        Rect::new(self.x + MINIMISE_X, self.y + 1, 20, ROW)
    }

    pub fn grip(&self) -> Rect {
        Rect::inclusive(self.x1() - GRIP + 1, self.y1() - GRIP + 1, self.x1(), self.y1())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_is_the_originals_place_and_size() {
        assert_eq!((DEFAULT.x, DEFAULT.y, DEFAULT.w, DEFAULT.h), (755, 48, 249, 417));
        assert_eq!(DEFAULT.x1(), 1004);
        assert_eq!(DEFAULT.y1(), 465);
        assert_eq!(DEFAULT.origin(), (755, 79));
        assert_eq!(DEFAULT.columns(), 5);
        assert_eq!(DEFAULT.visible_h(), VISIBLE_H);
        assert_eq!(DEFAULT.content(), Rect { x0: 759, y0: 154, x1: 1004, y1: 465 });
        // The original's place is no setting.
        assert_eq!(DEFAULT.setting(), None);
        assert_eq!(Frame::from_setting(None), DEFAULT);
    }

    #[test]
    fn a_move_follows_the_cursor_and_stays_on_the_screen() {
        let drag = Drag::new(Grab::Move, false, (800, 60), DEFAULT);
        let f = drag.at((700, 100));
        assert_eq!((f.x, f.y, f.w, f.h), (655, 88, 249, 417));
        // Far left and up: against the corner; far right: the right edge on the last column.
        let f = drag.at((-900, -900));
        assert_eq!((f.x, f.y), (0, 0));
        let f = drag.at((5000, 60));
        assert_eq!(f.x, SCREEN[0] - 1 - 249);
        // Far down: the extras box under the window still fits.
        let f = drag.at((800, 5000));
        assert_eq!(f.y + f.h + EXTRAS_MAX, SCREEN[1]);
    }

    #[test]
    fn a_resize_never_goes_below_the_original_or_off_the_screen() {
        let moved = Frame { x: 400, y: 20, ..DEFAULT };
        let drag = Drag::new(Grab::Resize, false, (1000, 460), moved);
        let f = drag.at((1100, 500));
        assert_eq!((f.w, f.h), (349, 457));
        // Smaller than the original: the original.
        let f = drag.at((0, 0));
        assert_eq!((f.w, f.h), (249, 417));
        // Bigger than the screen: to its edge.
        let f = drag.at((9000, 9000));
        assert_eq!(f.x1(), SCREEN[0] - 1);
        assert_eq!(f.y1() + EXTRAS_MAX, SCREEN[1]);
        // At the original's place the width has only 20 pixels to grow.
        let f = Drag::new(Grab::Resize, false, (1000, 460), DEFAULT).at((9000, 460));
        assert_eq!(f.w, 268);
    }

    #[test]
    fn a_column_comes_with_every_44_pixels_of_width() {
        let wide = |w| Frame { x: 100, w, ..DEFAULT }.columns();
        assert_eq!([wide(249), wide(292), wide(293), wide(336), wide(337), wide(425)], [5, 5, 6, 6, 7, 9]);
        let tall = Frame { h: 417 + 88, ..DEFAULT };
        assert_eq!(tall.visible_h(), VISIBLE_H + 88);
        assert_eq!(tall.content().y1, tall.y1());
        assert_eq!(Frame { w: 337, x: 100, ..DEFAULT }.chars(), 44 + 14);
    }

    #[test]
    fn the_grips_follow_the_frame() {
        let f = Frame { x: 100, y: 200, w: 337, h: 500, folded: false };
        assert_eq!(f.handle().x0, 224);
        assert_eq!(f.grip().x1, f.x1() + 1);
        assert_eq!(f.grip().y1, f.y1() + 1);
        assert!(f.grip().contains([f.x1() as f32, f.y1() as f32]));
        // The title row's grips stay clear of the Schlacht button and of each other.
        assert!(f.handle().x1 <= f.minimise().x0 && f.minimise().x1 <= f.x1() - 70);
        assert_eq!(f.shift(), (100 - 755, 200 - 48));
        assert_eq!(f.at_default().shift(), (0, 0));
        assert_eq!(f.at_default().w, 337);
    }

    #[test]
    fn folding_keeps_the_place_and_unfolding_makes_room() {
        let low = Frame { y: 700, folded: true, ..DEFAULT };
        assert_eq!(low.fitted().y, 700);
        // Unfolded again the window and its extras box would leave the screen: it moves up.
        let open = low.toggled();
        assert!(!open.folded && open.y + open.h + EXTRAS_MAX <= SCREEN[1]);
        assert_eq!(open.toggled().x, open.x);
    }

    #[test]
    fn the_setting_round_trips_and_a_bad_one_is_fitted() {
        let f = Frame { x: 40, y: 12, w: 337, h: 500, folded: true };
        assert_eq!(Frame::from_setting(f.setting()), f);
        // Off the screen and too small: kept on it, at the least size.
        let bad = EditorWindow { x: 5000, y: -4, w: 10, h: 10, folded: false };
        let f = Frame::from_setting(Some(bad));
        assert_eq!((f.x, f.y, f.w, f.h), (SCREEN[0] - 1 - 249, 0, 249, 417));
    }
}
