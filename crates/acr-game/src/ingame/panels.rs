//! What the side panels show of the game: the unit encyclopedia's type (key U) and the
//! game info's pages (key I, 0x4cbde0 forces, 0x4ccbb0 deaths, 0x4ccc30 market prices,
//! 0x4ccfe0 income), plus the score and death counts they need.

use std::collections::HashMap;

use acr_sim::world::EntityId;
use acr_sim::{TypeId, World};

use super::InGame;
use crate::hud::panels::{self, GamePanel, UnitInfo};
use crate::hud::{self, Panel};
use crate::ui::Ctx;
use crate::ui::draw::{Draw, Rect};
use crate::ui::fonts::FontId;

const NATIONS: &str = "NATIONS.LST";
/// Line colour of the tables (0x84) and of a profitable market cell (0x96).
const LINE: u8 = 0x84;
const PROFIT: u8 = 0x96;
/// Screen order of the market's resources (0x66a97c, market slots) and the `Border` icon
/// of each row/column (37 + 0x66a974[k]).
const MARKET_ORDER: [usize; 6] = [2, 0, 4, 5, 1, 3];
const MARKET_ICONS: [usize; 6] = [0, 3, 2, 1, 4, 5];

/// The score (`City+0xf54`) and own losses per type (`Nation+0x2015`), counted by watching
/// the world: a new object adds its `VES` to its owner, a death takes twice that away and
/// gives the killer's owner three times (`Die`, 0x4e99d0), a capture moves five times from
/// the old owner to the new one.
#[derive(Default)]
pub struct Stats {
    /// Living objects with their owner and kill count when last seen.
    alive: HashMap<EntityId, (u8, u16)>,
    pub score: [i64; 8],
    pub deaths: HashMap<(u8, TypeId), u32>,
    /// Players that had objects (`Nation+0x1a78b`, "in game").
    pub present: [bool; 8],
}

impl Stats {
    /// Updates the counts after the world has stepped; `ves` gives a type's `VES`.
    pub fn update(&mut self, world: &World, ves: impl Fn(TypeId) -> i32) {
        // The simulation counts kills (`+0x51`) but does not say whom: the victims are
        // credited to the players whose units' kill counts rose meanwhile, in order.
        let mut kills = [0u32; 8];
        let mut victims = Vec::new();
        for u in &world.units {
            let owner = u.owner.min(7);
            let o = usize::from(owner);
            match (u.alive, self.alive.get(&u.id).copied()) {
                (true, None) => {
                    self.alive.insert(u.id, (owner, u.kills));
                    self.score[o] += i64::from(ves(u.kind));
                    self.present[o] = true;
                }
                (true, Some((before, k))) => {
                    if before != owner {
                        let v = 5 * i64::from(ves(u.kind));
                        self.score[usize::from(before)] -= v;
                        self.score[o] += v;
                        self.present[o] = true;
                    }
                    kills[o] += u32::from(u.kills.saturating_sub(k));
                    self.alive.insert(u.id, (owner, u.kills));
                }
                (false, Some((_, k))) => {
                    self.alive.remove(&u.id);
                    self.score[o] -= 2 * i64::from(ves(u.kind));
                    *self.deaths.entry((u.owner, u.kind)).or_default() += 1;
                    kills[o] += u32::from(u.kills.saturating_sub(k));
                    victims.push((owner, u.kind));
                }
                (false, None) => {}
            }
        }
        // Player 7's losses credit nobody (0x4e992a).
        for (owner, kind) in victims.into_iter().filter(|v| v.0 != 7) {
            if let Some(k) = (0..8).find(|&k| k != usize::from(owner) && kills[k] > 0) {
                kills[k] -= 1;
                self.score[k] += 3 * i64::from(ves(kind));
            }
        }
    }
}

impl InGame {
    fn ves(&self, kind: TypeId) -> i32 {
        let Some(world) = &self.world else { return 0 };
        let def = self.objects.objects().definition(&world.catalog.get(kind).id);
        def.and_then(|d| d.get("VES")).and_then(|c| c.int(0)).unwrap_or(0)
    }

