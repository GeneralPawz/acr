//! Projectiles on a synthetic flat map: an arrow flies for a while and hits, a cannon ball
//! flies an arc and bounces, a musket ball is a trace.

use std::sync::Arc;

use acr_formats::defs::UnitDef;
use acr_formats::defs::nres::weapon_kinds;
use acr_sim::catalog::{Catalog, ObjectType};
use acr_sim::formation::Formations;
use acr_sim::map::MapGrid;
use acr_sim::weapon::Weapons;
use acr_sim::world::EntityId;
use acr_sim::{Command, Pos, TypeId, World};

const ARCHER: &str = "NAME Arc\r\nUSERLC 0 G SHADOW -10 -20\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n\
    @MOTION_R 9 0 10 19\r\n@PATTACK1 9 0 0 9\r\n@UATTACK1 9 0 9 0\r\n@ATTACK1 9 0 0 19\r\n\
    SETACTIVEPOINT #ATTACK1 6 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18\r\n@DEATH 9 0 0 9\r\n\
    LIFE 100\r\nGEOMETRY 1 10 40\r\nDAMAGE 1 30\r\nATTACK_RADIUS 1 60 600\r\nDET_RADIUS 1 60 7000\r\n\
    ATTACK_PAUSE 1 40\r\nWEAPONKIND 1 STRELA\r\nWEAPON 1 STRELA\r\nZPOINTS 20 20\r\nMATHERIAL 1 BODY\r\n\
    CANKILL 1 BODY\r\n[END]\r\n";
const GUNNER: &str = "NAME Gun\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 16 1 0 0\r\n@MOTION_L 16 0 0 9\r\n\
    @MOTION_R 16 0 10 19\r\n@ATTACK1 16 0 0 19\r\n\
    @PATTACK1 16 0 0 9\r\n@DEATH 16 0 0 9\r\nLIFE 1000\r\nGEOMETRY 1 10 40\r\nDAMAGE 1 340\r\n\
    ATTACK_RADIUS 1 100 2000\r\nDET_RADIUS 1 100 7000\r\nATTACK_PAUSE 1 400\r\nWEAPONKIND 1 IADRO\r\n\
    WEAPON 1 PPOINTT\r\nMATHERIAL 1 BODY\r\nCANKILL 1 BODY\r\n[END]\r\n";
const TARGET: &str = "NAME Tgt\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@DEATH 9 0 0 9\r\n\
    #DEATHLIE1 9 1 0 9\r\nLIFE 1000\r\nGEOMETRY 1 10 40\r\nMATHERIAL 1 BODY\r\nUNITRADIUS 16\r\n[END]\r\n";

const NDS: &[u8] = b"[MEMBERS]\r\n\
    STRELA #STRELA 25 15 NONE HI_GRAVITY FLY 25 -1\r\n\
    POINT #POINT 0 1 NONE HI_GRAVITY1 RANDOM 3 -1\r\n\
    PPOINTT #POINT 0 0 NONE NO_GRAVITY SLIGHTUP 0 1\r\n\
    DIM32_2 #DIM32 1 20 WHITE NO_GRAVITY SLIGHTUP 0 1\r\n\
    NUCL2P #NUCL1 340 35 NONE HI_GRAVITY ANGLE 10 -1\r\n\
    SKOKAT #NUCL1 500 16 NONE HI_GRAVITY REFLECT 32 -1\r\n\
    [CHILDWEAPON]\r\nNUCL2P 1 1 0 4 SKOKAT SKOKAT SKOKAT SKOKAT\r\n\
    [FULLPARENT]\r\nSTRELA\r\n\
    [CUSTOMEXPLOSION]\r\nSTRELA FLASH 1 1 0 1 POINT\r\n\
    [SYNCWEAPON]\r\nPPOINTT 2 DIM32_2 NUCL2P\r\n\
    [DAMAGEHEIGHT]\r\nSKOKAT 95\r\n[END]\r\n";
const ADS: &[u8] = b"@STRELA 1 0 0 0\r\n@POINT 1 0 0 0\r\n@NUCL1 1 0 0 0\r\n@DIM32 1 0 0 43\r\n";
const NRES: &[u8] = b"[WEAPONS]\r\n#GENSHIELD 13\r\nMECH 21 19 O\r\nSTRELA 1 14 HRB\r\nPIKA 11 17 O\r\nIADRO 2 15 HX\r\n[UNITKINDS]\r\n";

