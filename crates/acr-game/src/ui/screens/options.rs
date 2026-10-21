//! Work in progress.

use super::super::draw::Draw;
use super::super::{Ctx, Input, Nav, Screen};
use super::placeholder::Placeholder;

pub struct Options(Placeholder);

impl Default for Options {
    fn default() -> Self {
        Self(Placeholder::new("INTERF2\\BACKGROUND_OPTIONS.BMP", "INTF_OPT_TITLE0"))
    }
}

impl Screen for Options {
    fn assets(&self, ctx: &Ctx) -> Vec<String> {
        self.0.assets(ctx)
    }

    fn build(&mut self, ctx: &Ctx) {
        self.0.build(ctx)
    }

    fn input(&mut self, ctx: &Ctx, input: Input) -> Nav {
        self.0.input(ctx, input)
    }

    fn draw(&self, ctx: &Ctx, out: &mut Vec<Draw>) {
        self.0.draw(ctx, out)
    }
}
