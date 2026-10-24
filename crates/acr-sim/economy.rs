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

const BUILDER: &str = "NAME Kri\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n@MOTION_R 9 0 10 19\r\n\
    @WORK 9 0 0 21\r\n@ATTACK1 9 0 0 27\r\n#PATTACK1 9 1 0 0\r\n#UATTACK1 9 1 0 0\r\nLIFE 50\r\nGEOMETRY 1 10 40\r\n\
    PEASANT\r\nTAKERESSTAGES WOOD 0 5 2\r\nPORTION 1 WOOD 100\r\n[END]\r\n";
const STORE: &str = "NAME Skl\r\nBUILDING\r\nLIFE 1000\r\nLOCATION 0 0 64 64\r\nBUILDSTAGES 10\r\nPRICE 1 WOOD 100\r\n\
    LOCKPOINTS 4 0 0 1 0 0 1 1 1\r\nBUILDLOCKPOINTS 1 0 0\r\nBUILDPOINTS 2 -2 0 3 0\r\nCONCENTRATOR 1 3 3\r\n\
    RESOURCEBASE 1 WOOD\r\nMFARM 25\r\n[END]\r\n";

fn builders() -> (World, TypeId, TypeId) {
    let mut c = Catalog::new();
    let kri = c.add(ObjectType::from_def("Kri(EN)", &UnitDef::parse(BUILDER.as_bytes())));
    let skl = c.add(ObjectType::from_def("Skl(EN)", &UnitDef::parse(STORE.as_bytes())));
    let w = World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default()));
    (w, kri, skl)
}

#[test]
fn peasants_build_a_storehouse() {
    let (mut w, kri, skl) = builders();
    let a = w.spawn(kri, 0, Pos::from_pixels(2000, 2100), 0);
    let b = w.spawn(kri, 0, Pos::from_pixels(2100, 2100), 0);
    economy::census_all(&mut w);
    let wood = w.players[0].resources[WOOD];
    w.step(&[Command::Build { player: 0, units: vec![a, b], kind: skl, x: 2048, y: 2048 }]);
    assert_eq!(w.players[0].resources[WOOD], wood - 100, "paid at placement");
    let site = w.units.iter().position(|u| u.kind == skl).expect("site placed");
    assert_eq!((w.units[site].stage, w.units[site].life), (0, 2));
    let base = (2048 >> 4, 2048 >> 4);
    assert!(w.map.land.check_pt(base.0, base.1), "BUILDLOCKPOINTS locked");
    assert!(!w.map.land.check_pt(base.0 + 1, base.1 + 1), "LOCKPOINTS not yet");
    // Two builders, one stage each per 10-tick #WORK cycle: 10 stages in about 50 ticks plus walking.
    let mut done_at = None;
    for t in 0..400 {
        w.step(&[]);
        if w.units[site].stage >= 10 {
            assert_eq!(w.born, vec![w.units[site].id], "the finished building's BORN_SOUND (0x5247c2)");
            done_at = Some(t);
            break;
        }
        assert!(w.born.is_empty(), "nothing born before the last stage");
        let u = &w.units[site];
        if u.stage > 0 {
            assert_eq!(u.life, u.stage * 1000 / 10, "life follows the stages");
        }
    }
    let t = done_at.expect("finished");
    assert!(t < 150, "finished at tick {t}");
    assert_eq!(w.units[site].life, 1000);
    assert!(w.map.land.check_pt(base.0 + 1, base.1 + 1), "LOCKPOINTS locked when finished");
    economy::census_all(&mut w);
    assert_eq!(w.players[0].housing, 25, "a finished building houses");
}

