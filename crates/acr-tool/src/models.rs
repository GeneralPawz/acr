//! 3D model commands (milestone M2): reference frames for the original camera and the
//! sprite check, which renders a model through Blender and scores it against the sprites.
//! See `docs/assets.md`.

use std::fmt::Write as _;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use acr_formats::defs::UnitDef;
use acr_formats::raster::{Image, RasterOptions, draw_frame};
use acr_formats::{GpFile, Vfs};

use crate::{Command, Result};

pub const COMMANDS: &[Command] = &[
    Command {
        name: "gp-frame",
        usage: "<game-dir> <gp-path> <frame> <out.png> [--player N] [--no-shadow]   (one frame, cropped; prints its origin)",
        run: |a| match a {
            [dir, gp, frame, out, rest @ ..] => gp_frame(Path::new(dir), gp, frame.parse()?, Path::new(out), rest),
            _ => Err(crate::WRONG_ARGS.into()),
        },
    },
    Command {
        name: "model-ref",
        usage: "<game-dir> <TYPE|LIST:INDEX> <out-dir> [--anim NAME] [--step N] [--player N] [--sections STATE]   (reference views for the original camera; TREELIST:104 = a map object)",
        run: |a| match a {
            [dir, ty, out, rest @ ..] => {
                RefOptions::parse(rest).and_then(|o| model_ref(Path::new(dir), ty, Path::new(out), &o).map(|_| ()))
            }
            _ => Err(crate::WRONG_ARGS.into()),
        },
    },
    Command {
        name: "model-check",
        usage: "<game-dir> <TYPE|LIST:INDEX> <model.glb> <out-dir> [--anim NAME] [--step N] [--sections STATE] [--min-iou X] [--blender PATH] [--scripts DIR] [--rendered]   (--rendered: score the render_<k>.png already in out-dir, rendered by render_views.py)",
        run: |a| match a {
            [dir, ty, glb, out, rest @ ..] => model_check(Path::new(dir), ty, Path::new(glb), Path::new(out), rest),
            _ => Err(crate::WRONG_ARGS.into()),
        },
    },
];

/// Encodings drawn into reference images: opaque, player colour and translucent layers.
/// Shadows (5, 0x26, 0x27) are left out: a model casts its own.
const BODY_ENCODINGS: [u8; 8] = [0, 1, 2, 3, 4, 0x2A, 0x2B, 0x2C];
/// Pixels with at least this alpha belong to a silhouette (the 25 % layers do not).
const SOLID_ALPHA: u8 = 128;
/// Free border around the reference views, in pixels.
const MARGIN: i32 = 16;
/// Search radius of the anchor offset, in pixels.
const SHIFT_RADIUS: i32 = 24;

/// The `RealDir` (0..256, 0 = east, 64 = south, clockwise on screen) that stored direction
/// `k` of an animation with `dirs` stored directions shows: the centre of the sector
/// [`acr_render::scene::sprite_direction`] maps to `k` without mirroring. Odd counts
/// (9: 64, 80, .., 192) and `FULLROTATION` (16: 64, 80, .., 304 mod 256) start at south;
/// even counts are offset by half a sector. The last of an odd count (north, 192) is the
/// one exception: the original only ever draws it mirrored (sector `i = 0`), which shows
/// only on asymmetric pictures.
pub fn stored_direction(k: usize, dirs: usize, full_rotation: bool) -> f64 {
    let (k, r) = (k as f64, dirs.max(1) as f64);
    let dir = if full_rotation {
        64.0 + 256.0 * k / r
    } else if dirs <= 1 {
        64.0
    } else if dirs % 2 == 1 {
        64.0 + 128.0 * k / (r - 1.0)
    } else {
        64.0 + 128.0 * (k + 0.5) / r
    };
    dir.rem_euclid(256.0)
}

struct RefOptions {
    anim: Option<String>,
    step: usize,
    player: u8,
    /// Draw every `ADDSECTION` section of a building in this state (0..=3) after the
    /// animation's parts, as a finished building does.
    sections: Option<u8>,
}

