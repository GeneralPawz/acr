//! Melee at men on the ground and critical hits (`docs/melee.md`, rule `hit_locations`):
//! crawlers are finished off after the standing threats, riders' lances thrust down, vital
//! hits spread by their tables, a femoral artery bleeds out in 20–40 s.

use std::sync::Arc;

use acr_formats::defs::UnitDef;
use acr_formats::defs::nres::weapon_kinds;
use acr_sim::body::{Body, Cause, KNEE, LimbId, Part, injure};
use acr_sim::catalog::{Catalog, ObjectType};
use acr_sim::combat::{DeathCause, StrikeHeight, StrikeKind, Vital};
use acr_sim::formation::Formations;
use acr_sim::map::MapGrid;
use acr_sim::melee::{FINISH_REACH, apply_vital, critical, on_ground};
use acr_sim::wounds::HitRegion;
use acr_sim::{Pos, Rules, TypeId, World};

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

const ON: Rules = Rules {
    hit_locations: true,
    body_spacing: false,
    mortars: false,
    corpse_piles: false,
    group_move: false,
    cannon_fire: false,
};

/// Pikemen and a lancer (a rider with `MECH`, named as a lancer).
fn setup(rules: Rules) -> (World, TypeId, TypeId) {
    let mut c = Catalog::new();
    let pik = c.add(ObjectType::from_def("Pik(EN)", &UnitDef::parse(PIKEMAN.as_bytes())));
    let lancer = PIKEMAN
        .replace("NAME Pik", "NAME SpLancer")
        .replace("WEAPONKIND 0 PIKA", "WEAPONKIND 0 MECH\r\nUSAGE FASTHORSE");
    let lan = c.add(ObjectType::from_def("SpLancer(SP)", &UnitDef::parse(lancer.as_bytes())));
    c.set_weapon_kinds(&weapon_kinds(NRES));
    let mut w = World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default()));
    w.rules = rules;
    (w, pik, lan)
}

/// A man whose legs gave out: he crawls on his arms.
fn crawler() -> Body {
    let mut b = Body::default();
    b.limbs[LimbId::LegL as usize].lost = KNEE - 20;
    b
}

/// One attacker of type `kind` 100 px from an enemy crawler, for `ticks` (not closer: the
/// search skips enemies within the minimum range of its centre, 70 px ahead).
fn finish(kind: fn(TypeId, TypeId) -> TypeId, ticks: usize) -> (World, usize) {
    let (mut w, pik, lan) = setup(ON);
    w.spawn(kind(pik, lan), 0, Pos::from_pixels(2000, 2000), 0);
    let c = w.spawn(pik, 1, Pos::from_pixels(2100, 2000), 128).0 as usize;
    w.units[c].body = crawler();
    for _ in 0..ticks {
        w.step(&[]);
    }
    (w, c)
}

#[test]
fn a_crawler_gets_finished_off() {
    let (w, c) = finish(|pik, _| pik, 600);
    assert!(!w.units[c].living(), "the crawler is dead");
    let hits: Vec<_> = w.hits.iter().filter(|h| h.victim == c as u32).collect();
    assert!(!hits.is_empty());
    for h in &hits {
        let s = h.strike.expect("every hit under the rule has a strike");
        assert_eq!((s.kind, s.height, s.mounted), (StrikeKind::Thrust, StrikeHeight::Down, false), "{h:?}");
    }
}

#[test]
fn a_lancer_thrusts_down() {
    let (w, c) = finish(|_, lan| lan, 600);
    let hits: Vec<_> = w.hits.iter().filter(|h| h.victim == c as u32).collect();
    assert!(!hits.is_empty(), "the lancer strikes the crawler");
    let s = hits[0].strike.expect("a strike");
    assert_eq!((s.kind, s.height, s.mounted), (StrikeKind::Thrust, StrikeHeight::Down, true));
}

