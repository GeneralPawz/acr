//! `mission-run`, `mission-api`: run original mission scripts headless on the simulation.

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use acr_formats::{M3dMap, Vfs};
use acr_mission::{MissionEvent, ScriptSetup, Scripts};
use acr_sim::World;

use crate::{Command, Result};

pub const COMMANDS: &[Command] = &[
    Command {
        name: "mission-run",
        usage: "<game-dir> <mission-id|map.m3d|script.dll> [--ticks N] [--trace] [--ai] [--units]   \
                (run a mission with its script headless, print the event log)",
        run: |a| match a {
            [dir, mission, rest @ ..] => mission_run(Path::new(dir), mission, rest),
            _ => Err(crate::WRONG_ARGS.into()),
        },
    },
    Command {
        name: "mission-api",
        usage: "   (markdown coverage table of the script API implementation)",
        run: |a| match a {
            [] => mission_api(),
            _ => Err(crate::WRONG_ARGS.into()),
        },
    },
];

/// A mission to run: map, script and text file.
struct MissionFiles {
    map: String,
    script: String,
    texts: Option<String>,
}

/// Resolves a mission id (`#columb1@`), a map path or a script path through `Missions.txt`.
fn resolve(vfs: &mut Vfs, what: &str) -> Result<MissionFiles> {
    let list = vfs.read("Missions\\Missions.txt").unwrap_or_default();
    let missions = acr_formats::missions::parse_missions(&list);
    let norm = acr_formats::normalize_path(what);
    let found = missions.iter().find(|m| {
        m.id.eq_ignore_ascii_case(what)
            || acr_formats::normalize_path(&m.map) == norm
            || acr_formats::normalize_path(&m.script) == norm
    });
    if let Some(m) = found {
        return Ok(MissionFiles { map: m.map.clone(), script: m.script.clone(), texts: Some(m.description.clone()) });
    }
    if norm.ends_with(".M3D") {
        let m3d = M3dMap::parse(&vfs.read(what)?)?;
        let script = m3d.mission_dll.filter(|s| !s.is_empty()).ok_or("the map names no mission script")?;
        return Ok(MissionFiles { map: what.to_owned(), script, texts: None });
    }
    Err(format!("{what}: not a mission id, map or script listed in Missions\\Missions.txt").into())
}

fn mission_run(dir: &Path, what: &str, rest: &[&str]) -> Result<()> {
    let mut ticks: u64 = 3000;
    let mut trace = false;
    let mut ai = false;
    let mut units = false;
    let mut it = rest.iter();
    while let Some(a) = it.next() {
        match *a {
            "--ticks" => ticks = it.next().ok_or("--ticks needs a number")?.parse()?,
            "--trace" => trace = true,
            "--ai" => ai = true,
            "--units" => units = true,
            _ => return Err(crate::WRONG_ARGS.into()),
        }
    }
    let mut vfs = Vfs::open_game_dir(dir)?;
    let files = resolve(&mut vfs, what)?;
    println!("map {}  script {}  texts {}", files.map, files.script, files.texts.as_deref().unwrap_or("-"));
    let data = crate::sim::load_sim_data(&mut vfs);
    let m3d = M3dMap::parse(&vfs.read(&files.map)?)?;
    let (mut world, report) = World::from_map(&m3d, &data.shapes, Arc::new(data.catalog), Arc::new(data.formations));
    if !report.unknown_types.is_empty() {
        println!("  {} unknown unit types", report.unknown_types.len());
    }
    let texts = files.texts.as_deref().and_then(|t| vfs.read(t).ok()).unwrap_or_default();
    let setup = ScriptSetup::default();
    let mut scripts = Scripts::new(&m3d, &world, &texts, setup);
    scripts.set_strings(load_strings(&mut vfs));
    scripts.trace = trace;
    let dll = vfs.read(&files.script)?;
    let t0 = Instant::now();
    if let Err(e) = scripts.load_mission(&mut world, &dll, &files.script) {
        println!("load failed: {e}");
    }
    let mut wanted: Vec<(u8, String)> = Vec::new();
    if ai {
        let nations = vfs.read("NATIONS.LST").unwrap_or_default();
        for n in 1..7u8 {
            if let Some(path) = scripts.ai_dll_for(&world, n, &nations) {
                wanted.push((n, path));
            }
        }
    }
    load_ais(&mut vfs, &mut scripts, &mut world, wanted);
    print_events(0, &mut scripts);
    print_counts("start", &world);
    if units {
        print_types(&world);
    }
    let mut ended = false;
    for _ in 0..ticks {
        let requests = scripts.ai_requests();
        load_ais(&mut vfs, &mut scripts, &mut world, requests);
        let commands = scripts.tick(&mut world);
        if scripts.waiting() {
            // Headless: answer every question with OK (1) and rerun the tick.
            print_events(world.tick.0, &mut scripts);
            scripts.answer(1);
            continue;
        }
        world.step(&commands);
        let tick = world.tick.0;
        for e in print_events(tick, &mut scripts) {
            if matches!(e, MissionEvent::Victory | MissionEvent::Defeat) {
                ended = true;
            }
        }
        if tick % 1000 == 0 {
            print_counts(&format!("tick {tick}"), &world);
        }
        if ended {
            break;
        }
    }
    print_counts("end", &world);
    let elapsed = t0.elapsed();
    println!(
        "{} ticks in {:.2?}; script instructions {}; API calls {}",
        world.tick.0,
        elapsed,
        scripts.instructions(),
        scripts.state().calls.values().sum::<u64>()
    );
    let mut calls: Vec<(&str, u64)> = scripts.state().calls.iter().map(|(n, c)| (*n, *c)).collect();
    calls.sort_by_key(|c| std::cmp::Reverse(c.1));
    let top: Vec<String> = calls.iter().take(25).map(|(n, c)| format!("{n} {c}")).collect();
    println!("most called: {}", top.join(", "));
    let stubs: Vec<String> = calls
        .iter()
        .filter(|(n, _)| {
            acr_mission::api::function(n).is_some_and(|f| matches!(f.status, acr_mission::api::Status::Stub(_)))
        })
        .map(|(n, c)| format!("{n} {c}"))
        .collect();
    if !stubs.is_empty() {
        println!("stubs called: {}", stubs.join(", "));
    }
    for e in scripts.errors() {
        println!("script error: {e}");
    }
    Ok(())
}

