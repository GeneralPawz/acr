//! The load and save dialogs (#78).
//!
//! - Load from the main menu (0x5d8090): `Interf2\Background_Load_Game.bmp`, page tabs
//!   `INTF_LOAD_P0` (savegames) and `INTF_LOAD_P1` (recordings), a list of names
//!   (446, 282) and one of dates (714, 282), 8 rows of `Interf2\LoadGame`, sharing the
//!   scroll bar (901, 266).
//! - Load and save over the game (0x5d8cd0, 0x5d9400): `Interf2\G_Load_Game` frames 4
//!   (shadow) and 5 at the game menu's origin − (60, 40), 9 rows of names and dates; the
//!   save dialog has a name field instead of the chosen game's name.
//!
//! The lists show `Save\*.sav`, newest first, as `name.sav` and
//! `%02d:%02d:%02d   %02d.%02d.%d` (0x5d7c10, 0x5d79c0). Neither dialog can delete a save.

use acr_formats::CodePage;

use super::super::dialog::{Align, CANCEL, Kind, OK, WidgetId};
use super::super::draw::Draw;
use super::super::{Ctx, Input, Nav, Screen};
use super::{Menu, font};
use crate::savegame::{SaveGame, SaveInfo};

const BACKGROUND: &str = "INTERF2\\BACKGROUND_LOAD_GAME.BMP";
const BUTTONS: &str = "INTERF2\\BUTTONS.GP";
const VSCROLL: &str = "INTERF2\\VSCROLL.GP";
const LOADGAME: &str = "INTERF2\\LOADGAME.GP";
const G_LOAD_GAME: &str = "INTERF2\\G_LOAD_GAME.GP";
const GSCROLL: &str = "INTERF2\\GSCROLL.GP";
/// At most this many savegames are listed (0x5d7c10).
const MAX_SAVES: usize = 100;
/// The name field holds 35 bytes including the terminator (0x5d9910).
const NAME_CAPACITY: usize = 35;

/// A savegame in the lists: its store name (file name without `.sav`) and header.
#[derive(Debug, Clone)]
pub struct Entry {
    pub file: String,
    pub info: SaveInfo,
}

/// The savegames of the store, newest first.
pub fn entries(ctx: &Ctx) -> Vec<Entry> {
    let mut out: Vec<Entry> = ctx
        .saves
        .list()
        .into_iter()
        .filter_map(|file| {
            let info = SaveInfo::peek(&ctx.saves.read(&file)?)?;
            Some(Entry { file, info })
        })
        .collect();
    out.sort_by(|a, b| b.info.time.unix.cmp(&a.info.time.unix).then_with(|| a.file.cmp(&b.file)));
    out.truncate(MAX_SAVES);
    out
}

/// A store name as the game's code page shows it.
fn shown(file: &str) -> Vec<u8> {
    CodePage::Cp1252.encode(file).unwrap_or_else(|| file.bytes().map(|b| if b.is_ascii() { b } else { b'?' }).collect())
}

/// The store name for a typed (CP1252) name: characters a file name cannot hold become `_`.
pub fn file_name(name: &[u8]) -> String {
    CodePage::Cp1252
        .decode(name)
        .chars()
        .map(|c| if c.is_control() || "\\/:*?\"<>|".contains(c) { '_' } else { c })
        .collect::<String>()
        .trim()
        .to_owned()
}

/// The dialog origin over the game at 1024 × 768 (`(RealLx − 470) >> 1`,
/// `(RealLy − 415) >> 1`), and the frame offset (−60, −40).
fn game_origin() -> (i32, i32) {
    ((1024 - 470) >> 1, (768 - 415) >> 1)
}
const FRAME_DX: i32 = -60;
const FRAME_DY: i32 = -40;

/// Names and dates of one page.
#[derive(Default, Clone, Copy)]
struct Lists {
    names: WidgetId,
    dates: WidgetId,
    bar: WidgetId,
}

/// Adds a list pair: names (with the row highlight) and dates (none).
#[allow(clippy::too_many_arguments)]
fn add_lists(
    m: &mut Menu,
    ctx: &Ctx,
    (x, y): (i32, i32),
    dates_x: i32,
    rows: usize,
    gp: &str,
    frame: i32,
    bar: WidgetId,
    name_clip: Option<super::super::draw::Rect>,
) -> Lists {
    m.d.set_clip(name_clip);
    let names = m.d.add_list_box(ctx.lib, x, y, rows, gp, frame, 26, font::F18W, font::F18Y3, Some(bar));
    m.d.set_clip(None);
    let dates = m.d.add_list_box(ctx.lib, dates_x, y, rows, gp, -3, 26, font::F18W, font::F18Y3, Some(bar));
    for l in [names, dates] {
        let list = m.d.list_mut(l);
        list.dy = -3;
        list.dx -= 16;
    }
    // The dates follow the names' selection every frame; clicking them does nothing.
    m.d.widgets[dates].enabled = false;
    Lists { names, dates, bar }
}

