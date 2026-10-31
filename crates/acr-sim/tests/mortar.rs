//! Mortars (rule `mortars`, `acr_sim::mortar`, #301): the `ArtMrt` types made from the field
//! gun, engagement zones and their fire; without the rule nothing changes.

use std::sync::Arc;

use acr_formats::defs::UnitDef;
use acr_formats::defs::nres::weapon_kinds;
use acr_sim::catalog::{Catalog, ObjectType};
use acr_sim::formation::Formations;
use acr_sim::map::MapGrid;
use acr_sim::mortar::{self, Zone};
use acr_sim::tech::TypeTech;
use acr_sim::weapon::Weapons;
use acr_sim::{Command, Pos, Rules, TypeId, World};

/// A field gun like `ArtPus`: ball (index 1, `IADRO`) and canister (index 2, `KARTECH`).
const GUN: &str = "NAME ArtPus\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 16 1 0 0\r\n@MOTION_L 16 0 0 9\r\n\
    @MOTION_R 16 0 10 19\r\n@ATTACK1 16 0 0 19\r\n@PATTACK1 16 0 0 9\r\n@ATTACK2 16 0 0 19\r\n\
    @PATTACK2 16 0 0 9\r\n@DEATH 16 0 0 9\r\nLIFE 1500\r\nGEOMETRY 1 60 20\r\n\
    WEAPON 1 PPOINTT\r\nWEAPONKIND 1 IADRO\r\nATTACK_RADIUS 1 1500 6000\r\nDET_RADIUS 1 1500 10000\r\n\
    ATTACK_PAUSE 1 100\r\nWEAPON 2 PPOINTT\r\nWEAPONKIND 2 KARTECH\r\nATTACK_RADIUS 2 0 1500\r\n\
    DET_RADIUS 2 0 1500\r\nATTACK_PAUSE 2 100\r\nWEAPONINSECTOR 2 64 1500 400 1 32 50\r\n\
    ARTPODGOTOVKA\r\nRAZBROS 250\r\nMATHERIAL 1 BODY\r\nCANKILL 1 BODY\r\n[END]\r\n";
const SOLDIER: &str = "NAME Sol\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@DEATH 9 0 0 9\r\n\
    #DEATHLIE1 9 1 0 9\r\nLIFE 60\r\nGEOMETRY 1 10 40\r\nMATHERIAL 1 BODY\r\nUNITRADIUS 8\r\n[END]\r\n";
const FORT: &str = "NAME Frt\r\nBUILDING\r\nLIFE 1000\r\nLOCATION 0 0 64 64\r\n[END]\r\n";

/// The original's weapons on this path (`WEAPON.NDS` lines 38, 41, 42, 50, 60, 70, 349).
const NDS: &[u8] = b"[MEMBERS]\r\n\
    PPOINTT #POINT 0 0 NONE NO_GRAVITY SLIGHTUP 0 1\r\n\
    DIM32_2 #DIM32 1 20 WHITE NO_GRAVITY SLIGHTUP 0 1\r\n\
    NUCL2P #NUCL1 340 35 NONE HI_GRAVITY ANGLE 10 -1\r\n\
    NUCL3M #NUCL2 800 60 NONE HI_GRAVITY3 ANGLE 75 -1\r\n\
    NUCLM #POINT 100 50 NONE HI_GRAVITY3 SLIGHTUP 0 2\r\n\
    NUCLMOSKOL #NUCL1 100 50 NONE HI_GRAVITY3 RANDOM 9 -1\r\n\
    PU2 #PU2 900 10 WHITE NO_GRAVITY SLIGHTUP 0 1\r\n\
    [CHILDWEAPON]\r\nNUCLM 4 9 0 1 NUCLMOSKOL\r\n\
    [SYNCWEAPON]\r\nPPOINTT 2 DIM32_2 NUCL2P\r\nNUCL3M 2 NUCLM PU2\r\n[END]\r\n";
const ADS: &[u8] = b"@POINT 1 0 0 0\r\n@NUCL1 1 0 0 0\r\n@NUCL2 1 0 0 0\r\n@DIM32 1 0 0 43\r\n@PU2 1 0 0 9\r\n";
const NRES: &[u8] = b"[WEAPONS]
#GENSHIELD 13
MECH 21 19 O
IADRO 2 15 HX
KARTECH 2 15 HX
IADROMOR 0 7 -
[UNITKINDS]
";

