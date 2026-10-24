//! Enemy armies (`ResearchArmyDistribution` 0x4423d0) on a synthetic flat map: touching
//! 256 px cells with more than 3 units form one army, a small cell next to it joins, a far
//! group is an army of its own, and units without a counted `USAGE` are left out.

use std::sync::Arc;

use acr_formats::defs::UnitDef;
use acr_sim::armies::Army;
use acr_sim::catalog::{Catalog, ObjectType};
use acr_sim::formation::Formations;
use acr_sim::map::MapGrid;
use acr_sim::{Pos, World};

const SOLDIER: &str = "NAME Sol\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n\
    @MOTION_R 9 0 10 19\r\nLIFE 100\r\nGEOMETRY 1 10 40\r\nUSAGE STRELOK\r\nFORCE 5\r\n[END]\r\n";
const SHEEP: &str = "NAME She\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n\
    @MOTION_R 9 0 10 19\r\nLIFE 100\r\nGEOMETRY 1 10 40\r\nFORCE 9\r\n[END]\r\n";

/// `n` units of `kind` in the 256 px cell `(cx, cy)`.
fn put(w: &mut World, kind: acr_sim::TypeId, owner: u8, (cx, cy): (i32, i32), n: i32) {
    for i in 0..n {
        w.spawn(kind, owner, Pos::from_pixels(cx * 256 + 40 + 32 * (i % 5), cy * 256 + 40 + 40 * (i / 5)), 0);
    }
}

#[test]
fn touching_cells_form_one_army_and_far_ones_their_own() {
    let mut c = Catalog::new();
    let sol = c.add(ObjectType::from_def("Sol(EN)", &UnitDef::parse(SOLDIER.as_bytes())));
    let she = c.add(ObjectType::from_def("She(EN)", &UnitDef::parse(SHEEP.as_bytes())));
    let mut w = World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default()));
    put(&mut w, sol, 0, (5, 5), 1);
    put(&mut w, sol, 1, (10, 10), 5);
    put(&mut w, sol, 1, (11, 10), 4);
    put(&mut w, sol, 1, (10, 11), 1);
    put(&mut w, sol, 1, (30, 30), 2);
    put(&mut w, she, 1, (30, 30), 3);
    assert!(w.armies.of(0).is_none(), "no EnemyInfo before the first tick");
    // Nation 0's EnemyInfo researches when (tmtmt & 255) == 1, i.e. on the first tick.
    w.step(&[]);
    let armies = w.armies.of(0).expect("nation 0 has units").to_vec();
    assert_eq!(
        armies,
        [
            Army { bounds: [10, 10, 11, 11], force: 50, count: 10, x: 10 * 256 + 128, y: 10 * 256 + 128 },
            Army { bounds: [30, 30, 30, 30], force: 10, count: 2, x: 30 * 256 + 128, y: 30 * 256 + 128 },
        ]
    );
    assert!(w.armies.of(1).expect("nation 1 has units").is_empty(), "nation 1 researches on tick 9");
    for _ in 0..8 {
        w.step(&[]);
    }
    assert_eq!(w.armies.of(1).expect("set").len(), 1, "nation 1 sees nation 0's soldier");
    assert!(w.armies.of(7).is_none(), "nation 7 never has an EnemyInfo");
}

#[test]
fn allies_share_an_enemy_info_and_are_no_army() {
    let mut c = Catalog::new();
    let sol = c.add(ObjectType::from_def("Sol(EN)", &UnitDef::parse(SOLDIER.as_bytes())));
    let mut w = World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default()));
    w.nation_mask[0] = 0b11;
    w.nation_mask[1] = 0b11;
    put(&mut w, sol, 0, (5, 5), 4);
    put(&mut w, sol, 1, (6, 5), 4);
    put(&mut w, sol, 2, (20, 5), 4);
    w.step(&[]);
    assert_eq!(w.armies.of(0).expect("set").len(), 1, "only nation 2 is an enemy");
    assert_eq!(w.armies.of(1).expect("shared").len(), 1, "nation 1 shares nation 0's EnemyInfo");
}
