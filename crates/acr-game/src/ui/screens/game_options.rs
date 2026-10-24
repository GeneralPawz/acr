//! The game options of the game menu (0x5dbe10): a dialog over the paused game, unlike
//! the main menu's options screen. `Interf2\Options_for_Game` frames 0 (shadow) and 1, combo
//! boxes `Interf2\Combo2`, sliders `Interf2\Scroll2`, OK and Abbruch on `Interf2\G_Load_Game`.
//! Enter and Escape work (the loop calls `StdKeys`). OK stores `FPSTime = 100 − speed` and
//! `ScrollSpeed = slider + 2`, game mode, resolution and volumes.
//!
//! ACR adds two pages in the same frame ("Weitere Optionen", docs/ui.md): the battle's
//! looks and the optional rules ([`BATTLE`]), and the 3D graphics ([`GRAPHICS`]). The rules
//! go into a new battle's setup only (they are part of the simulation), which the page says.

use super::super::dialog::{Align, CANCEL, Kind, OK, WidgetId};
use super::super::draw::Draw;
use super::super::settings::{CONTROL_CHOICES, CameraMode, Gore, Models, RESOLUTIONS, Settings};
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
/// `UserParam` of the page switch (ACR).
const NEXT_PAGE: i32 = 0xfe;
/// Pages: the original's, [`BATTLE`], [`GRAPHICS`].
const PAGES: usize = 3;

/// One line of an ACR page: a label and a combo box over a setting.
struct Choice {
    label: &'static [u8],
    lines: &'static [&'static [u8]],
    get: fn(&Settings) -> usize,
    set: fn(&mut Settings, usize),
}

/// `corpse_minutes` offered (the setting takes 1..=60; another value is shown as the nearest).
const CORPSE_MINUTES: [u32; 9] = [1, 2, 3, 5, 10, 15, 20, 30, 60];

fn on_off(on: bool) -> usize {
    usize::from(on)
}

