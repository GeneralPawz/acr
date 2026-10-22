//! Screens that are not playable yet: the original background and title, nothing else.
//! Escape or a click goes back.

use super::super::dialog::{Align, Dialog};
use super::super::draw::Draw;
use super::super::fonts::FontId;
use super::super::{Ctx, Input, Key, Nav, Screen};

pub struct Placeholder {
    background: &'static str,
    title: &'static str,
    dialog: Dialog,
}

impl Placeholder {
    pub fn new(background: &'static str, title: &'static str) -> Self {
        Self { background, title, dialog: Dialog::default() }
    }

    /// The connection type screen of multiplayer games (0x4a28b0).
    pub fn multiplayer() -> Self {
        Self { background: "INTERF2\\BACKGROUND_TYPE_CONNECTION.BMP", title: "INTF_PROT_T0", dialog: Dialog::default() }
    }
}

impl Screen for Placeholder {
    fn assets(&self, _ctx: &Ctx) -> Vec<String> {
        vec![self.background.into()]
    }

    fn build(&mut self, ctx: &Ctx) {
        self.dialog.add_picture(ctx.lib, 0, 0, self.background);
        let font = FontId("fon40y5");
        self.dialog.add_text_button(ctx.lib, ctx.fonts, 512, 18, ctx.texts.get(self.title), [font; 3], Align::Center);
    }

    fn input(&mut self, _ctx: &Ctx, input: Input) -> Nav {
        match input {
            Input::Key(Key::Escape) | Input::MouseDown(_) => Nav::Pop,
            _ => Nav::Stay,
        }
    }

    fn draw(&self, ctx: &Ctx, out: &mut Vec<Draw>) {
        self.dialog.draw(ctx.lib, ctx.fonts, out);
    }
}
