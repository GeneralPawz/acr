//! The in-game interface hooked into the game: what the HUD shows of the world, mouse and
//! keyboard input of the game screen, scrolling, and the overlays drawn over the map
//! (selection brackets, life bars, the move marker, the selection rectangle).

use std::collections::HashMap;

use acr_formats::raster::Image;
use acr_sim::world::EntityId;
use acr_sim::{Command, Unit};
use winit::keyboard::{Key, ModifiersState, NamedKey};

use super::InGame;
use crate::canvas::Canvas;
use crate::hud::cursor::{self, Cursor};
use crate::hud::minimap::{self, Dot, Minimap};
use crate::hud::{self, Hud, HudCommand, HudEvent, Icon, Stats, View, color, nearest};
use crate::ui::Ctx;
use crate::ui::draw::{Draw, GAME_PALETTE, Rect};
use crate::ui::settings::Settings;

/// `Data\Keyboard.dat` (in `Patch02.GS1`): `[CTRL|ALT|SHIFT] KEY : ICON` lines (0x4f0635).
const KEYBOARD: &str = "DATA\\KEYBOARD.DAT";
/// `CursGo`: frames shown at a move order's target, one per game frame (0x587fa9).
const MARKER_FRAMES: u8 = 17;
/// Edge width that scrolls the view (0x4c5910), in canvas pixels.
const EDGE: f32 = 6.0;
/// Speed set by the arrow keys (0x4c5910).
const KEY_SCROLL: i32 = 4;
/// Two clicks on the same unit within this time select all units of its type on screen.
const DOUBLE_CLICK: f64 = 0.35;

/// What a key does for the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameKey {
    None,
    /// F12 or Escape: the game menu.
    Menu,
}

/// What the app should do after mouse input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameMouse {
    None,
    Menu,
    Restart,
}

/// A move marker: world position and frame.
#[derive(Debug, Clone, Copy)]
struct Marker {
    x: i32,
    y: i32,
    frame: u8,
}

/// Interface state of the game screen.
pub struct State {
    pub hud: Hud,
    cursor: Cursor,
    cursor_loaded: bool,
    /// The Pause key (op 0x46) stopped the simulation.
    pub paused: bool,
    /// Control groups 0..9.
    pub(super) groups: Vec<Vec<EntityId>>,
    markers: Vec<Marker>,
    /// Scroll speed per axis (0x4c5910) and the time not yet scrolled.
    scroll: [i32; 2],
    scroll_time: f32,
    pub scroll_speed: i32,
    /// Formation groups each officer type can lead (`[OFFICERS]` of the nation files).
    officers: Option<HashMap<String, Vec<String>>>,
    keyboard: Option<Vec<(u8, u8, String)>>,
    pub mods: ModifiersState,
    view: View,
    canvas: Canvas,
    window: [f32; 2],
    last_click: Option<(web_time::Instant, EntityId)>,
    /// Space cycles through the selection (0x4c6020).
    next_centred: usize,
}

impl Default for State {
    fn default() -> Self {
        Self {
            hud: Hud::default(),
            cursor: Cursor::default(),
            cursor_loaded: false,
            paused: false,
            groups: vec![Vec::new(); 10],
            markers: Vec::new(),
            scroll: [0; 2],
            scroll_time: 0.0,
            scroll_speed: Settings::default().scroll_speed,
            officers: None,
            keyboard: None,
            mods: ModifiersState::empty(),
            view: View::default(),
            canvas: Canvas::fit([1024.0, 768.0]),
            window: [1024.0, 768.0],
            last_click: None,
            next_centred: 0,
        }
    }
}

/// Simulation ticks per second for `FPSTime` (`1000 / (2·FPSTime)`, see docs/simulation.md);
/// the uncapped setting 0 runs at 100.
pub fn tick_rate(frame_time: i32) -> u32 {
    (1000 / (2 * frame_time.clamp(5, 100))) as u32
}

/// Map cell size in screen pixels: 32 wide, 16 high (world y is halved on screen).
const CELL: [f32; 2] = [32.0, 16.0];

impl InGame {
    /// Applies the options: game speed (tick rate) and scroll speed.
    pub fn apply_settings(&mut self, settings: &Settings) {
        self.frame_time = settings.frame_time;
        self.clock.set_rate(tick_rate(settings.frame_time));
        self.interface.scroll_speed = settings.scroll_speed.clamp(2, 10);
    }

    pub fn set_modifiers(&mut self, mods: ModifiersState) {
        self.interface.mods = mods;
    }