impl RefOptions {
    fn parse(args: &[&str]) -> Result<Self> {
        let mut o = Self { anim: None, step: 0, player: 0, sections: None };
        let mut it = args.iter();
        while let Some(&flag) = it.next() {
            let value = *it.next().ok_or_else(|| format!("{flag} needs a value"))?;
            match flag {
                "--anim" => o.anim = Some(value.to_owned()),
                "--step" => o.step = value.parse()?,
                "--player" => o.player = value.parse()?,
                "--sections" => o.sections = Some(value.parse()?),
                _ => return Err(format!("unknown option {flag}").into()),
            }
        }
        Ok(o)
    }
}

/// Writes one frame of a sprite pack cropped to its bounds and prints the frame origin in
/// the image (the point the frame is drawn at, see `docs/assets.md`).
fn gp_frame(dir: &Path, gp: &str, index: usize, out: &Path, rest: &[&str]) -> Result<()> {
    let mut player = 0;
    let mut no_shadow = false;
    let mut it = rest.iter();
    while let Some(&flag) = it.next() {
        match flag {
            "--player" => player = it.next().ok_or("--player needs a value")?.parse()?,
            "--no-shadow" => no_shadow = true,
            _ => return Err(format!("unknown option {flag}").into()),
        }
    }
    let mut vfs = Vfs::open_game_dir(dir)?;
    let palette = crate::images::load_palette(&mut vfs, acr_formats::sprite_palette(gp))?;
    let file = GpFile::parse(vfs.read(gp)?)?;
    let frame = file.decode_frame(index)?;
    let (x0, y0, x1, y1) = frame.bounds().ok_or("empty frame")?;
    let mut image = Image::new((x1 - x0) as u32, (y1 - y0) as u32);
    let only = no_shadow.then_some(&BODY_ENCODINGS[..]);
    draw_frame(&mut image, &frame, -x0, -y0, &RasterOptions { palette: &palette, player, only });
    crate::write_png(out, &image)?;
    println!("{{\"width\": {}, \"height\": {}, \"origin\": [{}, {}]}}", x1 - x0, y1 - y0, -x0, -y0);
    Ok(())
}

/// One stored direction of a reference.
struct View {
    k: usize,
    dir: f64,
    image: Image,
}

/// Reference views of an object type, all on one canvas whose ground point is `anchor`.
struct Reference {
    name: String,
    anim: String,
    dirs: usize,
    full_rotation: bool,
    width: u32,
    height: u32,
    anchor: (i32, i32),
    /// The section state drawn (`--sections`), if any.
    sections: Option<u8>,
    views: Vec<View>,
}

/// Composes the frames an object type shows in each stored direction of an animation
/// (buildings: all parts of the animation; units: the frame at `step`), without shadows,
/// and writes `ref_<k>.png` and `ref.json` to `out`.
fn model_ref(dir: &Path, ty: &str, out: &Path, o: &RefOptions) -> Result<Reference> {
    let mut vfs = Vfs::open_game_dir(dir)?;
    if let Some((list, index)) = ty.split_once(':') {
        return list_ref(&mut vfs, list, index.parse()?, out, o);
    }
    let (_, def) = crate::defs::find_unit(&mut vfs, ty)?;
    let anim = match &o.anim {
        Some(name) => def.animation(name).ok_or_else(|| format!("{ty} has no animation {name}"))?,
        None => default_animation(&def).ok_or_else(|| format!("{ty} has no animation"))?,
    };
    let dirs = anim.directions.max(1) as usize;
    let full_rotation = def.get("FULLROTATION").is_some();
    // A building's picture is placed at its position + `LOCATION` (see `acr_render::scene`).
    let location = def
        .get("LOCATION")
        .filter(|_| def.is_building())
        .map_or((0, 0), |c| (c.int(0).unwrap_or(0), c.int(1).unwrap_or(0)));
    let refs = anim.frame_refs();
    let refs: Vec<_> =
        if def.is_building() { refs } else { refs.get(o.step % refs.len().max(1)).copied().into_iter().collect() };

    // Per view: the decoded parts with their offset from the ground point.
    let mut packs: Vec<(String, GpFile, acr_formats::Palette)> = Vec::new();
    let mut parts_per_view = Vec::new();
    for k in 0..dirs {
        let mut parts = Vec::new();
        for r in &refs {
            let set = def.sprite(r.sprite).ok_or_else(|| format!("no sprite set {}", r.sprite))?;
            let path = set.gp_path();
            let i = match packs.iter().position(|(p, ..)| *p == path) {
                Some(i) => i,
                None => {
                    let palette = crate::images::load_palette(&mut vfs, acr_formats::sprite_palette(&path))?;
                    packs.push((path.clone(), GpFile::parse(vfs.read(&path)?)?, palette));
                    packs.len() - 1
                }
            };
            let index = r.frame.max(0) as usize * dirs + k;
            let frame = packs[i].1.decode_frame(index)?;
            parts.push((i, frame, (set.offset.0 + location.0, set.offset.1 + location.1)));
        }
        // A finished building draws the frame of each section's state after its parts.
        if let Some(state) = o.sections {
            for section in def.sections() {
                let Some((frame, _)) = section.shown(state) else { continue };
                let set = def.sprite(section.sprite).ok_or_else(|| format!("no sprite set {}", section.sprite))?;
                let path = set.gp_path();
                let i = match packs.iter().position(|(p, ..)| *p == path) {
                    Some(i) => i,
                    None => {
                        let palette = crate::images::load_palette(&mut vfs, acr_formats::sprite_palette(&path))?;
                        packs.push((path.clone(), GpFile::parse(vfs.read(&path)?)?, palette));
                        packs.len() - 1
                    }
                };
                let frame = packs[i].1.decode_frame(usize::from(frame) * dirs + k)?;
                parts.push((i, frame, (set.offset.0 + location.0, set.offset.1 + location.1)));
            }
        }
        parts_per_view.push(parts);
    }
    let name = def.name().unwrap_or(ty).to_owned();
    let reference = compose(name, anim.name.clone(), dirs, full_rotation, o, &packs, &parts_per_view)?;
    write_reference(&reference, out)?;
    Ok(reference)
}

