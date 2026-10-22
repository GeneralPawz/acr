//! Food on a synthetic flat map: fields ripen, are harvested and regrow; mills plant them;
//! hunters turn animals into carcasses and carry the meat home.

use std::sync::Arc;

use acr_formats::defs::{NationDef, SpriteResources, UnitDef};
use acr_formats::m3d::SpriteGroup;
use acr_formats::sprite_list::SpriteList;
use acr_sim::catalog::{Catalog, ObjectType};
use acr_sim::economy::{self, FOOD, GOLD};
use acr_sim::formation::Formations;
use acr_sim::map::MapGrid;
use acr_sim::sprite::SpriteKinds;
use acr_sim::tech::Tech;
use acr_sim::{Command, Pos, TypeId, World};

const COMPLEX_LST: &str = "COMPLEX 7\r\nKUK00 159 146 30\r\nKUK01 159 146 30\r\nKUK10 159 146 30\r\nKUK11 159 146 30\r\n\
    KUD00 159 146 30\r\nKUD01 159 146 30\r\nKUB00 159 146 30\r\n";
const COMPLEX_RSR: &[u8] = b"[WORKTRANSFORM]\r\nKUK01 KUD00 5\r\nKUK11 KUD00 5\r\nKUD01 KUB00 5\r\n\
    [TIMETRANSFORM]\r\nKUK00 KUK01 500\r\nKUK10 KUK11 500\r\nKUD00 KUD01 500\r\nKUB00 NONE 1\r\n\
    [SOURCES]\r\nKUK01 FOOD 3 64\r\nKUK11 FOOD 3 64\r\nKUD01 FOOD 3 64\r\n[END]\r\n";
const MEAT_LST: &str = "trees 2\r\nM00 0 0 10\r\nM01 0 0 10\r\n";
const MEAT_RSR: &[u8] = b"[WORKTRANSFORM]\r\nM00 M01 10\r\nM01 NONE 250\r\n[TIMETRANSFORM]\r\nM00 NONE 7500\r\n\
    [SOURCES]\r\nM00 #R8 50 64\r\nM01 #R8 50 64\r\n[END]\r\n";

const PEASANT: &str = "NAME Kri\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n@MOTION_R 9 0 10 19\r\n\
    @ATTACK3 9 0 0 57\r\n#PATTACK3 9 1 0 0\r\n#UATTACK3 9 1 0 0\r\nLIFE 50\r\nGEOMETRY 1 10 40\r\nPEASANT\r\n\
    TAKERESSTAGES FOOD 3 3 4\r\nPORTION 1 FOOD 90\r\n[END]\r\n";
const HUNTER: &str = "NAME Hun\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n@MOTION_R 9 0 10 19\r\n\
    @ATTACK 9 0 0 9\r\n@ATTACK3 9 0 0 57\r\n#PATTACK 9 1 0 0\r\n#UATTACK 9 1 0 0\r\n#PATTACK3 9 1 0 0\r\n#UATTACK3 9 1 0 0\r\n\
    SETACTIVEPOINT #ATTACK 5 0 0\r\nLIFE 50\r\nGEOMETRY 1 10 40\r\nATTACK_RADIUS 0 0 40\r\nDAMAGE 0 50\r\nCANKILL 1 BODY\r\n\
    TAKERESSTAGES FOOD 8 3 4\r\nPORTION 1 FOOD 90\r\n[END]\r\n";
const DEER: &str = "NAME Deer\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n@MOTION_R 9 0 10 19\r\n\
    @DEATH 9 0 0 19\r\nLIFE 40\r\nGEOMETRY 1 10 40\r\nMATHERIAL 1 BODY\r\nTRANSFORMTOMEAT 0\r\n[END]\r\n";
const BASE: &str = "NAME Mel\r\nBUILDING\r\nLIFE 1000\r\nLOCATION 0 0 64 64\r\nBUILDSTAGES 10\r\nUSAGE MELNICA\r\n\
    CONCENTRATOR 1 3 3\r\nRESOURCEBASE 2 FOOD #R8\r\nFARM\r\n[END]\r\n";
const FIELD: &str = "NAME FIELD\r\nSPRITEOBJECT\r\nEXSPRITES KUK00 2 3 64 0\r\nPRICE 1 GOLD 1\r\n[END]\r\n";

fn kinds() -> (SpriteKinds, SpriteKinds) {
    let k = |l: &str, r: &[u8]| SpriteKinds::new(&SpriteList::parse(l).unwrap(), &SpriteResources::parse(r));
    (k(COMPLEX_LST, COMPLEX_RSR), k(MEAT_LST, MEAT_RSR))
}

fn setup() -> (World, [TypeId; 5]) {
    let mut c = Catalog::new();
    let defs = [("Kri(EN)", PEASANT), ("Hun(EN)", HUNTER), ("Deer(EN)", DEER), ("Mel(EN)", BASE), ("FIELD(EN)", FIELD)];
    let ids = defs.map(|(id, md)| c.add(ObjectType::from_def(id, &UnitDef::parse(md.as_bytes()))));
    let mut tech = Tech::default();
    tech.add_nation(
        &c,
        &NationDef::parse(
            b"[FIXED_PRODUCE]\r\nMel(EN) 1\r\nFIELD(EN) 0 0\r\n[ENABLED]\r\nKri(EN)\r\nFIELD(EN)\r\n[END]\r\n",
        ),
    );
    c.tech = tech;
    let mut w = World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default()));
    w.sprite_kinds = Arc::new(kinds());
    (w, ids)
}

