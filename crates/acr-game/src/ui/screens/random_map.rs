//! Work in progress.

use super::super::draw::Draw;
use super::super::{Ctx, Input, Nav, Screen};
use super::placeholder::Placeholder;

pub struct RandomMap(Placeholder);

impl Default for RandomMap {
    fn default() -> Self {
        Self(Placeholder::new("INTERF2\\RANDOM_MAP_BACKGROUND.BMP", "INTF_CROOM_SCR1"))
    }
}

impl Screen for RandomMap {
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