    fn canvas_point(&self, p: [f32; 2]) -> [f32; 2] {
        self.interface.canvas.to_virtual(p)
    }

    /// Map shift `s` (1 for normal maps).
    fn shift(&self) -> u32 {
        self.map.as_ref().and_then(|m| m.size).map_or(1, |s| u32::from(s.shift))
    }

    /// Camera position and view size in minimap pixels.
    fn minimap_camera(&self, window: [f32; 2]) -> ([f32; 2], [f32; 2]) {
        let k = (1 << self.shift()) as f32;
        let s = self.camera.scale;
        (
            [self.camera.x / (CELL[0] * k), self.camera.y / (CELL[1] * k)],
            [window[0] / s / (CELL[0] * k), window[1] / s / (CELL[1] * k)],
        )
    }

    /// Centres the camera on a minimap pixel (0x4c5c9a).
    fn centre_on_minimap(&mut self, px: [f32; 2], window: [f32; 2]) {
        let k = (1 << self.shift()) as f32;
        let s = self.camera.scale;
        self.camera.x = px[0] * CELL[0] * k - window[0] / s / 2.0;
        self.camera.y = px[1] * CELL[1] * k - window[1] / s / 2.0;
        self.clamp_camera(window);
    }

    fn clamp_camera(&mut self, window: [f32; 2]) {
        if let Some(terrain) = &self.terrain {
            let side = (terrain.width - 1) as f32 * 32.0;
            let s = self.camera.scale;
            self.camera.x = self.camera.x.clamp(-window[0] / s / 2.0, side - window[0] / s / 2.0);
            self.camera.y = self.camera.y.clamp(-window[1] / s / 2.0, side / 2.0 - window[1] / s / 2.0);
        }
    }

    /// World pixel of a minimap pixel.
    fn minimap_world(&self, px: [f32; 2]) -> (i32, i32) {
        let k = (1 << self.shift()) as f32;
        ((px[0] * 32.0 * k) as i32, (px[1] * 32.0 * k) as i32)
    }

    /// The scroll speeds of 0x4c5910, updated once per game frame: arrow keys set ±4, the
    /// cursor at the edge accelerates up to `ScrollSpeed`, otherwise the speed decays by 1;
    /// the view moves by half the speed in map cells per frame.
    pub(super) fn scroll_view(&mut self, seconds: f32, window: [f32; 2]) {
        let rate = 1.0 / self.clock_period();
        self.interface.scroll_time += seconds.min(0.25);
        let frame = 1.0 / rate;
        let edge = EDGE * self.interface.canvas.scale.max(1.0);
        while self.interface.scroll_time >= frame {
            self.interface.scroll_time -= frame;
            for axis in 0..2 {
                let (minus, plus) = if axis == 0 {
                    (NamedKey::ArrowLeft, NamedKey::ArrowRight)
                } else {
                    (NamedKey::ArrowUp, NamedKey::ArrowDown)
                };
                let v = &mut self.interface.scroll[axis];
                let at = self.cursor.filter(|_| self.drag_from.is_none()).map(|c| c[axis]);
                if self.held.contains(&minus) {
                    *v = -KEY_SCROLL;
                } else if self.held.contains(&plus) {
                    *v = KEY_SCROLL;
                } else if at.is_some_and(|c| c < edge) {
                    *v = (*v - 1).max(-self.interface.scroll_speed);
                } else if at.is_some_and(|c| c > window[axis] - edge) {
                    *v = (*v + 1).min(self.interface.scroll_speed);
                } else {
                    *v -= v.signum();
                }
            }
        }
        // Spread each frame's step over the frame so the view moves smoothly.
        let step = seconds.min(0.25) * rate;
        self.camera.x += (self.interface.scroll[0] / 2) as f32 * CELL[0] * step;
        self.camera.y += (self.interface.scroll[1] / 2) as f32 * CELL[1] * step;
        self.clamp_camera(window);
    }

    fn clock_period(&self) -> f32 {
        1.0 / tick_rate(self.frame_time) as f32
    }

    /// Advances the move markers by one game frame.
    pub(super) fn tick_markers(&mut self) {
        for m in &mut self.interface.markers {
            m.frame += 1;
        }
        self.interface.markers.retain(|m| m.frame < MARKER_FRAMES);
    }

    fn add_marker(&mut self, x: i32, y: i32) {
        self.interface.markers.push(Marker { x, y, frame: 0 });
    }

    fn own(&self) -> u8 {
        self.local_player()
    }

