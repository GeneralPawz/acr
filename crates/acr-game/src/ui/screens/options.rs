//! The options screen (0x5c9740): graphics, sound and game options.
//!
//! Sliders (`Interf2\Scroll`, 292x12) for sound and music volume, scroll speed and game
//! speed; combo boxes (`Interf2\Combo`) for resolution, music mode and game mode; a check
//! button (`Interf2\Checker`) and a name field for recording games. OK (0xf5) stores the
//! options, Abbruch (0xff) or Escape leaves them unchanged.

use super::super::dialog::{Align, CANCEL, Kind, OK, WidgetId};
use super::super::draw::Draw;
use super::super::settings::{CONTROL_CHOICES, RESOLUTIONS, Settings};
use super::super::{Ctx, Input, Nav, Screen};
use super::{Menu, font};

const BACKGROUND: &str = "INTERF2\\BACKGROUND_OPTIONS.BMP";
const BUTTONS: &str = "INTERF2\\BUTTONS.GP";
const COMBO: &str = "INTERF2\\COMBO.GP";
const SCROLL: &str = "INTERF2\\SCROLL.GP";
const CHECKER: &str = "INTERF2\\CHECKER.GP";
const TEXTBOX: &str = "INTERF2\\TEXTBOX2.GP";

/// `UserParam` of the cancel button and of the record check button.
const ABORT: i32 = 0xff;
const RECORD: i32 = 77;

#[derive(Default)]
pub struct Options {
    menu: Menu,
    settings: Settings,
    sound: WidgetId,
    music: WidgetId,
    scroll: WidgetId,
    speed: WidgetId,
    resolution: WidgetId,
    game_mode: WidgetId,
    music_mode: WidgetId,
    record: WidgetId,
    record_file: WidgetId,
    controls: WidgetId,
}

impl Options {
    fn set_record(&mut self, on: bool) {
        self.settings.record = on;
        let frame = usize::from(on);
        if let Kind::GpButton { active, passive, .. } = &mut self.menu.d.widgets[self.record].kind {
            *active = frame;
            *passive = frame;
        }
        let file = &mut self.menu.d.widgets[self.record_file];
        file.visible = on;
        file.enabled = on;
    }

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
            record: self.settings.record,
            record_file: d.input_box(self.record_file).text.clone(),
            ..self.settings.clone()
        };
        settings.set_control_choice(d.combo(self.controls).cur);
        settings
    }
}

impl Screen for Options {
    fn dialog(&self) -> Option<&super::super::dialog::Dialog> {
        Some(&self.menu.d)
    }

    fn assets(&self, _ctx: &Ctx) -> Vec<String> {
        [BACKGROUND, BUTTONS, COMBO, SCROLL, CHECKER, TEXTBOX].map(String::from).to_vec()
    }

