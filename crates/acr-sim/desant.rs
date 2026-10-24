//! The AI landing on a synthetic map: land west of x = 1500 px (the AI's island 0), a sea up
//! to x = 3000 px, land east of it (the enemy's island 1). A transport of the computer player
//! loads the units of its start group at the west coast and lands them at the east coast.

use std::sync::Arc;

use acr_formats::defs::UnitDef;
use acr_sim::catalog::{Catalog, ObjectType};
use acr_sim::desant::CostPlace;
use acr_sim::formation::{BOrder, Formations};
use acr_sim::map::MapGrid;
use acr_sim::world::EntityId;
use acr_sim::{Pos, TypeId, World};

const FERRY: &str = "NAME Par\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 65 1 0 0\r\n#MOTION_L 65 1 0 0\r\nLIFE 1000\r\n\
    GEOMETRY 1 100 16\r\nMEDIA WATER\r\nMOTIONSTYLE SHEEPS\r\nTRANSPORT 250\r\nMATHERIAL 1 WOOD\r\n[END]\r\n";
const SOLDIER: &str = "NAME Sol\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n\
    @MOTION_R 9 0 10 19\r\nLIFE 100\r\nGEOMETRY 1 10 40\r\nMATHERIAL 1 BODY\r\n[END]\r\n";

fn setup() -> (World, TypeId, TypeId) {
    let mut g = MapGrid::open(1);
    let side = g.land.width;
    let (a, b) = (1500 >> 4, 3000 >> 4);
    g.water.clr_rect(a, 1, b - a, side - 2);
    g.land.set_rect(a, 0, b - a, side);
    let mut c = Catalog::new();
    let par = c.add(ObjectType::from_def("Par(EN)", &UnitDef::parse(FERRY.as_bytes())));
    let sol = c.add(ObjectType::from_def("Sol(EN)", &UnitDef::parse(SOLDIER.as_bytes())));
    let mut w = World::new(Arc::new(g), Arc::new(c), Arc::new(Formations::default()));
    // A coast place on each side, 10 cells (16 px) from the shore.
    let place = |xw: i32, xc: i32, island: u8| CostPlace {
        xw,
        yw: 125,
        xc,
        yc: 125,
        island: Some(island),
        r: 6,
        access: true,
        transport: 0xFFFF,
    };
    w.desant.places = vec![place(104, 84, 0), place(177, 197, 1)];
    w.desant.cities[0].my_island = 1;
    w.desant.cities[1].my_island = 0;
    w.ai[1] = true;
    (w, par, sol)
}

fn diversion(w: &World) -> Option<&acr_sim::desant::Diversion> {
    w.brigades.iter().filter(|b| b.alive).find_map(|b| match b.orders.last() {
        Some(BOrder::Diversion(d)) => Some(d.as_ref()),
        _ => None,
    })
}

#[test]
fn a_transport_lands_the_start_group_on_the_enemy_island() {
    let (mut w, par, sol) = setup();
    let soldiers: Vec<EntityId> = (0..5).map(|i| w.spawn(sol, 1, Pos::from_pixels(1200 - 40 * i, 1925), 0)).collect();
    w.desant.cities[1].start = Some(soldiers.clone());
    w.desant.cities[1].dest = true;
    w.desant.cities[1].max_in_transport = 3;
    let f = w.spawn(par, 1, Pos::from_pixels(2200, 2000), 0);
    let t = f.0 as usize;
    // `RegisterNewUnit`: the new transport has a diversion brigade with the city's load.
    assert_eq!(diversion(&w).map(|d| d.max_u), Some(3));
    let mut loaded = false;
    for _ in 0..12_000 {
        w.step(&[]);
        let d = diversion(&w).expect("the order stays");
        loaded |= d.phase == 1;
        if loaded && w.desant.cities[1].landed.len() == 3 {
            break;
        }
    }
    assert!(loaded, "set off: {:?}", diversion(&w));
    let landed = w.desant.cities[1].landed.clone();
    assert_eq!(landed.len(), 3, "landed: {landed:?}, transport {:?}", w.units[t].dock);
    // The three last units of the start group went, the others stay.
    assert_eq!(w.desant.cities[1].taken, soldiers[2..].iter().rev().copied().collect::<Vec<_>>());
    assert_eq!(w.desant.cities[1].start.as_deref(), Some(&soldiers[..2]));
    for id in &landed {
        let (x, _) = w.units[id.0 as usize].pos.pixels();
        assert!(x > 3000, "{id:?} on the east coast: x {x}");
    }
    // Empty, the transport is free again and sinks a while later.
    for _ in 0..400 {
        w.step(&[]);
    }
    assert!(!w.units[t].living(), "the transport sank");
}

#[test]
fn a_short_start_group_sets_off_after_256_ticks() {
    let (mut w, par, sol) = setup();
    let soldiers: Vec<EntityId> = (0..2).map(|i| w.spawn(sol, 1, Pos::from_pixels(1200 - 40 * i, 1925), 0)).collect();
    w.desant.cities[1].start = Some(soldiers.clone());
    w.desant.cities[1].dest = true;
    let f = w.spawn(par, 1, Pos::from_pixels(2200, 2000), 0);
    let mut aboard = None;
    for _ in 0..6000 {
        w.step(&[]);
        let d = diversion(&w).expect("the order stays");
        if aboard.is_none() && w.units[f.0 as usize].inside.len() == 2 {
            aboard = Some(w.tick.0);
        }
        if d.phase == 1 {
            // Nobody left on the way for 256 ticks: the load is complete (`MaxU = NU`).
            assert_eq!(d.max_u, 2);
            let waited = w.tick.0 - aboard.expect("both aboard first");
            assert!((256..=256 + 16).contains(&waited), "set off {waited} ticks after boarding");
            return;
        }
    }
    panic!("never set off: {:?}", diversion(&w));
}

#[test]
fn coast_places_refresh_access_and_transport() {
    let (mut w, par, sol) = setup();
    // Seven units of nation 0 around the east land point: no access.
    for i in 0..7 {
        w.spawn(sol, 0, Pos::from_pixels(3150 + 20 * i, 2000), 0);
    }
    let f = w.spawn(par, 0, Pos::from_pixels(2832, 2000), 0);
    for _ in 0..2 {
        w.step(&[]);
    }
    let east = w.desant.places[1];
    assert!(!east.access);
    assert_eq!(east.transport, f.0 as u16);
    let west = w.desant.places[0];
    assert!(west.access);
    assert_eq!(west.transport, 0xFFFF);
}