const ON: Rules = Rules {
    hit_locations: false,
    body_spacing: false,
    mortars: true,
    corpse_piles: false,
    group_move: false,
    cannon_fire: false,
    ball_terrain: false,
    dart_fire: false,
};

/// The catalog: gun, soldier and a fort that builds the gun.
fn catalog() -> (Catalog, TypeId, TypeId, TypeId) {
    let mut c = Catalog::new();
    let gun = c.add(ObjectType::from_def("ArtPus(EN)", &UnitDef::parse(GUN.as_bytes())));
    let sol = c.add(ObjectType::from_def("Sol(EN)", &UnitDef::parse(SOLDIER.as_bytes())));
    let fort = c.add(ObjectType::from_def("Frt(EN)", &UnitDef::parse(FORT.as_bytes())));
    c.set_weapon_kinds(&weapon_kinds(NRES));
    c.set_weapons(Weapons::parse(NDS, Some(ADS), None));
    c.tech.types.resize(3, TypeTech::default());
    c.tech.types[usize::from(fort.0)].produce = vec![sol, gun];
    (c, gun, sol, fort)
}

/// A world on `rules`, the mortars appended when the rule is on; the mortar's type (the
/// gun's without the rule).
fn setup(rules: Rules) -> (World, TypeId, TypeId) {
    let (mut c, gun, sol, _) = catalog();
    if rules.mortars {
        mortar::append_types(&mut c);
    }
    let mrt = c.find("ArtMrt(EN)").unwrap_or(gun);
    let mut w = World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default()));
    w.rules = rules;
    (w, mrt, sol)
}

/// A mortar at (800, 2000) px and a block of 24 enemy soldiers around `(cx, 2000)`; the
/// mortar gets one zone over them and fires for `ticks`. Returns the world and every point
/// a shell was laid on (its destination after the scatter).
fn shell(rules: Rules, cx: i32, ticks: u32) -> (World, Vec<(i32, i32)>, i32) {
    let (mut w, mrt, sol) = setup(rules);
    let m = w.spawn(mrt, 0, Pos::from_pixels(800, 2000), 0);
    for k in 0..24 {
        w.spawn(sol, 1, Pos::from_pixels(cx - 100 + (k % 6) * 40, 1940 + (k / 6) * 40), 128);
    }
    let life0: i32 = w.units.iter().filter(|u| u.owner == 1).map(|u| u.life).sum();
    w.step(&[
        Command::MortarZones { player: 0, units: vec![m], zones: vec![Zone { x: cx, y: 2000, r: 200 }] },
        Command::MortarFire { player: 0, units: vec![m], on: true },
    ]);
    let shell = w.catalog.weapons.find(mortar::WEAPON);
    let mut laid = Vec::new();
    for _ in 0..ticks {
        w.step(&[]);
        for p in w.projectiles.slots.iter().flatten() {
            let d = (p.dest.0 >> 14, p.dest.1 >> 14);
            if Some(p.weapon) == shell && !laid.contains(&d) {
                laid.push(d);
            }
        }
    }
    let life1: i32 = w.units.iter().filter(|u| u.owner == 1 && u.alive).map(|u| u.life.max(0)).sum();
    (w, laid, life0 - life1)
}

#[test]
fn the_rule_appends_mortars_after_the_original_types() {
    let (c0, gun, sol, fort) = catalog();
    let mut c = c0.clone();
    mortar::append_types(&mut c);
    assert_eq!(c.len(), c0.len() + 1);
    let mrt = c.find("ArtMrt(EN)").expect("ArtMrt(EN)");
    assert_eq!(usize::from(mrt.0), c0.len(), "after the original types");
    for (id, t) in c0.iter() {
        assert_eq!(c.get(id), t, "original types unchanged");
    }
    let t = c.get(mrt);
    assert!(t.mortar && t.sector_weapon.is_none() && t.md_name == "ArtPus");
    let k = mortar::shell_index(t).unwrap();
    let a = &t.adv.attacks[k];
    assert_eq!((a.radius, a.kind.as_str()), ((mortar::MIN_RANGE, mortar::MAX_RANGE), mortar::KIND));
    assert_eq!(a.weapon_id, c.weapons.find(mortar::WEAPON));
    assert_eq!(a.pause, c.get(gun).adv.attacks[k].pause * mortar::RELOAD_PCT / 100);
    assert!(!t.adv.attacks[2].usable(), "no canister");
    // The fort that builds the gun builds the mortar right after it.
    assert_eq!(c.tech.of(fort).unwrap().produce, [sol, gun, mrt]);
    // Twice does not add it again.
    mortar::append_types(&mut c);
    assert_eq!(c.len(), c0.len() + 1);
}

