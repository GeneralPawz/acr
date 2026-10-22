//! Upgrades on a synthetic flat map: research, its price and time, effects, links, stages.

use std::sync::Arc;

use acr_formats::defs::{NationDef, UnitDef};
use acr_sim::catalog::{Catalog, ObjectType};
use acr_sim::economy::{self, GOLD};
use acr_sim::formation::Formations;
use acr_sim::map::MapGrid;
use acr_sim::tech::{Tech, UpgradeId};
use acr_sim::upgrade;
use acr_sim::{Command, Pos, TypeId, World};

const PEASANT: &str = "NAME Kri\r\nLIFE 50\r\nGEOMETRY 1 10 40\r\nPEASANT\r\nBUILDSTAGES 40\r\n[END]\r\n";
const PIKEMAN: &str = "NAME Pik\r\nLIFE 100\r\nGEOMETRY 1 10 40\r\nATTACK_RADIUS 0 0 40\r\nDAMAGE 0 7\r\n\
    WEAPONKIND 0 PIKA\r\nPROTECTION 1 PIKA 2\r\nBUILDSTAGES 40\r\n[END]\r\n";
const CENTER: &str = "NAME Cen\r\nBUILDING\r\nLIFE 1000\r\nLOCATION 0 0 64 64\r\nBORNPOINTS 2 2 2 2 6\r\n\
    CONCENTRATOR 1 2 3\r\nMFARM 30\r\nBUILDSTAGES 10\r\nPEASANTABSORBER 5\r\nADDSECTION 0 0 500\r\n[END]\r\n";
const NATION: &[u8] = b"[UPGRADE]\r\n\
    DMG ICO #POSITION 0 0 #LEVEL 1 M #COST 1 GOLD 100 #TIME 21\r\nDAMAGE WEAPONKIND PIKA Pik(EN) +2\r\n\
    DMG2 ICO #POSITION 0 0 #LEVEL 2 M #COST 1 GOLD 100 #TIME 21\r\nPROTECTION GRP 2 PIKA MECH Pik(EN) +3\r\n\
    FAST ICO #POSITION 1 0 #LEVEL 1 M #COST 0 #TIME 3\r\nBUILD GRP 2 Kri(EN) Pik(EN) -50%\r\n\
    TOUGH ICO #POSITION 2 0 #LEVEL 1 M #COST 0 #TIME 3\r\nLIFE GRP 1 Pik(EN) +50%\r\n\
    ROOM ICO #POSITION 3 0 #LEVEL 0 M #COST 0 #TIME 3\r\nSINGLE_INSIDE Cen(EN) +5\r\n\
    ROOM2 ICO #POSITION 3 0 #LEVEL 1 M #COST 0 #TIME 3\r\nSINGLE_INSIDE Cen(EN) +3\r\n\
    FORT ICO #POSITION 4 0 #LEVEL 0 M #COST 0 #TIME 3\r\nUPSTAGE0\r\n\
    [FIXED_PRODUCE]\r\nCen(EN) 2\r\nKri(EN) 0 0\r\nPik(EN) 1 0\r\n[ENABLED]\r\nKri(EN)\r\nPik(EN)\r\nCen(EN)\r\n\
    [UPGRADEPLACE]\r\nCen(EN) 7 DMG\r\nDMG2\r\nFAST\r\nTOUGH\r\nROOM\r\nROOM2\r\nFORT\r\n\
    [UPGRADEENABLE]\r\nDMG\r\nFAST\r\nTOUGH\r\nROOM\r\nROOM2\r\nFORT\r\n[UPGRADELINKS]\r\nDMG 1 DMG2\r\n\
    [PRIVATE]\r\nROOM\r\nROOM2\r\n[UPGRADESTAGE]\r\nFORT\r\n[STAGEDEPENDENT]\r\nPik(EN) 1\r\n[END]\r\n";

fn setup() -> (World, TypeId, TypeId, TypeId) {
    let mut c = Catalog::new();
    let kri = c.add(ObjectType::from_def("Kri(EN)", &UnitDef::parse(PEASANT.as_bytes())));
    let pik = c.add(ObjectType::from_def("Pik(EN)", &UnitDef::parse(PIKEMAN.as_bytes())));
    let cen = c.add(ObjectType::from_def("Cen(EN)", &UnitDef::parse(CENTER.as_bytes())));
    let mut tech = Tech::default();
    tech.add_nation(&c, &NationDef::parse(NATION));
    c.tech = tech;
    let w = World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default()));
    (w, kri, pik, cen)
}

fn up(w: &World, id: &str) -> UpgradeId {
    w.catalog.tech.find_upgrade(id).unwrap()
}

