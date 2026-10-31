//! The body model of the damage model (#262, `docs/damage-model.md` section 1): outcomes
//! per cause, what a body can still do, bleeding, out of the fight and the death snapshot.

use std::sync::Arc;

use acr_formats::defs::UnitDef;
use acr_formats::defs::nres::weapon_kinds;
use acr_sim::body::{
    ANKLE, BALL_HIPS, Body, Cause, ELBOW, Fracture, FractureKind, Injury, KNEE, Limb, LimbId, Outcome, Part, Torn,
    WHOLE, WRIST, apply, injure, leave_fight,
};
use acr_sim::catalog::{Catalog, ObjectType};
use acr_sim::combat::{DeathCause, make_damage};
use acr_sim::formation::Formations;
use acr_sim::map::MapGrid;
use acr_sim::wounds::HitRegion;
use acr_sim::{Command, Pos, Rules, TypeId, World};

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
    VISTREL 12 18 HRB\r\nVAFLIA 4 4 HFB\r\n[UNITKINDS]\r\n";
/// Pikemen, musketeers and a gunner whose blow is a cannonball (`IADRO`).
fn setup() -> (World, TypeId, TypeId, TypeId) {
    let mut c = Catalog::new();
    let pik = c.add(ObjectType::from_def("Pik(EN)", &UnitDef::parse(PIKEMAN.as_bytes())));
    let mus = c.add(ObjectType::from_def("Mus(EN)", &UnitDef::parse(MUSKETEER.as_bytes())));
    let gunner = PIKEMAN.replace("NAME Pik", "NAME Gun").replace("WEAPONKIND 0 PIKA", "WEAPONKIND 0 IADRO");
    let gun = c.add(ObjectType::from_def("Gun(EN)", &UnitDef::parse(gunner.as_bytes())));
    c.set_weapon_kinds(&weapon_kinds(NRES));
    (World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default())), pik, mus, gun)
}

const ON: Rules = Rules {
    hit_locations: true,
    body_spacing: false,
    mortars: false,
    corpse_piles: false,
    group_move: false,
    cannon_fire: false,
    ball_terrain: false,
    formation_attack: false,
    animal_give_up: false,
    dart_fire: false,
};

fn world() -> (World, usize) {
    let (mut w, pik, _, _) = setup();
    w.rules = ON;
    let p = w.spawn(pik, 0, Pos::from_pixels(2000, 2000), 0);
    (w, p.0 as usize)
}

/// `n` hits by `cause` with `energy` on `region`, each on a fresh body (the world's fixed
/// seed).
fn spread(cause: Cause, energy: u8, region: HitRegion, n: usize) -> Vec<Outcome> {
    let (mut w, p) = world();
    (0..n)
        .map(|_| {
            w.units[p].body = Body::default();
            injure(&mut w, p, cause, energy, region).outcome
        })
        .collect()
}

/// Percent of `v` matching `f`.
fn share(v: &[Outcome], f: impl Fn(&Outcome) -> bool) -> usize {
    v.iter().filter(|o| f(o)).count() * 100 / v.len()
}

fn is_break(o: &Outcome) -> bool {
    matches!(o, Outcome::Fracture(_))
}

