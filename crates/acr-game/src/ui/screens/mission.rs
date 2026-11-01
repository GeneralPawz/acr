//! The mission windows over the running game (see `docs/ui.md`, mission windows):
//!
//! - [`MessageWindow`]: a page of the mission text (`ProcessMissionText` 0x5e16b0), an OK/No
//!   question (0x5adfb0, `AskQuestion`) and the objectives (0x5e11c0, F1). All three use
//!   `Interf2\Messages`: a caption, the page's portrait over a separator line, the text in a
//!   scrolling viewer and `Interf2\Button2` buttons.
//! - [`ChoiceWindow`]: the multiple choice of `AskMultilineQuestion` (0x5ad540), a paper box
//!   with the text and one clickable text per choice.
//!
//! The original runs each window as a modal loop inside the script call; ACR pushes it on
//! the screen stack, which stops the game while it is open, and leaves the answer in an
//! [`Answer`] cell for the mission script.

use std::cell::Cell;
use std::rc::Rc;

use super::super::dialog::{Align, CANCEL, OK, WidgetId};
use super::super::draw::{Draw, Rect};
use super::super::fonts::FontId;
use super::super::{Ctx, Input, Nav, Screen};
use super::{Menu, font};

pub const MESSAGES: &str = "INTERF2\\MESSAGES.GP";
pub const BUTTON2: &str = "INTERF2\\BUTTON2.GP";
pub const VSCROLL2: &str = "INTERF2\\VSCROLL2.GP";
pub const LINE2: &str = "INTERF2\\LINE2.GP";
const BORDER: &str = crate::hud::gp::BORDER;

/// `RLCFont(rom11)` with `SetRedColor` (the question text, a choice under the mouse) and
/// plain (the choices) of `AskMultilineQuestion`.
const ROM11_RED: FontId = FontId("rom11red");
const ROM11: FontId = FontId("rom11");

/// Where a closed window leaves its answer: 1 = OK, 0 = No, or the index of the choice.
pub type Answer = Rc<Cell<Option<u32>>>;

/// The picture of a page header, `"<gp> <frame>"` read with `sscanf("%s%d")`; `NONE`, a
/// header without a number and an empty header show none.
pub fn portrait(header: &str) -> Option<(String, usize)> {
    if header == "NONE" {
        return None;
    }
    let mut words = header.split_ascii_whitespace();
    let name = words.next()?;
    let frame: i32 = words.next()?.parse().ok()?;
    (frame >= 0).then(|| (format!("{name}.GP"), frame as usize))
}

/// The three windows built on `Interf2\Messages`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Window {
    /// `ShowPage`: frames 0/1, caption `INTF_MESSAGE`, OK.
    Page,
    /// `AskQuestion`: frames 2/3, caption `INTF_CROOM_GS0`, OK and No.
    Question,
    /// F1: frames 0/1, caption `MISSOBJ`, the objectives text without picture, OK.
    Objectives,
}

pub struct MessageWindow {
    window: Window,
    text: Vec<u8>,
    portrait: Option<(String, usize)>,
    answer: Answer,
    menu: Menu,
}

impl MessageWindow {
    pub fn new(window: Window, header: &str, text: Vec<u8>, answer: Answer) -> Self {
        let portrait = if window == Window::Objectives { None } else { portrait(header) };
        Self { window, text, portrait, answer, menu: Menu::default() }
    }
}

impl Screen for MessageWindow {
    fn dialog(&self) -> Option<&super::super::dialog::Dialog> {
        Some(&self.menu.d)
    }

    fn assets(&self, _ctx: &Ctx) -> Vec<String> {
        let mut files: Vec<String> = [MESSAGES, BUTTON2, VSCROLL2, LINE2].map(str::to_owned).into();
        files.extend(self.portrait.as_ref().map(|(gp, _)| gp.clone()));
        files
    }