fn fill(m: &mut Menu, lists: Lists, saves: &[Entry]) {
    let names = saves.iter().map(|e| [shown(&e.file), b".sav".to_vec()].concat()).collect();
    m.d.list_mut(lists.names).items = names;
    m.d.list_mut(lists.dates).items = saves.iter().map(|e| e.info.time.text()).collect();
}

/// The selected row of a list, if it is an item.
fn selected(m: &Menu, list: WidgetId) -> Option<usize> {
    let l = m.d.list(list);
    (l.cur < l.items.len()).then_some(l.cur)
}

fn set_text(m: &mut Menu, id: WidgetId, text: &[u8]) {
    if let Kind::TextButton(b) = &mut m.d.widgets[id].kind {
        b.text = text.to_vec();
    }
}

fn set_visible(m: &mut Menu, lists: Lists, on: bool) {
    for w in [lists.names, lists.dates, lists.bar] {
        m.d.widgets[w].visible = on;
    }
    m.d.widgets[lists.names].enabled = on;
    m.d.widgets[lists.bar].enabled = on;
    m.d.list_mut(lists.names).focus = on;
}

/// The load dialog: full screen from the main menu, or a window over the game.
pub struct LoadGame {
    menu: Menu,
    over_game: bool,
    saves: Vec<Entry>,
    lists: Lists,
    /// Recordings (`*.rec`, `Autorecord\*.rec`) of the second page; ACR has none.
    records: Option<(Lists, WidgetId, WidgetId)>,
    ok: WidgetId,
    chosen: WidgetId,
}

impl LoadGame {
    /// From the main menu's "Laden".
    pub fn main_menu() -> Self {
        Self::new(false)
    }

    /// From the game menu.
    pub fn over_game() -> Self {
        Self::new(true)
    }

    fn new(over_game: bool) -> Self {
        Self {
            menu: Menu::default(),
            over_game,
            saves: Vec::new(),
            lists: Lists::default(),
            records: None,
            ok: 0,
            chosen: 0,
        }
    }

    fn build_main_menu(&mut self, ctx: &Ctx) {
        let m = &mut self.menu;
        m.d.load_random(ctx.lib);
        m.d.add_picture(ctx.lib, 0, 0, BACKGROUND);
        let bar = m.d.add_scroll_bar(ctx.lib, 901, 266, 238, 1000, 0, VSCROLL, 0);
        let rec_bar = m.d.add_scroll_bar(ctx.lib, 901, 266, 238, 1000, 0, VSCROLL, 0);
        let clip = Some(super::super::draw::Rect::inclusive(0, 0, 709, 767));
        self.lists = add_lists(m, ctx, (446, 282), 714, 8, LOADGAME, 0, bar, clip);
        let records = add_lists(m, ctx, (446, 282), 714, 8, LOADGAME, 0, rec_bar, clip);
        set_visible(m, records, false);
        m.d.list_mut(self.lists.names).focus = true;
        // Page tabs: `Interf2\Buttons` frames 4 (normal) and 5 (selected or hovered).
        let frames = [[4; 5], [5; 5], [5; 5], [4; 5], [5; 5], [5; 5]];
        let fonts = [Some(font::F16Y3), Some(font::F16Y3), Some(font::F16Y1)];
        let mut tabs = [0; 2];
        for (k, (x, id)) in [(506, "INTF_LOAD_P0"), (506 + 165 + 21, "INTF_LOAD_P1")].into_iter().enumerate() {
            tabs[k] = m.d.add_universal(ctx.lib, x, 244, 165, ctx.texts.get(id), BUTTONS, frames, fonts, Some(0));
        }
        m.d.universal_mut(tabs[0]).state = 1;
        self.records = Some((records, tabs[0], tabs[1]));
        for (x, id, param) in [(472, "INTF_LOAD", OK), (704, "INTF_CANCEL", CANCEL)] {
            let b = m.d.add_gp_text_button(ctx.lib, x, 577, ctx.texts.get(id), BUTTONS, 0, font::F18Y5, font::F18Y5);
            m.d.choose(b, param);
            if let Kind::GpTextButton(t) = &mut m.d.widgets[b].kind {
                t.font_dy -= 1;
            }
            if param == OK {
                self.ok = b;
            }
        }
        m.title(ctx, "INTF_LOAD_TITLE0");
        m.text(ctx, 680, 203, "INTF_LOAD_TITLE1", font::F16Y5, Align::Center);
        m.text(ctx, 446, 528, "INTF_LOAD_CG", font::F18Y3, Align::Left);
        self.chosen = m.d.add_text(ctx.lib, ctx.fonts, 602, 528, b"", font::F18W, Align::Left);
    }