    fn build(&mut self, ctx: &Ctx) {
        self.settings = ctx.settings.clone();
        let s = &self.settings;
        let m = &mut self.menu;
        m.d.load_random(ctx.lib);
        m.d.add_picture(ctx.lib, 0, 0, BACKGROUND);
        m.title(ctx, "INTF_OPT_TITLE0");
        m.text(ctx, 680, 217, "INTF_OPT_TITLE1", font::F16Y5, Align::Center);
        let (x, y, dy) = (443, 258, 27);
        m.text(ctx, x, y, "INTF_OPT_VO", font::F18W, Align::Left);
        m.text(ctx, x, y + dy * 2, "INTF_OPT_AO", font::F18W, Align::Left);
        m.text(ctx, x, y + dy * 6, "INTF_OPT_GO", font::F18W, Align::Left);
        for (row, id) in [
            (1, "INTF_OPT_VO_0"),
            (3, "INTF_OPT_AO_0"),
            (4, "INTF_OPT_AO_1"),
            (5, "INTF_OPT_AO_2"),
            (7, "INTF_OPT_GO_0"),
            (8, "INTF_OPT_GO_1"),
            (9, "INTF_OPT_GO_2"),
            (10, "INTF_OPT_GO_3"),
        ] {
            m.text(ctx, x + 10, y + dy * row, id, font::F18Y3, Align::Left);
        }
        for (bx, id, param) in [(472, "INTF_ACCEPT", OK), (704, "INTF_CANCEL", ABORT)] {
            let b = m.d.add_gp_text_button(ctx.lib, bx, 577, ctx.texts.get(id), BUTTONS, 0, font::F18Y5, font::F18Y5);
            m.d.choose(b, param);
            m.d.set_key(b, if param == OK { "ok" } else { "cancel" });
            if let Kind::GpTextButton(t) = &mut m.d.widgets[b].kind {
                t.font_dy -= 1;
            }
        }
        let slider =
            |m: &mut Menu, y: i32, max: i32, pos: i32| m.d.add_slider(ctx.lib, 621, y, max, pos, SCROLL, 0, [292, 12]);
        self.sound = slider(m, 376, 100, s.sound_volume);
        self.music = slider(m, 402, 100, s.music_volume);
        self.scroll = slider(m, 484, 8, s.scroll_speed - 2);
        self.speed = slider(m, 510, 100, 100 - s.frame_time);
        for (w, key) in [(self.sound, "sound"), (self.music, "music"), (self.scroll, "scroll"), (self.speed, "speed")] {
            m.d.set_key(w, key);
        }
        let combo = |m: &mut Menu, y: i32, lines: Vec<Vec<u8>>, cur: usize| {
            let c = m.d.add_combo_box(ctx.lib, 619, y, COMBO, 0, 9, font::F18W, font::F18Y3);
            let combo = m.d.combo_mut(c);
            combo.drop_dx -= 5;
            combo.font_dy -= 2;
            combo.one_dy -= 1;
            combo.cur = cur.min(lines.len().saturating_sub(1));
            combo.lines = lines;
            c
        };
        let resolutions = RESOLUTIONS.iter().map(|[w, h]| format!("{w}x{h}").into_bytes()).collect();
        self.resolution = combo(m, 284, resolutions, s.resolution);
        let modes = vec![ctx.texts.get("SLOWMOD").to_vec(), ctx.texts.get("FASTMOD").to_vec()];
        self.game_mode = combo(m, 448, modes, s.game_mode);
        let music = vec![ctx.texts.get("NOCD").to_vec(), ctx.texts.get("RNDMUS").to_vec()];
        self.music_mode = combo(m, 340, music, s.music_mode);
        // ACR: the control scheme (#129) beside the game options' heading.
        let controls = CONTROL_CHOICES.iter().map(|c| c.0.to_vec()).collect();
        self.controls = combo(m, 419, controls, s.control_choice());
        for (w, key) in [
            (self.resolution, "resolution"),
            (self.game_mode, "game_mode"),
            (self.music_mode, "music_mode"),
            (self.controls, "controls"),
        ] {
            m.d.set_key(w, key);
        }
        self.record = m.d.add_gp_button(ctx.lib, 619, 525, CHECKER, 1, 0);
        m.d.choose(self.record, RECORD);
        m.d.set_key(self.record, "record");
        m.d.add_strip(ctx.lib, 649, 525, 267, TEXTBOX, [5, 6, 7, 8, 9]);
        self.record_file = m.d.add_input_box(659, 524, &s.record_file, 120, [248, 26], font::F18Y3, font::F18W);
        m.d.set_key(self.record_file, "record_file");
        let record = self.settings.record;
        self.set_record(record);
    }

    fn tick(&mut self, _ctx: &Ctx) -> Nav {
        self.menu.d.tick(1.0 / super::super::TICK_RATE);
        Nav::Stay
    }

    fn input(&mut self, _ctx: &Ctx, input: Input) -> Nav {
        match self.menu.input(input) {
            Some(OK) => Nav::Apply(Box::new(self.collect())),
            Some(ABORT | CANCEL) => Nav::Pop,
            Some(RECORD) => {
                let on = !self.settings.record;
                self.set_record(on);
                Nav::Stay
            }
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
