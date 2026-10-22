//! Mission texts drawn over the map (see `docs/ui.md`, mission windows): the hint line and
//! the four message lines (`AssignHint`/`AssignHint1`, drawn by 0x493fc5) and the central
//! box (0x4bc490) that shows `ShowCentralText` and the end of the game (0x4bc7b0).
//!
//! Coordinates are on the interface's canvas (`RealLx` × `RealLy`, see
//! [`crate::canvas::Canvas::screen`]).

use crate::hud::BOX_CORNERS;
use crate::ui::Ctx;
use crate::ui::draw::{Draw, Rect};
use crate::ui::fonts::FontId;
use crate::ui::screens::mission::paper_box;

const WHITE: FontId = FontId("WhiteFont");
const BLACK: FontId = FontId("BlackFont");
/// `BigWhiteFont`; the original measures the lines with `BigYellowFont`, the same pack.
const BIG_WHITE: FontId = FontId("BigWhiteFont");

/// The hint line at (`smapx + 4`, `RealLy − PanelLy − 40`) (0x5aa9ec); the message lines
/// go up from 20 pixels above it, 20 pixels apart. `HINT_Y` is relative to `RealLy`.
const HINT_X: i32 = 4;
const HINT_Y: i32 = -144 - 40;

/// Centre of the map view (`smapx + smaplx · 16`, `smapy + smaply · 8`): the middle of the
/// screen.
fn centre(screen: [i32; 2]) -> (i32, i32) {
    (screen[0] / 2, screen[1] / 2)
}
/// Paper of the central box (0x480130): `Border` 28..30.
const CENTRAL_FILL: usize = 28;

/// The hint line and the message list with their remaining frames.
#[derive(Default)]
pub struct Messages {
    /// `AssignHint` (0x494283).
    hint: (Vec<u8>, u32),
    /// `AssignHint1` (0x4942db): the newest first.
    list: [(Vec<u8>, u32); 4],
}

impl Messages {
    pub fn add(&mut self, text: Vec<u8>, frames: u32, list: bool) {
        if !list {
            self.hint = (text, frames);
        } else if self.list[0].0 == text {
            // The same text again only renews its time.
            self.list[0].1 = frames;
        } else {
            self.list.rotate_right(1);
            self.list[0] = (text, frames);
        }
    }

    /// One frame passed (0x4944a9 counts every timer down once per drawn frame).
    pub fn tick(&mut self) {
        for (_, t) in std::iter::once(&mut self.hint).chain(&mut self.list) {
            *t = t.saturating_sub(1);
        }
    }

    /// `BlackFont` 2 pixels down-right as a shadow, then `WhiteFont`.
    pub fn draw(&self, screen: [i32; 2], out: &mut Vec<Draw>) {
        let hint_y = screen[1] + HINT_Y;
        let mut line = |text: &[u8], y: i32| {
            out.push(Draw::game_text(BLACK, text, HINT_X + 2, y + 2));
            out.push(Draw::game_text(WHITE, text, HINT_X, y));
        };
        if self.hint.1 > 0 {
            line(&self.hint.0, hint_y);
        }
        for (i, (text, t)) in self.list.iter().enumerate() {
            if *t > 0 {
                line(text, hint_y - 20 - 20 * i as i32);
            }
        }
    }
}

/// The central box (0x4bc490): `\` separates lines, drawn centred in `BigWhiteFont` on a
/// paper box. The box is sized for as many lines as there are `\`, one fewer than drawn.
pub fn central_text(ctx: &Ctx, out: &mut Vec<Draw>, text: &[u8], player: u8, screen: [i32; 2]) {
    let width = |t: &[u8]| ctx.fonts.width(ctx.lib, BIG_WHITE, t);
    let n = text.iter().filter(|&&c| c == b'\\').count() as i32;
    let max = text.split(|&c| c == b'\\').map(width).max().unwrap_or(0);
    let lh = ctx.fonts.glyph_height(ctx.lib, BIG_WHITE, b'y') + 5;
    let (cx, mut cy) = centre(screen);
    cy -= ((lh * n) >> 1) + 32;
    let half = max >> 1;
    let rect = Rect::inclusive(cx - half - 64, cy - 8, cx + half + 64, cy + lh * n + 27);
    paper_box(ctx, out, rect, CENTRAL_FILL, BOX_CORNERS, player);
    for line in text.split(|&c| c == b'\\') {
        out.push(Draw::game_text(BIG_WHITE, line, cx - (width(line) >> 1), cy - 9));
        cy += lh;
    }
}
