//! The in-game interface hooked into the game: what the HUD shows of the world, mouse and
//! keyboard input of the game screen, scrolling, and the overlays drawn over the map
//! (selection brackets, life bars, the move marker, the selection rectangle).

use std::collections::HashMap;
use std::rc::Rc;

use acr_formats::raster::Image;
use acr_sim::world::EntityId;
use acr_sim::{Command, TypeId, Unit};
use winit::keyboard::{Key, ModifiersState, NamedKey};

use super::InGame;
use crate::canvas::Canvas;
use crate::hud::cursor::{self, Cursor};
use crate::hud::data::HudData;
use crate::hud::minimap::{self, Dot, Minimap};
use crate::hud::produce::Nations;
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
/// Colour tables of the building preview (`ShowGPDarkN` / `ShowGPRedN`, loaded at 0x483feb
/// and 0x4840d1): a free site and a blocked one.
const PREVIEW_FREE: &str = "0\\AGEW_1W.GRD";
const PREVIEW_BLOCKED: &str = "0\\AGEW_1R.GRD";
/// `NRES.DAT [RESTYPES]` names, the text ids of the resources in cost hints (0x606229).
const RESOURCE_NAMES: [&str; 6] = ["Wood", "Gold", "Stone", "Food", "Iron", "Coal"];
/// Shift repeats production orders five times (0x60897d).
const SHIFT_REPEAT: usize = 5;
/// The zoomed-out view draws 8 screen pixels per 32-pixel cell (the world shift 0x90d6b4
/// goes from 5 to 3).
const ZOOM_SCALE: f32 = 0.25;

/// Slot of the icon that lets the units out of a building.
const LEAVE_SLOT: usize = 11 + 12;

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
    /// Open the mission history (`UM_HIST`).
    History,
}

/// What a right click on the map orders (and the cursor shown for it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Context {
    Attack(EntityId),
    Repair(EntityId),
    Enter(EntityId),
    /// World pixel and source type.
    Gather(i32, i32, u8),
    Move(i32, i32),
    Rally(i32, i32),
}

impl Context {
    /// The cursor frame (globals 0x66a904..0x66a934).
    fn cursor(self) -> usize {
        match self {
            Context::Attack(_) => cursor::frame::SWORD,
            Context::Repair(_) => cursor::frame::BUILD,
            Context::Enter(_) => cursor::frame::ENTER,
            Context::Gather(_, _, kind) => {
                cursor::frame::GATHER.get(usize::from(kind)).map_or(cursor::frame::ARROW, |&f| usize::from(f))
            }
            Context::Move(..) | Context::Rally(..) => cursor::frame::ARROW,
        }
    }
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
    /// Production lists, availability and officers of the nation files.
    nations: Option<Rc<Nations>>,
    /// A building being placed (`BuildMode` 0x10fc644, `BuildingID` 0x10fca60).
    pub placing: Option<TypeId>,
    /// The next click on the map sets the selected buildings' rally point.
    rally_mode: bool,
    /// Shown page of the icons per building type (`+0x183c` of its type).
    pages: HashMap<TypeId, u8>,
    /// Start of the game screen, for the pulsing preview.
    started: web_time::Instant,
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
            nations: None,
            placing: None,
            rally_mode: false,
            pages: HashMap::new(),
            started: web_time::Instant::now(),
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

