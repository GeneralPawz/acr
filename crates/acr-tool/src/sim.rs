//! `sim-check`: runs the simulation headless on a real map and checks determinism.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use acr_formats::defs::{NationDef, OrderGroups, OrderList, SpriteResources, UnitDef, name_list};
use acr_formats::m3d::SpriteGroup;
use acr_formats::sprite_list::SpriteList;
use acr_formats::{M3dMap, Vfs, normalize_path};
use acr_sim::catalog::{Catalog, ObjectType};
use acr_sim::formation::Formations;
use acr_sim::map::SpriteShapes;
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

/// Builds the object catalog: every member of every nation file, with its `.MD` definition.
pub fn load_catalog(vfs: &mut Vfs) -> Result<Catalog> {
    let mut defs: BTreeMap<String, UnitDef> = BTreeMap::new();
    for name in name_list(&vfs.read("NMLIST.TXT")?) {
        let file = format!("{}.MD", name.trim_end_matches(".MD").trim_end_matches(".md"));
        if let Ok(data) = vfs.read(&file) {
            let def = UnitDef::parse(&data);
            if let Some(n) = def.name() {
                defs.insert(n.to_ascii_uppercase(), def);
            }
        }
    }
    let mut nds: Vec<String> = vfs
        .list()
        .map(|(p, _)| p.to_owned())
        .filter(|p| {
            let n = normalize_path(p);
            n.ends_with(".NDS") && n != "WEAPON.NDS"
        })
        .collect();
    nds.sort();
    let mut catalog = Catalog::new();
    for path in nds {
        let nation = NationDef::parse(&vfs.read(&path)?);
        for m in &nation.members {
            if let Some(def) = defs.get(&m.md.to_ascii_uppercase()) {
                catalog.add(ObjectType::from_def(&m.id, def));
            }
        }
    }
    Ok(catalog)
}

/// Blocking polygons of the map sprite lists (`*.LST` + `*.RSR`).
pub fn load_sprite_shapes(vfs: &mut Vfs) -> Result<SpriteShapes> {
    let mut shapes = SpriteShapes::default();
    let lists = [
        (SpriteGroup::Trees, "TREELIST"),
        (SpriteGroup::Stones, "STONLIST"),
        (SpriteGroup::Holes, "HOLELIST"),
        (SpriteGroup::Complex, "COMPLEX"),
        (SpriteGroup::Meat, "MEAT"),
    ];
    for (group, name) in lists {
        let text = String::from_utf8_lossy(&vfs.read(&format!("{name}.LST"))?).into_owned();
        let list = SpriteList::parse(&text).ok_or_else(|| format!("{name}.LST: bad header"))?;
        let rsr = vfs.read(&format!("{name}.RSR")).map(|d| SpriteResources::parse(&d)).unwrap_or_default();
        shapes.add(group, &list, &rsr);
    }
    Ok(shapes)
}

pub fn load_formations(vfs: &mut Vfs) -> Result<Formations> {
    let orders = OrderList::parse(&vfs.read("ORDERS.LST")?);
    let groups = OrderGroups::parse(&vfs.read("ORD_GROUPS.LST")?);
    if !orders.issues.is_empty() || !groups.issues.is_empty() {
        eprintln!("formation issues: {:?} {:?}", orders.issues, groups.issues);
    }
    Ok(Formations::new(&orders, &groups))
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
    let catalog = Arc::new(load_catalog(&mut vfs)?);
    let formations = Arc::new(load_formations(&mut vfs)?);
    let sprites = load_sprite_shapes(&mut vfs)?;
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