#[test]
fn outcomes_spread_by_cause() {
    // Musket balls shatter bones; at close range they tear hands, forearms and feet off.
    let musket = spread(Cause::Musket, 100, HitRegion::Legs, 600);
    let wound = share(&musket, |o| *o == Outcome::Wound);
    let (broke, off) = (share(&musket, is_break), share(&musket, |o| *o == Outcome::Off));
    assert!(
        (39..=59).contains(&wound) && (35..=55).contains(&broke) && (2..=11).contains(&off),
        "{wound} {broke} {off}"
    );
    let open = share(&musket, |o| *o == Outcome::Fracture(FractureKind::Open));
    assert!(open * 3 > broke && open < broke, "open {open} of {broke}");
    let far = spread(Cause::Musket, 40, HitRegion::Arms, 600);
    assert!(share(&far, |o| *o == Outcome::Off) <= 5);
    let belly = spread(Cause::Musket, 100, HitRegion::Torso, 600);
    assert!((3..=14).contains(&share(&belly, |o| *o == Outcome::BellyOpen)));
    assert!(spread(Cause::Musket, 40, HitRegion::Torso, 100).iter().all(|o| *o == Outcome::Wound));

    // Thrown axes: deep gashes, hands off, bellies open, most heads split.
    let axe = spread(Cause::Thrown, 100, HitRegion::Arms, 600);
    let (deep, off) = (share(&axe, |o| *o == Outcome::DeepWound), share(&axe, |o| *o == Outcome::Off));
    assert!((60..=80).contains(&deep) && (6..=18).contains(&off), "{deep} {off}");
    let axe = spread(Cause::Thrown, 100, HitRegion::Torso, 600);
    assert!((5..=16).contains(&share(&axe, |o| *o == Outcome::BellyOpen)));
    let axe = spread(Cause::Thrown, 100, HitRegion::Head, 600);
    assert!((48..=72).contains(&share(&axe, |o| *o == Outcome::Kill)));
    assert_eq!(Cause::of(DeathCause::Thrown, false), Cause::Thrown);
    assert_eq!(Cause::of(DeathCause::Melee, false), Cause::Bayonet);

    let grape = spread(Cause::Grapeshot, 100, HitRegion::Arms, 600);
    let wound = share(&grape, |o| *o == Outcome::Wound);
    let (broke, off) = (share(&grape, is_break), share(&grape, |o| *o == Outcome::Off));
    assert!(
        (30..=50).contains(&wound) && (25..=45).contains(&broke) && (15..=35).contains(&off),
        "{wound} {broke} {off}"
    );

    let bayonet = spread(Cause::Bayonet, 100, HitRegion::Legs, 400);
    assert!((75..=95).contains(&share(&bayonet, |o| *o == Outcome::Wound)));
    let sabre = spread(Cause::Sabre, 100, HitRegion::Arms, 400);
    assert!((10..=30).contains(&share(&sabre, |o| *o == Outcome::Off)));
    assert!((50..=70).contains(&share(&sabre, |o| *o == Outcome::DeepWound)));

    // A cannonball with real energy tears a man apart (#262: "a direct hit should shred
    // the troop"): through the legs it takes both and throws the torso, through an arm it
    // goes on across the chest, through the trunk it pulverizes or tears him in two.
    let torn = |o: &Outcome| matches!(o, Outcome::Torn(Torn::InTwo { .. } | Torn::Pulverized));
    let pulp = |o: &Outcome| *o == Outcome::Torn(Torn::Pulverized);
    for e in [100, 80, 70] {
        let full = spread(Cause::Cannonball, e, HitRegion::Torso, 400);
        let p = share(&full, pulp);
        assert!((55..=75).contains(&p) && full.iter().all(torn), "{e}: {p}");
        let arms = spread(Cause::Cannonball, e, HitRegion::Arms, 400);
        let p = share(&arms, pulp);
        assert!((55..=75).contains(&p) && arms.iter().all(torn), "{e}: {p}");
    }
    for e in [100, 60, 40] {
        let legs = spread(Cause::Cannonball, e, HitRegion::Legs, 100);
        assert!(legs.iter().all(|o| *o == Outcome::Torn(Torn::InTwo { level: BALL_HIPS })), "{e}");
    }
    let half = spread(Cause::Cannonball, 60, HitRegion::Torso, 100);
    assert!(half.iter().all(|o| matches!(o, Outcome::Torn(Torn::InTwo { .. }))));
    let half = spread(Cause::Cannonball, 40, HitRegion::Torso, 100);
    assert!(half.iter().all(|o| matches!(o, Outcome::Torn(Torn::InTwo { .. }))));
    // Only a spent ball (the fifth man, energy 20) leaves him in one piece: a limb off or
    // crushed, the chest crushed or the belly open.
    let ball = spread(Cause::Cannonball, 20, HitRegion::Legs, 400);
    assert!((75..=95).contains(&share(&ball, |o| *o == Outcome::Off)));
    let spent = spread(Cause::Cannonball, 10, HitRegion::Legs, 100);
    assert!(spent.iter().all(|o| *o == Outcome::Fracture(FractureKind::Open)));
    let low = spread(Cause::Cannonball, 20, HitRegion::Torso, 400);
    let kill = share(&low, |o| *o == Outcome::Kill);
    assert!((25..=50).contains(&kill) && low.iter().all(|o| matches!(o, Outcome::Kill | Outcome::BellyOpen)), "{kill}");
    let heads = spread(Cause::Cannonball, 20, HitRegion::Head, 10);
    assert!(heads.iter().all(|o| *o == Outcome::Torn(Torn::Headless)));
    // Heads: a ball kills, a sabre half the time.
    assert!(spread(Cause::Musket, 100, HitRegion::Head, 20).iter().all(|o| *o == Outcome::Kill));
    let sabre = spread(Cause::Sabre, 100, HitRegion::Head, 400);
    assert!((35..=65).contains(&share(&sabre, |o| *o == Outcome::Kill)));
    // Bellies: a bayonet stab opens one in five.
    let bayonet = spread(Cause::Bayonet, 100, HitRegion::Torso, 600);
    assert!((13..=28).contains(&share(&bayonet, |o| *o == Outcome::BellyOpen)));
}