    fn build(&mut self, ctx: &Ctx) {
        let frame = if self.window == Window::Question { 2 } else { 0 };
        let caption = match self.window {
            Window::Page => "INTF_MESSAGE",
            Window::Question => "INTF_CROOM_GS0",
            Window::Objectives => "MISSOBJ",
        };
        let picture = if self.window == Window::Objectives { None } else { Some(self.portrait.as_ref()) };
        let (x0, y0) = page_layout(&mut self.menu, ctx, frame, caption, picture, &self.text);
        let lib = ctx.lib;
        let d = &mut self.menu.d;
        let ok = ctx.texts.get("@MSOK");
        if self.window == Window::Question {
            let b = d.add_gp_text_button(lib, x0 + 75, y0 + 521, ok, BUTTON2, 3, font::F18Y5, font::F18Y5);
            d.choose(b, OK);
            let no = ctx.texts.get("@MSNO");
            let b = d.add_gp_text_button(lib, x0 + 307, y0 + 521, no, BUTTON2, 3, font::F18Y5, font::F18Y5);
            d.choose(b, CANCEL);
        } else {
            let b = d.add_gp_text_button(lib, x0 + 191, y0 + 520, ok, BUTTON2, 3, font::F18Y5, font::F18Y5);
            d.choose(b, OK);
        }
    }

    fn input(&mut self, _ctx: &Ctx, input: Input) -> Nav {
        // Enter is OK; Escape is No in a question and closes the other windows like OK.
        let answer = match self.menu.input(input) {
            Some(OK) => 1,
            Some(CANCEL) if self.window == Window::Question => 0,
            Some(CANCEL) => 1,
            _ => return Nav::Stay,
        };
        self.answer.set(Some(answer));
        Nav::Pop
    }

    fn draw(&self, ctx: &Ctx, out: &mut Vec<Draw>) {
        self.menu.draw(ctx, out);
    }

    fn take_sounds(&mut self) -> Vec<String> {
        self.menu.take_sounds()
    }

    fn in_game(&self) -> bool {
        true
    }
}

/// The common layout of the windows on `Interf2\Messages` (0x5e16b0, 0x5adfb0, 0x5e11c0,
/// 0x5e1d30): frame `frame` centred with its shadow `frame + 1`, the caption, then for pages
/// (`picture` is `Some`) the portrait centred at y = 75 over `Interf2\Line2`, and the text
/// viewer with its scroll bar down to y = 495. Returns the window origin.
fn page_layout(
    menu: &mut Menu,
    ctx: &Ctx,
    frame: usize,
    caption: &str,
    picture: Option<Option<&(String, usize)>>,
    text: &[u8],
) -> (i32, i32) {
    let lib = ctx.lib;
    let [w, h] = lib.frame_size(MESSAGES, frame);
    // `DialogsSystem(x0, y0)`: the window is centred on the screen.
    let (x0, y0) = ((1024 - w) / 2, (768 - h) / 2);
    let d = &mut menu.d;
    // The shadow is drawn once straight to the screen before the loop (`ShowGP`).
    d.add_gp_picture(lib, x0, y0, MESSAGES, frame + 1);
    d.add_gp_picture(lib, x0, y0, MESSAGES, frame);
    d.add_text(lib, ctx.fonts, x0 + (w >> 1), y0 + 34, ctx.texts.get(caption), font::F16Y5, Align::Center);
    let mut top = 75;
    if let Some(portrait) = picture {
        // The portrait centred at y = 75, 16 pixels above the separator line.
        let mut ph = 0;
        if let Some((gp, f)) = portrait {
            let [pw, height] = lib.frame_size(gp, *f);
            ph = height + 16;
            d.add_gp_picture(lib, x0 + ((w - pw) >> 1), y0 + 75, gp, *f);
        }
        let [lw, lh] = lib.frame_size(LINE2, 0);
        d.add_gp_button(lib, x0 + ((w - lw) >> 1), y0 + 75 + ph, LINE2, 0, 0);
        top = 75 + ph + lh + 10;
    }
    // The text from x = 55 to the scroll bar at the right edge x = 520, down to y = 495,
    // in `fon18y3` with 21 pixels per line.
    let sw = lib.frame_size(VSCROLL2, 0)[0];
    let height = 495 - top;
    let sb = d.add_scroll_bar(lib, x0 + 520 - sw, y0 + top, height, 1, 0, VSCROLL2, 0);
    let tv = d.add_text_viewer(lib, ctx.fonts, x0 + 55, y0 + top, [520 - sw - 5 - 55, height], text, font::F18Y3);
    let viewer = d.viewer_mut(tv);
    viewer.scroll = Some(sb);
    viewer.set_line_height(21, height);
    (x0, y0)
}

