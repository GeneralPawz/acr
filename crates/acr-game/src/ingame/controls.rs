//! ACR's optional modern controls (#129, see docs/controls.md): the mouse wheel zooms about
//! the point under the cursor, Ctrl + left drag and middle drag pan the map, W/A/S/D pan
//! like the arrow keys, Home centres the selection and a control group digit pressed twice
//! centres the group, Ctrl + click takes units out of the selection. The original controls stay the default and are not changed by any of
//! this: every entry point here returns at once unless the modern scheme is chosen.

use acr_render::terrain::Camera;
use acr_sim::world::EntityId;
use winit::keyboard::{Key, ModifiersState, NamedKey};

use super::interface::{GameKey, corrected_selection};
use super::{CLICK_SLOP, InGame};
use crate::hud::HudCommand;
use crate::ui::Ctx;
use crate::ui::settings::{Controls, Settings};

/// The zoom range of the wheel (output pixels per original pixel).
pub(super) const MIN_ZOOM: f32 = 0.5;
pub(super) const MAX_ZOOM: f32 = 2.0;
/// One wheel notch: four notches double or halve the scale.
const ZOOM_STEP: f32 = 1.189_207_1;
/// The scale glides to the wheel's level at this rate (1/s).
const ZOOM_RATE: f32 = 16.0;
/// A second press of the same group digit within this time centres the camera on the group.
const DOUBLE_TAP: f64 = 0.5;
/// W, A, S, D in the order of [`Modern::keys`]: left, right, up, down.
const PAN_KEYS: [u8; 4] = *b"ADWS";
/// After Ctrl + Space, a letter within this time stands for one of the original's special
/// keys.
const LEADER_TIME: f64 = 2.0;
/// The leader chords: letter, the key it stands for, its name in the hint.
pub(super) const LEADER: [(u8, NamedKey, &str); 7] = [
    (b'P', NamedKey::Pause, "Pause"),
    (b'M', NamedKey::F12, "Menü"),
    (b'O', NamedKey::F1, "Ziele"),
    (b'D', NamedKey::F8, "Diplomatie"),
    (b'L', NamedKey::Tab, "Lebensbalken"),
    (b'F', NamedKey::F11, "Vollbild"),
    (b'N', NamedKey::Space, "nächste Einheit"),
];
/// How long the hint of the leader chords stays (frames of the message list).
const LEADER_HINT: u32 = 100;

/// The key a leader chord's letter stands for.
pub(super) fn leader_key(ch: u8) -> Option<NamedKey> {
    LEADER.iter().find(|&&(c, ..)| c == ch.to_ascii_uppercase()).map(|&(_, k, _)| k)
}

/// The camera zoomed to `scale` about the window point `at`: the map point under `at` stays
/// under it.
pub(super) fn zoom_about(camera: Camera, at: [f32; 2], scale: f32) -> Camera {
    let (x, y) = (camera.x + at[0] / camera.scale, camera.y + at[1] / camera.scale);
    Camera { x: x - at[0] / scale, y: y - at[1] / scale, scale }
}

/// The camera of a grab-and-drag pan that started at the window point `from` with `start`:
/// the map point grabbed stays under the cursor at `at`.
pub(super) fn dragged(start: Camera, from: [f32; 2], at: [f32; 2]) -> Camera {
    Camera { x: start.x - (at[0] - from[0]) / start.scale, y: start.y - (at[1] - from[1]) / start.scale, ..start }
}

/// The next zoom level after `lines` wheel notches (positive: in), clamped, and exactly 1
/// when it comes close so the sprites are drawn pixel for pixel.
pub(super) fn wheel_zoom(level: f32, lines: f32) -> f32 {
    let next = (level * ZOOM_STEP.powf(lines)).clamp(MIN_ZOOM, MAX_ZOOM);
    if next.log2().abs() < 0.02 { 1.0 } else { next }
}