#[test]
fn peasants_carry_wood_to_the_store() {
    use acr_sim::gather::{Source, Sources};
    let (mut w, kri, skl) = builders();
    let store = w.spawn(skl, 0, Pos::from_pixels(2048, 2048), 0);
    let _ = store;
    let trees: Vec<Source> =
        (0..4).map(|k| Source { x: 2400 + 40 * k, y: 2048, kind: 0, per_work: 4, radius: 30 }).collect();
    w.sources = Arc::new(Sources::new(trees, 16384));
    let p = w.spawn(kri, 0, Pos::from_pixels(2200, 2100), 0);
    economy::census_all(&mut w);
    let wood = w.players[0].resources[WOOD];
    w.step(&[Command::Gather { player: 0, units: vec![p], x: 2400, y: 2048 }]);
    let mut first = None;
    for t in 0..2000 {
        w.step(&[]);
        if w.players[0].resources[WOOD] > wood {
            first = Some(t);
            break;
        }
    }
    let t = first.expect("delivered");
    assert_eq!(w.players[0].resources[WOOD], wood + 100, "a full load of 100 wood");
    // 25 work cycles of 13 ticks (#ATTACK1, 28 frames) plus walking there and back.
    assert!((325..700).contains(&t), "first delivery at tick {t}");
}

const BARRACKS: &str = "NAME Bar\r\nBUILDING\r\nLIFE 1000\r\nLOCATION 0 0 64 64\r\nBUILDSTAGES 10\r\n[END]\r\n";

/// A nation where the centre trains peasants (enabled) and pikemen, which need barracks.
fn nation() -> (World, TypeId, TypeId, TypeId, TypeId) {
    use acr_formats::defs::NationDef;
    let mut c = Catalog::new();
    let kri = c.add(ObjectType::from_def("Kri(EN)", &UnitDef::parse(BUILDER.as_bytes())));
    let pik = c.add(ObjectType::from_def("Hardy(EN)", &UnitDef::parse(HARDY.as_bytes())));
    let cen = c.add(ObjectType::from_def("Cen(EN)", &UnitDef::parse(CENTER.as_bytes())));
    let bar = c.add(ObjectType::from_def("Bar(EN)", &UnitDef::parse(BARRACKS.as_bytes())));
    let def = NationDef::parse(
        b"[FIXED_PRODUCE]\r\nCen(EN) 2\r\nKri(EN) 0 0\r\nHardy(EN) 1 0\r\nKri(EN) 1\r\nBar(EN) 0 0\r\n\
          [ENABLED]\r\nKri(EN)\r\nHardy(EN)\r\nCen(EN)\r\n[ACCESSCONTROL]\r\nHardy(EN) 1 Bar(EN)\r\n\
          [CANSETDEST]\r\nCen(EN)\r\n[END]\r\n",
    );
    let mut tech = acr_sim::tech::Tech::default();
    tech.add_nation(&c, &def);
    c.tech = tech;
    let w = World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default()));
    (w, kri, pik, cen, bar)
}

#[test]
fn production_follows_the_build_lists_and_access_control() {
    let (mut w, kri, pik, cen, bar) = nation();
    let center = w.spawn(cen, 0, Pos::from_pixels(2048, 2048), 0);
    economy::census_all(&mut w);
    assert_eq!(economy::producible(&w, center.0 as usize), [kri], "pikemen need barracks");
    w.step(&[Command::Produce { player: 0, building: center, kind: pik }]);
    w.step(&[Command::Produce { player: 0, building: center, kind: cen }]);
    assert!(w.units[center.0 as usize].queue.is_empty(), "refused");
    // A finished barracks enables the pikemen at the next census.
    let site = w.spawn(bar, 0, Pos::from_pixels(3000, 2048), 0);
    w.units[site.0 as usize].stage = 0;
    economy::census_all(&mut w);
    assert!(!w.players[0].enabled[usize::from(pik.0)], "an unfinished building does not count");
    w.units[site.0 as usize].stage = 10;
    economy::census_all(&mut w);
    assert_eq!(economy::producible(&w, center.0 as usize), [kri, pik]);
    w.step(&[Command::Produce { player: 0, building: center, kind: pik }]);
    assert_eq!(w.units[center.0 as usize].queue.len(), 1);
    // Barracks have no rally point (`[CANSETDEST]`).
    w.step(&[Command::Rally { player: 0, building: site, x: 100, y: 100 }]);
    assert_eq!(w.units[site.0 as usize].rally, None);
}

