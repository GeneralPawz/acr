//! A building placed over the player's own units (#143): the placement is accepted, as in the
//! original (only enemies block a site), and the units standing on its construction locks are
//! moved out (0x51374e).

use std::sync::Arc;

use acr_formats::defs::UnitDef;
use acr_sim::catalog::{Catalog, ObjectType};
use acr_sim::formation::Formations;
use acr_sim::map::MapGrid;
use acr_sim::{Pos, World};

const PIKEMAN: &str = "NAME Pik\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n\
    @MOTION_R 9 0 10 19\r\nLIFE 100\r\nGEOMETRY 1 10 40\r\n[END]\r\n";
const CENTRE: &str = "NAME Cen\r\nBUILDING\r\nLIFE 1000\r\nLOCATION 0 0 64 64\r\nBUILDSTAGES 10\r\n\
    LOCKPOINTS 9 0 0 1 0 2 0 0 1 1 1 2 1 0 2 1 2 2 2\r\nBUILDLOCKPOINTS 9 0 0 1 0 2 0 0 1 1 1 2 1 0 2 1 2 2 2\r\n\
    CHECKPOINTS 9 0 0 1 0 2 0 0 1 1 1 2 1 0 2 1 2 2 2\r\n[END]\r\n";

#[test]
fn units_on_a_new_building_are_moved_out() {
    let mut c = Catalog::new();
    let pik = c.add(ObjectType::from_def("Pik(EN)", &UnitDef::parse(PIKEMAN.as_bytes())));
    let cen = c.add(ObjectType::from_def("Cen(EN)", &UnitDef::parse(CENTRE.as_bytes())));
    let mut w = World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default()));
    w.players[0].resources = [10_000; 8];
    // Three pikemen inside the 3 × 3 cells the centre will lock at (2048, 2048).
    let ids: Vec<_> = (0..3).map(|k| w.spawn(pik, 0, Pos::from_pixels(2056 + 16 * k, 2072), 0)).collect();
    let b = acr_sim::construction::place(&mut w, 0, cen, 2048, 2048).expect("own units do not block");
    let locked: Vec<(i32, i32)> = w.catalog.get(cen).lock_cells(w.units[b.0 as usize].pos).collect();
    for id in ids {
        let u = &w.units[id.0 as usize];
        let (cx, cy) = u.cell;
        assert!(!locked.contains(&(cx, cy)), "unit {id:?} still on a lock cell {:?}", u.cell);
        assert!(!w.map.land.check_bar(cx - 1, cy - 1, 3, 3), "a free 3 x 3 box around {:?}", u.cell);
    }
}
