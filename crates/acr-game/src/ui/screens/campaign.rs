//! The campaigns screen (0x5ca690): the campaigns of `Missions\Campaigns.txt` in a 5-row
//! list (`Interf2\ListBox` frames 6-8) with the description of the selected one below.
//! Start opens the campaign's missions (0x5c4e30).

use super::super::dialog::{Align, CANCEL, OK, WidgetId};
use super::super::draw::Draw;
use super::super::missions::{self, Campaign};
use super::super::{Ctx, Input, Nav, Screen};
use super::single_mission::SingleMission;
use super::{Menu, font};

const BACKGROUND: &str = "INTERF2\\BACKGROUND_CAMPAIGN.BMP";
const VSCROLL: &str = "INTERF2\\VSCROLL.GP";
const LISTBOX: &str = "INTERF2\\LISTBOX.GP";
const GAMEBUTTONS: &str = "INTERF2\\GAMEBUTTONS.GP";
pub const CAMPAIGNS: &str = "MISSIONS\\CAMPAIGNS.TXT";

#[derive(Default)]
pub struct Campaigns {
    menu: Menu,
    campaigns: Vec<Campaign>,
    list: WidgetId,
    texts: Vec<(WidgetId, WidgetId)>,
    shown: Option<usize>,
    /// The campaign selected when the list opens.
    start: usize,
}

impl Campaigns {
    /// Opens the list with campaign `index` selected (returning from a game).
    pub fn at(index: usize) -> Self {
        Self { start: index, ..Self::default() }
    }

    fn load(ctx: &Ctx) -> Vec<Campaign> {
        ctx.lib.bytes(CAMPAIGNS).map(|b| missions::parse_campaigns(&b)).unwrap_or_default()
    }

    fn select(&mut self) {
        let cur = self.menu.d.list(self.list).cur;
        if self.shown == Some(cur) {
            return;
        }
        self.shown = Some(cur);
        for (i, &(text, bar)) in self.texts.iter().enumerate() {
            for w in [text, bar] {
                self.menu.d.widgets[w].visible = i == cur;
                self.menu.d.widgets[w].enabled = i == cur;
            }
        }
    }
}

impl Screen for Campaigns {
    fn dialog(&self) -> Option<&super::super::dialog::Dialog> {
        Some(&self.menu.d)
    }

    fn assets(&self, ctx: &Ctx) -> Vec<String> {
        let mut files: Vec<String> = [BACKGROUND, VSCROLL, LISTBOX, GAMEBUTTONS, CAMPAIGNS].map(String::from).to_vec();
        files.extend(Self::load(ctx).into_iter().map(|c| c.description));
        files
    }

    fn build(&mut self, ctx: &Ctx) {
        self.campaigns = Self::load(ctx);
        let m = &mut self.menu;
        *m = std::mem::take(m).with_hints();
        m.d.ok_sound = Some(super::super::dialog::sound::START);
        m.d.load_random(ctx.lib);
        m.d.add_picture(ctx.lib, 0, 0, BACKGROUND);
        m.title(ctx, "INTF_CAMPAIGN_T0");
        m.text(ctx, 512, 334, "INTF_CAMPAIGN_W0", font::F16Y5, Align::Center);
        let bar = m.d.add_scroll_bar(ctx.lib, 705, 377 - 13, 156, 100, 0, VSCROLL, 0);
        self.list = m.d.add_list_box(ctx.lib, 311, 377, 5, LISTBOX, 6, 26, font::F18W, font::F18Y3, Some(bar));
        let list = m.d.list_mut(self.list);
        list.focus = true;
        list.dy = -3;
        list.dx -= 1 + 16;
        list.items = self.campaigns.iter().map(|c| c.name.clone()).collect();
        list.cur = self.start.min(list.items.len().saturating_sub(1));
        for campaign in &self.campaigns {
            let bar = m.d.add_scroll_bar(ctx.lib, 755, 338, 290, 1, 0, VSCROLL, 0);
            let text = ctx.lib.bytes(&campaign.description).map(|b| b.to_vec()).unwrap_or_default();
            let viewer = m.d.add_text_viewer(ctx.lib, ctx.fonts, 311, 542, [400, 90], &text, font::F16Y3);
            let v = m.d.viewer_mut(viewer);
            v.set_line_height(21, 89);
            v.scroll = Some(bar);
            v.focus = false;
            for w in [viewer, bar] {
                m.d.widgets[w].visible = false;
                m.d.widgets[w].enabled = false;
            }
            self.texts.push((viewer, bar));
        }
        let start = m.d.add_gp_button(ctx.lib, 899, 706, GAMEBUTTONS, 0, 1);
        m.d.choose(start, OK);
        m.d.set_hint(start, ctx.texts.get("MISS_RUN"));
        let cancel = m.d.add_gp_button(ctx.lib, 959, 706, GAMEBUTTONS, 3, 4);
        m.d.choose(cancel, CANCEL);
        m.d.set_hint(cancel, ctx.texts.get("SOCANCEL"));
        self.select();
    }

    fn tick(&mut self, _ctx: &Ctx) -> Nav {
        self.menu.d.tick(1.0 / super::super::TICK_RATE);
        Nav::Stay
    }

    fn input(&mut self, _ctx: &Ctx, input: Input) -> Nav {
        let chosen = self.menu.input(input);
        self.select();
        match chosen {
            Some(OK) if !self.campaigns.is_empty() => {
                Nav::Push(Box::new(SingleMission::campaign(self.menu.d.list(self.list).cur)))
            }
            Some(CANCEL) => Nav::Pop,
            _ => Nav::Stay,
        }
    }

    fn draw(&self, ctx: &Ctx, out: &mut Vec<Draw>) {
        self.menu.draw(ctx, out);
    }

    fn take_sounds(&mut self) -> Vec<String> {
        self.menu.take_sounds()
    }
}