/// `selected` without the units a Ctrl + click on the map hit (`hit`, the units a plain click
/// there would select): those, the formations grouped with theirs and every member of
/// their formations (also the officer and drummer a plain click leaves out).
pub(super) fn deselected(world: &acr_sim::World, own: u8, selected: &[EntityId], hit: Vec<EntityId>) -> Vec<EntityId> {
    let hit = corrected_selection(world, own, hit);
    let brigades: Vec<u16> = hit.iter().filter_map(|&id| world.unit(id)?.brigade).collect();
    let in_hit_brigade =
        |id: EntityId| world.unit(id).is_some_and(|u| u.brigade.is_some_and(|b| brigades.contains(&b)));
    selected.iter().copied().filter(|&id| !hit.contains(&id) && !in_hit_brigade(id)).collect()
}

/// The selected units an icon of the selection panel or the army list stands for: a type
/// icon its free units of that type, a formation icon its members. `None` for other icons.
pub(super) fn icon_units(world: &acr_sim::World, selected: &[EntityId], command: &HudCommand) -> Option<Vec<EntityId>> {
    let of = |f: &dyn Fn(&acr_sim::Unit) -> bool| {
        selected.iter().copied().filter(|&id| world.unit(id).is_some_and(f)).collect::<Vec<_>>()
    };
    match command {
        HudCommand::SelectType(kind) => Some(of(&|u| u.brigade.is_none() && u.kind.0 as usize == *kind)),
        HudCommand::SelectBrigade(b) | HudCommand::ArmySelect { brigade: b, .. } => {
            Some(of(&|u| u.brigade == Some(*b)))
        }
        _ => None,
    }
}

/// What starts a pan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PanButton {
    /// Ctrl + left button on the map; a release without moving is a plain click.
    CtrlLeft,
    Middle,
}

/// A grab-and-drag pan in progress.
#[derive(Debug, Clone, Copy)]
pub(super) struct Pan {
    from: [f32; 2],
    start: Camera,
    pub button: PanButton,
    /// The cursor has left the click slop: the drag pans.
    moved: bool,
}

/// State of the modern controls.
#[derive(Debug, Clone)]
pub(super) struct Modern {
    pub scheme: Controls,
    pub edge_scroll: bool,
    /// The wheel's zoom level; the camera scale glides to it.
    pub zoom: f32,
    /// The window point the zoom keeps fixed.
    anchor: [f32; 2],
    pub pan: Option<Pan>,
    /// W/A/S/D held: left, right, up, down.
    pub keys: [bool; 4],
    /// The last group recalled and when.
    last_group: Option<(usize, web_time::Instant)>,
    /// When Ctrl + Space was pressed: the next letter is a leader chord.
    leader: Option<web_time::Instant>,
}

impl Default for Modern {
    fn default() -> Self {
        Self {
            scheme: Controls::Original,
            edge_scroll: true,
            zoom: 1.0,
            anchor: [0.0; 2],
            pan: None,
            keys: [false; 4],
            last_group: None,
            leader: None,
        }
    }
}

impl InGame {
    /// The modern controls are chosen.
    pub(super) fn modern(&self) -> bool {
        self.interface.modern.scheme == Controls::Modern
    }

    /// A click now takes units out of the selection: modern controls, Ctrl held (without
    /// Shift or Alt).
    pub(super) fn ctrl_deselects(&self) -> bool {
        let mods = self.interface.mods;
        self.modern() && mods.control_key() && !mods.shift_key() && !mods.alt_key()
    }

    /// A Ctrl + click on the map (modern controls) on a selected unit: the unit leaves the
    /// selection with its whole formation. True if it did; otherwise (nothing selected hit,
    /// or a building, rally, patrol or attack-ground click pending) it is a plain click.
    pub(super) fn ctrl_click_deselect(&mut self) -> bool {
        let i = &self.interface;
        if !self.ctrl_deselects() || i.placing.is_some() || i.rally_mode || i.patrol_mode || i.attack_ground_mode {
            return false;
        }
        let (Some(at), Some(world)) = (self.cursor, &self.world) else { return false };
        let hit = self.pick(at, at);
        if !hit.iter().any(|id| self.selected.contains(id)) {
            return false;
        }
        self.selected = deselected(world, self.own(), &self.selected, hit);
        true
    }