/// What a row of a page holds: a heading, a choice, or a note in a box.
enum Row {
    Heading(&'static [u8]),
    Choice(Choice),
    Note(&'static [u8]),
}

/// The rows of a page (relative y), as the original page's: headings on its headings' rows
/// (203, 257, 365), combo boxes on the rest, where the frame has the empty boxes and the
/// sliders' tracks drawn in ([`BOX_ROWS`]).
const ROW_Y: [i32; 10] = [203, 230, 257, 284, 311, 338, 365, 392, 419, 446];
/// The rows that always show a box (a combo box or a note)
#[cfg(test)]
const BOX_ROWS: [i32; 7] = [230, 284, 311, 338, 392, 419, 446];

/// Page 2: the battle's looks and the optional rules (German like the game's texts, Latin-1),
/// one row per [`ROW_Y`].
const BATTLE: &[Row] = &[
    Row::Heading(b"Gefecht"),
    Row::Choice(Choice {
        label: b"Blutdarstellung",
        lines: &[b"Voll", b"Verringert", b"Aus (Original)"],
        get: |s| match s.gore {
            Gore::Full => 0,
            Gore::Reduced => 1,
            Gore::Off => 2,
        },
        set: |s, i| s.gore = [Gore::Full, Gore::Reduced, Gore::Off][i.min(2)],
    }),
    Row::Heading(b"Tote und Spuren"),
    Row::Choice(Choice {
        // How far corpses rot, and whether their fluids stain the ground (`decay_stain`;
        // one combo box, the page's rows are the frame's).
        label: b"Verwesung",
        lines: &[
            b"Knochen, mit Bodenfleck",
            b"Knochen, ohne Bodenfleck",
            b"Nur halb, mit Bodenfleck",
            b"Nur halb, ohne Bodenfleck",
        ],
        get: |s| 2 * on_off(s.corpse_decay_half) + on_off(!s.decay_stain()),
        set: |s, i| {
            s.corpse_decay_half = i >= 2;
            s.decay_stain = Some(i % 2 == 0);
        },
    }),
    Row::Choice(Choice {
        label: b"Liegezeit",
        lines: &[
            b"1 Minute",
            b"2 Minuten",
            b"3 Minuten",
            b"5 Minuten",
            b"10 Minuten",
            b"15 Minuten",
            b"20 Minuten",
            b"30 Minuten",
            b"60 Minuten",
        ],
        get: |s| (0..CORPSE_MINUTES.len()).min_by_key(|&i| CORPSE_MINUTES[i].abs_diff(s.corpse_minutes)).unwrap_or(3),
        set: |s, i| s.corpse_minutes = CORPSE_MINUTES[i.min(CORPSE_MINUTES.len() - 1)],
    }),
    Row::Choice(Choice {
        label: b"Schlachtfeldspuren",
        lines: &[b"Verblassen", b"Bleiben (ganze Schlacht)"],
        get: |s| on_off(s.marks_keep),
        set: |s, i| s.marks_keep = i == 1,
    }),
    Row::Heading(b"Regeln"),
    Row::Note(b"Gelten ab der n\xe4chsten Schlacht"),
    Row::Choice(Choice {
        label: b"Trefferzonen",
        lines: &[b"Aus (Original)", b"An: Wunden, Glieder, Verbluten"],
        get: |s| on_off(s.hit_locations),
        set: |s, i| s.hit_locations = i == 1,
    }),
    Row::Choice(Choice {
        label: b"K\xf6rperabstand",
        lines: &[b"Aus (Original)", b"An: K\xf6rper brauchen Platz"],
        get: |s| on_off(s.body_spacing),
        set: |s, i| s.body_spacing = i == 1,
    }),
];

/// Page 3: the 3D graphics, one row per [`ROW_Y`].
const GRAPHICS: &[Row] = &[
    Row::Heading(b"3D-Grafik"),
    Row::Choice(Choice {
        label: b"3D-Modelle",
        lines: &[b"Aus (Original)", b"Einheiten", b"Einheiten und Geb\xe4ude"],
        get: |s| match s.models {
            Models::Off => 0,
            Models::Units => 1,
            Models::All => 2,
        },
        set: |s, i| s.models = [Models::Off, Models::Units, Models::All][i.min(2)],
    }),
    Row::Heading(b"Darstellung"),
    Row::Choice(Choice {
        label: b"Kamera",
        lines: &[b"Fest (Original)", b"Frei"],
        get: |s| on_off(s.camera == CameraMode::Free),
        set: |s, i| s.camera = if i == 1 { CameraMode::Free } else { CameraMode::Fixed },
    }),
    Row::Choice(Choice {
        label: b"Schatten",
        lines: &[b"An", b"Aus"],
        get: |s| on_off(!s.shadows),
        set: |s, i| s.shadows = i == 0,
    }),
    Row::Choice(Choice {
        label: b"Detailstufen",
        lines: &[b"An", b"Aus"],
        get: |s| on_off(!s.lod),
        set: |s, i| s.lod = i == 0,
    }),
    // The note "Nur in der 3D-Ansicht" went into the heading, and water and ground share
    // one box, so the trees and the wind have their rows.
    Row::Heading(b"Landschaft (nur 3D)"),
    Row::Choice(Choice {
        label: b"3D-B\xe4ume, Steine",
        lines: &[b"An", b"Aus"],
        get: |s| on_off(!s.map_models),
        set: |s, i| s.map_models = i == 0,
    }),
    Row::Choice(Choice {
        label: b"Wind",
        lines: &[b"An", b"Aus"],
        get: |s| on_off(!s.wind),
        set: |s, i| s.wind = i == 0,
    }),
    Row::Choice(Choice {
        label: b"Wasser, Boden",
        lines: &[b"Detailliert", b"Wasser original", b"Boden original", b"Original"],
        get: |s| 2 * on_off(!s.ground_detailed) + on_off(!s.water_detailed),
        set: |s, i| {
            s.water_detailed = i % 2 == 0;
            s.ground_detailed = i < 2;
        },
    }),
];

/// The rows of ACR page `page` (1 or 2).
fn rows(page: usize) -> &'static [Row] {
    if page == 1 { BATTLE } else { GRAPHICS }
}

#[derive(Default)]
pub struct GameOptions {
    menu: Menu,
    /// The options being edited: the pages not shown keep theirs here.
    settings: Settings,
    page: usize,
    sound: WidgetId,
    music: WidgetId,
    scroll: WidgetId,
    speed: WidgetId,
    resolution: WidgetId,
    game_mode: WidgetId,
    music_mode: WidgetId,
    controls: WidgetId,
    /// The combo boxes of an ACR page, by row.
    choices: Vec<(usize, WidgetId)>,
}

/// The dialog origin at 1024 × 768: `((RealLx − 484) >> 1) − 57`, `((RealLy − 425) >> 1) − 166`.
fn origin() -> (i32, i32) {
    (((1024 - 484) >> 1) - 57, ((768 - 425) >> 1) - 166)
}

impl GameOptions {
    /// The options with the shown page's widgets read.
    fn collect(&self) -> Settings {
        let d = &self.menu.d;
        if self.page > 0 {
            let mut settings = self.settings.clone();
            for &(row, w) in &self.choices {
                if let Row::Choice(c) = &rows(self.page)[row] {
                    (c.set)(&mut settings, d.combo(w).cur);
                }
            }
            return settings;
        }
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

    /// Builds the widgets of the current page.
    fn layout(&mut self, ctx: &Ctx) {
        self.menu = Menu::default();
        self.choices.clear();
        let (ox, oy) = origin();
        let m = &mut self.menu;
        m.d.load_random(ctx.lib);
        m.d.add_gp_picture(ctx.lib, ox + 3, oy + 126, DIALOG, 0);
        m.d.add_gp_picture(ctx.lib, ox + 3, oy + 126, DIALOG, 1);
        m.text(ctx, ox + 303, oy + 162, "INTF_OPT_TITLE1", font::F16Y5, Align::Center);
        let combo = |m: &mut Menu, y: i32, lines: Vec<Vec<u8>>, cur: usize| {
            let c = m.d.add_combo_box(ctx.lib, ox + 239, oy + y, COMBO, 0, 9, WHITE, YELLOW);
            let combo = m.d.combo_mut(c);
            combo.cur = cur.min(lines.len().saturating_sub(1));
            combo.lines = lines;
            c
        };
        // ACR: the page switch, right-aligned on the first heading's line.
        let label: &[u8] = match self.page {
            0 => b"Weitere Optionen \xbb",
            1 => b"Seite 2/3: Grafik \xbb",
            _ => b"Seite 3/3: Zur\xfcck \xbb",
        };
        let width = ctx.fonts.width(ctx.lib, font::F18Y3, label);
        m.entry(ctx, ox + 534 - width, oy + 203, label, [font::F18W, font::F18Y3, font::F18Y3], Align::Left, NEXT_PAGE);
        if self.page == 0 {
            let s = &self.settings;
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
        } else {
            let mut combos = Vec::new();
            for (i, (row, &y)) in rows(self.page).iter().zip(&ROW_Y).enumerate() {
                match row {
                    Row::Note(text) => {
                        m.d.add_text(ctx.lib, ctx.fonts, ox + 262, oy + y + 3, text, YELLOW, Align::Left);
                    }
                    Row::Heading(text) => {
                        m.d.add_text(ctx.lib, ctx.fonts, ox + 66, oy + y, text, font::F18W, Align::Left);
                    }
                    Row::Choice(c) => {
                        m.d.add_text(ctx.lib, ctx.fonts, ox + 76, oy + y, c.label, font::F18Y3, Align::Left);
                        combos.push((i, y, c));
                    }
                }
            }
            for &(i, y, c) in &combos {
                let lines = c.lines.iter().map(|l| l.to_vec()).collect();
                let w = combo(m, y, lines, (c.get)(&self.settings));
                self.choices.push((i, w));
            }
        }
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
        self.page = 0;
        self.layout(ctx);
    }

    fn tick(&mut self, _ctx: &Ctx) -> Nav {
        self.menu.d.tick(1.0 / super::super::TICK_RATE);
        Nav::Stay
    }

    fn input(&mut self, ctx: &Ctx, input: Input) -> Nav {
        match self.menu.input(input) {
            Some(OK) => Nav::Apply(Box::new(self.collect())),
            Some(ABORT | CANCEL) => Nav::Pop,
            Some(NEXT_PAGE) => {
                self.settings = self.collect();
                self.page = (self.page + 1) % PAGES;
                self.layout(ctx);
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

    fn in_game(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_choice_round_trips_its_setting() {
        for page in 1..PAGES {
            for row in rows(page) {
                let Row::Choice(c) = row else { continue };
                for i in 0..c.lines.len() {
                    let mut s = Settings::default();
                    (c.set)(&mut s, i);
                    assert_eq!((c.get)(&s), i, "{}", String::from_utf8_lossy(c.label));
                }
            }
        }
        // Every row has its place; the frame's boxes and tracks are covered, headings free.
        for page in [BATTLE, GRAPHICS] {
            assert_eq!(page.len(), ROW_Y.len());
            for (row, y) in page.iter().zip(ROW_Y) {
                assert_eq!(BOX_ROWS.contains(&y), !matches!(row, Row::Heading(_)), "row {y}");
            }
        }
    }
}
