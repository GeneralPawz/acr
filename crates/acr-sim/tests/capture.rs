//! Capture (`TryCapture` 0x528868): undefended `CAPTURE` objects change sides when an enemy
//! stands within 250 px.

use std::sync::Arc;

use acr_formats::defs::UnitDef;
use acr_sim::catalog::{Catalog, ObjectType};
use acr_sim::formation::Formations;
use acr_sim::map::MapGrid;
use acr_sim::world::{EntityId, Order};
use acr_sim::{Pos, TypeId, World};

const PEASANT: &str = "NAME Kri\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n\
    @MOTION_R 9 0 10 19\r\nLIFE 50\r\nGEOMETRY 1 10 40\r\nPEASANT\r\nUSAGE PEASANT\r\nCAPTURE\r\n\
    MATHERIAL 1 BODY\r\n[END]\r\n";
const SOLDIER: &str = "NAME Sol\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n\
    @MOTION_R 9 0 10 19\r\nLIFE 100\r\nGEOMETRY 1 10 40\r\nMATHERIAL 1 BODY\r\n[END]\r\n";
const SITE: &str = "NAME Hou\r\nBUILDING\r\nLIFE 1000\r\nLOCATION 0 0 64 64\r\n\
    LOCKPOINTS 4 0 0 1 0 0 1 1 1\r\nBUILDSTAGES 10\r\nMATHERIAL 1 BUILDING\r\n[END]\r\n";

struct Kinds {
    peasant: TypeId,
    soldier: TypeId,
    animal: TypeId,
    site: TypeId,
}

fn setup() -> (World, Kinds) {
    let mut c = Catalog::new();
    let peasant = c.add(ObjectType::from_def("Kri(EN)", &UnitDef::parse(PEASANT.as_bytes())));
    let soldier = c.add(ObjectType::from_def("Sol(EN)", &UnitDef::parse(SOLDIER.as_bytes())));
    let animal = c.add(ObjectType::from_def(
        "Anm(NW)",
        &UnitDef::parse(SOLDIER.replace("[END]", "CANTCAPTURE\r\n[END]").as_bytes()),
    ));
    let site = c.add(ObjectType::from_def("Hou(EN)", &UnitDef::parse(SITE.as_bytes())));
    let w = World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default()));
    (w, Kinds { peasant, soldier, animal, site })
}

fn owner(w: &World, id: EntityId) -> u8 {
    w.units[id.0 as usize].owner
}

/// Runs the 32 ticks in which every object is looked at once; returns the captures seen.
fn run32(w: &mut World) -> Vec<EntityId> {
    let mut seen = Vec::new();
    for _ in 0..32 {
        w.step(&[]);
        seen.extend(w.captured.iter().copied());
    }
    seen
}

/// An enemy 200 px away and no friend near: the peasant goes over in the tick of its index
/// (`index & 31 == tmtmt & 31`), with the new mask and without its orders.
#[test]
fn an_undefended_peasant_is_captured() {
    let (mut w, k) = setup();
    let p = w.spawn(k.peasant, 1, Pos::from_pixels(2000, 2000), 0);
    w.spawn(k.soldier, 0, Pos::from_pixels(2200, 2000), 0);
    w.units[p.0 as usize].orders.push(Order::SendTo { cell: (130, 125), prio: 16 });
    let mut tick = None;
    for _ in 0..32 {
        let t = w.tick.0;
        w.step(&[]);
        if owner(&w, p) == 0 {
            tick = Some(t);
            assert_eq!(w.captured, [p]);
            break;
        }
    }
    assert_eq!(tick, Some(31), "peasant 0 is looked at when (tick + 1) & 31 == 0");
    let u = &w.units[p.0 as usize];
    assert_eq!(u.nmask, w.nation_mask[0]);
    assert!(u.orders.is_empty(), "a captured peasant forgets its orders");
}

/// A friend anywhere in the 7 × 7 cells defends; an enemy 260 px away is too far; animals
/// (`CANTCAPTURE`) and objects of nation 7 never capture; the capture rule 1 spares
/// peasants.
#[test]
fn defenders_distance_and_rules_prevent_capture() {
    type Add = fn(&mut World, &Kinds);
    let cases: [(&str, Add); 5] = [
        ("a friend 350 px away", |w, k| {
            w.spawn(k.soldier, 0, Pos::from_pixels(2200, 2000), 0);
            w.spawn(k.soldier, 1, Pos::from_pixels(2000, 2350), 0);
        }),
        ("an enemy 260 px away", |w, k| {
            w.spawn(k.soldier, 0, Pos::from_pixels(2260, 2000), 0);
        }),
        ("an animal", |w, k| {
            w.spawn(k.animal, 0, Pos::from_pixels(2200, 2000), 0);
        }),
        ("nation 7", |w, k| {
            w.spawn(k.soldier, 7, Pos::from_pixels(2200, 2000), 0);
        }),
        ("capture rule 1", |w, k| {
            w.capture_rule = 1;
            w.spawn(k.soldier, 0, Pos::from_pixels(2200, 2000), 0);
        }),
    ];
    for (what, add) in cases {
        let (mut w, k) = setup();
        let p = w.spawn(k.peasant, 1, Pos::from_pixels(2000, 2000), 0);
        add(&mut w, &k);
        assert!(run32(&mut w).is_empty(), "{what}");
        assert_eq!(owner(&w, p), 1, "{what}");
    }
    // A friend 600 px away is outside the 7 × 7 cells.
    let (mut w, k) = setup();
    let p = w.spawn(k.peasant, 1, Pos::from_pixels(2000, 2000), 0);
    w.spawn(k.soldier, 0, Pos::from_pixels(2200, 2000), 0);
    w.spawn(k.soldier, 1, Pos::from_pixels(2000, 2600), 0);
    assert_eq!(run32(&mut w), [p]);
}

/// A building site nobody works on (not `Ready`, no order) is taken like a `CAPTURE` object;
/// a finished building of the same type is not.
#[test]
fn an_idle_building_site_is_captured() {
    let (mut w, k) = setup();
    let site = w.spawn(k.site, 1, Pos::from_pixels(2000, 2000), 0);
    let done = w.spawn(k.site, 1, Pos::from_pixels(2600, 2000), 0);
    w.units[site.0 as usize].stage = 4;
    w.spawn(k.soldier, 0, Pos::from_pixels(2150, 2000), 0);
    w.spawn(k.soldier, 0, Pos::from_pixels(2750, 2000), 0);
    assert_eq!(run32(&mut w), [site]);
    assert_eq!(owner(&w, done), 1);
    assert_eq!(w.units[site.0 as usize].stage, 4, "the same share of construction");
}