#[test]
fn the_same_seed_gives_the_same_injuries() {
    let a = spread(Cause::Grapeshot, 100, HitRegion::Legs, 100);
    assert_eq!(a, spread(Cause::Grapeshot, 100, HitRegion::Legs, 100));
}

fn limb(b: &mut Body, l: LimbId) -> &mut Limb {
    &mut b.limbs[l as usize]
}

#[test]
fn a_lost_foot_hobbles_and_two_lost_shins_crawl() {
    let mut b = Body::default();
    assert_eq!((b.move_percent(), b.crawling(), b.out_of_fight()), (100, false, false));
    limb(&mut b, LimbId::LegL).lost = ANKLE + 10;
    assert_eq!((b.leg(LimbId::LegL), b.crawling()), (15, false));
    assert_eq!(b.move_percent(), 15, "hobbling on the stump");
    let mut b = Body::default();
    limb(&mut b, LimbId::LegL).lost = KNEE + 40;
    limb(&mut b, LimbId::LegR).lost = KNEE + 30;
    assert!(b.crawling() && b.out_of_fight());
    assert_eq!(b.move_percent(), 15, "crawling on two arms");
    limb(&mut b, LimbId::ArmL).wounds = 3;
    assert_eq!(b.move_percent(), 8, "one working arm");
    limb(&mut b, LimbId::ArmR).lost = 30;
    assert_eq!(b.move_percent(), 0, "writhing");
    assert_eq!(b.walk_rate(16), 0);
}

#[test]
fn a_fracture_crawls_a_flesh_wound_slows() {
    let mut b = Body::default();
    limb(&mut b, LimbId::LegR).wounds = 1;
    assert_eq!((b.leg(LimbId::LegR), b.move_percent(), b.crawling()), (65, 65, false));
    limb(&mut b, LimbId::LegR).wounds = 4;
    assert_eq!(b.leg(LimbId::LegR), 30);
    let mut b = Body::default();
    limb(&mut b, LimbId::LegL).fracture = Some(Fracture { level: KNEE - 30, kind: FractureKind::Closed, bend: 8 });
    assert!(b.crawling());
    // A dislocated knee still carries a little.
    limb(&mut b, LimbId::LegL).fracture = Some(Fracture { level: KNEE, kind: FractureKind::Dislocated, bend: 0 });
    assert_eq!((b.leg(LimbId::LegL), b.crawling()), (20, false));
}

