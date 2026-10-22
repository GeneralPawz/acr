//! The side panels at the right edge (`0x1100a74`): the unit encyclopedia (key U, built by
//! 0x4c8cd0 and run by 0x4cbb10) and the game info (key I, 0x4ce310). Both sit in the
//! column `RealLx − 259 .. RealLx − 15` from `InfY1` to `InfY2` and close with the box at
//! their top right corner. See `docs/ui.md` (side panels).

use acr_formats::Palette;
use acr_formats::raster::Image;

use super::{SCREEN, font, gp};
use crate::ui::Ctx;
use crate::ui::dialog::{Align, Dialog, Kind};
use crate::ui::draw::{Draw, Rect};
use crate::ui::fonts::FontId;

/// `InfDX`, `InfDY` (8 when `RealLy` ≤ 600), `InfY1 = InfDY + 32`, `InfY2 = InfDY + 450`,
/// `InfY3 = InfDY + 428` at 1024 × 768 (0x66a8f8, 0x66a8f4, 0xff3010, 0x10bb018, 0x10bb01c).
const INF_DX: i32 = 259;
const INF_DY: i32 = 16;
pub const INF_Y1: i32 = INF_DY + 32;
pub const INF_Y2: i32 = INF_DY + 450;
const INF_Y3: i32 = INF_DY + 428;

/// `Interf2\VScroll2`, GP 3 of the engine (loaded with the icons at start).
const VSCROLL2: &str = "INTERF2\\VSCROLL2.GP";

const FN10: FontId = FontId("fn10");
const FON16Y1: FontId = FontId("fon16y1");
const SMALL_YELLOW: FontId = FontId("SmallYellowFont");
const SPECIAL_WHITE: FontId = FontId("SpecialWhiteFont");
const SPECIAL_YELLOW: FontId = FontId("SpecialYellowFont");

/// Corner tables of `Border` (TL, TR, BL, BR, left, right, top, bottom; −1 = none): the
/// panel frame (0x668660), the paper of the unit panel (0x6686a0) and the footer of the
/// game panel (0x6686c0).
const PANEL_CORNERS: [i32; 8] = [17, 87, -1, -1, 12, 36, 23, -1];
const PAPER_CORNERS: [i32; 8] = [3, 22, 6, 24, 12, 36, 10, 11];
const FOOTER_CORNERS: [i32; 8] = [3, 86, 6, 8, 12, 14, 10, 11];

/// The mouse input the panels swallow (0x4c1c36): `x > RealLx − 262`, `y < InfY2 + 8`.
pub fn covers(p: [f32; 2]) -> bool {
    p[0] > (SCREEN[0] - 262) as f32 && p[1] < (INF_Y2 + 8) as f32
}

/// The close box: `RealLx − 42 .. RealLx − 16`, `InfDY + 27 .. InfDY + 53` (exclusive).
pub fn close_box(p: [f32; 2]) -> bool {
    let [x, y] = p;
    x > (SCREEN[0] - 42) as f32 && x < (SCREEN[0] - 16) as f32 && y > (INF_DY + 27) as f32 && y < (INF_DY + 53) as f32
}

/// A box of `Border` frames (0x47f48a): edges tiled and clipped between the corners, then
/// the corners, each clipped to its quarter of the box. Coordinates are inclusive.
pub fn frame_box(ctx: &Ctx, out: &mut Vec<Draw>, (x0, y0, x1, y1): (i32, i32, i32, i32), t: [i32; 8]) {
    let size = |f: i32| if f < 0 { [0, 0] } else { ctx.lib.frame_size(gp::BORDER, f as usize) };
    let cw = size(t[0])[0].max(64);
    let hw = cw / 2;
    let hb = size(if t[2] < 0 { t[0] } else { t[2] })[0].max(64) / 2;
    let (tw, bw) = (size(t[6])[0].max(64), size(t[7])[0].max(64));
    let ch = size(t[0])[1];
    let hc = ch / 2;
    let (lh, rh) = (size(t[4])[1].max(64), size(t[5])[1].max(64));
    let mut put = |f: i32, x: i32, y: i32, clip: Rect| {
        if f >= 0 {
            out.push(Draw::game_frame(gp::BORDER, f as usize, x, y, 0).clipped(clip));
        }
    };
    let n = (x1 - x0 + 1 - cw) / tw;
    let clip = Rect::inclusive(x0 + hw, y0 - hc, x1 - hw - 1, y1 + hc);
    for i in 0..=n + 2 {
        put(t[6], x0 + i * tw + hw, y0 - hc, clip);
    }
    let n = (x1 - x0 + 1 - cw) / bw;
    let clip = Rect::inclusive(x0 + hb, y0 - hc, x1 - hb - 1, y1 + hc);
    for i in 0..=n + 2 {
        put(t[7], x0 + i * bw + hb, y1 - hb, clip);
    }
    let clip = Rect::inclusive(x0 - hb, y0 + hc, x1 + hb, y1 - hc);
    for i in 0..=(y1 - y0 + 1 - ch) / lh + 1 {
        put(t[4], x0 - hb, y0 + i * lh + hc, clip);
    }
    for i in 0..=(y1 - y0 + 1 - ch) / rh + 1 {
        put(t[5], x1 - hb, y0 + i * rh + hc, clip);
    }
    let (mx, my) = ((x0 + x1) / 2, (y0 + y1) / 2);
    put(t[0], x0 - hw, y0 - hc, Rect::inclusive(x0 - hw, y0 - hc, mx - 1, my - 1));
    put(t[1], x1 - hw, y0 - hc, Rect::inclusive(mx, y0 - hc, x1 + hw, my - 1));
    put(t[2], x0 - hb, y1 - hc, Rect::inclusive(x0 - hb, my, mx - 1, y1 + hc));
    put(t[3], x1 - hb, y1 - hc, Rect::inclusive(mx, my, x1 + hb, y1 + hc));
}