/// The interface texts (`GetTextByID`) in the original's file order.
fn load_strings(vfs: &mut Vfs) -> acr_formats::string_table::Texts {
    let paths: Vec<String> = vfs.list().map(|(p, _)| acr_formats::normalize_path(p)).collect();
    let mut t = acr_formats::string_table::Texts::default();
    for p in acr_formats::string_table::Texts::sources(&paths) {
        if let Ok(b) = vfs.read(&p) {
            t.add(&b);
        }
    }
    t
}

/// Loads the AI DLLs for `(nation, path)` pairs, printing failures.
fn load_ais(vfs: &mut Vfs, scripts: &mut Scripts, world: &mut World, wanted: Vec<(u8, String)>) {
    for (nation, path) in wanted {
        let r = vfs.read(&path).map_err(|e| e.to_string()).and_then(|b| scripts.load_ai(world, nation, &b, &path));
        match r {
            Ok(()) => println!("AI for nation {nation}: {path}"),
            Err(e) => println!("AI for nation {nation} ({path}): {e}"),
        }
    }
}

fn print_events(tick: u64, scripts: &mut Scripts) -> Vec<MissionEvent> {
    let events = scripts.take_events();
    for e in &events {
        println!("[{tick:>6}] {}", acr_mission::describe(e));
    }
    for line in scripts.take_trace() {
        println!("[{tick:>6}]   {line}");
    }
    events
}

/// Object types per player.
fn print_types(world: &World) {
    for p in 0..8u8 {
        let mut kinds: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
        for u in world.units.iter().filter(|u| u.living() && u.owner == p) {
            *kinds.entry(&world.catalog.get(u.kind).id).or_default() += 1;
        }
        if !kinds.is_empty() {
            let list: Vec<String> = kinds.iter().map(|(k, n)| format!("{k} {n}")).collect();
            println!("  P{p}: {}", list.join(", "));
        }
    }
}

/// Units per player: alive units and buildings.
fn print_counts(label: &str, world: &World) {
    let mut parts = Vec::new();
    for p in 0..8u8 {
        let units = world.units.iter().filter(|u| u.living() && u.owner == p && !u.building).count();
        let buildings = world.units.iter().filter(|u| u.living() && u.owner == p && u.building).count();
        if units + buildings > 0 {
            parts.push(format!("P{p} {units}u/{buildings}b"));
        }
    }
    println!("  {label}: {}", parts.join("  "));
}

/// Prints the coverage table of `docs/script-runtime.md`.
fn mission_api() -> Result<()> {
    use acr_mission::api::{Status, functions};
    let all: Vec<_> = functions().collect();
    let count = |f: fn(&Status) -> bool| all.iter().filter(|a| f(&a.status)).count();
    println!(
        "{} functions: {} done, {} partial, {} stubbed\n",
        all.len(),
        count(|s| matches!(s, Status::Done)),
        count(|s| matches!(s, Status::Partial(_))),
        count(|s| matches!(s, Status::Stub(_)))
    );
    println!("| function | original | status | notes |\n|---|---|---|---|");
    for f in all {
        let (status, note) = match f.status {
            Status::Done => ("done", ""),
            Status::Partial(n) => ("partial", n),
            Status::Stub(n) => ("stub", n),
        };
        let addr = if f.original == 0 { "DipServer".to_owned() } else { format!("{:#x}", f.original) };
        println!("| `{}` | {addr} | {status} | {note} |", f.name);
    }
    Ok(())
}