fn run(w: &mut World, ticks: usize) {
    for _ in 0..ticks {
        w.step(&[]);
    }
}

#[test]
fn fields_ripen_are_harvested_and_regrow() {
    let (mut w, [kri, .., mel, _]) = setup();
    let field = w.add_sprite(SpriteGroup::Complex, 0, 2600, 2048);
    // 500 frames = 250 ticks to ripen.
    run(&mut w, 249);
    assert_eq!(w.sprites[field].index, 0);
    run(&mut w, 1);
    assert_eq!(w.sprites[field].index, 1, "ripe");
    w.spawn(mel, 0, Pos::from_pixels(2048, 2048), 0);
    let p = w.spawn(kri, 0, Pos::from_pixels(2500, 2100), 0);
    // No gold: the city tick's `CreateFields` cannot pay for new fields around the mill.
    w.players[0].resources[economy::GOLD] = 0;
    economy::census_all(&mut w);
    w.step(&[Command::Gather { player: 0, units: vec![p], x: 2600, y: 2048 }]);
    // Five cycles of 3 food (`FoodEff` 1010: the field lasts WorkAmount · 10 · 10 / 100
    // cycles), then the field turns into its regrowing stage.
    let mut harvested = None;
    for t in 0..400 {
        w.step(&[]);
        if w.sprites[field].index != 1 {
            harvested = Some(t);
            break;
        }
    }
    let t = harvested.expect("harvested");
    // Walking there plus five 28-tick cycles of the 58-frame #ATTACK3.
    assert!((100..220).contains(&t), "harvested after {t} ticks");
    assert_eq!(w.sprites[field].index, 4, "KUD00");
    assert_eq!(w.units[p.0 as usize].carried, 15);
    run(&mut w, 40);
    assert!(w.units[p.0 as usize].orders.is_empty(), "nothing ripe left");
    // It regrows in 250 ticks and gives five more cycles; the last stage then vanishes.
    run(&mut w, 208);
    assert_eq!(w.sprites[field].index, 4);
    run(&mut w, 1);
    assert_eq!(w.sprites[field].index, 5);
    w.step(&[Command::Gather { player: 0, units: vec![p], x: 2600, y: 2048 }]);
    run(&mut w, 300);
    assert!(w.sprites[field].removed, "{:?}", w.sprites[field]);
    assert_eq!(w.units[p.0 as usize].carried, 30);
}

#[test]
fn mills_plant_fields_and_pay_for_them() {
    let (mut w, [.., mel, field]) = setup();
    let mill = w.spawn(mel, 0, Pos::from_pixels(4096, 4096), 0);
    economy::census_all(&mut w);
    let gold = w.players[0].resources[GOLD];
    w.step(&[Command::PlantFields { player: 0, buildings: vec![mill], kind: field }]);
    let planted = w.sprites.len();
    // A 7 × 7 diamond 64 px apart, no jitter: every point is free and flat.
    assert_eq!(planted, 49);
    assert_eq!(w.players[0].resources[GOLD], gold - planted as i32, "one gold each");
    assert!(w.sprites.iter().all(|s| s.index == 0 || s.index == 1 || s.index == 2 || s.index == 3));
    // Planting again finds every spot taken (30 px spacing).
    w.step(&[Command::PlantFields { player: 0, buildings: vec![mill], kind: field }]);
    assert_eq!(w.sprites.len(), 49);
}

#[test]
fn hunters_carry_meat_home() {
    let (mut w, [_, hun, deer, mel, _]) = setup();
    w.spawn(mel, 0, Pos::from_pixels(2048, 2048), 0);
    let h = w.spawn(hun, 0, Pos::from_pixels(2300, 2100), 0);
    let d = w.spawn(deer, 7, Pos::from_pixels(2400, 2100), 0);
    economy::census_all(&mut w);
    let food = w.players[0].resources[FOOD];
    w.step(&[Command::Attack { player: 0, units: vec![h], target: d }]);
    let mut carcass = None;
    for _ in 0..300 {
        w.step(&[]);
        if carcass.is_none() && !w.sprites.is_empty() {
            carcass = Some(w.sprites[0]);
            assert!(!w.units[d.0 as usize].alive, "the body became the carcass");
        }
    }
    let c = carcass.expect("a carcass appeared");
    assert_eq!((c.group, c.index), (SpriteGroup::Meat, 0));
    // 50 meat per cycle: two cycles fill the 90-food portion; the base takes meat (#R8).
    for _ in 0..2000 {
        w.step(&[]);
        if w.players[0].resources[FOOD] > food {
            break;
        }
    }
    assert!(w.players[0].resources[FOOD] >= food + 50, "{} -> {}", food, w.players[0].resources[FOOD]);
}