#[test]
fn a_mortar_shells_its_zone_with_scatter_and_the_bursts_hurt() {
    let (w, laid, lost) = shell(ON, 2600, 900);
    assert!(laid.len() >= 3, "shells laid: {laid:?}");
    // Every shell falls in the zone, give or take the scatter (RAZBROS 250 at 1800 px:
    // at most 1800/32·512·250/32000 ≈ 220 px).
    for &(x, y) in &laid {
        let d = f64::from(x - 2600).hypot(f64::from(y - 2000));
        assert!(d <= 200.0 + 230.0, "{x},{y} off the zone");
    }
    assert!(laid.windows(2).any(|p| p[0] != p[1]), "scattered: {laid:?}");
    assert!(lost > 0, "the bursts hurt nobody");
    assert!(!w.mortar_zones.is_empty());
    // Deterministic.
    let (w2, laid2, _) = shell(ON, 2600, 900);
    assert_eq!(laid, laid2);
    assert_eq!(w.state_hash(), w2.state_hash());
}

#[test]
fn a_mortar_keeps_its_minimum_range() {
    // Enemies 400 px off: the zone is too close, nothing is fired.
    let (w, laid, lost) = shell(ON, 1200, 400);
    assert!(laid.is_empty(), "{laid:?}");
    assert_eq!(lost, 0);
    assert!(w.mortar_zones.values().all(|m| m.firing));
    // A ground attack inside the minimum range fires only from beyond it (the gun draws
    // back first, as the original's ball does inside its `ATTACK_RADIUS` minimum).
    let (mut w, mrt, _) = setup(ON);
    let m = w.spawn(mrt, 0, Pos::from_pixels(800, 2000), 0);
    w.step(&[Command::AttackGround { player: 0, units: vec![m], x: 1200, y: 2000 }]);
    let shell = w.catalog.weapons.find(mortar::WEAPON);
    for _ in 0..400 {
        w.step(&[]);
        let (x, y) = w.units[m.0 as usize].pos.pixels();
        let born = w.projectiles.slots.iter().flatten().any(|p| Some(p.weapon) == shell && p.frame <= 1);
        let d = acr_sim::trig::norma(1200 - x, 2000 - y);
        assert!(!born || d >= mortar::MIN_RANGE, "fired from {d} px");
    }
}

#[test]
fn off_the_mortar_commands_change_nothing() {
    // Without the rule there is no ArtMrt; zones given to the field gun are ignored and the
    // hash is that of the same game without the commands.
    let run = |zones: bool| {
        let (mut w, gun, sol) = setup(Rules::default());
        assert!(w.catalog.find("ArtMrt(EN)").is_none());
        let m = w.spawn(gun, 0, Pos::from_pixels(800, 2000), 0);
        w.spawn(sol, 1, Pos::from_pixels(2600, 2000), 128);
        let cmds = if zones {
            vec![
                Command::MortarZones { player: 0, units: vec![m], zones: vec![Zone { x: 2600, y: 2000, r: 200 }] },
                Command::MortarFire { player: 0, units: vec![m], on: true },
            ]
        } else {
            vec![]
        };
        w.step(&cmds);
        for _ in 0..300 {
            w.step(&[]);
        }
        assert!(w.mortar_zones.is_empty());
        w.state_hash()
    };
    assert_eq!(run(true), run(false));
}

