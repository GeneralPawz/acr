//! Combat on a synthetic flat map: melee, musket fire, death.

use std::sync::Arc;

use acr_formats::defs::UnitDef;
use acr_formats::defs::nres::weapon_kinds;
use acr_sim::catalog::{Catalog, ObjectType};
use acr_sim::formation::Formations;
use acr_sim::map::MapGrid;
use acr_sim::world::{AnimKind, EntityId};
use acr_sim::{Command, Pos, TypeId, World};

const PIKEMAN: &str = "NAME Pik\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n\
    @MOTION_R 9 0 10 19\r\n@PATTACK 9 0 0 35\r\n@UATTACK 9 0 35 0\r\n@ATTACK 9 0 0 20\r\n\
    @DEATH 9 0 0 39\r\n#DEATHLIE1 9 1 0 39\r\nLIFE 100\r\nGEOMETRY 1 10 40\r\nDAMAGE 0 10\r\n\
    ATTACK_RADIUS 0 40 100\r\nATTACK_PAUSE 0 1\r\nSEARCH_ENEMY_RADIUS 700\r\nSEARCH_ENEMY_RADIUS_SHIFT 70\r\n\
    DET_RADIUS 0 0 7000\r\nMATHERIAL 1 BODY\r\nCANKILL 1 BODY\r\nWEAPONKIND 0 PIKA\r\nPROTECTION 1 PIKA 3\r\n\
    UNITRADIUS 8\r\n[END]\r\n";

const MUSKETEER: &str = "NAME Mus\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n\
    @MOTION_R 9 0 10 19\r\n@PATTACK1 9 0 0 62\r\n@UATTACK1 9 0 62 0\r\n@ATTACK1 9 0 0 67\r\n@ATTACK 9 0 0 52\r\n\
    @ATTACK3 9 0 68 101\r\n@TRANSX3 9 0 0 35\r\n@TRANS3X 9 0 35 0\r\nSETACTIVEPOINT #ATTACK1 13 0 0\r\n\
    @DEATH 9 0 0 39\r\n#DEATHLIE1 9 1 0 39\r\nLIFE 100\r\nGEOMETRY 1 10 40\r\nDAMAGE 0 15\r\nDAMAGE 1 380\r\n\
    ATTACK_RADIUS 0 40 80\r\nDET_RADIUS 0 0 250\r\nATTACK_RADIUS 1 250 850\r\nDET_RADIUS 1 250 7000\r\n\
    ADD_ATTACK_RADIUS 0 32\r\nATTACK_PAUSE 0 20\r\nATTACK_PAUSE 1 550\r\nWEAPONKIND 0 MECH\r\nWEAPONKIND 1 VISTREL\r\n\
    WEAPON 1 SHOT\r\nDAMAGEDEC 1 300\r\nRAZBROS 300\r\nNOPAUSEDATTACK 0\r\nSLOWRECHARGE\r\nUSETRANSX 0\r\n\
    SEARCH_ENEMY_RADIUS 850\r\nSEARCH_ENEMY_RADIUS_SHIFT 70\r\nMATHERIAL 1 BODY\r\nCANKILL 1 BODY\r\n\
    PROTECTION 3 PIKA 1 VISTREL 10 MECH 1\r\nZPOINTS 33 33\r\nUNITRADIUS 8\r\nFRIENDLYFIRE\r\n[END]\r\n";

const NRES: &[u8] =
    b"[WEAPONS]\r\n#GENSHIELD 13\r\nMECH 21 19 O\r\nSTRELA 1 14 HRB\r\nPIKA 11 17 O\r\nIADRO 2 15 HX\r\n\
    VISTREL 12 18 HRB\r\n[UNITKINDS]\r\n";

fn setup() -> (World, TypeId, TypeId) {
    let mut c = Catalog::new();
    let pik = c.add(ObjectType::from_def("Pik(EN)", &UnitDef::parse(PIKEMAN.as_bytes())));
    let mus = c.add(ObjectType::from_def("Mus(EN)", &UnitDef::parse(MUSKETEER.as_bytes())));
    c.set_weapon_kinds(&weapon_kinds(NRES));
    (World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default())), pik, mus)
}

fn unit(w: &World, id: EntityId) -> &acr_sim::Unit {
    &w.units[id.0 as usize]
}