/// Standing threats first: with an enemy on his feet and a crawler nearer, the first blow
/// goes to the man standing.
#[test]
fn standing_threats_come_first() {
    let (mut w, pik, _) = setup(ON);
    w.spawn(pik, 0, Pos::from_pixels(2000, 2000), 0);
    let c = w.spawn(pik, 1, Pos::from_pixels(2100, 1990), 128).0 as usize;
    w.units[c].body = crawler();
    let s = w.spawn(pik, 1, Pos::from_pixels(2140, 2010), 128).0 as usize;
    for _ in 0..400 {
        w.step(&[]);
        if !w.hits.is_empty() {
            break;
        }
    }
    let first = w.hits.iter().find(|h| h.victim == c as u32 || h.victim == s as u32).expect("a blow");
    assert_eq!(first.victim, s as u32, "the standing enemy first");
}

/// Thrusts at a man on the ground find a vital in the chest about two thirds of the time
/// (heart 24, lung 36, spine 6 in the table); on his feet a third of that.
#[test]
fn critical_rates_within_their_ranges() {
    let rate = |down: bool, part_ok: fn(Part) -> bool, want: fn(Option<Vital>) -> bool| {
        let (mut w, pik, _) = setup(ON);
        let v = w.spawn(pik, 1, Pos::from_pixels(2000, 2000), 0).0 as usize;
        let strike = acr_sim::combat::Strike {
            kind: StrikeKind::Thrust,
            height: if down { StrikeHeight::Down } else { StrikeHeight::Mid },
            mounted: false,
            finishing: false,
        };
        let (mut n, mut hit) = (0, 0);
        while n < 800 {
            w.units[v].body = Body::default();
            let mut injury = injure(&mut w, v, Cause::Bayonet, 100, HitRegion::Torso);
            if !part_ok(injury.part) {
                continue;
            }
            n += 1;
            if want(critical(&mut w, v, &mut injury, strike, down, 0)) {
                hit += 1;
            }
        }
        hit * 100 / n
    };
    let chest = |p| p == Part::Chest;
    let any = rate(true, chest, |v| v.is_some());
    assert!((56..=76).contains(&any), "chest, down: {any}");
    let any = rate(false, chest, |v| v.is_some());
    assert!((25..=41).contains(&any), "chest, standing: {any}");
    let heart = rate(true, chest, |v| v == Some(Vital::Heart));
    assert!((17..=31).contains(&heart), "heart: {heart}");
    let liver = rate(false, |p| p == Part::Belly, |v| v == Some(Vital::Liver));
    assert!((14..=26).contains(&liver), "liver: {liver}");
}

#[test]
fn a_femoral_artery_bleeds_out_in_20_to_40_seconds() {
    for roll in [0, 37, 99] {
        let (mut w, pik, _) = setup(ON);
        let v = w.spawn(pik, 1, Pos::from_pixels(2000, 2000), 0).0 as usize;
        let mut injury = injure(&mut w, v, Cause::Bayonet, 100, HitRegion::Legs);
        w.units[v].body = Body::default();
        injury.level = 40;
        apply_vital(&mut w.units[v].body, &mut injury, Vital::Femoral, roll);
        let start = w.tick.0;
        let mut ticks = 0;
        while w.units[v].living() && ticks < 1000 {
            w.step(&[]);
            ticks += 1;
        }
        let secs = (w.tick.0 - start) / 10;
        assert!((19..=41).contains(&secs), "roll {roll}: {secs} s");
        assert_eq!(w.deaths.get(&(v as u32)).map(|d| d.cause), Some(DeathCause::BledOut));
    }
}