/// Paper (0x47fb51): 64-pixel tiles `fill + (i² + j³) % 3` clipped to the box, then its frame.
fn paper(ctx: &Ctx, out: &mut Vec<Draw>, r: (i32, i32, i32, i32), fill: usize, corners: [i32; 8]) {
    let (x0, y0, x1, y1) = r;
    let clip = Rect::inclusive(x0, y0, x1, y1);
    for i in 0..=(x1 - x0) >> 6 {
        for j in 0..=(y1 - y0) >> 6 {
            let f = fill + ((i * i + j * j * j) % 3) as usize;
            out.push(Draw::game_frame(gp::BORDER, f, x0 + 64 * i, y0 + 64 * j, 0).clipped(clip));
        }
    }
    frame_box(ctx, out, r, corners);
}

/// What the unit panel shows of a type, from its `.MD` file (collected by the game).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UnitInfo {
    /// Catalog index: the panel is rebuilt when it changes.
    pub kind: usize,
    pub name: Vec<u8>,
    /// `INFO kind frame` (0x4f993e): 0 unit, 1 artillery, 2 ship, 3 building.
    pub info: Option<(u8, usize)>,
    /// `NatsIDS` of the type's nation, for `<nation>_BLD.bmp`.
    pub nation: String,
    /// `PRICE`, per resource (wood, gold, stone, food, iron, coal).
    pub price: [i32; 6],
    /// Upkeep lines: text id and value (`RESCONSUMER`, `RASTRATA_NA_VISTREL`).
    pub upkeep: Vec<(&'static str, i32)>,
    /// `BUILDSTAGES`.
    pub stages: i32,
    /// `CAPTURE`.
    pub capture: bool,
    pub building: bool,
    /// `VES`.
    pub ves: i32,
    /// `STORMFORCE`.
    pub storm: i32,
    /// `OPPOSIT a b`.
    pub opposit: (i32, i32),
    /// `Description\<md>.md`.
    pub description: String,
    /// The upgrades that concern the type (0x4c84b0, at most 25): `ICONS.GP` frame (none
    /// for an upgrade without a picture) and hint.
    pub upgrades: Vec<(Option<usize>, Vec<u8>)>,
    /// The attack and shield upgrade levels (0x4c7cda, 0x4c80a9).
    pub attack_levels: LevelTable,
    pub shield_levels: LevelTable,
    /// An own unit of this type is the first of the selection: the `THIS_UNIT` page exists.
    pub this_unit: bool,
}

/// The selected unit's panic chances and what changed them (0x601d70), see
/// `docs/ui.md`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MoraleInfo {
    /// Fear type (0..4, `FEARFACTOR` not 0), start and current chance (0..32767).
    pub rows: Vec<(usize, i32, i32)>,
    /// Factor lines: text, hint id, factor · 1000, and whether it raises the fear.
    pub factors: Vec<(Vec<u8>, &'static str, i32, bool)>,
}

/// The levels of the `DAMAGE` (or `PROTECTION`) upgrades of one type, one row per weapon
/// kind (below 8) that any level raises.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LevelTable {
    pub rows: Vec<LevelRow>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct LevelRow {
    /// `SMICONS.GP` frame of the weapon kind.
    pub icon: usize,
    /// The type's own value for the kind.
    pub base: i32,
    /// Hints of the icon (`UI_ATTk` / `UI_SHLk`) and of the base column (`UI_SATTk` /
    /// `UI_SSHLk`).
    pub hints: (Vec<u8>, Vec<u8>),
    /// Per level: the increase (0: an empty cell) and its hint.
    pub cells: Vec<(i32, Vec<u8>)>,
}

/// Portrait files per `INFO` kind and their frame size (0x4c7990).
const PORTRAITS: [(&str, i32, i32); 4] =
    [("UNITS.BMP", 104, 132), ("ARTILLERY.BMP", 132, 114), ("SHIPS.BMP", 132, 151), ("_BLD.BMP", 240, 151)];

/// Header frames of `Border` per kind (unit 0x13, artillery 0x15, ship 0x14, building 4).
const HEADERS: [usize; 4] = [0x13, 0x15, 0x14, 4];

/// Page ids (`UI_PrmPage`, 0x66a958): General, Descr, Upgr, PriceUpg; Next and Prev step
/// through the types.
const GENERAL: i32 = 0;
const DESCR: i32 = 1;
const UPGR: i32 = 2;
const PRICE_UPG: i32 = 3;
/// `THIS_UNIT`: the selected unit's morale (0x601d70).
const THIS_UNIT: i32 = 7;
/// The morale page is rebuilt this often while shown (0x4c8ce5).
const MORALE_REFRESH: f64 = 8.0;
const NEXT: i32 = 100;
const PREV: i32 = 101;

/// The panel's origin (`RealLx − InfDX − 1`, `InfY1 + 31`).
const BASE: (i32, i32) = (SCREEN[0] - INF_DX - 1, INF_Y1 + 31);

/// The portrait image in [`crate::ui::draw::Textures`].
pub const PORTRAIT_IMAGE: &str = "hud:portrait";

