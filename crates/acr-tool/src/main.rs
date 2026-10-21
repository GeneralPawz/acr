//! `acr` - inspect and extract original American Conquest data.

mod audio;
mod images;

use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use acr_formats::m3d::{Grid, SpriteGroup, tag_name};
use acr_formats::raster::{Image, RasterOptions, draw_frame};
use acr_formats::{GpFile, M3dMap, Palette, Vfs, normalize_path};

const USAGE: &str = "\
usage:
  acr list     <game-dir> [filter]
  acr cat      <game-dir> <path>
  acr extract  <game-dir> <out-dir> [filter]
  acr gp-sheet <game-dir> <gp-path> <out.png> [--pal PATH] [--player N]
                [--only ENC,ENC..] [--frames FROM..TO]
  acr gp-check <game-dir> [filter]
  acr m3d-check <game-dir> [filter]
  acr m3d-png  <game-dir> <map-path> <out.png> [--layer NAME]
                layers: terrain (default), heights, textures, preview, old-preview,
                water-depth, water-brightness, land-areas, water-areas, rivers0, rivers1,
                peace, ground0, ground1
  acr script-api <game-dir>   (markdown report of the API missions/AI import)
  acr audio-check <game-dir>  (decode every sound effect and music track)
  acr play <game-dir> <path>  (play a .wav or .ogg on the default device)
  acr grd-fit  <game-dir>   (blend strengths of the sprite lookup tables)
  acr gp-palette <game-dir> [filter]   (rank palettes per sprite pack)
  acr play <game-dir> <path>  (play a .wav or .ogg on the default device)
  acr bmp-png <game-dir> <bmp> <out.png>   (--pal of gp-sheet also accepts a .BMP)";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = match args.as_slice() {
        ["list", dir, rest @ ..] => list(Path::new(dir), rest.first().copied()),
        ["cat", dir, path] => cat(Path::new(dir), path),
        ["extract", dir, out, rest @ ..] => extract(Path::new(dir), Path::new(out), rest.first().copied()),
        ["gp-sheet", dir, gp, out, rest @ ..] => {
            SheetOptions::parse(rest).and_then(|o| gp_sheet(Path::new(dir), gp, Path::new(out), &o))
        }
        ["gp-check", dir, rest @ ..] => gp_check(Path::new(dir), rest.first().copied()),
        ["grd-fit", dir] => grd_fit(Path::new(dir)),
        ["gp-palette", dir, rest @ ..] => gp_palette(Path::new(dir), rest.first().copied()),
        ["script-api", dir] => script_api(Path::new(dir)),
        ["audio-check", dir] => audio::check(Path::new(dir)),
        ["play", dir, path] => audio::play(Path::new(dir), path),
        ["m3d-check", dir, rest @ ..] => m3d_check(Path::new(dir), rest.first().copied()),
        ["m3d-png", dir, map, out, rest @ ..] => match rest {
            [] => m3d_png(Path::new(dir), map, Path::new(out), "terrain"),
            ["--layer", layer] => m3d_png(Path::new(dir), map, Path::new(out), layer),
            _ => Err("expected --layer NAME".into()),
        },
        ["bmp-png", dir, bmp, out] => images::bmp_png(Path::new(dir), bmp, Path::new(out)),
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Case-insensitive substring match on normalized paths.
fn matches(path: &str, filter: Option<&str>) -> bool {
    filter.is_none_or(|f| normalize_path(path).contains(&normalize_path(f)))
}

fn list(dir: &Path, filter: Option<&str>) -> Result<()> {
    let vfs = Vfs::open_game_dir(dir)?;
    let mut out = io::stdout().lock();
    for (path, source) in vfs.list().filter(|(p, _)| matches(p, filter)) {
        writeln!(out, "{path}\t{source}")?;
    }
    Ok(())
}

fn cat(dir: &Path, path: &str) -> Result<()> {
    let mut vfs = Vfs::open_game_dir(dir)?;
    io::stdout().lock().write_all(&vfs.read(path)?)?;
    Ok(())
}

