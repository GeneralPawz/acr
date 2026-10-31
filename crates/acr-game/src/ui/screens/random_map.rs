//! The random map screen: the create-game-room dialog (0x5cd790) in its single player
//! form (`Interf2\Random_Map_Background.bmp`, title `INTF_CROOM_SCR1`).
//!
//! - Players (left): seven rows 26 px apart from y = 162. Row 0 is the player (name
//!   field), the others are computer slots whose combo box chooses `Kein(e)` or a
//!   difficulty. Each row has a nation combo box, a colour flag (`Interf2\PlayerList`
//!   frames 1-8) and a team field ("X" = none), both changed by clicking.
//! - Game settings (top right): option rows clipped to 601..=952 × 185..=340 with a
//!   scroll bar: landscape, relief, resources and minerals from `Terrain\Res.dat`, map
//!   size, the `GMTP_TP1..7` game options and the climate zone.
//! - Battlefield (bottom right): the map preview frame.
//! - `Interf2\GameButtons`: start (1/0) and cancel (3/4).

use super::super::dialog::{Align, CANCEL, Kind, OK, WidgetId};
use super::super::draw::{Draw, Rect};
use super::super::setup::{Control, Difficulty, GameKind, GameSetup, MapSize, PlayerSetup, RandomMapSetup};
use super::super::{Ctx, Input, Nav, Screen};
use super::{Menu, font};

const BACKGROUND: &str = "INTERF2\\RANDOM_MAP_BACKGROUND.BMP";
const COMBO: &str = "INTERF2\\COMBO.GP";
const VSCROLL: &str = "INTERF2\\VSCROLL.GP";
const PLAYERLIST: &str = "INTERF2\\PLAYERLIST.GP";
const GAMEMAP: &str = "INTERF2\\GAMEMAP.GP";
const ROOM: &str = "INTERF2\\CREATEGAMEROOM.GP";
const GAMEBUTTONS: &str = "INTERF2\\GAMEBUTTONS.GP";
const RES: &str = "TERRAIN\\RES.DAT";
const NATIONS: &str = "TEXT\\NATIONS.TXT";

const PLAYERS: usize = 7;
const ROW_Y: i32 = 162;
const ROW_H: i32 = 26;
/// Playable nations: the entries of `NATIONS.TXT` before `#NEUTRAL`.
const MAX_NATIONS: usize = 12;
const COLORS: u8 = 8;
const TEAMS: u8 = 7;

/// Option rows: label x, combo x and width, first row y, rows visible in the clip.
const OPT_LABEL_X: i32 = 606;
const OPT_COMBO_X: i32 = 739;
const OPT_WIDTH: i32 = 208;
const OPT_Y: i32 = 186;
const OPT_CLIP: Rect = Rect { x0: 601, y0: 185, x1: 953, y1: 341 };

/// `UserParam` of the colour flags and team fields: base + row.
const COLOR_PARAM: i32 = 100;
const TEAM_PARAM: i32 = 200;

/// The sections of `Terrain\Res.dat`: a header `count default` then `count` lines whose
/// first word is a text identifier.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ResDat {
    pub minerals: (Vec<String>, usize),
    pub resources: (Vec<String>, usize),
    pub relief: (Vec<String>, usize),
    pub landscape: (Vec<String>, usize),
}

impl ResDat {
    pub fn parse(data: &[u8]) -> Self {
        let text = String::from_utf8_lossy(data);
        let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
        let mut section = || {
            let header = lines.next().unwrap_or("0 0");
            let mut nums = header.split_whitespace().map(|n| n.parse::<usize>().unwrap_or(0));
            let (count, default) = (nums.next().unwrap_or(0), nums.next().unwrap_or(0));
            let ids = lines
                .by_ref()
                .take(count)
                .map(|l| {
                    let mut words = l.split_whitespace();
                    let first = words.next().unwrap_or("");
                    // Landscape lines start with `#<n>`.
                    if first.starts_with('#') { words.next().unwrap_or("") } else { first }.to_owned()
                })
                .collect::<Vec<_>>();
            (ids, default.min(count.saturating_sub(1)))
        };
        Self { minerals: section(), resources: section(), relief: section(), landscape: section() }
    }
}

struct Row {
    control: WidgetId,
    nation: WidgetId,
    color: WidgetId,
    team_text: WidgetId,
}

