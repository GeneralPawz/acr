//! The engine half of a computer player (`City`, 0x92df18): before every `ProcessAI` the city
//! lists what it could produce now (`EnumProp`, Megapolis.cpp), the AI DLL picks projects
//! with `TryUnit`/`TryUpgrade`, and afterwards the city carries them out (0x432871): units are
//! queued in their producer, buildings are placed near the city centre and built by
//! peasants.
//!
//! Build lists and availability come from acr-sim's tech tree (`[FIXED_PRODUCE]`,
//! `[ENABLED]`, `[ACCESSCONTROL]`), upgrades are researched with acr-sim's `PerformUpgrade`.
//! Buildings are placed by acr-sim's `aiplace` (the original's approximate place per kind and
//! exact site test); with a buildings collector the DLL sends the builders.

use std::collections::BTreeMap;

use acr_sim::world::EntityId;
use acr_sim::{TypeId, World};

use crate::state::ScriptState;

/// A project the city could start now (`ProposedProject`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Prop {
    pub kind: TypeId,
    /// An upgrade project (`PKind` 1) instead of a unit; `kind` is then unused.
    pub upgrade: Option<acr_sim::tech::UpgradeId>,
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
    /// `UPGIndex[upgrade]`.
    pub upgrade_index: BTreeMap<u16, usize>,
    /// `BestProj`: proposals the DLL accepted this call.
    pub best: Vec<usize>,
    /// `BuildingsCollector`, `UnitsCollector`, `EnemyBuildingsCollector` (City+0xf46..):
    /// script groups that collect the nation's new buildings, the units its AI produces and
    /// new enemy buildings.
    pub buildings_collector: Option<usize>,
    pub units_collector: Option<usize>,
    pub enemy_collector: Option<usize>,
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
            if u.stage >= world.max_stage(u) && u.queue.is_empty() {
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
                upgrade: None,
                producer: if building { None } else { producer },
                percent: cost_percent(&stock, &cost),
            });
        }
    }
    // Upgrades the idle buildings offer (`AddUpgr`), with the same price percentage.
    let idle: Vec<usize> = world
        .units
        .iter()
        .filter(|u| {
            u.active() && u.owner == nation && u.building && u.queue.is_empty() && u.stage >= world.max_stage(u)
        })
        .map(|u| u.id.0 as usize)
        .collect();
    for b in idle {
        for up in acr_sim::upgrade::available(world, b) {
            if city.upgrade_index.contains_key(&up.0)
                || city.props.len() >= 100
                || !acr_sim::upgrade::can_research(world, b, up)
            {
                continue;
            }
            let cost = world.catalog.tech.upgrade(up).cost;
            city.upgrade_index.insert(up.0, city.props.len());
            city.props.push(Prop {
                kind: TypeId(0),
                upgrade: Some(up),
                producer: Some(world.units[b].id),
                percent: cost_percent(&stock, &cost),
            });
        }
    }
    st.cities[usize::from(nation)] = city;
}

/// Carries out the projects the DLL accepted (0x432871), then the pending building project's
/// step.
pub fn execute(st: &mut ScriptState, world: &mut World, nation: u8) {
    let city = std::mem::take(&mut st.cities[usize::from(nation)]);
    for &k in &city.best {
        let Some(p) = city.props.get(k) else { continue };
        if let (Some(up), Some(b)) = (p.upgrade, p.producer) {
            // `PerformUpgrade` in the producer.
            acr_sim::upgrade::start(world, b.0 as usize, up);
        } else if let Some(b) = p.producer {
            // `ProduceByAI`: queued in the producer, paying the price; with a units collector
            // the unit joins it when it is born.
            if acr_sim::economy::produce(world, b.0 as usize, p.kind, false)
                && let Some(g) = city.units_collector
            {
                st.production.push(crate::state::Production { building: b, kind: p.kind, group: g, ready: 0 });
            }
        } else if world.catalog.get(p.kind).building {
            acr_sim::aiplace::accept(world, nation, p.kind);
        }
    }
    acr_sim::aiplace::process(world, nation, city.buildings_collector.is_some());
    st.cities[usize::from(nation)] = city;
}
