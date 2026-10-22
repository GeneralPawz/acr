//! Economy on a synthetic flat map: upkeep, starvation, production, prices, mines.

use std::sync::Arc;

use acr_formats::defs::UnitDef;
use acr_sim::catalog::{Catalog, ObjectType};
use acr_sim::economy::{self, Deposit, FOOD, GOLD, WOOD};
use acr_sim::formation::Formations;
use acr_sim::map::MapGrid;
use acr_sim::world::EntityId;
use acr_sim::{Command, Pos, TypeId, World};

const PEASANT: &str = "NAME Kri\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n@MOTION_R 9 0 10 19\r\n\
    LIFE 50\r\nGEOMETRY 1 10 40\r\nPEASANT\r\nBUILDSTAGES 20\r\nPRICE 1 FOOD 50\r\nRESCONSUMER FOOD 32\r\n[END]\r\n";
const HARDY: &str = "NAME Hardy\r\nLIFE 50\r\nGEOMETRY 1 10 40\r\nNO_HUNGRY\r\n[END]\r\n";
const CENTER: &str = "NAME Cen\r\nBUILDING\r\nLIFE 1000\r\nLOCATION 0 0 64 64\r\nBORNPOINTS 2 2 2 2 6\r\n\
    CONCENTRATOR 1 2 3\r\nMFARM 3\r\nBUILDSTAGES 10\r\nPRICE 1 WOOD 100\r\nCOSTPERCENT 200\r\nPEASANTABSORBER 5\r\n\
    PRODUCER 1 GOLD 0 10\r\n[END]\r\n";

fn setup() -> (World, TypeId, TypeId, TypeId) {
    let mut c = Catalog::new();
    let kri = c.add(ObjectType::from_def("Kri(EN)", &UnitDef::parse(PEASANT.as_bytes())));
    let hardy = c.add(ObjectType::from_def("Hardy(EN)", &UnitDef::parse(HARDY.as_bytes())));
    let cen = c.add(ObjectType::from_def("Cen(EN)", &UnitDef::parse(CENTER.as_bytes())));
    let w = World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default()));
    (w, kri, hardy, cen)
}

fn run(w: &mut World, ticks: usize) {
    for _ in 0..ticks {
        w.step(&[]);
    }
}

#[test]
fn units_eat_food() {
    let (mut w, kri, _, _) = setup();
    for i in 0..10 {
        w.spawn(kri, 0, Pos::from_pixels(1000 + 40 * i, 1000), 0);
    }
    economy::census_all(&mut w);
    w.players[0].resources[FOOD] = 1000;
    run(&mut w, 1000);
    // 3/1000 food per unit and tick plus 32/10000 per peasant (RESCONSUMER): 30 + 32 = 62.
    let eaten = 1000 - w.players[0].resources[FOOD];
    assert!((60..=63).contains(&eaten), "ate {eaten}");
}

#[test]
fn famine_kills_all_but_the_hardy() {
    let (mut w, kri, hardy, _) = setup();
    let peasants: Vec<EntityId> = (0..20).map(|i| w.spawn(kri, 0, Pos::from_pixels(1000 + 40 * i, 1000), 0)).collect();
    let tough: Vec<EntityId> = (0..5).map(|i| w.spawn(hardy, 0, Pos::from_pixels(1000 + 40 * i, 1200), 0)).collect();
    economy::census_all(&mut w);
    w.players[0].resources[FOOD] = 0;
    run(&mut w, 3000);
    let dead = |ids: &[EntityId]| ids.iter().filter(|&&id| !w.units[id.0 as usize].living()).count();
    assert!(dead(&peasants) > 0, "starvation kills");
    assert_eq!(dead(&tough), 0, "NO_HUNGRY units survive");
}

#[test]
fn buildings_produce_units_and_prices_grow() {
    let (mut w, kri, _, cen) = setup();
    let center = w.spawn(cen, 0, Pos::from_pixels(2048, 2048), 0);
    let second = w.spawn(cen, 0, Pos::from_pixels(4096, 2048), 0);
    economy::census_all(&mut w);
    // Two centres exist: the next one costs 100 * 2 * 2.
    assert_eq!(economy::unit_cost(&w, 0, cen, None)[WOOD], 400);
    let food = w.players[0].resources[FOOD];
    w.step(&[Command::Produce { player: 0, building: center, kind: kri }]);
    assert_eq!(w.players[0].resources[FOOD], food - 50, "paid when queued");
    assert_eq!(w.units[center.0 as usize].queue.len(), 1);
    // BUILDSTAGES 20 at +2 per tick, counting the tick of the order: done on the 10th.
    run(&mut w, 8);
    let before = w.units.len();
    run(&mut w, 1);
    assert_eq!(w.units.len(), before + 1, "the peasant appeared");
    let p = w.units.last().unwrap();
    assert_eq!(p.kind, kri);
    // It walks out to the last born point (base cell + (2, 6)) and settles there.
    run(&mut w, 100);
    let p = w.units.last().unwrap();
    assert!(!p.unlimited && p.orders.is_empty(), "{:?}", p.orders);
    // Born points are cell corners (`cell << 8`), whose lock cell is one less; the unit stops
    // within STOPDISTANCE (one cell) of it.
    let target = ((2048 >> 4) + 1, (2048 >> 4) + 5);
    assert!((p.cell.0 - target.0).abs() <= 1 && (p.cell.1 - target.1).abs() <= 1, "{:?}", p.cell);
    // Housing: 2 centres * 3 = 6. With 6 units the queue waits.
    for i in 0..5 {
        w.spawn(kri, 0, Pos::from_pixels(1000 + 40 * i, 3000), 0);
    }
    economy::census_all(&mut w);
    w.step(&[Command::Produce { player: 0, building: second, kind: kri }]);
    run(&mut w, 50);
    assert_eq!(w.units[second.0 as usize].queue.len(), 1, "waits for housing");
    // Cancelling refunds the price.
    let food = w.players[0].resources[FOOD];
    w.step(&[Command::Unproduce { player: 0, building: second, kind: kri }]);
    assert!(w.units[second.0 as usize].queue.is_empty());
    assert!(w.players[0].resources[FOOD] >= food + 49, "refunded");
}

#[test]
fn peasants_in_a_mine_produce_gold() {
    let (mut w, kri, _, cen) = setup();
    w.deposits.push(Deposit { x: 2048, y: 2048, resource: GOLD });
    let mine = w.spawn(cen, 0, Pos::from_pixels(2048, 2048), 0);
    economy::bind_mines(&mut w);
    assert!(w.units[mine.0 as usize].mine.is_some(), "bound to the deposit");
    let workers: Vec<EntityId> = (0..5).map(|i| w.spawn(kri, 0, Pos::from_pixels(1800 + 30 * i, 2300), 0)).collect();
    economy::census_all(&mut w);
    w.step(&[Command::Enter { player: 0, units: workers.clone(), building: mine }]);
    run(&mut w, 300);
    assert_eq!(w.units[mine.0 as usize].inside.len(), 5, "all went in");
    let gold = w.players[0].resources[GOLD];
    run(&mut w, 1000);
    // 5 peasants * 10 << 3 = 400 per tick -> 0.4 gold per tick.
    let earned = w.players[0].resources[GOLD] - gold;
    assert!((399..=401).contains(&earned), "earned {earned}");
    // Leaving puts them back on the map.
    w.step(&[Command::Leave { player: 0, building: mine }]);
    assert!(w.units[mine.0 as usize].inside.is_empty());
    assert!(workers.iter().all(|&id| w.units[id.0 as usize].inside_of.is_none()));
}
