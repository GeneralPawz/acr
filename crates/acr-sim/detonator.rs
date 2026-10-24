//! `[DETONATOR]` blasts throw the dead away (`Die` 0x4e86b9 → 0x55a070, flight 0x559f20).

use std::sync::Arc;

use acr_formats::defs::UnitDef;
use acr_sim::catalog::{Catalog, ObjectType};
use acr_sim::formation::Formations;
use acr_sim::map::MapGrid;
use acr_sim::weapon::Weapons;
use acr_sim::{Pos, World};

const VICTIM: &str = "NAME Vic\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@DEATH 9 0 0 9\r\n\
    #DEATHLIE1 9 1 0 9\r\nLIFE 10\r\nGEOMETRY 1 10 40\r\nMATHERIAL 1 BODY\r\n[END]\r\n";

/// A blast of 150 px that explodes where it is created (hot frame 255); `BLAST2` is the same
/// without `[DETONATOR]`.
const NDS: &[u8] = b"[MEMBERS]\r\n\
    BLAST #POINT 500 150 NONE NO_GRAVITY STAND 0 1\r\n\
    BLAST2 #POINT 500 150 NONE NO_GRAVITY STAND 0 1\r\n\
    [CHILDWEAPON]\r\nBLAST 0 0 255 0\r\nBLAST2 0 0 255 0\r\n\
    [DETONATOR]\r\nBLAST 8000\r\n[END]\r\n";
const ADS: &[u8] = b"@POINT 1 0 0 0\r\n";

/// Blows up `weapon` at (2000, 2000) next to a victim 100 px east; returns the world.
fn blast(weapon: &str) -> (World, usize) {
    let mut c = Catalog::new();
    let vic = c.add(ObjectType::from_def("Vic(EN)", &UnitDef::parse(VICTIM.as_bytes())));
    c.set_weapons(Weapons::parse(NDS, Some(ADS), None));
    let wid = c.weapons.find(weapon).expect("weapon");
    let mut w = World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default()));
    let v = w.spawn(vic, 1, Pos::from_pixels(2100, 2000), 0).0 as usize;
    w.step(&[]); // the 128 px cell lists the round damage looks into
    acr_sim::projectile::create(&mut w, wid, (2000, 2000, 1), (2000, 2000, 0), None, 0, None, (0, 0, 0), 0);
    (w, v)
}

/// r = 100, c = 120: vx = vz = 20·8000/120 = 1333 (1/256 px per tick), vy = 0. Heights
/// above the flat ground: 1 px, then 6, 7, 4 and −2 px: down after the fourth step,
/// (2100·256 + 4·1333) / 16 = 33933 in 1/16 px. No random numbers are drawn.
#[test]
fn a_detonator_throws_the_dead_away() {
    let (mut w, v) = blast("BLAST");
    assert!(w.units[v].death != 0, "the blast killed the victim");
    assert_eq!(w.thrown.len(), 1, "and threw it");
    assert_eq!(w.thrown[0].vel, (1333, 0, 1333));
    let random = w.random;
    let mut heights = Vec::new();
    for _ in 0..4 {
        acr_sim::projectile::fly_corpses(&mut w);
        heights.extend(w.thrown.first().map(|t| t.pos.2 >> 8));
    }
    assert_eq!(heights, [6, 7, 4], "in the air for three steps");
    assert!(w.thrown.is_empty(), "landed in the fourth");
    assert_eq!(w.units[v].pos, Pos::new(33_933, 32_000));
    assert_eq!(w.random, random);
}

/// Without `[DETONATOR]` the dead stay where they fell.
#[test]
fn other_blasts_leave_the_dead() {
    let (w, v) = blast("BLAST2");
    assert!(w.units[v].death != 0);
    assert!(w.thrown.is_empty() && w.detonation.is_none());
}