/// One entry of the mission history (`Player+0x1a783`, appended by 0x5624c5): the header
/// (`"<gp> <frame>"` or `NONE`) and the text of a page or question.
pub type HistoryEntry = (String, Vec<u8>);

/// Return values of the history window's buttons (0x5e1d30).
const HISTORY_PREV: i32 = 1;
const HISTORY_NEXT: i32 = 2;
const HISTORY_CLOSE: i32 = 3;

/// The mission history (`UM_HIST` of the "M" drop-down, 0x562bea, `MHISTORY` window
/// 0x5e1d30): one entry at a time on `Interf2\Messages` frames 4/5, from the newest.
/// Zurück shows the older entry, Weiter the newer one; at either end the click shows the
/// same entry again. Schließen, Enter and Escape close it.
pub struct HistoryWindow {
    entries: Rc<[HistoryEntry]>,
    index: usize,
    portrait: Option<(String, usize)>,
    menu: Menu,
}

impl HistoryWindow {
    /// The window on the newest entry; `None` without entries (the row is not offered then).
    pub fn newest(entries: Rc<[HistoryEntry]>) -> Option<Self> {
        let index = entries.len().checked_sub(1)?;
        Some(Self::at(entries, index))
    }

    fn at(entries: Rc<[HistoryEntry]>, index: usize) -> Self {
        let portrait = portrait(&entries[index].0);
        Self { entries, index, portrait, menu: Menu::default() }
    }

    /// The entry after a button (0x562bea): 1 = older, 2 = newer, both kept at the ends.
    fn step(&self, button: i32) -> usize {
        match button {
            HISTORY_PREV => self.index.saturating_sub(1),
            HISTORY_NEXT => (self.index + 1).min(self.entries.len() - 1),
            _ => self.index,
        }
    }
}

impl Screen for HistoryWindow {
    fn dialog(&self) -> Option<&super::super::dialog::Dialog> {
        Some(&self.menu.d)
    }

    fn assets(&self, _ctx: &Ctx) -> Vec<String> {
        let mut files: Vec<String> = [MESSAGES, BUTTON2, VSCROLL2, LINE2].map(str::to_owned).into();
        files.extend(self.portrait.as_ref().map(|(gp, _)| gp.clone()));
        files
    }

    fn build(&mut self, ctx: &Ctx) {
        let text = self.entries[self.index].1.clone();
        let (x0, y0) = page_layout(&mut self.menu, ctx, 4, "MHISTORY", Some(self.portrait.as_ref()), &text);
        let lib = ctx.lib;
        let d = &mut self.menu.d;
        // `addGP_TextButton(x, 520, text, Button2, 1, fon18y5, fon18y5)`, never disabled.
        let buttons =
            [(50, "@MSPREV", HISTORY_PREV), (217, "@MSCANCEL", HISTORY_CLOSE), (384, "@MSNEXT", HISTORY_NEXT)];
        for (x, id, value) in buttons {
            let text = ctx.texts.get(id);
            let b = d.add_gp_text_button(lib, x0 + x, y0 + 520, text, BUTTON2, 1, font::F18Y5, font::F18Y5);
            d.choose(b, value);
        }
    }

    fn input(&mut self, _ctx: &Ctx, input: Input) -> Nav {
        match self.menu.input(input) {
            // Enter and Escape close it (`LastKey` 13 or 27).
            Some(HISTORY_CLOSE | OK | CANCEL) => Nav::Pop,
            // The original rebuilds the window for every entry, also for the same one.
            Some(button @ (HISTORY_PREV | HISTORY_NEXT)) => {
                Nav::Replace(Box::new(Self::at(self.entries.clone(), self.step(button))))
            }
            _ => Nav::Stay,
        }
    }