/// What a click on the unit panel asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitPanelEvent {
    Used,
    Close,
    /// `BTN_Next` / `BTN_Prev`: the next or previous type with an icon (0x4c8920, 0x4c8a20).
    Step(i32),
}

/// The unit encyclopedia (key U).
#[derive(Default)]
pub struct UnitPanel {
    /// The page per kind; starts at General, PriceUpg for buildings.
    pages: [i32; 4],
    /// A type chosen with Next/Prev, shown until the selection changes.
    pub stepped: Option<usize>,
    last_selected: Option<usize>,
    built: Option<(UnitInfo, i32)>,
    d: Dialog,
    /// Divider below the header (`0xbd8290`).
    div: i32,
    portrait: Option<(Image, i32, i32)>,
    portrait_key: Option<(String, usize)>,
    new_portrait: Option<Image>,
    /// When the panel was last built, for the morale page's refresh.
    built_at: Option<web_time::Instant>,
}

impl UnitPanel {
    pub fn new() -> Self {
        Self { pages: [GENERAL, GENERAL, GENERAL, PRICE_UPG], ..Self::default() }
    }

    /// The type to show for the first selected own unit's type: the selection, unless a
    /// type was stepped to since it last changed.
    pub fn shown(&mut self, selected: Option<usize>) -> Option<usize> {
        if selected.is_some() && selected != self.last_selected {
            self.stepped = None;
        }
        if selected.is_some() {
            self.last_selected = selected;
        }
        self.stepped.or(self.last_selected)
    }

    /// The portrait image when it changed.
    pub fn take_image(&mut self) -> Option<Image> {
        self.new_portrait.take()
    }

    fn kind_of(info: &UnitInfo) -> usize {
        info.info.map_or(if info.building { 3 } else { 0 }, |(k, _)| usize::from(k.min(3)))
    }

    /// Rebuilds the widgets when the type or the page changed (0x4c8cd0).
    fn build(&mut self, ctx: &Ctx, info: &UnitInfo, morale: Option<&MoraleInfo>, palette: &Palette) {
        let kind = Self::kind_of(info);
        let same_info = self.built.as_ref().is_some_and(|(i, _)| i == info);
        // A selected unit opens on its morale (0x4c98a3); without one the page falls back
        // to General.
        if !same_info && kind == 0 {
            if info.this_unit {
                self.pages[0] = THIS_UNIT;
            } else if self.pages[0] == THIS_UNIT {
                self.pages[0] = GENERAL;
            }
        }
        let page = self.pages[kind];
        let stale = page == THIS_UNIT && self.built_at.is_none_or(|t| t.elapsed().as_secs_f64() >= MORALE_REFRESH);
        if same_info && self.built.as_ref().is_some_and(|(_, p)| *p == page) && !stale {
            return;
        }
        self.built = Some((info.clone(), page));
        self.built_at = Some(web_time::Instant::now());
        let lib = ctx.lib;
        let mut d = Dialog::default();
        let (bx, by) = BASE;
        let header = HEADERS[kind];
        let h = lib.frame_size(gp::BORDER, header)[1];
        self.div = INF_Y1 + 29 + h;
        // The portrait (a raw 8-bit BMP drawn with the game palette) lies behind the header.
        self.update_portrait(ctx, info, kind, palette);
        let dy = if kind == 3 { -4 } else { -3 };
        d.add_gp_picture(lib, bx, by + dy, gp::BORDER, header);
        d.add_text(lib, ctx.fonts, bx + 8, by - 29, &info.name, FON16Y1, Align::Left);
        // Page buttons: the frame shows only under the mouse; white when hovered.
        let buttons: Vec<(i32, i32, &str, i32)> = match kind {
            1 => [
                (GENERAL, "BTN_General"),
                (DESCR, "BTN_Descr"),
                (UPGR, "BTN_Upgr"),
                (NEXT, "BTN_Next"),
                (PREV, "BTN_Prev"),
            ]
            .iter()
            .enumerate()
            .map(|(i, &(v, t))| (140, 3 + 22 * i as i32, t, v))
            .collect(),
            2 => [
                (GENERAL, "BTN_General", 20),
                (DESCR, "BTN_Descr", 42),
                (UPGR, "BTN_Upgr", 64),
                (NEXT, "BTN_Next", 86),
                (PREV, "BTN_Prev", 109),
            ]
            .iter()
            .map(|&(v, t, y)| (143, y, t, v))
            .collect(),
            3 => vec![
                (12, 158, "BTN_Descr", DESCR),
                (132, 158, "BTN_PriceUpg", PRICE_UPG),
                (132, 176, "BTN_Next", NEXT),
                (12, 176, "BTN_Prev", PREV),
            ],
            // With a selected unit `THIS_UNIT` comes first and the others close up (0x4c8fb0).
            _ if info.this_unit => [
                (GENERAL, "BTN_General"),
                (DESCR, "BTN_Descr"),
                (UPGR, "BTN_Upgr"),
                (NEXT, "BTN_Next"),
                (PREV, "BTN_Prev"),
            ]
            .iter()
            .enumerate()
            .map(|(i, &(v, t))| (114, 24 + 20 * i as i32, t, v))
            .chain(std::iter::once((114, 5, "THIS_UNIT", THIS_UNIT)))
            .collect(),
            _ => [
                (GENERAL, "BTN_General"),
                (DESCR, "BTN_Descr"),
                (UPGR, "BTN_Upgr"),
                (NEXT, "BTN_Next"),
                (PREV, "BTN_Prev"),
            ]
            .iter()
            .enumerate()
            .map(|(i, &(v, t))| (114, 12 + 22 * i as i32, t, v))
            .collect(),
        };
        let sprite = [0x37, 0x39, 0x5e, 0x39][kind];
        for (x, y, text, value) in buttons {
            if kind == 1 {
                d.add_gp_picture(lib, bx + x, by + y, gp::BORDER, 0x38);
            }
            let text = ctx.texts.get(text);
            let b =
                d.add_gp_text_button(lib, bx + x, by + y, text, gp::BORDER, sprite, font::SMALL_WHITE, SMALL_YELLOW);
            d.choose(b, value);
        }
        // The page, clipped below the divider.
        d.set_clip(Some(Rect::inclusive(0, self.div + 2, SCREEN[0] - 1, INF_Y2)));
        let yy0 = h + 16;
        match page {
            DESCR => {
                d.add_text(
                    lib,
                    ctx.fonts,
                    bx + 9,
                    by + yy0 - 3,
                    ctx.texts.get("BTN_Descr"),
                    font::SMALL_WHITE,
                    Align::Left,
                );
                let path = format!("DESCRIPTION\\{}.MD", info.description);
                lib.want(&path);
                let text = lib.bytes(&path).map(|b| b.to_vec()).unwrap_or_default();
                if lib.bytes(&path).is_none() && !lib.settled(&path) {
                    // Rebuilt once the file is there.
                    self.built = None;
                }
                let height = INF_Y2 - self.div - 32;
                let sb = d.add_scroll_bar(lib, bx + 218, by + yy0 - 9, INF_Y2 - self.div - 10, 1, 0, VSCROLL2, 0);
                let tv = d.add_text_viewer(lib, ctx.fonts, bx + 9, by + yy0 + 13, [205, height], &text, SMALL_YELLOW);
                let line_h = ctx.fonts.height(lib, SMALL_YELLOW) + 2;
                let viewer = d.viewer_mut(tv);
                viewer.scroll = Some(sb);
                viewer.set_line_height(line_h, height);
            }
            GENERAL | PRICE_UPG => self.price_page(ctx, &mut d, info, yy0, page == GENERAL),
            THIS_UNIT => {
                if let Some(m) = morale {
                    let first = d.widgets.len();
                    let end = morale_page(ctx, &mut d, m, palette, yy0);
                    self.page_scroll(ctx, &mut d, first, yy0, end);
                }
            }
            UPGR => {
                let first = d.widgets.len();
                let mut yy = yy0;
                upgrade_grid(ctx, &mut d, info, &mut yy);
                level_table(ctx, &mut d, &info.attack_levels, "UI_AttUpg", &mut yy);
                level_table(ctx, &mut d, &info.shield_levels, "UI_ShlUpg", &mut yy);
                self.page_scroll(ctx, &mut d, first, yy0, yy + 4);
            }
            _ => {}
        }
        self.d = d;
    }