    fn selected_units(&self) -> impl Iterator<Item = &Unit> {
        let world = self.world.as_ref();
        self.selected.iter().filter_map(move |&id| world.and_then(|w| w.unit(id)).filter(|u| u.alive))
    }

    /// Loads the officers' formation lists from the nation files.
    fn officers(&mut self, ctx: &Ctx) -> Option<&HashMap<String, Vec<String>>> {
        if self.interface.officers.is_none() {
            let paths: Vec<String> =
                ctx.lib.list("").into_iter().filter(|p| p.ends_with(".NDS") && p != "WEAPON.NDS").collect();
            let mut files = Vec::new();
            for p in &paths {
                if !ctx.lib.settled(p) {
                    return None;
                }
                files.extend(ctx.lib.bytes(p));
            }
            let mut map = HashMap::new();
            for bytes in files {
                let nation = acr_formats::defs::NationDef::parse(&bytes);
                for g in nation.officers {
                    let names: Vec<String> = g.formations.iter().map(|f| f.name.to_ascii_uppercase()).collect();
                    if let Some(officer) = g.commanders.first() {
                        map.entry(officer.to_ascii_uppercase()).or_insert(names);
                    }
                }
            }
            self.interface.officers = Some(map);
        }
        self.interface.officers.as_ref()
    }

    fn keyboard(&mut self, ctx: &Ctx) -> Option<&Vec<(u8, u8, String)>> {
        if self.interface.keyboard.is_none() && ctx.lib.settled(KEYBOARD) {
            let text = ctx.lib.bytes(KEYBOARD).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
            let mut keys = Vec::new();
            for line in text.lines() {
                let line = line.split("//").next().unwrap_or("");
                let Some((left, icon)) = line.split_once(':') else { continue };
                let words: Vec<&str> = left.split_whitespace().collect();
                let Some(key) =
                    words.last().and_then(|k| k.bytes().next()).filter(|_| words.last().unwrap().len() == 1)
                else {
                    continue;
                };
                let mods = words[..words.len() - 1].iter().fold(0u8, |m, w| {
                    m | match w.to_ascii_uppercase().as_str() {
                        "CTRL" => 1,
                        "ALT" => 2,
                        "SHIFT" => 4,
                        _ => 0,
                    }
                });
                keys.push((mods, key.to_ascii_uppercase(), icon.trim().to_owned()));
            }
            self.interface.keyboard = Some(keys);
        }
        self.interface.keyboard.as_ref()
    }