#[test]
fn clearing_the_zones_stops_the_fire() {
    let (mut w, mrt, sol) = setup(ON);
    let m = w.spawn(mrt, 0, Pos::from_pixels(800, 2000), 0);
    w.spawn(sol, 1, Pos::from_pixels(2600, 2000), 128);
    w.step(&[Command::MortarZones { player: 0, units: vec![m], zones: vec![Zone { x: 2600, y: 2000, r: 5000 }] }]);
    let z = &w.mortar_zones[&m];
    assert_eq!((z.zones[0].r, z.firing), (mortar::ZONE_RADIUS.1, false));
    w.step(&[Command::MortarZones { player: 0, units: vec![m], zones: vec![] }]);
    assert!(w.mortar_zones.is_empty());
}

/// Mortars and hit locations on: the mortar's shells burst (fragments, round damage) but
/// start no ground fire and set nobody burning; the renderer learns they are a mortar's.
#[test]
fn a_mortar_shell_starts_no_fire() {
    let rules = Rules { hit_locations: true, ..ON };
    let (mut w, mrt, sol) = setup(rules);
    let m = w.spawn(mrt, 0, Pos::from_pixels(800, 2000), 0);
    for k in 0..24 {
        w.spawn(sol, 1, Pos::from_pixels(2500 + (k % 6) * 40, 1940 + (k / 6) * 40), 128);
    }
    w.step(&[
        Command::MortarZones { player: 0, units: vec![m], zones: vec![Zone { x: 2600, y: 2000, r: 200 }] },
        Command::MortarFire { player: 0, units: vec![m], on: true },
    ]);
    let mut bursts = 0;
    for _ in 0..900 {
        w.step(&[]);
        assert!(w.ground_fires.list.is_empty(), "tick {}: a mortar shell left a fire", w.tick.0);
        assert!(!w.units.iter().any(|u| u.burning), "tick {}: someone burns", w.tick.0);
        bursts = bursts.max(w.impacts.iter().filter(|i| i.cause == acr_sim::combat::DeathCause::Explosion).count());
    }
    assert!(bursts > 0, "no shell burst");
    assert!(w.impacts.iter().all(|i| i.mortar), "impacts marked as a mortar's");
    assert!(!w.fragments.is_empty() && w.fragments.iter().all(|f| f.mortar), "fragments flew, marked");
}

/// A howitzer's shell (`IADROMOR` on a gun that is no mortar) still leaves its fire and
/// sets the men in it burning; a mortar's at the same time does not.
#[test]
fn a_howitzer_shell_still_starts_a_fire() {
    let (mut c, _, sol, _) = catalog();
    mortar::append_types(&mut c);
    let how_md = GUN.replace("WEAPONKIND 1 IADRO\r\n", "WEAPONKIND 1 IADROMOR\r\n");
    let how = c.add(ObjectType::from_def("How(EN)", &UnitDef::parse(how_md.as_bytes())));
    let mrt = c.find("ArtMrt(EN)").unwrap();
    let mut w = World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default()));
    w.rules = Rules { hit_locations: true, ..ON };
    let h = w.spawn(how, 0, Pos::from_pixels(400, 1000), 0).0 as usize;
    let m = w.spawn(mrt, 0, Pos::from_pixels(400, 3000), 0).0 as usize;
    let near_h = w.spawn(sol, 1, Pos::from_pixels(2000, 1000), 128).0 as usize;
    let near_m = w.spawn(sol, 1, Pos::from_pixels(2000, 3000), 128).0 as usize;
    let (ah, am) = (acr_sim::shrapnel::shell_attack(&w, h).unwrap(), acr_sim::shrapnel::shell_attack(&w, m).unwrap());
    acr_sim::ground_fire::shell_burst(&mut w, Some(h), ah, (2000, 1000), false);
    acr_sim::ground_fire::shell_burst(&mut w, Some(m), am, (2000, 3000), false);
    assert_eq!(w.ground_fires.list.len(), 1, "only the howitzer's shell burns");
    assert_eq!((w.ground_fires.list[0].x, w.ground_fires.list[0].y), (2000, 1000));
    for _ in 0..20 {
        w.step(&[]);
    }
    assert!(w.units[near_h].burning, "the man in the howitzer's fire burns");
    assert!(!w.units[near_m].burning, "the man where the mortar's shell fell does not");
}
