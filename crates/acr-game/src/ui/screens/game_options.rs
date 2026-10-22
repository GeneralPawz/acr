//! The game options of the game menu (0x5dbe10): a dialog over the paused game, unlike
//! the main menu's options screen. `Interf2\Options_for_Game` frames 0 (shadow) and 1, combo
//! boxes `Interf2\Combo2`, sliders `Interf2\Scroll2`, OK and Abbruch on `Interf2\G_Load_Game`.
//! Enter and Escape work (the loop calls `StdKeys`). OK stores `FPSTime = 100 − speed` and
//! `ScrollSpeed = slider + 2`, game mode, resolution and volumes.

use super::super::dialog::{Align, CANCEL, Kind, OK, WidgetId};
use super::super::draw::Draw;
use super::super::settings::{CONTROL_CHOICES, RESOLUTIONS, Settings};
use super::super::{Ctx, Input, Nav, Screen};
use super::{Menu, font};
use crate::ui::fonts::FontId;

const DIALOG: &str = "INTERF2\\OPTIONS_FOR_GAME.GP";
const COMBO: &str = "INTERF2\\COMBO2.GP";
const SCROLL: &str = "INTERF2\\SCROLL2.GP";
const BUTTONS: &str = "INTERF2\\G_LOAD_GAME.GP";

const WHITE: FontId = FontId("WhiteFont");
const YELLOW: FontId = FontId("YellowFont");

/// `UserParam` of Abbruch.
const ABORT: i32 = 0xff;

#[derive(Default)]
pub struct GameOptions {
    menu: Menu,
    settings: Settings,
    sound: WidgetId,
    music: WidgetId,
    scroll: WidgetId,
    speed: WidgetId,
    resolution: WidgetId,
    game_mode: WidgetId,
    music_mode: WidgetId,
    controls: WidgetId,
}

/// The dialog origin at 1024 × 768: `((RealLx − 484) >> 1) − 57`, `((RealLy − 425) >> 1) − 166`.
fn origin() -> (i32, i32) {
    (((1024 - 484) >> 1) - 57, ((768 - 425) >> 1) - 166)
}

impl GameOptions {
    fn collect(&self) -> Settings {
        let d = &self.menu.d;
        let mut settings = Settings {
            sound_volume: d.slider(self.sound).pos,
            music_volume: d.slider(self.music).pos,
            scroll_speed: d.slider(self.scroll).pos + 2,
            frame_time: 100 - d.slider(self.speed).pos,
            resolution: d.combo(self.resolution).cur,
            game_mode: d.combo(self.game_mode).cur,
            music_mode: d.combo(self.music_mode).cur,
            ..self.settings.clone()
        };
        settings.set_control_choice(d.combo(self.controls).cur);
        settings
    }
}

impl Screen for GameOptions {
    fn dialog(&self) -> Option<&super::super::dialog::Dialog> {
        Some(&self.menu.d)
    }

    fn assets(&self, _ctx: &Ctx) -> Vec<String> {
        [DIALOG, COMBO, SCROLL, BUTTONS].map(String::from).to_vec()
    }

    fn build(&mut self, ctx: &Ctx) {
        self.settings = ctx.settings.clone();
        let s = &self.settings;
        let (ox, oy) = origin();
        let m = &mut self.menu;
        m.d.load_random(ctx.lib);
        m.d.add_gp_picture(ctx.lib, ox + 3, oy + 126, DIALOG, 0);
        m.d.add_gp_picture(ctx.lib, ox + 3, oy + 126, DIALOG, 1);
        m.text(ctx, ox + 303, oy + 162, "INTF_OPT_TITLE1", font::F16Y5, Align::Center);
        for (y, id) in [(203, "INTF_OPT_VO"), (257, "INTF_OPT_AO"), (365, "INTF_OPT_GO")] {
            m.text(ctx, ox + 66, oy + y, id, font::F18W, Align::Left);
        }
        for (y, id) in [
            (230, "INTF_OPT_VO_0"),
            (284, "INTF_OPT_AO_0"),
            (311, "INTF_OPT_AO_1"),
            (338, "INTF_OPT_AO_2"),
            (392, "INTF_OPT_GO_0"),
            (419, "INTF_OPT_GO_1"),
            (446, "INTF_OPT_GO_2"),
        ] {
            m.text(ctx, ox + 76, oy + y, id, font::F18Y3, Align::Left);
        }
        let combo = |m: &mut Menu, y: i32, lines: Vec<Vec<u8>>, cur: usize| {
            let c = m.d.add_combo_box(ctx.lib, ox + 239, oy + y, COMBO, 0, 9, WHITE, YELLOW);
            let combo = m.d.combo_mut(c);
            combo.cur = cur.min(lines.len().saturating_sub(1));
            combo.lines = lines;
            c
        };
        let resolutions = RESOLUTIONS.iter().map(|[w, h]| format!("{w}x{h}").into_bytes()).collect();
        self.resolution = combo(m, 229, resolutions, s.resolution);
        let music = vec![ctx.texts.get("NOCD").to_vec(), ctx.texts.get("RNDMUS").to_vec()];
        self.music_mode = combo(m, 285, music, s.music_mode);
        let modes = vec![ctx.texts.get("SLOWMOD").to_vec(), ctx.texts.get("FASTMOD").to_vec()];
        self.game_mode = combo(m, 393, modes, s.game_mode);
        // ACR: the control scheme (#129) beside the game options' heading.
        let controls = CONTROL_CHOICES.iter().map(|c| c.0.to_vec()).collect();
        self.controls = combo(m, 366, controls, s.control_choice());
        let slider = |m: &mut Menu, y: i32, max: i32, pos: i32| {
            m.d.add_slider(ctx.lib, ox + 241, oy + y, max, pos, SCROLL, 0, [292, 12])
        };
        self.sound = slider(m, 321, 100, s.sound_volume);
        self.music = slider(m, 347, 100, s.music_volume);
        self.scroll = slider(m, 429, 8, s.scroll_speed - 2);
        self.speed = slider(m, 455, 100, 100 - s.frame_time.clamp(0, 100));
        for (bx, id, param) in [(79, "INTF_ACCEPT", OK), (311, "INTF_CANCEL", ABORT)] {
            let b = m.d.add_gp_text_button(
                ctx.lib,
                ox + bx,
                oy + 487,
                ctx.texts.get(id),
                BUTTONS,
                1,
                font::F18Y5,
                font::F18Y5,
            );
            m.d.choose(b, param);
            if let Kind::GpTextButton(t) = &mut m.d.widgets[b].kind {
                t.font_dy = 3;
            }
        }
    }

    fn tick(&mut self, _ctx: &Ctx) -> Nav {
        self.menu.d.tick(1.0 / super::super::TICK_RATE);
        Nav::Stay
    }

    fn input(&mut self, _ctx: &Ctx, input: Input) -> Nav {
        match self.menu.input(input) {
            Some(OK) => Nav::Apply(Box::new(self.collect())),
            Some(ABORT | CANCEL) => Nav::Pop,
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