fn setup() -> (World, TypeId, TypeId, TypeId) {
    let mut c = Catalog::new();
    let arc = c.add(ObjectType::from_def("Arc(EN)", &UnitDef::parse(ARCHER.as_bytes())));
    let gun = c.add(ObjectType::from_def("Gun(EN)", &UnitDef::parse(GUNNER.as_bytes())));
    let tgt = c.add(ObjectType::from_def("Tgt(EN)", &UnitDef::parse(TARGET.as_bytes())));
    c.set_weapon_kinds(&weapon_kinds(NRES));
    c.set_weapons(Weapons::parse(NDS, Some(ADS), None));
    (World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default())), arc, gun, tgt)
}

fn life(w: &World, id: EntityId) -> i32 {
    w.units[id.0 as usize].life
}

#[test]
fn an_arrow_flies_and_hits() {
    let (mut w, arc, _, tgt) = setup();
    let a = w.spawn(arc, 0, Pos::from_pixels(2000, 2000), 0);
    let t = w.spawn(tgt, 1, Pos::from_pixels(2400, 2000), 128);
    w.step(&[Command::Attack { player: 0, units: vec![a], target: t }]);
    let (mut launched, mut max_flight) = (None, 0);
    for tick in 0..200 {
        w.step(&[]);
        let n = w.projectiles.iter().count();
        if n > 0 && launched.is_none() {
            launched = Some(tick);
            let p = w.projectiles.iter().next().unwrap();
            // The arrow leaves at the muzzle: the SETACTIVEPOINT point east (mirrored) plus
            // the sprite offset, not the unit's centre.
            assert_ne!(p.pos.0 >> 14, 2000, "muzzle offset applied");
        }
        if n > 0 {
            max_flight += 1;
        }
        if life(&w, t) < 1000 {
            break;
        }
    }
    assert!(launched.is_some(), "the archer shot");
    // 400 px at 50 px per tick: about 8 ticks in the air.
    assert!((6..=10).contains(&max_flight), "flight {max_flight} ticks");
    assert_eq!(life(&w, t), 1000 - 30, "the arrow's round damage is the archer's DAMAGE");
}

#[test]
fn a_cannon_ball_flies_an_arc_and_bounces() {
    let (mut w, _, gun, tgt) = setup();
    let g = w.spawn(gun, 0, Pos::from_pixels(1500, 2000), 0);
    let t = w.spawn(tgt, 1, Pos::from_pixels(2600, 2000), 128);
    w.step(&[Command::Attack { player: 0, units: vec![g], target: t }]);
    let (mut max_z, mut bounced, mut hit) = (0, false, false);
    for _ in 0..300 {
        w.step(&[]);
        for p in w.projectiles.iter() {
            let wp = w.catalog.weapons.get(p.weapon);
            if wp.name == "NUCL2P" {
                max_z = max_z.max(p.pos.2 >> 14);
            }
            bounced |= wp.name == "SKOKAT";
        }
        hit |= life(&w, t) < 1000;
    }
    // 1100 px at 10 degrees: the arc peaks about D·tan(10°)/4 ≈ 50 px up.
    assert!((30..=120).contains(&max_z), "peak {max_z} px");
    assert!(bounced, "the ball bounced on");
    assert!(hit, "and hurt the target");
}

#[test]
fn projectiles_are_deterministic() {
    let run = || {
        let (mut w, arc, gun, tgt) = setup();
        let mut cmds = Vec::new();
        for k in 0..6 {
            let a = w.spawn(arc, 0, Pos::from_pixels(1800, 1800 + 60 * k), 0);
            let g = w.spawn(gun, 0, Pos::from_pixels(1400, 1800 + 60 * k), 0);
            let t = w.spawn(tgt, 1, Pos::from_pixels(2300, 1800 + 60 * k), 128);
            cmds.push(Command::Attack { player: 0, units: vec![a, g], target: t });
        }
        w.step(&cmds);
        for _ in 0..600 {
            w.step(&[]);
        }
        w.state_hash()
    };
    assert_eq!(run(), run());
}