#[test]
fn arms_decide_the_musket() {
    let hit = |part, outcome, level| Injury { part, level, outcome, energy: 100, cause: Cause::Musket };
    let mut b = Body::default();
    apply(&mut b, &hit(Part::Limb(LimbId::ArmL), Outcome::Wound, 80));
    assert!(!b.dropped && !b.disarmed() && b.arm(LimbId::ArmL) == 60);
    apply(&mut b, &hit(Part::Limb(LimbId::ArmR), Outcome::Fracture(FractureKind::Closed), 60));
    assert!(b.dropped && b.disarmed() && b.holds_one_handed() && !b.out_of_fight());
    let mut b = Body::default();
    apply(&mut b, &hit(Part::Limb(LimbId::ArmR), Outcome::Off, 220));
    assert!(b.dropped, "a lost hand drops it");
    apply(&mut b, &hit(Part::Limb(LimbId::ArmL), Outcome::Off, 40));
    assert!(!b.holds_one_handed() && b.out_of_fight());
    // A crawler drops his long weapon too.
    let mut b = Body::default();
    apply(&mut b, &hit(Part::Limb(LimbId::LegL), Outcome::Off, 100));
    assert!(b.dropped && b.limbs[LimbId::LegL as usize].lost == 100);
}

#[test]
fn bleeding_by_the_table() {
    let rate = |f: &dyn Fn(&mut Body)| {
        let mut b = Body::default();
        f(&mut b);
        b.bleeding()
    };
    let open = Some(Fracture { level: 50, kind: FractureKind::Open, bend: 0 });
    assert_eq!(rate(&|_| {}), 0);
    assert_eq!(rate(&|b| limb(b, LimbId::LegL).lost = KNEE - 1), 6);
    assert_eq!(rate(&|b| limb(b, LimbId::LegL).lost = KNEE), 4);
    assert_eq!(rate(&|b| limb(b, LimbId::LegL).lost = ANKLE), 2);
    // A man's arm stumps bleed slower (#254).
    assert_eq!(rate(&|b| limb(b, LimbId::ArmL).lost = ELBOW - 1), 4);
    assert_eq!(rate(&|b| limb(b, LimbId::ArmL).lost = ELBOW), 3);
    assert_eq!(rate(&|b| limb(b, LimbId::ArmL).lost = WRIST), 1);
    assert_eq!(rate(&|b| limb(b, LimbId::LegR).fracture = open), 2);
    assert_eq!(rate(&|b| b.belly_open = true), 5);
    let all = |b: &mut Body| {
        b.belly_open = true;
        limb(b, LimbId::ArmR).lost = 10;
        limb(b, LimbId::LegR).lost = 200;
    };
    assert_eq!(rate(&all), 5 + 4 + 4);
    assert_eq!(Body::default().limbs[0].lost, WHOLE);
}

#[test]
fn out_of_the_fight_ignores_orders_and_drags_himself_away() {
    let (mut w, p) = world();
    let pik = w.units[p].kind;
    let enemy = w.spawn(pik, 1, Pos::from_pixels(2500, 2000), 128);
    limb(&mut w.units[p].body, LimbId::LegL).lost = 60;
    assert!(w.units[p].body.out_of_fight());
    // No orders, no attacks.
    let id = w.units[p].id;
    w.step(&[Command::Move { player: 0, units: vec![id], x: 3500, y: 2000 }]);
    assert!(w.units[p].orders.is_empty(), "{:?}", w.units[p].orders);
    assert!(!acr_sim::combat::attack_obj(&mut w, p, enemy.0 as usize, 0xFE));
    // Away from the nearest enemy (east), towards the west.
    leave_fight(&mut w, p);
    for _ in 0..300 {
        w.step(&[Command::Stop { player: 1, units: vec![enemy] }]);
    }
    let x = w.units[p].pos.pixels().0;
    assert!(x < 1990, "crawled to {x}");
}

