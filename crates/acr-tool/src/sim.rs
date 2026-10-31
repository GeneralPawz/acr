//! `sim-check`: runs the simulation headless on a real map and checks determinism.

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use acr_formats::{M3dMap, Vfs};
use acr_sim::assets::{SimAssets, SimData};
use acr_sim::replay::Replay;
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
    let (mut cmds, groups) = orders(&base);
    println!(
        "  {} move orders for {} units, {ticks} ticks",
        cmds.len(),
        groups.iter().map(|g| g.0.len()).sum::<usize>()
    );
    // Economy: every finished building that can produce something queues its first item
    // with a rally point 200 px south.
    let tech = &catalog.tech;
    let (mut produced, mut researched, mut mills) = (0, 0, 0);
    for (b, u) in base.units.iter().enumerate().filter(|(_, u)| u.living() && u.building) {
        let t = catalog.get(u.kind);
        if t.economy.mill
            && let Some(&field) = tech.of(u.kind).and_then(|tt| tt.produce.first())
        {
            cmds.push(Command::PlantFields { player: u.owner, buildings: vec![u.id], kind: field });
            mills += 1;
        } else if let Some(&kind) = acr_sim::economy::producible(&base, b).first() {
            let (x, y) = u.pos.pixels();
            cmds.push(Command::Rally { player: u.owner, building: u.id, x, y: y + 200 });
            cmds.push(Command::Produce { player: u.owner, building: u.id, kind, endless: false });
            produced += 1;
        } else if let Some(&upgrade) = acr_sim::upgrade::available(&base, b).first() {
            cmds.push(Command::Upgrade { player: u.owner, buildings: vec![u.id], upgrade });
            researched += 1;
        }
    }
    // Market: every player sells 1000 wood for gold.
    if catalog.market.is_some() {
        for p in 0..8u8 {
            cmds.push(Command::Trade { player: p, sell: 0, buy: 1, amount: 1000 });
        }
    }
    println!(
        "  tech: {} upgrades, {} access rules; {produced} buildings queue a unit, {researched} research, {mills} mills plant",
        tech.upgrades.len(),
        tech.access.len()
    );

    // Run A records a replay; run B plays back the decoded replay and must reproduce the
    // recorded state hashes.
    let checkpoints = [ticks / 4, ticks / 2, ticks];
    let simulate = |label: &str, input: &Replay, record: bool| -> (World, Vec<u64>, Replay) {
        let mut w = base.clone();
        let mut out = Replay::new(map, w.random.index);
        let mut hashes = Vec::new();
        let t = Instant::now();
        for tick in 0..ticks {
            let commands = if record && tick == 0 { cmds.as_slice() } else { input.commands_at(tick) };
            out.record(tick, commands);
            w.step(commands);
            if checkpoints.contains(&(tick + 1)) {
                hashes.push(w.state_hash());
                out.checkpoints.push((tick + 1, w.state_hash()));
            }
        }
        let el = t.elapsed();
        println!("  run {label}: {:.2?} ({:.0} ticks/s)", el, ticks as f64 / el.as_secs_f64());
        (w, hashes, out)
    };
    let (a, ha, recorded) = simulate("A (recording)", &Replay::default(), true);
    let bytes = recorded.encode();
    let replay = Replay::decode(&bytes)?;
    let (_, hb, _) = simulate("B (replay)", &replay, false);
    println!("  replay: {} bytes, {} command ticks", bytes.len(), replay.entries.len());
    for ((tick, x), y) in checkpoints.iter().zip(&ha).zip(&hb) {
        println!("  tick {tick:>6}: {x:016x} {y:016x} {}", if x == y { "equal" } else { "DIFFERENT" });
    }
    let replay_ok = replay.checkpoints.iter().map(|c| c.1).eq(hb.iter().copied());

    let (mut arrived, mut moving, mut total) = (0, 0, 0);
    let mut far: Vec<(EntityId, i32)> = Vec::new();
    for (ids, (tx, ty)) in &groups {
        for id in ids {
            let Some(u) = a.unit(*id).filter(|u| u.living()) else { continue };
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
        a.units.iter().filter(|u| u.living() && !u.building && catalog.get(u.kind).mobile()).map(|u| u.cell).collect();
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
    let done = a.players.iter().map(|p| p.upgrades.iter().filter(|u| u.done).count()).sum::<usize>();
    let live = |g| a.sprites.iter().filter(|s| !s.removed && s.group == g).count();
    println!(
        "  food: {} fields ({} at the start), {} carcasses",
        live(acr_formats::m3d::SpriteGroup::Complex),
        base.sprites.iter().filter(|s| s.group == acr_formats::m3d::SpriteGroup::Complex).count(),
        live(acr_formats::m3d::SpriteGroup::Meat)
    );
    println!(
        "  economy: {} objects created, {done} upgrades done; gold buys at {:.4}",
        a.units.len() - base.units.len(),
        a.catalog.market.as_ref().and_then(|d| d.slot(1)).map_or(0.0, |s| a.market.buy[s])
    );
    let dead = a.units.iter().filter(|u| u.death != 0 || !u.alive).count();
    let corpses = a.units.iter().filter(|u| u.alive && u.death != 0).count();
    println!("  combat: {dead} units died ({corpses} corpses still lying)");
    let panicking = a.units.iter().filter(|u| u.living() && u.panicking()).count();
    let courage = a.units.iter().filter(|u| u.living() && u.courage != 0).count();
    println!("  morale: {panicking} units panicking at the end, {courage} with courage != 0");
    let burning = a.units.iter().filter(|u| u.living() && u.burning).count();
    println!(
        "  projectiles: {} in flight at the end; {burning} objects burning; {} garrisoned buildings",
        a.projectiles.iter().count(),
        a.units.iter().filter(|u| u.living() && u.building && !u.inside.is_empty()).count()
    );
    let ships: Vec<_> =
        a.units.iter().filter(|u| u.living() && acr_sim::ship::is_ship(a.catalog.get(u.kind))).collect();
    if !ships.is_empty() {
        let moving = ships.iter().filter(|u| u.speed > 0).count();
        let docked = ships.iter().filter(|u| matches!(u.dock, acr_sim::ship::Dock::At(_))).count();
        let fishing = ships.iter().filter(|u| matches!(u.orders.last(), Some(acr_sim::world::Order::Fishing))).count();
        println!("  ships: {} ({moving} under way, {docked} transports docked, {fishing} fishing)", ships.len());
    }
    far.sort_by_key(|f| std::cmp::Reverse(f.1));
    if !far.is_empty() {
        let sample: Vec<String> = far.iter().take(5).map(|(id, d)| format!("#{} {d} px", id.0)).collect();
        println!("  farthest: {}", sample.join(", "));
    }
    if ha != hb || !replay_ok {
        return Err("simulation is not deterministic".into());
    }
    Ok(())
}
