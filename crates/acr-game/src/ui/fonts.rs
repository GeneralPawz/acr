//! The original's named fonts (`acr_formats::font`), as used by the menus.
//!
//! Text is drawn with the menu palette (set 1) and its colour tables; rasterized strings
//! are cached as textures by [`super::draw::Textures`].

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use acr_formats::font::{COLOR_TABLE_COUNT, Font, FontColors, color_table_path, named_font};
use acr_formats::raster::Image;

use super::draw::{GAME_PALETTE, MENU_PALETTE};
use super::library::Library;

/// A global `RLCFont` object of the original (`fonG60w`, `fon18y3`, ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FontId(pub &'static str);

/// Palette set of the menus (`1\agew_1.pal`).
const MENU_SET: u8 = 1;
/// Palette set of the game screen (`0\agew_1.pal`), used by the in-game interface.
pub const GAME_SET: u8 = 0;

/// Fonts the menus use; their files are loaded before the first screen.
const MENU_FONTS: &[&str] = &[
    "fonG60w",
    "fonG60y5",
    "fon40y5",
    "fon18w",
    "fon18y3",
    "fon18y5",
    "fon16y1",
    "fon16y3",
    "fon16y5",
    "fon13y2",
    "SmallYellowFont",
    "SmallYellowFont1",
    "SmallWhiteFont",
    // The in-game interface.
    "YellowFont",
    "WhiteFont",
    "BlackFont",
    "SmallBlackFont",
    "SmallBlackFont1",
    "SpecialWhiteFont",
    "SpecialYellowFont",
    "fn10",
    // Mission messages (`mission_ui`).
    "BigWhiteFont",
    "rom11",
    "rom11red",
];

#[derive(Default)]
pub struct Fonts {
    fonts: RefCell<HashMap<FontId, Option<Rc<Font>>>>,
    colors: RefCell<Option<Rc<FontColors>>>,
    game_colors: RefCell<Option<Rc<FontColors>>>,
}

impl Fonts {
    /// Files the fonts need: sprite packs, metrics, palette and colour tables.
    pub fn sources(&self) -> Vec<String> {
        let mut files = vec![MENU_PALETTE.to_owned(), GAME_PALETTE.to_owned()];
        for name in MENU_FONTS {
            let Some(font) = named_font(name) else { continue };
            files.push(font.gp_path());
            files.push(font.gpi_path());
            if let Some((gp, gpi)) = font.shadow_paths() {
                files.extend([gp, gpi]);
            }
        }
        for mode in 0..COLOR_TABLE_COUNT {
            for set in [MENU_SET, 0] {
                files.push(color_table_path(set, mode));
            }
        }
        files.sort();
        files.dedup();
        files
    }

    pub fn ready(&self, lib: &Library) -> bool {
        self.sources().iter().all(|p| lib.settled(p))
    }

    fn font(&self, lib: &Library, id: FontId) -> Option<Rc<Font>> {
        self.fonts
            .borrow_mut()
            .entry(id)
            .or_insert_with(|| {
                let named = named_font(id.0)?;
                named
                    .load(|p| lib.bytes(p).map(|b| b.to_vec()))
                    .map_err(|e| log::error!("font {}: {e}", id.0))
                    .ok()
                    .map(Rc::new)
            })
            .clone()
    }

    fn colors(&self, lib: &Library, set: u8) -> Option<Rc<FontColors>> {
        let (cache, path) = match set {
            GAME_SET => (&self.game_colors, GAME_PALETTE),
            _ => (&self.colors, MENU_PALETTE),
        };
        if cache.borrow().is_none() {
            let palette = lib.palette(path)?;
            let colors = FontColors::load((*palette).clone(), set, |p| lib.bytes(p).map(|b| b.to_vec()));
            *cache.borrow_mut() = Some(Rc::new(colors));
        }
        cache.borrow().clone()
    }

    /// `GetRLCStrWidth`.
    pub fn width(&self, lib: &Library, font: FontId, text: &[u8]) -> i32 {
        self.font(lib, font).map_or(0, |f| f.text_width(text))
    }

    /// `GetRLCHeight(font, 'W')`.
    pub fn height(&self, lib: &Library, font: FontId) -> i32 {
        self.font(lib, font).map_or(0, |f| f.height())
    }

    /// `GetRLCHeight(font, c)`: the height of one glyph frame.
    pub fn glyph_height(&self, lib: &Library, font: FontId, c: u8) -> i32 {
        self.font(lib, font).map_or(0, |f| f.glyph_height(c))
    }

    /// Width of one character (`GetRLCWidthUNICODE`).
    pub fn char_width(&self, lib: &Library, font: FontId, c: u8) -> i32 {
        self.font(lib, font).map_or(0, |f| f.char_width(c))
    }

    /// The text as an image and the offset of its top-left pixel from the `ShowString`
    /// position; `None` while files are loading, `Some(None)` if nothing is drawn.
    #[allow(clippy::type_complexity)]
    pub fn rasterize(&self, lib: &Library, font: FontId, text: &[u8]) -> Option<Option<(Image, [i32; 2])>> {
        self.rasterize_in(lib, font, text, MENU_SET)
    }

    /// [`Fonts::rasterize`] with the colours of palette set `set`.
    #[allow(clippy::type_complexity)]
    pub fn rasterize_in(&self, lib: &Library, font: FontId, text: &[u8], set: u8) -> Option<Option<(Image, [i32; 2])>> {
        if !self.ready(lib) {
            return None;
        }
        let (Some(font), Some(colors)) = (self.font(lib, font), self.colors(lib, set)) else { return Some(None) };
        let t = font.draw(text, &colors);
        if t.image.width == 0 || t.image.height == 0 {
            return Some(None);
        }
        Some(Some((t.image, [t.left, t.top])))
    }
}