/// A loaded musketeer over a crawler shoots him point-blank (a shot, down); then, reloading,
/// he uses the bayonet.
#[test]
fn a_loaded_musketeer_shoots_down_then_stabs() {
    let mut c = Catalog::new();
    let mus = c.add(ObjectType::from_def("Mus(EN)", &UnitDef::parse(MUSKETEER.as_bytes())));
    let pik =
        c.add(ObjectType::from_def("Pik(EN)", &UnitDef::parse(PIKEMAN.replace("LIFE 100", "LIFE 2000").as_bytes())));
    c.set_weapon_kinds(&weapon_kinds(NRES));
    let mut w = World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default()));
    w.rules = ON;
    w.spawn(mus, 0, Pos::from_pixels(2000, 2000), 0);
    let v = w.spawn(pik, 1, Pos::from_pixels(2100, 2000), 128).0 as usize;
    w.units[v].body = Body { mounted: false, ..Body::default() };
    w.units[v].body.limbs[LimbId::LegL as usize].fracture =
        Some(acr_sim::body::Fracture { level: 60, kind: acr_sim::body::FractureKind::Closed, bend: 0 });
    for _ in 0..400 {
        w.step(&[]);
    }
    let kinds: Vec<_> = w.hits.iter().filter(|h| h.victim == v as u32).filter_map(|h| h.strike).collect();
    assert!(!kinds.is_empty(), "the musketeer strikes");
    assert!(kinds.iter().all(|s| s.height == StrikeHeight::Down), "{kinds:?}");
    assert_eq!(kinds[0].kind, StrikeKind::Shot, "loaded: point-blank first");
    assert!(kinds.iter().any(|s| s.kind == StrikeKind::Thrust), "then the bayonet: {kinds:?}");
}

#[test]
fn rule_off_no_strikes_and_crawlers_untouched() {
    let (mut w, pik, _) = setup(Rules::default());
    w.spawn(pik, 0, Pos::from_pixels(2000, 2000), 0);
    w.spawn(pik, 1, Pos::from_pixels(2060, 2000), 128);
    for _ in 0..400 {
        w.step(&[]);
    }
    assert!(!w.hits.is_empty());
    assert!(w.hits.iter().all(|h| h.strike.is_none() && h.critical.is_none()));
    assert!(!on_ground(&Body::default()));
    // The pinned original hashes are in `tests/wounds.rs` and `tests/plough.rs`.
}

/// A crawler with life for many blows dies of the first one: a finishing blow (the heart for
/// a thrust), struck from within ~1 m (`FINISH_REACH`), not from the pike's reach.
#[test]
fn one_finishing_blow_kills_a_crawler_from_close() {
    let (mut w, pik, _) = setup(ON);
    w.spawn(pik, 0, Pos::from_pixels(2000, 2000), 0);
    let c = w.spawn(pik, 1, Pos::from_pixels(2100, 2000), 128).0 as usize;
    w.units[c].body = crawler();
    w.units[c].life = 5000;
    for _ in 0..600 {
        w.step(&[]);
        if w.hits.iter().any(|h| h.victim == c as u32) {
            break;
        }
    }
    let hits: Vec<_> = w.hits.iter().filter(|h| h.victim == c as u32).collect();
    assert_eq!(hits.len(), 1, "one blow: {hits:?}");
    let h = hits[0];
    assert!(h.killed && !w.units[c].living(), "the blow kills: {h:?}");
    let s = h.strike.expect("a strike");
    assert!(s.finishing && s.height == StrikeHeight::Down && s.kind == StrikeKind::Thrust, "{s:?}");
    assert_eq!(h.critical, Some(Vital::Heart));
    let d = f64::from(h.dir.0).hypot(f64::from(h.dir.1));
    assert!(d <= f64::from(FINISH_REACH) + 5.0, "he stands over him: {d} px");
}

