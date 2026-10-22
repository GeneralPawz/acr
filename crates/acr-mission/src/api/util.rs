//! Helpers shared by the API functions: handle validation, zones, group members, units.

use acr_script::runtime::Result;
use acr_sim::Unit;
use acr_sim::world::EntityId;

use super::call::magic;
use super::{Api, Call, Handle};
use crate::state::{Member, NO_UNIT};

/// `Norma(dx, dy)` (0x427210): the engine's cheap distance, `(max + |dx| + |dy|) / 2`.
pub fn norma(dx: i32, dy: i32) -> i32 {
    let (ax, ay) = (dx.abs(), dy.abs());
    (ax.max(ay) + ax + ay) >> 1
}

/// A circle in world pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Circle {
    pub x: i32,
    pub y: i32,
    pub r: i32,
}

impl Circle {
    /// The zone test of every counting function: `Norma(RealX − x·16, RealY − y·16) < R·16`.
    pub fn contains(&self, u: &Unit) -> bool {
        norma(u.pos.x - self.x * 16, u.pos.y - self.y * 16) < self.r * 16
    }
}

/// Whether a handle is an ad-hoc point zone (`'@'` in the top byte of the type).
pub fn is_point(h: &Handle) -> bool {
    h.magic & 0xFF00_0000 == 0x4000_0000
}

