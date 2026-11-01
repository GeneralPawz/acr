//! The main menu (0x5c6eb0): a map behind a frame with three sets of entries.
//!
//! The main entries sit on the visible part of a large map (`Interf2\MainMenu` frames
//! 1-4, 1020x502 pixels). The single player and multiplayer entries lie on other parts
//! of the map; choosing one slides map and entries diagonally in 25 steps until they
//! show through the frame (`Interf2\MainMenu` frame 0). Everything below the frame is
//! clipped to 342..=676 x 464..=707.

use super::super::dialog::{Align, WidgetId};
use super::super::draw::{Draw, Rect};
use super::super::{Ctx, Input, Key, Nav, Screen};
use super::placeholder::Placeholder;
use super::player_name::PlayerName;
use super::save_load::LoadGame;
use super::{Menu, font};

const BACKGROUND: &str = "INTERF2\\BACKGROUND_MAIN_MENU.BMP";
const MAP: &str = "INTERF2\\MAINMENU.GP";

/// `BuildVersion`, shown in the bottom right corner.
const VERSION: &[u8] = b"V 1.46+";

/// Main entries: centre x, top y and spacing.
const X: i32 = 510;
const Y: i32 = 475;
const STEP: i32 = 35;
/// Offsets of the single player and multiplayer parts of the map.
const SINGLE: (i32, i32) = (340, -250);
const MULTI: (i32, i32) = (-340, -250);
/// Frames of a slide.
const SLIDE_STEPS: i32 = 25;

/// `UserParam` values of the entries (`ItemChoose`).
mod item {
    pub const SINGLE: i32 = 0xf1;
    pub const MULTI: i32 = 0xf2;
    pub const LOAD: i32 = 0xf7;
    pub const OPTIONS: i32 = 0xfc;
    pub const EDITOR: i32 = 0xf3;
    pub const EXIT: i32 = 0xff;
    /// Submenu entries.
    pub const SUB_1: i32 = 1;
    pub const SUB_2: i32 = 2;
    pub const SUB_3: i32 = 3;
    pub const BACK: i32 = 5;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Main,
    Single,
    Multi,
}

#[derive(Debug, Clone, Copy)]
struct Slide {
    to: Page,
    step: i32,
    /// Movement per step; integer division as in the original, so a slide moves 325
    /// pixels horizontally rather than 340.
    dx: i32,
    dy: i32,
}

#[derive(Default)]
pub struct MainMenu {
    menu: Menu,
    /// Map pictures and entries that slide.
    moving: Vec<WidgetId>,
    page: Option<Page>,
    slide: Option<Slide>,
}

impl MainMenu {
    fn start_slide(&mut self, to: Page) {
        let (dx, dy) = match (self.page, to) {
            (Some(Page::Main), Page::Single) => (-SINGLE.0, -SINGLE.1),
            (Some(Page::Main), Page::Multi) => (-MULTI.0, -MULTI.1),
            (Some(Page::Single), Page::Main) => SINGLE,
            (Some(Page::Multi), Page::Main) => MULTI,
            _ => return,
        };
        self.page = None;
        self.slide = Some(Slide { to, step: 0, dx: dx / SLIDE_STEPS, dy: dy / SLIDE_STEPS });
    }
}

impl Screen for MainMenu {
    fn dialog(&self) -> Option<&super::super::dialog::Dialog> {
        Some(&self.menu.d)
    }

    fn assets(&self, _ctx: &Ctx) -> Vec<String> {
        vec![BACKGROUND.into(), MAP.into()]
    }

    fn build(&mut self, ctx: &Ctx) {
        let m = &mut self.menu;
        m.d.add_picture(ctx.lib, 0, 0, BACKGROUND);
        m.d.set_clip(Some(Rect::inclusive(342, 464, 676, 707)));
        for i in 0..4 {
            self.moving.push(m.d.add_gp_picture(ctx.lib, i * 255, 210, MAP, i as usize + 1));
        }
        let fonts = [font::G60W, font::G60Y5, font::G60Y5];
        let mut entries = |m: &mut Menu, x: i32, y: i32, list: &[(&str, i32, &str)]| {
            for (i, (id, param, key)) in list.iter().enumerate() {
                let w = m.entry(ctx, x, y + STEP * i as i32, ctx.texts.get(id), fonts, Align::Center, *param);
                m.d.set_key(w, *key);
                m.d.widgets[w].hover_sound = true;
                self.moving.push(w);
            }
        };
        let main = [
            ("INTF_MAIN_0", item::SINGLE, "single"),
            ("INTF_MAIN_1", item::MULTI, "multi"),
            ("INTF_MAIN_2", item::LOAD, "load"),
            ("INTF_MAIN_3", item::OPTIONS, "options"),
            ("INTF_MAIN_4", item::EDITOR, "editor"),
            ("INTF_MAIN_5", item::EXIT, "exit"),
        ];
        entries(m, X, Y, &main);
        let multi = [
            ("INTF_PROT_B3", item::SUB_3, "multi_3"),
            ("INTF_MAIN_M0", item::SUB_1, "multi_1"),
            ("INTF_MAIN_M1", item::SUB_2, "multi_2"),
            ("INTF_MAIN_B", item::BACK, "multi_back"),
        ];
        entries(m, X + MULTI.0 + 10, Y + MULTI.1 + 38, &multi);
        let single = [
            ("INTF_MAIN_S0", item::SUB_1, "campaigns"),
            ("INTF_MAIN_S1", item::SUB_2, "single_mission"),
            ("INTF_MAIN_S2", item::SUB_3, "random_map"),
            ("INTF_MAIN_B", item::BACK, "single_back"),
        ];
        entries(m, X + SINGLE.0 - 20, Y + SINGLE.1 + 38, &single);
        m.d.set_clip(None);
        m.d.add_gp_picture(ctx.lib, 339, 458, MAP, 0);
        let width = ctx.fonts.width(ctx.lib, font::SMALL_YELLOW1, VERSION);
        m.d.add_text_button(
            ctx.lib,
            ctx.fonts,
            1024 - width,
            751,
            VERSION,
            [font::SMALL_YELLOW, font::SMALL_YELLOW1, font::SMALL_YELLOW],
            Align::Left,
        );
        self.page = Some(Page::Main);
    }