/// m2-finish2: he finishes from beside the torso, square to the body's axis (the facing),
/// wherever the man lies (toward him, away, across), never on the legs, facing the torso.
#[test]
fn the_finishing_stance_is_beside_the_torso() {
    use acr_sim::melee::{STANCE_SIDE, TORSO_AHEAD};
    for vdir in [128u8, 0, 64, 192, 96] {
        let (mut w, pik, _) = setup(ON);
        let a = w.spawn(pik, 0, Pos::from_pixels(2000, 2000), 0).0 as usize;
        let c = w.spawn(pik, 1, Pos::from_pixels(2070, 2000), vdir).0 as usize;
        w.units[c].body = crawler();
        w.units[c].life = 5000;
        for _ in 0..600 {
            w.step(&[]);
            if w.hits.iter().any(|h| h.victim == c as u32) {
                break;
            }
        }
        let h = w.hits.iter().find(|h| h.victim == c as u32).expect("he strikes");
        assert!(h.killed, "{h:?}");
        let ang = f64::from(vdir) * std::f64::consts::TAU / 256.0;
        let torso = (2070.0 + ang.cos() * f64::from(TORSO_AHEAD), 2000.0 + ang.sin() * f64::from(TORSO_AHEAD));
        let me = (f64::from(h.pos.0 - h.dir.0), f64::from(h.pos.1 - h.dir.1));
        let (rx, ry) = (me.0 - torso.0, me.1 - torso.1);
        // Along the axis about at the torso; across it STANCE_SIDE out (within half a
        // stride: his steps are ~22 px).
        let along = rx * ang.cos() + ry * ang.sin();
        let side = (-rx * ang.sin() + ry * ang.cos()).abs();
        assert!(along.abs() <= 12.0, "dir {vdir}: {along} px along the body");
        assert!((side - f64::from(STANCE_SIDE)).abs() <= 12.0, "dir {vdir}: {side} px beside it");
        let face = acr_sim::trig::get_dir((torso.0 - me.0) as i32, (torso.1 - me.1) as i32);
        assert!((w.units[a].dir.wrapping_sub(face) as i8).abs() <= 12, "dir {vdir}: faces the torso");
    }
}

/// A cut finishes at the throat, a smash at the skull; a rider strikes from his reach.
#[test]
fn finishing_vitals_by_kind() {
    use acr_sim::melee::finishing_vital;
    assert_eq!(finishing_vital(StrikeKind::Thrust), Some((Part::Chest, Vital::Heart)));
    assert_eq!(finishing_vital(StrikeKind::Cut), Some((Part::Head, Vital::Neck)));
    assert_eq!(finishing_vital(StrikeKind::Smash), Some((Part::Head, Vital::Brain)));
    assert_eq!(finishing_vital(StrikeKind::Shot), None);
    let (w, c) = finish(|_, lan| lan, 600);
    let h = w.hits.iter().find(|h| h.victim == c as u32).expect("the lancer strikes");
    assert!(h.strike.is_some_and(|s| s.finishing && s.mounted) && h.killed, "{h:?}");
    let d = f64::from(h.dir.0).hypot(f64::from(h.dir.1));
    assert!(d > f64::from(FINISH_REACH) + 3.0, "the rider keeps his reach: {d} px");
}

/// Two men on their feet fence as before: no finishing blows, struck from the weapon's reach.
#[test]
fn a_standing_duel_is_unchanged() {
    let (mut w, pik, _) = setup(ON);
    let a = w.spawn(pik, 0, Pos::from_pixels(2000, 2000), 0).0 as usize;
    let b = w.spawn(pik, 1, Pos::from_pixels(2100, 2000), 128).0 as usize;
    w.units[a].life = 5000;
    w.units[b].life = 5000;
    for _ in 0..300 {
        w.step(&[]);
    }
    let blows: Vec<_> =
        w.hits.iter().filter(|h| !on_ground(&w.units[h.victim as usize].body) || h.strike.is_some()).collect();
    assert!(!blows.is_empty(), "they fence");
    let first = blows[0];
    assert!(!first.strike.expect("a strike").finishing, "{first:?}");
    let d = f64::from(first.dir.0).hypot(f64::from(first.dir.1));
    assert!(d > f64::from(FINISH_REACH) + 3.0, "from the pike's reach: {d} px");
}
