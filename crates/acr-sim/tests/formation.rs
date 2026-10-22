//! Player formations: forming from the units near an officer (`CreateBrigade`), re-forming,
//! filling and the stand-ground bonuses.

use std::sync::Arc;

use acr_formats::defs::{OrderGroups, OrderList, UnitDef};
use acr_sim::catalog::{Catalog, ObjectType};
use acr_sim::formation::{Formations, Officer, OfficerFormation, count_near};
use acr_sim::map::MapGrid;
use acr_sim::world::EntityId;
use acr_sim::{Command, Pos, TypeId, World};

fn def(name: &str, extra: &str) -> UnitDef {
    UnitDef::parse(
        format!(
            "NAME {name}\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n\
             @MOTION_R 9 0 10 19\r\nLIFE 100\r\nGEOMETRY 1 10 40\r\n{extra}[END]\r\n"
        )
        .as_bytes(),
    )
}

const ORDERS: &[u8] = b"#LINE15 SYM2 19 2 9 3 1 2\r\n   C   C   C\r\n      \r\n***************\r\n#END\r\n\
#LINE36 SYM2 9 2 9 3 1 2\r\n   C   C   C\r\n\r\n******************\r\n******************\r\n#END\r\n\
#KARE15 SYM0 9 2 9 3\r\n*****\r\n* C *\r\n** C*\r\n*****\r\n#END\r\n#EXIT\r\n";
const GROUPS: &[u8] = b"2\r\nLINE 2 0 1 2 #LINE15 #LINE36\r\nKARE 2 0 1 1 #KARE15\r\n";

struct Setup {
    w: World,
    pik: TypeId,
    off: TypeId,
    drum: TypeId,
    flag: TypeId,
}

fn setup(needs_drummer: bool) -> Setup {
    let mut c = Catalog::new();
    let pik = c.add(ObjectType::from_def("Pik(EN)", &def("Pik", "")));
    let off = c.add(ObjectType::from_def("Off(EN)", &def("Off", "OFFICER\r\n")));
    let drum = c.add(ObjectType::from_def("Drum(EN)", &def("Drum", "BARABAN\r\n")));
    let flag = c.add(ObjectType::from_def("Flag(EN)", &def("Flag", "")));
    let mut f = Formations::new(&OrderList::parse(ORDERS), &OrderGroups::parse(GROUPS));
    let line = vec![f.shape_index("LINE15").unwrap(), f.shape_index("LINE36").unwrap()];
    f.officers.insert(
        off,
        Officer {
            drummer: needs_drummer.then_some(drum),
            flag: Some(flag),
            formations: vec![OfficerFormation { group: 0, shapes: line, kinds: vec![pik] }],
        },
    );
    Setup { w: World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(f)), pik, off, drum, flag }
}

fn run(w: &mut World, ticks: usize) {
    for _ in 0..ticks {
        w.step(&[]);
    }
}

fn soldiers(s: &mut Setup, n: i32, x: i32, y: i32) -> Vec<EntityId> {
    (0..n).map(|i| s.w.spawn(s.pik, 0, Pos::from_pixels(x + 20 * (i % 5), y + 20 * (i / 5)), 0)).collect()
}

fn form(s: &mut Setup, officer: EntityId, shape: &str) {
    let shape = s.w.formations.shape_index(shape).unwrap() as u16;
    s.w.step(&[Command::FormNear { player: 0, officer, shape, kind: s.pik }]);
}

#[test]
fn forming_takes_exactly_the_shape_size_near_the_officer() {
    let mut s = setup(true);
    let officer = s.w.spawn(s.off, 0, Pos::from_pixels(2000, 2000), 0);
    soldiers(&mut s, 14, 1900, 2100);
    // Too far: 1100 px away.
    s.w.spawn(s.pik, 0, Pos::from_pixels(3100, 2000), 0);
    s.w.spawn(s.drum, 0, Pos::from_pixels(2050, 2000), 0);
    run(&mut s.w, 1);
    form(&mut s, officer, "LINE15");
    assert!(s.w.brigades.is_empty(), "14 soldiers within 1024 px are not 15");
    assert_eq!(count_near(&s.w, 2000 << 4, 2000 << 4, s.pik, 0), 14);
    s.w.spawn(s.pik, 0, Pos::from_pixels(2000, 2900), 0);
    soldiers(&mut s, 5, 1500, 1500);
    run(&mut s.w, 1);
    form(&mut s, officer, "LINE15");
    assert_eq!(s.w.brigades.len(), 1);
    let b = &s.w.brigades[0];
    assert_eq!(b.members.iter().skip(3).flatten().count(), 15);
    assert!(b.members[1].is_some(), "the drummer is taken");
    assert!(b.members[2].is_none(), "no flag bearer near");
    // The normal bonuses of #LINE15 (19 2 9 3 1 2): attack 2, defence 3.
    assert_eq!(b.bonus, (2, 3));
    let officer_unit = s.w.unit(officer).unwrap();
    assert!(officer_unit.stand_ground && officer_unit.brigade == Some(0));
    for &m in b.members.iter().skip(3).flatten() {
        let u = s.w.unit(m).unwrap();
        assert_eq!((u.attack_bonus, u.defence_bonus, u.stand_ground), (2, 3, false));
    }
    // Free units left: the five beyond 15 and the far one.
    assert_eq!(count_near(&s.w, 2000 << 4, 2000 << 4, s.pik, 0), 5);
}