    /// Takes the control scheme of the options. Back to the original controls, the view
    /// returns to the original scale (unless the zoomed-out view of key L is on).
    pub(super) fn apply_controls(&mut self, settings: &Settings) {
        let m = &mut self.interface.modern;
        m.scheme = settings.controls;
        m.edge_scroll = settings.edge_scrolls();
        if m.scheme == Controls::Original {
            m.zoom = 1.0;
            m.pan = None;
            m.keys = [false; 4];
            if !self.interface.hud.zoom && self.camera.scale != 1.0 {
                let window = self.window_size();
                self.camera = zoom_about(self.camera, [window[0] / 2.0, window[1] / 2.0], 1.0);
                self.clamp_camera(window);
            }
        }
    }

    /// The mouse wheel over the map: zoom in (positive) or out about the cursor. Not in the
    /// zoomed-out view of key L, which has its own scale.
    pub fn wheel(&mut self, lines: f32) {
        if !self.modern() || self.interface.hud.zoom || lines == 0.0 {
            return;
        }
        let m = &mut self.interface.modern;
        m.zoom = wheel_zoom(m.zoom, lines);
        if let Some(at) = self.cursor {
            self.interface.modern.anchor = at;
        }
    }

    /// Moves the camera scale towards the wheel's level, once per frame.
    pub(super) fn glide_zoom(&mut self, seconds: f32, window: [f32; 2]) {
        if self.interface.hud.zoom {
            return;
        }
        let (target, anchor) = (self.interface.modern.zoom, self.interface.modern.anchor);
        let scale = self.camera.scale;
        if scale == target {
            return;
        }
        let next = target + (scale - target) * (-ZOOM_RATE * seconds.min(0.25)).exp();
        let next = if (next / target - 1.0).abs() < 0.002 { target } else { next };
        self.camera = zoom_about(self.camera, anchor, next);
        self.clamp_camera(window);
        // A pan in progress goes on from the zoomed view.
        if let (Some(pan), Some(at)) = (&mut self.interface.modern.pan, self.cursor) {
            pan.start = self.camera;
            pan.from = at;
        }
    }

    /// Starts a pan at the cursor.
    pub(super) fn start_pan(&mut self, button: PanButton) {
        let Some(at) = self.cursor else { return };
        self.interface.modern.pan = Some(Pan { from: at, start: self.camera, button, moved: false });
    }

    /// The middle button pans the map while held (modern controls).
    pub fn middle_button(&mut self, pressed: bool) {
        if !self.modern() {
            return;
        }
        if pressed {
            if self.drag_from.is_none() && self.interface.modern.pan.is_none() {
                self.start_pan(PanButton::Middle);
            }
        } else if self.interface.modern.pan.is_some_and(|p| p.button == PanButton::Middle) {
            self.interface.modern.pan = None;
        }
    }

    /// Follows the cursor while panning; true if a pan is in progress (the move does
    /// nothing else then).
    pub(super) fn pan_moved(&mut self, at: [f32; 2]) -> bool {
        let Some(pan) = &mut self.interface.modern.pan else { return false };
        if !pan.moved && (pan.from[0] - at[0]).abs() < CLICK_SLOP && (pan.from[1] - at[1]).abs() < CLICK_SLOP {
            return true;
        }
        pan.moved = true;
        self.camera = dragged(pan.start, pan.from, at);
        let window = self.window_size();
        self.clamp_camera(window);
        true
    }

    /// Ends a Ctrl + left pan; true if it never moved, so the release is a plain click.
    pub(super) fn end_ctrl_pan(&mut self) -> Option<bool> {
        let pan = self.interface.modern.pan.filter(|p| p.button == PanButton::CtrlLeft)?;
        self.interface.modern.pan = None;
        Some(!pan.moved)
    }