    /// The selection's icons: one per selected type (with count and life), then the
    /// commands (0x606710).
    fn build_view(&mut self, ctx: &Ctx, window: [f32; 2]) -> View {
        let (camera, view_size) = self.minimap_camera(window);
        let player = self.own();
        let mut view = View { player, camera, view: view_size, ..View::default() };
        let Some(world) = &self.world else { return view };
        let res = &world.players[usize::from(player).min(world.players.len() - 1)].resources;
        view.resources = std::array::from_fn(|i| res[i].max(0) as u32);
        let shift = self.shift() + 9;
        for u in world.units.iter().filter(|u| u.alive) {
            let selected = u.owner == player && self.selected.contains(&u.id);
            let color = if selected { minimap::WHITE } else { minimap::PLAYER_COLORS[usize::from(u.owner % 8)] };
            view.dots.push(Dot { x: u.pos.x >> shift, y: u.pos.y >> shift, color });
        }
        let Some(data) = self.interface.hud.data(ctx) else { return view };
        // One icon per selected type, in selection order.
        let mut types: Vec<(acr_sim::TypeId, i32, i32, i32)> = Vec::new();
        for u in self.selected_units() {
            let max = world.catalog.get(u.kind).life.max(1);
            match types.iter_mut().find(|t| t.0 == u.kind) {
                Some(t) => {
                    t.1 += 1;
                    t.2 += u.life;
                    t.3 += max;
                }
                None => types.push((u.kind, 1, u.life, max)),
            }
        }
        let outline = color::PLAYER.wrapping_add(4 * (player % 8));
        let objects = self.objects.objects();
        for (slot, &(kind, count, life, max)) in types.iter().take(12).enumerate() {
            let id = &world.catalog.get(kind).id;
            let def = objects.definition(id);
            let frame = def.and_then(|d| d.icon()).and_then(|i| data.icon(i)).unwrap_or(0);
            view.icons.push(Icon {
                frame,
                slot,
                outline: Some(outline),
                life: Some((life, max)),
                count: Some(count),
                selected: false,
                disabled: false,
                hint: Vec::new(),
                action: Some(HudCommand::SelectType(kind.0 as usize)),
            });
        }
        if let [(kind, ..)] = types[..]
            && let Some(def) = objects.definition(&world.catalog.get(kind).id)
        {
            view.stats = Some(stats(def, &data));
        }
        // Formations: the officer's groups in column 8 (`ORDERICONS`), "Formation auflösen"
        // (slot 24) for a formed brigade; stand ground and "attack on the way" are shown
        // but not simulated yet.
        let officer =
            self.selected_units().find(|u| world.catalog.get(u.kind).officer).map(|u| (u.id, u.kind, u.brigade));
        let current = officer.and_then(|(_, _, b)| b).and_then(|b| world.brigades.get(usize::from(b))).map(|b| {
            let name = world.formations.shapes.get(b.shape).map(|s| s.name.clone()).unwrap_or_default();
            world
                .formations
                .groups
                .iter()
                .find(|g| g.variants.contains(&b.shape))
                .map(|g| g.name.clone())
                .unwrap_or(name)
        });
        if let Some((_, kind, brigade)) = officer {
            let officer_id = world.catalog.get(kind).id.to_ascii_uppercase();
            let groups = self.officers(ctx).and_then(|m| m.get(&officer_id).cloned()).unwrap_or_default();
            for g in &groups {
                if let Some(icon) = data.order(g) {
                    view.icons.push(Icon {
                        frame: icon.frame,
                        slot: icon.slot,
                        outline: Some(outline),
                        life: None,
                        count: None,
                        selected: current.as_deref().is_some_and(|c| c.eq_ignore_ascii_case(g)),
                        disabled: false,
                        hint: ctx.texts.get(&icon.hint).to_vec(),
                        action: Some(HudCommand::Form(g.clone())),
                    });
                }
            }
            if brigade.is_some() {
                let mut command = |name: &str, slot: usize, hint: &str, action: HudCommand| {
                    if let Some(frame) = data.icon(name) {
                        view.icons.push(Icon {
                            frame,
                            slot,
                            outline: Some(outline),
                            life: None,
                            count: None,
                            selected: false,
                            disabled: false,
                            hint: ctx.texts.get(hint).to_vec(),
                            action: Some(action),
                        });
                    }
                };
                command("Stand_Ground_OFF", 24, "SFREORD", HudCommand::Disband);
                command("Stand_Ground_ON", 25, "STANDGR", HudCommand::Inactive);
                command("Stand_Ground_OFF", 26, "NOSTAND", HudCommand::Inactive);
                if let Some(go) = data.consts.get("GO&ATTACK") {
                    command("Go_With_Attack", go.slot, "GONATT", HudCommand::Inactive);
                }
            }
        }
        view
    }

    /// Runs a command of the interface.
    fn hud_command(&mut self, command: HudCommand) {
        let player = self.own();
        let Some(world) = &self.world else { return };
        match command {
            HudCommand::SelectType(kind) => {
                let keep: Vec<EntityId> = self
                    .selected
                    .iter()
                    .copied()
                    .filter(|&id| world.unit(id).is_some_and(|u| u.kind.0 as usize == kind))
                    .collect();
                self.selected = keep;
            }
            HudCommand::Form(group) => {
                let Some(officer) = self.selected_units().find(|u| world.catalog.get(u.kind).officer) else { return };
                let (officer, brigade) = (officer.id, officer.brigade);
                let mut units: Vec<EntityId> = self.selected.clone();
                if let Some(b) = brigade.and_then(|b| world.brigades.get(usize::from(b))) {
                    // Re-forming: the formation's soldiers form the new one.
                    units.extend(b.members.iter().flatten().copied());
                    units.sort_by_key(|e| e.0);
                    units.dedup();
                    self.commands.push(Command::Disband { player, officer });
                }
                self.commands.push(Command::Form { player, officer, units, kind: group });
                self.unit_sound(officer, "ORDER_SOUND");
            }
            HudCommand::Disband => {
                let officers: Vec<EntityId> = self
                    .selected_units()
                    .filter(|u| u.brigade.is_some() && world.catalog.get(u.kind).officer)
                    .map(|u| u.id)
                    .collect();
                for officer in officers {
                    self.commands.push(Command::Disband { player, officer });
                }
            }
            HudCommand::Inactive => {}
        }
    }

