//! The formation-attack rule (`Rules::formation_attack`, `docs/simulation.md`, "Formation
//! attack"): a right-click attack on an enemy unit (priority 0xFE) is ignored by the members of
//! a close-combat formation in the original (`AttackObj` 0x51a930, 0x51ad61..0x51ad6c); with the
//! rule on they take it like free units.

use std::sync::Arc;

use acr_formats::defs::nres::weapon_kinds;
use acr_formats::defs::{OrderGroups, OrderList, UnitDef};
use acr_sim::catalog::{Catalog, ObjectType};
use acr_sim::formation::{Formations, Officer, OfficerFormation};
use acr_sim::map::MapGrid;
use acr_sim::world::EntityId;
use acr_sim::{Command, Pos, Rules, World};

const PIKEMAN: &str = "NAME Pik\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n\
    @MOTION_R 9 0 10 19\r\n@PATTACK 9 0 0 35\r\n@UATTACK 9 0 35 0\r\n@ATTACK 9 0 0 20\r\n\
    @DEATH 9 0 0 39\r\n#DEATHLIE1 9 1 0 39\r\nLIFE 100\r\nGEOMETRY 1 10 40\r\nDAMAGE 0 10\r\n\
    ATTACK_RADIUS 0 40 100\r\nATTACK_PAUSE 0 1\r\nSEARCH_ENEMY_RADIUS 700\r\nSEARCH_ENEMY_RADIUS_SHIFT 70\r\n\
    DET_RADIUS 0 0 7000\r\nMATHERIAL 1 BODY\r\nCANKILL 1 BODY\r\nWEAPONKIND 0 PIKA\r\nPROTECTION 1 PIKA 3\r\n\
    UNITRADIUS 8\r\n";

const NRES: &[u8] = b"[WEAPONS]\r\nPIKA 11 17 O\r\n[UNITKINDS]\r\n";

const ORDERS: &[u8] = b"#LINE15 SYM2 19 2 9 3 1 2\r\n   C   C   C\r\n      \r\n***************\r\n#END\r\n#EXIT\r\n";
const GROUPS: &[u8] = b"1\r\nLINE 1 0 1 2 #LINE15\r\n";

/// Fifteen pikemen in a LINE formation with their officer, and an enemy far outside every
/// search radius; the whole line is ordered to attack the enemy.
fn line(formation_attack: bool) -> (World, Vec<EntityId>, EntityId) {
    let mut c = Catalog::new();
    let pik = c.add(ObjectType::from_def("Pik(EN)", &UnitDef::parse(format!("{PIKEMAN}[END]\r\n").as_bytes())));
    let off =
        c.add(ObjectType::from_def("Off(EN)", &UnitDef::parse(format!("{PIKEMAN}OFFICER\r\n[END]\r\n").as_bytes())));
    let flag = c.add(ObjectType::from_def(
        "Flag(EN)",
        &UnitDef::parse(
            b"NAME Flag\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\nLIFE 100\r\nGEOMETRY 1 10 40\r\n[END]\r\n",
        ),
    ));
    c.set_weapon_kinds(&weapon_kinds(NRES));
    let mut f = Formations::new(&OrderList::parse(ORDERS), &OrderGroups::parse(GROUPS));
    let shapes = vec![f.shape_index("LINE15").unwrap()];
    f.officers.insert(
        off,
        Officer {
            drummer: None,
            flag: Some(flag),
            formations: vec![OfficerFormation { group: 0, shapes, kinds: vec![pik] }],
        },
    );
    let mut w = World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(f));
    w.rules = Rules { formation_attack, ..Rules::default() };
    let officer = w.spawn(off, 0, Pos::from_pixels(2000, 2000), 0);
    let mut men: Vec<_> =
        (0..15).map(|i| w.spawn(pik, 0, Pos::from_pixels(1900 + 20 * (i % 5), 2100 + 20 * (i / 5)), 0)).collect();
    let enemy = w.spawn(pik, 1, Pos::from_pixels(2000, 3600), 128);
    w.step(&[]);
    let shape = w.formations.shape_index("LINE15").unwrap() as u16;
    w.step(&[Command::FormNear { player: 0, officer, shape, kind: pik }]);
    assert!(w.units[officer.0 as usize].brigade.is_some(), "the line formed");
    men.push(officer);
    w.step(&[Command::Attack { player: 0, units: men.clone(), target: enemy }]);
    (w, men, enemy)
}

fn engaged(w: &World, men: &[EntityId], enemy: EntityId) -> usize {
    men.iter().filter(|&&m| w.units[m.0 as usize].target() == Some(enemy)).count()
}

#[test]
fn a_formation_stands_as_in_the_original() {
    let (mut w, men, enemy) = line(false);
    for _ in 0..20 {
        w.step(&[]);
    }
    assert_eq!(engaged(&w, &men, enemy), 0, "no member took the attack");
}

#[test]
fn a_formation_engages_with_the_rule() {
    let (mut w, men, enemy) = line(true);
    for _ in 0..20 {
        w.step(&[]);
    }
    assert!(engaged(&w, &men, enemy) > 0, "members took the attack like free units");
}

#[test]
fn the_rule_counts_in_the_state_hash_only_when_on() {
    let (off, ..) = line(false);
    let (on, ..) = line(true);
    assert_eq!(off.rules, Rules::default());
    assert_ne!(off.rules, on.rules);
    assert_ne!(off.state_hash(), on.state_hash());
}
