//! The diplomacy panel: DipServer's tribe dialogs (see `docs/ui.md`, "Diplomacy panel").
//!
//! The original shows them in the side panel of the game screen (mode 4 of `0x1100b54`,
//! drawn by 0x4ce310 and `PopupDipDialog` 0x5c3840) with `intExplorer.dll`, a browser of CML
//! pages that builds its widgets with the `DialogsSystem` API. DipServer writes the pages
//! (`Dip\tmp.cml`) and answers the requests of their links (`{DS|perform&1&0&2}`); this module
//! lays the pages out and draws them like intExplorer, and hands the requests to
//! [`crate::mission::MissionLink::dip_request`]. The game goes on while the panel is open.

pub mod cml;
mod page;

use acr_mission::diplomacy::{DipCommand, parse_commands};

use crate::hud::font;
use crate::ui::Ctx;
use crate::ui::draw::{Draw, Rect};
use crate::ui::screens::mission::paper_box;
pub use page::Page;

/// Panel width `0x66a8f8` (259) and the layout of 0x5aa5ac for `RealLy` ≥ 768:
/// `0x66a8f4` = 16, `0xff3010` = 48, `0x10bb018` = 466.
const PANEL_W: i32 = 259;
const PANEL_TOP: i32 = 16 + 0x20;
const PANEL_Y2: i32 = 16 + 0x1c2;
/// The panel box (0x4ce45a → 0x4803fb): paper from (`RealLx − W`, 48) to (`RealLx − 16`,
/// `0x10bb018 − 25`) with corner table 0x668660, a bottom strip down to `0x10bb018 + 3` with
/// corner table 0x6686c0 (`Border` frames).
fn panel(screen: [i32; 2]) -> Rect {
    Rect { x0: screen[0] - PANEL_W, y0: PANEL_TOP, x1: screen[0] - 16 + 1, y1: PANEL_Y2 + 3 + 1 }
}
const PAPER_BOTTOM: i32 = PANEL_Y2 - 0x19;
/// Corner table 0x668660: frame 87 is the top right corner with the close button.
const TOP_CORNERS: [i32; 8] = [17, 87, -1, -1, 12, 36, 23, -1];
/// Corner table 0x6686c0.
const BOTTOM_CORNERS: [i32; 8] = [3, 86, 6, 8, 12, 14, 10, 11];
/// `Border` paper tiles 28..30 (0x4803fb passes 0x1c, 3).
const PAPER: usize = 28;
/// Close: a press strictly inside (`RealLx − 42`..`RealLx − 16`, 43..69) (0x4ce4a4).
fn close(screen: [i32; 2]) -> (i32, i32, i32, i32) {
    (screen[0] - 42, 16 + 27, screen[0] - 16, 16 + 53)
}
/// The browser window (0x5c3770): `x0 = RealLx − W − 1` (`DialogsSystem` base), y0 = 48 + 31;
/// `RunExplorer(1, page, x0 − 2, y0 − 2, x0 + W − 7, RealLy − 298)`.
fn base(screen: [i32; 2]) -> (i32, i32) {
    (screen[0] - PANEL_W - 1, PANEL_TOP + 0x1f)
}
pub fn window(screen: [i32; 2]) -> Rect {
    let (x, y) = base(screen);
    Rect { x0: x - 2, y0: y - 2, x1: x + PANEL_W - 7 + 1, y1: screen[1] - 0x12a + 1 }
}

/// What the panel wants from the game after input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DipAction {
    /// Send this request to DipServer (`perform&1&0&2`).
    Request(Vec<u8>),
}

pub struct DipPanel {
    /// The layout size (`RealLx`, `RealLy`).
    screen: [i32; 2],
    /// `0x1100a74 == 1 && 0x1100b54 == 4`: the panel shows the diplomacy dialog.
    open: bool,
    /// The page shown and the file it came from.
    page: Option<Page>,
    /// A page has been shown since the start (F8 otherwise asks for the contact list).
    started: bool,
}

impl Default for DipPanel {
    fn default() -> Self {
        Self { screen: crate::hud::SCREEN, open: false, page: None, started: false }
    }
}