    /// Draw layers of the game screen: the overlays over the map (world coordinates), the
    /// selection rectangle (window coordinates) and the interface (canvas coordinates);
    /// plus the minimap image when it changed and the cursor frame with its hotspot.
    #[allow(clippy::type_complexity)]
    pub fn interface(
        &mut self,
        ctx: &Ctx,
        window: [f32; 2],
        seconds: f32,
    ) -> (Vec<(Canvas, Vec<Draw>)>, Option<Image>, Option<(usize, [i32; 2])>) {
        self.interface.canvas = Canvas::fit(window);
        self.interface.window = window;
        if !self.interface.hud.has_minimap()
            && let (Some(map), Some(assets)) = (&self.map, &self.assets)
            && let Some(m) = Minimap::new(map, assets)
        {
            self.interface.hud.set_minimap(m);
        }
        if !self.interface.cursor_loaded && ctx.lib.settled(cursor::FILE) {
            if let Some(bytes) = ctx.lib.bytes(cursor::FILE) {
                self.interface.cursor = Cursor::parse(&bytes);
            }
            self.interface.cursor_loaded = true;
        }
        let _ = self.keyboard(ctx);
        let view = self.build_view(ctx, window);
        let mut hud_draws = Vec::new();
        self.interface.hud.draw(ctx, &view, &mut hud_draws);
        if self.interface.paused {
            let text = ctx.texts.get("PAUSETEXT");
            let w = ctx.fonts.width(ctx.lib, hud::font::YELLOW, text);
            hud_draws.push(Draw::game_text(hud::font::YELLOW, text, (hud::SCREEN[0] - w) / 2, hud::SCREEN[1] / 2));
        }
        let image = self.interface.hud.minimap_image(ctx, &view);
        self.interface.view = view;
        // The cursor (0x4c1f70): the arrow, the sword over an enemy with an own selection.
        let over_hud = self.cursor.is_some_and(|c| {
            let p = self.canvas_point(c);
            self.interface.hud.covers(ctx, &self.interface.view, p)
        });
        let frame = if !over_hud && !self.selected.is_empty() && self.unit_at_cursor(false).is_some() {
            cursor::frame::SWORD
        } else {
            cursor::frame::ARROW
        };
        self.interface.cursor.set(frame);
        self.interface.cursor.tick(f64::from(seconds));
        let c = &self.interface.cursor;
        let cursor = Some((c.frame, c.hotspot()));
        let mut layers = vec![(self.world_canvas(), self.world_overlays(ctx))];
        if let Some(rect) = self.selection_rectangle(ctx) {
            layers.push((Canvas { scale: 1.0, origin: [0.0, 0.0] }, rect));
        }
        layers.push((self.interface.canvas, hud_draws));
        (layers, image, cursor)
    }

    /// Window = (world screen position − camera) · scale.
    fn world_canvas(&self) -> Canvas {
        let s = self.camera.scale;
        Canvas { scale: s, origin: [-self.camera.x * s, -self.camera.y * s] }
    }