fn extract(dir: &Path, out: &Path, filter: Option<&str>) -> Result<()> {
    let mut vfs = Vfs::open_game_dir(dir)?;
    let paths = archived_paths(&vfs, filter);
    for path in &paths {
        let target: PathBuf = out.join(path.replace('\\', std::path::MAIN_SEPARATOR_STR));
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&target, vfs.read(path)?)?;
    }
    eprintln!("extracted {} files to {}", paths.len(), out.display());
    Ok(())
}

fn archived_paths(vfs: &Vfs, filter: Option<&str>) -> Vec<String> {
    vfs.list().filter(|(p, source)| *source != "<loose>" && matches(p, filter)).map(|(p, _)| p.to_owned()).collect()
}

struct SheetOptions {
    /// Defaults to the palette the sprite pack was drawn for.
    palette: Option<String>,
    player: u8,
    only: Option<Vec<u8>>,
    frames: Option<std::ops::Range<usize>>,
}

impl SheetOptions {
    fn parse(args: &[&str]) -> Result<Self> {
        let mut o = Self { palette: None, player: 0, only: None, frames: None };
        let mut it = args.iter();
        while let Some(&flag) = it.next() {
            let value = *it.next().ok_or_else(|| format!("{flag} needs a value"))?;
            match flag {
                "--pal" => o.palette = Some(value.to_owned()),
                "--player" => o.player = value.parse()?,
                "--only" => o.only = Some(value.split(',').map(str::parse).collect::<std::result::Result<_, _>>()?),
                "--frames" => {
                    let (a, b) = value.split_once("..").ok_or("--frames expects FROM..TO")?;
                    o.frames = Some(a.parse()?..b.parse()?);
                }
                _ => return Err(format!("unknown option {flag}").into()),
            }
        }
        Ok(o)
    }
}

/// Renders the frames of a sprite pack into one grid image.
fn gp_sheet(dir: &Path, gp: &str, out: &Path, o: &SheetOptions) -> Result<()> {
    let mut vfs = Vfs::open_game_dir(dir)?;
    let palette = images::load_palette(&mut vfs, &o.palette)?;
    let pal_path = o.palette.as_deref().unwrap_or(acr_formats::sprite_palette(gp));
    let palette = Palette::parse(&vfs.read(pal_path)?).ok_or("palette too short")?;
    let file = GpFile::parse(vfs.read(gp)?)?;

    let range = o.frames.clone().unwrap_or(0..file.frame_count());
    let n = file.frame_count();
    let frames: Vec<_> = (range.start.min(n)..range.end.min(n)).map(|i| file.decode_frame(i)).collect();
    for (i, f) in frames.iter().enumerate() {
        if let Err(e) = f {
            eprintln!("frame {i}: {e}");
        }
    }
    let ok: Vec<_> = frames.iter().flatten().collect();
    // Use one cell size for all frames so animations line up.
    let (mut x0, mut y0, mut x1, mut y1) = (0, 0, 1, 1);
    for (a, b, c, d) in ok.iter().filter_map(|f| f.bounds()) {
        (x0, y0, x1, y1) = (x0.min(a), y0.min(b), x1.max(c), y1.max(d));
    }
    let (cell_w, cell_h) = ((x1 - x0 + 2) as u32, (y1 - y0 + 2) as u32);
    let cols = (ok.len() as f64).sqrt().ceil().max(1.0) as u32;
    let rows = (ok.len() as u32).div_ceil(cols).max(1);
    let mut image = Image::new(cols * cell_w, rows * cell_h);
    let opt = RasterOptions { palette: &palette, player: o.player, only: o.only.as_deref() };
    for (i, frame) in ok.iter().enumerate() {
        let (cx, cy) = ((i as u32 % cols) * cell_w, (i as u32 / cols) * cell_h);
        draw_frame(&mut image, frame, cx as i32 - x0 + 1, cy as i32 - y0 + 1, &opt);
    }
    write_png(out, &image)?;
    eprintln!("{} of {} frames, cell {cell_w}x{cell_h} -> {}", ok.len(), frames.len(), out.display());
    Ok(())
}