#[test]
fn torn_in_two_keeps_the_body_at_death() {
    let (mut w, pik, _, gun) = setup();
    w.rules = ON;
    let s = w.spawn(gun, 1, Pos::from_pixels(1000, 1000), 0).0 as usize;
    let mut torn = None;
    for k in 0..40 {
        let v = w.spawn(pik, 0, Pos::from_pixels(2000 + 40 * k, 2000), 0).0 as usize;
        limb(&mut w.units[v].body, LimbId::ArmL).lost = 150; // an older wound
        w.hit_energy = Some(70);
        make_damage(&mut w, v, 1, Some(s), 0);
        w.hit_energy = None;
        let hit = *w.hits.back().unwrap();
        let injury = hit.injury.expect("an injury");
        assert_eq!(injury.energy, 70);
        if let Some(r) = w.deaths.get(&(v as u32)) {
            assert!(hit.killed && injury.outcome.kills());
            assert_eq!(r.cause, DeathCause::Cannonball);
            assert_eq!(r.body.limbs[LimbId::ArmL as usize].lost, 150, "the snapshot keeps the old wounds");
            if let Some(Torn::InTwo { level }) = r.torn {
                assert_eq!(injury.outcome, Outcome::Torn(Torn::InTwo { level }));
                torn = Some(level);
            }
        } else {
            // A limb hit: the limb is off (or crushed) and he lives on.
            assert!(matches!(injury.part, Part::Limb(_)), "{injury:?}");
        }
    }
    assert!(torn.is_some(), "a torso hit at energy 70 tears in two");
}

/// Rule `cannon_fire`: under hit locations a round-shot hit (`IADRO`, flag `X`) sets the man
/// it does not kill on fire only with the rule; without hit locations the original's flag
/// goes on either way.
#[test]
fn round_shot_lights_men_only_with_cannon_fire() {
    let lit = |rules: Rules| {
        let (mut w, pik, _, gun) = setup();
        w.rules = rules;
        let s = w.spawn(gun, 1, Pos::from_pixels(1000, 1000), 0).0 as usize;
        let v = w.spawn(pik, 0, Pos::from_pixels(2000, 2000), 0).0 as usize;
        w.hit_energy = Some(5);
        make_damage(&mut w, v, 1, Some(s), 0);
        assert!(w.units[v].living());
        w.units[v].burning
    };
    assert!(!lit(ON), "off by default under hit locations");
    assert!(lit(Rules { cannon_fire: true, ..ON }));
    assert!(lit(Rules::default()), "the original sets its flag");
    assert!(lit(Rules { cannon_fire: true, ..Rules::default() }));
}

/// Rule `dart_fire` (#350): under hit locations a blowpipe dart (`VAFLIA`, flag `F`) sets
/// the man it hits on fire only with the rule; without hit locations the original's flag
/// goes on either way.
#[test]
fn darts_light_men_only_with_dart_fire() {
    let lit = |rules: Rules| {
        let mut c = Catalog::new();
        let pik = c.add(ObjectType::from_def("Pik(EN)", &UnitDef::parse(PIKEMAN.as_bytes())));
        let blower = PIKEMAN.replace("NAME Pik", "NAME Trb").replace("WEAPONKIND 0 PIKA", "WEAPONKIND 0 VAFLIA");
        let trb = c.add(ObjectType::from_def("Trb(EN)", &UnitDef::parse(blower.as_bytes())));
        c.set_weapon_kinds(&weapon_kinds(NRES));
        let mut w = World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default()));
        w.rules = rules;
        let s = w.spawn(trb, 1, Pos::from_pixels(1000, 1000), 0).0 as usize;
        let v = w.spawn(pik, 0, Pos::from_pixels(2000, 2000), 0).0 as usize;
        w.hit_energy = Some(5);
        make_damage(&mut w, v, 1, Some(s), 0);
        assert!(w.units[v].living());
        w.units[v].burning
    };
    assert!(!lit(ON), "off by default under hit locations");
    assert!(lit(Rules { dart_fire: true, ..ON }));
    assert!(lit(Rules::default()), "the original sets its flag");
}

#[test]
fn rule_off_no_body_no_injuries() {
    let (mut w, pik, mus, _) = setup();
    for k in 0..10 {
        w.spawn(if k % 2 == 0 { pik } else { mus }, 0, Pos::from_pixels(1800 + 30 * k, 2000), 0);
        w.spawn(if k % 2 == 0 { mus } else { pik }, 1, Pos::from_pixels(1800 + 30 * k, 2400), 192);
    }
    for _ in 0..1500 {
        w.step(&[]);
    }
    // The original hashes are pinned in `tests/wounds.rs` and `tests/plough.rs`.
    assert!(!w.deaths.is_empty());
    assert!(w.deaths.values().all(|d| d.body == Body::default() && d.torn.is_none()));
    assert!(w.hits.iter().all(|h| h.injury.is_none()));
}