    fn tick(&mut self, _ctx: &Ctx) -> Nav {
        self.menu.d.tick(1.0 / super::super::TICK_RATE);
        let Some(slide) = &mut self.slide else { return Nav::Stay };
        let (dx, dy) = (slide.dx, slide.dy);
        slide.step += 1;
        let done = slide.step >= SLIDE_STEPS;
        let to = slide.to;
        for &w in &self.moving {
            self.menu.d.shift(w, dx, dy);
        }
        if done {
            self.slide = None;
            self.page = Some(to);
        }
        Nav::Stay
    }

    fn input(&mut self, _ctx: &Ctx, input: Input) -> Nav {
        let chosen = self.menu.input(input.clone());
        // Clicks during a slide are dropped: the original resets `ItemChoose` when the
        // submenu loop starts.
        let Some(page) = self.page else { return Nav::Stay };
        // Escape leaves a submenu (the submenu loops map it to "back").
        let chosen = match (input, chosen) {
            (Input::Key(Key::Escape), _) if page != Page::Main => Some(item::BACK),
            (Input::Key(_), _) => None,
            (_, c) => c,
        };
        let Some(param) = chosen else { return Nav::Stay };
        match (page, param) {
            (Page::Main, item::SINGLE) => self.start_slide(Page::Single),
            (Page::Main, item::MULTI) => self.start_slide(Page::Multi),
            (Page::Main, item::OPTIONS) => return Nav::Push(Box::new(super::options::Options::default())),
            (Page::Main, item::LOAD) => return Nav::Push(Box::new(LoadGame::main_menu())),
            (Page::Main, item::EXIT) => return Nav::Quit,
            (Page::Main, item::EDITOR) => return Nav::Start(Box::new(editor_setup())),
            (Page::Single, item::SUB_1 | item::SUB_2 | item::SUB_3) => {
                let next: Box<dyn Screen> = match param {
                    item::SUB_1 => Box::new(super::campaign::Campaigns::default()),
                    item::SUB_2 => Box::new(super::single_mission::SingleMission::default()),
                    _ => Box::new(super::random_map::RandomMap::default()),
                };
                return Nav::Push(Box::new(PlayerName::new(next)));
            }
            (Page::Multi, item::SUB_1 | item::SUB_2 | item::SUB_3) => {
                return Nav::Push(Box::new(Placeholder::multiplayer()));
            }
            (Page::Single | Page::Multi, item::BACK) => self.start_slide(Page::Main),
            _ => {}
        }
        Nav::Stay
    }

    fn draw(&self, ctx: &Ctx, out: &mut Vec<Draw>) {
        self.menu.draw(ctx, out);
    }

    fn take_sounds(&mut self) -> Vec<String> {
        self.menu.take_sounds()
    }

    fn busy(&self) -> bool {
        self.slide.is_some()
    }
}

/// The test editor (docs/controls.md, "Editor"): a blank grass field for two human
/// players, with the placing panel; the setting rules (hit locations, body spacing) apply.
fn editor_setup() -> crate::ui::GameSetup {
    use crate::ui::setup::{BlankSetup, Control, PlayerSetup};
    use crate::ui::{GameKind, GameSetup};
    let player = |i: usize, name: &[u8]| PlayerSetup {
        name: name.to_vec(),
        nation: i,
        color: i as u8,
        team: 0,
        control: Control::Human,
    };
    GameSetup {
        kind: GameKind::Blank(BlankSetup { editor: true, ..Default::default() }),
        players: vec![player(0, b"Spieler 1"), player(1, b"Spieler 2")],
        local_player: 0,
        rules: Default::default(),
    }
}
