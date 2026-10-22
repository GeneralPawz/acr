//! The engine half of a computer player (`City`, 0x92df18): before every `ProcessAI` the city
//! lists what it could produce now (`EnumProp`, Megapolis.cpp), the AI DLL picks projects
//! with `TryUnit`/`TryUpgrade`, and afterwards the city carries them out (0x432871): units are
//! queued in their producer, buildings are placed near the city centre and built by
//! peasants.
//!
//! Build lists and availability come from acr-sim's tech tree (`[FIXED_PRODUCE]`,
//! `[ENABLED]`, `[ACCESSCONTROL]`). Simplified against the original: no upgrades yet, and
//! building places are searched around the centre instead of the original's farm, mill and
//! field heuristics.

use std::collections::BTreeMap;

use acr_sim::world::EntityId;
use acr_sim::{TypeId, World};

use crate::state::ScriptState;

/// A project the city could start now (`ProposedProject`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Prop {
    pub kind: TypeId,
    /// Building that produces it (`ProducerIndex`); `None` for buildings (peasants build).
    pub producer: Option<EntityId>,
    /// Price as a percentage of the stock (`GetMonsterCostPercent`): the largest
    /// `cost·100 / stock` over the resources, at least 1, 101 when nothing is in stock.
    pub percent: i32,
}

/// The AI city of a nation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct City {
    pub props: Vec<Prop>,
    /// `PRPIndex[type]`.
    pub index: BTreeMap<TypeId, usize>,
    /// `BestProj`: proposals the DLL accepted this call.
    pub best: Vec<usize>,
}

/// `PERCONV[p]` (0x671508): `p·32768/100`.
pub fn perconv(p: i32) -> i32 {
    p * 32768 / 100
}

/// `GetMonsterCostPercent` (Megapolis.cpp 2671).
fn cost_percent(stock: &[i32; 8], cost: &[i32; 8]) -> i32 {
    let mut maxp = 0;
    for (s, c) in stock.iter().zip(cost) {
        if *s != 0 {
            let p = (c * 100 / s).max(1);
            maxp = maxp.max(p);
        }
    }
    if maxp == 0 { 101 } else { maxp }
}

/// Whether the scripts switched a type off for a nation (`EnableUnit(false)`).
fn disabled(st: &ScriptState, nation: u8, kind: TypeId) -> bool {
    st.availability.get(&(nation, false, kind.0)).is_some_and(|&s| s < 0)
}

/// Applies the scripts' manual enable/disable flags (`ManualEnable`/`ManualDisable` of
/// `EnableUnit`, `SetUnitEnableState`, `DisableUpgrade`, `SetUpgradeEnableStatus`) on top of
/// the tech tree's availability, which the census recomputes (`RefreshAbility` 0x42ffed
/// honours them the same way). Called after the flags change and before every tick.
pub fn apply_availability(st: &ScriptState, world: &mut World) {
    for (&(n, upgrade, i), &state) in &st.availability {
        let Some(p) = world.players.get_mut(usize::from(n)) else { continue };
        if upgrade {
            if let Some(u) = p.upgrades.get_mut(usize::from(i)) {
                u.enabled = state > 0 && !u.done;
            }
        } else if let Some(e) = p.enabled.get_mut(usize::from(i)) {
            *e = state > 0;
        }
    }
}