    fn draw(&self, ctx: &Ctx, out: &mut Vec<Draw>) {
        self.menu.draw(ctx, out);
    }

    fn take_sounds(&mut self) -> Vec<String> {
        self.menu.take_sounds()
    }

    fn in_game(&self) -> bool {
        true
    }
}

/// Fill frames and corners of the multiple choice box (0x480455): `Border` 51..53 and the
/// corner table 0x6686e0.
const CHOICE_FILL: usize = 51;
const CHOICE_CORNERS: [i32; 8] = [43, 44, 45, 46, 49, 50, 47, 48];

/// `AskMultilineQuestion`'s window (0x5ad540).
pub struct ChoiceWindow {
    /// `nx · 64` pixels.
    width: i32,
    orient: u8,
    text: Vec<u8>,
    choices: Vec<Vec<u8>>,
    portrait: Option<(String, usize)>,
    answer: Answer,
    menu: Menu,
    choice_ids: Vec<WidgetId>,
    /// The paper box drawn under the widgets every frame.
    frame: Rect,
}

impl ChoiceWindow {
    pub fn new(width: u32, orient: u8, header: &str, text: Vec<u8>, choices: Vec<Vec<u8>>, answer: Answer) -> Self {
        Self {
            width: (width.min(64) * 64) as i32,
            orient,
            text,
            choices,
            portrait: portrait(header),
            answer,
            menu: Menu::default(),
            choice_ids: Vec::new(),
            frame: Rect::new(0, 0, 0, 0),
        }
    }

    /// The choice under the mouse is red (the font pointer is swapped every frame).
    fn update_fonts(&mut self) {
        for &id in &self.choice_ids {
            let font = if self.menu.d.widgets[id].hover { ROM11_RED } else { ROM11 };
            self.menu.d.viewer_mut(id).set_font(font);
        }
    }
}

impl Screen for ChoiceWindow {
    fn dialog(&self) -> Option<&super::super::dialog::Dialog> {
        Some(&self.menu.d)
    }

    fn assets(&self, _ctx: &Ctx) -> Vec<String> {
        let mut files = vec![BORDER.to_owned()];
        files.extend(self.portrait.as_ref().map(|(gp, _)| gp.clone()));
        files
    }

    fn build(&mut self, ctx: &Ctx) {
        let lib = ctx.lib;
        let lx = self.width;
        // `DialogsSystem((RealLx − Lx) / 2, 80)`; everything moves down later to centre it.
        let (bx, by) = ((1024 - lx) >> 1, 80);
        let orient = if self.orient > 2 { 0 } else { self.orient };
        let d = &mut self.menu.d;
        let (mut tx, mut ty, mut picw, mut ph) = (16, 16, 0, 0);
        if let Some((gp, f)) = &self.portrait {
            let [pw, height] = lib.frame_size(gp, *f);
            ph = height + 16;
            match orient {
                1 => {
                    d.add_gp_picture(lib, bx + 16, by + 16, gp, *f);
                    tx += pw;
                    picw = pw;
                }
                2 => {
                    d.add_gp_picture(lib, bx + lx - 16 - pw, by + 16, gp, *f);
                    picw = pw;
                }
                _ => {
                    d.add_gp_picture(lib, bx + ((lx - pw) >> 1), by + 16, gp, *f);
                    ty += ph + 8;
                }
            }
        }
        let tv = d.add_text_viewer(lib, ctx.fonts, bx + tx, by + ty, [lx - 32 - picw, 1000], &self.text, ROM11_RED);
        d.widgets[tv].enabled = false;
        let viewer = d.viewer_mut(tv);
        viewer.focus = false;
        let th = viewer.text_height();
        ty += if orient != 0 { th.max(ph) } else { th };
        let mut y = ty + 32;
        for (i, choice) in self.choices.iter().enumerate() {
            let x = bx + 32;
            let id = d.add_text_viewer(lib, ctx.fonts, x, by + y, [lx - 64, 300], choice, ROM11);
            let viewer = d.viewer_mut(id);
            viewer.focus = false;
            let h = viewer.text_height();
            d.widgets[id].rect = Rect::inclusive(x, by + y, x + lx - 64 - 1, by + y + h + 4);
            d.choose(id, i as i32);
            self.choice_ids.push(id);
            y += h + 16;
        }
        let total = y + 32;
        let dy = (768 - total) / 2 - 80;
        for id in 0..d.widgets.len() {
            d.shift(id, 0, dy);
        }
        self.frame = Rect::inclusive(bx, dy + 48, bx + lx, total + dy + 80);
        self.update_fonts();
    }