impl Api<'_> {
    /// `OneObject::AttackObj(target, prio, ordType)` (0x51a930) at once, as the scripts call
    /// it: `front` is order type 1 (in front of the current orders).
    pub fn attack(&mut self, id: acr_sim::world::EntityId, target: acr_sim::world::EntityId, prio: u8, front: bool) {
        acr_sim::combat::attack_obj_ex(self.world, id.0 as usize, target.0 as usize, prio, front);
    }

    /// Remaps a nation argument through `AssignTBL`.
    pub fn assign(&self, nation: u8) -> u8 {
        self.st.assign.get(usize::from(nation)).copied().unwrap_or(nation)
    }

    /// The circles of a zone handle: a point zone (x = Index, y = Serial, R = low 24 bits of
    /// the type, pixels) or every zone of a `'ZONE'` group. `None` for anything else.
    pub fn zone_circles(&self, h: &Handle) -> Option<Vec<Circle>> {
        if is_point(h) {
            return Some(vec![Circle {
                x: i32::from(h.index),
                y: i32::from(h.serial),
                r: (h.magic & 0xFF_FFFF) as i32,
            }]);
        }
        if h.magic != magic::ZONE {
            return None;
        }
        let ids = self.st.zone_groups.get(usize::from(h.index))?;
        Some(ids.iter().filter_map(|&z| self.st.zones.get(z)).map(|z| Circle { x: z.x, y: z.y, r: z.r }).collect())
    }

    /// The point of a zone handle as most consumers use it: a point zone's centre or the
    /// first zone of a `'ZONE'` group (pixels).
    pub fn zone_point(&self, h: &Handle) -> Option<(i32, i32)> {
        self.zone_circles(h)?.first().map(|c| (c.x, c.y))
    }

    /// Reads a zone handle argument; logs `what` as an error if it is not a zone.
    pub fn zone_arg(&mut self, c: &Call, i: u32, what: &str) -> Result<Option<Vec<Circle>>> {
        if c.u32(i)? == 0 {
            self.st.error(format!("{what} : Incorrect parameter <Zone>"));
            return Ok(None);
        }
        let h = c.handle(i)?;
        let z = self.zone_circles(&h);
        if z.is_none() {
            self.st.error(format!("{what} : Incorrect parameter <Zone>"));
        }
        Ok(z)
    }

    /// The group index of a `'UNIT'` handle argument; logs an error otherwise.
    pub fn group_arg(&mut self, c: &Call, i: u32, what: &str) -> Result<Option<usize>> {
        if c.u32(i)? == 0 {
            self.st.error(format!("{what} : Incorrect parameter <Units>"));
            return Ok(None);
        }
        let h = c.handle(i)?;
        if h.magic != magic::GROUP || usize::from(h.index) >= self.st.groups.len() {
            self.st.error(format!("{what} : Incorrect parameter <Units>"));
            return Ok(None);
        }
        Ok(Some(usize::from(h.index)))
    }

    /// The unit type of a `'UTYP'` handle argument.
    pub fn type_arg(&mut self, c: &Call, i: u32, what: &str) -> Result<Option<acr_sim::TypeId>> {
        if c.u32(i)? == 0 {
            self.st.error(format!("{what} : Incorrect parameter <UnitType>"));
            return Ok(None);
        }
        let h = c.handle(i)?;
        if h.magic != magic::UNIT_TYPE || usize::from(h.index) >= self.world.catalog.len() {
            self.st.error(format!("{what} : Incorrect parameter <UnitType>"));
            return Ok(None);
        }
        Ok(Some(acr_sim::TypeId(h.index)))
    }

    /// The object at a raw index (`Group[i]`): living or dying, `None` once removed.
    pub fn object(&self, id: u16) -> Option<&Unit> {
        if id == NO_UNIT {
            return None;
        }
        self.world.units.get(usize::from(id)).filter(|u| u.alive)
    }

    /// A group member's unit if the slot still holds the unit that joined (serial check)
    /// and it is alive in the engine's sense: `Sdoxlo == 0`, which excludes units inside
    /// buildings (they have `Sdoxlo` = 1 and `Hidden` set).
    pub fn member(&self, m: Member) -> Option<&Unit> {
        let u = self.object(m.id)?;
        (u.active() && self.st.serial(u.id) == m.serial).then_some(u)
    }

    /// Like [`Api::member`] but also accepting units inside buildings (`Hidden || Sdoxlo == 0`,
    /// "aliveH" in the RE notes).
    pub fn member_h(&self, m: Member) -> Option<&Unit> {
        let u = self.object(m.id)?;
        (u.living() && self.st.serial(u.id) == m.serial).then_some(u)
    }

    /// Like [`Api::member_h`], comparing serials without bit 0 (`ser>>1`: captured units match).
    pub fn member_loose(&self, m: Member) -> Option<&Unit> {
        let u = self.object(m.id)?;
        (u.living() && self.st.serial(u.id) >> 1 == m.serial >> 1).then_some(u)
    }

    /// Alive members of a group (not dead, not inside a building), in member order.
    pub fn members(&self, g: usize) -> Vec<EntityId> {
        self.st.groups[g].members.iter().filter_map(|&m| self.member(m)).map(|u| u.id).collect()
    }

    /// Members that are alive or inside a building.
    pub fn members_h(&self, g: usize) -> Vec<EntityId> {
        self.st.groups[g].members.iter().filter_map(|&m| self.member_h(m)).map(|u| u.id).collect()
    }

    /// A member record for a unit.
    pub fn member_of(&self, id: EntityId) -> Member {
        Member { id: id.0 as u16, serial: self.st.serial(id) }
    }

    /// The index of an upgrade by name (exact, like `strcmp`): the tech tree's upgrade list;
    /// without nation data every name is numbered in registration order.
    pub fn upgrade_named(&mut self, name: &str) -> Option<u16> {
        let tech = &self.world.catalog.tech;
        if tech.loaded {
            return tech.find_upgrade(name).map(|u| u.0);
        }
        let i = match self.st.upgrades.iter().position(|u| u == name) {
            Some(i) => i,
            None => {
                self.st.upgrades.push(name.to_owned());
                self.st.upgrades.len() - 1
            }
        };
        Some(i as u16)
    }

    /// A nation's state of an upgrade, if the index is valid.
    pub fn upgrade_state(&mut self, nation: u8, u: u16) -> Option<&mut acr_sim::tech::UpgradeState> {
        self.world.players.get_mut(usize::from(nation))?.upgrades.get_mut(usize::from(u))
    }

    /// Text by identifier (`GetTextByID`): the game's string tables, else the id itself.
    pub fn text(&self, id: &[u8]) -> Vec<u8> {
        let s = String::from_utf8_lossy(id);
        self.st.strings.get(&s).to_vec()
    }
}