#[derive(Default)]
pub struct RandomMap {
    menu: Menu,
    name: WidgetId,
    rows: Vec<Row>,
    colors: Vec<u8>,
    teams: Vec<u8>,
    options: Vec<WidgetId>,
    landscape: WidgetId,
    relief: WidgetId,
    resources: WidgetId,
    minerals: WidgetId,
    size: WidgetId,
    climate: WidgetId,
    nations: usize,
}

impl RandomMap {
    fn nations(ctx: &Ctx) -> Vec<Vec<u8>> {
        let Some(data) = ctx.lib.bytes(NATIONS) else { return Vec::new() };
        data.split(|&b| b == b'\n')
            .map(|l| l.strip_suffix(b"\r").unwrap_or(l).trim_ascii())
            .take_while(|l| !l.starts_with(b"#NEUTRAL"))
            .filter_map(|l| {
                let space = l.iter().position(|&b| b == b' ')?;
                Some(l[space + 1..].trim_ascii().to_vec())
            })
            .take(MAX_NATIONS)
            .collect()
    }

    fn set_team_text(&mut self, ctx: &Ctx, row: usize) {
        let text = match self.teams[row] {
            0 => b"X".to_vec(),
            t => t.to_string().into_bytes(),
        };
        let id = self.rows[row].team_text;
        let w = &mut self.menu.d.widgets[id];
        if let Kind::TextButton(b) = &mut w.kind {
            let width = ctx.fonts.width(ctx.lib, b.passive, &text);
            b.text = text;
            w.x = b.anchor - width / 2;
        }
    }

    fn setup(&self, ctx: &Ctx) -> GameSetup {
        let d = &self.menu.d;
        let mut players = Vec::new();
        for (i, row) in self.rows.iter().enumerate() {
            let control = if i == 0 {
                Control::Human
            } else {
                match d.combo(row.control).cur {
                    0 => continue,
                    n => Control::Computer(Difficulty::from_index(n - 1)),
                }
            };
            let name = if i == 0 { d.input_box(self.name).text.clone() } else { Vec::new() };
            let nation = d.combo(row.nation).cur;
            players.push(PlayerSetup {
                name,
                // The last entry ("???") picks a nation at random.
                nation: if nation >= self.nations { random_below(self.nations, i) } else { nation },
                color: self.colors[i],
                team: self.teams[i],
                control,
            });
        }
        let _ = ctx;
        let seed =
            web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map(|t| t.as_millis() as u32).unwrap_or(0);
        GameSetup {
            kind: GameKind::RandomMap(RandomMapSetup {
                seed: seed & 0xffff,
                landscape: d.combo(self.landscape).cur,
                ground: d.combo(self.relief).cur,
                resources: d.combo(self.resources).cur,
                minerals: d.combo(self.minerals).cur,
                size: [MapSize::Normal, MapSize::Large, MapSize::Huge][d.combo(self.size).cur.min(2)],
                climate: d.combo(self.climate).cur,
            }),
            players,
            local_player: 0,
            rules: Default::default(),
        }
    }
}

fn random_below(n: usize, salt: usize) -> usize {
    let t = web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map(|t| t.as_nanos()).unwrap_or(0);
    ((t as usize).wrapping_mul(31).wrapping_add(salt * 7919)) % n.max(1)
}

impl Screen for RandomMap {
    fn dialog(&self) -> Option<&super::super::dialog::Dialog> {
        Some(&self.menu.d)
    }

    fn assets(&self, _ctx: &Ctx) -> Vec<String> {
        [BACKGROUND, COMBO, VSCROLL, PLAYERLIST, GAMEMAP, ROOM, GAMEBUTTONS, RES, NATIONS].map(String::from).to_vec()
    }