    /// Counts score and losses after the ticks of a frame.
    pub(super) fn update_stats(&mut self) {
        let Some(world) = &self.world else { return };
        if !self.objects.ready() {
            return;
        }
        let mut stats = std::mem::take(&mut self.stats);
        stats.update(world, |k| self.ves(k));
        self.stats = stats;
    }

    /// The side panels' contents for this frame.
    pub(super) fn panel_view(&mut self, ctx: &Ctx, view: &mut hud::View) {
        match self.interface.hud.panel {
            Panel::Unit => {
                let selected = self.selected_units().find(|u| u.owner == self.own()).map(|u| usize::from(u.kind.0));
                if let Some(kind) = self.interface.hud.unit_panel.shown(selected) {
                    view.unit_info = self.unit_info(ctx, TypeId(kind as u16));
                }
            }
            Panel::Game => view.game_page = self.game_page(ctx),
            Panel::None => {}
        }
    }

    /// `BTN_Next` / `BTN_Prev` (0x4c8920, 0x4c8a20): the next or previous type that has an
    /// icon, in the order of the nations' lists; nothing at either end.
    pub(super) fn step_unit_panel(&mut self, step: i32) {
        let Some(world) = &self.world else { return };
        let objects = self.objects.objects();
        let with_icon: Vec<usize> = (0..world.catalog.len())
            .filter(|&i| {
                objects.definition(&world.catalog.get(TypeId(i as u16)).id).is_some_and(|d| d.icon().is_some())
            })
            .collect();
        let selected = self.selected_units().find(|u| u.owner == self.own()).map(|u| usize::from(u.kind.0));
        let Some(current) = self.interface.hud.unit_panel.shown(selected) else { return };
        let next = match with_icon.iter().position(|&i| i == current) {
            Some(p) => with_icon.get((p as i32 + step).max(-1) as usize).copied(),
            None if step > 0 => with_icon.iter().copied().find(|&i| i > current),
            None => with_icon.iter().copied().rev().find(|&i| i < current),
        };
        if let Some(next) = next {
            self.interface.hud.unit_panel.stepped = Some(next);
        }
    }

    /// The `.MD` values the unit panel shows (0x4c7990).
    fn unit_info(&self, ctx: &Ctx, kind: TypeId) -> Option<UnitInfo> {
        let world = self.world.as_ref()?;
        let t = world.catalog.get(kind);
        let def = self.objects.objects().definition(&t.id)?;
        let int = |k: &str, i: usize| def.get(k).and_then(|c| c.int(i)).unwrap_or(0);
        let mut upkeep = Vec::new();
        if let Some(c) = def.get("RESCONSUMER") {
            let v = c.int(1).unwrap_or(0);
            match c.word(0).map(str::to_ascii_uppercase).as_deref() {
                Some("GOLD") => upkeep.push(("UI_UGold", v * 100 / 80)),
                Some("WOOD") => upkeep.push(("UI_UWood", v * 100 / 800)),
                Some("STONE") => upkeep.push(("UI_UStone", v * 100 / 800)),
                _ => {}
            }
        }
        // `RASTRATA_NA_VISTREL attack n RES amount …`: iron and coal per shot.
        if let Some(c) = def.get("RASTRATA_NA_VISTREL") {
            for (res, v) in c.word_int_pairs(2) {
                match res.to_ascii_uppercase().as_str() {
                    "IRON" => upkeep.push(("UI_UIron", v)),
                    "COAL" => upkeep.push(("UI_UCoal", v)),
                    _ => {}
                }
            }
        }
        upkeep.retain(|u| u.1 != 0);
        let order = ["UI_UGold", "UI_UWood", "UI_UStone", "UI_UIron", "UI_UCoal"];
        upkeep.sort_by_key(|u| order.iter().position(|o| *o == u.0));
        let info = def.get("INFO").and_then(|c| Some((c.int(0)? as u8, c.int(1)?.max(0) as usize)));
        Some(UnitInfo {
            kind: usize::from(kind.0),
            name: super::interface::unit_name(ctx, &t.md_name),
            info,
            nation: nation_name(ctx, &t.id).unwrap_or_default(),
            price: std::array::from_fn(|i| t.economy.price[i]),
            upkeep,
            stages: t.economy.build_stages,
            capture: def.has("CAPTURE"),
            building: t.building,
            ves: int("VES", 0),
            storm: int("STORMFORCE", 0),
            opposit: (int("OPPOSIT", 0), int("OPPOSIT", 1)),
            description: t.md_name.to_ascii_uppercase(),
        })
    }

