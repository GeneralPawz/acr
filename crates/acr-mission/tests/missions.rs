//! Runs original missions with their scripts on the simulation. Skipped unless
//! `ACR_GAME_DIR` points to the American Conquest directory.

use std::path::PathBuf;
use std::sync::Arc;

use acr_formats::{M3dMap, Vfs};
use acr_mission::{MissionEvent, ScriptSetup, Scripts};
use acr_sim::World;
use acr_sim::assets::SimAssets;

fn game() -> Option<Vfs> {
    let dir = PathBuf::from(std::env::var_os("ACR_GAME_DIR")?);
    Some(Vfs::open_game_dir(&dir).expect("game directory"))
}

/// Loads a mission of `Missions.txt` and its world.
fn load(vfs: &mut Vfs, id: &str) -> (World, Scripts) {
    let list = vfs.read("Missions\\Missions.txt").unwrap();
    let missions = acr_formats::missions::parse_missions(&list);
    let m = acr_formats::missions::find(&missions, id).expect("mission");
    let paths: Vec<String> = vfs.list().map(|(p, _)| p.to_owned()).collect();
    let mut assets = SimAssets::default();
    let mut queue = assets.start(&paths);
    while let Some(p) = queue.pop() {
        let data = vfs.read(&p).ok();
        queue.extend(assets.add_file(&p, data.as_deref()));
    }
    let data = assets.build();
    let map = M3dMap::parse(&vfs.read(&m.map).unwrap()).unwrap();
    let (mut world, _) = World::from_map(&map, &data.shapes, Arc::new(data.catalog), Arc::new(data.formations));
    let texts = vfs.read(&m.description).unwrap_or_default();
    let mut scripts = Scripts::new(&map, &world, &texts, ScriptSetup::default());
    let dll = vfs.read(&m.script).unwrap();
    scripts.load_mission(&mut world, &dll, &m.script).unwrap();
    (world, scripts)
}

fn run(world: &mut World, scripts: &mut Scripts, ticks: u64) -> Vec<MissionEvent> {
    let mut events = Vec::new();
    for _ in 0..ticks {
        let commands = scripts.tick(world);
        if scripts.waiting() {
            scripts.answer(1);
            continue;
        }
        world.step(&commands);
        events.extend(scripts.take_events());
    }
    events
}

#[test]
fn columbus_1_shows_its_first_page_and_lands_the_natives() {
    let Some(mut vfs) = game() else {
        eprintln!("ACR_GAME_DIR not set, skipping");
        return;
    };
    let (mut world, mut scripts) = load(&mut vfs, "#columb1@");
    let before = world.units.iter().filter(|u| u.owner == 1).count();
    let events = run(&mut world, &mut scripts, 400);
    assert!(scripts.errors().is_empty(), "{:?}", scripts.errors());
    assert!(
        events.iter().any(|e| matches!(e, MissionEvent::Page { id, .. } if id == "#PAGE0")),
        "no #PAGE0 in {events:?}"
    );
    // CreateObject0 spawns the natives of nation 1 in formation.
    assert!(world.units.iter().filter(|u| u.owner == 1).count() > before);
    assert!(!scripts.briefing().is_empty());
}

#[test]
fn scripts_are_deterministic() {
    let Some(mut vfs) = game() else { return };
    let (world, scripts) = load(&mut vfs, "#MISSUSA01@");
    let mut a = (world.clone(), scripts.clone());
    let mut b = (world, scripts);
    run(&mut a.0, &mut a.1, 300);
    run(&mut b.0, &mut b.1, 300);
    assert_eq!(a.0.state_hash(), b.0.state_hash());
    assert_eq!(a.1.instructions(), b.1.instructions());
}

#[test]
fn computer_players_build_on_a_random_map() {
    let Some(mut vfs) = game() else { return };
    let map_path = "RN1 3281 123132 00101111 5002012.m3d";
    let paths: Vec<String> = vfs.list().map(|(p, _)| p.to_owned()).collect();
    let mut assets = SimAssets::default();
    let mut queue = assets.start(&paths);
    while let Some(p) = queue.pop() {
        let data = vfs.read(&p).ok();
        queue.extend(assets.add_file(&p, data.as_deref()));
    }
    let data = assets.build();
    let Ok(bytes) = vfs.read(map_path) else { return };
    let map = M3dMap::parse(&bytes).unwrap();
    let (mut world, _) = World::from_map(&map, &data.shapes, Arc::new(data.catalog), Arc::new(data.formations));
    let mut scripts = Scripts::new(&map, &world, &[], ScriptSetup::default());
    let mut strings = acr_formats::string_table::Texts::default();
    let normalized: Vec<String> = paths.iter().map(|p| acr_formats::normalize_path(p)).collect();
    for p in acr_formats::string_table::Texts::sources(&normalized) {
        strings.add(&vfs.read(&p).unwrap());
    }
    scripts.set_strings(strings);
    let nations = vfs.read("NATIONS.LST").unwrap();
    let buildings = |w: &World, n: u8| w.units.iter().filter(|u| u.living() && u.owner == n && u.building).count();
    let mut ais = Vec::new();
    for n in 1..7u8 {
        if let Some(path) = scripts.ai_dll_for(&world, n, &nations) {
            let dll = vfs.read(&path).unwrap();
            scripts.load_ai(&mut world, n, &dll, &path).unwrap();
            ais.push((n, buildings(&world, n)));
        }
    }
    assert!(!ais.is_empty());
    run(&mut world, &mut scripts, 600);
    assert!(scripts.errors().is_empty(), "{:?}", scripts.errors());
    assert!(ais.iter().any(|&(n, before)| buildings(&world, n) > before), "no computer player built anything");
}