/// Decodes every sprite pack and reports failures grouped by reason.
fn gp_check(dir: &Path, filter: Option<&str>) -> Result<()> {
    let mut vfs = Vfs::open_game_dir(dir)?;
    let paths: Vec<String> = vfs
        .list()
        .filter(|(p, _)| p.to_ascii_uppercase().ends_with(".GP") && matches(p, filter))
        .map(|(p, _)| p.to_owned())
        .collect();
    let (mut frames, mut layers) = (0usize, 0usize);
    let mut failures: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for path in &paths {
        let file = match GpFile::parse(vfs.read(path)?) {
            Ok(f) => f,
            Err(e) => {
                failures.entry(e.reason.to_owned()).or_default().push(path.clone());
                continue;
            }
        };
        for i in 0..file.frame_count() {
            frames += 1;
            match file.decode_frame(i) {
                Ok(f) => layers += f.layers.len(),
                Err(e) => failures.entry(e.reason.to_owned()).or_default().push(format!("{path}#{i}")),
            }
        }
    }
    println!("{} files, {frames} frames, {layers} layers decoded", paths.len());
    for (reason, items) in &failures {
        let sample: Vec<_> = items.iter().take(4).map(String::as_str).collect();
        println!("  {:>6}  {reason}  e.g. {}", items.len(), sample.join(", "));
    }
    Ok(())
}

/// Fits the blend tables the original uses for sprite layers to alpha values.
fn grd_fit(dir: &Path) -> Result<()> {
    use acr_formats::grd::BlendTable;

    let mut vfs = Vfs::open_game_dir(dir)?;
    for set in 0..3 {
        let palette = Palette::parse(&vfs.read(&format!("{set}\\AGEW_1.PAL"))?).ok_or("palette too short")?;
        let mut table = |name: &str| -> Result<BlendTable> {
            Ok(BlendTable::parse(&vfs.read(&format!("{set}\\{name}"))?).ok_or("not a GRD table")?)
        };
        for name in ["AGEW_TR4.GRD", "AGEW_TR.GRD"] {
            let w = table(name)?.mix_weight(&palette).ok_or("not a 256-row table")?;
            println!("{set}\\{name}: row colour weight {w:.3}");
        }
        let alphas: Vec<String> =
            table("AGEW_1D.GRD")?.darkness(&palette).iter().map(|a| format!("{}", (a * 255.0).round())).collect();
        println!("{set}\\AGEW_1D.GRD: black alpha per row [{}]", alphas.join(", "));
    }
    Ok(())
}

/// Ranks the sprite palette sets per sprite pack. Neighbouring pixels of artwork are similar
/// in the palette it was drawn for: the score is the mean squared RGB distance of horizontally
/// adjacent opaque pixels relative to that of random pixel pairs (lower is better).
fn gp_palette(dir: &Path, filter: Option<&str>) -> Result<()> {
    use acr_formats::Encoding;
    use acr_formats::pal::SPRITE_PALETTES;

    let mut vfs = Vfs::open_game_dir(dir)?;
    let palettes = SPRITE_PALETTES
        .iter()
        .map(|&name| Ok((name, Palette::parse(&vfs.read(name)?).ok_or("palette too short")?)))
        .collect::<Result<Vec<_>>>()?;
    let paths: Vec<String> = vfs
        .list()
        .filter(|(p, _)| p.to_ascii_uppercase().ends_with(".GP") && matches(p, filter))
        .map(|(p, _)| p.to_owned())
        .collect();
    println!("sprite\tmapped\tscores");
    for path in &paths {
        let Ok(file) = GpFile::parse(vfs.read(path)?) else { continue };
        let mut pairs: BTreeMap<(u8, u8), u64> = BTreeMap::new();
        for frame in (0..file.frame_count()).filter_map(|i| file.decode_frame(i).ok()) {
            for layer in frame.layers.iter().filter(|l| matches!(l.encoding, Encoding::DictLz | Encoding::SelfLz(_))) {
                let mut pixels = layer.pixels.iter().copied();
                for run in &layer.runs {
                    let row: Vec<u8> = pixels.by_ref().take(usize::from(run.len)).collect();
                    for w in row.windows(2) {
                        *pairs.entry((w[0], w[1])).or_default() += 1;
                    }
                }
            }
        }
        let total: u64 = pairs.values().sum();
        if total == 0 {
            continue;
        }
        let mut hist = [0u64; 256];
        for (&(a, _), &n) in &pairs {
            hist[usize::from(a)] += n;
        }
        let scores: Vec<String> = palettes
            .iter()
            .map(|(name, pal)| {
                let dist = |a: u8, b: u8| -> f64 {
                    let (ca, cb) = (pal.rgb(a), pal.rgb(b));
                    (0..3).map(|k| (f64::from(ca[k]) - f64::from(cb[k])).powi(2)).sum()
                };
                let near: f64 = pairs.iter().map(|(&(a, b), &n)| dist(a, b) * n as f64).sum::<f64>() / total as f64;
                let mut far = 0.0;
                for a in 0..=255u8 {
                    for b in 0..=255u8 {
                        far += dist(a, b) * (hist[usize::from(a)] * hist[usize::from(b)]) as f64;
                    }
                }
                let far = far / (total * total) as f64;
                format!("{name} {:.3}", if far > 0.0 { near / far } else { 1.0 })
            })
            .collect();
        println!("{path}\t{}\t{}", acr_formats::sprite_palette(path), scores.join("\t"));
    }
    Ok(())
}