    /// The game info's page: the title in `WhiteFont`, then the body clipped to the page.
    fn game_page(&mut self, ctx: &Ctx) -> Vec<Draw> {
        let mut out = Vec::new();
        let panel = &self.interface.hud.game_panel;
        let (x, y, clip) = GamePanel::page_origin();
        let title = match panel.tab {
            1 => "INFO_G3",
            2 => "Economica",
            3 => "INFO_G4",
            _ => "NationalForces",
        };
        out.push(Draw::game_text(hud::font::WHITE, ctx.texts.get(title), x - 7, y - 24));
        let Some(palette) = ctx.lib.palette(crate::ui::draw::GAME_PALETTE) else { return out };
        let rgba = |i: u8| {
            let [r, g, b] = palette.rgb(i);
            [r, g, b, 255]
        };
        let mut body = Vec::new();
        // The page is drawn scrolled up by the scroll bar (`0x1100b5c`); its height then
        // sets the scroll bar's range (0x4ce6ad).
        let tab = panel.tab;
        let y = y - self.interface.hud.game_panel.scroll(ctx);
        let height = match tab {
            0 => self.forces_page(ctx, x, y, &mut body),
            1 => self.deaths_page(ctx, x, y, rgba(LINE), &mut body),
            2 => self.market_page(ctx, x, y, rgba(PROFIT), &mut body),
            3 => self.income_page(ctx, x, y, rgba(LINE), &mut body),
            _ => 0,
        };
        self.interface.hud.game_panel.set_page_height(height);
        out.extend(body.into_iter().map(|d| d.clipped(clip)));
        out
    }

    /// Tab 0 (0x4cbde0): the players in the game, best team and player first, each with
    /// its colour plate, name and score (`INFO_G2`, `INFO_G1`, both the score / 100).
    fn forces_page(&self, ctx: &Ctx, x: i32, y: i32, out: &mut Vec<Draw>) -> i32 {
        let own = self.own();
        // In a mission only the local player has a name (the others are "???" and skipped).
        let mut players: Vec<(u8, Vec<u8>, u8)> = match &self.setup.kind {
            crate::ui::GameKind::RandomMap(_) => self
                .setup
                .players
                .iter()
                .map(|p| (p.color, p.name.clone(), p.team))
                .filter(|p| self.stats.present[usize::from(p.0.min(7))])
                .collect(),
            crate::ui::GameKind::Mission(_) => vec![(own, ctx.profile.borrow().name.clone(), 0)],
        };
        let score = |n: u8| self.stats.score[usize::from(n.min(7))];
        // Teams by their average score, then players by their own.
        let team_score = |t: u8, n: u8| {
            if t == 0 {
                return score(n);
            }
            let members: Vec<i64> = players.iter().filter(|p| p.2 == t).map(|p| score(p.0)).collect();
            members.iter().sum::<i64>() / members.len().max(1) as i64
        };
        let keys: Vec<(i64, u8, i64)> = players.iter().map(|p| (-team_score(p.2, p.0), p.2, -score(p.0))).collect();
        let mut order: Vec<usize> = (0..players.len()).collect();
        order.sort_by_key(|&i| keys[i]);
        players = order.into_iter().map(|i| players[i].clone()).collect();
        let mut yy = y;
        for (color, name, _) in players {
            out.push(Draw::game_frame(hud::gp::BORDER, 95 + usize::from(color % 8), x - 6, yy, 0));
            out.push(Draw::game_text(hud::font::WHITE, &name, x + 60, yy + 6));
            yy += 50;
            out.push(Draw::game_frame(hud::gp::BORDER, 91, x - 3, yy, 0));
            let value = format!("{:.2}", score(color) as f64 / 100.0);
            for label in ["INFO_G2", "INFO_G1"] {
                out.push(Draw::game_text(panels::FONT_SPECIAL_YELLOW, ctx.texts.get(label), x + 2, yy + 4));
                let w = ctx.fonts.width(ctx.lib, panels::FONT_SPECIAL_WHITE, value.as_bytes());
                out.push(Draw::game_text(panels::FONT_SPECIAL_WHITE, value.as_bytes(), x + 165 - w / 2, yy + 4));
                yy += 24;
            }
        }
        yy - y
    }