    /// The General page (and the price part of PriceUpg for buildings): price, upkeep,
    /// build time, capture, account value, storm force (General) or resistance (PriceUpg).
    fn price_page(&self, ctx: &Ctx, d: &mut Dialog, info: &UnitInfo, yy0: i32, general: bool) {
        let lib = ctx.lib;
        let (bx, by) = BASE;
        let text = |d: &mut Dialog, x: i32, y: i32, t: &[u8], f: FontId| {
            d.add_text(lib, ctx.fonts, bx + x, by + y, t, f, Align::Left);
        };
        let mut yy = yy0;
        text(d, 13, yy, ctx.texts.get("UI_Price"), font::SMALL_WHITE);
        yy += 16;
        for i in 0..6 {
            d.add_gp_picture(lib, bx + 16 + 35 * i, by + yy, gp::BORDER, 37 + i as usize);
        }
        yy += if general { 26 } else { 24 };
        d.add_gp_picture(lib, bx + 13, by + yy, gp::BORDER, 35);
        for (i, &c) in info.price.iter().enumerate().filter(|(_, c)| **c != 0) {
            let s = c.to_string();
            d.add_text(lib, ctx.fonts, bx + 30 + 34 * i as i32, by + yy + 3, s.as_bytes(), FN10, Align::Center);
        }
        yy += 22;
        if !info.upkeep.is_empty() {
            text(d, 13, yy, ctx.texts.get("UI_Upkeep"), font::SMALL_WHITE);
            yy += 16;
            for &(id, v) in &info.upkeep {
                text(d, 16, yy, &format_int(ctx.texts.get(id), v), SMALL_YELLOW);
                yy += 16;
            }
        }
        text(d, 13, yy, &format_int(ctx.texts.get("UI_STAGE"), info.stages), font::SMALL_WHITE);
        yy += 16;
        if info.capture {
            text(d, 13, yy, ctx.texts.get(if info.building { "UI_BCAP" } else { "UI_CAPT" }), font::SMALL_WHITE);
            yy += 16;
        }
        let accnt = format_float(ctx.texts.get("UI_ACCNT"), f64::from(info.ves) / 100.0);
        text(d, 13, yy, &accnt, font::SMALL_WHITE);
        yy += 16;
        if general {
            text(d, 13, yy, &format_int(ctx.texts.get("STORM_I"), info.storm), font::SMALL_WHITE);
        } else {
            // PriceUpg: the upgrade grid, then the resistance (0x4ca6c9, 0x4ca9ee).
            let first = d.widgets.len();
            upgrade_grid(ctx, d, info, &mut yy);
            let t = format_ints(ctx.texts.get("OPPOSIT_I"), &[info.opposit.0, info.opposit.1]);
            text(d, 13, yy, &t, font::SMALL_WHITE);
            self.page_scroll(ctx, d, first, yy0, yy);
        }
    }

