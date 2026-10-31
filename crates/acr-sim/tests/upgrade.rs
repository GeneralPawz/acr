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
    ROOM ICO #POSITION 3 0 #LEVEL 2 M #COST 0 #TIME 3\r\nSINGLE_INSIDE Cen(EN) +5\r\n\
    ROOM2 ICO #POSITION 3 0 #LEVEL 3 M #COST 0 #TIME 3\r\nSINGLE_INSIDE Cen(EN) +3\r\n\
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
    w.step(&[Command::Produce { player: 0, building: center, kind: kri, endless: false }]);
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
    assert_eq!(w.units[b].add_inside, 8, "ROOM (level 2, the start) then ROOM2 (level 3)");
    assert_eq!(w.units[b].upgrade_level, 7);
    assert_eq!(w.units[b].stages & 7, 2, "stage 0 done");
    assert_eq!(w.units[b].max_life, 1500, "the stage adds its life");
    assert_eq!(economy::producible(&w, b), [kri, pik]);
    // The halved BUILDSTAGES: a peasant in 10 ticks.
    w.step(&[Command::Produce { player: 0, building: center, kind: kri, endless: false }]);
    let n = w.units.len();
    run(&mut w, 9);
    assert_eq!(w.units.len(), n + 1);
}

const FORT: &str = "NAME Frt\r\nBUILDING\r\nLIFE 1000\r\nLOCATION 0 0 64 64\r\nBUILDSTAGES 10\r\n\
    ADDSECTION 0 0 500 1 0 0 2 0 0 3 0 0 4 0 0\r\nADDSECTION 1 0 300 5 0 0 6 0 0 7 0 0 8 0 0\r\n[END]\r\n";
const FORT_NATION: &[u8] = b"[UPGRADE]\r\n\
    SEC0 ICO #POSITION 0 0 #LEVEL 0 M #COST 1 GOLD 100 #TIME 11\r\nUPSTAGE0\r\n\
    SEC1 ICO #POSITION 1 0 #LEVEL 0 M #COST 0 #TIME 3\r\nUPSTAGE1\r\n\
    [FIXED_PRODUCE]\r\nFrt(EN) 0\r\n[ENABLED]\r\nFrt(EN)\r\n[UPGRADEPLACE]\r\nFrt(EN) 2 SEC0\r\nSEC1\r\n\
    [UPGRADEENABLE]\r\nSEC0\r\nSEC1\r\n[UPGRADESTAGE]\r\nSEC0\r\nSEC1\r\n[STAGEDEPENDENT]\r\nSEC1 1\r\n[END]\r\n";

#[test]
fn sections_are_built_ruined_and_rebuilt() {
    use acr_sim::combat::make_damage;
    use acr_sim::upgrade::{section, section_state};
    let mut c = Catalog::new();
    let frt = c.add(ObjectType::from_def("Frt(EN)", &UnitDef::parse(FORT.as_bytes())));
    let mut tech = Tech::default();
    tech.add_nation(&c, &NationDef::parse(FORT_NATION));
    c.tech = tech;
    let mut w = World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default()));
    let fort = w.spawn(frt, 0, Pos::from_pixels(2048, 2048), 0);
    economy::census_all(&mut w);
    let b = fort.0 as usize;
    let (sec0, sec1) = (up(&w, "SEC0"), up(&w, "SEC1"));
    let state = |w: &World, s| section_state(w.units[b].stages, s);
    // Created like 0x516150 / 0x515ff8.
    assert_eq!((w.units[b].stages, w.units[b].upgrade_level), (0x8000, 2));
    assert_eq!(upgrade::available(&w, b), [sec0], "SEC1 needs section 0 done");

    // Building section 0: state 1 while researched.
    w.step(&[Command::Upgrade { player: 0, buildings: vec![fort], upgrade: sec0 }]);
    assert_eq!(state(&w, 0), section::BUILDING);
    assert_eq!(upgrade::available(&w, b), [sec0], "offered (in progress) until done");
    // Losing life down to the own life drops it back to 0; the research goes on.
    make_damage(&mut w, b, 600, None, 0xFF);
    assert_eq!((w.units[b].life, state(&w, 0)), (400, section::NONE));
    w.step(&[]);
    assert_eq!(state(&w, 0), section::BUILDING);
    run(&mut w, 4);
    assert!(w.units[b].research.is_none());
    assert_eq!(state(&w, 0), section::DONE);
    assert_eq!((w.units[b].life, w.units[b].max_life), (1500, 1500), "done: type life + section life");
    assert_eq!(upgrade::available(&w, b), [sec1]);

    w.step(&[Command::Upgrade { player: 0, buildings: vec![fort], upgrade: sec1 }]);
    run(&mut w, 2);
    assert_eq!((state(&w, 0), state(&w, 1)), (section::DONE, section::DONE));
    assert_eq!((w.units[b].life, w.units[b].max_life), (1800, 1800));
    assert!(upgrade::available(&w, b).is_empty());

    // Above own life + section 0 (1500): no change; the upgrade count is reset to 2.
    w.units[b].upgrade_level = 9;
    make_damage(&mut w, b, 200, None, 0xFF);
    assert_eq!((state(&w, 0), state(&w, 1), w.units[b].upgrade_level), (section::DONE, section::DONE, 2));
    // At or below it section 1 is ruined; life and maximum stay.
    make_damage(&mut w, b, 100, None, 0xFF);
    assert_eq!((state(&w, 0), state(&w, 1)), (section::DONE, section::RUIN));
    assert_eq!((w.units[b].life, w.units[b].max_life), (1500, 1800));
    // A ruin is offered again; rebuilding it restores the life.
    assert_eq!(upgrade::available(&w, b), [sec1]);
    w.step(&[Command::Upgrade { player: 0, buildings: vec![fort], upgrade: sec1 }]);
    run(&mut w, 2);
    assert_eq!((state(&w, 1), w.units[b].life, w.units[b].max_life), (section::DONE, 1800, 1800));
    // Down to the own life both fall; section 1 counts section 0's life although it just fell.
    make_damage(&mut w, b, 800, None, 0xFF);
    assert_eq!((state(&w, 0), state(&w, 1)), (section::RUIN, section::RUIN));
    assert_eq!(upgrade::available(&w, b), [sec0], "SEC1 needs section 0 again");
}

