//! The mortars' controls (#301, not in the original game; `docs/ui.md` "Mortar buttons"):
//! with the mortars rule on and own mortars (`ArtMrt`) selected, the command panel shows four
//! icons in the style of the original's command icons, each issuing the simulation's
//! commands, so replays and savegames keep them:
//!
//! - **Zielzone setzen** (slot 15, the `Rally_Point` icon): the next click on the map adds a
//!   zone of [`DEFAULT_RADIUS`] there, a drag one as wide as the drag
//!   ([`acr_sim::Command::MortarZones`] per mortar with its zones and the new one; Shift
//!   keeps the mode, Escape or a right click leaves it);
//! - **Zonen löschen** (slot 16, `Cancel`): `MortarZones` without zones;
//! - **Feuer frei** (slot 27, `FireKartech_Cannon`) and **Feuer einstellen** (slot 28,
//!   `Attack_OFF`): [`acr_sim::Command::MortarFire`].
//!
//! The modern controls add Alt + Z, Alt + X and Alt + F (fire on or off). The selected
//! mortars' zones and minimum range are drawn on the ground like the gun marks
//! (`gun_marks.rs`).

use acr_sim::mortar::{self, Zone};
use acr_sim::world::EntityId;
use acr_sim::{Command, Unit, World};

use super::InGame;
use super::gun_marks::{self, Colours};
use crate::hud::data::HudData;
use crate::hud::{HudCommand, Icon};
use crate::ui::draw::Draw;

/// The radius (world px) of a zone set by a click (a drag shorter than the smallest zone).
pub(super) const DEFAULT_RADIUS: i32 = 150;

/// The icons: slot, `ICONLIST.TXT` name, hint, modern key.
const ZONE: (usize, &str, &[u8], &[u8]) = (15, "Rally_Point", b"Zielzone setzen", b" (Alt+Z)");
const CLEAR: (usize, &str, &[u8], &[u8]) = (16, "Cancel", b"Zonen l\xf6schen", b" (Alt+X)");
const FIRE: (usize, &str, &[u8], &[u8]) = (27, "FireKartech_Cannon", b"Feuer frei", b" (Alt+F)");
const CEASE: (usize, &str, &[u8], &[u8]) = (28, "Attack_OFF", b"Feuer einstellen", b" (Alt+F)");

/// The zone mode: on, and the press point (world px) while the button is held.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ZoneMode {
    pub(super) on: bool,
    pub(super) press: Option<(i32, i32)>,
}

/// Whether `u` is a mortar the rule lets take zones.
fn is_mortar(world: &World, u: &Unit) -> bool {
    world.rules.mortars && world.catalog.get(u.kind).mortar && u.death == 0
}

/// The radius of a zone pressed at `a` and released at `b`.
fn radius(a: (i32, i32), b: (i32, i32)) -> i32 {
    let r = acr_sim::trig::norma(b.0 - a.0, b.1 - a.1);
    if r < mortar::ZONE_RADIUS.0 { DEFAULT_RADIUS } else { r.min(mortar::ZONE_RADIUS.1) }
}

impl InGame {
    /// The selected own mortars.
    fn own_mortars(&self) -> Vec<EntityId> {
        let Some(world) = &self.world else { return Vec::new() };
        let own = self.own();
        self.selected_units().filter(|u| u.owner == own && is_mortar(world, u)).map(|u| u.id).collect()
    }

    /// The mortar icons for the selection (see the module doc).
    pub(super) fn mortar_icons(&self, data: &HudData, outline: u8, icons: &mut Vec<Icon>) {
        let Some(world) = &self.world else { return };
        let mortars = self.own_mortars();
        if mortars.is_empty() {
            return;
        }
        let state = |id: &EntityId| world.mortar_zones.get(id);
        let firing = mortars.iter().any(|id| state(id).is_some_and(|m| m.firing));
        let holding = mortars.iter().any(|id| state(id).is_some_and(|m| !m.firing));
        let modern = self.modern();
        let mut push = |(slot, name, hint, key): (usize, &str, &[u8], &[u8]), selected: bool, action: HudCommand| {
            let Some(frame) = data.icon(name) else { return };
            let mut hint = hint.to_vec();
            if modern {
                hint.extend_from_slice(key);
            }
            icons.retain(|i| i.slot != slot);
            icons.push(Icon {
                frame,
                slot,
                outline: Some(outline),
                selected,
                hint,
                action: Some(action),
                ..Icon::default()
            });
        };
        push(ZONE, self.interface.mortar.on, HudCommand::MortarZoneMode);
        push(CLEAR, false, HudCommand::MortarClearZones);
        push(FIRE, firing, HudCommand::MortarFire(true));
        push(CEASE, holding, HudCommand::MortarFire(false));
    }