    /// A page taller than the paper gets a scroll bar beside it (0x4ca8a8): `Interf2\VScroll2`
    /// at (218, `yy0 − 9`), `InfY2 − div − 10` long, moving the page's widgets from `first`
    /// on, 10 pixels per arrow step.
    fn page_scroll(&self, ctx: &Ctx, d: &mut Dialog, first: usize, yy0: i32, end: i32) {
        let (bx, by) = BASE;
        let over = end - (INF_Y2 - by);
        if over <= 0 {
            return;
        }
        let sb = d.add_scroll_bar(ctx.lib, bx + 218, by + yy0 - 9, INF_Y2 - self.div - 10, over, 0, VSCROLL2, 0);
        if let Kind::ScrollBar(bar) = &mut d.widgets[sb].kind {
            bar.step = 10;
        }
        for w in &mut d.widgets[first..sb] {
            w.scroll_owner = Some(sb);
        }
    }

    /// Cuts the type's frame out of its portrait file (0x4c7990): `Lx × Ly` frames stacked
    /// in a bottom-up 8-bit BMP after a 0x436-byte header, the palette ignored.
    fn update_portrait(&mut self, ctx: &Ctx, info: &UnitInfo, kind: usize, palette: &Palette) {
        let Some((_, frame)) = info.info else {
            self.portrait = None;
            return;
        };
        let (file, w, h) = PORTRAITS[kind];
        let path = if kind == 3 { format!("{}{file}", info.nation) } else { file.to_owned() };
        let key = (path.clone(), frame);
        if self.portrait_key.as_ref() == Some(&key) {
            return;
        }
        ctx.lib.want(&path);
        let Some(bytes) = ctx.lib.bytes(&path) else {
            self.portrait = None;
            if !ctx.lib.settled(&path) {
                self.built = None;
            }
            return;
        };
        self.portrait_key = Some(key);
        let image = portrait(&bytes, w, h, frame, palette);
        self.new_portrait = image.clone();
        self.portrait = image.map(|i| (i, if kind == 3 { 4 } else { 1 }, -3));
    }

    /// Mouse moved over the screen.
    pub fn mouse_move(&mut self, p: [f32; 2]) {
        self.d.mouse_move(p);
    }

    /// The hint of the widget under the mouse (upgrade icons and level cells).
    pub fn hint(&self) -> Option<&[u8]> {
        self.d.hint()
    }

    pub fn release(&mut self) {
        self.d.release();
    }

    /// Left press inside the panel column.
    pub fn press(&mut self, p: [f32; 2]) -> UnitPanelEvent {
        if close_box(p) {
            return UnitPanelEvent::Close;
        }
        let Some((info, _)) = &self.built else { return UnitPanelEvent::Used };
        let kind = Self::kind_of(info);
        match self.d.press(p).chosen {
            Some(NEXT) => UnitPanelEvent::Step(1),
            Some(PREV) => UnitPanelEvent::Step(-1),
            Some(page) if (0..4).contains(&page) || page == THIS_UNIT => {
                self.pages[kind] = page;
                UnitPanelEvent::Used
            }
            _ => UnitPanelEvent::Used,
        }
    }

    /// The frame (0x480343: the header box, then the paper below the divider), the
    /// portrait and the widgets.
    pub fn draw(
        &mut self,
        ctx: &Ctx,
        info: &UnitInfo,
        morale: Option<&MoraleInfo>,
        palette: &Palette,
        out: &mut Vec<Draw>,
    ) {
        self.build(ctx, info, morale, palette);
        let (x0, x1) = (SCREEN[0] - INF_DX, SCREEN[0] - 15);
        frame_box(ctx, out, (x0, INF_Y1, x1, self.div), PANEL_CORNERS);
        paper(ctx, out, (x0, self.div, x1, INF_Y2), 0x1c, PAPER_CORNERS);
        if let Some((image, dx, dy)) = &self.portrait {
            let rect = Rect::new(BASE.0 + dx, BASE.1 + dy, image.width as i32, image.height as i32);
            out.push(Draw::image(PORTRAIT_IMAGE, rect));
        }
        let mut widgets = Vec::new();
        self.d.draw(ctx.lib, ctx.fonts, &mut widgets);
        out.extend(widgets.into_iter().map(Draw::in_game));
    }
}

/// Wraps `text` into lines at most `x1 − x0` wide (0x601a70): breaks at CR/LF and before a
/// word that does not fit, drops the spaces that start a wrapped line; `{`, `}` and `|`
/// only end words. Each line becomes a text with `hint`; returns the y below the last line
/// (`GetRLCHeight(font, 'y') + 1` per line).
#[allow(clippy::too_many_arguments)]
fn wrapped(
    ctx: &Ctx,
    d: &mut Dialog,
    (x0, x1): (i32, i32),
    y: i32,
    font: FontId,
    text: &[u8],
    hint: Option<&[u8]>,
) -> i32 {
    let (bx, by) = BASE;
    let line_h = ctx.fonts.height(ctx.lib, font) + 1;
    let width = x1 - x0;
    let mut lines: Vec<Vec<u8>> = Vec::new();
    for para in text.split(|&c| c == b'\n').map(|l| l.strip_suffix(b"\r").unwrap_or(l)) {
        let para: Vec<u8> = para.iter().copied().filter(|&c| c != b'{' && c != b'}' && c != b'\r').collect();
        let mut line: Vec<u8> = Vec::new();
        for word in para.split_inclusive(|&c| c == b' ' || c == b'|') {
            let mut candidate = line.clone();
            candidate.extend_from_slice(word);
            let trimmed: Vec<u8> = candidate.iter().copied().filter(|&c| c != b'|').collect();
            if !line.is_empty() && ctx.fonts.width(ctx.lib, font, trimmed.trim_ascii_end()) > width {
                lines.push(std::mem::take(&mut line));
                line = word.iter().copied().skip_while(|&c| c == b' ').collect();
            } else {
                line = candidate;
            }
        }
        lines.push(line);
    }
    let mut yy = y;
    for l in lines {
        let l: Vec<u8> = l.into_iter().filter(|&c| c != b'|').collect();
        let id = d.add_text(ctx.lib, ctx.fonts, bx + x0, by + yy, l.trim_ascii_end(), font, Align::Left);
        if let Some(h) = hint {
            d.set_hint(id, h);
        }
        yy += line_h;
    }
    yy
}