/// A decoded frame of pack `.0`, drawn with its origin at the ground point + `.2`.
type Part = (usize, acr_formats::gp::Frame, (i32, i32));

/// Draws the parts of every view onto one canvas that holds all views plus a margin.
fn compose(
    name: String,
    anim: String,
    dirs: usize,
    full_rotation: bool,
    o: &RefOptions,
    packs: &[(String, GpFile, acr_formats::Palette)],
    parts_per_view: &[Vec<Part>],
) -> Result<Reference> {
    let mut bounds: Option<(i32, i32, i32, i32)> = None;
    for (_, frame, (dx, dy)) in parts_per_view.iter().flatten() {
        if let Some((a, b, c, d)) = frame.bounds() {
            let (a, b, c, d) = (a + dx, b + dy, c + dx, d + dy);
            bounds = Some(bounds.map_or((a, b, c, d), |(x0, y0, x1, y1)| (x0.min(a), y0.min(b), x1.max(c), y1.max(d))));
        }
    }
    let (x0, y0, x1, y1) = bounds.ok_or("the animation shows nothing")?;
    let (x0, y0) = (x0 - MARGIN, y0 - MARGIN);
    let (width, height) = ((x1 + MARGIN - x0) as u32, (y1 + MARGIN - y0) as u32);
    let anchor = (-x0, -y0);
    let views = parts_per_view
        .iter()
        .enumerate()
        .map(|(k, parts)| {
            let mut image = Image::new(width, height);
            for (i, frame, (dx, dy)) in parts {
                let opt = RasterOptions { palette: &packs[*i].2, player: o.player, only: Some(&BODY_ENCODINGS) };
                draw_frame(&mut image, frame, anchor.0 + dx, anchor.1 + dy, &opt);
            }
            View { k, dir: stored_direction(k, dirs, full_rotation), image }
        })
        .collect();
    Ok(Reference { name, anim, dirs, full_rotation, width, height, anchor, sections: o.sections, views })
}