/// A rider's legs do not make him crawl: the horse carries him on (the crawl clip would
/// drag the horse along).
#[test]
fn a_rider_never_crawls() {
    let mut b = Body { mounted: true, ..Body::default() };
    limb(&mut b, LimbId::LegL).lost = KNEE + 40;
    limb(&mut b, LimbId::LegR).lost = KNEE + 30;
    assert!(!b.crawling() && !b.out_of_fight());
    assert_eq!(b.move_percent(), 100);
}

/// A scripted hit (the close-up bench, #277) takes the body path of battle: the region
/// asked for, an injury, a hit record with the given direction, and a death record when it
/// kills; the same script on the same world gives the same result.
#[test]
fn scripted_hits_injure_record_and_replay() {
    use acr_sim::combat::{ScriptedHit, scripted_hit};
    let strike = |cause, mounted, region, dir, energy, dmg| ScriptedHit {
        cause,
        mounted,
        region,
        dir,
        energy,
        dmg,
        aim: None,
        maul: None,
    };
    let run = || {
        let (mut w, p) = world();
        let first = scripted_hit(&mut w, p, strike(DeathCause::Gunshot, false, Some(HitRegion::Legs), (10, 0), 80, 1))
            .expect("a living victim is hit");
        let hit = w.hits[first];
        assert_eq!(hit.region, Some(HitRegion::Legs));
        assert_eq!(hit.dir, (10, 0));
        assert!(matches!(hit.injury.map(|i| i.part), Some(Part::Limb(LimbId::LegL | LimbId::LegR))));
        let ball =
            scripted_hit(&mut w, p, strike(DeathCause::Cannonball, false, Some(HitRegion::Torso), (0, 5), 100, 1000))
                .expect("still alive before the ball");
        assert!(w.hits[ball].killed);
        assert_eq!(w.deaths.get(&(p as u32)).map(|d| d.cause), Some(DeathCause::Cannonball));
        assert!(
            scripted_hit(&mut w, p, strike(DeathCause::Melee, true, None, (1, 1), 50, 1)).is_none(),
            "the dead are not hit"
        );
        (w.hits.iter().map(|h| (h.injury, h.killed)).collect::<Vec<_>>(), w.units[p].body)
    };
    assert_eq!(run(), run());
}

/// Mauling (#276): claws gash and tear arms off, jaws crush and (an alligator's) sever,
/// horns puncture; heads are torn or killed.
#[test]
fn mauling_outcomes_spread() {
    // Claws: deep gashes, an arm off at high energy, bones broken; never a leg off.
    let claw = spread(Cause::Claw, 100, HitRegion::Arms, 600);
    let (deep, off, broke) =
        (share(&claw, |o| *o == Outcome::DeepWound), share(&claw, |o| *o == Outcome::Off), share(&claw, is_break));
    assert!((55..=75).contains(&deep) && (9..=22).contains(&off) && (13..=28).contains(&broke), "{deep} {off} {broke}");
    assert!(spread(Cause::Claw, 100, HitRegion::Legs, 300).iter().all(|o| *o != Outcome::Off));
    assert!(spread(Cause::Claw, 60, HitRegion::Arms, 300).iter().all(|o| *o != Outcome::Off));
    let claw = spread(Cause::Claw, 100, HitRegion::Torso, 600);
    let belly = share(&claw, |o| *o == Outcome::BellyOpen);
    assert!((17..=35).contains(&belly), "{belly}");
    assert!(claw.iter().all(|o| matches!(o, Outcome::BellyOpen | Outcome::DeepWound)));
    // The face and scalp torn, a quarter killed (throat, skull).
    let claw = spread(Cause::Claw, 100, HitRegion::Head, 600);
    let kill = share(&claw, |o| *o == Outcome::Kill);
    assert!((15..=35).contains(&kill) && claw.iter().all(|o| matches!(o, Outcome::Kill | Outcome::DeepWound)));

    // Jaws: an alligator's take limbs off at the bite, crush the rest.
    let bite = spread(Cause::Bite, 95, HitRegion::Legs, 600);
    let (off, broke) = (share(&bite, |o| *o == Outcome::Off), share(&bite, is_break));
    assert!((21..=39).contains(&off) && (55..=75).contains(&broke), "{off} {broke}");
    let bite = spread(Cause::Bite, 80, HitRegion::Arms, 600);
    let broke = share(&bite, is_break);
    assert!(bite.iter().all(|o| *o != Outcome::Off) && (55..=75).contains(&broke), "{broke}");
    let kill = share(&spread(Cause::Bite, 85, HitRegion::Head, 600), |o| *o == Outcome::Kill);
    assert!((60..=80).contains(&kill), "{kill}");

    // Horns: belly punctures, a few chests through.
    let gore = spread(Cause::Gore, 100, HitRegion::Torso, 600);
    let (belly, kill) = (share(&gore, |o| *o == Outcome::BellyOpen), share(&gore, |o| *o == Outcome::Kill));
    assert!((20..=37).contains(&belly) && (1..=12).contains(&kill), "{belly} {kill}");
    assert!(gore.iter().all(|o| matches!(o, Outcome::BellyOpen | Outcome::Kill | Outcome::DeepWound)));
    let gore = spread(Cause::Gore, 100, HitRegion::Legs, 600);
    assert!((50..=70).contains(&share(&gore, |o| *o == Outcome::DeepWound)));
}