const TOWER: &str = "NAME Tow\r\nBUILDING\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 1 1 0 0\r\nLIFE 5000\r\n\
    LOCATION -32 -32 64 64\r\nLOCKPOINTS 1 0 0\r\nUNITABSORBER 20\r\nEXDAMAGE 0 20 145\r\n\
    ATTACK_RADIUS 0 200 1000\r\nATTACK_PAUSE 0 60\r\nWEAPONKIND 0 VISTREL\r\nWEAPON 0 SHOTFOG\r\n\
    SEARCH_ENEMY_RADIUS 1200\r\nCANKILL 1 BODY\r\nMATHERIAL 1 BUILDING\r\n\
    EX_SHOTS2 1 40 10 30 200 1000 0 64 50 0 0\r\n[END]\r\n";
const SHOOTER: &str = "NAME Sho\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\nLIFE 100\r\nGEOMETRY 1 10 40\r\n\
    USAGE STRELOK\r\nEVROUNIT\r\nMATHERIAL 1 BODY\r\n[END]\r\n";
const MUSKET_NDS: &[u8] = b"[MEMBERS]\r\nSHOTFOG #SHOTFOG 0 0 NONE NO_GRAVITY STAND 1 1\r\n\
    ENDSHOT #POINT 0 10 NONE NO_GRAVITY FLY 1500 -1\r\n[CHILDWEAPON]\r\nSHOTFOG 1 1 255 1 ENDSHOT\r\n\
    [FULLPARENT]\r\nENDSHOT\r\nSHOTFOG\r\n[END]\r\n";

#[test]
fn a_garrisoned_tower_shoots_and_an_empty_one_does_not() {
    let mut c = Catalog::new();
    let tow = c.add(ObjectType::from_def("Tow(EN)", &UnitDef::parse(TOWER.as_bytes())));
    let sho = c.add(ObjectType::from_def("Sho(EN)", &UnitDef::parse(SHOOTER.as_bytes())));
    let tgt = c.add(ObjectType::from_def("Tgt(EN)", &UnitDef::parse(TARGET.as_bytes())));
    c.set_weapon_kinds(&weapon_kinds(b"[WEAPONS]\r\nMECH 21 19 O\r\nVISTREL 12 18 HRB\r\n[UNITKINDS]\r\n"));
    c.set_weapons(Weapons::parse(MUSKET_NDS, Some(b"#SHOTFOG 1 8 0 0 0 1 0 2 0 3 0 4 0 5 0 6 0 7\r\n"), None));
    let mut w = World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default()));
    let t = w.spawn(tow, 0, Pos::from_pixels(2000, 2000), 0);
    let e = w.spawn(tgt, 1, Pos::from_pixels(2500, 2000), 128);
    for _ in 0..100 {
        w.step(&[]);
    }
    assert_eq!(life(&w, e), 1000, "an empty tower does not shoot");
    let s = w.spawn(sho, 0, Pos::from_pixels(1900, 2000), 0);
    let su = &mut w.units[s.0 as usize];
    su.inside_of = Some(t);
    let cell = su.cell;
    w.occupancy.clr_bar(cell.0, cell.1, 1);
    w.units[t.0 as usize].inside.push(s);
    let mut shots = Vec::new();
    for tick in 0..120 {
        let before = life(&w, e);
        w.step(&[]);
        if life(&w, e) < before {
            shots.push((tick, before - life(&w, e)));
        }
    }
    // One shooter inside: 20 + 125·1/20 = 26 per shot (no protection), every
    // ceil(60/4) = 15 ticks once the point's own reload (50, two per tick) is over.
    assert!(shots.len() >= 3, "shots {shots:?}");
    assert!(shots.iter().all(|s| s.1 == 26), "{shots:?}");
    let gaps: Vec<i32> = shots.windows(2).map(|p| p[1].0 - p[0].0).collect();
    assert!(gaps.iter().all(|&g| g == 25), "the point's reload paces the shots: {gaps:?}");
}

