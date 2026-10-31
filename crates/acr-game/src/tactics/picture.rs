//! What the controller sees of a body: one [`Picture`] per [`super::Body`], read from the
//! world at a decision and never stored.

use acr_sim::World;
use acr_sim::catalog::usage;
use acr_sim::{EntityId, TypeId};

use super::frame::to_army;
use super::{Army, Body};

/// A body whose longest ranged attack reaches no further than this, world pixels, fights in
/// melee (the bayonet and pike men; muskets reach 850).
pub const MELEE_REACH: i32 = 150;
/// A soldier stands in its place when it is within this of its formation slot, pixels.
const SLOT_TOLERANCE: i32 = 64;
/// A formation is in order when at least this many percent of its living soldiers are.
const ORDER_PERCENT: usize = 80;

/// One body of an army as it stands now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picture {
    /// The body's index in [`Army::bodies`].
    pub body: usize,
    /// The mean position of the living units, world pixels (the army's origin without any).
    pub centroid: (i32, i32),
    /// The centroid in its own army's frame ([`super::frame`]).
    pub f: i32,
    pub s: i32,
    /// Living units now, and at the start (the body's `units`).
    pub alive: u32,
    pub start: u32,
    /// The formation the body stands in, if any, and its group (`ORD_GROUPS.LST` index).
    pub brigade: Option<u16>,
    pub group: Option<usize>,
    /// The soldiers stand in their formation slots (always, without a formation).
    pub in_order: bool,
    /// The body is mounted (`USAGE` 8, 22 or 24).
    pub cavalry: bool,
    /// The largest range of a ranged attack of the body's soldiers, world pixels (0 without).
    pub reach: i32,
    /// The soldiers fight in melee ([`MELEE_REACH`]).
    pub melee: bool,
    /// Units in panic (`unlimited`, whose orders the world ignores).
    pub panicking: u32,
    /// The way the body faces: its formation's, else its first living unit's.
    pub facing: u8,
}

/// A unit that is alive and not dying.
pub fn living(world: &World, id: EntityId) -> Option<&acr_sim::Unit> {
    world.units.get(id.0 as usize).filter(|u| u.alive && u.death == 0 && !u.building)
}

/// The pictures of all bodies of `army`, in the order of its bodies.
pub fn of(world: &World, army: &Army) -> Vec<Picture> {
    army.bodies.iter().enumerate().map(|(i, b)| picture(world, army, i, b)).collect()
}

fn picture(world: &World, army: &Army, index: usize, body: &Body) -> Picture {
    let live: Vec<&acr_sim::Unit> = body.units.iter().filter_map(|&id| living(world, id)).collect();
    let n = live.len() as i64;
    let centroid = if n == 0 {
        army.origin
    } else {
        let sum = live.iter().fold((0i64, 0i64), |a, u| {
            let (x, y) = u.pos.pixels();
            (a.0 + i64::from(x), a.1 + i64::from(y))
        });
        ((sum.0 / n) as i32, (sum.1 / n) as i32)
    };
    let (f, s) = to_army(army.facing, centroid.0 - army.origin.0, centroid.1 - army.origin.1);
    // The brigade of the first living unit that still has one.
    let brigade =
        live.iter().filter_map(|u| u.brigade).find(|&b| world.brigades.get(usize::from(b)).is_some_and(|b| b.alive));
    let formation = brigade.map(|b| &world.brigades[usize::from(b)]);
    let in_order = formation.is_none_or(|br| {
        let (mut soldiers, mut placed) = (0, 0);
        for (k, member) in br.members.iter().enumerate().skip(3) {
            let Some(u) = member.and_then(|id| living(world, id)) else { continue };
            soldiers += 1;
            let (x, y) = u.pos.pixels();
            let (dx, dy) = (i64::from(x - br.slots[k].0), i64::from(y - br.slots[k].1));
            placed += usize::from(dx * dx + dy * dy <= i64::from(SLOT_TOLERANCE * SLOT_TOLERANCE));
        }
        placed * 100 >= soldiers * ORDER_PERCENT
    });
    // The body's soldiers are its commonest type (the commanders are one to three).
    let mut kinds: Vec<(TypeId, usize)> = Vec::new();
    for u in &live {
        match kinds.iter_mut().find(|k| k.0 == u.kind) {
            Some(k) => k.1 += 1,
            None => kinds.push((u.kind, 1)),
        }
    }
    let kind = kinds.iter().fold(None, |best: Option<(TypeId, usize)>, &k| best.filter(|b| b.1 >= k.1).or(Some(k)));
    let (cavalry, reach) = match (kind, live.first()) {
        (Some((kind, _)), Some(u)) => {
            let cavalry = matches!(
                world.catalog.get(kind).role.usage,
                usage::FASTHORSE | usage::HARDHORCE | usage::HORSE_STRELOK
            );
            let adv = world.adv(u.owner, kind);
            (cavalry, adv.attacks.iter().filter(|a| a.weapon.is_some()).map(|a| a.radius.1).max().unwrap_or(0))
        }
        _ => (false, 0),
    };
    Picture {
        body: index,
        centroid,
        f,
        s,
        alive: live.len() as u32,
        start: body.units.len() as u32,
        brigade,
        group: formation.and_then(|br| world.formations.shapes[br.shape].group),
        in_order,
        cavalry,
        reach,
        melee: reach <= MELEE_REACH,
        panicking: live.iter().filter(|u| u.unlimited).count() as u32,
        facing: formation.map_or_else(|| live.first().map_or(army.facing, |u| u.dir), |br| br.dir),
    }
}
