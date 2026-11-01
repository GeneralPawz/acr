//! The game's pictures and fonts for the UI, decoded from the player's files at runtime
//! (`/asset/v1/..`; docs/webui.md "Assets"). Nothing of the original data is stored anywhere.
//!
//! | Path (game paths percent-encoded) | Answer |
//! |---|---|
//! | `bmp/<path>` | PNG of a bitmap |
//! | `frame/<menu\|game\|sprite>/<player>/<gp>/<frame>` | PNG of a sprite frame (its bounding box) |
//! | `font/<set>/<FontId>.png` / `.json` | glyph atlas / metrics |
//! | `text/<set>/<FontId>/<fade>/<hex bytes>` | PNG of a whole string, the exact fallback |
//! | `remap/<player>/<gp>/<frame>/<table>/<row>` | PNG of a game frame through a colour table (gray icons) |
//! | `image/<name>/<version>` | PNG of an image the engine made (the minimap, the portrait) |
//! | `scale/<n>/frame/..` / `scale/<n>/remap/..` | the same frame enlarged `n` (2 or 4) times by the pixel-art scaler ([`super::upscale`]) |
//!
//! Failures are errors, never placeholders: a bad path, a missing file or a file that does not
//! decode.

use std::collections::HashMap;

use acr_formats::raster::Image;

use super::font_atlas;
use super::upscale::{MAX_SCALE, upscale};
use crate::ui::draw::{GAME_PALETTE, MENU_PALETTE, rasterize_frame_as};
use crate::ui::fonts::{FontId, Fonts};
use crate::ui::library::Library;

/// The bytes of an answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetBody {
    Png(Vec<u8>),
    Json(String),
}

impl AssetBody {
    pub fn mime(&self) -> &'static str {
        match self {
            AssetBody::Png(_) => "image/png",
            AssetBody::Json(_) => "application/json",
        }
    }
}

/// Why an asset cannot be answered; the HTTP status of the native host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetError {
    /// 400: the path is none of the above.
    BadPath(String),
    /// 404: a file it needs does not exist.
    NotFound(String),
    /// 500: a file does not decode.
    Decode(String),
}

impl AssetError {
    pub fn status(&self) -> u16 {
        match self {
            AssetError::BadPath(_) => 400,
            AssetError::NotFound(_) => 404,
            AssetError::Decode(_) => 500,
        }
    }
}

impl std::fmt::Display for AssetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (AssetError::BadPath(m) | AssetError::NotFound(m) | AssetError::Decode(m)) = self;
        f.write_str(m)
    }
}

/// Where an answer goes once it is known. Sendable off the browser, where a native host's
/// protocol handler (wry) carries it inside `Event`.
#[cfg(not(target_arch = "wasm32"))]
pub type AssetReply = Box<dyn FnOnce(Result<AssetBody, AssetError>) + Send>;
/// Where an answer goes once it is known.
#[cfg(target_arch = "wasm32")]
pub type AssetReply = Box<dyn FnOnce(Result<AssetBody, AssetError>)>;

/// An answer, or the files to wait for.
pub enum Resolved {
    Pending,
    Ready(AssetBody),
}

/// Atlases made so far (the PNG and the metrics are two requests).
#[derive(Default)]
pub struct AssetCache {
    atlases: HashMap<(u8, String), font_atlas::Atlas>,
    /// The engine's dynamic images by name: the last versions, newest last.
    images: HashMap<String, Vec<(u32, Image)>>,
}

/// Versions of a dynamic image kept, for the UI that still asks for the one before.
const KEEP_VERSIONS: usize = 3;

impl AssetCache {
    /// Records `image` as `version` of `name`.
    pub fn add_image(&mut self, name: &str, version: u32, image: Image) {
        let versions = self.images.entry(name.to_owned()).or_default();
        versions.push((version, image));
        if versions.len() > KEEP_VERSIONS {
            versions.remove(0);
        }
    }
}

/// A rasterized sprite with the offset of its top-left pixel from the sprite's origin.
pub type Placed = (Image, [i32; 2]);