const RUIN_FORT: &str = "NAME Frt\r\nBUILDING\r\nLIFE 1000\r\nLOCATION 0 0 64 64\r\nBUILDSTAGES 10\r\n\
    DESTRUCT 32767 2 BOOM BOOM\r\nADDSECTION 0 0 500 1 0 0 2 0 0 3 10 200 4 0 0\r\n\
    EXPLPOINTS 0 3 1 2 3 4 5 6\r\nSTAGELOCKPOINTS 0 2 7 7 7 8\r\nSTAGELOCKPOINTS 3 1 9 9\r\n[END]\r\n";
const RUIN_NDS: &[u8] = b"[MEMBERS]\r\nBOOM #POINT 0 1 NONE HI_GRAVITY1 RANDOM 3 -1\r\n[END]\r\n";

#[test]
fn a_ruined_section_explodes_at_its_cells() {
    use acr_sim::combat::make_damage;
    use acr_sim::upgrade::{section, section_state};
    let mut c = Catalog::new();
    let frt = c.add(ObjectType::from_def("Frt(EN)", &UnitDef::parse(RUIN_FORT.as_bytes())));
    c.set_weapons(acr_sim::weapon::Weapons::parse(RUIN_NDS, Some(b"@POINT 1 0 0 0\r\n"), None));
    let mut tech = Tech::default();
    tech.add_nation(&c, &NationDef::parse(FORT_NATION));
    c.tech = tech;
    let mut w = World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default()));
    let fort = w.spawn(frt, 0, Pos::from_pixels(2048, 2048), 0);
    economy::census_all(&mut w);
    let b = fort.0 as usize;
    // The lock base cell is (128, 128).
    assert!(!w.map.land.check_pt(135, 135));
    // Burning, a finished research changes nothing (0x5326a1); the fire's life loss even
    // puts the section back to "not built".
    w.units[b].burning = true;
    w.step(&[Command::Upgrade { player: 0, buildings: vec![fort], upgrade: up(&w, "SEC0") }]);
    run(&mut w, 6);
    assert!(w.units[b].research.is_none());
    assert_ne!(section_state(w.units[b].stages, 0), section::DONE);
    assert!(!w.map.land.check_pt(135, 135));
    w.units[b].burning = false;
    w.step(&[Command::Upgrade { player: 0, buildings: vec![fort], upgrade: up(&w, "SEC0") }]);
    run(&mut w, 6);
    assert_eq!(section_state(w.units[b].stages, 0), section::DONE);
    assert_eq!(w.units[b].life, 1500);
    assert!(w.map.land.check_pt(135, 135) && w.map.land.check_pt(135, 136), "STAGELOCKPOINTS 0 locked");

    std::sync::Arc::make_mut(&mut w.map).land.set_pt(137, 137);
    let before = w.random;
    make_damage(&mut w, b, 600, None, 0xFF);
    assert_eq!(section_state(w.units[b].stages, 0), section::RUIN);
    // One BOOM per cell, lifted by 200 + 80 − 8·4 = 248: two numbers per cell, then the
    // object's own four (ASerial, RANDOM velocity).
    let booms: Vec<_> = w.projectiles.iter().map(|p| (p.pos.0 >> 14, p.pos.1 >> 14, p.pos.2 >> 14)).collect();
    assert_eq!(booms, [(2064, 2080 + 496, 248), (2096, 2112 + 496, 248), (2128, 2144 + 496, 248)]);
    let mut r = before;
    for _ in 0..3 * 6 {
        r.rando();
    }
    assert_eq!(w.random, r);
    // Then the STAGELOCKPOINTS of stage 3 (the cell count) are freed.
    assert!(!w.map.land.check_pt(137, 137));
    assert!(w.map.land.check_pt(135, 135), "the ruin's own cells stay locked");
}