    fn build(&mut self, ctx: &Ctx) {
        let res = ctx.lib.bytes(RES).map(|b| ResDat::parse(&b)).unwrap_or_default();
        let nations = Self::nations(ctx);
        self.nations = nations.len();
        let profile = ctx.profile.borrow().clone();
        let texts = ctx.texts;
        let m = &mut self.menu;
        *m = std::mem::take(m).with_hints();
        m.d.load_random(ctx.lib);
        m.d.add_picture(ctx.lib, 0, 0, BACKGROUND);
        m.d.add_gp_picture(ctx.lib, 580, 450, GAMEMAP, 0);
        m.title(ctx, "INTF_CROOM_SCR1");
        m.text(ctx, 301, 107, "INTF_CROOM_WIN0", font::F16Y5, Align::Center);
        m.text(ctx, 786, 107, "INTF_CROOM_WIN1", font::F16Y5, Align::Center);
        m.text(ctx, 786, 441, "INTF_CROOM_WIN2", font::F16Y5, Align::Center);
        m.text(ctx, 98, 145, "INTF_CROOM_PL0", font::F16Y3, Align::Left);
        m.text(ctx, 228, 145, "INTF_CROOM_PL1", font::F16Y3, Align::Left);
        m.text(ctx, 388, 145, "INTF_CROOM_PL2", font::F16Y3, Align::Center);
        m.text(ctx, 470, 145, "INTF_CROOM_PL3", font::F16Y3, Align::Center);

        let mut nation_lines = nations.clone();
        nation_lines.push(texts.get("???").to_vec());
        let difficulties: Vec<Vec<u8>> = ["None", "@RMID_EASY", "@RMID_NORMAL", "@RMID_HARD", "@RMID_VERYHARD"]
            .iter()
            .map(|id| texts.get(id).to_vec())
            .collect();
        for i in 0..PLAYERS {
            let y = ROW_Y + ROW_H * i as i32;
            let control = if i == 0 {
                self.name = m.d.add_input_box(93, y - 2, &profile.name, 20, [120, 24], font::F18W, font::F18Y3);
                self.name
            } else {
                let c = m.d.add_combo_box_dlx(ctx.lib, 93, y, 128, COMBO, 0, 8, font::F18W, font::F18Y3);
                let combo = m.d.combo_mut(c);
                combo.lines = difficulties.clone();
                // One computer opponent by default.
                combo.cur = if i == 1 { 2 } else { 0 };
                c
            };
            let nation = m.d.add_combo_box_dlx(ctx.lib, 223, y, 126, COMBO, 0, 8, font::F18W, font::F18Y3);
            let combo = m.d.combo_mut(nation);
            combo.lines = nation_lines.clone();
            combo.cur = if i == 0 { 1 } else { nation_lines.len() - 1 };
            let color = m.d.add_gp_picture(ctx.lib, 355, y + 1, PLAYERLIST, i + 1);
            m.d.choose(color, COLOR_PARAM + i as i32);
            let team = m.d.add_gp_picture(ctx.lib, 398, y + 5, PLAYERLIST, 10);
            m.d.choose(team, TEAM_PARAM + i as i32);
            let team_text = m.d.add_text(ctx.lib, ctx.fonts, 413, y + 2, b"X", font::F18Y3, Align::Center);
            self.rows.push(Row { control, nation, color, team_text });
            self.colors.push(i as u8);
            self.teams.push(0);
        }

        // Game settings: option rows in a clipped, scrollable area.
        let names = |ids: &[String]| ids.iter().map(|id| texts.get(id).to_vec()).collect::<Vec<_>>();
        let gmtp_counts = [5, 2, 3, 10, 3, 2, 7];
        let mut rows: Vec<(Vec<u8>, Vec<Vec<u8>>, usize)> = vec![
            // The original's styles, then ACR's sea battle map (#408, `acr_mapgen::SEA_LANDSCAPE`).
            (
                texts.get("INTF_CROOM_O0").to_vec(),
                names(&res.landscape.0).into_iter().chain([b"Meer".to_vec()]).collect(),
                res.landscape.1,
            ),
            (texts.get("INTF_CROOM_O1").to_vec(), names(&res.relief.0), res.relief.1),
            (texts.get("INTF_CROOM_O2").to_vec(), names(&res.resources.0), res.resources.1),
            (texts.get("INTF_CROOM_O3").to_vec(), names(&res.minerals.0), res.minerals.1),
            (
                texts.get("INTF_CROOM_O4").to_vec(),
                ["MS_NORMAL", "MS_LARGE", "MS_HUGE"].iter().map(|id| texts.get(id).to_vec()).collect(),
                0,
            ),
        ];
        for (k, count) in gmtp_counts.iter().enumerate() {
            let lines = (0..*count).map(|j| texts.get(&format!("@GMTP_TP{}_0{j}", k + 1)).to_vec()).collect();
            rows.push((texts.get(&format!("GMTP_TP{}", k + 1)).to_vec(), lines, 0));
        }
        rows.push((
            texts.get("INTF_CROOM_O5").to_vec(),
            (0..4).map(|j| texts.get(&format!("#CL_ZONE{j}")).to_vec()).collect(),
            1,
        ));
        let visible = (OPT_CLIP.height() / ROW_H) as usize;
        let bar = m.d.add_scroll_bar(ctx.lib, 955, 175, 176, 1, 0, VSCROLL, 0);
        if let Kind::ScrollBar(b) = &mut m.d.widgets[bar].kind {
            b.max = (rows.len() as i32 - visible as i32) * ROW_H;
            b.step = ROW_H;
            b.page = ROW_H * (visible as i32 - 1);
        }
        m.d.set_clip(Some(OPT_CLIP));
        for (k, (label, lines, default)) in rows.into_iter().enumerate() {
            let y = OPT_Y + ROW_H * k as i32;
            let l = m.d.add_text(ctx.lib, ctx.fonts, OPT_LABEL_X, y, &label, font::F18Y3, Align::Left);
            let c = m.d.add_combo_box_dlx(ctx.lib, OPT_COMBO_X, y, OPT_WIDTH, COMBO, 0, 9, font::F16Y1, font::F16Y3);
            let combo = m.d.combo_mut(c);
            combo.cur = default.min(lines.len().saturating_sub(1));
            combo.lines = lines;
            for w in [l, c] {
                m.d.widgets[w].scroll_owner = Some(bar);
            }
            self.options.push(c);
        }
        m.d.set_clip(None);
        // Page tabs (`addUniversalButton` at y = 144, 125 px each); the random map page is
        // selected. Frame 3 of `CreateGameRoom`, the map list of the other pages, stays hidden.
        for (k, id) in ["INTF_CROOM_GS0", "INTF_CROOM_GS1", "INTF_CROOM_GS2"].into_iter().enumerate() {
            let tab_font = if k == 1 { font::F16Y1 } else { font::F16Y3 };
            m.text(ctx, 602 + 125 * k as i32 + 62, 147, id, tab_font, Align::Center);
        }
        [self.landscape, self.relief, self.resources, self.minerals, self.size] =
            [self.options[0], self.options[1], self.options[2], self.options[3], self.options[4]];
        self.climate = *self.options.last().unwrap();

        m.d.add_gp_picture(ctx.lib, 672, 689, ROOM, 7);
        let start = m.d.add_gp_button(ctx.lib, 899, 706, GAMEBUTTONS, 1, 0);
        m.d.choose(start, OK);
        m.d.set_hint(start, texts.get("MISS_RUN"));
        let cancel = m.d.add_gp_button(ctx.lib, 959, 706, GAMEBUTTONS, 3, 4);
        m.d.choose(cancel, CANCEL);
        m.d.set_hint(cancel, texts.get("SOCANCEL"));
        m.d.ok_sound = Some(super::super::dialog::sound::START);
    }