    /// Keys of the modern controls; `Some` if the key was used.
    pub(super) fn modern_key(&mut self, ctx: &Ctx, key: &Key, pressed: bool) -> Option<GameKey> {
        if !self.modern() {
            return None;
        }
        let mods = self.interface.mods;
        if pressed && let Some(used) = self.leader_chord(key, mods) {
            return Some(used);
        }
        match key {
            Key::Character(c) => {
                let ch = c.chars().next().filter(char::is_ascii_alphabetic)?.to_ascii_uppercase() as u8;
                let i = PAN_KEYS.iter().position(|&k| k == ch)?;
                if !pressed {
                    // Released: always let go, whatever the modifiers are now.
                    let was = self.interface.modern.keys[i];
                    self.interface.modern.keys[i] = false;
                    return was.then_some(GameKey::None);
                }
                if mods.alt_key() && !mods.control_key() {
                    // Alt + W/A/S/D: the command icons W/A/S/D press in the original.
                    self.icon_hotkey(ctx, ch, mods - ModifiersState::ALT);
                    return Some(GameKey::None);
                }
                if mods.control_key() || mods.alt_key() {
                    return None;
                }
                self.interface.modern.keys[i] = true;
                Some(GameKey::None)
            }
            Key::Named(NamedKey::Home) if pressed => {
                self.centre_selection();
                Some(GameKey::None)
            }
            Key::Named(NamedKey::Escape) if pressed => {
                // Escape cancels what is going on before it opens the game menu.
                if self.cancel_mode() {
                    return Some(GameKey::None);
                }
                let busy = self.interface.modern.pan.is_some()
                    || self.drag_from.is_some()
                    || self.interface.right_drag.is_some();
                if busy {
                    self.interface.modern.pan = None;
                    self.interface.right_drag = None;
                    if self.drag_from.take().is_some() {
                        self.selected = std::mem::take(&mut self.interface.select_before);
                    }
                    return Some(GameKey::None);
                }
                None
            }
            _ => None,
        }
    }

    /// Ctrl + Space starts a leader chord and shows the letters; the next letter (within
    /// [`LEADER_TIME`]) stands for one of the original's special keys. Modifier keys pressed
    /// on the way keep the chord, any other key ends it unused.
    fn leader_chord(&mut self, key: &Key, mods: ModifiersState) -> Option<GameKey> {
        if *key == Key::Named(NamedKey::Space) && mods.control_key() && !mods.alt_key() {
            self.interface.modern.leader = Some(web_time::Instant::now());
            let list: Vec<String> = LEADER.iter().map(|&(c, _, name)| format!("{} {name}", c as char)).collect();
            let text = format!("Strg+Leertaste: {}", list.join(", "));
            let bytes = acr_formats::CodePage::Cp1252.encode(&text).unwrap_or_else(|| text.into_bytes());
            self.mission.hint(bytes, LEADER_HINT);
            return Some(GameKey::None);
        }
        let since = self.interface.modern.leader?;
        if matches!(key, Key::Named(NamedKey::Control | NamedKey::Shift | NamedKey::Alt)) {
            return None;
        }
        self.interface.modern.leader = None;
        if since.elapsed().as_secs_f64() > LEADER_TIME {
            return None;
        }
        let ch = match key {
            Key::Character(c) => c.chars().next().filter(char::is_ascii_alphabetic).map(|c| c as u8),
            _ => None,
        };
        match ch.and_then(leader_key) {
            // Space here would start the chord again while Ctrl is still held.
            Some(NamedKey::Space) => {
                self.centre_next();
                Some(GameKey::None)
            }
            Some(named) => Some(GameKey::Leader(named)),
            None => Some(GameKey::None),
        }
    }

    /// A group recalled with its digit: pressed twice in a row, the camera centres on it.
    pub(super) fn group_recalled(&mut self, n: usize) {
        if !self.modern() {
            return;
        }
        let now = web_time::Instant::now();
        let twice =
            self.interface.modern.last_group.is_some_and(|(g, t)| g == n && (now - t).as_secs_f64() < DOUBLE_TAP);
        self.interface.modern.last_group = if twice { None } else { Some((n, now)) };
        if twice {
            self.centre_selection();
        }
    }