/// The reference of a map object: frame `index` of a sprite list (`TREELIST:104` reads
/// `TREELIST.LST`), one view, with the ground point at the entry's `DX DY` in the frame.
fn list_ref(vfs: &mut Vfs, list: &str, index: usize, out: &Path, o: &RefOptions) -> Result<Reference> {
    let file = format!("{}.LST", list.to_ascii_uppercase());
    let text = String::from_utf8_lossy(&vfs.read(&file)?).into_owned();
    let parsed = acr_formats::sprite_list::SpriteList::parse(&text).ok_or_else(|| format!("cannot parse {file}"))?;
    let entry = parsed.entries.get(index).ok_or_else(|| format!("{file} has no entry {index}"))?;
    let path = parsed.gp_path();
    let palette = crate::images::load_palette(vfs, acr_formats::sprite_palette(&path))?;
    let gp = GpFile::parse(vfs.read(&path)?)?;
    let frame = gp.decode_frame(index)?;
    let packs = [(path, gp, palette)];
    let parts = vec![vec![(0, frame, (-entry.dx, -entry.dy))]];
    let reference = compose(format!("{list}:{index}"), "#STAND".into(), 1, false, o, &packs, &parts)?;
    write_reference(&reference, out)?;
    Ok(reference)
}

/// The animation a model is compared with by default: a building's `#STANDLO` (finished),
/// a unit's `#STAND`.
fn default_animation(def: &UnitDef) -> Option<&acr_formats::defs::md::Animation> {
    let names: &[&str] =
        if def.is_building() { &["#STANDLO", "#STAND", "#STANDHI"] } else { &["#STAND", "#PSTAND", "#MOTION_L"] };
    names.iter().find_map(|n| def.animation(n)).or_else(|| def.animations.first())
}

fn write_reference(r: &Reference, out: &Path) -> Result<()> {
    std::fs::create_dir_all(out)?;
    let mut views = Vec::new();
    for v in &r.views {
        let file = format!("ref_{}.png", v.k);
        crate::write_png(&out.join(&file), &v.image)?;
        views.push(format!("    {{\"k\": {}, \"dir\": {}, \"image\": \"{file}\"}}", v.k, v.dir));
    }
    let json = format!(
        "{{\n  \"type\": \"{}\",\n  \"anim\": \"{}\",\n  \"dirs\": {},\n  \"full_rotation\": {},\n  \
         \"sections\": {},\n  \"width\": {},\n  \"height\": {},\n  \"anchor\": [{}, {}],\n  \"views\": [\n{}\n  ]\n}}\n",
        r.name,
        r.anim,
        r.dirs,
        r.full_rotation,
        r.sections.map_or(-1, i32::from),
        r.width,
        r.height,
        r.anchor.0,
        r.anchor.1,
        views.join(",\n")
    );
    std::fs::write(out.join("ref.json"), json)?;
    Ok(())
}

/// The Blender executable: `--blender`, `$BLENDER`, the default install path of Blender 5.2
/// on Windows, or `blender` on the `PATH`.
pub fn find_blender(explicit: Option<&str>) -> PathBuf {
    if let Some(p) = explicit {
        return PathBuf::from(p);
    }
    if let Some(p) = std::env::var_os("BLENDER") {
        return PathBuf::from(p);
    }
    let default = Path::new(r"C:\Program Files\Blender Foundation\Blender 5.2\blender.exe");
    if default.is_file() { default.to_path_buf() } else { PathBuf::from("blender") }
}

