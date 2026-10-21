//! `m3d-check`, `m3d-png`: validate and render the original maps.

use std::collections::BTreeMap;
use std::path::Path;

use acr_formats::m3d::{Grid, SpriteGroup, tag_name};
use acr_formats::raster::Image;
use acr_formats::{M3dMap, Palette, Vfs};

use crate::{Command, Result};

pub const COMMANDS: &[Command] = &[
    Command {
        name: "m3d-check",
        usage: "<game-dir> [filter]",
        run: |a| match a {
            [dir, rest @ ..] => m3d_check(Path::new(dir), rest.first().copied()),
            _ => Err(crate::WRONG_ARGS.into()),
        },
    },
    Command {
        name: "m3d-png",
        usage: "<game-dir> <map-path> <out.png> [--layer NAME]   layers: terrain (default), heights, textures, \
                preview, old-preview, water-depth, water-brightness, land-areas, water-areas, rivers0, rivers1, \
                peace, ground0, ground1",
        run: |a| match a {
            [dir, map, out] => m3d_png(Path::new(dir), map, Path::new(out), "terrain"),
            [dir, map, out, "--layer", layer] => m3d_png(Path::new(dir), map, Path::new(out), layer),
            _ => Err(crate::WRONG_ARGS.into()),
        },
    },
];

fn map_paths(vfs: &Vfs, filter: Option<&str>) -> Vec<String> {
    vfs.list()
        .filter(|(p, _)| p.to_ascii_uppercase().ends_with(".M3D") && crate::matches(p, filter))
        .map(|(p, _)| p.to_owned())
        .collect()
}

fn read_text(vfs: &mut Vfs, path: &str) -> Result<String> {
    Ok(vfs.read(path)?.iter().map(|&b| b as char).collect())
}

/// First word of every line in the `[MEMBERS]` section of each nation file: the unit ids
/// that `UNI3` records refer to.
fn unit_ids(vfs: &mut Vfs) -> Result<std::collections::BTreeSet<String>> {
    let mut ids = std::collections::BTreeSet::new();
    let paths: Vec<String> =
        vfs.list().filter(|(p, _)| p.to_ascii_uppercase().ends_with(".NDS")).map(|(p, _)| p.to_owned()).collect();
    for path in paths {
        let text = read_text(vfs, &path)?;
        let members = text.lines().skip_while(|l| !l.starts_with("[MEMBERS]")).skip(1);
        let members = members.take_while(|l| !l.starts_with('['));
        ids.extend(members.filter_map(|l| l.split_whitespace().next().map(str::to_owned)));
    }
    Ok(ids)
}

/// Entry count from the first line (`NAME count`) of a sprite list file.
///
/// Animation lists describe frame ranges (`# first last @NAME ...`) instead; each frame
/// appears to yield two objects (`...F`/`...B` in `ANIMATIONS.RSR`).
fn list_len(vfs: &mut Vfs, path: &str) -> Result<usize> {
    let text = read_text(vfs, path)?;
    let first = text.lines().next().unwrap_or_default();
    let count = first.split_whitespace().nth(1).ok_or("empty list")?.parse()?;
    let mut frames = 0;
    for line in text.lines().filter(|l| l.starts_with('#')) {
        let f: Vec<usize> =
            line.split_whitespace().skip(1).take(2).map(str::parse).collect::<std::result::Result<_, _>>()?;
        frames += 2 * (f.get(1).ok_or("bad animation line")? + 1 - f[0]);
    }
    Ok(if frames > 0 { frames } else { count })
}