/// `%d.%d%%` of a chance 0..32767 as per mille (0x60243b), with `x` for factors.
fn per_mille(v: i32) -> Vec<u8> {
    let d = (i64::from(v) * 25000 / 32767) as i32;
    format!("{}.{}%", d / 10, d % 10).into_bytes()
}

/// The `THIS_UNIT` page (0x601d70, x relative to the panel origin): `THIS_GEN_INF`,
/// `FEAR_INF_BORN`, per fear type the label with the start and current chance in boxes,
/// then, if any chance changed, `FEAR_ACTS` and the factor lines. Returns the bottom.
fn morale_page(ctx: &Ctx, d: &mut Dialog, m: &MoraleInfo, palette: &Palette, y0: i32) -> i32 {
    let (bx, by) = BASE;
    let [r, g, b] = palette.rgb(0x84);
    let line = [r, g, b, 255];
    let t = |id: &str| ctx.texts.get(id).to_vec();
    let centred = |d: &mut Dialog, x: i32, w: i32, y: i32, text: &[u8], hint: &[u8]| {
        let tw = ctx.fonts.width(ctx.lib, SMALL_YELLOW, text);
        let id = d.add_text(ctx.lib, ctx.fonts, bx + x + (w - tw) / 2, by + y, text, SMALL_YELLOW, Align::Left);
        d.set_hint(id, hint);
    };
    let mut y = wrapped(ctx, d, (6, 230), y0, font::SMALL_WHITE, &t("THIS_GEN_INF"), None) + 4;
    y = wrapped(ctx, d, (6, 230), y, SMALL_YELLOW, &t("FEAR_INF_BORN"), Some(&t("FEAR_INF_BORNH"))) + 4;
    let mut changed = false;
    for &(i, start, current) in &m.rows {
        let top = y;
        let label = t(&format!("FEAR_TXWIDE{i}"));
        let end = wrapped(ctx, d, (6, 150), top, SMALL_YELLOW, &label, Some(&t(&format!("FEAR_TXWIDE{i}H"))));
        let h = end - top + 3;
        for (x, w) in [(2, 145), (147, 40), (187, 40)] {
            d.add_outline(bx + x, by + top - 2, w, h, line);
        }
        centred(d, 147, 40, top, &per_mille(start), &t("START_MVAL"));
        let (ds, dc) = (i64::from(start) * 25000 / 32767, i64::from(current) * 25000 / 32767);
        let same = ds == dc;
        changed |= !same;
        let hint = match (dc, same) {
            (..=30, true) => "CURR_MVAL0",
            (..=30, false) => "CURR_MVAL",
            (..100, true) => "CURR_MVAL2",
            (..100, false) => "CURR_MVAL1",
            (..300, true) => "CURR_MVAL4",
            (..300, false) => "CURR_MVAL3",
            (_, true) => "CURR_MVAL6",
            (_, false) => "CURR_MVAL5",
        };
        centred(d, 187, 40, top, &per_mille(current), &t(hint));
        y = end + 3;
    }
    y += 2;
    if !changed {
        return y;
    }
    y = wrapped(ctx, d, (6, 235), y, SMALL_YELLOW, &t("FEAR_ACTS"), None) + 4;
    for (text, hint, f, raises) in &m.factors {
        let top = y;
        let end = wrapped(ctx, d, (6, 170), top, SMALL_YELLOW, text, Some(&t(hint)));
        let h = end - top + 3;
        for (x, w) in [(2, 165), (167, 60)] {
            d.add_outline(bx + x, by + top - 2, w, h, line);
        }
        let v = format!("x{}.{}%", f / 10, f % 10);
        centred(d, 167, 60, top, v.as_bytes(), &t(if *raises { "FEAR_INC" } else { "FEAR_DEC" }));
        y = end + 3;
    }
    y
}

/// The upgrades of the type as icons, five per row 42 pixels apart, from (7, `yy` − 1)
/// (0x4ca6c9), each with its hint.
fn upgrade_grid(ctx: &Ctx, d: &mut Dialog, info: &UnitInfo, yy: &mut i32) {
    let (bx, by) = BASE;
    let n = info.upgrades.len() as i32;
    for (i, (frame, hint)) in info.upgrades.iter().enumerate() {
        let i = i as i32;
        let (x, y) = (bx + 13 - 5 + 42 * (i % 5), by + *yy + 42 * (i / 5));
        let id = match frame {
            Some(f) => d.add_gp_picture(ctx.lib, x - 1, y - 1, gp::ICONS, *f),
            None => d.add_area(Rect::new(x - 1, y - 1, 40, 40)),
        };
        d.set_hint(id, hint);
    }
    *yy += 42 * ((n + 4) / 5);
}