const HUT: &str = "NAME Hut\r\nBUILDING\r\nLIFE 1000\r\nLOCATION 0 0 64 64\r\nBUILDSTAGES 10\r\n\
    DESTRUCT 32767 2 BOOM BOOM\r\nCHECKPOINTS 2 1 2 3 4\r\n[END]\r\n";

#[test]
fn a_destroyed_building_explodes_at_its_check_points() {
    use acr_sim::combat::make_damage;
    let mut c = Catalog::new();
    let hut = c.add(ObjectType::from_def("Hut(EN)", &UnitDef::parse(HUT.as_bytes())));
    c.set_weapons(acr_sim::weapon::Weapons::parse(RUIN_NDS, Some(b"@POINT 1 0 0 0\r\n"), None));
    let mut w = World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default()));
    let mut destroy = |setup: &dyn Fn(&mut acr_sim::world::Unit)| {
        let b = w.spawn(hut, 0, Pos::from_pixels(2048, 2048), 0).0 as usize;
        setup(&mut w.units[b]);
        let old = w.projectiles.iter().count();
        let before = w.random;
        make_damage(&mut w, b, 5000, None, 0xFF);
        assert!(w.units[b].death != 0);
        let mut drawn = 0;
        let mut r = before;
        while r != w.random {
            r.rando();
            drawn += 1;
        }
        let booms: Vec<_> =
            w.projectiles.iter().skip(old).map(|p| (p.pos.0 >> 14, p.pos.1 >> 14, p.pos.2 >> 14)).collect();
        (booms, drawn)
    };
    // A building does not turn (no random number for it, 0x4e78e1), one number for `Die`;
    // then per check point (lock base cell 128, 128) the weapon, the chance and the
    // object's own four.
    assert_eq!(destroy(&|_| {}), (vec![(2064, 2080, 16), (2096, 2112, 16)], 1 + 2 * 6));
    // Not built half way, or destroyed by its owner (`delay` 6000): no explosions.
    assert_eq!(destroy(&|u| u.stage = 4), (vec![], 1));
    assert_eq!(destroy(&|u| u.delay = 6000), (vec![], 1));
    assert_eq!(destroy(&|u| u.stage = 5).0.len(), 2);
}

#[test]
fn editor_type_levels_apply_and_undo_the_upgrades_of_one_type() {
    let (mut w, _, pik, _) = setup();
    let ups = upgrade::of_type(&w.catalog, pik);
    let names: Vec<&str> = ups.iter().map(|&u| w.catalog.tech.upgrade(u).id.as_str()).collect();
    assert_eq!(names, ["DMG", "DMG2", "TOUGH"], "fighting only: no build time, private, stage or building upgrades");
    let man = w.spawn(pik, 0, Pos::from_pixels(2048, 2048), 0).0 as usize;
    upgrade::set_type_level(&mut w, 0, pik, ups.len());
    let a = w.adv(0, pik);
    assert_eq!((a.attacks[0].damage, a.life, a.build_stages), (9, 150, 40));
    assert!(a.protection.contains(&("PIKA".into(), 5)), "{:?}", a.protection);
    assert_eq!((w.units[man].life, w.units[man].max_life), (150, 150), "the man standing there too");
    assert_eq!(w.adv(1, pik).attacks[0].damage, 7, "other players unchanged");
    assert!(!w.players[0].upgrades[usize::from(ups[0].0)].done, "nothing marked researched");
    upgrade::set_type_level(&mut w, 0, pik, 2);
    let a = w.adv(0, pik);
    assert_eq!((a.attacks[0].damage, a.life, a.build_stages), (9, 100, 40), "half: DMG and DMG2 only");
    assert_eq!((w.units[man].life, w.units[man].max_life), (100, 100));
    upgrade::set_type_level(&mut w, 0, pik, 0);
    assert_eq!(*w.adv(0, pik), *w.catalog.get(pik).adv, "back to the type's own");
}
