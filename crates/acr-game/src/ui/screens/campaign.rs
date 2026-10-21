//! Work in progress.

use super::super::draw::Draw;
use super::super::{Ctx, Input, Nav, Screen};
use super::placeholder::Placeholder;

pub struct Campaigns(Placeholder);

impl Default for Campaigns {
    fn default() -> Self {
        Self(Placeholder::new("INTERF2\\BACKGROUND_CAMPAIGN.BMP", "INTF_CAMPAIGN_T0"))
    }
}

impl Screen for Campaigns {
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
