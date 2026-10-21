//! `mapgen`, `mapgen-check`: run the random map generator and compare it with the random
//! maps the original saved (`RN* *.m3d` in the game folder).

use std::path::Path;

use acr_formats::{M3dMap, Vfs};
use acr_mapgen::{Settings, generate};

use crate::{Command, Result};

pub const COMMANDS: &[Command] = &[
    Command {
        name: "mapgen",
        usage: "<game-dir> [--seed N] [--size normal|large|huge] [--players N] [--landscape N] [--relief N] \
                [--resources N] [--minerals N] [--climate N] [--options N] [--name \"RN1 ...\"] [--layer NAME] \
                <out.png>   (generate a random map and render it like m3d-png)",
        run: |a| match a {
            [dir, rest @ .., out] => mapgen(Path::new(dir), rest, Path::new(out)),
            _ => Err(crate::WRONG_ARGS.into()),
        },
    },
    Command {
        name: "mapgen-check",
        usage: "<game-dir>   (regenerate the original's saved random maps and compare)",
        run: |a| match a {
            [dir] => check(Path::new(dir)),
            _ => Err(crate::WRONG_ARGS.into()),
        },
    },
];

fn source(vfs: &mut Vfs) -> impl FnMut(&str) -> Option<Vec<u8>> + '_ {
    |path| vfs.read(path).ok()
}

fn mapgen(dir: &Path, args: &[&str], out: &Path) -> Result<()> {
    let mut s = Settings::from_name("RN0 1 000 00000021 2").ok_or("default settings")?;
    let mut layer = "terrain";
    let mut players = 2;
    let mut it = args.iter();
    while let Some(&flag) = it.next() {
        let value = *it.next().ok_or(crate::WRONG_ARGS)?;
        let n = || value.parse::<u32>();
        match flag {
            "--seed" => s.seed = n()?,
            "--size" => {
                s.size = match value {
                    "normal" => 1,
                    "large" => 2,
                    "huge" => 3,
                    _ => return Err(crate::WRONG_ARGS.into()),
                }
            }
            "--players" => players = n()?.clamp(2, 7),
            "--landscape" => s.landscape = n()? as u8,
            "--relief" => s.relief = n()? as u8,
            "--resources" => s.resources = n()? as u8,
            "--minerals" => s.minerals = n()? as u8,
            "--climate" => s.climate = n()? as u8,
            "--options" => s.options = n()?,
            "--layer" => layer = value,
            "--name" => {
                s = Settings::from_name(value).ok_or("bad map name")?;
                players = 0;
            }
            _ => return Err(crate::WRONG_ARGS.into()),
        }
    }
    if players != 0 {
        s.shape = (players - 2) as u8;
        // Spain, England (USA), France, ... by colour.
        let nations = [0, 2, 8, 4, 1, 6, 10];
        s.nations = [None; 8];
        for (slot, n) in nations.iter().take(players as usize).enumerate() {
            s.nations[slot] = Some(*n);
        }
    }
    let mut vfs = Vfs::open_game_dir(dir)?;
    let started = std::time::Instant::now();
    let g = generate(&s, &mut source(&mut vfs))?;
    eprintln!(
        "{}: {} sprites, {} units, starts {:?} ({:.1} s)",
        s.name(),
        g.map.sprites.len(),
        g.map.units.len(),
        g.starts.iter().flatten().collect::<Vec<_>>(),
        started.elapsed().as_secs_f32()
    );
    let image = crate::maps::render_layer(&mut vfs, &g.map, &s.name(), layer)?;
    crate::write_png(out, &image)?;
    eprintln!("{}x{} -> {}", image.width, image.height, out.display());
    Ok(())
}

/// Share of equal cells.
fn same<T: PartialEq>(a: &[T], b: &[T]) -> f64 {
    let n = a.len().min(b.len()).max(1);
    a.iter().zip(b).filter(|(x, y)| x == y).count() as f64 * 100.0 / n as f64
}

fn check(dir: &Path) -> Result<()> {
    let mut vfs = Vfs::open_game_dir(dir)?;
    let names: Vec<String> = vfs
        .list()
        .filter(|(p, _)| {
            let u = p.to_ascii_uppercase();
            u.starts_with("RN") && u.ends_with(".M3D") && !u.contains('\\') && !u.contains('/')
        })
        .map(|(p, _)| p.to_owned())
        .collect();
    for name in names {
        let Some(settings) = Settings::from_name(&name) else { continue };
        let original = M3dMap::parse(&vfs.read(&name)?)?;
        let g = generate(&settings, &mut source(&mut vfs))?;
        let ours = &g.map;
        println!("{name}");
        let water = |m: &M3dMap| m.water.clone().map(|w| (w.depth.cells, w.brightness.cells)).unwrap_or_default();
        let (wa, wb) = (water(&original), water(ours));
        println!("  water depth {:6.2} %   brightness {:6.2} %", same(&wa.0, &wb.0), same(&wa.1, &wb.1));
        let cells = |g: &Option<acr_formats::m3d::Grid<i16>>| g.as_ref().map(|g| g.cells.clone()).unwrap_or_default();
        println!("  heights     {:6.2} %", same(&cells(&original.heights), &cells(&ours.heights)));
        let tiles = |m: &M3dMap| m.textures.as_ref().map(|g| g.cells.clone()).unwrap_or_default();
        println!("  textures    {:6.2} %", same(&tiles(&original), &tiles(ours)));
        let peace = |m: &M3dMap| m.peace.as_ref().map(|p| p.cells.cells.clone()).unwrap_or_default();
        println!("  territories {:6.2} %", same(&peace(&original), &peace(ours)));
        let ground = |m: &M3dMap| m.ground.as_ref().map(|g| (g.0.cells.clone(), g.1.cells.clone())).unwrap_or_default();
        let (ga, gb) = (ground(&original), ground(ours));
        println!("  ground      {:6.2} % / {:6.2} %", same(&ga.0, &gb.0), same(&ga.1, &gb.1));
        let key = |s: &acr_formats::m3d::Sprite| (s.x, s.y, s.index);
        let a: std::collections::BTreeSet<_> = original.sprites.iter().map(key).collect();
        let b: std::collections::BTreeSet<_> = ours.sprites.iter().map(key).collect();
        println!(
            "  sprites     {} original, {} generated, {} equal, order equal {}",
            a.len(),
            b.len(),
            a.intersection(&b).count(),
            original.sprites.iter().map(key).eq(ours.sprites.iter().map(key))
        );
        let ukey = |u: &acr_formats::m3d::Unit| (u.nation, u.x, u.y, u.name.clone());
        let a: std::collections::BTreeSet<_> = original.units.iter().map(ukey).collect();
        let b: std::collections::BTreeSet<_> = ours.units.iter().map(ukey).collect();
        println!(
            "  units       {} original, {} generated, {} equal, order equal {}",
            a.len(),
            b.len(),
            a.intersection(&b).count(),
            original.units.iter().map(ukey).eq(ours.units.iter().map(ukey))
        );
        let locks = |m: &M3dMap| m.locks.clone().map(|l| (l.locked, l.unlocked)).unwrap_or_default();
        let (la, lb) = (locks(&original), locks(ours));
        println!(
            "  locks       {}/{} original, {}/{} generated, equal {}",
            la.0.len(),
            la.1.len(),
            lb.0.len(),
            lb.1.len(),
            la == lb
        );
        println!("  resources equal {}", original.resources == ours.resources);
    }
    Ok(())
}