/// Percent-encodes a game path for use as a path segment.
pub fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Decodes `%XX` escapes.
pub fn decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// The palettes a frame url can name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FramePalette {
    /// The menus' (`1\agew_1.pal`).
    Menu,
    /// The game screen's (`0\agew_1.pal`).
    Game,
    /// The one the pack was made for (`acr_formats::sprite_palette`).
    Sprite,
}

impl FramePalette {
    fn name(self) -> &'static str {
        match self {
            FramePalette::Menu => "menu",
            FramePalette::Game => "game",
            FramePalette::Sprite => "sprite",
        }
    }

    fn path(self, gp: &str) -> String {
        match self {
            FramePalette::Menu => MENU_PALETTE.to_owned(),
            FramePalette::Game => GAME_PALETTE.to_owned(),
            FramePalette::Sprite => acr_formats::sprite_palette(gp).to_owned(),
        }
    }
}

pub fn bmp_url(path: &str) -> String {
    format!("bmp/{}", encode(path))
}

pub fn frame_url(palette: FramePalette, player: u8, gp: &str, frame: usize) -> String {
    format!("frame/{}/{player}/{}/{frame}", palette.name(), encode(gp))
}

/// The url of the same sprite enlarged `factor` times by the pixel-art scaler (`factor` 1 is
/// `url`): the touch skin's portraits.
pub fn scaled_url(url: &str, factor: u32) -> String {
    if factor > 1 { format!("scale/{factor}/{url}") } else { url.to_owned() }
}

pub fn remap_url(player: u8, gp: &str, frame: usize, table: &str, row: u8) -> String {
    format!("remap/{player}/{}/{frame}/{}/{row}", encode(gp), encode(table))
}

pub fn image_url(name: &str, version: u32) -> String {
    format!("image/{}/{version}", encode(name))
}

pub fn text_url(set: u8, font: FontId, fade: u8, text: &[u8]) -> String {
    let hex: String = text.iter().map(|b| format!("{b:02x}")).collect();
    format!("text/{set}/{}/{fade}/{hex}", font.0)
}

/// A sprite frame as an image. `None` while its files load (they are requested).
pub fn frame_image(
    lib: &Library,
    palette: FramePalette,
    player: u8,
    gp: &str,
    frame: usize,
) -> Result<Option<Placed>, AssetError> {
    let palette_path = palette.path(gp);
    for path in [gp, palette_path.as_str()] {
        lib.want(path);
    }
    if !lib.settled(gp) || !lib.settled(&palette_path) {
        return Ok(None);
    }
    let file = lib.sprites(gp).ok_or_else(|| missing(lib, gp))?;
    let colors = lib.palette(&palette_path).ok_or_else(|| missing(lib, &palette_path))?;
    rasterize_frame_as(&file, frame, &colors, player)
        .map(Some)
        .ok_or_else(|| AssetError::NotFound(format!("{gp} has no drawable frame {frame}")))
}

/// A game frame whose colours go through row `row` of the 256-byte table `table`
/// (`Sprite::GameFrameRemap`). `None` while its files load.
pub fn remap_image(
    lib: &Library,
    player: u8,
    gp: &str,
    frame: usize,
    table: &str,
    row: u8,
) -> Result<Option<Placed>, AssetError> {
    let palette_path = FramePalette::Sprite.path(gp);
    for path in [gp, palette_path.as_str(), table] {
        lib.want(path);
    }
    if ![gp, palette_path.as_str(), table].iter().all(|p| lib.settled(p)) {
        return Ok(None);
    }
    let file = lib.sprites(gp).ok_or_else(|| missing(lib, gp))?;
    let colors = lib.palette(&palette_path).ok_or_else(|| missing(lib, &palette_path))?;
    let bytes = lib.bytes(table).ok_or_else(|| missing(lib, table))?;
    let base = usize::from(row) * 256;
    let remapped =
        acr_formats::Palette(std::array::from_fn(|i| colors.rgb(bytes.get(base + i).copied().unwrap_or(i as u8))));
    rasterize_frame_as(&file, frame, &remapped, player)
        .map(Some)
        .ok_or_else(|| AssetError::NotFound(format!("{gp} has no drawable frame {frame}")))
}

