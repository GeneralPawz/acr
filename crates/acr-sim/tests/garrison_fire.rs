//! Shooting at a garrisoned building (`AttackObjLink` 0x51c232, 0x51f12e; `B` weapons
//! 0x4e8870): musketeers attack a `CANBEKILLEDINSIDE` house while someone is inside, their
//! balls kill the men inside one by one, the house itself takes no damage.

use std::sync::Arc;

use acr_formats::defs::UnitDef;
use acr_formats::defs::nres::weapon_kinds;
use acr_sim::catalog::{Catalog, ObjectType};
use acr_sim::formation::Formations;
use acr_sim::map::MapGrid;
use acr_sim::weapon::Weapons;
use acr_sim::world::EntityId;
use acr_sim::{Command, Pos, TypeId, World};

const MUSKETEER: &str = "NAME Mus\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n\
    @MOTION_R 9 0 10 19\r\n@PATTACK1 9 0 0 62\r\n@UATTACK1 9 0 62 0\r\n@ATTACK1 9 0 0 67\r\n@ATTACK 9 0 0 52\r\n\
    @ATTACK3 9 0 68 101\r\n@TRANSX3 9 0 0 35\r\n@TRANS3X 9 0 35 0\r\nSETACTIVEPOINT #ATTACK1 13 0 0\r\n\
    @DEATH 9 0 0 39\r\n#DEATHLIE1 9 1 0 39\r\nLIFE 100\r\nGEOMETRY 1 10 40\r\nDAMAGE 0 15\r\nDAMAGE 1 380\r\n\
    ATTACK_RADIUS 0 40 80\r\nDET_RADIUS 0 0 250\r\nATTACK_RADIUS 1 250 1100\r\nDET_RADIUS 1 250 7000\r\n\
    ADD_ATTACK_RADIUS 0 32\r\nATTACK_PAUSE 0 20\r\nATTACK_PAUSE 1 550\r\nWEAPONKIND 0 MECH\r\nWEAPONKIND 1 VISTREL\r\n\
    WEAPON 1 SHOTFOG\r\nDAMAGEDEC 1 300\r\nRAZBROS 300\r\nNOPAUSEDATTACK 0\r\nSLOWRECHARGE\r\nUSETRANSX 0\r\n\
    SEARCH_ENEMY_RADIUS 1100\r\nSEARCH_ENEMY_RADIUS_SHIFT 70\r\nMATHERIAL 1 BODY\r\nCANKILL 1 BODY\r\n\
    ZPOINTS 33 33\r\nUNITRADIUS 8\r\nUSAGE STRELOK\r\nPROMAXCTG 1\r\n[END]\r\n";
/// `ENGDO1.MD`'s garrison lines, without its shot points (no return fire here).
const HOUSE: &str = "NAME Hou\r\nBUILDING\r\nLIFE 5000\r\nLOCATION 0 0 64 64\r\nBUILDSTAGES 10\r\n\
    LOCKPOINTS 4 0 0 1 0 0 1 1 1\r\n3DBARS 1 0 0 100 100 90\r\nUNITABSORBER 5\r\nCANBEKILLEDINSIDE\r\n\
    OPPOSIT 1 10\r\nPROMAX 98\r\nPROMAX2 1 50\r\nMATHERIAL 1 BUILDING\r\n[END]\r\n";
const MUSKET_NDS: &[u8] = b"[MEMBERS]\r\nSHOTFOG #SHOTFOG 0 0 NONE NO_GRAVITY STAND 1 1\r\n\
    ENDSHOT #POINT 0 10 NONE NO_GRAVITY FLY 1500 -1\r\n[CHILDWEAPON]\r\nSHOTFOG 1 1 255 1 ENDSHOT\r\n\
    [FULLPARENT]\r\nENDSHOT\r\nSHOTFOG\r\n[END]\r\n";
const NRES: &[u8] = b"[WEAPONS]\r\nMECH 21 19 O\r\nVISTREL 12 18 HRB\r\n[UNITKINDS]\r\n";

struct Kinds {
    mus: TypeId,
    house: TypeId,
    shed: TypeId,
}