fn run(w: &mut World, ticks: usize) {
    for _ in 0..ticks {
        w.step(&[]);
    }
}

#[test]
fn research_costs_time_and_changes_the_type() {
    let (mut w, kri, pik, cen) = setup();
    let center = w.spawn(cen, 0, Pos::from_pixels(2048, 2048), 0);
    economy::census_all(&mut w);
    let (dmg, dmg2) = (up(&w, "DMG"), up(&w, "DMG2"));
    assert!(upgrade::available(&w, center.0 as usize).contains(&dmg));
    assert!(!upgrade::available(&w, center.0 as usize).contains(&dmg2), "DMG2 needs DMG");
    let gold = w.players[0].resources[GOLD];
    w.step(&[Command::Upgrade { player: 0, buildings: vec![center], upgrade: dmg }]);
    assert_eq!(w.players[0].resources[GOLD], gold - 100, "paid at the start");
    assert!(!upgrade::available(&w, center.0 as usize).contains(&dmg), "not offered while researched");
    // A researching building takes no orders.
    w.step(&[Command::Produce { player: 0, building: center, kind: kri }]);
    assert!(w.units[center.0 as usize].queue.is_empty());
    // #TIME 21 from 1 at +2 per tick: done on the 10th tick, counting the order's tick.
    run(&mut w, 7);
    assert!(w.units[center.0 as usize].research.is_some());
    run(&mut w, 1);
    assert!(w.units[center.0 as usize].research.is_none());
    assert!(w.players[0].upgrades[usize::from(dmg.0)].done);
    assert_eq!(w.adv(0, pik).attacks[0].damage, 9, "+2 damage for player 0");
    assert_eq!(w.adv(1, pik).attacks[0].damage, 7, "other players unchanged");
    assert!(upgrade::available(&w, center.0 as usize).contains(&dmg2), "the link enabled DMG2");
    w.step(&[Command::Upgrade { player: 0, buildings: vec![center], upgrade: dmg2 }]);
    run(&mut w, 10);
    let prot = &w.adv(0, pik).protection;
    assert!(prot.contains(&("PIKA".into(), 5)) && prot.contains(&("MECH".into(), 3)), "{prot:?}");
}

#[test]
fn cancelling_refunds() {
    let (mut w, _, _, cen) = setup();
    let center = w.spawn(cen, 0, Pos::from_pixels(2048, 2048), 0);
    economy::census_all(&mut w);
    let dmg = up(&w, "DMG");
    let gold = w.players[0].resources[GOLD];
    w.step(&[Command::Upgrade { player: 0, buildings: vec![center], upgrade: dmg }]);
    w.step(&[Command::CancelUpgrade { player: 0, building: center }]);
    assert_eq!(w.players[0].resources[GOLD], gold);
    assert!(upgrade::available(&w, center.0 as usize).contains(&dmg));
}

#[test]
fn build_life_private_and_stage_upgrades() {
    let (mut w, kri, pik, cen) = setup();
    let center = w.spawn(cen, 0, Pos::from_pixels(2048, 2048), 0);
    let p = w.spawn(pik, 0, Pos::from_pixels(1000, 1000), 0);
    w.units[p.0 as usize].life = 60;
    economy::census_all(&mut w);
    let b = center.0 as usize;
    // Pikemen need stage 0 of the centre.
    assert_eq!(economy::producible(&w, b), [kri]);
    for id in ["ROOM", "ROOM2", "FAST", "TOUGH", "FORT"] {
        let u = up(&w, id);
        w.step(&[Command::Upgrade { player: 0, buildings: vec![center], upgrade: u }]);
        run(&mut w, 2);
        assert!(w.units[b].research.is_none(), "{id} done");
    }
    assert_eq!(w.adv(0, kri).build_stages, 20, "BUILD -50%");
    assert_eq!(
        (w.units[p.0 as usize].life, w.units[p.0 as usize].max_life),
        (150, 150),
        "LIFE heals to the new maximum"
    );
    assert_eq!(w.units[b].add_inside, 8, "ROOM (level 0) then ROOM2 (level 1)");
    assert_eq!(w.units[b].upgrade_level, 5);
    assert_eq!(w.units[b].stages & 7, 2, "stage 0 done");
    assert_eq!(w.units[b].max_life, 1500, "the stage adds its life");
    assert_eq!(economy::producible(&w, b), [kri, pik]);
    // The halved BUILDSTAGES: a peasant in 10 ticks.
    w.step(&[Command::Produce { player: 0, building: center, kind: kri }]);
    let n = w.units.len();
    run(&mut w, 9);
    assert_eq!(w.units.len(), n + 1);
}