    /// Tab 1 (0x4ccbb0 → 0x4cc660): the local player's losses, units then buildings, names
    /// merged, in type order, as a table with the count right-aligned.
    fn deaths_page(&self, ctx: &Ctx, x: i32, y: i32, line: [u8; 4], out: &mut Vec<Draw>) -> i32 {
        let Some(world) = &self.world else { return 0 };
        let own = self.own();
        let p = x - 5;
        let r = p + 200;
        let mut yy = y + 14;
        for (header, buildings) in [("GI_UNITS", false), ("GI_BLD", true)] {
            let mut rows: Vec<(Vec<u8>, u32)> = Vec::new();
            for i in 0..world.catalog.len() {
                let kind = TypeId(i as u16);
                let t = world.catalog.get(kind);
                let Some(&n) = self.stats.deaths.get(&(own, kind)).filter(|_| t.building == buildings) else {
                    continue;
                };
                let name = super::interface::unit_name(ctx, &t.md_name);
                match rows.iter_mut().find(|r| r.0 == name) {
                    Some(r) => r.1 += n,
                    None => rows.push((name, n)),
                }
            }
            if rows.is_empty() {
                continue;
            }
            out.push(Draw::game_text(hud::font::SMALL_WHITE, ctx.texts.get(header), p + 5, yy));
            yy += 17;
            out.push(Draw::fill(line, Rect::new(p - 2, yy, r + 5 - (p - 2) + 1, 1)));
            for (name, n) in rows {
                out.push(Draw::game_text(panels::FONT_SMALL_YELLOW, &name, p + 5, yy + 1));
                let count = n.to_string();
                let w = ctx.fonts.width(ctx.lib, panels::FONT_SMALL_YELLOW, count.as_bytes());
                out.push(Draw::game_text(panels::FONT_SMALL_YELLOW, count.as_bytes(), r - w, yy + 1));
                out.push(Draw::fill(line, Rect::new(p - 2, yy + 15, r + 5 - (p - 2) + 1, 1)));
                for vx in [r - 45, p - 2, r + 5] {
                    out.push(Draw::fill(line, Rect::new(vx, yy, 1, 16)));
                }
                yy += 14;
            }
            yy += 4;
        }
        yy - y
    }

    /// Tab 2 (0x4ccc30): what 1000 of the row resource buys of the column resource, at the
    /// current rates (white) and the normal ones (yellow); an outline marks a cell above its
    /// normal value. Then `E_Comment`.
    fn market_page(&self, ctx: &Ctx, x: i32, y: i32, profit: [u8; 4], out: &mut Vec<Draw>) -> i32 {
        let Some(world) = &self.world else { return 0 };
        let Some(def) = world.catalog.market.as_ref() else { return 0 };
        let normal = acr_sim::market::Market::new(def);
        for i in 0..6 {
            for j in 0..6 {
                let (cx, cy) = (x + 41 + 32 * i as i32, y + 50 + 27 * j as i32);
                if i == 0 {
                    out.push(Draw::game_frame(hud::gp::BORDER, 37 + MARKET_ICONS[j], cx - 48, cy - 8, 0));
                }
                if j == 0 {
                    out.push(Draw::game_frame(hud::gp::BORDER, 37 + MARKET_ICONS[i], cx - 13, cy - 38, 0));
                }
                if i == j {
                    continue;
                }
                let (ri, rj) = (MARKET_ORDER[i], MARKET_ORDER[j]);
                let v1 = world.market.quote(rj, ri, 1000);
                let v2 = normal.quote(rj, ri, 1000);
                for (v, f, dy) in [(v1, hud::font::SMALL_WHITE, -5), (v2, panels::FONT_SMALL_YELLOW, 5)] {
                    let s = v.to_string();
                    let w = ctx.fonts.width(ctx.lib, f, s.as_bytes());
                    out.push(Draw::game_text(f, s.as_bytes(), cx - w / 2, cy + dy));
                }
                if v2 < v1 {
                    hud::outline(out, Rect::new(cx - 15, cy - 8, 31, 26), profit);
                }
            }
        }
        let comment = ctx.texts.get("E_Comment");
        let mut n = 0;
        for l in comment.split(|&c| c == b'\\') {
            out.push(Draw::game_text(panels::FONT_SMALL_YELLOW, l, x - 3, y + 210 + 15 * n));
            n += 1;
        }
        210 + 15 * n
    }