    fn build_over_game(&mut self, ctx: &Ctx) {
        let (ox, oy) = game_origin();
        let m = &mut self.menu;
        let (fx, fy) = (ox + FRAME_DX, oy + FRAME_DY);
        m.d.add_gp_picture(ctx.lib, fx, fy, G_LOAD_GAME, 4);
        m.d.add_gp_picture(ctx.lib, fx, fy, G_LOAD_GAME, 5);
        let bar = m.d.add_scroll_bar(ctx.lib, fx + 515, fy + 70, 255, 1, 0, GSCROLL, 0);
        self.lists = add_lists(m, ctx, (fx + 63, fy + 81), fx + 63 + 268, 9, G_LOAD_GAME, 6, bar, None);
        self.ok = game_buttons(m, ctx, (fx, fy), "INTF_LOAD");
        m.text(ctx, fx + 295, fy + 36, "INTF_LOAD_TITLE1", font::F16Y5, Align::Center);
        m.text(ctx, fx + 120, fy + 353, "INTF_LOAD_CG", font::F18Y3, Align::Center);
        self.chosen = m.d.add_text(ctx.lib, ctx.fonts, fx + 218, fy + 353, b"", font::F18W, Align::Left);
    }

    /// The page showing savegames is selected (always, over the game).
    fn saves_page(&mut self) -> bool {
        match self.records {
            Some((_, tab, _)) => self.menu.d.universal_mut(tab).state > 0,
            None => true,
        }
    }
}

/// OK (`text`) and Abbruch over the game: `G_Load_Game` frame 1 at (+76, +391) and
/// (+308, +391) from the frame, `FontDy` + 4.
fn game_buttons(m: &mut Menu, ctx: &Ctx, (fx, fy): (i32, i32), text: &str) -> WidgetId {
    let mut ok = 0;
    for (dx, id, param) in [(0x4c, text, OK), (0x134, "INTF_CANCEL", CANCEL)] {
        let b = m.d.add_gp_text_button(
            ctx.lib,
            fx + dx,
            fy + 0x187,
            ctx.texts.get(id),
            G_LOAD_GAME,
            1,
            font::F18Y5,
            font::F18Y5,
        );
        m.d.choose(b, param);
        if let Kind::GpTextButton(t) = &mut m.d.widgets[b].kind {
            t.font_dy += 4;
        }
        if param == OK {
            ok = b;
        }
    }
    ok
}

impl Screen for LoadGame {
    fn dialog(&self) -> Option<&super::super::dialog::Dialog> {
        Some(&self.menu.d)
    }

    fn assets(&self, _ctx: &Ctx) -> Vec<String> {
        let files: &[&str] =
            if self.over_game { &[G_LOAD_GAME, GSCROLL] } else { &[BACKGROUND, BUTTONS, VSCROLL, LOADGAME] };
        files.iter().map(|f| (*f).to_owned()).collect()
    }

    fn build(&mut self, ctx: &Ctx) {
        if self.over_game {
            self.build_over_game(ctx);
        } else {
            self.build_main_menu(ctx);
        }
        self.saves = entries(ctx);
        fill(&mut self.menu, self.lists, &self.saves);
    }

    fn tick(&mut self, _ctx: &Ctx) -> Nav {
        self.menu.d.tick(1.0 / super::super::TICK_RATE);
        let page = self.saves_page();
        if let Some((records, _, _)) = self.records {
            set_visible(&mut self.menu, self.lists, page);
            set_visible(&mut self.menu, records, !page);
        }
        let active = match (page, self.records) {
            (true, _) | (false, None) => self.lists,
            (false, Some((records, _, _))) => records,
        };
        let cur = self.menu.d.list(active.names).cur;
        self.menu.d.list_mut(active.dates).cur = cur;
        let pick = selected(&self.menu, active.names);
        self.menu.d.widgets[self.ok].enabled = pick.is_some();
        let text = pick.map(|i| self.menu.d.list(active.names).items[i].clone()).unwrap_or_default();
        set_text(&mut self.menu, self.chosen, &text);
        Nav::Stay
    }