fn setup() -> (World, Kinds) {
    let mut c = Catalog::new();
    let def = |c: &mut Catalog, id: &str, text: &str| c.add(ObjectType::from_def(id, &UnitDef::parse(text.as_bytes())));
    let mus = def(&mut c, "Mus(EN)", MUSKETEER);
    let house = def(&mut c, "Hou(EN)", HOUSE);
    let shed = def(&mut c, "She(EN)", &HOUSE.replace("CANBEKILLEDINSIDE\r\n", ""));
    c.set_weapon_kinds(&weapon_kinds(NRES));
    c.set_weapons(Weapons::parse(MUSKET_NDS, Some(b"#SHOTFOG 1 8 0 0 0 1 0 2 0 3 0 4 0 5 0 6 0 7\r\n"), None));
    (World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default())), Kinds { mus, house, shed })
}

/// A house of player 1 at (2048, 2048) with `n` musketeers inside.
fn garrisoned(w: &mut World, kind: TypeId, n: usize) -> (EntityId, Vec<EntityId>) {
    let b = w.spawn(kind, 1, Pos::from_pixels(2048, 2048), 0);
    let mus = w.catalog.find("Mus(EN)").unwrap();
    let men = (0..n)
        .map(|k| {
            let s = w.spawn(mus, 1, Pos::from_pixels(2000 + 10 * k as i32, 2048), 0);
            let su = &mut w.units[s.0 as usize];
            su.inside_of = Some(b);
            let cell = su.cell;
            w.occupancy.clr_bar(cell.0, cell.1, 1);
            w.units[b.0 as usize].inside.push(s);
            s
        })
        .collect();
    (b, men)
}

/// Five musketeers of player 0 in a row 700 px south of the house.
fn line(w: &mut World, k: &Kinds) -> Vec<EntityId> {
    (0..5).map(|n| w.spawn(k.mus, 0, Pos::from_pixels(1950 + 40 * n, 2750), 192)).collect()
}

fn inside(w: &World, b: EntityId) -> usize {
    w.units[b.0 as usize].inside.len()
}

fn shoot(ticks: usize) -> (World, EntityId, Vec<usize>) {
    let (mut w, k) = setup();
    let (b, _) = garrisoned(&mut w, k.house, 5);
    let ms = line(&mut w, &k);
    w.step(&[Command::Attack { player: 0, units: ms, target: b }]);
    let mut counts = vec![inside(&w, b)];
    for t in 0..ticks {
        w.step(&[]);
        if t % 100 == 99 {
            counts.push(inside(&w, b));
        }
    }
    (w, b, counts)
}

#[test]
fn musketeers_shoot_the_garrison_out_of_a_house() {
    let (w, b, counts) = shoot(3000);
    eprintln!("men inside every 100 ticks: {counts:?}");
    assert_eq!(counts[0], 5);
    assert!(counts.windows(2).all(|c| c[1] <= c[0]), "{counts:?}");
    assert_eq!(*counts.last().unwrap(), 0, "the garrison falls: {counts:?}");
    let house = &w.units[b.0 as usize];
    assert!(house.alive && house.life == 5000, "the house itself takes no damage from muskets");
    // Without anyone inside the house is no target any more (0x51f3df): the attack ends.
    let mus = w.catalog.find("Mus(EN)").unwrap();
    assert!(w.units.iter().filter(|u| u.owner == 0 && u.kind == mus).all(|u| u.target() != Some(b)));
}

#[test]
fn the_garrison_fire_is_deterministic() {
    let (a, _, ca) = shoot(1500);
    let (b, _, cb) = shoot(1500);
    assert_eq!(ca, cb);
    assert_eq!(a.state_hash(), b.state_hash());
}

#[test]
fn only_garrisoned_houses_that_can_be_killed_inside_are_attacked() {
    let (mut w, k) = setup();
    // No CANBEKILLEDINSIDE: the musketeers' BODY mask does not fit a building.
    let (shed, _) = garrisoned(&mut w, k.shed, 3);
    let ms = line(&mut w, &k);
    w.step(&[Command::Attack { player: 0, units: ms.clone(), target: shed }]);
    assert!(ms.iter().all(|m| w.units[m.0 as usize].target() != Some(shed)));
    // An empty house: nothing to shoot at.
    let (mut w, k) = setup();
    let (house, _) = garrisoned(&mut w, k.house, 0);
    let ms = line(&mut w, &k);
    w.step(&[Command::Attack { player: 0, units: ms.clone(), target: house }]);
    assert!(ms.iter().all(|m| w.units[m.0 as usize].target() != Some(house)));
}
