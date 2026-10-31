//! The single missions screen (0x5d66b0) and the missions of a campaign (0x5c4e30).
//!
//! The campaign form uses the campaign's background from `Campaigns.txt`, a 10-row list of
//! the campaign's missions up to the last one the player has opened (0x5c5432, see
//! [`progress`]), previews next to the briefings (`.txt` -> `.bmp`) and the title
//! `INTF_CMISS_T0`.
//!
//! A list of the missions of `Missions\SingleMiss.txt` (17 rows, `Interf2\ListBox`
//! highlight frames 3-5) with its scroll bar, the map preview `Maps2\single<n>.bmp`
//! (400x224 at (588, 120)) under the `Interf2\GameMap` frame, the mission's briefing in a
//! text viewer, the difficulty combo box, and the swords (start, `Interf2\GameButtons`
//! 0/1) and cancel (3/4) buttons. The chosen difficulty is kept in `smiss.opt` by the
//! original; here it lives as long as the program.

use super::super::dialog::{Align, CANCEL, OK, WidgetId};
use super::super::draw::Draw;
use super::super::missions::{self, Mission};
use super::super::setup::{Difficulty, GameKind, GameSetup, MissionSetup};
use super::super::{Ctx, Input, Nav, Screen};
use super::{Menu, font};
use crate::acr_missions::AcrMission;

const BACKGROUND: &str = "INTERF2\\SINGLE_MISSION_BACKGROUND.BMP";
const VSCROLL: &str = "INTERF2\\VSCROLL.GP";
const COMBO: &str = "INTERF2\\COMBO.GP";
const LISTBOX: &str = "INTERF2\\LISTBOX.GP";
const GAMEMAP: &str = "INTERF2\\GAMEMAP.GP";
const GAMEBUTTONS: &str = "INTERF2\\GAMEBUTTONS.GP";
pub const MISSIONS: &str = "MISSIONS\\MISSIONS.TXT";
const SINGLE: &str = "MISSIONS\\SINGLEMISS.TXT";
/// The progress of players without a file of their own.
const STANDARD_PLAYER: &str = "PLAYERS\\STANDART PLAYER INFO.TXT";

/// The progress file of player `name` (CP1252).
pub fn player_file(name: &[u8]) -> String {
    format!("PLAYERS\\{}.TXT", acr_formats::CodePage::Cp1252.decode(name))
}

/// The current player's campaign progress: kept since a victory, else the player's file
/// (written by us, else the installation's), else `standart player info.txt`, else the `ON`
/// missions of `Campaigns.txt` (0x56af11).
pub fn progress(ctx: &Ctx) -> missions::Progress {
    if let Some(p) = &ctx.profile.borrow().progress {
        return p.clone();
    }
    let campaigns =
        ctx.lib.bytes(super::campaign::CAMPAIGNS).map(|b| missions::parse_campaigns(&b)).unwrap_or_default();
    let name = ctx.profile.borrow().name.clone();
    let written = ctx.profiles.read(&acr_formats::CodePage::Cp1252.decode(&name));
    let data = written.or_else(|| ctx.lib.bytes(&player_file(&name)).map(|b| b.to_vec()));
    match data.or_else(|| ctx.lib.bytes(STANDARD_PLAYER).map(|b| b.to_vec())) {
        Some(data) => missions::Progress::read(&data, &campaigns),
        None => missions::Progress::new(&campaigns),
    }
}

/// Text viewer of the briefing: position, size, line height and its scroll bar.
const TEXT: (i32, i32, [i32; 2]) = (605, 445, [342, 226]);
const TEXT_LINE_H: i32 = 21;

#[derive(Default)]
pub struct SingleMission {
    /// Index into `Campaigns.txt` for the missions of a campaign.
    campaign: Option<usize>,
    /// The mission selected when the list opens.
    start: usize,
    menu: Menu,
    missions: Vec<Mission>,
    /// ACR's own missions, listed after the original's (single missions only).
    acr: Vec<AcrMission>,
    /// The ACR mission selected when the list opens.
    start_acr: Option<AcrMission>,
    list: WidgetId,
    difficulty: WidgetId,
    preview: WidgetId,
    /// Per mission: text viewer and its scroll bar; only the selected pair is shown.
    texts: Vec<(WidgetId, WidgetId)>,
    shown: Option<usize>,
}