const CANNON: &str = "NAME Can\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 16 1 0 0\r\n@MOTION_L 16 0 0 9\r\n\
    @MOTION_R 16 0 10 19\r\n@MOTION_L0 16 0 0 9\r\n@MOTION_R0 16 0 10 19\r\n@MOTION_L1 16 0 0 19\r\n\
    @MOTION_R1 16 0 0 19\r\n@PATTACK1 16 0 0 9\r\n@UATTACK1 16 0 9 0\r\n@PATTACK4 16 0 0 19\r\n\
    @UATTACK4 16 0 19 0\r\n@ATTACK4 16 0 0 39\r\n@PATTACK5 16 0 0 19\r\n@UATTACK5 16 0 19 0\r\n@ATTACK5 16 0 0 39\r\n\
    SETACTIVEPOINT #ATTACK4 10 10 0 10 0 10 0 10 0 10 0 10 0 10 0 10 0 10 0 10 0 10 0 10 0 10 0 10 0 10 0 10 0\r\n\
    SETACTIVEPOINT #ATTACK5 10 10 0 10 0 10 0 10 0 10 0 10 0 10 0 10 0 10 0 10 0 10 0 10 0 10 0 10 0 10 0 10 0\r\n\
    LIFE 1500\r\nGEOMETRY 1 10 40\r\nMOTIONSTYLE COMPLEXROTATE\r\nUSAGE PUSHKA\r\nARTPODGOTOVKA\r\n\
    ATTACK_RADIUS 4 300 3000\r\nATTACK_PAUSE 4 800\r\nWEAPON 4 PPOINTT\r\nWEAPONKIND 4 IADRO\r\n\
    ATTACK_RADIUS 5 0 300\r\nATTACK_PAUSE 5 800\r\nWEAPON 5 PPOINTT\r\nWEAPONKIND 5 IADRO\r\n\
    WEAPONINSECTOR 5 20 300 400 1 32 50\r\nRASTRATA_NA_VISTREL2 4 5 2 IRON 50 COAL 100\r\n\
    SEARCH_ENEMY_RADIUS 900\r\nCANKILL 1 BODY\r\nMATHERIAL 1 BODY\r\n[END]\r\n";

#[test]
fn a_cannon_turns_in_steps_bombards_the_ground_and_pays() {
    let mut c = Catalog::new();
    let can = c.add(ObjectType::from_def("Can(EN)", &UnitDef::parse(CANNON.as_bytes())));
    c.set_weapon_kinds(&weapon_kinds(NRES));
    c.set_weapons(Weapons::parse(NDS, Some(ADS), None));
    let mut w = World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default()));
    let g = w.spawn(can, 0, Pos::from_pixels(2500, 2000), 0);
    let (iron, coal) = (w.players[0].resources[4], w.players[0].resources[5]);
    w.step(&[Command::AttackGround { player: 0, units: vec![g], x: 1500, y: 2000 }]);
    let mut dirs = vec![w.units[g.0 as usize].dir];
    let mut turned_with_anim = false;
    let mut shot = false;
    for _ in 0..600 {
        w.step(&[]);
        let u = &w.units[g.0 as usize];
        if *dirs.last().unwrap() != u.dir {
            dirs.push(u.dir);
        }
        turned_with_anim |= u.anim.name() == "#MOTION_L1" || u.anim.name() == "#MOTION_R1";
        shot |= w.projectiles.iter().count() > 0;
    }
    assert!(turned_with_anim, "the cannon turned with its turn animations");
    assert!(dirs.windows(2).all(|d| d[1].wrapping_sub(d[0]) % 16 == 0 || d[1] == 128), "16ths: {dirs:?}");
    assert_eq!(w.units[g.0 as usize].dir, 128, "facing the point");
    assert!(shot, "and fired");
    assert_eq!((w.players[0].resources[4], w.players[0].resources[5]), (iron - 50, coal - 100), "one shot paid");
}

#[test]
fn a_deployed_cannon_fires_canister_at_a_crowd() {
    let mut c = Catalog::new();
    let can = c.add(ObjectType::from_def("Can(EN)", &UnitDef::parse(CANNON.as_bytes())));
    let tgt = c.add(ObjectType::from_def("Tgt(EN)", &UnitDef::parse(TARGET.as_bytes())));
    c.set_weapon_kinds(&weapon_kinds(NRES));
    c.set_weapons(Weapons::parse(NDS, Some(ADS), None));
    let mut w = World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default()));
    let g = w.spawn(can, 0, Pos::from_pixels(2000, 2000), 0);
    {
        let u = &mut w.units[g.0 as usize];
        (u.stance, u.local_stance) = (6, 6);
    }
    let crowd: Vec<EntityId> = (0..6).map(|k| w.spawn(tgt, 1, Pos::from_pixels(2150, 1950 + 20 * k), 128)).collect();
    for _ in 0..200 {
        w.step(&[]);
    }
    let hurt = crowd.iter().filter(|e| life(&w, **e) < 1000).count();
    assert!(hurt >= 4, "canister hit {hurt} of 6");
}