    fn input(&mut self, _ctx: &Ctx, input: Input) -> Nav {
        // Only a click on a choice ends the loop; the keyboard does nothing.
        if matches!(input, Input::Key(_) | Input::Wheel(_)) {
            return Nav::Stay;
        }
        let chosen = self.menu.input(input);
        self.update_fonts();
        match chosen {
            Some(i) if (0..self.choices.len() as i32).contains(&i) => {
                self.answer.set(Some(i as u32));
                Nav::Pop
            }
            _ => Nav::Stay,
        }
    }

    fn backdrop(&self, ctx: &Ctx, out: &mut Vec<Draw>) {
        paper_box(ctx, out, self.frame, CHOICE_FILL, CHOICE_CORNERS, 0);
    }

    fn draw(&self, ctx: &Ctx, out: &mut Vec<Draw>) {
        self.backdrop(ctx, out);
        self.menu.draw(ctx, out);
    }

    fn take_sounds(&mut self) -> Vec<String> {
        self.menu.take_sounds()
    }

    fn in_game(&self) -> bool {
        true
    }
}

/// A box of `Border` paper (0x47fb51): 64-pixel tiles `fill + (i² + j³) % 3` clipped to the
/// box, then the frame of `corners` (0x47f48a). `rect` is inclusive like the original's
/// `x0, y0, x1, y1`.
pub fn paper_box(ctx: &Ctx, out: &mut Vec<Draw>, rect: Rect, fill: usize, corners: [i32; 8], player: u8) {
    let (x0, y0, x1, y1) = (rect.x0, rect.y0, rect.x1 - 1, rect.y1 - 1);
    for i in 0..=((x1 - x0) >> 6) {
        for j in 0..=((y1 - y0) >> 6) {
            let frame = fill + ((i * i + j * j * j) % 3) as usize;
            out.push(Draw::game_frame(BORDER, frame, x0 + 64 * i, y0 + 64 * j, player).clipped(rect));
        }
    }
    crate::hud::border(ctx, out, x0, y0, x1, y1, corners);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_starts_at_the_newest_and_stops_at_the_ends() {
        assert!(HistoryWindow::newest(Rc::from(Vec::new())).is_none());
        let entries: Rc<[HistoryEntry]> =
            Rc::from(vec![("NONE".into(), b"a".to_vec()), ("NONE".into(), b"b".to_vec())]);
        let w = HistoryWindow::newest(entries.clone()).unwrap();
        assert_eq!(w.index, 1);
        assert_eq!(w.step(HISTORY_NEXT), 1);
        assert_eq!(w.step(HISTORY_PREV), 0);
        let w = HistoryWindow::at(entries, 0);
        assert_eq!(w.step(HISTORY_PREV), 0);
        assert_eq!(w.step(HISTORY_NEXT), 1);
    }

    #[test]
    fn page_headers() {
        assert_eq!(portrait("Missions\\Pictures\\UMord 0"), Some(("Missions\\Pictures\\UMord.GP".into(), 0)));
        assert_eq!(portrait("NONE"), None);
        assert_eq!(portrait(""), None);
        assert_eq!(portrait("Missions\\Pictures\\UMord"), None);
    }
}