    /// Selection brackets, life bars (Tab) and the move markers, in world screen pixels.
    fn world_overlays(&self, ctx: &Ctx) -> Vec<Draw> {
        let mut out = Vec::new();
        let (Some(world), Some(terrain), Some(palette)) = (&self.world, &self.terrain, ctx.lib.palette(GAME_PALETTE))
        else {
            return out;
        };
        let rgba = |i: u8| {
            let [r, g, b] = palette.rgb(i);
            [r, g, b, 255]
        };
        let bracket = rgba(nearest(&palette, color::BRACKETS));
        let green = rgba(nearest(&palette, color::LIFE));
        let white = rgba(color::WHITE);
        let fill = |out: &mut Vec<Draw>, c: [u8; 4], x0: i32, y0: i32, x1: i32, y1: i32| {
            out.push(Draw::fill(c, Rect { x0, y0, x1: x1 + 1, y1: y1 + 1 }).unclipped());
        };
        let objects = self.objects.objects();
        let own = self.own();
        for u in world.units.iter().filter(|u| u.alive) {
            let selected = u.owner == own && self.selected.contains(&u.id);
            if !selected && !self.interface.hud.life_bars {
                continue;
            }
            let kind = world.catalog.get(u.kind);
            // `RECTANGLE dx dy w h` (defaults 0, 3, 40, 64; 0x4bad90).
            let r = objects.definition(&kind.id).and_then(|d| d.get("RECTANGLE"));
            let [dx, dy, w, h] = [0, 1, 2, 3].map(|i| r.and_then(|c| c.int(i)).unwrap_or([0, 3, 40, 64][i]));
            let (x, y) = (u.pos.x >> 4, u.pos.y >> 4);
            let x0 = x - w / 2 + dx;
            let y0 = (y >> 1) - h + dy - terrain.height_at(x, y);
            let (x1, y1) = (x0 + w - 1, y0 + h - 1);
            // Soldiers of a formed formation get no brackets of their own (0x4bae50).
            let in_formation = u.brigade.is_some() && !kind.officer;
            if selected && !u.building && !in_formation {
                for (cx, cy, sx, sy) in [(x0, y0, 1, 1), (x1, y0, -1, 1), (x0, y1, 1, -1), (x1, y1, -1, -1)] {
                    fill(&mut out, bracket, cx.min(cx + 2 * sx), cy, cx.max(cx + 2 * sx), cy);
                    fill(&mut out, bracket, cx, cy.min(cy + 2 * sy), cx, cy.max(cy + 2 * sy));
                }
            }
            if self.interface.hud.life_bars && !u.building {
                // Life bar (0x4bb390): two green lines above the box, white end caps.
                let max = kind.life.max(1);
                let l = (w - 12) * u.life.clamp(0, max) / max;
                fill(&mut out, green, x0 + 5, y0 - 12, x0 + l + 7, y0 - 11);
                fill(&mut out, white, x0 + 4, y0 - 12, x0 + 4, y0 - 11);
                fill(&mut out, white, x0 + w - 5, y0 - 12, x0 + w - 5, y0 - 11);
            }
        }
        for m in &self.interface.markers {
            let h = terrain.height_at(m.x, m.y);
            out.push(
                Draw::game_frame(hud::gp::CURSGO, usize::from(m.frame), m.x - 24, (m.y >> 1) - 17 - h, 0).unclipped(),
            );
        }
        out
    }

    /// The drag rectangle (0x4c4356), 1 pixel in `GetPaletteColor(0, 255, 255)`.
    fn selection_rectangle(&self, ctx: &Ctx) -> Option<Vec<Draw>> {
        let (Some(from), Some(at)) = (self.drag_from, self.cursor) else { return None };
        let palette = ctx.lib.palette(GAME_PALETTE)?;
        let [red, green, blue] = palette.rgb(nearest(&palette, color::RECTANGLE));
        let r = Rect {
            x0: from[0].min(at[0]) as i32,
            y0: from[1].min(at[1]) as i32,
            x1: from[0].max(at[0]) as i32 + 1,
            y1: from[1].max(at[1]) as i32 + 1,
        };
        let mut out = Vec::new();
        hud::outline(&mut out, r, [red, green, blue, 255]);
        Some(out.into_iter().map(|d| d.unclipped()).collect())
    }

    /// The nearest unit under the cursor: own units (`own`) or others.
    fn unit_at_cursor(&self, own: bool) -> Option<EntityId> {
        let me = self.own();
        self.unit_at(self.cursor?, |u| (u.owner == me) == own)
    }

    /// The unit whose box (`RECTANGLE dx dy w h`, as for the selection brackets) contains
    /// the window point, the front-most if several do, among the living units passing `keep`.
    pub(super) fn unit_at(&self, at: [f32; 2], keep: impl Fn(&Unit) -> bool) -> Option<EntityId> {
        let (world, terrain) = (self.world.as_ref()?, self.terrain.as_ref()?);
        let s = self.camera.scale;
        let (px, py) = ((self.camera.x + at[0] / s) as i32, (self.camera.y + at[1] / s) as i32);
        let objects = self.objects.objects();
        world
            .units
            .iter()
            .filter(|u| u.alive && keep(u))
            .filter(|u| {
                let r = objects.definition(&world.catalog.get(u.kind).id).and_then(|d| d.get("RECTANGLE"));
                let [dx, dy, w, h] = [0, 1, 2, 3].map(|i| r.and_then(|c| c.int(i)).unwrap_or([0, 3, 40, 64][i]));
                let (x, y) = (u.pos.x >> 4, u.pos.y >> 4);
                let x0 = x - w / 2 + dx;
                let y0 = (y >> 1) - h + dy - terrain.height_at(x, y);
                (x0..x0 + w).contains(&px) && (y0..y0 + h).contains(&py)
            })
            .max_by_key(|u| u.pos.y)
            .map(|u| u.id)
    }