    /// Centres the camera on the middle of the selection.
    fn centre_selection(&mut self) {
        let units: Vec<(i32, i32)> = self.selected_units().map(|u| (u.pos.x >> 4, u.pos.y >> 4)).collect();
        if units.is_empty() {
            return;
        }
        let n = units.len() as i64;
        let x = units.iter().map(|u| i64::from(u.0)).sum::<i64>() / n;
        let y = units.iter().map(|u| i64::from(u.1)).sum::<i64>() / n;
        self.centre_camera(x as i32, y as i32);
    }

    /// Centres the camera on a world pixel.
    pub(super) fn centre_camera(&mut self, x: i32, y: i32) {
        let window = self.window_size();
        let s = self.camera.scale;
        self.camera.x = x as f32 - window[0] / s / 2.0;
        self.camera.y = (y / 2) as f32 - window[1] / s / 2.0;
        self.clamp_camera(window);
    }

    /// Lets go of held keys and drags, e.g. when the window loses the focus.
    pub fn release_all(&mut self) {
        self.held.clear();
        self.interface.modern.keys = [false; 4];
        self.interface.modern.pan = None;
        self.three_d.release_keys();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    /// The map point under a window point (original screen pixels).
    fn under(c: Camera, at: [f32; 2]) -> [f32; 2] {
        [c.x + at[0] / c.scale, c.y + at[1] / c.scale]
    }

    #[test]
    fn zoom_keeps_the_point_under_the_cursor() {
        let camera = Camera { x: 1234.0, y: 567.0, scale: 1.0 };
        for at in [[0.0, 0.0], [512.0, 384.0], [1000.0, 20.0], [37.5, 700.25]] {
            for scale in [MIN_ZOOM, 0.8, 1.0, 1.5, MAX_ZOOM] {
                let z = zoom_about(camera, at, scale);
                assert_eq!(z.scale, scale);
                let (p, q) = (under(camera, at), under(z, at));
                assert!(near(p[0], q[0]) && near(p[1], q[1]), "{at:?} at {scale}: {p:?} vs {q:?}");
                // And the point projects back to the cursor.
                let back = z.project(p[0] as i32, (p[1] * 2.0) as i32, 0);
                assert!((back[0] - at[0]).abs() <= scale && (back[1] - at[1]).abs() <= scale * 2.0);
            }
        }
        // Zooming in and out again returns to the same camera.
        let there = zoom_about(zoom_about(camera, [300.0, 200.0], 2.0), [300.0, 200.0], 1.0);
        assert!(near(there.x, camera.x) && near(there.y, camera.y));
    }

    #[test]
    fn ctrl_click_takes_formations_and_types_out() {
        use acr_formats::defs::{OrderGroups, OrderList, UnitDef};
        use acr_sim::catalog::{Catalog, ObjectType};
        use acr_sim::formation::Formations;
        use acr_sim::map::MapGrid;
        use acr_sim::{Command, Pos};
        use std::sync::Arc;
        let mut c = Catalog::new();
        let def = |extra: &str| {
            format!(
                "NAME X\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n@MOTION_R 9 0 10 19\r\n\
                 LIFE 100\r\nGEOMETRY 1 10 40\r\n{extra}[END]\r\n"
            )
        };
        let mus = c.add(ObjectType::from_def("Mus(EN)", &UnitDef::parse(def("").as_bytes())));
        let off = c.add(ObjectType::from_def("Off(EN)", &UnitDef::parse(def("OFFICER\r\n").as_bytes())));
        let horse = c.add(ObjectType::from_def("Dra(EN)", &UnitDef::parse(def("").as_bytes())));
        let f = Formations::new(
            &OrderList::parse(
                b"#LINE15 SYM2 19 2 9 3 1 2\r\n   C   C   C\r\n      \r\n***************\r\n#END\r\n#EXIT\r\n",
            ),
            &OrderGroups::parse(b"1\r\nLINE 0 1 #LINE15\r\n"),
        );
        let mut w = acr_sim::World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(f));
        let officer = w.spawn(off, 0, Pos::from_pixels(1500, 2000), 0);
        let soldiers: Vec<EntityId> =
            (0..15).map(|i| w.spawn(mus, 0, Pos::from_pixels(1400 + 20 * (i % 5), 2100 + 20 * (i / 5)), 0)).collect();
        w.step(&[Command::Form { player: 0, officer, units: soldiers.clone(), kind: "LINE".into() }]);
        let b = w.unit(soldiers[0]).unwrap().brigade.expect("formed");
        let horses: Vec<EntityId> = (0..3).map(|i| w.spawn(horse, 0, Pos::from_pixels(900 + 40 * i, 900), 0)).collect();
        let free_mus = w.spawn(mus, 0, Pos::from_pixels(700, 700), 0);
        let mut selected = vec![officer];
        selected.extend(&soldiers);
        selected.extend(&horses);
        selected.push(free_mus);

        // A click on one horse takes that horse out; on a soldier, the whole formation with
        // its officer.
        let kept = deselected(&w, 0, &selected, vec![horses[1]]);
        assert_eq!(kept.len(), selected.len() - 1);
        assert!(!kept.contains(&horses[1]));
        let kept = deselected(&w, 0, &selected, vec![soldiers[4]]);
        assert_eq!(kept, [horses[0], horses[1], horses[2], free_mus]);

        // The horses' type icon: the free horses; the formation's icon: all its members. The
        // free musketeer of the formation's type stays out of the formation's icon.
        let units = icon_units(&w, &selected, &HudCommand::SelectType(horse.0 as usize)).unwrap();
        assert_eq!(units, horses);
        let units = icon_units(&w, &selected, &HudCommand::SelectType(mus.0 as usize)).unwrap();
        assert_eq!(units, [free_mus]);
        let units = icon_units(&w, &selected, &HudCommand::SelectBrigade(b)).unwrap();
        assert_eq!(units.len(), 16);
        assert!(!units.contains(&free_mus));
        let army = icon_units(&w, &selected, &HudCommand::ArmySelect { brigade: b, add: false }).unwrap();
        assert_eq!(army, units);
        assert_eq!(icon_units(&w, &selected, &HudCommand::Fill), None);
    }

