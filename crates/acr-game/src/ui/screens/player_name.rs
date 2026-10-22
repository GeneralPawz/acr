//! The player name screen (0x5c6170), shown before campaigns, single missions and random
//! maps.
//!
//! The names of `players.txt` (at most ten) are listed as `Interf2\TextBox1` strips at
//! (446, 255 + 26·i); clicking one copies it into the name field. OK (only with a
//! non-empty name) continues to the chosen screen, cancel returns to the main menu.

use super::super::dialog::{Align, CANCEL, Kind, OK, WidgetId};
use super::super::draw::Draw;
use super::super::{Ctx, Input, Nav, Screen};
use super::{Menu, font};

const BACKGROUND: &str = "INTERF2\\BACKGROUND_PLAYER_NAME.BMP";
const BUTTONS: &str = "INTERF2\\BUTTONS.GP";
const TEXTBOX: &str = "INTERF2\\TEXTBOX1.GP";
/// The player list of the original installation.
pub const PLAYERS: &str = "PLAYERS.TXT";

/// Strip frames of the three name row styles (row `i` uses style `i % 3`): left cap,
/// three middle pieces and right cap for normal and selected rows, same under the mouse.
const STYLES: [[[usize; 5]; 2]; 3] =
    [[[0, 1, 2, 3, 4], [5, 6, 7, 8, 9]], [[0, 1, 2, 3, 4], [5, 8, 6, 8, 9]], [[0, 1, 2, 3, 4], [5, 8, 7, 6, 9]]];

/// The player profile kept while the program runs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Profile {
    pub name: Vec<u8>,
    pub names: Vec<Vec<u8>>,
    /// The name last saved under, the save dialog's default (0xbd63a8).
    pub last_save: Vec<u8>,
    /// The campaign progress of `name` once read or changed by a victory.
    pub progress: Option<acr_formats::missions::Progress>,
}

impl Profile {
    /// Reads `players.txt`: one name per line.
    pub fn load(data: &[u8]) -> Self {
        let names: Vec<Vec<u8>> = data
            .split(|&b| b == b'\n')
            .map(|l| l.strip_suffix(b"\r").unwrap_or(l).trim_ascii().to_vec())
            .filter(|l| !l.is_empty())
            .take(10)
            .collect();
        Self { name: names.first().cloned().unwrap_or_default(), names, last_save: Vec::new(), progress: None }
    }

    /// Makes `name` the current player and moves it to the top of the list.
    pub fn choose(&mut self, name: &[u8]) {
        if self.name != name {
            self.progress = None;
        }
        self.name = name.to_vec();
        self.names.retain(|n| n != name);
        self.names.insert(0, name.to_vec());
        self.names.truncate(10);
    }
}

pub struct PlayerName {
    menu: Menu,
    next: Option<Box<dyn Screen>>,
    input: WidgetId,
    ok: WidgetId,
    rows: Vec<WidgetId>,
}

impl PlayerName {
    /// Asks for the player name, then opens `next`.
    pub fn new(next: Box<dyn Screen>) -> Self {
        Self { menu: Menu::default().with_hints(), next: Some(next), input: 0, ok: 0, rows: Vec::new() }
    }

    fn name(&self) -> Vec<u8> {
        self.menu.d.input_box(self.input).text.clone()
    }
}

impl Screen for PlayerName {
    fn assets(&self, _ctx: &Ctx) -> Vec<String> {
        [BACKGROUND, BUTTONS, TEXTBOX].map(String::from).to_vec()
    }

    fn build(&mut self, ctx: &Ctx) {
        let profile = ctx.profile.borrow().clone();
        let m = &mut self.menu;
        m.d.load_random(ctx.lib);
        m.d.add_picture(ctx.lib, 0, 0, BACKGROUND);
        m.title(ctx, "INTF_PLAYER_TITLE0");
        m.text(ctx, 680, 211, "INTF_PLAYER_TITLE1", font::F16Y5, Align::Center);
        m.text(ctx, 446, 528, "INTF_PLAYER_CP", font::F18Y3, Align::Left);
        self.input = m.d.add_input_box(594, 524, &profile.name, 16, [308, 40], font::F18Y3, font::F18W);
        m.d.input_box_mut(self.input).focus = true;
        for (i, name) in profile.names.iter().enumerate() {
            let [normal, selected] = STYLES[i % 3];
            let frames = [normal, selected, selected, normal, selected, selected];
            let fonts = [Some(font::F18Y3), Some(font::F18Y3), Some(font::F18W)];
            let row = m.d.add_universal(ctx.lib, 446, 255 + 26 * i as i32, 467, name, TEXTBOX, frames, fonts, Some(0));
            m.d.widgets[row].rect.y1 -= 2;
            m.d.choose(row, i as i32);
            if *name == profile.name {
                m.d.universal_mut(row).state = 1;
            }
            self.rows.push(row);
        }
        for (x, id, param, hint) in
            [(472, "INTF_ACCEPT", OK, "IHINT_ACEPT_NAME"), (704, "INTF_CANCEL", CANCEL, "SOCANCEL")]
        {
            let b = m.d.add_gp_text_button(ctx.lib, x, 577, ctx.texts.get(id), BUTTONS, 0, font::F18Y5, font::F18Y5);
            m.d.choose(b, param);
            m.d.set_hint(b, ctx.texts.get(hint));
            if let Kind::GpTextButton(t) = &mut m.d.widgets[b].kind {
                t.font_dy -= 1;
            }
            if param == OK {
                self.ok = b;
            }
        }
    }

    fn tick(&mut self, _ctx: &Ctx) -> Nav {
        self.menu.d.tick(1.0 / super::super::TICK_RATE);
        // OK is enabled only with a name (0x5c6b7c).
        let has_name = !self.name().is_empty();
        self.menu.d.widgets[self.ok].enabled = has_name;
        Nav::Stay
    }

    fn input(&mut self, ctx: &Ctx, input: Input) -> Nav {
        let chosen = self.menu.input(input);
        match chosen {
            Some(OK) if !self.name().is_empty() => {
                ctx.profile.borrow_mut().choose(&self.name());
                self.next.take().map_or(Nav::Pop, Nav::Replace)
            }
            Some(CANCEL) => Nav::Pop,
            Some(i) if (0..self.rows.len() as i32).contains(&i) => {
                let row = self.rows[i as usize];
                for &r in &self.rows {
                    self.menu.d.universal_mut(r).state = usize::from(r == row);
                }
                let name = self.menu.d.universal_mut(row).text.clone();
                let input = self.menu.d.input_box_mut(self.input);
                input.set_text(&name);
                input.focus = true;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_list() {
        let mut p = Profile::load(b"EINSTEIN\r\n\r\nNEWTON\r\n");
        assert_eq!(p.name, b"EINSTEIN");
        assert_eq!(p.names, [b"EINSTEIN".to_vec(), b"NEWTON".to_vec()]);
        p.choose(b"NEWTON");
        assert_eq!(p.names[0], b"NEWTON");
        assert_eq!(p.names.len(), 2);
    }
}
