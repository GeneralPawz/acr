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