/// Renders `glb` with the original camera in every stored direction, compares the renders
/// with the sprites and writes `report.md`, `report.json` and `side_<k>.png`.
fn model_check(dir: &Path, ty: &str, glb: &Path, out: &Path, rest: &[&str]) -> Result<()> {
    let mut ref_args = Vec::new();
    let (mut min_iou, mut blender, mut scripts) = (None, None, None);
    let mut rendered = false;
    let mut it = rest.iter();
    while let Some(&flag) = it.next() {
        if flag == "--rendered" {
            rendered = true;
            continue;
        }
        let value = *it.next().ok_or_else(|| format!("{flag} needs a value"))?;
        match flag {
            "--min-iou" => min_iou = Some(value.parse::<f64>()?),
            "--blender" => blender = Some(value),
            "--scripts" => scripts = Some(PathBuf::from(value)),
            _ => ref_args.extend([flag, value]),
        }
    }
    let reference = model_ref(dir, ty, out, &RefOptions::parse(&ref_args)?)?;
    if !rendered {
        let scripts = scripts.unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/blender"));
        let script = scripts.join("render_views.py");
        if !script.is_file() {
            return Err(format!("{} not found (use --scripts)", script.display()).into());
        }
        let blender = find_blender(blender);
        let status = std::process::Command::new(&blender)
            .args(["-b", "--factory-startup", "--python"])
            .arg(&script)
            .arg("--")
            .arg("--ref")
            .arg(out.join("ref.json"))
            .arg("--model")
            .arg(glb)
            .arg("--out")
            .arg(out)
            .status()
            .map_err(|e| format!("cannot run Blender ({}): {e}; set BLENDER or --blender", blender.display()))?;
        if !status.success() {
            return Err(format!("Blender failed ({status})").into());
        }
    }
    // The game stands unit models `UNIT_SOUTH` south of their anchor (#196): compare them there.
    let unit = !ty.contains(':') && !crate::defs::find_unit(&mut Vfs::open_game_dir(dir)?, ty)?.1.is_building();
    let down = if unit { UNIT_ROWS } else { 0 };
    let mut scores = Vec::new();
    for v in &reference.views {
        let render = lowered(read_png(&out.join(format!("render_{}.png", v.k)))?, down);
        if (render.width, render.height) != (v.image.width, v.image.height) {
            return Err(format!(
                "render_{}.png is {}x{}, expected {}x{}",
                v.k, render.width, render.height, v.image.width, v.image.height
            )
            .into());
        }
        let score = compare(&v.image, &render);
        crate::write_png(&out.join(format!("side_{}.png", v.k)), &side_by_side(&v.image, &render))?;
        scores.push((v, score));
    }
    let report = write_report(&reference, glb, &scores, out)?;
    print!("{report}");
    let worst = scores.iter().map(|(_, s)| s.iou).fold(1.0, f64::min);
    if let Some(min) = min_iou
        && worst < min
    {
        return Err(format!("IoU {worst:.3} below {min}").into());
    }
    Ok(())
}

/// Reads an 8-bit PNG as RGBA.
fn read_png(path: &Path) -> Result<Image> {
    let mut decoder = png::Decoder::new(BufReader::new(std::fs::File::open(path)?));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info()?;
    let mut buf = vec![0; reader.output_buffer_size().ok_or("PNG too large")?];
    let info = reader.next_frame(&mut buf)?;
    let pixels = (info.width * info.height) as usize;
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => buf[..pixels * 4].to_vec(),
        png::ColorType::Rgb => buf[..pixels * 3].chunks(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => buf[..pixels * 2].chunks(2).flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
        png::ColorType::Grayscale => buf[..pixels].iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return Err("indexed PNG not expanded".into()),
    };
    Ok(Image { width: info.width, height: info.height, rgba })
}

/// Scores of one view.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Score {
    /// Intersection over union of the silhouettes (alpha ≥ 128).
    pub iou: f64,
    /// Mean absolute RGB difference (0..255, averaged over the channels) where both are solid.
    pub color: f64,
    /// Shift of the render (pixels) that maximises the IoU: how far the anchor is off.
    pub shift: (i32, i32),
    /// IoU at that shift.
    pub shifted_iou: f64,
    /// Silhouette areas in pixels: sprite, render.
    pub area: (usize, usize),
}

/// How many pixels below its anchor the game draws a unit model at zoom 1: half of
/// `acr_render::view3d::UNIT_SOUTH` (world pixels; the projection halves `y`).
const UNIT_ROWS: u32 = 3;

/// `image` moved `rows` pixels down (the top rows clear): a render drawn where the game draws.
fn lowered(mut image: Image, rows: u32) -> Image {
    let stride = image.width as usize * 4;
    let shift = (rows as usize * stride).min(image.rgba.len());
    let len = image.rgba.len();
    image.rgba.copy_within(..len - shift, shift);
    image.rgba[..shift].fill(0);
    image
}

fn solid(image: &Image) -> Vec<bool> {
    image.rgba.chunks(4).map(|p| p[3] >= SOLID_ALPHA).collect()
}