/// One level table of the Upgr page (0x4cacf0 attack, 0x4cb25b shield): the title, the
/// header strip `Border` 0x21, then per weapon kind a row on `Border` 0x20 (0x1f for the
/// last) with the kind's icon, its base value centred at x = 31 and the increase of every
/// level centred 26 pixels apart from x = 62, in `fn10`; 19 pixels per row.
fn level_table(ctx: &Ctx, d: &mut Dialog, table: &LevelTable, title: &str, yy: &mut i32) {
    if table.rows.is_empty() {
        return;
    }
    let (bx, by) = BASE;
    let x = bx + 13;
    d.add_text(ctx.lib, ctx.fonts, x, by + *yy, ctx.texts.get(title), font::SMALL_WHITE, Align::Left);
    *yy += 16;
    d.add_gp_picture(ctx.lib, x, by + *yy, gp::BORDER, 0x21);
    *yy += 16;
    let last = table.rows.len() - 1;
    for (r, row) in table.rows.iter().enumerate() {
        let y = by + *yy;
        d.add_gp_picture(ctx.lib, x, y, gp::BORDER, if r < last { 0x20 } else { 0x1f });
        d.add_gp_picture(ctx.lib, x + 1, y - 1, gp::SMICONS, row.icon);
        let base_hint = d.add_area(Rect::new(x + 14, y, 35, 19));
        d.set_hint(base_hint, &row.hints.1);
        let icon_hint = d.add_area(Rect::new(x, y, 19, 19));
        d.set_hint(icon_hint, &row.hints.0);
        for (j, (inc, hint)) in row.cells.iter().enumerate() {
            if *inc == 0 {
                continue;
            }
            let cx = x + 0x3e + 26 * j as i32;
            let t = format!("+{inc}");
            d.add_text(ctx.lib, ctx.fonts, cx, y + 2, t.as_bytes(), FN10, Align::Center);
            let cell = d.add_area(Rect::new(x + 0x31 + 26 * j as i32, y, 26, 19));
            d.set_hint(cell, hint);
        }
        let base = row.base.to_string();
        d.add_text(ctx.lib, ctx.fonts, x + 0x1f, y + 2, base.as_bytes(), FN10, Align::Center);
        *yy += 19;
    }
    *yy += 3;
}

/// Frame `frame` of a portrait file as an image, `None` past the last frame.
pub fn portrait(bmp: &[u8], w: i32, h: i32, frame: usize, palette: &Palette) -> Option<Image> {
    const HEADER: usize = 0x436;
    let (w, h) = (w as usize, h as usize);
    let count = bmp.len().saturating_sub(HEADER) / w / h;
    if frame >= count {
        return None;
    }
    let block = HEADER + (count - frame - 1) * w * h;
    let mut rgba = Vec::with_capacity(w * h * 4);
    for row in 0..h {
        let src = block + (h - 1 - row) * w;
        for &i in &bmp[src..src + w] {
            let [r, g, b] = palette.rgb(i);
            rgba.extend_from_slice(&[r, g, b, 255]);
        }
    }
    Some(Image { width: w as u32, height: h as u32, rgba })
}

/// `sprintf(fmt, v)` for a text with one `%d`.
pub fn format_int(fmt: &[u8], v: i32) -> Vec<u8> {
    format_ints(fmt, &[v])
}

/// `sprintf` for `%d` and `%s`-free texts with several `%d`.
pub fn format_ints(fmt: &[u8], values: &[i32]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut values = values.iter();
    let mut i = 0;
    while i < fmt.len() {
        if fmt[i] == b'%' && fmt.get(i + 1) == Some(&b'd') {
            out.extend_from_slice(values.next().map(i32::to_string).unwrap_or_default().as_bytes());
            i += 2;
        } else {
            out.push(fmt[i]);
            i += 1;
        }
    }
    out
}

/// `sprintf(fmt, v)` for a text with one `%.2f`.
fn format_float(fmt: &[u8], v: f64) -> Vec<u8> {
    let s = String::from_utf8_lossy(fmt);
    s.replacen("%.2f", &format!("{v:.2}"), 1).into_bytes()
}

/// The game info (key I): five tabs (0x4ce780), the scrolled page clipped inside.
#[derive(Default)]
pub struct GamePanel {
    /// `0x1100b54`: 0 forces, 1 deaths, 2 market, 3 income, 4 diplomacy.
    pub tab: u8,
    /// The page's scroll bar (`0x10fb562`), created with the panel.
    d: Option<Dialog>,
}

/// The page's scroll bar: `Interf2\VScroll2` at (`RealLx − 42`, `InfDY + 63`), `InfY2 − 108`
/// long (0x4ce7e4).
const PAGE_SCROLL: (i32, i32, i32) = (SCREEN[0] - 42, INF_DY + 63, INF_Y2 - 108);

/// Tab viewports at `InfY3 + 2`, 21 pixels high: x offset from `RealLx − 259`, width, page,
/// hint (0x4ce780).
const TABS: [(i32, i32, u8, &str); 5] = [
    (32, 33, 0, "NationalForces"),
    (69, 32, 2, "Economica"),
    (105, 32, 1, "DeathList"),
    (141, 32, 3, "Doxod"),
    (177, 34, 4, "Diplomacy"),
];

impl GamePanel {
    fn tab_rect(i: usize) -> Rect {
        let (x, w, _, _) = TABS[i];
        Rect::new(SCREEN[0] - INF_DX + x, INF_Y3 + 2, w, 21)
    }