#[test]
fn forming_needs_a_drummer_unless_the_officer_has_none() {
    let mut s = setup(true);
    let officer = s.w.spawn(s.off, 0, Pos::from_pixels(2000, 2000), 0);
    soldiers(&mut s, 15, 1900, 2100);
    run(&mut s.w, 1);
    form(&mut s, officer, "LINE15");
    assert!(s.w.brigades.is_empty());

    let mut s = setup(false);
    let officer = s.w.spawn(s.off, 0, Pos::from_pixels(2000, 2000), 0);
    soldiers(&mut s, 15, 1900, 2100);
    let flag = s.w.spawn(s.flag, 0, Pos::from_pixels(1950, 2000), 0);
    run(&mut s.w, 1);
    form(&mut s, officer, "LINE15");
    let b = &s.w.brigades[0];
    assert_eq!(b.members[2], Some(flag));
    // With the flag bearer: + 1 attack, + 2 defence.
    assert_eq!(b.bonus, (3, 5));
}

#[test]
fn stand_ground_bonus_after_standing_still() {
    let mut s = setup(false);
    let officer = s.w.spawn(s.off, 0, Pos::from_pixels(2000, 2000), 0);
    let ids = soldiers(&mut s, 15, 1900, 2100);
    run(&mut s.w, 1);
    form(&mut s, officer, "LINE15");
    run(&mut s.w, 300);
    s.w.step(&[Command::StandGround { player: 0, units: vec![ids[0]], on: true }]);
    let b = &s.w.brigades[0];
    assert_eq!((b.bonus, b.stand_timer), ((2, 3), 50));
    assert!(ids.iter().all(|&id| s.w.unit(id).unwrap().stand_ground));
    // 50 / FrmDec = 25 city ticks of the nation, one in 8 ticks.
    run(&mut s.w, 25 * 8);
    let b = &s.w.brigades[0];
    assert_eq!((b.bonus, b.stand_timer), ((19, 9), 0), "the standing bonuses (n1, n3)");
    let u = s.w.unit(ids[3]).unwrap();
    assert_eq!((u.attack_bonus, u.defence_bonus), (19, 9));
    // A move ends it, with the normal bonuses.
    s.w.step(&[Command::Move { player: 0, units: vec![officer], x: 2600, y: 2000 }]);
    let b = &s.w.brigades[0];
    assert_eq!((b.bonus, b.stand_timer), ((2, 3), 0));
    assert!(ids.iter().all(|&id| !s.w.unit(id).unwrap().stand_ground));
}

#[test]
fn reform_and_fill() {
    let mut s = setup(false);
    let officer = s.w.spawn(s.off, 0, Pos::from_pixels(2000, 2000), 0);
    soldiers(&mut s, 15, 1900, 2100);
    run(&mut s.w, 1);
    form(&mut s, officer, "LINE15");
    run(&mut s.w, 200);
    // KARE (group 1): the 15 soldiers fit #KARE15.
    s.w.step(&[Command::Reform { player: 0, brigade: 0, group: 1 }]);
    let b = &s.w.brigades[0];
    assert_eq!(s.w.formations.shapes[b.shape].name, "#KARE15");
    assert_eq!(b.members.iter().skip(3).flatten().count(), 15);
    // Back to a line: the first variant with room is #LINE15.
    s.w.step(&[Command::Reform { player: 0, brigade: 0, group: 0 }]);
    assert_eq!(s.w.formations.shapes[s.w.brigades[0].shape].name, "#LINE15");
    // Two soldiers fall; filling takes free soldiers of the type near the formation.
    let dead: Vec<EntityId> = s.w.brigades[0].members.iter().skip(3).take(2).flatten().copied().collect();
    for d in &dead {
        s.w.units[d.0 as usize].alive = false;
    }
    let fresh = soldiers(&mut s, 3, 2100, 1800);
    run(&mut s.w, 1);
    s.w.step(&[Command::Fill { player: 0, brigade: 0 }]);
    let b = &s.w.brigades[0];
    let members: Vec<EntityId> = b.members.iter().skip(3).flatten().copied().collect();
    assert_eq!(members.len(), 15);
    assert_eq!(fresh.iter().filter(|f| members.contains(f)).count(), 2);
    assert!(dead.iter().all(|d| !members.contains(d)));
    // Another player's brigade cannot be filled.
    s.w.step(&[Command::Fill { player: 1, brigade: 0 }]);
    assert_eq!(s.w.unit(fresh[2]).unwrap().brigade, None);
}