    fn tick(&mut self, _ctx: &Ctx) -> Nav {
        self.menu.d.tick(1.0 / super::super::TICK_RATE);
        Nav::Stay
    }

    fn input(&mut self, ctx: &Ctx, input: Input) -> Nav {
        match self.menu.input(input) {
            Some(OK) => Nav::Start(Box::new(self.setup(ctx))),
            Some(CANCEL) => Nav::Pop,
            Some(p) if (COLOR_PARAM..COLOR_PARAM + PLAYERS as i32).contains(&p) => {
                let row = (p - COLOR_PARAM) as usize;
                self.colors[row] = (self.colors[row] + 1) % COLORS;
                let id = self.rows[row].color;
                if let Kind::GpPicture { frame, .. } = &mut self.menu.d.widgets[id].kind {
                    *frame = usize::from(self.colors[row]) + 1;
                }
                Nav::Stay
            }
            Some(p) if (TEAM_PARAM..TEAM_PARAM + PLAYERS as i32).contains(&p) => {
                let row = (p - TEAM_PARAM) as usize;
                self.teams[row] = (self.teams[row] + 1) % (TEAMS + 1);
                self.set_team_text(ctx, row);
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
    fn parses_res_dat() {
        let res = ResDat::parse(
            b"3 1\r\nRA_Lean   1 1 1\r\nRA_Medium 2 2 2\r\nRA_Rich  4 3 3\r\n1 0\r\nRA_Normal 6 1\r\n\
2 1\r\nRT_Plane\r\nRT_Low\r\n2 1\r\n#0 LT_Land 000.STY\r\n#1 LT_Med 000.STY\r\n",
        );
        assert_eq!(res.minerals, (vec!["RA_Lean".into(), "RA_Medium".into(), "RA_Rich".into()], 1));
        assert_eq!(res.resources.0, ["RA_Normal"]);
        assert_eq!(res.relief, (vec!["RT_Plane".into(), "RT_Low".into()], 1));
        assert_eq!(res.landscape.0, ["LT_Land", "LT_Med"]);
    }
}