/// Side of a cell of the 9-slice sources (`nine/`): the frames of `BORDER.GP` are 64 pixels
/// wide with the line through their middle.
pub const NINE_CELL: u32 = 64;

/// The frames of `BORDER.GP` for a 9-slice, in the order top left, top, top right, left, right,
/// bottom left, bottom, bottom right (`BOX_CORNERS` of the HUD, 0x668580).
const NINE_BOX: [usize; 8] = [0, 9, 2, 12, 14, 6, 11, 8];

/// `cells` (see [`NINE_BOX`]) pasted into a 3 x 3 grid of [`NINE_CELL`] pixels, the middle left
/// empty: the source of a CSS `border-image` with `slice` [`NINE_CELL`].
pub fn nine_grid(cells: &[Placed; 8]) -> Image {
    let side = NINE_CELL * 3;
    let mut out = Image::new(side, side);
    for (i, (image, offset)) in cells.iter().enumerate() {
        // The middle cell (4) is skipped in the numbering of the grid.
        let cell = if i < 4 { i } else { i + 1 } as u32;
        let (ox, oy) = (
            i64::from(cell % 3 * NINE_CELL) + i64::from(offset[0]),
            i64::from(cell / 3 * NINE_CELL) + i64::from(offset[1]),
        );
        for y in 0..image.height {
            for x in 0..image.width {
                let (dx, dy) = (ox + i64::from(x), oy + i64::from(y));
                if !(0..i64::from(side)).contains(&dx) || !(0..i64::from(side)).contains(&dy) {
                    continue;
                }
                let from = ((y * image.width + x) * 4) as usize;
                let to = ((dy as u32 * side + dx as u32) * 4) as usize;
                out.rgba[to..to + 4].copy_from_slice(&image.rgba[from..from + 4]);
            }
        }
    }
    out
}

/// The game's box frame as a 9-slice source (`nine/box.png`); `None` while its files load.
pub fn nine_box(lib: &Library) -> Result<Option<Image>, AssetError> {
    let mut cells = Vec::with_capacity(NINE_BOX.len());
    for frame in NINE_BOX {
        match frame_image(lib, FramePalette::Game, 0, "BORDER.GP", frame)? {
            Some(cell) => cells.push(cell),
            None => return Ok(None),
        }
    }
    let cells: [Placed; 8] = cells.try_into().expect("eight frames");
    Ok(Some(nine_grid(&cells)))
}

/// Side of the clean parchment patch cut from the main menu's map (`nine/parchment.png`).
const PARCHMENT_PATCH: u32 = 128;
/// Where it lies in frame 4 of `MAINMENU.GP`: a stretch of the paper without ink or folds.
const PARCHMENT_AT: [u32; 2] = [24, 120];

/// The paper of the main menu's map as a seamless tile (`nine/parchment.png`): a clean patch of
/// `MAINMENU.GP` mirrored into a 2 x 2 block, so that the edges meet. `None` while its files load.
pub fn parchment(lib: &Library) -> Result<Option<Image>, AssetError> {
    let Some((map, _)) = frame_image(lib, FramePalette::Menu, 0, "INTERF2\\MAINMENU.GP", 4)? else { return Ok(None) };
    let side = PARCHMENT_PATCH;
    let [ox, oy] = PARCHMENT_AT;
    if map.width < ox + side || map.height < oy + side {
        return Err(AssetError::Decode(format!(
            "MAINMENU.GP frame 4 is {}x{}, too small for the paper",
            map.width, map.height
        )));
    }
    let mut out = Image::new(side * 2, side * 2);
    for y in 0..side * 2 {
        for x in 0..side * 2 {
            let (sx, sy) = (if x < side { x } else { side * 2 - 1 - x }, if y < side { y } else { side * 2 - 1 - y });
            let from = (((oy + sy) * map.width + ox + sx) * 4) as usize;
            let to = ((y * side * 2 + x) * 4) as usize;
            out.rgba[to..to + 4].copy_from_slice(&map.rgba[from..from + 4]);
        }
    }
    Ok(Some(out))
}

