//! The game menu (`GameMenu`, 0x5d9c30): a dialog over the paused game, opened with F12
//! or the "M" drop-down. `Interf2\GameMenu` frame 0 (shadow) and 1 (dialog) at
//! `((RealLx − 470) >> 1) − 14`, `(RealLy − 415) >> 1`; five rows 26 pixels apart whose
//! highlight frame shows under the mouse. There is no confirmation for leaving the game.
//! The menu closes after the options, save and load dialogs (0x5da143, `0x28ccc6d`).

use super::super::dialog::{Align, CANCEL, Kind};
use super::super::draw::Draw;
use super::super::{Ctx, Input, Nav, Screen};
use super::{Menu, font};

pub const GAMEMENU: &str = "INTERF2\\GAMEMENU.GP";

/// `UserParam`s of the rows.
const CONTINUE: i32 = 0xf4;
const SAVE: i32 = 0xf8;
const LOAD: i32 = 0xf7;
const OPTIONS: i32 = 0xe7;
const LEAVE: i32 = 0xff;

#[derive(Default)]
pub struct GameMenu {
    menu: Menu,
    /// The options, save or load dialog was opened; the menu closes when it does.
    options_open: bool,
}

/// The dialog origin at 1024 × 768.
pub fn origin() -> (i32, i32) {
    (((1024 - 470) >> 1) - 14, (768 - 415) >> 1)
}

impl Screen for GameMenu {
    fn assets(&self, _ctx: &Ctx) -> Vec<String> {
        vec![GAMEMENU.to_owned()]
    }

    fn build(&mut self, ctx: &Ctx) {
        let (x, y) = origin();
        let m = &mut self.menu;
        m.d.add_gp_picture(ctx.lib, x, y, GAMEMENU, 0);
        m.d.add_gp_picture(ctx.lib, x, y, GAMEMENU, 1);
        m.text(ctx, x + 254, y + 36, "INTF_GMENU_WIN", font::F16Y5, Align::Center);
        // The original disables load (frame 4, passive fonts only) in multiplayer games.
        let rows: [(&str, usize, i32); 5] = [
            ("INTF_GMENU_PAD0", 3, CONTINUE),
            ("INTF_GMENU_PAD1", 5, SAVE),
            ("INTF_MAIN_2", 5, LOAD),
            ("INTF_GMENU_PAD3", 9, OPTIONS),
            ("INTF_GMENU_PAD4", 11, LEAVE),
        ];
        for (i, (id, frame, param)) in rows.into_iter().enumerate() {
            let b = m.d.add_gp_text_button(
                ctx.lib,
                x + 46,
                y + 75 + 26 * i as i32,
                ctx.texts.get(id),
                GAMEMENU,
                frame,
                font::F18W,
                font::F18Y3,
            );
            m.d.choose(b, param);
            if param == LOAD
                && let Kind::GpTextButton(t) = &mut m.d.widgets[b].kind
            {
                // The load row's text is not raised (0x5d9e8a).
                t.font_dy = 0;
            }
        }
    }

    fn tick(&mut self, _ctx: &Ctx) -> Nav {
        if self.options_open {
            return Nav::Pop;
        }
        Nav::Stay
    }

    fn input(&mut self, _ctx: &Ctx, input: Input) -> Nav {
        match self.menu.input(input) {
            Some(CONTINUE) => Nav::Pop,
            // The original's menu loop ignores the keyboard; ACR opens it with Escape too, so
            // Escape also closes it.
            Some(CANCEL) => Nav::Pop,
            Some(OPTIONS) => {
                self.options_open = true;
                Nav::Push(Box::<super::game_options::GameOptions>::default())
            }
            Some(SAVE) => {
                self.options_open = true;
                Nav::Push(Box::<super::save_load::SaveGameDialog>::default())
            }
            Some(LOAD) => {
                self.options_open = true;
                Nav::Push(Box::new(super::save_load::LoadGame::over_game()))
            }
            Some(LEAVE) => Nav::LeaveGame,
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