/// Compares a render with the sprite reference of the same size.
pub fn compare(sprite: &Image, render: &Image) -> Score {
    let (w, h) = (sprite.width as i32, sprite.height as i32);
    let (a, b) = (solid(sprite), solid(render));
    let iou_at = |sx: i32, sy: i32| {
        let (mut inter, mut union) = (0usize, 0usize);
        for y in 0..h {
            for x in 0..w {
                let pa = a[(y * w + x) as usize];
                let (bx, by) = (x - sx, y - sy);
                let pb = bx >= 0 && by >= 0 && bx < w && by < h && b[(by * w + bx) as usize];
                inter += usize::from(pa && pb);
                union += usize::from(pa || pb);
            }
        }
        if union == 0 { 0.0 } else { inter as f64 / union as f64 }
    };
    let iou = iou_at(0, 0);
    // Coarse-to-fine search of the best shift (the IoU is smooth in the shift).
    let (mut best, mut best_iou): ((i32, i32), f64) = ((0, 0), iou);
    let mut step = 8;
    while step >= 1 {
        let (cx, cy) = best;
        for dy in [-step, 0, step] {
            for dx in [-step, 0, step] {
                let s = (cx + dx, cy + dy);
                if s.0.abs() > SHIFT_RADIUS || s.1.abs() > SHIFT_RADIUS || s == best {
                    continue;
                }
                let v = iou_at(s.0, s.1);
                if v > best_iou + 1e-12 {
                    (best, best_iou) = (s, v);
                }
            }
        }
        if best == (cx, cy) {
            step /= 2;
        }
    }
    let (mut sum, mut n) = (0.0, 0usize);
    for (i, (pa, pb)) in a.iter().zip(&b).enumerate() {
        if *pa && *pb {
            let (s, r) = (&sprite.rgba[i * 4..i * 4 + 3], &render.rgba[i * 4..i * 4 + 3]);
            sum += s.iter().zip(r).map(|(x, y)| f64::from(x.abs_diff(*y))).sum::<f64>() / 3.0;
            n += 1;
        }
    }
    let color = if n == 0 { f64::NAN } else { sum / n as f64 };
    let area = (a.iter().filter(|&&s| s).count(), b.iter().filter(|&&s| s).count());
    Score { iou, color, shift: best, shifted_iou: best_iou, area }
}

/// Sprite | render | silhouettes (both: grey, sprite only: magenta, render only: cyan),
/// on a neutral background.
fn side_by_side(sprite: &Image, render: &Image) -> Image {
    let (w, h) = (sprite.width, sprite.height);
    let mut out = Image::new(w * 3 + 8, h);
    let bg = [72u8, 76, 64];
    for y in 0..h {
        for x in 0..out.width {
            let i = ((y * out.width + x) * 4) as usize;
            out.rgba[i..i + 4].copy_from_slice(&[bg[0], bg[1], bg[2], 255]);
        }
    }
    let over = |img: &Image, x: u32, y: u32| {
        let i = ((y * w + x) * 4) as usize;
        let a = u32::from(img.rgba[i + 3]);
        let mix = |c: u8, b: u8| ((u32::from(c) * a + u32::from(b) * (255 - a)) / 255) as u8;
        [mix(img.rgba[i], bg[0]), mix(img.rgba[i + 1], bg[1]), mix(img.rgba[i + 2], bg[2])]
    };
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            let (pa, pb) = (sprite.rgba[i + 3] >= SOLID_ALPHA, render.rgba[i + 3] >= SOLID_ALPHA);
            let mask = match (pa, pb) {
                (true, true) => [170, 170, 170],
                (true, false) => [230, 40, 200],
                (false, true) => [40, 210, 230],
                (false, false) => bg,
            };
            for (col, c) in [(0, over(sprite, x, y)), (w + 4, over(render, x, y)), (2 * w + 8, mask)] {
                let o = ((y * out.width + col + x) * 4) as usize;
                out.rgba[o..o + 3].copy_from_slice(&c);
            }
        }
    }
    out
}