/// A bitmap as an image; `None` while it loads.
pub fn bitmap_image(lib: &Library, path: &str) -> Result<Option<Placed>, AssetError> {
    lib.want(path);
    if !lib.settled(path) {
        return Ok(None);
    }
    let bitmap = lib.bitmap(path).ok_or_else(|| missing(lib, path))?;
    Ok(Some((Image { width: bitmap.width, height: bitmap.height, rgba: bitmap.to_rgba() }, [0, 0])))
}

/// A string as an image, `None` while the fonts load. An empty string has no image.
pub fn text_image(
    lib: &Library,
    fonts: &Fonts,
    set: u8,
    font: FontId,
    fade: u8,
    text: &[u8],
) -> Result<Option<Option<Placed>>, AssetError> {
    for path in fonts.sources() {
        lib.want(&path);
    }
    Ok(fonts.rasterize_faded(lib, font, text, set, i32::from(fade)))
}

/// A file that is settled but not usable: absent or undecodable.
fn missing(lib: &Library, path: &str) -> AssetError {
    if lib.bytes(path).is_some() {
        AssetError::Decode(format!("{path} does not decode"))
    } else {
        AssetError::NotFound(format!("{path} is not in the game files"))
    }
}

/// The font a path names.
pub fn font_id(name: &str) -> Result<FontId, AssetError> {
    acr_formats::font::named_font(name)
        .map(|f| FontId(f.name))
        .ok_or_else(|| AssetError::BadPath(format!("unknown font {name}")))
}

fn bad(path: &str) -> AssetError {
    AssetError::BadPath(format!("bad asset path {path}"))
}

pub fn png(image: &Image) -> Result<Vec<u8>, AssetError> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, image.width, image.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .and_then(|mut w| w.write_image_data(&image.rgba))
        .map_err(|e| AssetError::Decode(format!("png: {e}")))?;
    Ok(out)
}

fn ready(image: Option<Placed>, what: &str) -> Result<Resolved, AssetError> {
    match image {
        Some((image, _)) => Ok(Resolved::Ready(AssetBody::Png(png(&image)?))),
        None => Err(AssetError::NotFound(format!("{what} draws nothing"))),
    }
}

/// The sprite a `frame/..` or `remap/..` path (without the leading `scale/<n>/`) names; `None`
/// while its files load.
fn sprite_asset(lib: &Library, parts: &[&str], path: &str) -> Result<Option<Placed>, AssetError> {
    match parts {
        ["frame", palette, player, gp, frame] => {
            let palette = match *palette {
                "menu" => FramePalette::Menu,
                "game" => FramePalette::Game,
                "sprite" => FramePalette::Sprite,
                _ => return Err(bad(path)),
            };
            let player: u8 = player.parse().map_err(|_| bad(path))?;
            let frame: usize = frame.parse().map_err(|_| bad(path))?;
            let gp = decode(gp).ok_or_else(|| bad(path))?;
            frame_image(lib, palette, player, &gp, frame)
        }
        ["remap", player, gp, frame, table, row] => {
            let player: u8 = player.parse().map_err(|_| bad(path))?;
            let frame: usize = frame.parse().map_err(|_| bad(path))?;
            let row: u8 = row.parse().map_err(|_| bad(path))?;
            let (gp, table) = (decode(gp).ok_or_else(|| bad(path))?, decode(table).ok_or_else(|| bad(path))?);
            remap_image(lib, player, &gp, frame, &table, row)
        }
        _ => Err(bad(path)),
    }
}