    /// Tab 3 (0x4ccfe0): the workers in the mines (`INFO_R1`: coal, iron, gold, stone), the
    /// gatherers (`INFO_R2`: wood, food), then the gold upkeep (`INFO_R4`, the `INFO_R7`
    /// explanation, units and buildings with their count and the gold workers they need,
    /// `RESCONSUMER GOLD v` · count / 80 in tenths, and the total `INFO_R8`). Returns the
    /// content height.
    fn income_page(&self, ctx: &Ctx, x: i32, y: i32, line: [u8; 4], out: &mut Vec<Draw>) -> i32 {
        use acr_sim::economy::{COAL, FOOD, GOLD, IRON, STONE, WOOD};
        let Some(world) = &self.world else { return 0 };
        let own = self.own();
        let names = super::interface::RESOURCE_NAMES;
        let small_white = hud::font::SMALL_WHITE;
        let yellow = panels::FONT_SMALL_YELLOW;
        let text = |out: &mut Vec<Draw>, f: FontId, t: &[u8], x: i32, y: i32| out.push(Draw::game_text(f, t, x, y));
        let centred = |out: &mut Vec<Draw>, f: FontId, t: &[u8], cx: i32, y: i32| {
            // The widths are measured in `SmallYellowFont` whatever the font (0x4834dc).
            let w = ctx.fonts.width(ctx.lib, yellow, t);
            out.push(Draw::game_text(f, t, cx - w / 2, y));
        };
        let hline = |out: &mut Vec<Draw>, y: i32| out.push(Draw::fill(line, Rect::new(x - 5, y, 219, 1)));
        let vline = |out: &mut Vec<Draw>, vx: i32, y0: i32, y1: i32| {
            out.push(Draw::fill(line, Rect::new(vx, y0.min(y1), 1, (y1 - y0).abs() + 1)))
        };
        // Rows of `UNI_LINEDLY1 + 15` = 15 pixels, text 1 pixel below the line (0x7c3430).
        let h = 15;
        let mut yy = y + 14;
        let table = |out: &mut Vec<Draw>, yy: &mut i32, header: &str, rows: &[(usize, i32)]| {
            text(out, small_white, ctx.texts.get(header), x, *yy);
            *yy += 1 + h;
            hline(out, *yy);
            for &(r, n) in rows {
                text(out, yellow, ctx.texts.get(names[r]), x, *yy + 1);
                centred(out, yellow, n.to_string().as_bytes(), x + 196, *yy + 1);
                *yy += h;
                for vx in [x - 5, x + 213, x + 178] {
                    vline(out, vx, *yy, *yy - h);
                }
                hline(out, *yy);
            }
        };
        // Workers inside the working mines (`0x28eac00..`, summed by 0x5f75b3 each tick).
        let workers = |r: usize| -> i32 {
            world
                .units_of(own)
                .filter(|u| u.mine.as_ref().is_some_and(|m| m.resource == r))
                .map(|u| u.inside.len() as i32)
                .sum()
        };
        let rows: Vec<(usize, i32)> = [COAL, IRON, GOLD, STONE].iter().map(|&r| (r, workers(r))).collect();
        table(out, &mut yy, "INFO_R1", &rows);
        // Gatherers (City fields read by 0x4cd5ed; ACR counts the units gathering for the
        // resource).
        let gatherers = |r: usize| -> i32 {
            world
                .units_of(own)
                .filter(|u| match u.orders.last() {
                    Some(acr_sim::world::Order::Gather { kind, .. }) => world
                        .catalog
                        .get(u.kind)
                        .economy
                        .take_res
                        .get(usize::from(*kind))
                        .copied()
                        .flatten()
                        .is_some_and(|e| e.credited == r),
                    _ => false,
                })
                .count() as i32
        };
        yy += 2;
        table(out, &mut yy, "INFO_R2", &[(WOOD, gatherers(WOOD)), (FOOD, gatherers(FOOD))]);
        // The gold upkeep, units and buildings apart, names merged (0x4cc530).
        let objects = self.objects.objects();
        let player = &world.players[usize::from(own)];
        let mut lists: [Vec<(Vec<u8>, i32, i32)>; 2] = [Vec::new(), Vec::new()];
        for i in 0..world.catalog.len() {
            let t = world.catalog.get(TypeId(i as u16));
            let n = i32::from(player.ready_count.get(i).copied().unwrap_or(0));
            let Some(c) = objects.definition(&t.id).and_then(|d| d.get("RESCONSUMER")) else { continue };
            if !c.word(0).is_some_and(|w| w.eq_ignore_ascii_case("GOLD")) {
                continue;
            }
            let v = i32::from(c.int(1).unwrap_or(0) as i16) * n / 80;
            if v == 0 {
                continue;
            }
            let name = super::interface::unit_name(ctx, &t.md_name);
            let list = &mut lists[usize::from(!t.building)];
            match list.iter_mut().find(|r| r.0 == name) {
                Some(r) => {
                    r.1 += n;
                    r.2 += v;
                }
                None => list.push((name, n, v)),
            }
        }
        let tenths = |v: i32| {
            let a = v.abs();
            let s = if a % 10 != 0 { format!("{}.{}", a / 10, a % 10) } else { (a / 10).to_string() };
            if v < 0 { format!("-{s}") } else { s }
        };
        yy += 10;
        let mut total = 0;
        if lists.iter().any(|l| !l.is_empty()) {
            let title = ctx.texts.get("INFO_R4");
            let w = ctx.fonts.width(ctx.lib, hud::font::WHITE, title);
            text(out, hud::font::WHITE, title, x + (210 - w) / 2, yy);
            yy += 20;
            for l in ctx.texts.get("INFO_R7").split(|&c| c == b'\\') {
                text(out, yellow, l, x, yy);
                yy += 14;
            }
            yy += 5;
            centred(out, small_white, ctx.texts.get("INFO_R5"), x + 160, yy);
            centred(out, small_white, ctx.texts.get("INFO_R6"), x + 196, yy);
            let h = 14;
            for (k, (header, list)) in [("GI_UNITS", &lists[1]), ("GI_BLD", &lists[0])].into_iter().enumerate() {
                if list.is_empty() {
                    continue;
                }
                if k == 1 && !lists[1].is_empty() {
                    yy += 3;
                }
                text(out, small_white, ctx.texts.get(header), x, yy);
                yy += 1 + h;
                hline(out, yy);
                for (name, n, v) in list {
                    text(out, yellow, name, x, yy + 1);
                    centred(out, yellow, n.to_string().as_bytes(), x + 161, yy + 1);
                    centred(out, yellow, tenths(*v).as_bytes(), x + 196, yy + 1);
                    hline(out, yy + 15);
                    for vx in [x + 178, x + 143, x - 5, x + 213] {
                        vline(out, vx, yy, yy + h);
                    }
                    yy += h;
                    total += v;
                }
            }
        }
        if total != 0 {
            yy += 5;
            centred(out, small_white, tenths(total).as_bytes(), x + 196, yy);
            text(out, small_white, ctx.texts.get("INFO_R8"), x, yy);
            yy += 16;
        }
        yy - y
    }
}

/// `NatsIDS` of a type's nation: the `NATIONS.LST` entry whose code matches the id's
/// suffix (`EngKre(EN)` → `ENGLAND`).
fn nation_name(ctx: &Ctx, id: &str) -> Option<String> {
    let code = id.split_once('(')?.1.trim_end_matches(')').to_ascii_uppercase();
    ctx.lib.want(NATIONS);
    let text = ctx.lib.bytes(NATIONS)?;
    String::from_utf8_lossy(&text).lines().find_map(|l| {
        let w: Vec<&str> = l.split_whitespace().collect();
        (w.len() >= 7 && w[6].trim_matches(|c| c == '(' || c == ')').eq_ignore_ascii_case(&code))
            .then(|| w[0].to_owned())
    })
}