impl SingleMission {
    /// The missions of campaign `index` (`Missions\Campaigns.txt`).
    pub fn campaign(index: usize) -> Self {
        Self { campaign: Some(index), ..Self::default() }
    }

    /// Opens the list with mission `index` selected (returning from a game).
    pub fn at(mut self, index: usize) -> Self {
        self.start = index;
        self
    }

    /// Opens the list with ACR's mission `mission` selected (returning from a game).
    pub fn at_acr(mut self, mission: AcrMission) -> Self {
        self.start_acr = Some(mission);
        self
    }

    /// The missions in menu order: the single missions, or the campaign's missions up to the
    /// last open one.
    fn load(&self, ctx: &Ctx) -> Vec<Mission> {
        let Some(all) = ctx.lib.bytes(MISSIONS) else { return Vec::new() };
        let all = missions::parse_missions(&all);
        let ids: Vec<String> = match self.campaign {
            None => ctx.lib.bytes(SINGLE).map(|b| missions::parse_single(&b)).unwrap_or_default(),
            Some(c) => {
                let shown = progress(ctx).shown(c);
                self.campaign_info(ctx)
                    .map(|camp| camp.missions.iter().take(shown).map(|m| m.id.clone()).collect())
                    .unwrap_or_default()
            }
        };
        ids.iter().filter_map(|id| missions::find(&all, id).cloned()).collect()
    }

    fn campaign_info(&self, ctx: &Ctx) -> Option<missions::Campaign> {
        let data = ctx.lib.bytes(super::campaign::CAMPAIGNS)?;
        missions::parse_campaigns(&data).into_iter().nth(self.campaign?)
    }

    fn background(&self, ctx: &Ctx) -> String {
        self.campaign_info(ctx).map_or(BACKGROUND.to_owned(), |c| c.background)
    }

    /// The preview of mission `index`: `Maps2\single<n>.bmp` for single missions, the
    /// briefing's `.bmp` for campaign missions (0x5c5893).
    fn preview(&self, index: usize) -> String {
        match self.campaign {
            // ACR's missions have no picture of the original's.
            None if index >= self.missions.len() => String::new(),
            None => format!("MAPS2\\SINGLE{}.BMP", index + 1),
            Some(_) => self.missions.get(index).map_or(String::new(), |m| {
                let b = m.briefing();
                let stem = b.rfind('.').map_or(b, |dot| &b[..dot]);
                format!("{stem}.BMP")
            }),
        }
    }

    /// Shows the preview and briefing of the selected mission.
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
        let preview = self.preview(cur);
        if let super::super::dialog::Kind::Picture { path } = &mut self.menu.d.widgets[self.preview].kind {
            *path = preview;
        }
    }

    fn setup(&self) -> Option<GameSetup> {
        let cur = self.menu.d.list(self.list).cur;
        if let Some(&mission) = cur.checked_sub(self.missions.len()).and_then(|i| self.acr.get(i)) {
            return Some(GameSetup {
                kind: GameKind::Acr(mission),
                players: mission.players(),
                local_player: 0,
                rules: Default::default(),
            });
        }
        let m = self.missions.get(cur)?;
        let difficulty = Difficulty::from_index(self.menu.d.combo(self.difficulty).cur);
        Some(GameSetup {
            kind: GameKind::Mission(MissionSetup {
                id: m.id.clone(),
                name: m.name.clone(),
                map: m.map.clone(),
                script: m.script.clone(),
                texts: m.description.clone(),
                difficulty,
                campaign: self.campaign.map(|c| (c, self.menu.d.list(self.list).cur)),
            }),
            players: Vec::new(),
            local_player: 0,
            rules: Default::default(),
        })
    }
}

impl Screen for SingleMission {
    fn dialog(&self) -> Option<&super::super::dialog::Dialog> {
        Some(&self.menu.d)
    }

    fn assets(&self, ctx: &Ctx) -> Vec<String> {
        let mut files: Vec<String> =
            [VSCROLL, COMBO, LISTBOX, GAMEMAP, GAMEBUTTONS, MISSIONS, SINGLE, super::campaign::CAMPAIGNS]
                .map(String::from)
                .to_vec();
        if self.campaign.is_some() {
            files.push(player_file(&ctx.profile.borrow().name));
            files.push(STANDARD_PLAYER.to_owned());
        }
        files.push(self.background(ctx));
        files.extend(self.load(ctx).iter().map(|m| m.briefing().to_owned()));
        files
    }