/// The links of the page DipServer wrote last (`{DS|cmd&args}` without `DS|`).
fn dip_links(scripts: &Scripts) -> Vec<String> {
    let page = String::from_utf8_lossy(scripts.script_file("Dip/tmp.cml").unwrap_or_default()).into_owned();
    page.split("{DS|").skip(1).filter_map(|r| r.split('}').next()).map(str::to_owned).collect()
}

#[test]
fn a_tribe_offers_an_alliance_and_sells_its_services() {
    let Some(mut vfs) = game() else { return };
    let map_path = "RN1 3281 123132 00101111 5002012.m3d";
    let paths: Vec<String> = vfs.list().map(|(p, _)| p.to_owned()).collect();
    let mut assets = SimAssets::default();
    let mut queue = assets.start(&paths);
    while let Some(p) = queue.pop() {
        let data = vfs.read(&p).ok();
        queue.extend(assets.add_file(&p, data.as_deref()));
    }
    let data = assets.build();
    let Ok(bytes) = vfs.read(map_path) else { return };
    let map = M3dMap::parse(&bytes).unwrap();
    let (mut world, _) = World::from_map(&map, &data.shapes, Arc::new(data.catalog), Arc::new(data.formations));
    let mut scripts = Scripts::new(&map, &world, &[], ScriptSetup::default());
    let mut strings = acr_formats::string_table::Texts::default();
    let normalized: Vec<String> = paths.iter().map(|p| acr_formats::normalize_path(p)).collect();
    for p in acr_formats::string_table::Texts::sources(&normalized) {
        strings.add(&vfs.read(&p).unwrap());
    }
    scripts.set_strings(strings);
    for p in &paths {
        let n = acr_formats::normalize_path(p);
        if n.starts_with("DIP/") {
            scripts.add_file(p, vfs.read(p).unwrap());
        }
    }
    let dll = vfs.read("DipServer.dll").unwrap();
    scripts.load_diplomacy(&mut world, &dll, "DipServer.dll").unwrap();
    // The settlement of the Delaware (nation 7); ten of player 0's units walk into its zone.
    let hut = world.units.iter().find(|u| u.living() && u.owner == 7 && u.building).unwrap();
    let (hut, x, y) = (hut.id.0 as u16, hut.pos.x, hut.pos.y);
    let ids: Vec<usize> = world
        .units
        .iter()
        .filter(|u| u.living() && u.owner == 0 && !u.building)
        .map(|u| u.id.0 as usize)
        .take(10)
        .collect();
    for (k, i) in ids.into_iter().enumerate() {
        world.units[i].pos.x = x - (300 << 4) + ((20 * k as i32) << 4);
        world.units[i].pos.y = y + (300 << 4);
    }
    // The tribe offers trade: `ActivateDipDialog` with the invitation page.
    let invited =
        (0..40).any(|_| run(&mut world, &mut scripts, 1).iter().any(|e| matches!(e, MissionEvent::Diplomacy { .. })));
    assert!(invited, "no invitation");
    let yes = dip_links(&scripts).into_iter().find(|l| l.starts_with("dipyes")).unwrap_or_else(|| {
        panic!("no yes link: {:?}", String::from_utf8_lossy(scripts.script_file("Dip/tmp.cml").unwrap_or_default()))
    });
    let answer = scripts.dip_request(&mut world, yes.as_bytes());
    assert_eq!(answer.first().map(|c| c.name.as_slice()), Some(&b"LW_file"[..]));
    run(&mut world, &mut scripts, 20);
    // A click on its building now shows its offers.
    scripts.dip_check_building(&mut world, 0, hut);
    assert!(scripts.take_events().iter().any(|e| matches!(e, MissionEvent::Diplomacy { .. })));
    let offers: Vec<String> = dip_links(&scripts).into_iter().filter(|l| l.starts_with("perform")).collect();
    assert!(offers.len() >= 3, "{offers:?}");
    let gold = world.players[0].resources[1];
    scripts.dip_request(&mut world, offers[0].as_bytes());
    run(&mut world, &mut scripts, 20);
    assert!(world.players[0].resources[1] < gold, "the offer was not paid");
    assert!(scripts.errors().is_empty(), "{:?}", scripts.errors());
}
