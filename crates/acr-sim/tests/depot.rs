//! The depot limit of production (`MAXINDEPO`, `ARTDEPO`, 0x49f21b).

use std::sync::Arc;

use acr_formats::defs::UnitDef;
use acr_sim::catalog::{Catalog, ObjectType};
use acr_sim::economy::{self, WOOD};
use acr_sim::formation::Formations;
use acr_sim::map::MapGrid;
use acr_sim::{Command, Pos, World};

const GUN: &str =
    "NAME Gun\r\nLIFE 50\r\nGEOMETRY 1 10 40\r\nBUILDSTAGES 20\r\nPRICE 1 WOOD 10\r\nMAXINDEPO 0 1\r\n[END]\r\n";
const SHOP: &str = "NAME Shop\r\nBUILDING\r\nLIFE 1000\r\nLOCATION 0 0 64 64\r\nBORNPOINTS 2 2 2 2 6\r\n\
    BUILDSTAGES 10\r\nMFARM 50\r\n[END]\r\n";
const DEPOT: &str =
    "NAME Depot\r\nBUILDING\r\nLIFE 1000\r\nLOCATION 0 0 64 64\r\nBUILDSTAGES 10\r\nARTDEPO 1 0 0 0\r\n[END]\r\n";

#[test]
fn production_over_the_depot_limit_is_cancelled() {
    let mut c = Catalog::new();
    let gun = c.add(ObjectType::from_def("Gun(EN)", &UnitDef::parse(GUN.as_bytes())));
    let shop = c.add(ObjectType::from_def("Shop(EN)", &UnitDef::parse(SHOP.as_bytes())));
    let depot = c.add(ObjectType::from_def("Depot(EN)", &UnitDef::parse(DEPOT.as_bytes())));
    let mut w = World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default()));
    let b = w.spawn(shop, 0, Pos::from_pixels(2048, 2048), 0);
    let d = w.spawn(depot, 0, Pos::from_pixels(4096, 2048), 0);
    for id in [b, d] {
        let u = &mut w.units[id.0 as usize];
        u.stage = 10;
    }
    economy::census_all(&mut w);
    w.players[0].resources[WOOD] = 1000;
    // One depot allows one gun: the first is made, the second is refunded.
    w.step(&[Command::Produce { player: 0, building: b, kind: gun }]);
    for _ in 0..20 {
        w.step(&[]);
    }
    let guns = |w: &World| w.units.iter().filter(|u| u.living() && u.kind == gun).count();
    assert_eq!(guns(&w), 1);
    let wood = w.players[0].resources[WOOD];
    w.step(&[Command::Produce { player: 0, building: b, kind: gun }]);
    for _ in 0..20 {
        w.step(&[]);
    }
    assert_eq!(guns(&w), 1, "over the limit");
    assert!(w.units[b.0 as usize].queue.is_empty(), "the order was cancelled");
    assert_eq!(w.players[0].resources[WOOD], wood, "with a refund");
}