    fn build(&mut self, ctx: &Ctx) {
        self.missions = self.load(ctx);
        self.acr = if self.campaign.is_none() { AcrMission::ALL.to_vec() } else { Vec::new() };
        if let Some(m) = self.start_acr.take()
            && let Some(i) = self.acr.iter().position(|&a| a == m)
        {
            self.start = self.missions.len() + i;
        }
        let background = self.background(ctx);
        let preview = self.preview(0);
        let m = &mut self.menu;
        *m = std::mem::take(m).with_hints();
        m.d.load_random(ctx.lib);
        m.d.add_picture(ctx.lib, 0, 0, &background);
        self.preview = m.d.add_picture(ctx.lib, 588, 120, &preview);
        m.d.add_gp_picture(ctx.lib, 580, 115, GAMEMAP, 0);
        let bar = m.d.add_scroll_bar(ctx.lib, 495, 136, 476, 1, 0, VSCROLL, 0);
        let rows = if self.campaign.is_some() { 10 } else { 17 };
        self.list = m.d.add_list_box(ctx.lib, 94, 156, rows, LISTBOX, 3, 26, font::F18W, font::F18Y3, Some(bar));
        let list = m.d.list_mut(self.list);
        list.focus = true;
        list.dy = -3;
        list.dx -= 16;
        list.items = self.missions.iter().map(|m| m.name.clone()).chain(self.acr.iter().map(|m| m.name())).collect();
        list.cur = self.start.min(list.items.len().saturating_sub(1));
        self.difficulty = m.d.add_combo_box(ctx.lib, 213, 638, COMBO, 0, 9, font::F18W, font::F18Y3);
        let combo = m.d.combo_mut(self.difficulty);
        combo.drop_dx -= 5;
        combo.font_dy -= 2;
        combo.one_dy -= 1;
        combo.cur = 1;
        combo.lines = ["@RMID_EASY", "@RMID_NORMAL", "@RMID_HARD", "@RMID_VERYHARD", "@RMID_IMPOSIBLE"]
            .iter()
            .map(|id| ctx.texts.get(id).to_vec())
            .collect();
        m.d.set_hint(self.difficulty, ctx.texts.get("DIFFICULT5"));
        let briefings = self
            .missions
            .iter()
            .map(|m| ctx.lib.bytes(m.briefing()).map(|b| b.to_vec()).unwrap_or_default())
            .chain(self.acr.iter().map(|m| m.briefing()))
            .collect::<Vec<_>>();
        for text in briefings {
            let (x, y, size) = TEXT;
            let viewer = m.d.add_text_viewer(ctx.lib, ctx.fonts, x, y, size, &text, font::F18Y3);
            m.d.viewer_mut(viewer).set_line_height(TEXT_LINE_H, size[1] - 1);
            let bar = m.d.add_scroll_bar(ctx.lib, 957, 434, 238, 1, 0, VSCROLL, 0);
            m.d.viewer_mut(viewer).scroll = Some(bar);
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
        let (cancel_hint, title) =
            if self.campaign.is_some() { ("WAR_CANCEL", "INTF_CMISS_T0") } else { ("SOCANCEL", "INTF_SMISS_T0") };
        m.d.set_hint(cancel, ctx.texts.get(cancel_hint));
        m.title(ctx, title);
        if self.campaign.is_some() {
            m.d.ok_sound = Some(super::super::dialog::sound::START);
        }
        m.text(ctx, 303, 107, "INTF_SMISS_T1", font::F16Y5, Align::Center);
        m.text(ctx, 787, 106, "INTF_SMISS_T2", font::F16Y5, Align::Center);
        m.text(ctx, 787, 401, "INTF_SMISS_T3", font::F16Y5, Align::Center);
        m.text(ctx, 98, 640, "INTF_SMISS_DIFF", font::F18Y3, Align::Left);
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
            Some(OK) => self.setup().map_or(Nav::Stay, |s| Nav::Start(Box::new(s))),
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