    /// Left button. Returns what the app should do.
    pub fn left_button(&mut self, ctx: &Ctx, pressed: bool) -> GameMouse {
        let Some(at) = self.cursor else { return GameMouse::None };
        let p = self.canvas_point(at);
        if pressed && self.drag_from.is_none() {
            let view = std::mem::take(&mut self.interface.view);
            let event = self.interface.hud.press(ctx, &view, p);
            self.interface.view = view;
            match event {
                HudEvent::None => {}
                HudEvent::Camera(px) => {
                    let window = self.window_size();
                    self.centre_on_minimap(px, window);
                    return GameMouse::None;
                }
                HudEvent::Command(c) => {
                    self.hud_command(c);
                    return GameMouse::None;
                }
                HudEvent::GameMenu => return GameMouse::Menu,
                HudEvent::Restart => return GameMouse::Restart,
                _ => return GameMouse::None,
            }
        }
        if !pressed && self.drag_from.is_none() {
            self.interface.hud.release();
            return GameMouse::None;
        }
        let before = self.selected.clone();
        self.on_left(pressed);
        if !pressed {
            self.after_selection(before);
        }
        GameMouse::None
    }

    /// Shift adds to the selection; a second click on a unit selects all own units of its
    /// type on the screen.
    fn after_selection(&mut self, before: Vec<EntityId>) {
        let now = web_time::Instant::now();
        if let [one] = self.selected[..] {
            if let Some((t, id)) = self.interface.last_click
                && id == one
                && (now - t).as_secs_f64() < DOUBLE_CLICK
                && let Some(world) = &self.world
            {
                let kind = world.unit(one).map(|u| u.kind);
                let window = self.window_size();
                let own = self.own();
                self.selected = world
                    .units
                    .iter()
                    .filter(|u| u.alive && u.owner == own && Some(u.kind) == kind)
                    .filter(|u| {
                        let [sx, sy] = self.screen_of(u);
                        (0.0..window[0]).contains(&sx) && (0.0..window[1]).contains(&sy)
                    })
                    .map(|u| u.id)
                    .collect();
                self.interface.last_click = None;
            } else {
                self.interface.last_click = Some((now, one));
            }
        }
        if self.interface.mods.shift_key() {
            let mut all = before;
            for id in std::mem::take(&mut self.selected) {
                if !all.contains(&id) {
                    all.push(id);
                }
            }
            self.selected = all;
        }
    }

    /// Mouse moved (window position).
    pub fn mouse_moved(&mut self, at: [f32; 2]) {
        self.cursor = Some(at);
        let p = self.canvas_point(at);
        if self.drag_from.is_none() {
            let view = std::mem::take(&mut self.interface.view);
            let event = self.interface.hud.drag(&view, p);
            self.interface.view = view;
            if let HudEvent::Camera(px) = event {
                let window = self.window_size();
                self.centre_on_minimap(px, window);
            }
        }
    }

    /// Right button: an order on the minimap, an attack on an enemy, else a move with the
    /// move marker.
    pub fn right_button(&mut self, ctx: &Ctx) {
        let Some(at) = self.cursor else { return };
        let p = self.canvas_point(at);
        let view = std::mem::take(&mut self.interface.view);
        let event = self.interface.hud.right_press(ctx, &view, p);
        self.interface.view = view;
        let player = self.own();
        match event {
            HudEvent::Order(px) if !self.selected.is_empty() => {
                let (x, y) = self.minimap_world(px);
                self.commands.push(Command::Move { player, units: self.selected.clone(), x, y });
                self.unit_sound(self.selected[0], "ORDER_SOUND");
            }
            HudEvent::None => {
                if self.selected.is_empty() {
                    return;
                }
                // An enemy under the cursor is attacked; a plain move shows the marker.
                let attack = self.unit_at_cursor(false).is_some();
                let (x, y) = self.world_at(at);
                self.on_right();
                if !attack {
                    self.add_marker(x, y);
                }
            }
            _ => {}
        }
    }

    fn window_size(&self) -> [f32; 2] {
        self.interface.window
    }

    /// A key of the game screen (0x4a30e8 and `Keyboard.dat`).
    pub fn key(&mut self, ctx: &Ctx, key: &Key, pressed: bool) -> GameKey {
        if let Key::Named(named) = key {
            self.on_key(*named, pressed);
        }
        if !pressed {
            return GameKey::None;
        }
        let mods = self.interface.mods;
        match key {
            Key::Named(NamedKey::F12 | NamedKey::Escape) => return GameKey::Menu,
            Key::Named(NamedKey::Pause) => self.interface.paused = !self.interface.paused,
            Key::Named(NamedKey::Tab) => self.interface.hud.life_bars = !self.interface.hud.life_bars,
            Key::Named(NamedKey::Space) => self.centre_next(),
            Key::Character(c) => {
                let Some(ch) = c.chars().next().filter(|ch| ch.is_ascii()) else { return GameKey::None };
                let ch = ch.to_ascii_uppercase() as u8;
                if let Some(n) = (ch as char).to_digit(10) {
                    self.control_group(n as usize, mods);
                } else if ch == b'M' && mods.is_empty() {
                    self.interface.hud.toggle_minimap();
                } else if ch == b'D' && mods.control_key() {
                    // Ctrl+D: slow/fast game mode (op 0x45); the simulation has one mode.
                } else {
                    self.icon_hotkey(ctx, ch, mods);
                }
            }
            _ => {}
        }
        GameKey::None
    }