#[test]
fn produced_units_walk_to_the_rally_point() {
    let (mut w, kri, _, cen, _) = nation();
    let center = w.spawn(cen, 0, Pos::from_pixels(2048, 2048), 0);
    economy::census_all(&mut w);
    w.step(&[
        Command::Rally { player: 0, building: center, x: 2600, y: 2400 },
        Command::Produce { player: 0, building: center, kind: kri },
    ]);
    // BUILDSTAGES 64 (the builder's default): 32 ticks, then the walk.
    for _ in 0..600 {
        w.step(&[]);
    }
    let p = w.units.iter().find(|u| u.kind == kri).expect("produced");
    let (x, y) = p.pos.pixels();
    assert!((x - 2600).abs() <= 32 && (y - 2400).abs() <= 32, "at ({x}, {y})");
    assert!(!p.unlimited && p.orders.is_empty(), "{:?}", p.orders);
}

#[test]
fn a_rally_point_on_trees_sends_peasants_to_chop() {
    use acr_sim::gather::{Source, Sources};
    use acr_sim::world::Order;
    let (mut w, kri, _, cen, _) = nation();
    let trees: Vec<Source> =
        (0..4).map(|k| Source { x: 2600 + 40 * k, y: 2400, kind: 0, per_work: 4, radius: 30 }).collect();
    w.sources = Arc::new(Sources::new(trees, 16384));
    let center = w.spawn(cen, 0, Pos::from_pixels(2048, 2048), 0);
    economy::census_all(&mut w);
    w.step(&[
        Command::Rally { player: 0, building: center, x: 2620, y: 2400 },
        Command::Produce { player: 0, building: center, kind: kri },
    ]);
    for _ in 0..33 {
        w.step(&[]);
    }
    let p = w.units.iter().find(|u| u.kind == kri).expect("produced");
    assert!(matches!(p.orders.first(), Some(Order::Gather { kind: 0, .. })), "{:?}", p.orders);
}

#[test]
fn trading_moves_the_shared_rates() {
    use acr_sim::market::MarketDef;
    let mut text = String::from("a b c d e f g h i j k\r\n");
    for (r, b, s) in [
        ("FOOD", "20 25 40", "5 15 20"),
        ("IRON", "140 190 240", "80 110 140"),
        ("WOOD", "40 50 60", "20 30 40"),
        ("COAL", "140 190 240", "80 110 140"),
        ("STONE", "75 90 105", "30 45 60"),
        ("GOLD", "270 300 330", "180 210 240"),
    ] {
        text += &format!(
            "{r}\r\nBUY {b} CHANGE 100 1.0 TIMERETURN 0.0001\r\nSELL {s} CHANGE 100 1.0 TIMERETURN 0.0001\r\n"
        );
    }
    let mut c = Catalog::new();
    c.market = MarketDef::parse(text.as_bytes());
    let mut w = World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default()));
    let (wood, gold) = (w.players[0].resources[WOOD], w.players[0].resources[GOLD]);
    // 1000 wood at 30 buy gold at 300: 100.
    w.step(&[Command::Trade { player: 0, sell: WOOD as u8, buy: GOLD as u8, amount: 1000 }]);
    assert_eq!(w.players[0].resources[WOOD], wood - 1000);
    assert_eq!(w.players[0].resources[GOLD], gold + 100);
    let def = w.catalog.market.clone().unwrap();
    let (sw, sg) = (def.slot(WOOD).unwrap(), def.slot(GOLD).unwrap());
    // V = 100 · 1e-4 for gold: buy (300 + 330 V) / (1 + V); wood sells at (30 + 20 V') / (1 + V')
    // with V' = 0.1, then one tick of drift.
    let v = 100.0 * 1e-4;
    let gold_buy = (v * 330.0 + 300.0) / (1.0 + v) - def.time_buy[sg];
    assert_eq!(w.market.buy[sg], gold_buy);
    let wood_sell = (0.1 * 20.0 + 30.0) / 1.1 + def.time_sell[sw];
    assert_eq!(w.market.sell[sw], wood_sell);
    // Another player now gets less gold for the same wood.
    w.step(&[Command::Trade { player: 1, sell: WOOD as u8, buy: GOLD as u8, amount: 1000 }]);
    assert!(w.players[1].resources[GOLD] - gold < 100);
    // The rates drift back by TIMERETURN per tick.
    let before = w.market.buy[sg];
    run(&mut w, 100);
    assert!(w.market.buy[sg] < before);
}