fn write_report(r: &Reference, glb: &Path, scores: &[(&View, Score)], out: &Path) -> Result<String> {
    let mut md = String::new();
    writeln!(md, "# Sprite check: {} {}\n", r.name, r.anim)?;
    writeln!(
        md,
        "Model `{}`, {} stored direction(s), canvas {}x{}, anchor {:?}.\n",
        glb.display(),
        r.dirs,
        r.width,
        r.height,
        r.anchor
    )?;
    writeln!(md, "| k | RealDir | IoU | colour diff | anchor shift | IoU shifted | area sprite / render | image |")?;
    writeln!(md, "|---|---:|---:|---:|---|---:|---|---|")?;
    let mut json = Vec::new();
    for (v, s) in scores {
        writeln!(
            md,
            "| {} | {} | {:.3} | {:.1} | {:?} | {:.3} | {} / {} | side_{}.png |",
            v.k, v.dir, s.iou, s.color, s.shift, s.shifted_iou, s.area.0, s.area.1, v.k
        )?;
        json.push(format!(
            "    {{\"k\": {}, \"dir\": {}, \"iou\": {:.4}, \"color\": {:.2}, \"shift\": [{}, {}], \"shifted_iou\": {:.4}, \"area\": [{}, {}]}}",
            v.k, v.dir, s.iou, if s.color.is_nan() { -1.0 } else { s.color }, s.shift.0, s.shift.1, s.shifted_iou, s.area.0, s.area.1
        ));
    }
    let mean = |f: fn(&Score) -> f64| scores.iter().map(|(_, s)| f(s)).sum::<f64>() / scores.len().max(1) as f64;
    writeln!(md, "\nMean IoU {:.3}, mean colour difference {:.1}.", mean(|s| s.iou), mean(|s| s.color))?;
    std::fs::write(out.join("report.md"), &md)?;
    let json = format!(
        "{{\n  \"type\": \"{}\",\n  \"anim\": \"{}\",\n  \"model\": \"{}\",\n  \"mean_iou\": {:.4},\n  \"views\": [\n{}\n  ]\n}}\n",
        r.name,
        r.anim,
        glb.display().to_string().replace('\\', "/"),
        mean(|s| s.iou),
        json.join(",\n")
    );
    std::fs::write(out.join("report.json"), json)?;
    Ok(md)
}

#[cfg(test)]
mod tests {
    use super::*;
    use acr_render::scene::sprite_direction;

    #[test]
    fn stored_directions_round_trip_through_sprite_direction() {
        for (dirs, full) in [(1, false), (2, false), (8, false), (9, false), (16, true), (65, false)] {
            for k in 0..dirs {
                let dir = stored_direction(k, dirs, full);
                // The last of an odd count (north) is only ever drawn mirrored (i = 0).
                let mirrored = !full && dirs % 2 == 1 && dirs > 1 && k == dirs - 1;
                assert_eq!(
                    sprite_direction(dir.round() as u8, dirs, full),
                    (k, mirrored),
                    "{dirs} dirs, k {k}, dir {dir}"
                );
            }
        }
        // 9 directions: south, west, north.
        assert_eq!([0, 4, 8].map(|k| stored_direction(k, 9, false)), [64.0, 128.0, 192.0]);
        assert_eq!(stored_direction(12, 16, true), 0.0, "full rotation: east at 12");
    }

    fn square(w: u32, x0: u32, y0: u32, s: u32) -> Image {
        let mut img = Image::new(w, w);
        for y in y0..y0 + s {
            for x in x0..x0 + s {
                let i = ((y * w + x) * 4) as usize;
                img.rgba[i..i + 4].copy_from_slice(&[100, 100, 100, 255]);
            }
        }
        img
    }

    #[test]
    fn a_lowered_render_moves_down() {
        // #196: unit renders are compared 3 px lower, where the game draws the model.
        let s = compare(&square(64, 20, 23, 10), &lowered(square(64, 20, 20, 10), 3));
        assert_eq!((s.iou, s.shift), (1.0, (0, 0)));
        assert_eq!(lowered(square(64, 20, 20, 10), 0).rgba, square(64, 20, 20, 10).rgba);
        assert_eq!(UNIT_ROWS as f32 * 2.0, acr_render::view3d::UNIT_SOUTH);
    }

    #[test]
    fn compare_finds_iou_and_shift() {
        let a = square(64, 20, 20, 10);
        let s = compare(&a, &a);
        assert_eq!((s.iou, s.color, s.shift), (1.0, 0.0, (0, 0)));
        // The render is 3 px right of and 2 px above the sprite: 7·8 = 56 of 144 pixels overlap.
        let s = compare(&a, &square(64, 23, 18, 10));
        assert!((s.iou - 56.0 / 144.0).abs() < 1e-9);
        assert_eq!((s.shift, s.shifted_iou), ((-3, 2), 1.0));
    }
}