/// Lists every function the mission/AI DLLs import from the game executable.
fn script_api(dir: &Path) -> Result<()> {
    use acr_script::pe::PeImage;

    let exe = PeImage::parse(&std::fs::read(dir.join("dmcr.exe"))?)?;
    let exported: std::collections::BTreeSet<&str> = exe.exports.iter().filter_map(|e| e.name.as_deref()).collect();

    // function -> (number of DLLs importing it, example DLL)
    let mut uses: BTreeMap<String, (usize, String)> = BTreeMap::new();
    let mut other_dlls: BTreeMap<String, usize> = BTreeMap::new();
    let mut dll_count = 0;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d)? {
            let path = entry?.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let rel = path.strip_prefix(dir)?.to_string_lossy().replace('\\', "/");
            let is_script = ["AI/", "Missions/", "History_battl/", "UserMissions/"].iter().any(|p| rel.starts_with(p));
            if !is_script || !rel.to_ascii_lowercase().ends_with(".dll") {
                continue;
            }
            let pe = PeImage::parse(&std::fs::read(&path)?).map_err(|e| format!("{rel}: {e}"))?;
            dll_count += 1;
            let mut seen = std::collections::BTreeSet::new();
            for import in &pe.imports {
                if import.dll.eq_ignore_ascii_case("dmcr.exe") {
                    if seen.insert(import.name.to_string()) {
                        let e = uses.entry(import.name.to_string()).or_insert((0, rel.clone()));
                        e.0 += 1;
                    }
                } else {
                    *other_dlls.entry(format!("{}!{}", import.dll.to_ascii_uppercase(), import.name)).or_default() += 1;
                }
            }
        }
    }

    let mut out = io::stdout().lock();
    writeln!(out, "# Mission and AI scripting API\n")?;
    writeln!(out, "Generated by `acr script-api` from {dll_count} mission/AI DLLs.\n")?;
    writeln!(out, "## Imports from `dmcr.exe` ({} functions)\n", uses.len())?;
    writeln!(out, "| function | used by DLLs | exported by dmcr.exe | example |\n|---|---:|---|---|")?;
    for (name, (count, example)) in &uses {
        let exp = if exported.contains(name.as_str()) { "yes" } else { "**no**" };
        writeln!(out, "| `{name}` | {count} | {exp} | `{example}` |")?;
    }
    writeln!(out, "\n## Other imports\n\n| function | used by DLLs |\n|---|---:|")?;
    for (name, count) in &other_dlls {
        writeln!(out, "| `{name}` | {count} |")?;
    }
    Ok(())
}

fn map_paths(vfs: &Vfs, filter: Option<&str>) -> Vec<String> {
    vfs.list()
        .filter(|(p, _)| p.to_ascii_uppercase().ends_with(".M3D") && matches(p, filter))
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
                draw_objects(&mut image, &map);
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
    write_png(out, &image)?;
    eprintln!("{}x{} -> {}", image.width, image.height, out.display());
    Ok(())
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

fn write_png(path: &Path, image: &Image) -> Result<()> {
    let file = io::BufWriter::new(std::fs::File::create(path)?);
    let mut encoder = png::Encoder::new(file, image.width, image.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(&image.rgba)?;
    Ok(())
}