#[test]
fn pikemen_fight_to_the_death() {
    let (mut w, pik, _) = setup();
    let a = w.spawn(pik, 0, Pos::from_pixels(2000, 2000), 0);
    let b = w.spawn(pik, 1, Pos::from_pixels(2150, 2000), 128);
    let mut saw_attack = false;
    for _ in 0..1500 {
        w.step(&[]);
        saw_attack |=
            matches!(unit(&w, a).anim, AnimKind::Attack(0)) || matches!(unit(&w, b).anim, AnimKind::Attack(0));
        if unit(&w, a).death != 0 || unit(&w, b).death != 0 {
            break;
        }
    }
    assert!(saw_attack, "the pikemen found each other and struck");
    let (ua, ub) = (unit(&w, a), unit(&w, b));
    assert!(ua.death != 0 || ub.death != 0, "one of them died: lives {} {}", ua.life, ub.life);
    let (winner, loser) = if ua.death != 0 { (ub, ua) } else { (ua, ub) };
    assert_eq!(winner.kills, 1);
    // 10 damage - 3 PIKA protection = 7 per hit (+ skill bonus, none here): the winner took < 100.
    assert!(winner.life < 100 && winner.life % 7 == 2, "winner life {}", winner.life);
    assert_eq!(loser.life, 0);
    // The corpse plays #DEATH, then lies.
    let mut w2 = w.clone();
    for _ in 0..30 {
        w2.step(&[]);
    }
    let l = &w2.units[loser.id.0 as usize];
    assert!(l.alive && l.anim == AnimKind::DeathLie(1), "{:?}", l.anim);
}

#[test]
fn musketeers_fire_on_schedule_and_hit() {
    // Without dispersion the ball flies straight at the target.
    let mut c = Catalog::new();
    let pik = c.add(ObjectType::from_def("Pik(EN)", &UnitDef::parse(PIKEMAN.as_bytes())));
    let mus = c.add(ObjectType::from_def(
        "Mus(EN)",
        &UnitDef::parse(
            MUSKETEER
                .replace(
                    "RAZBROS 300
",
                    "",
                )
                .as_bytes(),
        ),
    ));
    c.set_weapon_kinds(&weapon_kinds(NRES));
    let mut w = World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default()));
    let shooter = w.spawn(mus, 0, Pos::from_pixels(2000, 2000), 0);
    let target = w.spawn(pik, 1, Pos::from_pixels(2300, 2000), 128);
    w.step(&[Command::Attack { player: 0, units: vec![shooter], target }]);
    let mut shot = None;
    for t in 1..200 {
        // Keep the pikeman in place so only the musket can hurt it.
        w.step(&[Command::Stop { player: 1, units: vec![target] }]);
        if unit(&w, shooter).delay > 0 {
            shot = Some(t);
            break;
        }
    }
    let t = shot.expect("the musketeer fired");
    // #PATTACK1 (63 frames = 32 ticks), then 7 ticks of #ATTACK1 to its active frame 14.
    assert!((38..=45).contains(&t), "shot at tick {t}");
    assert_eq!(unit(&w, shooter).delay, 550);
    // 380 halved at 300 px = 190 against 100 life and no VISTREL protection: one ball kills.
    let p = unit(&w, target);
    assert!(p.death != 0 && p.life == 0, "pikeman life {}", p.life);
    assert_eq!(unit(&w, shooter).kills, 1);
}

#[test]
fn combat_is_deterministic() {
    let run = || {
        let (mut w, pik, mus) = setup();
        for k in 0..10 {
            w.spawn(if k % 2 == 0 { pik } else { mus }, 0, Pos::from_pixels(1800 + 30 * k, 2000), 0);
            w.spawn(if k % 2 == 0 { mus } else { pik }, 1, Pos::from_pixels(1800 + 30 * k, 2400), 192);
        }
        for _ in 0..800 {
            w.step(&[]);
        }
        (w.state_hash(), w.units.iter().filter(|u| u.death != 0 || !u.alive).count())
    };
    let (h1, dead) = run();
    assert_eq!(h1, run().0);
    assert!(dead > 0, "somebody died");
}

#[test]
fn queries_kill_and_remove() {
    let (mut w, pik, _) = setup();
    let a = w.spawn(pik, 0, Pos::from_pixels(1000, 1000), 0);
    let b = w.spawn(pik, 1, Pos::from_pixels(5000, 5000), 0);
    let c = w.spawn(pik, 1, Pos::from_pixels(5100, 5000), 0);
    assert_eq!(w.units_of(1).count(), 2);
    assert_eq!(w.units_in_circle(5000, 5000, 120).map(|u| u.id).collect::<Vec<_>>(), [b, c]);
    w.kill(b);
    assert!(w.unit(b).is_some_and(|u| u.death != 0), "a corpse remains");
    w.remove(c);
    assert!(w.unit(c).is_none());
    assert_eq!(w.units_of(1).count(), 0);
    assert_eq!(w.living().map(|u| u.id).collect::<Vec<_>>(), [a]);
}