    /// Reads the nation files once they are loaded.
    fn nations(&mut self, ctx: &Ctx) -> Option<Rc<Nations>> {
        if self.interface.nations.is_none() {
            let paths: Vec<String> =
                ctx.lib.list("").into_iter().filter(|p| p.ends_with(".NDS") && p != "WEAPON.NDS").collect();
            let mut files = Vec::new();
            for p in &paths {
                if !ctx.lib.settled(p) {
                    return None;
                }
                files.extend(ctx.lib.bytes(p));
            }
            self.interface.nations = Some(Rc::new(Nations::parse(files.iter().map(|b| &b[..]))));
        }
        self.interface.nations.clone()
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
        let nations = self.nations(ctx);
        let Some(world) = &self.world else { return view };
        let res = &world.players[usize::from(player).min(world.players.len() - 1)].resources;
        view.resources = std::array::from_fn(|i| res[i].max(0) as u32);
        view.history = self.mission.has_history();
        let shift = self.shift() + 9;
        for u in world.units.iter().filter(|u| u.alive && u.inside_of.is_none()) {
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
        if let [(kind, ..)] = types[..]
            && let Some(def) = objects.definition(&world.catalog.get(kind).id)
        {
            view.stats = Some(stats(def, &data));
        }
        if let ([(kind, ..)], Some(nations)) = (&types[..], &nations) {
            self.production_icons(ctx, &data, nations, *kind, outline, &mut view.icons);
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
            let groups = nations.as_ref().and_then(|n| n.officers.get(&officer_id).cloned()).unwrap_or_default();
            for g in &groups {
                if let Some(icon) = data.order(g) {
                    view.icons.push(Icon {
                        frame: icon.frame,
                        slot: icon.slot,
                        outline: Some(outline),
                        life: None,
                        count: None,
                        selected: current.as_deref().is_some_and(|c| c.eq_ignore_ascii_case(g)),
                        hint: ctx.texts.get(&icon.hint).to_vec(),
                        action: Some(HudCommand::Form(g.clone())),
                        ..Icon::default()
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
                            hint: ctx.texts.get(hint).to_vec(),
                            action: Some(action),
                            ..Icon::default()
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
        // The type icons take the first slots the commands leave free (`AddIcon` without a
        // position, 0x495631).
        let mut free = (0..36).filter(|s| !view.icons.iter().any(|i| i.slot == *s)).collect::<Vec<_>>().into_iter();
        for &(kind, count, life, max) in types.iter().take(12) {
            let Some(slot) = free.next() else { break };
            let id = &world.catalog.get(kind).id;
            let def = objects.definition(id);
            let frame = def.and_then(|d| d.icon()).and_then(|i| data.icon(i)).unwrap_or(0);
            view.icons.push(Icon {
                frame,
                slot,
                outline: Some(outline),
                life: Some((life, max)),
                count: Some(count),
                action: Some(HudCommand::SelectType(kind.0 as usize)),
                ..Icon::default()
            });
        }
        view
    }

    /// The production icons of a single selected type (0x609230): the available items of its
    /// `[FIXED_PRODUCE]` list at `x + 12·y` of the shown page (a later item in the same slot
    /// replaces an earlier one, as `AddIcon` reuses the slot); for buildings also the page
    /// buttons, the rally point and the leave icon.
    fn production_icons(
        &self,
        ctx: &Ctx,
        data: &HudData,
        nations: &Nations,
        kind: TypeId,
        outline: u8,
        icons: &mut Vec<Icon>,
    ) {
        let Some(world) = &self.world else { return };
        let player = self.own();
        let catalog = &world.catalog;
        let t = catalog.get(kind);
        let objects = self.objects.objects();
        let finished = |u: &Unit| !u.building || u.stage >= world.max_stage(u);
        let working: Vec<&Unit> =
            self.selected_units().filter(|u| u.kind == kind && u.owner == player && finished(u)).collect();
        if working.is_empty() {
            return;
        }
        // What the selected objects can make now (enabled, accessible, their stages done):
        // the simulation's tech tree; slots and keys come from `[FIXED_PRODUCE]`.
        let mut makeable: Vec<TypeId> = Vec::new();
        for u in &working {
            for k in acr_sim::economy::producible(world, u.id.0 as usize) {
                if !makeable.contains(&k) {
                    makeable.push(k);
                }
            }
        }
        let stock = &world.players[usize::from(player)].resources;
        let page = self.interface.pages.get(&kind).copied().unwrap_or(0);
        let mut pages_used = [false; 3];
        for item in nations.items(&t.id) {
            let p = item.slot / 36;
            let Some(k) = catalog.find(&item.id).filter(|k| makeable.contains(k)) else { continue };
            if p >= 3 {
                continue;
            }
            // Page flags 0x28cff78..7a: set by icons outside column 0.
            pages_used[p] |= !item.slot.is_multiple_of(12);
            let it = catalog.get(k);
            if p != usize::from(page) {
                continue;
            }
            let frame = objects.definition(&it.id).and_then(|d| d.icon()).and_then(|i| data.icon(i)).unwrap_or(0);
            let cost = acr_sim::economy::unit_cost(world, player, k, None);
            let mut icon = Icon {
                frame,
                slot: item.slot % 36,
                outline: Some(outline),
                gray: stock.iter().zip(&cost).any(|(s, c)| s < c),
                key: item.key,
                hint: cost_hint(unit_name(ctx, &it.md_name), &it.economy.price, &cost, ctx),
                ..Icon::default()
            };
            if it.building && !t.building {
                icon.action = Some(HudCommand::Place(usize::from(k.0)));
            } else {
                let queued: usize = working.iter().map(|b| b.queue.iter().filter(|q| q.kind == k).count()).sum();
                icon.count = (queued > 0).then_some(queued as i32);
                icon.life = working
                    .iter()
                    .find_map(|b| b.queue.first().filter(|q| q.kind == k).map(|q| (q.progress, q.stages.max(1))));
                icon.action = Some(HudCommand::Produce(usize::from(k.0)));
                icon.right = Some(HudCommand::Unproduce(usize::from(k.0)));
            }
            icons.retain(|i| i.slot != icon.slot);
            icons.push(icon);
        }
        // Upgrades (entries of kind 1 in 0x609230): those the buildings offer now, and the
        // research running in them, at their `#POSITION`.
        let tech = &catalog.tech;
        let mut upgrades: Vec<(acr_sim::tech::UpgradeId, Option<(i32, i32)>)> = Vec::new();
        for u in &working {
            if let Some(r) = &u.research {
                upgrades.retain(|e| e.0 != r.upgrade);
                upgrades.push((r.upgrade, Some((r.stage, r.stages.max(1)))));
            }
            for up in acr_sim::upgrade::available(world, u.id.0 as usize) {
                if !upgrades.iter().any(|e| e.0 == up) {
                    upgrades.push((up, None));
                }
            }
        }
        for (up, progress) in upgrades {
            let def = tech.upgrade(up);
            let (x, y) = def.position;
            if x < 0 || y < 0 {
                continue;
            }
            let slot = (x + 12 * y) as usize;
            let p = slot / 36;
            if p >= 3 {
                continue;
            }
            pages_used[p] |= !slot.is_multiple_of(12);
            if p != usize::from(page) {
                continue;
            }
            let mut icon = Icon {
                frame: data.icon(&def.icon).unwrap_or(0),
                slot: slot % 36,
                outline: Some(outline),
                gray: progress.is_none() && stock.iter().zip(&def.cost).any(|(s, c)| s < c),
                hint: upgrade_hint(ctx, &def.message, &def.cost),
                life: progress,
                ..Icon::default()
            };
            if progress.is_some() {
                icon.right = Some(HudCommand::CancelUpgrade(up.0));
            } else {
                icon.action = Some(HudCommand::Upgrade(up.0));
            }
            icons.retain(|i| i.slot != icon.slot);
            icons.push(icon);
        }
        if !t.building {
            return;
        }
        // Page buttons in column 0 (`PAGE1..3` and `PAGE1HINT..` of the building, 0x609b18).
        let def = objects.definition(&t.id);
        if pages_used[1] || pages_used[2] {
            for (p, _) in pages_used.iter().enumerate().filter(|(_, used)| **used) {
                let word = |key: String| def.and_then(|d| d.get(&key)).and_then(|c| c.word(0)).map(str::to_owned);
                let Some(frame) = word(format!("PAGE{}", p + 1)).and_then(|w| data.icon(&w)) else { continue };
                let hint = word(format!("PAGE{}HINT", p + 1)).map(|h| ctx.texts.get(&h).to_vec()).unwrap_or_default();
                icons.retain(|i| i.slot != 12 * p);
                icons.push(Icon {
                    frame,
                    slot: 12 * p,
                    selected: p == usize::from(page),
                    hint,
                    action: Some(HudCommand::Page(p as u8)),
                    ..Icon::default()
                });
            }
        }
        // `[CANSETDEST]` buildings have a rally point.
        let can_rally = !tech.loaded || tech.of(kind).is_some_and(|t| t.can_set_dest);
        if can_rally && let Some(rally) = data.consts.get("SET_DEST") {
            icons.push(Icon {
                frame: rally.frame,
                slot: rally.slot,
                selected: self.interface.rally_mode,
                hint: ctx.texts.get(&rally.hint).to_vec(),
                action: Some(HudCommand::Rally),
                ..Icon::default()
            });
        }
        let inside: Vec<EntityId> = working.iter().flat_map(|b| b.inside.iter().copied()).collect();
        if let Some(first) = inside.first().and_then(|&id| world.unit(id)) {
            let it = catalog.get(first.kind);
            let frame = objects.definition(&it.id).and_then(|d| d.icon()).and_then(|i| data.icon(i)).unwrap_or(0);
            icons.retain(|i| i.slot != LEAVE_SLOT);
            icons.push(Icon {
                frame,
                slot: LEAVE_SLOT,
                outline: Some(outline),
                count: Some(inside.len() as i32),
                hint: unit_name(ctx, &it.md_name),
                action: Some(HudCommand::Leave),
                ..Icon::default()
            });
        }
    }

    /// Production orders for the selected buildings (`Produce` 0x4d6226, `UnProduce`
    /// 0x4d6e08), five with Shift. A new unit goes to the building with the shortest queue,
    /// a cancel to the one with the most units of the type queued.
    fn queue_orders(&mut self, kind: TypeId, produce: bool) {
        let player = self.own();
        let mut queues: Vec<(EntityId, usize, usize)> = self
            .selected_units()
            .filter(|u| u.building && u.owner == player)
            .map(|u| (u.id, u.queue.len(), u.queue.iter().filter(|q| q.kind == kind).count()))
            .collect();
        let n = if self.interface.mods.shift_key() { SHIFT_REPEAT } else { 1 };
        for _ in 0..n {
            let pick = if produce {
                queues.iter_mut().min_by_key(|q| q.1)
            } else {
                queues.iter_mut().filter(|q| q.2 > 0).max_by_key(|q| q.2)
            };
            let Some(q) = pick else { break };
            let building = q.0;
            if produce {
                q.1 += 1;
                self.commands.push(Command::Produce { player, building, kind });
            } else {
                q.1 -= 1;
                q.2 -= 1;
                self.commands.push(Command::Unproduce { player, building, kind });
            }
        }
    }

    /// Where the building being placed would stand at a world pixel (0x5142c2): the snapped
    /// position if it is free, else the nearest free one within 10 rings of 16 px cells
    /// (`0x28a6f10`, approximated by square rings); `None` if there is none.
    fn placement_site(&self, kind: TypeId, x: i32, y: i32) -> Option<(i32, i32)> {
        let world = self.world.as_ref()?;
        let site = |x: i32, y: i32| acr_sim::construction::site(world, kind, x, y).map(|p| p.pixels());
        if let Some(p) = site(x, y) {
            return Some(p);
        }
        let (bx, by) = (x & !15, y & !15);
        for r in 1..10i32 {
            for dy in -r..=r {
                for dx in -r..=r {
                    if (dx.abs() == r || dy.abs() == r)
                        && let Some(p) = site(bx + 16 * dx, by + 16 * dy)
                    {
                        return Some(p);
                    }
                }
            }
        }
        None
    }

    /// What a right click at a window point would order the selection to do, with the
    /// cursor shown for it (0x4c4668..0x4c4ce2).
    pub(super) fn context_order(&self, at: [f32; 2]) -> Option<Context> {
        let world = self.world.as_ref()?;
        let own = self.own();
        let units: Vec<&Unit> = self.selected_units().filter(|u| u.owner == own && u.inside_of.is_none()).collect();
        if units.is_empty() {
            return None;
        }
        // Buildings only: the rally point of those that have one (`[CANSETDEST]`).
        if units.iter().all(|u| u.building) {
            let tech = &world.catalog.tech;
            let rally = units.iter().any(|u| !tech.loaded || tech.of(u.kind).is_some_and(|t| t.can_set_dest));
            let (x, y) = self.world_at(at);
            return rally.then_some(Context::Rally(x, y));
        }
        if let Some(target) = self.unit_at(at, |u| u.owner != own) {
            return Some(Context::Attack(target));
        }
        if let Some(b) = self.unit_at(at, |u| u.owner == own && u.building) {
            let builders = units.iter().any(|u| world.catalog.get(u.kind).anims.contains_key("#WORK"));
            // The hammer unless Ctrl is held (0x4c49df).
            if builders
                && !self.interface.mods.control_key()
                && acr_sim::construction::needs_work(world, b.0 as usize, own)
            {
                return Some(Context::Repair(b));
            }
            let building = world.unit(b)?;
            if units.iter().any(|u| !u.building && acr_sim::economy::can_enter(world, u, building)) {
                return Some(Context::Enter(b));
            }
        }
        let (x, y) = self.world_at(at);
        if let Some(kind) = world.sources.determine(x, y)
            && units
                .iter()
                .any(|u| world.catalog.get(u.kind).economy.take_res.get(usize::from(kind)).is_some_and(Option::is_some))
        {
            return Some(Context::Gather(x, y, kind));
        }
        Some(Context::Move(x, y))
    }

    /// Right click on the map: the context order (attack, repair, enter, gather, rally point,
    /// move) for the selection.
    pub(super) fn order_at(&mut self, at: [f32; 2]) {
        let Some(context) = self.context_order(at) else { return };
        let player = self.own();
        let units: Vec<EntityId> =
            self.selected_units().filter(|u| u.owner == player && !u.building).map(|u| u.id).collect();
        let command = match context {
            Context::Attack(target) => Command::Attack { player, units, target },
            Context::Repair(building) => Command::Repair { player, units, building },
            Context::Enter(building) => Command::Enter { player, units, building },
            Context::Gather(x, y, _) => Command::Gather { player, units, x, y },
            Context::Move(x, y) => {
                self.add_marker(x, y);
                Command::Move { player, units, x, y }
            }
            Context::Rally(x, y) => {
                self.set_rally(x, y);
                return;
            }
        };
        self.commands.push(command);
        self.unit_sound(self.selected[0], "ORDER_SOUND");
    }

    /// Sets the rally point of the selected buildings (`SET_DEST`).
    fn set_rally(&mut self, x: i32, y: i32) {
        let player = self.own();
        let buildings: Vec<EntityId> =
            self.selected_units().filter(|u| u.owner == player && u.building).map(|u| u.id).collect();
        for building in buildings {
            self.commands.push(Command::Rally { player, building, x, y });
        }
        self.add_marker(x, y);
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
            HudCommand::Produce(kind) => self.queue_orders(TypeId(kind as u16), true),
            HudCommand::Unproduce(kind) => self.queue_orders(TypeId(kind as u16), false),
            HudCommand::Place(kind) => {
                // `CanAfford` (0x514c65) before entering the build mode.
                let kind = TypeId(kind as u16);
                let cost = acr_sim::economy::unit_cost(world, player, kind, None);
                let stock = &world.players[usize::from(player)].resources;
                if stock.iter().zip(&cost).all(|(s, c)| s >= c) {
                    self.interface.placing = Some(kind);
                    self.interface.rally_mode = false;
                }
            }
            HudCommand::Page(page) => {
                let kind = self.selected_units().next().map(|u| u.kind);
                if let Some(kind) = kind {
                    self.interface.pages.insert(kind, page);
                }
            }
            HudCommand::Leave => {
                let buildings: Vec<EntityId> =
                    self.selected_units().filter(|u| u.building && !u.inside.is_empty()).map(|u| u.id).collect();
                for building in buildings {
                    self.commands.push(Command::Leave { player, building });
                }
            }
            HudCommand::Rally => {
                self.interface.rally_mode = true;
                self.interface.placing = None;
            }
            HudCommand::Upgrade(up) => {
                let buildings: Vec<EntityId> =
                    self.selected_units().filter(|u| u.building && u.owner == player).map(|u| u.id).collect();
                let upgrade = acr_sim::tech::UpgradeId(up);
                self.commands.push(Command::Upgrade { player, buildings, upgrade });
            }
            HudCommand::CancelUpgrade(up) => {
                let buildings: Vec<EntityId> = self
                    .selected_units()
                    .filter(|u| u.owner == player && u.research.as_ref().is_some_and(|r| r.upgrade.0 == up))
                    .map(|u| u.id)
                    .collect();
                for building in buildings {
                    self.commands.push(Command::CancelUpgrade { player, building });
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
        self.dip_draw(ctx, &mut hud_draws);
        if self.interface.paused && self.mission.ended.is_none() {
            pause_text(ctx, &mut hud_draws);
        }
        let image = self.interface.hud.minimap_image(ctx, &view);
        self.interface.view = view;
        // The cursor (0x4c1f70): the arrow, the sword over an enemy with an own selection.
        let over_hud = self.cursor.is_some_and(|c| self.over_hud(ctx, c));
        let modal = self.interface.placing.is_some() || self.interface.rally_mode;
        let frame = match self.cursor.filter(|_| !over_hud && !modal).and_then(|c| self.context_order(c)) {
            Some(context) => context.cursor(),
            None => cursor::frame::ARROW,
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

    /// Whether a window point is over the interface.
    fn over_hud(&self, ctx: &Ctx, at: [f32; 2]) -> bool {
        let p = self.canvas_point(at);
        self.interface.hud.covers(ctx, &self.interface.view, p) || self.diplomacy.covers(p)
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
        for u in world.units.iter().filter(|u| u.alive && u.inside_of.is_none()) {
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
        if self.interface.hud.locks && !self.interface.hud.zoom {
            self.lock_overlay(&palette, &mut out);
        }
        for m in &self.interface.markers {
            let h = terrain.height_at(m.x, m.y);
            out.push(
                Draw::game_frame(hud::gp::CURSGO, usize::from(m.frame), m.x - 24, (m.y >> 1) - 17 - h, 0).unclipped(),
            );
        }
        // The building being placed (0x4bdd1e): pulsing dark where it fits
        // (`ShowGPDarkN`), red where it does not (`ShowGPRedN`), with
        // N = 5 + (sin(t / 100 ms) + 2)·8 (0x4bde42).
        if let Some(kind) = self.interface.placing
            && let Some(at) = self.cursor.filter(|&c| !self.over_hud(ctx, c))
        {
            let (x, y) = self.world_at(at);
            let ((sx, sy), table) = match self.placement_site(kind, x, y) {
                Some(p) => (p, PREVIEW_FREE),
                None => ((x, y), PREVIEW_BLOCKED),
            };
            let t = self.interface.started.elapsed().as_secs_f64() * 10.0;
            let row = (5.0 + (t.sin() + 2.0) * 8.0) as u8;
            // The packs come from the library: the type may not be on the map yet.
            let def = objects.definition(&world.catalog.get(kind).id);
            let look = def.and_then(|d| {
                let anim = acr_render::scene::idle_animation(d)?;
                acr_render::scene::animation_look(d, anim, 0, 0, |p| ctx.lib.sprites(p).map(|g| g.frame_count()))
            });
            if let Some(look) = look {
                let h = terrain.height_at(sx, sy);
                for ((gp, frame), (ox, oy), _, _) in look.parts {
                    out.push(
                        Draw::game_frame_remap(&gp, frame, sx + ox, (sy >> 1) + oy - h, 0, table, row).unclipped(),
                    );
                }
            }
        }
        out
    }

    /// The inaccessible areas (key Q, 0x518306): every blocked 16 × 16 cell of the land
    /// lock map in view gets its two diagonals in colour 0xfe and, towards free neighbours,
    /// its edges in colour 0xff, following the ground height at the corners. Cells outside
    /// the map count as blocked.
    fn lock_overlay(&self, palette: &acr_formats::Palette, out: &mut Vec<Draw>) {
        let (Some(world), Some(terrain)) = (&self.world, &self.terrain) else { return };
        let locks = &world.map.land;
        let rgba = |i: u8| {
            let [r, g, b] = palette.rgb(i);
            [r, g, b, 255]
        };
        let (cross, edge) = (rgba(0xfe), rgba(0xff));
        let s = self.camera.scale;
        let window = self.window_size();
        let (x0, y0) = ((self.camera.x / 16.0).floor() as i32 - 1, (self.camera.y / 8.0).floor() as i32 - 1);
        let (x1, y1) = (x0 + (window[0] / s / 16.0) as i32 + 3, y0 + (window[1] / s / 8.0) as i32 + 3);
        // Hills lift cells into the view from below.
        let y1 = y1 + 64;
        let h = |cx: i32, cy: i32| terrain.height_at(cx * 16, cy * 16);
        for cy in y0.max(-1)..=y1.min(locks.height) {
            for cx in x0.max(-1)..=x1.min(locks.width) {
                if !locks.check_pt(cx, cy) {
                    continue;
                }
                let (sx, sy) = (cx * 16, cy * 8);
                let (h00, h10, h01, h11) = (h(cx, cy), h(cx + 1, cy), h(cx, cy + 1), h(cx + 1, cy + 1));
                line(out, cross, (sx, sy - h00), (sx + 16, sy + 8 - h11));
                line(out, cross, (sx, sy + 8 - h01), (sx + 16, sy - h10));
                if !locks.check_pt(cx + 1, cy) {
                    line(out, edge, (sx + 16, sy - h10), (sx + 16, sy + 8 - h11));
                }
                if !locks.check_pt(cx - 1, cy) {
                    line(out, edge, (sx, sy - h00), (sx, sy + 8 - h01));
                }
                if !locks.check_pt(cx, cy - 1) {
                    line(out, edge, (sx, sy - h00), (sx + 16, sy - h10));
                }
                if !locks.check_pt(cx, cy + 1) {
                    line(out, edge, (sx, sy + 8 - h01), (sx + 16, sy + 8 - h11));
                }
            }
        }
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
            .filter(|u| u.alive && u.inside_of.is_none() && keep(u))
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
        if pressed && self.drag_from.is_none() && self.dip_press(ctx, p) {
            return GameMouse::None;
        }
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
                HudEvent::History => return GameMouse::History,
                HudEvent::Surrender => {
                    self.mission.surrender();
                    return GameMouse::None;
                }
                HudEvent::Zoom => {
                    self.toggle_zoom(at);
                    return GameMouse::None;
                }
                _ => return GameMouse::None,
            }
        }
        if pressed && self.drag_from.is_none() && self.map_click(at) {
            return GameMouse::None;
        }
        if pressed {
            self.dip_check_building(at);
        }
        if !pressed && self.drag_from.is_none() {
            self.dip_release();
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

    /// A left click on the map in the build mode (`Build` with the selected units; Shift
    /// keeps the mode) or while picking a rally point. True if it was used.
    fn map_click(&mut self, at: [f32; 2]) -> bool {
        let player = self.own();
        if self.interface.rally_mode {
            self.interface.rally_mode = false;
            let (x, y) = self.world_at(at);
            self.set_rally(x, y);
            return true;
        }
        let Some(kind) = self.interface.placing else { return false };
        let (x, y) = self.world_at(at);
        if let Some((x, y)) = self.placement_site(kind, x, y) {
            let units: Vec<EntityId> =
                self.selected_units().filter(|u| u.owner == player && !u.building).map(|u| u.id).collect();
            if let Some(&first) = units.first() {
                self.unit_sound(first, "ORDER_SOUND");
            }
            self.commands.push(Command::Build { player, units, kind, x, y });
            if !self.interface.mods.shift_key() {
                self.interface.placing = None;
            }
        }
        true
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
        self.diplomacy.mouse_moved(p);
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
            HudEvent::None if self.cancel_mode() => {}
            HudEvent::None => {
                if self.selected.is_empty() {
                    return;
                }
                self.order_at(at);
            }
            _ => {}
        }
    }

    /// Leaves the build mode or the rally point mode; false if neither was on.
    fn cancel_mode(&mut self) -> bool {
        let on = self.interface.placing.is_some() || self.interface.rally_mode;
        self.interface.placing = None;
        self.interface.rally_mode = false;
        on
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
            // Escape first leaves a command mode (0x4a31a3).
            Key::Named(NamedKey::Escape) if self.cancel_mode() => {}
            Key::Named(NamedKey::F12 | NamedKey::Escape) => return GameKey::Menu,
            Key::Named(NamedKey::F8) => self.dip_toggle(),
            Key::Named(NamedKey::Pause) if self.mission.ended.is_none() => {
                self.interface.paused = !self.interface.paused
            }
            Key::Named(NamedKey::Tab) => self.interface.hud.life_bars = !self.interface.hud.life_bars,
            Key::Named(NamedKey::Space) => self.centre_next(),
            Key::Character(c) => {
                let Some(ch) = c.chars().next().filter(|ch| ch.is_ascii()) else { return GameKey::None };
                let ch = ch.to_ascii_uppercase() as u8;
                if let Some(n) = (ch as char).to_digit(10) {
                    self.control_group(n as usize, mods);
                } else if ch == b'M' && mods.is_empty() {
                    self.interface.hud.toggle_minimap();
                } else if ch == b'U' && mods.is_empty() {
                    self.interface.hud.toggle_unit_panel();
                } else if ch == b'I' && mods.is_empty() {
                    self.interface.hud.toggle_game_panel();
                } else if ch == b'Q' && mods.is_empty() {
                    self.interface.hud.toggle_locks();
                } else if ch == b'O' && mods.is_empty() {
                    self.interface.hud.toggle_transparency();
                } else if ch == b'L' && mods.is_empty() {
                    if let Some(at) = self.cursor {
                        self.toggle_zoom(at);
                    }
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

    /// Key L or `4XSCALE_ON`/`OFF` (0x5fb4dd): the view at a quarter of the size, or back,
    /// centred on the point under the cursor.
    fn toggle_zoom(&mut self, at: [f32; 2]) {
        let (x, y) = self.world_at(at);
        let zoom = !self.interface.hud.zoom;
        self.interface.hud.zoom = zoom;
        self.camera.scale = if zoom { ZOOM_SCALE } else { 1.0 };
        let window = self.window_size();
        let s = self.camera.scale;
        self.camera.x = x as f32 - window[0] / s / 2.0;
        self.camera.y = (y / 2) as f32 - window[1] / s / 2.0;
        self.clamp_camera(window);
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
        // The keys of the production icons (`[FIXED_PRODUCE]`, without modifiers).
        let produce = self.interface.view.icons.iter().find(|i| flags == 0 && i.key == Some(key) && !i.disabled);
        if let Some(action) = produce.and_then(|i| i.action.clone()) {
            self.hud_command(action);
            return;
        }
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

/// A one pixel line (`DrawLine` 0x4012ff) as runs of fills, in world screen pixels.
fn line(out: &mut Vec<Draw>, rgba: [u8; 4], (x0, y0): (i32, i32), (x1, y1): (i32, i32)) {
    let (dx, dy) = (x1 - x0, y1 - y0);
    let n = dx.abs().max(dy.abs());
    // The pixel of step k, rounded to the nearest.
    let at = |k: i32| {
        if n == 0 {
            return (x0, y0);
        }
        let round = |d: i32| (2 * d * k + n * d.signum()) / (2 * n);
        (x0 + round(dx), y0 + round(dy))
    };
    let horizontal = dx.abs() >= dy.abs();
    // Consecutive pixels in the same row (column) become one fill.
    let mut start = at(0);
    let mut last = start;
    for k in 1..=n + 1 {
        let p = if k <= n { Some(at(k)) } else { None };
        let same = p.is_some_and(|p| if horizontal { p.1 == start.1 } else { p.0 == start.0 });
        if !same {
            let r = Rect {
                x0: start.0.min(last.0),
                y0: start.1.min(last.1),
                x1: start.0.max(last.0) + 1,
                y1: start.1.max(last.1) + 1,
            };
            out.push(Draw::fill(rgba, r).unclipped());
            if let Some(p) = p {
                start = p;
            }
        }
        if let Some(p) = p {
            last = p;
        }
    }
}

/// `PAUSETEXT` while the game is paused (0x4c0130): `BigWhiteFont` centred at half the
/// screen height on a paper box (0x480130) 64 pixels wider on each side.
fn pause_text(ctx: &Ctx, out: &mut Vec<Draw>) {
    let text = ctx.texts.get("PAUSETEXT");
    let w = ctx.fonts.width(ctx.lib, BIG_WHITE, text);
    let (x, y) = ((hud::SCREEN[0] - w) / 2, hud::SCREEN[1] / 2);
    let rect = Rect::inclusive(x - 64, y - 8, x + w + 64, y + 27);
    crate::ui::screens::mission::paper_box(ctx, out, rect, PAPER, hud::BOX_CORNERS, 0);
    out.push(Draw::game_text(BIG_WHITE, text, x - 2, y - 8));
}

/// `BigWhiteFont` and the paper of 0x480130 (`Border` 28..30).
const BIG_WHITE: crate::ui::fonts::FontId = crate::ui::fonts::FontId("BigWhiteFont");
const PAPER: usize = 28;

/// The name of a unit type: the text `NAME.MD` (`GeneralObject::Message`), else its name.
fn unit_name(ctx: &Ctx, md_name: &str) -> Vec<u8> {
    let key = format!("{}.MD", md_name.to_ascii_uppercase());
    let text = ctx.texts.get(&key);
    if text == key.as_bytes() { md_name.as_bytes().to_vec() } else { text.to_vec() }
}

/// A production hint (0x606180, 0x6062a0 for upgrades): the name, " -", then
/// " Resource: price" for every resource of the base price, with the current price.
fn cost_hint(name: Vec<u8>, base: &[i32; 8], cost: &[i32; 8], ctx: &Ctx) -> Vec<u8> {
    let mut out = name;
    out.extend_from_slice(b" -");
    for (r, name) in RESOURCE_NAMES.iter().enumerate() {
        if base[r] != 0 {
            out.push(b' ');
            out.extend_from_slice(ctx.texts.get(name));
            out.extend_from_slice(format!(": {}", cost[r]).as_bytes());
        }
    }
    out
}

/// An upgrade's hint: its message, and on a second line (0x49483d) the price as
/// `Resource:amount` joined by spaces (0x6062a0).
fn upgrade_hint(ctx: &Ctx, message: &str, cost: &[i32; 8]) -> Vec<u8> {
    let mut out = ctx.texts.get(message).to_vec();
    let mut first = true;
    for (r, name) in RESOURCE_NAMES.iter().enumerate().filter(|(r, _)| cost[*r] != 0) {
        out.push(if first { b'/' } else { b' ' });
        first = false;
        out.extend_from_slice(ctx.texts.get(name));
        out.extend_from_slice(format!(":{}", cost[r]).as_bytes());
    }
    out
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The pixels a line covers.
    fn pixels(from: (i32, i32), to: (i32, i32)) -> Vec<(i32, i32)> {
        let mut out = Vec::new();
        line(&mut out, [0; 4], from, to);
        let mut px: Vec<(i32, i32)> = out
            .iter()
            .flat_map(|d| {
                let r = Rect::new(d.x, d.y, d.size[0], d.size[1]);
                (r.y0..r.y1).flat_map(move |y| (r.x0..r.x1).map(move |x| (x, y)))
            })
            .collect();
        px.sort_unstable();
        px
    }

    #[test]
    fn lines_cover_every_step_once() {
        // A lock cell's diagonal: 17 pixels from (0, 0) to (16, 8), two per row.
        let px = pixels((0, 0), (16, 8));
        assert_eq!(px.len(), 17);
        assert_eq!((px[0], px[16]), ((0, 0), (16, 8)));
        assert!(px.windows(2).all(|w| w[0] != w[1]));
        // Rising and vertical lines.
        assert_eq!(pixels((0, 8), (16, 0)).len(), 17);
        assert_eq!(pixels((3, 0), (3, 5)), (0..=5).map(|y| (3, y)).collect::<Vec<_>>());
    }
}
