//! Changes to the [`World`] the scripts make directly (not through player commands): killing,
//! erasing, changing the owner, creating objects.

use acr_sim::world::EntityId;
use acr_sim::{Pos, TypeId, World};

/// `Die()`: the object dies with its death animation and leaves a corpse.
pub fn kill(world: &mut World, id: EntityId) {
    world.kill(id);
}

/// `DestructBuilding` (0x4e7720), behind `DieUnit` and `SelDie`: a finished building (or one
/// with an order) of a `SLOWDEATH` type only drops to 1999 life and then burns down
/// (`acr_sim::combat::slow_death`); anything else dies.
pub fn destruct(world: &mut World, id: EntityId) {
    if world.units.get(id.0 as usize).is_some_and(|u| u.living()) {
        acr_sim::combat::destruct_building(world, id.0 as usize);
    }
}

/// `SelErase`: dies and disappears at once (no corpse).
pub fn erase(world: &mut World, id: EntityId) {
    world.remove(id);
}

/// `SelChangeNation`: the object now belongs to `owner`.
pub fn set_owner(world: &mut World, id: EntityId, owner: u8) {
    if let Some(u) = world.units.get_mut(id.0 as usize) {
        u.owner = owner;
        u.orders.clear();
        u.path.clear();
    }
}

/// `CreateNewMonsterAt(x, y, type, Anyway=1)` (0x5155c5): a new object at `pos` (1/16 px),
/// no cost or buildability check; fails outside the map. Buildings are created finished.
pub fn create(world: &mut World, kind: TypeId, owner: u8, pos: Pos) -> Option<EntityId> {
    let side = world.map.land.width * 16 * 16;
    if pos.x < 1024 || pos.y < 1024 || pos.x >= side || pos.y >= side {
        return None;
    }
    // The original gives every new unit a random direction (`rando() & 255`, 0x5162da).
    let dir = (world.random.rando() & 255) as u8;
    Some(world.spawn(kind, owner, pos, dir))
}