    #[test]
    fn leader_letters_stand_for_the_special_keys() {
        assert_eq!(leader_key(b'p'), Some(NamedKey::Pause));
        assert_eq!(leader_key(b'P'), Some(NamedKey::Pause));
        assert_eq!(leader_key(b'M'), Some(NamedKey::F12));
        assert_eq!(leader_key(b'O'), Some(NamedKey::F1));
        assert_eq!(leader_key(b'X'), None);
        // Every letter once.
        let mut letters: Vec<u8> = LEADER.iter().map(|&(c, ..)| c).collect();
        letters.sort_unstable();
        letters.dedup();
        assert_eq!(letters.len(), LEADER.len());
    }

    #[test]
    fn wheel_steps_and_clamps() {
        let mut z = 1.0;
        for _ in 0..4 {
            z = wheel_zoom(z, 1.0);
        }
        assert!(near(z, 2.0));
        assert_eq!(wheel_zoom(z, 1.0), MAX_ZOOM);
        for _ in 0..4 {
            z = wheel_zoom(z, -1.0);
        }
        assert_eq!(z, 1.0, "back to exactly 1");
        assert_eq!(wheel_zoom(1.0, -20.0), MIN_ZOOM);
        // A trackpad's fractions of a notch.
        assert!(wheel_zoom(1.0, 0.25) > 1.0 && wheel_zoom(1.0, 0.25) < ZOOM_STEP);
    }

    #[test]
    fn drag_keeps_the_grabbed_point_under_the_cursor() {
        for scale in [0.5, 1.0, 2.0] {
            let start = Camera { x: 800.0, y: 400.0, scale };
            let (from, at) = ([100.0, 100.0], [160.0, 40.0]);
            let c = dragged(start, from, at);
            let (p, q) = (under(start, from), under(c, at));
            assert!(near(p[0], q[0]) && near(p[1], q[1]));
            assert_eq!(c.scale, scale);
            // Dragging right moves the view left.
            assert!(c.x < start.x && c.y > start.y);
        }
    }
}