#[test]
fn allies_do_not_fight() {
    let (mut w, pik, _) = setup();
    // Nations 0 and 1 are friends (one-sided masks are enough when both share a bit).
    w.nation_mask[0] = 0b11;
    w.nation_mask[1] = 0b11;
    let a = w.spawn(pik, 0, Pos::from_pixels(2000, 2000), 0);
    let b = w.spawn(pik, 1, Pos::from_pixels(2150, 2000), 128);
    for _ in 0..300 {
        w.step(&[]);
    }
    assert!(unit(&w, a).target().is_none() && unit(&w, b).target().is_none());
    assert_eq!((unit(&w, a).life, unit(&w, b).life), (100, 100));
    // An explicit order on an ally is refused, too.
    w.step(&[Command::Attack { player: 0, units: vec![a], target: b }]);
    assert!(unit(&w, a).target().is_none());
    // Breaking the alliance of one unit makes the two enemies again.
    w.units[b.0 as usize].nmask = 0b100;
    for _ in 0..300 {
        w.step(&[]);
    }
    assert!(unit(&w, a).life < 100 || unit(&w, b).life < 100);
}

#[test]
fn corpses_keep_their_direction_and_disappear() {
    let (mut w, pik, _) = setup();
    let a = w.spawn(pik, 0, Pos::from_pixels(1000, 1000), 40);
    w.kill(a);
    let dir = unit(&w, a).dir;
    let mut lying = None;
    let mut gone = None;
    for t in 1..=2000 {
        w.step(&[]);
        let Some(u) = w.unit(a) else {
            gone = Some(t);
            break;
        };
        assert_eq!(u.dir, dir, "a corpse never turns");
        if lying.is_none() && u.anim == AnimKind::DeathLie(1) {
            lying = Some((t, u.death));
        }
    }
    // #DEATH (40 frames, 2 per tick) counts 1 per tick, then lying counts 2 per tick; a type
    // without #DEATHLIE2 is removed once the counter passes 1200 (0x5120ab).
    let (t0, d0) = lying.expect("the corpse lies");
    assert!((19..=21).contains(&t0), "lies after the death animation: {t0}");
    let gone = gone.expect("the corpse is removed");
    assert_eq!(gone, t0 + (1201 - i32::from(d0) + 1) as usize / 2, "removed at {gone}");
}

/// A shooter turns to its target before it fires and keeps facing it through the aiming and
/// firing animations (`AttackObjLink`: `ROTATE` steps until within 1.5·`ROTATE`, then
/// `RealDir = GetDir(target)`; the strike frame sets it again, 0x51d7e9). The drawing picks
/// the sprite from that direction for every animation alike (0x5062a3).
#[test]
fn shooters_face_their_target() {
    let (mut w, pik, mus) = setup();
    let m = w.spawn(mus, 0, Pos::from_pixels(2000, 2000), 64);
    // A target to the west, out of melee reach: the musketeer aims and fires facing west.
    let p = w.spawn(pik, 1, Pos::from_pixels(1400, 2000), 0);
    w.units[p.0 as usize].stand_ground = true;
    w.step(&[Command::Attack { player: 0, units: vec![m], target: p }]);
    let (mut aims, mut fired) = (0, false);
    for _ in 0..400 {
        w.step(&[]);
        let u = unit(&w, m);
        if matches!(u.anim, AnimKind::Attack(_) | AnimKind::PAttack(_)) {
            let (ex, ey) = unit(&w, p).pos.pixels();
            let (ux, uy) = u.pos.pixels();
            let want = acr_sim::trig::get_dir(ex - ux, ey - uy);
            assert!((u.dir.wrapping_sub(want) as i8).unsigned_abs() <= 4, "faces {} for {want} in {:?}", u.dir, u.anim);
            aims += 1;
            fired |= u.anim == AnimKind::Attack(1);
        }
    }
    assert!(aims > 0 && fired, "the musketeer aimed and fired");
}