/// `City::EnumProp`: every type an idle finished building (or any peasant) of the nation can
/// produce and the nation can afford.
pub fn enum_prop(st: &mut ScriptState, world: &World, nation: u8) {
    let mut city = City::default();
    let stock = world.players[usize::from(nation)].resources;
    // Producer types with an idle producer, in type order (MinType..MaxType).
    let mut producers: BTreeMap<TypeId, Option<EntityId>> = BTreeMap::new();
    for u in world.units.iter().filter(|u| u.active() && u.owner == nation) {
        let t = world.catalog.get(u.kind);
        if u.building {
            if u.stage >= t.economy.build_stages && u.queue.is_empty() {
                producers.entry(u.kind).or_insert(Some(u.id));
            }
        } else if t.peasant {
            producers.entry(u.kind).or_insert(None);
        }
    }
    let tech = &world.catalog.tech;
    let player = &world.players[usize::from(nation)];
    for (ptype, producer) in producers {
        // The tech tree's build list (`[FIXED_PRODUCE]`), limited to what the nation may
        // produce now (`Enabled`, stage-dependent types: `economy::producible`).
        let list: Vec<TypeId> = match producer {
            Some(b) => acr_sim::economy::producible(world, b.0 as usize),
            None => tech
                .of(ptype)
                .map(|t| {
                    t.produce
                        .iter()
                        .copied()
                        .filter(|k| player.enabled.get(usize::from(k.0)).copied().unwrap_or(false))
                        .collect()
                })
                .unwrap_or_default(),
        };
        for s in list {
            if disabled(st, nation, s) || city.index.contains_key(&s) || city.props.len() >= 100 {
                continue;
            }
            let cost = acr_sim::economy::unit_cost(world, nation, s, None);
            if stock.iter().zip(&cost).any(|(s, c)| c > s) {
                continue;
            }
            let building = world.catalog.get(s).building;
            city.index.insert(s, city.props.len());
            city.props.push(Prop {
                kind: s,
                producer: if building { None } else { producer },
                percent: cost_percent(&stock, &cost),
            });
        }
    }
    st.cities[usize::from(nation)] = city;
}

/// Carries out the projects the DLL accepted (0x432871).
pub fn execute(st: &mut ScriptState, world: &mut World, nation: u8) {
    let city = std::mem::take(&mut st.cities[usize::from(nation)]);
    for &k in &city.best {
        let Some(p) = city.props.get(k) else { continue };
        if let Some(b) = p.producer {
            // `ProduceByAI`: queued in the producer, paying the price.
            acr_sim::economy::produce(world, b.0 as usize, p.kind);
        } else if world.catalog.get(p.kind).building {
            build_near_centre(world, nation, p.kind);
        }
    }
    st.cities[usize::from(nation)] = city;
}

/// Places a building around the city centre (rings of 128 px, random start like the
/// original's `rando() & 4096` jitter) and sends up to 3 idle peasants to build it.
fn build_near_centre(world: &mut World, nation: u8, kind: TypeId) {
    let mine: Vec<&acr_sim::Unit> = world.units.iter().filter(|u| u.active() && u.owner == nation).collect();
    let anchor: Vec<_> = mine.iter().filter(|u| u.building).collect();
    let base: Vec<_> = if anchor.is_empty() { mine.iter().collect() } else { anchor };
    if base.is_empty() {
        return;
    }
    let n = base.len() as i64;
    let cx = (base.iter().map(|u| i64::from(u.pos.x >> 4)).sum::<i64>() / n) as i32;
    let cy = (base.iter().map(|u| i64::from(u.pos.y >> 4)).sum::<i64>() / n) as i32;
    let peasants: Vec<usize> = world
        .units
        .iter()
        .filter(|u| u.active() && u.owner == nation && u.orders.is_empty() && world.catalog.get(u.kind).peasant)
        .map(|u| u.id.0 as usize)
        .take(3)
        .collect();
    if peasants.is_empty() {
        return;
    }
    let start = world.random.rando() & 7;
    for ring in 2..8 {
        for k in 0..8 {
            let dir = ((start + k) & 7) as u8 * 32;
            let (dx, dy) = ((acr_sim::trig::cos(dir) * ring * 128) >> 8, (acr_sim::trig::sin(dir) * ring * 128) >> 8);
            if let Some(b) = acr_sim::construction::place(world, nation, kind, cx + dx, cy + dy) {
                for &i in &peasants {
                    acr_sim::construction::build_obj(world, i, b.0 as usize, 0x80);
                }
                return;
            }
        }
    }
}