    /// A mortar icon pressed (or its modern key).
    pub(super) fn mortar_command(&mut self, c: &HudCommand) {
        let units = self.own_mortars();
        let Some(&first) = units.first() else { return };
        let player = self.own();
        match *c {
            HudCommand::MortarZoneMode => {
                self.interface.mortar = ZoneMode { on: true, press: None };
                return;
            }
            HudCommand::MortarClearZones => {
                self.commands.push(Command::MortarZones { player, units, zones: Vec::new() })
            }
            HudCommand::MortarFire(on) => self.commands.push(Command::MortarFire { player, units, on }),
            _ => return,
        }
        self.unit_sound(first, "ORDER_SOUND");
    }

    /// The modern keys: Alt + Z sets a zone, Alt + X clears them, Alt + F starts or stops the
    /// fire. True if used.
    pub(super) fn mortar_key(&mut self, ch: u8) -> bool {
        let cmd = match ch {
            b'Z' => HudCommand::MortarZoneMode,
            b'X' => HudCommand::MortarClearZones,
            b'F' => {
                let firing = self.world.as_ref().is_some_and(|w| {
                    self.own_mortars().iter().any(|id| w.mortar_zones.get(id).is_some_and(|m| m.firing))
                });
                HudCommand::MortarFire(!firing)
            }
            _ => return false,
        };
        if self.own_mortars().is_empty() {
            return false;
        }
        self.mortar_command(&cmd);
        true
    }

    /// A left press on the map in the zone mode: the zone's centre. True if used.
    pub(super) fn mortar_press(&mut self, at: [f32; 2]) -> bool {
        if !self.interface.mortar.on {
            return false;
        }
        self.interface.mortar.press = Some(self.world_at(at));
        true
    }

    /// The left button released after [`Self::mortar_press`]: every selected mortar gets its
    /// zones plus the new one (the oldest goes beyond [`mortar::MAX_ZONES`]).
    pub(super) fn mortar_release(&mut self, at: [f32; 2]) -> bool {
        let Some(press) = self.interface.mortar.press.take() else { return false };
        if !self.interface.mods.shift_key() {
            self.interface.mortar.on = false;
        }
        let zone = Zone { x: press.0, y: press.1, r: radius(press, self.world_at(at)) };
        let player = self.own();
        let units = self.own_mortars();
        let Some(world) = &self.world else { return true };
        for &id in &units {
            let mut zones = world.mortar_zones.get(&id).map(|m| m.zones.clone()).unwrap_or_default();
            zones.push(zone);
            let extra = zones.len().saturating_sub(mortar::MAX_ZONES);
            zones.drain(..extra);
            self.commands.push(Command::MortarZones { player, units: vec![id], zones });
        }
        if let Some(&first) = units.first() {
            self.unit_sound(first, "ORDER_SOUND");
        }
        true
    }

    /// The selected own mortars' marks: a hatched grey ring of the minimum range, a dashed
    /// circle per zone and, in the zone mode, the zone being set under the cursor.
    pub(super) fn mortar_marks(
        &self,
        world: &World,
        cs: Colours,
        at: &dyn Fn(f64, f64) -> (i32, i32),
        cursor: Option<[f32; 2]>,
        out: &mut Vec<Draw>,
    ) {
        let own = self.own();
        let mut any = false;
        for u in self.selected_units().filter(|u| u.owner == own && u.inside_of.is_none() && is_mortar(world, u)) {
            any = true;
            let t = world.catalog.get(u.kind);
            let adv = world.adv(u.owner, u.kind);
            let min = mortar::shell_index(t).and_then(|k| adv.attacks.get(k)).map_or(0, |a| a.radius.0);
            let zones = world.mortar_zones.get(&u.id).map_or(&[][..], |m| &m.zones[..]);
            gun_marks::mortar(u.pos.pixels(), min.max(mortar::MIN_RANGE), zones, cs, at, out);
        }
        let m = self.interface.mortar;
        if any && m.on {
            let Some(c) = cursor else { return };
            let c = self.world_at(c);
            let zone = match m.press {
                Some(p) => Zone { x: p.0, y: p.1, r: radius(p, c) },
                None => Zone { x: c.0, y: c.1, r: DEFAULT_RADIUS },
            };
            gun_marks::zone(&zone, cs, at, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_click_sets_the_default_radius_a_drag_its_own() {
        assert_eq!(radius((100, 100), (110, 100)), DEFAULT_RADIUS);
        assert_eq!(radius((100, 100), (400, 100)), 300);
        assert_eq!(radius((0, 0), (5000, 0)), mortar::ZONE_RADIUS.1);
    }
}