/// Deaths keep their cause for the renderer: a pike thrust is melee from the killer's
/// position, a script kill is unknown; the record is not part of the state hash.
#[test]
fn deaths_keep_their_cause() {
    use acr_sim::combat::{DeathCause, DeathRecord};
    let (mut w, pik, _) = setup();
    let a = w.spawn(pik, 0, Pos::from_pixels(2000, 2000), 0);
    let b = w.spawn(pik, 1, Pos::from_pixels(2150, 2000), 128);
    let k = w.spawn(pik, 0, Pos::from_pixels(3000, 3000), 0);
    w.kill(k);
    for _ in 0..1500 {
        w.step(&[]);
        if unit(&w, a).death != 0 || unit(&w, b).death != 0 {
            break;
        }
    }
    let (winner, loser) = if unit(&w, a).death != 0 { (b, a) } else { (a, b) };
    let record = w.deaths[&loser.0];
    assert_eq!(record.cause, DeathCause::Melee);
    let (wx, wy) = unit(&w, winner).pos.pixels();
    let (fx, fy) = record.from.expect("the killer's position");
    assert!((fx - wx).abs() < 64 && (fy - wy).abs() < 64, "from {fx},{fy}, killer at {wx},{wy}");
    assert_eq!(w.deaths[&k.0], DeathRecord::default());
    assert!(!w.deaths.contains_key(&winner.0));
    let hash = w.state_hash();
    w.deaths.clear();
    assert_eq!(w.state_hash(), hash, "causes are not part of the state");
}

#[test]
fn weapon_kinds_map_to_causes() {
    use acr_sim::combat::DeathCause;
    for (kind, cause) in [
        ("MECH", DeathCause::Melee),
        ("PIKA", DeathCause::Melee),
        ("VISTREL2", DeathCause::Gunshot),
        ("STRELAOTRAVLENIE", DeathCause::Arrow),
        ("OSTRELA", DeathCause::Arrow),
        ("TAMAGAVK", DeathCause::Thrown),
        ("IADRO", DeathCause::Cannonball),
        ("KARTECH", DeathCause::Grapeshot),
        ("IADROMOR", DeathCause::Explosion),
        ("MEDIK", DeathCause::Unknown),
    ] {
        assert_eq!(DeathCause::of_weapon(kind), cause, "{kind}");
    }
}

#[test]
fn a_shooter_pressed_against_its_target_backs_away_and_fights() {
    let (mut w, pik, mus) = setup();
    let m = w.spawn(mus, 0, Pos::from_pixels(2000, 2000), 64);
    // A passive enemy 20 px away: inside both the bayonet's (40) and the musket's (250)
    // minimum range. The musket is still reloading, so the bayonet is the attack to use.
    let p = w.spawn(pik, 1, Pos::from_pixels(2020, 2000), 0);
    let e = &mut w.units[p.0 as usize];
    (e.stand_ground, e.no_search_victim) = (true, true);
    w.units[m.0 as usize].delay = 550;
    w.step(&[Command::Attack { player: 0, units: vec![m], target: p }]);
    let dist = |w: &World| {
        let ((ax, ay), (bx, by)) = (unit(w, m).pos.pixels(), unit(w, p).pos.pixels());
        acr_sim::trig::norma(ax - bx, ay - by)
    };
    let (mut waited, mut struck) = (0, false);
    for tick in 0..400 {
        w.step(&[]);
        let u = unit(&w, m);
        if tick < 30 {
            // 0x5206a2: first it stands for 32 ticks.
            assert_eq!(u.pos.pixels(), (2000, 2000), "stands at tick {tick}");
            waited += 1;
        }
        struck |= u.anim == AnimKind::Attack(0);
    }
    assert_eq!(waited, 30);
    assert!(dist(&w) >= 40 || struck, "backed away ({} px) or fought", dist(&w));
    assert!(struck, "the musketeer used its bayonet once it had room");
}

#[test]
fn a_walker_that_stops_at_an_enemy_keeps_its_cell() {
    let (mut w, pik, _) = setup();
    let a = w.spawn(pik, 0, Pos::from_pixels(2000, 2000), 0);
    let e = w.spawn(pik, 1, Pos::from_pixels(2200, 2000), 0);
    let en = &mut w.units[e.0 as usize];
    (en.stand_ground, en.no_search_victim) = (true, true);
    w.units[a.0 as usize].no_search_victim = true;
    // Walk past the enemy: the walker meets it on the way and stops to fight (0x41df98).
    w.step(&[Command::Move { player: 0, units: vec![a], x: 2600, y: 2000 }]);
    for _ in 0..200 {
        w.step(&[]);
        let u = unit(&w, a);
        if matches!(u.orders.last(), Some(acr_sim::world::Order::Attack { .. })) && !u.in_motion {
            // Deviation (#195): its cell stays reserved, so friends can't walk into it.
            assert!(w.occupancy.check_pt(u.cell.0, u.cell.1), "cell {:?} reserved", u.cell);
            return;
        }
    }
    panic!("the walker never stopped at the enemy");
}
