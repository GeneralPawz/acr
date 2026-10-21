//! Shown after "start" until the in-game screen takes over.

use super::super::dialog::Dialog;
use super::super::draw::Draw;
use super::super::{Ctx, Input, Nav, Screen};

const BACKGROUND: &str = "INTERF2\\MISSION_LOADING.BMP";

#[derive(Default)]
pub struct Loading {
    dialog: Dialog,
}

impl Screen for Loading {
    fn assets(&self, _ctx: &Ctx) -> Vec<String> {
        vec![BACKGROUND.into()]
    }

    fn build(&mut self, ctx: &Ctx) {
        self.dialog.add_picture(ctx.lib, 0, 0, BACKGROUND);
    }

    fn input(&mut self, _ctx: &Ctx, _input: Input) -> Nav {
        Nav::Stay
    }

    fn draw(&self, ctx: &Ctx, out: &mut Vec<Draw>) {
        self.dialog.draw(ctx.lib, ctx.fonts, out);
    }
}