/// Parses every map and cross-checks the decoded objects against the game data.
fn m3d_check(dir: &Path, filter: Option<&str>) -> Result<()> {
    let mut vfs = Vfs::open_game_dir(dir)?;
    let ids = unit_ids(&mut vfs)?;
    let lists = [
        (SpriteGroup::Trees, "TREELIST.LST"),
        (SpriteGroup::Stones, "STONLIST.LST"),
        (SpriteGroup::Holes, "HOLELIST.LST"),
        (SpriteGroup::Meat, "MEAT.LST"),
        (SpriteGroup::Animations, "ANIMATIONS.LST"),
        (SpriteGroup::Complex, "COMPLEX.LST"),
    ];
    let mut list_lens = BTreeMap::new();
    for (group, file) in lists {
        list_lens.insert(group, list_len(&mut vfs, file)?);
    }
    let paths = map_paths(&vfs, filter);
    let mut failures: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut fail = |reason: String, item: &str| failures.entry(reason).or_default().push(item.to_owned());
    let mut tags: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let (mut parsed, mut units, mut sprites, mut zones, mut groups, mut brigades) = (0, 0, 0, 0, 0, 0);
    for path in &paths {
        let map = match M3dMap::parse(&vfs.read(path)?) {
            Ok(m) => m,
            Err(e) => {
                let chunk = e.chunk.map(|t| tag_name(&t) + ": ").unwrap_or_default();
                fail(format!("{chunk}{}", e.reason), &format!("{path}@{:#x}", e.offset));
                continue;
            }
        };
        parsed += 1;
        let size = map.size.expect("parsed maps have a size");
        if map.header != [size.vertex_width() as u32, size.vertex_height() as u32] {
            fail("header does not match size".into(), path);
        }
        for (tag, _, len) in &map.chunks {
            let e = tags.entry(tag_name(tag)).or_default();
            *e = (e.0 + 1, e.1 + len);
        }
        let (playable, world) = (size.playable_size(), size.world_size());
        for u in &map.units {
            if !ids.contains(&u.name) {
                fail(format!("unit id not in any .NDS: {}", u.name), path);
            }
            if !(0..playable * 16).contains(&u.x) || !(0..playable * 16).contains(&u.y) || u.nation >= 8 {
                fail("unit position or nation out of range".into(), path);
            }
        }
        let at: std::collections::BTreeSet<(i32, i32)> = map.units.iter().map(|u| (u.x, u.y)).collect();
        let members = map.groups.iter().flat_map(|g| g.members.iter().copied());
        let slots = map.brigades.iter().flat_map(|b| b.members.iter().filter_map(|m| m.position));
        for pos in members.chain(slots) {
            if !at.contains(&pos) {
                fail("group or brigade member without a unit at its position".into(), path);
            }
        }
        for s in &map.sprites {
            if list_lens.get(&s.group).is_none_or(|&n| usize::from(s.index) >= n) {
                fail(format!("sprite index outside {:?} list", s.group), path);
            }
            if !(0..world).contains(&s.x) || !(0..world).contains(&s.y) {
                fail("sprite position out of range".into(), path);
            }
        }
        units += map.units.len();
        sprites += map.sprites.len();
        zones += map.zones.len();
        groups += map.groups.len();
        brigades += map.brigades.len();
    }
    println!("{parsed} of {} maps parsed", paths.len());
    println!("{units} units, {sprites} sprites, {zones} zones, {groups} groups, {brigades} brigades");
    println!("chunk  maps  payload bytes");
    for (tag, (count, bytes)) in &tags {
        println!("{tag}  {count:>5}  {bytes:>13}");
    }
    for (reason, items) in &failures {
        let mut sample: Vec<_> = items.iter().map(String::as_str).collect();
        sample.dedup();
        println!("  {:>6}  {reason}  e.g. {}", items.len(), sample[..sample.len().min(3)].join(", "));
    }
    Ok(())
}

const PLAYER_COLORS: [[u8; 3]; 8] = [
    [220, 30, 30],
    [30, 60, 230],
    [40, 200, 200],
    [160, 40, 200],
    [240, 140, 20],
    [40, 40, 40],
    [250, 250, 250],
    [120, 70, 20],
];

fn put(image: &mut Image, x: i64, y: i64, rgb: [u8; 3]) {
    if x >= 0 && y >= 0 && (x as u32) < image.width && (y as u32) < image.height {
        let i = ((y as u32 * image.width + x as u32) * 4) as usize;
        image.rgba[i..i + 4].copy_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
    }
}

/// Spreads small integers over clearly distinct colors.
fn hash_color(v: u32) -> [u8; 3] {
    let h = v.wrapping_mul(0x9E37_79B9).rotate_left(13);
    [(h >> 16) as u8 | 0x30, (h >> 8) as u8 | 0x30, h as u8 | 0x30]
}

fn grid_image<T: Copy>(grid: &Grid<T>, color: impl Fn(T) -> [u8; 3]) -> Image {
    let mut image = Image::new(grid.width as u32, grid.height as u32);
    for y in 0..grid.height {
        for x in 0..grid.width {
            put(&mut image, x as i64, y as i64, color(grid.cells[y * grid.width + x]));
        }
    }
    image
}

/// Renders one decoded layer; `terrain` is a shaded texture map with objects on top.
fn m3d_png(dir: &Path, path: &str, out: &Path, layer: &str) -> Result<()> {
    let mut vfs = Vfs::open_game_dir(dir)?;
    let map = M3dMap::parse(&vfs.read(path)?)?;
    let image = render_layer(&mut vfs, &map, path, layer)?;
    crate::write_png(out, &image)?;
    eprintln!("{}x{} -> {}", image.width, image.height, out.display());
    Ok(())
}