#[test]
fn beasts_by_their_names() {
    use acr_sim::body::Beast;
    assert_eq!(Beast::of("AnmMed"), Some(Beast::Bear));
    assert_eq!(Beast::of("AnmMedSm"), Some(Beast::Bear));
    assert_eq!(Beast::of("ANMCRO"), Some(Beast::Alligator));
    assert_eq!(Beast::of("AnmJug"), Some(Beast::Jaguar));
    assert_eq!(Beast::of("AnmOlns"), Some(Beast::Elk));
    assert_eq!(Beast::of("AnmBiz"), Some(Beast::Bison));
    assert_eq!(Beast::of("AnmEgl"), Some(Beast::Other));
    assert_eq!(Beast::of("EngM18"), None);
    assert!(
        Beast::small("AnmMedSm") && Beast::small("AnmOlns") && Beast::small("AnmMed_Sm") && !Beast::small("AnmMed")
    );
    assert_eq!(Beast::Bear.cause(10), Cause::Claw);
    assert_eq!(Beast::Bear.cause(90), Cause::Bite);
    assert_eq!(Beast::Alligator.cause(10), Cause::Bite);
    assert_eq!(Beast::Bison.cause(10), Cause::Gore);
    assert!(Beast::Bear.energy(Cause::Claw, true) < Beast::Bear.energy(Cause::Claw, false));
    for b in [Beast::Bear, Beast::Jaguar, Beast::Alligator, Beast::Elk, Beast::Bison, Beast::Other] {
        for down in [false, true] {
            assert_eq!(b.regions(down).iter().sum::<i32>(), 100, "{b:?}");
        }
    }
    assert_eq!(Beast::Alligator.region(false, 30), HitRegion::Legs);
}

/// Claws at high energy and horns always knock the man down; he cannot move or strike and
/// gets up when the ticks have run out.
#[test]
fn knocked_down_by_a_mauling_and_up_again() {
    use acr_sim::body::{DOWN_CLAW, DOWN_GORE, knock_down};
    assert_eq!(knock_down(Cause::Claw, 100, 50), DOWN_CLAW);
    assert_eq!(knock_down(Cause::Claw, 40, 50), 0);
    assert_eq!(knock_down(Cause::Gore, 30, 99), DOWN_GORE);
    assert_eq!(knock_down(Cause::Musket, 100, 0), 0);
    let bites = (0..100).filter(|&r| knock_down(Cause::Bite, 95, r) > 0).count();
    assert!((50..=70).contains(&bites), "{bites}");
    let (mut w, p) = world();
    injure(&mut w, p, Cause::Claw, 100, HitRegion::Torso);
    assert_eq!(w.units[p].body.down, DOWN_CLAW);
    assert!(w.units[p].body.downed() && w.units[p].body.walk_rate(16) == 0);
    for _ in 0..DOWN_CLAW - 1 {
        w.step(&[]);
    }
    assert!(w.units[p].body.downed());
    w.step(&[]);
    assert!(!w.units[p].body.downed());
}