    /// Ctrl+N stores the selection as group N, N recalls it, Shift+N adds it.
    fn control_group(&mut self, n: usize, mods: ModifiersState) {
        if mods.control_key() {
            self.interface.groups[n] = self.selected.clone();
            return;
        }
        let alive: Vec<EntityId> = self.interface.groups[n]
            .iter()
            .copied()
            .filter(|&id| self.world.as_ref().and_then(|w| w.unit(id)).is_some_and(|u| u.alive))
            .collect();
        if mods.shift_key() {
            for id in alive {
                if !self.selected.contains(&id) {
                    self.selected.push(id);
                }
            }
        } else {
            self.selected = alive;
        }
    }

    /// Space: the camera jumps to the next selected unit (0x4c6020).
    fn centre_next(&mut self) {
        let units: Vec<(i32, i32)> = self.selected_units().map(|u| (u.pos.x >> 4, u.pos.y >> 4)).collect();
        if units.is_empty() {
            return;
        }
        let i = self.interface.next_centred % units.len();
        self.interface.next_centred = i + 1;
        let (x, y) = units[i];
        let window = self.window_size();
        let s = self.camera.scale;
        self.camera.x = x as f32 - window[0] / s / 2.0;
        self.camera.y = (y / 2) as f32 - window[1] / s / 2.0;
        self.clamp_camera(window);
    }

    /// Letter keys of `Keyboard.dat`: press the shown icon of that name.
    fn icon_hotkey(&mut self, ctx: &Ctx, key: u8, mods: ModifiersState) {
        let flags = u8::from(mods.control_key()) | (u8::from(mods.alt_key()) << 1) | (u8::from(mods.shift_key()) << 2);
        let Some(data) = self.interface.hud.data(ctx) else { return };
        let names: Vec<String> = self
            .keyboard(ctx)
            .map(|k| k.iter().filter(|e| e.0 == flags && e.1 == key).map(|e| e.2.clone()).collect())
            .unwrap_or_default();
        for name in names {
            let Some(frame) = data.icon(&name) else { continue };
            let hit = self
                .interface
                .view
                .icons
                .iter()
                .find(|i| i.frame == frame && !i.disabled)
                .and_then(|i| i.action.clone());
            if let Some(action) = hit {
                self.hud_command(action);
                return;
            }
        }
    }
}

/// The numbers of a unit type (0x606edd): damage per attack (with its weapon kind's icon),
/// the shield and the protections, from the `.MD` file.
fn stats(def: &acr_formats::defs::UnitDef, data: &hud::data::HudData) -> Stats {
    let kinds: HashMap<i32, usize> =
        def.all("WEAPONKIND").filter_map(|c| Some((c.int(0)?, data.weapon(c.word(1)?)?))).collect();
    let mut attacks = Vec::new();
    for c in def.all("DAMAGE") {
        let (Some(i), Some(v)) = (c.int(0), c.int(1)) else { continue };
        if v == 0 || attacks.len() >= 2 {
            continue;
        }
        let icon = kinds.get(&i).and_then(|&k| data.weapons.get(k)).map_or(0, |w| w.1);
        attacks.push((icon, v));
    }
    let shield = def.get("SHIELD").and_then(|c| c.int(0)).unwrap_or(0);
    let mut protections: Vec<(usize, usize, i32)> = def
        .all("PROTECTION")
        .flat_map(|c| c.word_int_pairs(0))
        .filter(|&(_, v)| v != 0)
        .filter_map(|(k, v)| {
            let i = data.weapon(k)?;
            Some((i, data.weapons[i].2, v))
        })
        .collect();
    protections.sort_by_key(|p| p.0);
    protections.dedup_by_key(|p| p.0);
    Stats { attacks, shield, protections: protections.into_iter().map(|(_, icon, v)| (icon, v)).take(6).collect() }
}