/// Answers `path` (below `/asset/v1/`), or says to try again after more files arrived.
pub fn resolve(lib: &Library, fonts: &Fonts, cache: &mut AssetCache, path: &str) -> Result<Resolved, AssetError> {
    let parts: Vec<&str> = path.split('/').collect();
    match parts.as_slice() {
        ["bmp", file] => {
            let file = decode(file).ok_or_else(|| bad(path))?;
            match bitmap_image(lib, &file)? {
                Some(image) => ready(Some(image), &file),
                None => Ok(Resolved::Pending),
            }
        }
        ["frame", ..] | ["remap", ..] => match sprite_asset(lib, &parts, path)? {
            Some(image) => ready(Some(image), path),
            None => Ok(Resolved::Pending),
        },
        ["scale", factor, rest @ ..] => {
            let factor: u32 = factor.parse().map_err(|_| bad(path))?;
            if !matches!(factor, 1 | 2 | MAX_SCALE) || !matches!(rest, ["frame", ..] | ["remap", ..]) {
                return Err(bad(path));
            }
            match sprite_asset(lib, rest, path)? {
                Some((image, offset)) => {
                    let image = upscale(&image, factor).ok_or_else(|| bad(path))?;
                    ready(Some((image, offset)), path)
                }
                None => Ok(Resolved::Pending),
            }
        }
        ["nine", "box.png"] => match nine_box(lib)? {
            Some(image) => ready(Some((image, [0, 0])), "the box frame"),
            None => Ok(Resolved::Pending),
        },
        ["nine", "parchment.png"] => match parchment(lib)? {
            Some(image) => ready(Some((image, [0, 0])), "the parchment"),
            None => Ok(Resolved::Pending),
        },
        ["image", name, version] => {
            let name = decode(name).ok_or_else(|| bad(path))?;
            let version: u32 = version.parse().map_err(|_| bad(path))?;
            let image = cache
                .images
                .get(&name)
                .and_then(|v| v.iter().find(|(n, _)| *n == version))
                .ok_or_else(|| AssetError::NotFound(format!("{name} has no version {version}")))?;
            Ok(Resolved::Ready(AssetBody::Png(png(&image.1)?)))
        }
        ["font", set, file] => {
            let set: u8 = set.parse().ok().filter(|s| *s <= 1).ok_or_else(|| bad(path))?;
            let (name, json) = match file.strip_suffix(".png") {
                Some(name) => (name, false),
                None => (file.strip_suffix(".json").ok_or_else(|| bad(path))?, true),
            };
            let font = font_id(name)?;
            let key = (set, name.to_owned());
            if !cache.atlases.contains_key(&key) {
                for path in fonts.sources() {
                    lib.want(&path);
                }
                let Some(atlas) = font_atlas::build(lib, fonts, font, set)? else { return Ok(Resolved::Pending) };
                cache.atlases.insert(key.clone(), atlas);
            }
            let atlas = &cache.atlases[&key];
            Ok(Resolved::Ready(if json {
                AssetBody::Json(atlas.json.clone())
            } else {
                AssetBody::Png(atlas.png.clone())
            }))
        }
        ["text", set, font, fade, hex] => {
            let set: u8 = set.parse().ok().filter(|s| *s <= 1).ok_or_else(|| bad(path))?;
            let fade: u8 = fade.parse().map_err(|_| bad(path))?;
            let font = font_id(font)?;
            if hex.len() % 2 != 0 {
                return Err(bad(path));
            }
            let text = (0..hex.len())
                .step_by(2)
                .map(|i| hex.get(i..i + 2).and_then(|h| u8::from_str_radix(h, 16).ok()))
                .collect::<Option<Vec<u8>>>()
                .ok_or_else(|| bad(path))?;
            match text_image(lib, fonts, set, font, fade, &text)? {
                Some(image) => ready(image, "the text"),
                None => Ok(Resolved::Pending),
            }
        }
        _ => Err(bad(path)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_paths_survive_the_url() {
        let path = "INTERF2\\BACKGROUND_MAIN_MENU.BMP";
        let url = bmp_url(path);
        assert_eq!(url, "bmp/INTERF2%5CBACKGROUND_MAIN_MENU.BMP");
        assert_eq!(decode(url.strip_prefix("bmp/").unwrap()).as_deref(), Some(path));
        assert_eq!(decode("%zz"), None);
        assert_eq!(decode("%4"), None);
    }

    #[test]
    fn the_nine_grid_puts_each_frame_in_its_cell() {
        let cell = |shade: u8| -> Placed {
            let mut image = Image::new(2, 2);
            image.rgba.iter_mut().for_each(|b| *b = shade);
            (image, [1, 1])
        };
        let cells: [Placed; 8] = std::array::from_fn(|i| cell(i as u8 + 1));
        let grid = nine_grid(&cells);
        assert_eq!((grid.width, grid.height), (192, 192));
        let at = |cx: u32, cy: u32| grid.rgba[(((cy * NINE_CELL + 1) * grid.width + cx * NINE_CELL + 1) * 4) as usize];
        // Row by row, the middle cell empty.
        assert_eq!(
            [at(0, 0), at(1, 0), at(2, 0), at(0, 1), at(1, 1), at(2, 1), at(0, 2), at(1, 2), at(2, 2)],
            [1, 2, 3, 4, 0, 5, 6, 7, 8]
        );
    }

    #[test]
    fn bad_paths_fail_with_400() {
        let lib = Library::new(&[]);
        let fonts = Fonts::default();
        let mut cache = AssetCache::default();
        for path in [
            "",
            "nope",
            "bmp",
            "frame/x/0/a/1",
            "frame/menu/0/a/b",
            "font/1/NoSuchFont.png",
            "font/7/fonG60w.png",
            "nine/other.png",
            "nine/parchment",
            "scale",
            "scale/3/frame/menu/0/a/1",
            "scale/x/frame/menu/0/a/1",
            "scale/2/bmp/a",
            "scale/2/scale/2/frame/menu/0/a/1",
        ] {
            let e = resolve(&lib, &fonts, &mut cache, path).err().unwrap_or_else(|| panic!("{path}"));
            assert_eq!(e.status(), 400, "{path}");
        }
        let e = resolve(&lib, &fonts, &mut cache, "text/1/fonG60w/0/abc").err().unwrap();
        assert_eq!(e.status(), 400);
    }

    #[test]
    fn unsettled_files_are_pending_and_absent_ones_404() {
        let mut lib = Library::new(&["A.BMP".to_owned()]);
        let fonts = Fonts::default();
        let mut cache = AssetCache::default();
        assert!(matches!(resolve(&lib, &fonts, &mut cache, "bmp/A.BMP"), Ok(Resolved::Pending)));
        assert_eq!(lib.take_requests(), vec!["A.BMP".to_owned()]);
        // The parchment waits for the map's frames, and is absent without them.
        let mut lib = Library::new(&["INTERF2\\MAINMENU.GP".to_owned()]);
        assert!(matches!(resolve(&lib, &fonts, &mut cache, "nine/parchment.png"), Ok(Resolved::Pending)));
        assert!(lib.take_requests().contains(&"INTERF2\\MAINMENU.GP".to_owned()));
        let none = Library::new(&[]);
        assert_eq!(resolve(&none, &fonts, &mut cache, "nine/parchment.png").err().map(|e| e.status()), Some(404));
        // A scaled frame waits for the same files as the frame.
        let mut lib = Library::new(&["ICONS.GP".to_owned(), GAME_PALETTE.to_owned()]);
        assert!(matches!(resolve(&lib, &fonts, &mut cache, "scale/4/frame/game/0/ICONS.GP/3"), Ok(Resolved::Pending)));
        assert_eq!(lib.take_requests(), vec!["ICONS.GP".to_owned(), GAME_PALETTE.to_owned()]);
    }

    #[test]
    fn urls_name_the_font_and_the_bytes() {
        assert_eq!(text_url(1, FontId("fonG60w"), 0, b"Ab\xfc"), "text/1/fonG60w/0/4162fc");
        assert_eq!(frame_url(FramePalette::Menu, 0, "CURSOR_01.GP", 0), "frame/menu/0/CURSOR_01.GP/0");
        assert_eq!(scaled_url("frame/menu/0/A.GP/0", 4), "scale/4/frame/menu/0/A.GP/0");
        assert_eq!(scaled_url("frame/menu/0/A.GP/0", 1), "frame/menu/0/A.GP/0");
    }
}
