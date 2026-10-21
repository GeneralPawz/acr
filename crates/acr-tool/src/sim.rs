//! `sim-check`: runs the simulation headless on a real map and checks determinism.

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use acr_formats::{M3dMap, Vfs};
use acr_sim::assets::{SimAssets, SimData};
use acr_sim::world::EntityId;
use acr_sim::{Command, World};

use crate::{Command as ToolCommand, Result};

pub const COMMANDS: &[ToolCommand] = &[ToolCommand {
    name: "sim-check",
    usage: "<game-dir> <map> [ticks]   (run a map's units headless twice, compare state hashes)",
    run: |a| match a {
        [dir, map, rest @ ..] => sim_check(Path::new(dir), map, rest),
        _ => Err(crate::WRONG_ARGS.into()),
    },
}];

/// Reads the simulation's static data through the shared sans-I/O loader.
pub fn load_sim_data(vfs: &mut Vfs) -> SimData {
    let paths: Vec<String> = vfs.list().map(|(p, _)| p.to_owned()).collect();
    let mut assets = SimAssets::default();
    let mut queue = assets.start(&paths);
    while let Some(path) = queue.pop() {
        let data = vfs.read(&path).ok();
        queue.extend(assets.add_file(&path, data.as_deref()));
    }
    assets.build()
}

/// The units of one move order and their target (world pixels).
type MoveGroup = (Vec<EntityId>, (i32, i32));

/// Units per move order.
const GROUP: usize = 12;
/// A unit counts as arrived within this distance (world pixels) of its group's target.
const ARRIVED: i32 = 320;

/// The scripted test: per player, mobile units in groups of 12, each group sent to the
/// centre of the next group, so everybody walks across terrain the map's own units stand on.
fn orders(world: &World) -> (Vec<Command>, Vec<MoveGroup>) {
    let mut cmds = Vec::new();
    let mut groups = Vec::new();
    for p in 0..8u8 {
        let ids: Vec<EntityId> = world
            .units
            .iter()
            .filter(|u| u.alive && u.owner == p && !u.building && world.catalog.get(u.kind).mobile())
            .map(|u| u.id)
            .collect();
        let chunks: Vec<&[EntityId]> = ids.chunks(GROUP).collect();
        if chunks.len() < 2 {
            continue;
        }
        let center = |c: &[EntityId]| {
            let n = c.len() as i64;
            let (sx, sy) = c.iter().fold((0i64, 0i64), |(sx, sy), id| {
                let (x, y) = world.unit(*id).unwrap().pos.pixels();
                (sx + i64::from(x), sy + i64::from(y))
            });
            ((sx / n) as i32, (sy / n) as i32)
        };
        for (i, c) in chunks.iter().enumerate() {
            let target = center(chunks[(i + 1) % chunks.len()]);
            cmds.push(Command::Move { player: p, units: c.to_vec(), x: target.0, y: target.1 });
            groups.push((c.to_vec(), target));
        }
    }
    (cmds, groups)
}

/// `acr sim-check <game-dir> <map> [ticks]` (default 600 ticks = one minute of game time).
pub fn sim_check(dir: &Path, map: &str, rest: &[&str]) -> Result<()> {
    let ticks: u64 = match rest {
        [] => 600,
        [t] => t.parse()?,
        _ => return Err("expected an optional tick count".into()),
    };
    let mut vfs = Vfs::open_game_dir(dir)?;
    let data = load_sim_data(&mut vfs);
    let catalog = Arc::new(data.catalog);
    let formations = Arc::new(data.formations);
    let sprites = data.shapes;
    let m3d = M3dMap::parse(&vfs.read(map)?)?;
    println!(
        "catalog: {} object types; formations: {} shapes, {} groups",
        catalog.len(),
        formations.shapes.len(),
        formations.groups.len()
    );

    let t0 = Instant::now();
    let (base, report) = World::from_map(&m3d, &sprites, catalog.clone(), formations.clone());
    let mobile = base.units.iter().filter(|u| !u.building && catalog.get(u.kind).mobile()).count();
    println!(
        "map {map}: {} units ({} mobile, {} buildings), {} map objects, built in {:.0?}",
        base.units.len(),
        mobile,
        base.units.iter().filter(|u| u.building).count(),
        base.objects.len(),
        t0.elapsed()
    );
    println!(
        "  land: {} of {} lock cells blocked; areas: {}",
        base.map.land.count(),
        base.map.land.width * base.map.land.height,
        base.map.land_areas.as_ref().map_or(0, |a| a.count)
    );
    if !report.unknown_types.is_empty() || report.skipped > 0 {
        let mut unknown = report.unknown_types.clone();
        unknown.sort();
        unknown.dedup();
        println!("  not placed: {} skipped (nation >= 8), unknown types {:?}", report.skipped, unknown);
    }
    let (cmds, groups) = orders(&base);
    println!(
        "  {} move orders for {} units, {ticks} ticks",
        cmds.len(),
        groups.iter().map(|g| g.0.len()).sum::<usize>()
    );

    let checkpoints = [ticks / 4, ticks / 2, ticks];
    let simulate = |label: &str| -> (World, Vec<u64>) {
        let mut w = base.clone();
        let mut hashes = Vec::new();
        let t = Instant::now();
        for tick in 0..ticks {
            w.step(if tick == 0 { &cmds } else { &[] });
            if checkpoints.contains(&(tick + 1)) {
                hashes.push(w.state_hash());
            }
        }
        let el = t.elapsed();
        println!("  run {label}: {:.2?} ({:.0} ticks/s)", el, ticks as f64 / el.as_secs_f64());
        (w, hashes)
    };
    let (a, ha) = simulate("A");
    let (_, hb) = simulate("B");
    for ((tick, x), y) in checkpoints.iter().zip(&ha).zip(&hb) {
        println!("  tick {tick:>6}: {x:016x} {y:016x} {}", if x == y { "equal" } else { "DIFFERENT" });
    }

    let (mut arrived, mut moving, mut total) = (0, 0, 0);
    let mut far: Vec<(EntityId, i32)> = Vec::new();
    for (ids, (tx, ty)) in &groups {
        for id in ids {
            let Some(u) = a.unit(*id) else { continue };
            total += 1;
            let (x, y) = u.pos.pixels();
            let d = (x - tx).abs().max((y - ty).abs());
            if !u.orders.is_empty() || u.in_motion {
                moving += 1;
            }
            if d <= ARRIVED {
                arrived += 1;
            } else {
                far.push((*id, d));
            }
        }
    }
    let mut cells: Vec<(i32, i32)> =
        a.units.iter().filter(|u| u.alive && !u.building && catalog.get(u.kind).mobile()).map(|u| u.cell).collect();
    let n = cells.len();
    cells.sort_unstable();
    cells.dedup();
    let side = a.map.land.width;
    let outside =
        a.units.iter().filter(|u| u.cell.0 < 0 || u.cell.1 < 0 || u.cell.0 >= side || u.cell.1 >= side).count();
    println!(
        "  arrived within {ARRIVED} px: {arrived}/{total} ({:.1} %); still moving: {moving}; shared cells: {}; outside map: {outside}",
        100.0 * f64::from(arrived) / f64::from(total.max(1)),
        n - cells.len()
    );
    far.sort_by_key(|f| std::cmp::Reverse(f.1));
    if !far.is_empty() {
        let sample: Vec<String> = far.iter().take(5).map(|(id, d)| format!("#{} {d} px", id.0)).collect();
        println!("  farthest: {}", sample.join(", "));
    }
    if ha != hb {
        return Err("simulation is not deterministic".into());
    }
    Ok(())
}
