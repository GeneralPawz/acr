//! Animals give up a chase (#355): an optional rule that is not in the original game. The
//! original lets an animal follow a priority-1 attack order until the target is dead
//! (`AttackObjLink` 0x51bcaf..0x5209b7 pops the order only for stand ground and dead,
//! friendly or invalid targets; `SearchVictim` 0x52622b never removes a target), so a bear
//! follows a man across the map. With [`crate::Rules::animal_give_up`] on, an animal (the
//! `ANIMAL` flag, NM+0x172f bit 3) remembers where its chase began. Once its target is more
//! than [`LEASH_FACTOR`] times its `SEARCH_ENEMY_RADIUS` from that point it drops the attack
//! and walks back. It does not take the same target again until that target has left the
//! search radius and come within it again.
//!
//! The distance is a multiple of the animal's own search radius because that is how far it
//! notices an enemy at all (the `.MD` radii: 200 px for the bear): it follows two radii
//! beyond where it first saw the target, then loses interest. `DET_RADIUS` is the weapon's
//! reach (up to 7000 in 1/16 px) and says nothing about interest.
//!
//! Off (the default) nothing is recorded and the state hash stays the original one.
//! See `docs/simulation.md` ("Animals give up").

use std::collections::BTreeMap;

use crate::trig::norma;
use crate::{EntityId, World};

/// Give-up distance in `SEARCH_ENEMY_RADIUS` units.
pub const LEASH_FACTOR: i32 = 3;

/// Where an animal's chase began and the target it gave up on.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Leash {
    /// The chase's start (`RealX/RealY`, 1/16 px); the animal walks back here.
    pub home: (i32, i32),
    /// The target it gave up on, ignored until it has left the search radius.
    pub shy: Option<EntityId>,
}

/// The leashes of the animals on a chase, by unit index.
#[derive(Debug, Clone, Default, Hash)]
pub struct Leashes {
    pub map: BTreeMap<u32, Leash>,
}

impl Leashes {
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Whether animal `i` gave up on `target` and has not seen it leave yet.
    pub fn shy_of(&self, i: usize, target: EntityId) -> bool {
        !self.map.is_empty() && self.map.get(&(i as u32)).is_some_and(|l| l.shy == Some(target))
    }
}

/// One tick of the rule (nothing when it is off): records the start of each animal's chase,
/// sends back the animals whose target got too far and forgets the ones that are home.
pub fn tick(w: &mut World) {
    if !w.rules.animal_give_up {
        return;
    }
    let catalog = w.catalog.clone();
    for i in 0..w.units.len() {
        let t = catalog.get(w.units[i].kind);
        if !t.role.animal {
            continue;
        }
        let key = i as u32;
        if !w.units[i].active() {
            w.leashes.map.remove(&key);
            continue;
        }
        let sr = t.search_radius;
        let pos = w.units[i].pos;
        // The shy target counts as seen leaving once it is out of the search radius (or gone).
        if let Some(l) = w.leashes.map.get_mut(&key)
            && let Some(s) = l.shy
            && w.units.get(s.0 as usize).is_none_or(|o| !o.active() || norma(o.pos.x - pos.x, o.pos.y - pos.y) >= sr)
        {
            l.shy = None;
        }
        match w.units[i].target() {
            Some(target) => {
                let leash = w.leashes.map.entry(key).or_insert(Leash { home: (pos.x, pos.y), shy: None });
                let o = &w.units[target.0 as usize];
                let (dx, dy) = (o.pos.x - leash.home.0, o.pos.y - leash.home.1);
                if norma(dx, dy) > sr * LEASH_FACTOR {
                    leash.shy = Some(target);
                    let home = crate::Pos::new(leash.home.0, leash.home.1);
                    let u = &mut w.units[i];
                    u.orders.pop();
                    // Priority 0, like a patrol leg: an enemy met on the way is fought.
                    crate::motion::send_to(u, home, 0, false);
                }
            }
            None => {
                if w.units[i].orders.is_empty() && w.leashes.map.get(&key).is_some_and(|l| l.shy.is_none()) {
                    w.leashes.map.remove(&key);
                }
            }
        }
    }
}
