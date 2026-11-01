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
//!
//! Failures are errors, never placeholders: a bad path, a missing file or a file that does not
//! decode.

use std::collections::HashMap;

use acr_formats::raster::Image;

use super::font_atlas;
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

/// Where an answer goes once it is known.
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
            match frame_image(lib, palette, player, &gp, frame)? {
                Some(image) => ready(Some(image), &format!("{gp} frame {frame}")),
                None => Ok(Resolved::Pending),
            }
        }
        ["remap", player, gp, frame, table, row] => {
            let player: u8 = player.parse().map_err(|_| bad(path))?;
            let frame: usize = frame.parse().map_err(|_| bad(path))?;
            let row: u8 = row.parse().map_err(|_| bad(path))?;
            let (gp, table) = (decode(gp).ok_or_else(|| bad(path))?, decode(table).ok_or_else(|| bad(path))?);
            match remap_image(lib, player, &gp, frame, &table, row)? {
                Some(image) => ready(Some(image), &format!("{gp} frame {frame} through {table}")),
                None => Ok(Resolved::Pending),
            }
        }
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
    fn bad_paths_fail_with_400() {
        let lib = Library::new(&[]);
        let fonts = Fonts::default();
        let mut cache = AssetCache::default();
        for path in
            ["", "nope", "bmp", "frame/x/0/a/1", "frame/menu/0/a/b", "font/1/NoSuchFont.png", "font/7/fonG60w.png"]
        {
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
    }

    #[test]
    fn urls_name_the_font_and_the_bytes() {
        assert_eq!(text_url(1, FontId("fonG60w"), 0, b"Ab\xfc"), "text/1/fonG60w/0/4162fc");
        assert_eq!(frame_url(FramePalette::Menu, 0, "CURSOR_01.GP", 0), "frame/menu/0/CURSOR_01.GP/0");
    }
}