const BEAR: &str = "NAME AnmMed\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n\
    @MOTION_R 9 0 10 19\r\n@PATTACK 9 0 0 35\r\n@UATTACK 9 0 35 0\r\n@ATTACK 9 0 0 20\r\n\
    @DEATH 9 0 0 39\r\n#DEATHLIE1 9 1 0 39\r\nLIFE 400\r\nGEOMETRY 1 10 40\r\nDAMAGE 0 12\r\n\
    ATTACK_RADIUS 0 0 100\r\nATTACK_PAUSE 0 1\r\nSEARCH_ENEMY_RADIUS 700\r\nSEARCH_ENEMY_RADIUS_SHIFT 70\r\n\
    DET_RADIUS 0 0 7000\r\nMATHERIAL 1 BODY\r\nCANKILL 1 BODY\r\nWEAPONKIND 0 MECH\r\nCANTCAPTURE\r\n\
    UNITRADIUS 8\r\n[END]\r\n";

/// A bear on a tough man (`rules`) for `ticks`: the world, the man and the bear, and per
/// tick the man's body after it, the bear's life after it and whether the man began it down.
fn mauling(rules: Rules, ticks: usize) -> (World, usize, usize, Vec<(Body, i32, bool)>) {
    let mut c = Catalog::new();
    let bear = c.add(ObjectType::from_def("AnmMed(XX)", &UnitDef::parse(BEAR.as_bytes())));
    let tough = PIKEMAN.replace("LIFE 100", "LIFE 1000").replace("NAME Pik", "NAME Tough");
    let man = c.add(ObjectType::from_def("Tough(EN)", &UnitDef::parse(tough.as_bytes())));
    c.set_weapon_kinds(&weapon_kinds(NRES));
    let mut w = World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default()));
    w.rules = rules;
    let m = w.spawn(man, 0, Pos::from_pixels(2000, 2000), 0).0 as usize;
    let b = w.spawn(bear, 1, Pos::from_pixels(2040, 2000), 128).0 as usize;
    let mut log = Vec::new();
    for _ in 0..ticks {
        let down = w.units[m].body.downed();
        w.step(&[]);
        log.push((w.units[m].body, w.units[b].life, down));
    }
    (w, m, b, log)
}

/// A bear mauls a man: its blows are claws and bites, they knock him down, he stays down
/// while it keeps striking and does not hit back while down. Deterministic; without the
/// rule the bear's blows are plain melee hits.
#[test]
fn a_bear_mauls_a_downed_man() {
    let (w, m, _, log) = mauling(ON, 400);
    let mauls: Vec<_> = w.hits.iter().filter(|h| h.victim == m as u32).filter_map(|h| h.injury).collect();
    assert!(!mauls.is_empty(), "the bear strikes");
    assert!(mauls.iter().all(|i| matches!(i.cause, Cause::Claw | Cause::Bite)), "{mauls:?}");
    assert!(log.iter().any(|(b, ..)| b.downed()), "knocked down");
    // Struck again while down.
    let on_the_ground = w
        .hits
        .iter()
        .filter(|h| h.victim == m as u32)
        .filter(|h| h.tick > 0 && log.get(h.tick as usize - 1).is_some_and(|(b, ..)| b.downed()))
        .count();
    assert!(on_the_ground > 0, "hits on the downed man");
    // The bear loses no life in a tick the man began on the ground.
    for k in 1..log.len() {
        if log[k].2 {
            assert!(log[k].1 >= log[k - 1].1, "the downed man struck back at tick {k}");
        }
    }
    let (again, ..) = mauling(ON, 400);
    assert_eq!(w.state_hash(), again.state_hash());
    let (off, m, ..) = mauling(Rules::default(), 400);
    assert!(off.units[m].body == Body::default());
    assert!(off.hits.iter().all(|h| h.injury.is_none()));
}
