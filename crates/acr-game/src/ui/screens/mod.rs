//! The menu screens.

pub mod campaign;
pub mod game_menu;
pub mod game_options;
pub mod loading;
pub mod main_menu;
pub mod mission;
pub mod options;
pub mod placeholder;
pub mod player_name;
pub mod random_map;
pub mod single_mission;

use super::dialog::{Align, Dialog, WidgetId};
use super::draw::Draw;
use super::fonts::FontId;
use super::{Ctx, Input};

/// Fonts of the menus (global `RLCFont` objects of the original).
pub mod font {
    use super::FontId;

    pub const G60W: FontId = FontId("fonG60w");
    pub const G60Y5: FontId = FontId("fonG60y5");
    pub const TITLE: FontId = FontId("fon40y5");
    pub const F18W: FontId = FontId("fon18w");
    pub const F18Y3: FontId = FontId("fon18y3");
    pub const F18Y5: FontId = FontId("fon18y5");
    pub const F16Y1: FontId = FontId("fon16y1");
    pub const F16Y3: FontId = FontId("fon16y3");
    pub const F16Y5: FontId = FontId("fon16y5");
    pub const F13Y2: FontId = FontId("fon13y2");
    pub const SMALL_YELLOW: FontId = FontId("SmallYellowFont");
    pub const SMALL_YELLOW1: FontId = FontId("SmallYellowFont1");
}

/// Where a screen shows the hint of the hovered widget: an `addTextButton` at
/// (440, 745) in `SmallYellowFont`, centred, whose text the menu loop replaces with the
/// dialog's current hint every frame (e.g. 0x5d7265).
const HINT_POS: (i32, i32) = (440, 745);

/// A screen's dialog plus the plumbing every menu loop shares.
#[derive(Default)]
pub struct Menu {
    pub d: Dialog,
    sounds: Vec<String>,
    show_hints: bool,
}

impl Menu {
    /// Shows hints of hovered widgets in the status line.
    pub fn with_hints(mut self) -> Self {
        self.show_hints = true;
        self
    }

    /// Feeds input to the widgets; returns the chosen `param` (`ItemChoose`), if any.
    pub fn input(&mut self, input: Input) -> Option<i32> {
        let out = match input {
            Input::MouseMove(p) => self.d.mouse_move(p),
            Input::MouseDown(p) => self.d.press(p),
            Input::MouseUp(_) => {
                self.d.release();
                Default::default()
            }
            Input::Key(k) => self.d.key(k),
            Input::Wheel(notches) => {
                self.d.wheel(notches);
                Default::default()
            }
        };
        self.sounds.extend(out.sounds);
        out.chosen
    }

    pub fn draw(&self, ctx: &Ctx, out: &mut Vec<Draw>) {
        self.d.draw(ctx.lib, ctx.fonts, out);
        if self.show_hints
            && let Some(hint) = self.d.hint()
        {
            let width = ctx.fonts.width(ctx.lib, font::SMALL_YELLOW, hint);
            out.push(Draw::text(font::SMALL_YELLOW, hint, HINT_POS.0 - width / 2, HINT_POS.1));
        }
    }

    pub fn take_sounds(&mut self) -> Vec<String> {
        std::mem::take(&mut self.sounds)
    }

    /// A text button that reports `param`, as the menus build them.
    #[allow(clippy::too_many_arguments)]
    pub fn entry(
        &mut self,
        ctx: &Ctx,
        x: i32,
        y: i32,
        text: &[u8],
        fonts: [FontId; 3],
        align: Align,
        param: i32,
    ) -> WidgetId {
        let w = self.d.add_text_button(ctx.lib, ctx.fonts, x, y, text, fonts, align);
        self.d.choose(w, param);
        w
    }

    /// A static text.
    pub fn text(&mut self, ctx: &Ctx, x: i32, y: i32, id: &str, font: FontId, align: Align) -> WidgetId {
        self.d.add_text(ctx.lib, ctx.fonts, x, y, ctx.texts.get(id), font, align)
    }

    /// The standard title of a screen: `fon40y5`, centred at (512, 18).
    pub fn title(&mut self, ctx: &Ctx, id: &str) -> WidgetId {
        self.text(ctx, 512, 18, id, font::TITLE, Align::Center)
    }
}