    /// The hint of the tab under the mouse.
    pub fn hint(p: [f32; 2]) -> Option<&'static str> {
        (0..TABS.len()).find(|&i| Self::tab_rect(i).contains(p)).map(|i| TABS[i].3)
    }

    /// Left press inside the panel column: false if it closes the panel.
    pub fn press(&mut self, p: [f32; 2]) -> bool {
        if close_box(p) {
            return false;
        }
        if let Some(i) = (0..TABS.len()).find(|&i| Self::tab_rect(i).contains(p)) {
            self.tab = TABS[i].2;
            // A tab click scrolls back to the top (0x4c6500).
            if let Some(d) = &mut self.d
                && let Kind::ScrollBar(bar) = &mut d.widgets[0].kind
            {
                bar.pos = 0;
            }
            return true;
        }
        if let Some(d) = &mut self.d {
            d.press(p);
        }
        true
    }

    pub fn mouse_move(&mut self, p: [f32; 2]) {
        if let Some(d) = &mut self.d {
            d.mouse_move(p);
        }
    }

    pub fn release(&mut self) {
        if let Some(d) = &mut self.d {
            d.release();
        }
    }

    /// How far the page is scrolled down (`0x1100b5c`): the scroll bar's position while it
    /// is shown.
    pub fn scroll(&mut self, ctx: &Ctx) -> i32 {
        let d = self.d.get_or_insert_with(|| {
            let mut d = Dialog::default();
            let (x, y, len) = PAGE_SCROLL;
            d.add_scroll_bar(ctx.lib, x, y, len, 1, 0, VSCROLL2, 0);
            d.widgets[0].visible = false;
            d
        });
        match &d.widgets[0] {
            w if w.visible => match &w.kind {
                Kind::ScrollBar(bar) => bar.pos,
                _ => 0,
            },
            _ => 0,
        }
    }

    /// After drawing the page: its content height decides whether the scroll bar shows and
    /// how far it reaches, `height − (InfY3 − 15 − Y0)` (0x4ce6ad).
    pub fn set_page_height(&mut self, height: i32) {
        let Some(d) = &mut self.d else { return };
        let range = height - (INF_Y3 - 15 - (INF_Y1 + 25));
        let w = &mut d.widgets[0];
        w.visible = range > 0;
        if let Kind::ScrollBar(bar) = &mut w.kind {
            bar.max = range.max(1);
            bar.pos = bar.pos.clamp(0, bar.max);
        }
    }

    /// The scroll bar, drawn after the page.
    pub fn draw_widgets(&self, ctx: &Ctx, out: &mut Vec<Draw>) {
        if let Some(d) = &self.d {
            let mut widgets = Vec::new();
            d.draw(ctx.lib, ctx.fonts, &mut widgets);
            out.extend(widgets.into_iter().map(Draw::in_game));
        }
    }

    /// The frame (0x4803fb): paper down to `InfY2 − 25`, the footer box below, the tab strip.
    pub fn draw_frame(&self, ctx: &Ctx, out: &mut Vec<Draw>) {
        let (x0, x1) = (SCREEN[0] - INF_DX, SCREEN[0] - 16);
        let ym = INF_Y2 - 25;
        paper(ctx, out, (x0, INF_Y1, x1, ym), 28, PANEL_CORNERS);
        frame_box(ctx, out, (x0, ym, x1, INF_Y2 + 3), FOOTER_CORNERS);
        out.push(Draw::game_frame(gp::BORDER, 85, SCREEN[0] - 260, INF_Y3, 0));
    }

    /// The page's title (the tab's name) and content origin: `X = RealLx − 243`,
    /// `Y0 = InfY1 + 25`, clipped to (X − 10, Y0 + 7) .. (RealLx − 4, InfY3 − 9).
    pub fn page_origin() -> (i32, i32, Rect) {
        let (x, y) = (SCREEN[0] - 243, INF_Y1 + 25);
        (x, y, Rect::inclusive(x - 10, y + 7, SCREEN[0] - 4, INF_Y3 - 9))
    }
}

pub const FONT_SPECIAL_WHITE: FontId = SPECIAL_WHITE;
pub const FONT_SPECIAL_YELLOW: FontId = SPECIAL_YELLOW;
pub const FONT_SMALL_YELLOW: FontId = SMALL_YELLOW;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_and_portraits() {
        assert_eq!(format_int(b"Bauzeit: %d", 90), b"Bauzeit: 90");
        assert_eq!(format_ints(b"Widerstand: %d - %d", &[1, 10]), b"Widerstand: 1 - 10");
        assert_eq!(format_float(b"Kosten: %.2f", 0.3), b"Kosten: 0.30");
        // Two 2 x 1 frames, stored last frame first, rows bottom-up.
        let mut bmp = vec![0u8; 0x436];
        bmp.extend_from_slice(&[3, 4, 1, 2]);
        let mut pal = Palette([[0; 3]; 256]);
        for i in 0..5 {
            pal.0[i] = [i as u8; 3];
        }
        let img = portrait(&bmp, 2, 1, 0, &pal).unwrap();
        assert_eq!((img.rgba[0], img.rgba[4]), (1, 2));
        assert_eq!(portrait(&bmp, 2, 1, 1, &pal).unwrap().rgba[0], 3);
        assert!(portrait(&bmp, 2, 1, 2, &pal).is_none());
        assert!(close_box([990.0, 50.0]) && !close_box([982.0, 50.0]));
    }
}
