//! Morale on a synthetic flat map: a kill frightens the victim's comrades, some panic and
//! run away out of control, and they calm down again.

use std::sync::Arc;

use acr_formats::defs::UnitDef;
use acr_sim::catalog::{Catalog, ObjectType};
use acr_sim::formation::Formations;
use acr_sim::map::MapGrid;
use acr_sim::{Command, Pos, World};

const PIKEMAN: &str = "NAME Pik\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n\
    @MOTION_R 9 0 10 19\r\n@ATTACK 9 0 0 20\r\n@DEATH 9 0 0 39\r\n#DEATHLIE1 9 1 0 39\r\nLIFE 100\r\n\
    GEOMETRY 1 10 40\r\nDAMAGE 0 10\r\nATTACK_RADIUS 0 40 100\r\nATTACK_PAUSE 0 1\r\nMATHERIAL 1 BODY\r\n\
    CANKILL 1 BODY\r\nFEARFACTOR 3 555\r\nFEARRADIUS 3 255\r\nFEARTYPE 0 3\r\nPSIXOZ 128\r\n[END]\r\n";

#[test]
fn a_kill_makes_comrades_panic_and_flee() {
    let mut c = Catalog::new();
    let pik = c.add(ObjectType::from_def("Pik(EN)", &UnitDef::parse(PIKEMAN.as_bytes())));
    let mut w = World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default()));
    let killer = w.spawn(pik, 0, Pos::from_pixels(1800, 2000), 0);
    let victim = w.spawn(pik, 1, Pos::from_pixels(2000, 2000), 128);
    let comrades: Vec<_> =
        (0..12).map(|k| w.spawn(pik, 1, Pos::from_pixels(2040 + 32 * (k % 4), 1950 + 40 * (k / 4)), 128)).collect();
    w.step(&[]);
    w.units[victim.0 as usize].life = 5;
    acr_sim::combat::make_damage(&mut w, victim.0 as usize, 10, Some(killer.0 as usize), 0);
    assert!(w.units[victim.0 as usize].death != 0);
    // The killer's comrades... are the enemy's: they gained courage; the victim's nation lost it.
    assert!(comrades.iter().all(|c| w.units[c.0 as usize].courage == -1));
    assert_eq!(w.units[killer.0 as usize].courage, 1);
    let panicked: Vec<_> = comrades.iter().copied().filter(|c| w.units[c.0 as usize].orders.len() == 1).collect();
    assert!(!panicked.is_empty() && panicked.len() < comrades.len(), "{} of 12 panicked", panicked.len());
    let p = panicked[0];
    let start = w.units[p.0 as usize].pos;
    let mut ran = false;
    for _ in 0..40 {
        w.step(&[Command::Move { player: 1, units: vec![p], x: 100, y: 100 }]);
        ran |= w.units[p.0 as usize].panicking();
    }
    assert!(ran, "the unit ran out of control");
    let u = &w.units[p.0 as usize];
    let moved = (u.pos.x - start.x).abs().max((u.pos.y - start.y).abs()) >> 4;
    assert!(moved > 60, "it fled {moved} px, ignoring the move order");
    // Sooner or later every panic ends.
    for _ in 0..3000 {
        w.step(&[]);
    }
    assert!(panicked.iter().all(|c| !w.units[c.0 as usize].panicking()));
}