impl DipPanel {
    /// Lays the panel out for a screen of `screen` canvas pixels; a shown page is laid out
    /// again when the size changes.
    pub fn set_screen(&mut self, screen: [i32; 2]) {
        if screen != self.screen {
            self.screen = screen;
            if let Some(page) = &self.page {
                self.page = Some(Page::new(page.text(), window(screen)));
            }
        }
    }

    /// `ActivateDipDialog`: opens the panel and executes the browser commands.
    pub fn activate(&mut self, command: &[u8], load: &dyn Fn(&str) -> Option<Vec<u8>>) {
        self.open = true;
        self.execute(&parse_commands(command), load);
    }

    /// Executes the commands of an answer: `LW_file&path` shows a page; `LW_new` starts a new
    /// page history (ACR keeps none).
    pub fn execute(&mut self, commands: &[DipCommand], load: &dyn Fn(&str) -> Option<Vec<u8>>) {
        for c in commands {
            if c.name.eq_ignore_ascii_case(b"LW_file")
                && let Some(path) = c.args.first()
            {
                let path = String::from_utf8_lossy(path).into_owned();
                match load(&path) {
                    Some(text) => {
                        let text = match &self.page {
                            Some(page) => page::merge_section(page.text(), &text),
                            None => text,
                        };
                        self.page = Some(Page::new(&text, window(self.screen)));
                        self.started = true;
                    }
                    None => log::warn!("diplomacy page {path} not found"),
                }
            }
        }
    }

    /// F8 (0x5c3a60): opens or closes the panel. Returns a request to make when the panel
    /// opens without a page (ACR asks for the list of allied tribes; the original would
    /// show its start page `LF|LW_new|open&req1.txt` of `LF_Server.dll`).
    pub fn toggle(&mut self) -> Option<DipAction> {
        self.open = !self.open;
        (self.open && !self.started).then(|| DipAction::Request(b"cnlist".to_vec()))
    }

    /// Whether the canvas point is over the open panel.
    pub fn covers(&self, p: [f32; 2]) -> bool {
        self.open && panel(self.screen).contains(p)
    }

    pub fn mouse_moved(&mut self, p: [f32; 2]) {
        if let Some(page) = &mut self.page {
            page.mouse_moved(p);
        }
    }

    /// Left button pressed at canvas point `p` over the panel: the request of a clicked link
    /// and the sounds to play.
    pub fn press(&mut self, ctx: &Ctx, p: [f32; 2]) -> (Option<DipAction>, Vec<String>) {
        let (x0, y0, x1, y1) = close(self.screen);
        if p[0] > x0 as f32 && p[0] < x1 as f32 && p[1] > y0 as f32 && p[1] < y1 as f32 {
            self.open = false;
            return (None, Vec::new());
        }
        let Some(page) = &mut self.page else { return (None, Vec::new()) };
        let (request, sounds) = page.press(ctx, p);
        (request.map(DipAction::Request), sounds)
    }

    pub fn release(&mut self) {
        if let Some(page) = &mut self.page {
            page.release();
        }
    }

    /// Time passes: `#time` requests that are due.
    pub fn update(&mut self, seconds: f64) -> Option<DipAction> {
        if !self.open {
            return None;
        }
        self.page.as_mut()?.update(seconds).map(DipAction::Request)
    }

    /// The panel box, the header and the page (canvas coordinates).
    pub fn draw(&mut self, ctx: &Ctx, out: &mut Vec<Draw>) {
        if !self.open {
            return;
        }
        let panel = panel(self.screen);
        let top = Rect { y1: PAPER_BOTTOM + 1, ..panel };
        paper_box(ctx, out, top, PAPER, TOP_CORNERS, 0);
        crate::hud::border(ctx, out, panel.x0, PAPER_BOTTOM, panel.x1 - 1, panel.y1 - 1, BOTTOM_CORNERS);
        // `DIP_HDR` in `YellowFont` at (base + 10, base − 29) (0x5c37e5).
        let (x, y) = base(self.screen);
        out.push(Draw::game_text(font::YELLOW, ctx.texts.get("DIP_HDR"), x + 10, y - 29));
        if let Some(page) = &mut self.page {
            page.draw(ctx, out);
        }
    }
}