/// Renders one layer of a decoded map (see `m3d-png`); `name` is used in error messages.
pub fn render_layer(vfs: &mut Vfs, map: &M3dMap, path: &str, layer: &str) -> Result<Image> {
    let gray = |v: u8| [v, v, v];
    let ids = |v: u16| if v == u16::MAX { [0, 0, 0] } else { hash_color(u32::from(v)) };
    let missing = || format!("{path} has no data for layer {layer}");
    let image = match layer {
        "terrain" | "heights" | "textures" => {
            let heights = map.heights.as_ref().ok_or_else(missing)?;
            let textures = map.textures.as_ref().ok_or_else(missing)?;
            let (lo, hi) = heights.cells.iter().fold((i16::MAX, i16::MIN), |(a, b), &v| (a.min(v), b.max(v)));
            let mut image = Image::new(heights.width as u32, heights.height as u32);
            for row in 0..heights.height {
                for col in 0..heights.width {
                    let h = heights.get(col, row).unwrap_or(0);
                    let level = (i32::from(h - lo) * 255 / i32::from(hi - lo).max(1)) as u8;
                    let left = heights.get(col.saturating_sub(1), row).unwrap_or(h);
                    let up = heights.get(col, row.saturating_sub(1)).unwrap_or(h);
                    let shade = (160 + 4 * i32::from(left - h) + 4 * i32::from(up - h)).clamp(40, 320) as u16;
                    let depth = map.water.as_ref().and_then(|w| w.depth.get(col, row)).unwrap_or(0);
                    let rgb = match layer {
                        "heights" => gray(level),
                        "textures" => hash_color(u32::from(textures.get(col, row).unwrap_or(0))),
                        // Hill-shaded relief (brighter = higher), water tinted blue by depth.
                        _ => {
                            let land = (u16::from(level) / 2 + 64) * shade / 160;
                            let water = u16::from(depth);
                            let mix = |l: u16, w: u16| ((l * (255 - water) + w * water) / 255).min(255) as u8;
                            [mix(land, 20), mix(land, 60), mix(land, 140)]
                        }
                    };
                    put(&mut image, col as i64, row as i64, rgb);
                }
            }
            if layer == "terrain" {
                draw_objects(&mut image, map);
            }
            image
        }
        "preview" | "old-preview" => {
            use acr_formats::m3d::{OLD_PREVIEW_HEIGHT, OLD_PREVIEW_WIDTH, PREVIEW_HEIGHT, PREVIEW_WIDTH};
            let palette =
                Palette::parse(&vfs.read(acr_formats::pal::SPRITE_PALETTES[0])?).ok_or("palette too short")?;
            let grid = if layer == "preview" {
                Grid { width: PREVIEW_WIDTH, height: PREVIEW_HEIGHT, cells: map.preview.clone().ok_or_else(missing)? }
            } else {
                let cells = map.old_preview.clone().ok_or_else(missing)?;
                Grid { width: OLD_PREVIEW_WIDTH, height: OLD_PREVIEW_HEIGHT, cells }
            };
            grid_image(&grid, |v| palette.rgb(v))
        }
        "water-depth" => grid_image(&map.water.as_ref().ok_or_else(missing)?.depth, gray),
        "water-brightness" => grid_image(&map.water.as_ref().ok_or_else(missing)?.brightness, gray),
        "rivers0" | "rivers1" => {
            grid_image(&map.rivers.as_ref().ok_or_else(missing)?[usize::from(layer == "rivers1")], gray)
        }
        "land-areas" => grid_image(&map.land_areas.as_ref().ok_or_else(missing)?.cells, ids),
        "water-areas" => grid_image(&map.water_areas.as_ref().ok_or_else(missing)?.cells, ids),
        "peace" => grid_image(&map.peace.as_ref().ok_or_else(missing)?.cells, gray),
        "ground0" => grid_image(&map.ground.as_ref().ok_or_else(missing)?.0, gray),
        "ground1" => grid_image(&map.ground.as_ref().ok_or_else(missing)?.1, ids),
        _ => return Err(format!("unknown layer {layer}").into()),
    };
    Ok(image)
}

/// Draws sprites, zones and units onto a vertex-resolution image (32 world pixels per pixel).
fn draw_objects(image: &mut Image, map: &M3dMap) {
    let scale = i64::from(acr_formats::m3d::VERTEX_SPACING);
    for s in &map.sprites {
        let rgb = match s.group {
            SpriteGroup::Trees => [10, 90, 10],
            SpriteGroup::Stones => [150, 150, 150],
            _ => [250, 230, 60],
        };
        put(image, i64::from(s.x) / scale, i64::from(s.y) / scale, rgb);
    }
    for z in &map.zones {
        let (cx, cy, r) = (i64::from(z.x) / scale, i64::from(z.y) / scale, (i64::from(z.radius) / scale).max(1));
        for step in 0..64 {
            let a = f64::from(step) * std::f64::consts::TAU / 64.0;
            put(image, cx + (r as f64 * a.cos()) as i64, cy + (r as f64 * a.sin()) as i64, [255, 255, 0]);
        }
    }
    for u in &map.units {
        let (x, y) = (i64::from(u.x) / 16 / scale, i64::from(u.y) / 16 / scale);
        for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            put(image, x + dx, y + dy, PLAYER_COLORS[usize::from(u.nation % 8)]);
        }
    }
}