    fn input(&mut self, ctx: &Ctx, input: Input) -> Nav {
        match self.menu.input(input) {
            Some(OK) => {
                let Some(i) = selected(&self.menu, self.lists.names).filter(|_| self.saves_page()) else {
                    return Nav::Stay;
                };
                let file = &self.saves[i].file;
                match ctx.saves.read(file).map(|b| SaveGame::decode(&b)) {
                    Some(Ok(save)) => Nav::Load(Box::new(save)),
                    Some(Err(e)) => {
                        log::error!("{file}: {e}");
                        Nav::Stay
                    }
                    None => Nav::Stay,
                }
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

    fn in_game(&self) -> bool {
        self.over_game
    }
}

/// The save dialog over the game (0x5d9400).
#[derive(Default)]
pub struct SaveGameDialog {
    menu: Menu,
    lists: Lists,
    input: WidgetId,
    ok: WidgetId,
    /// The list selection last seen, to copy a newly chosen name into the field.
    last: Option<usize>,
}

impl SaveGameDialog {
    fn name(&self) -> Vec<u8> {
        self.menu.d.input_box(self.input).text.clone()
    }

    /// A list row's name without `.sav`.
    fn row_name(&self, i: usize) -> Vec<u8> {
        let item = &self.menu.d.list(self.lists.names).items[i];
        item[..item.len().saturating_sub(4)].to_vec()
    }
}

impl Screen for SaveGameDialog {
    fn dialog(&self) -> Option<&super::super::dialog::Dialog> {
        Some(&self.menu.d)
    }

    fn assets(&self, _ctx: &Ctx) -> Vec<String> {
        [G_LOAD_GAME, GSCROLL].map(String::from).to_vec()
    }

    fn build(&mut self, ctx: &Ctx) {
        let (ox, oy) = game_origin();
        let (fx, fy) = (ox + FRAME_DX, oy + FRAME_DY);
        let m = &mut self.menu;
        m.d.add_gp_picture(ctx.lib, fx, fy, G_LOAD_GAME, 4);
        m.d.add_gp_picture(ctx.lib, fx, fy, G_LOAD_GAME, 5);
        let bar = m.d.add_scroll_bar(ctx.lib, fx + 515, fy + 70, 255, 1, 0, GSCROLL, 0);
        self.lists = add_lists(m, ctx, (fx + 63, fy + 81), fx + 63 + 268, 9, G_LOAD_GAME, 6, bar, None);
        self.ok = game_buttons(m, ctx, (fx, fy), "INTF_SAVE");
        m.text(ctx, fx + 295, fy + 36, "INTF_LOAD_TITLE1", font::F16Y5, Align::Center);
        m.text(ctx, fx + 62, fy + 353, "INTF_LOAD_CG", font::F18Y3, Align::Left);
        let last = ctx.profile.borrow().last_save.clone();
        self.input = m.d.add_input_box(fx + 210, fy + 349, &last, NAME_CAPACITY, [302, 24], font::F18Y3, font::F18W);
        m.d.input_box_mut(self.input).focus = true;
        let saves = entries(ctx);
        fill(m, self.lists, &saves);
        // Nothing is selected at first (0x5d9949).
        m.d.list_mut(self.lists.names).cur = usize::MAX;
    }

    fn tick(&mut self, _ctx: &Ctx) -> Nav {
        self.menu.d.tick(1.0 / super::super::TICK_RATE);
        let cur = selected(&self.menu, self.lists.names);
        if cur != self.last {
            // A chosen row puts its name into the field (0x5d99cc).
            self.last = cur;
            if let Some(i) = cur {
                let name = self.row_name(i);
                let input = self.menu.d.input_box_mut(self.input);
                input.set_text(&name);
                input.focus = true;
            }
        } else if let Some(i) = cur
            && self.row_name(i) != self.name()
        {
            // Typing another name drops the selection.
            self.menu.d.list_mut(self.lists.names).cur = usize::MAX;
            self.last = None;
        }
        let list_cur = self.menu.d.list(self.lists.names).cur;
        self.menu.d.list_mut(self.lists.dates).cur = list_cur;
        self.menu.d.widgets[self.ok].enabled = !self.name().is_empty();
        Nav::Stay
    }

    fn input(&mut self, ctx: &Ctx, input: Input) -> Nav {
        match self.menu.input(input) {
            Some(OK) if !file_name(&self.name()).is_empty() => {
                ctx.profile.borrow_mut().last_save = self.name();
                Nav::Save(self.name())
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

    fn in_game(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names() {
        assert_eq!(file_name(b"Gr\xfcn: 1/2"), "Grün_ 1_2");
        assert_eq!(file_name(b"  x "), "x");
        assert_eq!(shown("Grün"), b"Gr\xfcn");
    }
}
