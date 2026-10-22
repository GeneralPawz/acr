//! A screen's widgets: the counterpart of the original's `DialogsSystem` (`ProcessDialogs`
//! 0x47cfcd).
//!
//! Widgets are kept in creation order, which is the processing and drawing order. The
//! behaviour follows the original (see `docs/ui.md`):
//! - Hit rectangles are inclusive and also limited by the clip rectangle that was active
//!   when the widget was added (`addClipper`).
//! - A click happens on mouse **press**. A widget with a `param` reports it (the
//!   `OnUserClick = MMItemChoose` pattern) and plays the click sound.
//! - Widgets with a hover sound play it when the mouse enters them.
//! - Lists select on press, scrollbar arrows repeat every 100 ms while held, sliders and
//!   scrollbar thumbs can be dragged, combo boxes open a drop-down that takes all input.

use super::draw::{Draw, Rect};
use super::fonts::{FontId, Fonts};
use super::library::Library;

pub type WidgetId = usize;

/// Sounds of the menus (`SoundList.txt`).
pub mod sound {
    /// `STANDART`: every click on an entry (`DialogsSystem::ClickSound`).
    pub const CLICK: &str = "SOUND\\STANDARTKEY.WAV";
    /// `START`: the OK sound of screens that start a game.
    pub const START: &str = "SOUND\\START.WAV";
    /// `SCROLL` (`@MOUSESOUND` in `Const.lst`): the mouse enters a main menu entry; one
    /// of three variants at random.
    pub const HOVER: [&str; 3] = ["SOUND\\MENUSVITOK1.WAV", "SOUND\\MENUSVITOK2.WAV", "SOUND\\MENUSVITOK3.WAV"];
}

/// `UserParam` of OK and cancel buttons (`mcmOk`, `mcmCancel`), also produced by Enter and
/// Escape (`StdKeys` 0x59bde0).
pub const OK: i32 = 0xf5;
pub const CANCEL: i32 = 0xf6;

/// Scroll units per list row or text line.
const UNITS_PER_ROW: i32 = 32;

/// Horizontal text alignment relative to the anchor (`addTextButton`'s last argument).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
}

#[derive(Debug, Clone)]
pub struct TextButton {
    pub text: Vec<u8>,
    /// Font under the mouse.
    pub active: FontId,
    pub passive: FontId,
    pub disabled: FontId,
    pub align: Align,
    /// The x passed at creation: left edge or centre depending on `align`.
    pub anchor: i32,
}

#[derive(Debug, Clone)]
pub struct GpTextButton {
    pub text: Vec<u8>,
    pub gp: String,
    /// Highlight plate shown under the mouse.
    pub sprite: usize,
    pub active: FontId,
    pub passive: FontId,
    pub font_dy: i32,
}

#[derive(Debug, Clone)]
pub struct ListBox {
    pub items: Vec<Vec<u8>>,
    pub cur: usize,
    pub first: usize,
    pub rows: usize,
    pub row_h: i32,
    pub gp: String,
    /// Frames of the selected row highlight: `base + visible row % 3`.
    pub selected_base: i32,
    pub dx: i32,
    pub dy: i32,
    pub active: FontId,
    pub passive: FontId,
    pub hovered: Option<usize>,
    pub scroll: Option<WidgetId>,
    /// Keyboard focus (flag bit 1): Up/Down/PgUp/PgDn move the selection.
    pub focus: bool,
}

#[derive(Debug, Clone)]
pub struct ScrollBar {
    pub gp: String,
    pub base: usize,
    pub max: i32,
    pub pos: i32,
    pub step: i32,
    pub page: i32,
    arrow_h: i32,
    thumb: [i32; 2],
    track: i32,
    drag: Option<(f32, i32)>,
    held: Option<i32>,
    last_repeat: f64,
}

#[derive(Debug, Clone)]
pub struct Slider {
    pub gp: String,
    pub frame: usize,
    pub max: i32,
    pub pos: i32,
    size: [i32; 2],
    thumb: [i32; 2],
    offset: [i32; 2],
    drag: Option<(f32, i32)>,
}

#[derive(Debug, Clone)]
pub struct ComboBox {
    pub lines: Vec<Vec<u8>>,
    pub cur: usize,
    pub gp: String,
    pub up: usize,
    /// x offset of the drop-down from the box (`Center`, +0xa7).
    pub drop_dx: i32,
    pub active: FontId,
    pub passive: FontId,
    /// Text offset of the list rows (`FontDy`, +0x6f) and of the closed box (`OneDy`, +0x83).
    pub font_dy: i32,
    pub one_dy: i32,
    pub open: bool,
    light: usize,
    width: i32,
    /// `addGP_ComboBoxDLX`: the frame is this much wider than the box; it is drawn as a
    /// left and a right half (`DLX`, +0xc9).
    dlx: i32,
    row_h: i32,
    head_h: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Line {
    start: usize,
    len: usize,
    width: i32,
    spaces: i32,
    justify: bool,
}

#[derive(Debug, Clone)]
pub struct TextViewer {
    text: Vec<u8>,
    font: FontId,
    lines: Vec<Line>,
    pub line: usize,
    page: usize,
    line_h: i32,
    width: i32,
    pub scroll: Option<WidgetId>,
    pub focus: bool,
}

impl InputBox {
    /// Replaces the text and puts the caret at its end.
    pub fn set_text(&mut self, text: &[u8]) {
        self.text = text.to_vec();
        self.caret = text.len();
    }
}

impl TextViewer {
    /// Overrides the line height (`SymSize`, +0x7b) for a viewer `height` pixels tall.
    pub fn set_line_height(&mut self, line_h: i32, height: i32) {
        self.line_h = line_h.max(1);
        self.page = (height / self.line_h) as usize;
    }

    /// The height of all lines (`NLines` +0x6b times `SymSize` +0x7b).
    pub fn text_height(&self) -> i32 {
        self.lines.len() as i32 * self.line_h
    }

    /// Changes the font (`Font`, +0x7d); the lines stay wrapped as before.
    pub fn set_font(&mut self, font: FontId) {
        self.font = font;
    }
}

#[derive(Debug, Clone)]
pub struct InputBox {
    pub text: Vec<u8>,
    pub capacity: usize,
    caret: usize,
    pub passive: FontId,
    pub active: FontId,
    pub focus: bool,
}

#[derive(Debug, Clone)]
pub enum Kind {
    /// An area without drawing (`addViewPort`).
    Area,
    /// A one-pixel frame around the area in a colour (`ColoredBar` with `+0x64 = 1`,
    /// 0x476094).
    Outline([u8; 4]),
    /// A bitmap with its top-left corner at the widget position (`addPicture`, `addBPXView`).
    Picture {
        path: String,
    },
    /// A sprite frame with its origin at the widget position (`addGPPicture`).
    GpPicture {
        gp: String,
        frame: usize,
    },
    TextButton(TextButton),
    /// `addGP_Button`: `active` frame under the mouse, else `passive`.
    GpButton {
        gp: String,
        active: usize,
        passive: usize,
    },
    GpTextButton(GpTextButton),
    ListBox(ListBox),
    ScrollBar(ScrollBar),
    Slider(Slider),
    ComboBox(ComboBox),
    TextViewer(TextViewer),
    InputBox(InputBox),
    Universal(Universal),
}

/// `addUniversalButton` (0x46ff83): a strip of a left cap, middle pieces picked by
/// `random.lst` and a right cap with a centred text. `frames` holds `[left, mid, mid, mid,
/// right]` for the states normal, selected, selected again, then the same under the mouse.
#[derive(Debug, Clone)]
pub struct Universal {
    pub gp: String,
    pub frames: [[usize; 5]; 6],
    pub width: i32,
    pub text: Vec<u8>,
    pub active: Option<FontId>,
    pub passive: Option<FontId>,
    pub selected: Option<FontId>,
    /// 0 normal, 1 selected, 2 selected again (clicking a selected radio button).
    pub state: usize,
    /// Radio group: clicking selects this button and clears the others of the group.
    pub group: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct Widget {
    /// Position: top-left for pictures and text, frame origin for sprites.
    pub x: i32,
    pub y: i32,
    /// Area that reacts to the mouse.
    pub rect: Rect,
    /// Clipping rectangle active when the widget was added (`addClipper`).
    pub clip: Rect,
    pub enabled: bool,
    pub visible: bool,
    pub hover: bool,
    /// Value reported on click (`UserParam` with `OnUserClick = MMItemChoose`).
    pub param: Option<i32>,
    /// Hint shown in the status line while hovered.
    pub hint: Option<Vec<u8>>,
    /// Plays [`sound::HOVER`] when the mouse enters (`AssignSound(.., 0x4321)`).
    pub hover_sound: bool,
    /// A scroll bar that moves this widget up by its position (`ParentSB`, +0x5b).
    pub scroll_owner: Option<WidgetId>,
    pub kind: Kind,
}

/// The result of an input event.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// `ItemChoose`: the `param` of a clicked widget, or [`OK`]/[`CANCEL`] from the keyboard.
    pub chosen: Option<i32>,
    /// Game paths of sounds to play.
    pub sounds: Vec<String>,
}

/// Keys the widgets react to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Up,
    Down,
    PageUp,
    PageDown,
    Left,
    Right,
    Home,
    End,
    Backspace,
    Delete,
    Enter,
    Escape,
    /// A typed character (CP1252).
    Char(u8),
}

#[derive(Default)]
pub struct Dialog {
    pub widgets: Vec<Widget>,
    clip: Option<Rect>,
    /// The open combo box, which takes all mouse input inside its drop-down.
    modal: Option<WidgetId>,
    mouse: Option<[f32; 2]>,
    held: bool,
    /// Milliseconds since the dialog was created, advanced by [`Dialog::tick`].
    now: f64,
    /// Click sound of [`OK`] (`DialogsSystem::OkSound`), `STANDART` unless set.
    pub ok_sound: Option<&'static str>,
    /// `RANDOM.LST`: 8192 pseudo-random `i16` (row variants of drop-down lists, middle
    /// pieces of strips).
    random: Vec<i16>,
    hover_variant: u32,
}

impl Dialog {
    /// Restricts the widgets added from now on to `rect` (`addClipper`); `None` resets.
    pub fn set_clip(&mut self, rect: Option<Rect>) {
        self.clip = rect;
    }

    fn push(&mut self, x: i32, y: i32, rect: Rect, kind: Kind) -> WidgetId {
        self.widgets.push(Widget {
            x,
            y,
            rect,
            clip: self.clip.unwrap_or(Rect::SCREEN),
            enabled: true,
            visible: true,
            hover: false,
            param: None,
            hint: None,
            hover_sound: false,
            scroll_owner: None,
            kind,
        });
        self.widgets.len() - 1
    }

    /// Loads `RANDOM.LST` (8192 little-endian `i16`).
    pub fn load_random(&mut self, lib: &Library) {
        if let Some(bytes) = lib.bytes(RANDOM_LST) {
            self.random = bytes.as_chunks::<2>().0.iter().map(|&b| i16::from_le_bytes(b)).collect();
        }
    }

    fn random(&self, i: usize) -> i32 {
        i32::from(self.random.get(i % self.random.len().max(1)).copied().unwrap_or(0))
    }

    /// A decorative strip of `width` pixels: a disabled [`Universal`] without text.
    pub fn add_strip(&mut self, lib: &Library, x: i32, y: i32, width: i32, gp: &str, frames: [usize; 5]) -> WidgetId {
        let id = self.add_universal(lib, x, y, width, b"", gp, [frames; 6], [None; 3], None);
        self.widgets[id].enabled = false;
        id
    }

    /// `addUniversalButton(x, y, Lx, text, gp, frames, group, selected, tiling, active,
    /// passive, selected)`; the rect is one pixel taller than the frame.
    #[allow(clippy::too_many_arguments)]
    pub fn add_universal(
        &mut self,
        lib: &Library,
        x: i32,
        y: i32,
        width: i32,
        text: &[u8],
        gp: &str,
        frames: [[usize; 5]; 6],
        [active, passive, selected]: [Option<FontId>; 3],
        group: Option<i32>,
    ) -> WidgetId {
        let h = lib.frame_size(gp, frames[0][0])[1];
        let button = Universal {
            gp: gp.to_owned(),
            frames,
            width,
            text: text.to_vec(),
            active,
            passive,
            selected,
            state: 0,
            group,
        };
        self.push(x, y, Rect::inclusive(x, y, x + width - 1, y + h), Kind::Universal(button))
    }

    pub fn universal_mut(&mut self, id: WidgetId) -> &mut Universal {
        match &mut self.widgets[id].kind {
            Kind::Universal(u) => u,
            _ => panic!("widget {id} is not a universal button"),
        }
    }

    pub fn add_area(&mut self, rect: Rect) -> WidgetId {
        self.push(rect.x0, rect.y0, rect, Kind::Area)
    }

    /// `ColoredBar(x, y, w, h, colour)` as an outline: covers `w + 1` × `h + 1` pixels.
    pub fn add_outline(&mut self, x: i32, y: i32, w: i32, h: i32, rgba: [u8; 4]) -> WidgetId {
        self.push(x, y, Rect::new(x, y, w + 1, h + 1), Kind::Outline(rgba))
    }

    pub fn add_picture(&mut self, lib: &Library, x: i32, y: i32, path: &str) -> WidgetId {
        let size = lib.bitmap(path).map_or([0, 0], |b| [b.width as i32, b.height as i32]);
        self.push(x, y, Rect::new(x, y, size[0], size[1]), Kind::Picture { path: path.to_owned() })
    }

    pub fn add_gp_picture(&mut self, lib: &Library, x: i32, y: i32, gp: &str, frame: usize) -> WidgetId {
        let [w, h] = lib.frame_size(gp, frame);
        self.push(x, y, Rect::new(x, y, w, h), Kind::GpPicture { gp: gp.to_owned(), frame })
    }

    /// `addTextButton(parent, x, y, text, active, passive, disabled, align)` (0x46ebc9): the
    /// mouse area is the text's width by the font height + 1.
    #[allow(clippy::too_many_arguments)]
    pub fn add_text_button(
        &mut self,
        lib: &Library,
        fonts: &Fonts,
        x: i32,
        y: i32,
        text: &[u8],
        [active, passive, disabled]: [FontId; 3],
        align: Align,
    ) -> WidgetId {
        let width = fonts.width(lib, passive, text);
        let left = if align == Align::Center { x - width / 2 } else { x };
        let rect = Rect::inclusive(left, y, left + width - 1, y + fonts.height(lib, passive));
        let button = TextButton { text: text.to_vec(), active, passive, disabled, align, anchor: x };
        self.push(left, y, rect, Kind::TextButton(button))
    }

    /// A text in one font (`addTextButton` with three equal fonts).
    #[allow(clippy::too_many_arguments)]
    pub fn add_text(
        &mut self,
        lib: &Library,
        fonts: &Fonts,
        x: i32,
        y: i32,
        text: &[u8],
        font: FontId,
        align: Align,
    ) -> WidgetId {
        self.add_text_button(lib, fonts, x, y, text, [font; 3], align)
    }

    /// `addGP_Button(parent, x, y, gp, active, passive)` (0x470a5c).
    pub fn add_gp_button(
        &mut self,
        lib: &Library,
        x: i32,
        y: i32,
        gp: &str,
        active: usize,
        passive: usize,
    ) -> WidgetId {
        let [w, h] = lib.frame_size(gp, passive);
        self.push(x, y, Rect::new(x, y, w, h), Kind::GpButton { gp: gp.to_owned(), active, passive })
    }

    /// `addGP_TextButton(parent, x, y, text, gp, sprite, active, passive)` (0x46f0eb): the
    /// text centred on the frame, the frame itself shown only under the mouse. `FontDy`
    /// starts at -1.
    #[allow(clippy::too_many_arguments)]
    pub fn add_gp_text_button(
        &mut self,
        lib: &Library,
        x: i32,
        y: i32,
        text: &[u8],
        gp: &str,
        sprite: usize,
        active: FontId,
        passive: FontId,
    ) -> WidgetId {
        let [w, h] = lib.frame_size(gp, sprite);
        let button = GpTextButton { text: text.to_vec(), gp: gp.to_owned(), sprite, active, passive, font_dy: -1 };
        self.push(x, y, Rect::new(x, y, w, h), Kind::GpTextButton(button))
    }

    /// `addNewGP_VScrollBar(parent, x, y, Ly, max, pos, gp, base)` (0x472884).
    #[allow(clippy::too_many_arguments)]
    pub fn add_scroll_bar(
        &mut self,
        lib: &Library,
        x: i32,
        y: i32,
        height: i32,
        max: i32,
        pos: i32,
        gp: &str,
        base: usize,
    ) -> WidgetId {
        let [w, arrow_h] = lib.frame_size(gp, base);
        let thumb = [w, lib.frame_size(gp, base + 4)[1]];
        let bar = ScrollBar {
            gp: gp.to_owned(),
            base,
            max,
            pos,
            step: 1,
            page: 0,
            arrow_h,
            thumb,
            track: height - 2 * arrow_h,
            drag: None,
            held: None,
            last_repeat: 0.0,
        };
        self.push(x, y, Rect::new(x, y, w, height), Kind::ScrollBar(bar))
    }

    /// `addGP_ScrollBarL(parent, x, y, max, pos, gp, thumb, Lx, Ly, 0, 0)` (0x472add): a
    /// horizontal slider whose thumb is centred on the value point.
    #[allow(clippy::too_many_arguments)]
    pub fn add_slider(
        &mut self,
        lib: &Library,
        x: i32,
        y: i32,
        max: i32,
        pos: i32,
        gp: &str,
        frame: usize,
        size: [i32; 2],
    ) -> WidgetId {
        let thumb = lib.frame_size(gp, frame);
        let slider = Slider {
            gp: gp.to_owned(),
            frame,
            max,
            pos: pos.clamp(0, max),
            size,
            thumb,
            offset: [-(thumb[0] / 2), -(thumb[1] / 2)],
            drag: None,
        };
        self.push(x, y, Rect::new(x, y, size[0], size[1]), Kind::Slider(slider))
    }

    /// `addGP_ListBox(parent, x, y, Ny, gp, frame, rowH, active, passive, scroll)` (0x474459).
    #[allow(clippy::too_many_arguments)]
    pub fn add_list_box(
        &mut self,
        lib: &Library,
        x: i32,
        y: i32,
        rows: usize,
        gp: &str,
        frame: i32,
        row_h: i32,
        active: FontId,
        passive: FontId,
        scroll: Option<WidgetId>,
    ) -> WidgetId {
        let width = lib.frame_size(gp, frame.max(0) as usize)[0];
        let list = ListBox {
            items: Vec::new(),
            cur: 0,
            first: 0,
            rows,
            row_h,
            gp: gp.to_owned(),
            selected_base: frame,
            dx: 24,
            dy: 0,
            active,
            passive,
            hovered: None,
            scroll,
            focus: false,
        };
        let rect = Rect::inclusive(x, y, x + width, y + rows as i32 * row_h);
        self.push(x, y, rect, Kind::ListBox(list))
    }

    /// `addGP_ComboBox(parent, x, y, gp, up, center, 0, active, passive, 0)` (0x47aad6).
    #[allow(clippy::too_many_arguments)]
    pub fn add_combo_box(
        &mut self,
        lib: &Library,
        x: i32,
        y: i32,
        gp: &str,
        up: usize,
        drop_dx: i32,
        active: FontId,
        passive: FontId,
    ) -> WidgetId {
        let [width, h] = lib.frame_size(gp, up);
        let combo = ComboBox {
            lines: Vec::new(),
            cur: 0,
            gp: gp.to_owned(),
            up,
            drop_dx,
            active,
            passive,
            font_dy: 2,
            one_dy: 3,
            open: false,
            light: 0,
            width,
            dlx: 0,
            row_h: lib.frame_size(gp, up + 5)[1].max(1),
            head_h: lib.frame_size(gp, up + 2)[1],
        };
        self.push(x, y, Rect::inclusive(x, y, x + width - 1, y + h), Kind::ComboBox(combo))
    }

    /// `addGP_ComboBoxDLX(parent, x, y, Lx, gp, up, center, 0, active, passive, 0)` (0x47ada9):
    /// a combo box `width` pixels wide built from the halves of a wider frame.
    #[allow(clippy::too_many_arguments)]
    pub fn add_combo_box_dlx(
        &mut self,
        lib: &Library,
        x: i32,
        y: i32,
        width: i32,
        gp: &str,
        up: usize,
        drop_dx: i32,
        active: FontId,
        passive: FontId,
    ) -> WidgetId {
        let id = self.add_combo_box(lib, x, y, gp, up, drop_dx, active, passive);
        let w = &mut self.widgets[id];
        w.rect.x1 = x + width;
        if let Kind::ComboBox(c) = &mut w.kind {
            c.dlx = c.width - width;
            c.width = width;
        }
        id
    }

    /// `addTextViewer(parent, x, y, Lx, Ly, file, font)` (0x47715b) with the file's bytes;
    /// the viewer takes keyboard focus.
    #[allow(clippy::too_many_arguments)]
    pub fn add_text_viewer(
        &mut self,
        lib: &Library,
        fonts: &Fonts,
        x: i32,
        y: i32,
        size: [i32; 2],
        text: &[u8],
        font: FontId,
    ) -> WidgetId {
        let line_h = fonts.height(lib, font).max(1);
        let lines = wrap(text, size[0], |c| fonts.char_width(lib, font, c));
        let viewer = TextViewer {
            text: text.to_vec(),
            font,
            lines,
            line: 0,
            page: (size[1] / line_h) as usize,
            line_h,
            width: size[0],
            scroll: None,
            focus: true,
        };
        self.push(x, y, Rect::new(x, y, size[0], size[1]), Kind::TextViewer(viewer))
    }

    /// `addInputBox(parent, x, y, buffer, capacity, Lx, Ly, passive, active)` (0x475150).
    #[allow(clippy::too_many_arguments)]
    pub fn add_input_box(
        &mut self,
        x: i32,
        y: i32,
        text: &[u8],
        capacity: usize,
        size: [i32; 2],
        passive: FontId,
        active: FontId,
    ) -> WidgetId {
        let input = InputBox { text: text.to_vec(), capacity, caret: text.len(), passive, active, focus: false };
        self.push(x, y, Rect::new(x, y, size[0], size[1]), Kind::InputBox(input))
    }

    /// Makes a widget report `param` when clicked (`UserParam`, `OnUserClick = MMItemChoose`).
    pub fn choose(&mut self, id: WidgetId, param: i32) {
        self.widgets[id].param = Some(param);
    }

    pub fn set_hint(&mut self, id: WidgetId, hint: &[u8]) {
        self.widgets[id].hint = Some(hint.to_vec());
    }

    /// Moves a widget by (`dx`, `dy`).
    pub fn shift(&mut self, id: WidgetId, dx: i32, dy: i32) {
        let w = &mut self.widgets[id];
        w.x += dx;
        w.y += dy;
        w.rect = w.rect.offset(dx, dy);
        if let Kind::TextButton(b) = &mut w.kind {
            b.anchor += dx;
        }
    }

    pub fn list(&self, id: WidgetId) -> &ListBox {
        match &self.widgets[id].kind {
            Kind::ListBox(l) => l,
            _ => panic!("widget {id} is not a list box"),
        }
    }

    pub fn list_mut(&mut self, id: WidgetId) -> &mut ListBox {
        match &mut self.widgets[id].kind {
            Kind::ListBox(l) => l,
            _ => panic!("widget {id} is not a list box"),
        }
    }

    pub fn combo(&self, id: WidgetId) -> &ComboBox {
        match &self.widgets[id].kind {
            Kind::ComboBox(c) => c,
            _ => panic!("widget {id} is not a combo box"),
        }
    }

    pub fn combo_mut(&mut self, id: WidgetId) -> &mut ComboBox {
        match &mut self.widgets[id].kind {
            Kind::ComboBox(c) => c,
            _ => panic!("widget {id} is not a combo box"),
        }
    }

    pub fn slider(&self, id: WidgetId) -> &Slider {
        match &self.widgets[id].kind {
            Kind::Slider(s) => s,
            _ => panic!("widget {id} is not a slider"),
        }
    }

    pub fn viewer_mut(&mut self, id: WidgetId) -> &mut TextViewer {
        match &mut self.widgets[id].kind {
            Kind::TextViewer(t) => t,
            _ => panic!("widget {id} is not a text viewer"),
        }
    }

    pub fn input_box(&self, id: WidgetId) -> &InputBox {
        match &self.widgets[id].kind {
            Kind::InputBox(i) => i,
            _ => panic!("widget {id} is not an input box"),
        }
    }

    pub fn input_box_mut(&mut self, id: WidgetId) -> &mut InputBox {
        match &mut self.widgets[id].kind {
            Kind::InputBox(i) => i,
            _ => panic!("widget {id} is not an input box"),
        }
    }

    fn hit(w: &Widget, p: [f32; 2]) -> bool {
        w.enabled && w.rect.contains(p) && w.clip.contains(p)
    }

    /// The vertical shift of a widget by its scroll owner (0x47d273).
    fn owner_dy(&self, w: &Widget) -> i32 {
        match w.scroll_owner.map(|id| &self.widgets[id]) {
            Some(Widget { visible: true, kind: Kind::ScrollBar(bar), .. }) => -bar.pos,
            _ => 0,
        }
    }

    /// The mouse in the coordinates of widget `i` (undoing its scroll shift).
    fn local(&self, i: usize, p: [f32; 2]) -> [f32; 2] {
        [p[0], p[1] - self.owner_dy(&self.widgets[i]) as f32]
    }

    /// The drop-down area of the open combo box (`DS.ActiveX..ActiveY1`).
    fn modal_zone(&self) -> Option<(WidgetId, Rect)> {
        let id = self.modal?;
        let w = &self.widgets[id];
        let Kind::ComboBox(c) = &w.kind else { return None };
        let (x, y) = drop_origin(w, c);
        let y = y + self.owner_dy(w);
        Some((id, Rect::inclusive(x, y, x + c.width - 1, y + c.row_h * c.lines.len() as i32 + c.font_dy + c.head_h)))
    }

    /// Keeps lists, text viewers and their scroll bars in step (`ListBox` Refresh 0x473f81,
    /// `TextViewer` draw 0x476a18).
    fn sync_scrolls(&mut self) {
        for i in 0..self.widgets.len() {
            let (scroll, count, page, is_viewer) = match &self.widgets[i].kind {
                Kind::ListBox(l) => (l.scroll, l.items.len(), l.rows, false),
                Kind::TextViewer(t) => (t.scroll, t.lines.len(), t.page, true),
                _ => continue,
            };
            let Some(sb) = scroll else { continue };
            let max = (count as i32 - page as i32) * UNITS_PER_ROW;
            let Kind::ScrollBar(bar) = &mut self.widgets[sb].kind else { continue };
            bar.max = max;
            bar.step = UNITS_PER_ROW;
            bar.page = (page as i32 - 1).max(0) * UNITS_PER_ROW;
            bar.pos = bar.pos.clamp(0, max.max(0));
            let first = ((bar.pos + UNITS_PER_ROW / 2) / UNITS_PER_ROW) as usize;
            if is_viewer {
                self.widgets[sb].visible = max > 0;
            }
            if max > 0 {
                match &mut self.widgets[i].kind {
                    Kind::ListBox(l) => l.first = first,
                    Kind::TextViewer(t) => t.line = first,
                    _ => {}
                }
            }
        }
    }

    /// Advances time by `dt` seconds: scroll bar arrows repeat while the button is held.
    pub fn tick(&mut self, dt: f64) {
        self.now += dt * 1000.0;
        let now = self.now;
        for w in &mut self.widgets {
            if let Kind::ScrollBar(bar) = &mut w.kind
                && let Some(dir) = bar.held
                && now - bar.last_repeat > 100.0
            {
                bar.pos = (bar.pos + dir * bar.step).clamp(0, bar.max.max(0));
                bar.last_repeat = now;
            }
        }
        self.sync_scrolls();
    }

    /// Mouse moved to `p` (canvas coordinates): hover states, drags and drag-selection.
    pub fn mouse_move(&mut self, p: [f32; 2]) -> Outcome {
        let mut out = Outcome::default();
        self.mouse = Some(p);
        let zone = self.modal_zone();
        let in_zone = zone.is_some_and(|(_, r)| r.contains(p));
        let held = self.held;
        for i in 0..self.widgets.len() {
            let lp = self.local(i, p);
            let hit = !in_zone && Self::hit(&self.widgets[i], lp) && self.widgets[i].clip.contains(p);
            let w = &mut self.widgets[i];
            if hit && !w.hover && w.hover_sound {
                self.hover_variant = self.hover_variant.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                out.sounds.push(sound::HOVER[(self.hover_variant >> 16) as usize % 3].to_owned());
            }
            w.hover = hit;
            match &mut w.kind {
                Kind::ListBox(l) => {
                    l.hovered = None;
                    if hit {
                        let row = ((p[1] as i32 - w.rect.y0) / l.row_h) as usize;
                        let idx = l.first + row;
                        if row < l.rows && idx < l.items.len() {
                            l.hovered = Some(idx);
                            if held {
                                l.cur = idx;
                            }
                        }
                    }
                }
                Kind::ScrollBar(bar) => {
                    if let Some((y0, pos0)) = bar.drag {
                        let room = (bar.track - bar.thumb[1]).max(1);
                        bar.pos = ((p[1] - y0) as i32 * (bar.max + 1) / room + pos0).clamp(0, bar.max.max(0));
                    }
                }
                Kind::Slider(s) => {
                    if let Some((x0, pos0)) = s.drag {
                        let room = (s.size[0] - s.thumb[0]).max(1);
                        s.pos = ((p[0] - x0) as i32 * (s.max + 1) / room + pos0).clamp(0, s.max);
                    }
                }
                _ => {}
            }
        }
        if let Some((id, rect)) = zone
            && rect.contains(p)
            && let Kind::ComboBox(c) = &mut self.widgets[id].kind
        {
            let y = p[1] as i32 - rect.y0;
            let y0 = c.one_dy + c.font_dy + c.head_h;
            if y >= y0 {
                c.light = (((y - y0) / c.row_h) as usize).min(c.lines.len().saturating_sub(1));
            }
        }
        self.sync_scrolls();
        out
    }

    /// Left button pressed at `p`.
    pub fn press(&mut self, p: [f32; 2]) -> Outcome {
        let mut out = self.mouse_move(p);
        self.held = true;
        // An open drop-down takes the click; a click elsewhere closes it and goes on.
        if let Some((id, zone)) = self.modal_zone() {
            if zone.contains(p) {
                if let Kind::ComboBox(c) = &mut self.widgets[id].kind
                    && p[1] as i32 - zone.y0 >= c.one_dy
                {
                    c.cur = c.light;
                    c.open = false;
                    self.modal = None;
                }
                return out;
            }
            if !self.widgets[id].rect.contains(p) {
                if let Kind::ComboBox(c) = &mut self.widgets[id].kind {
                    c.open = false;
                }
                self.modal = None;
            }
        }
        let now = self.now;
        for i in 0..self.widgets.len() {
            if !(Self::hit(&self.widgets[i], self.local(i, p)) && self.widgets[i].clip.contains(p)) {
                continue;
            }
            if let Some(param) = self.widgets[i].param {
                let sound = if param == OK { self.ok_sound.unwrap_or(sound::CLICK) } else { sound::CLICK };
                out.sounds.push(sound.to_owned());
                out.chosen = Some(param);
                return out;
            }
            let w = &mut self.widgets[i];
            let consumed = match &mut w.kind {
                Kind::ListBox(l) => {
                    if let Some(idx) = l.hovered {
                        l.cur = idx;
                    }
                    false
                }
                Kind::ScrollBar(bar) if bar.max > 0 && w.visible => {
                    let (x, y) = (w.rect.x0 as f32, w.rect.y0 as f32);
                    let (x1, y1) = ((w.rect.x1 - 1) as f32, (w.rect.y1 - 1) as f32);
                    let arrow = bar.arrow_h as f32;
                    let ty = (w.rect.y0 + bar.arrow_h + bar.pos * (bar.track - bar.thumb[1]) / bar.max) as f32;
                    let up = p[0] > x && p[0] < x1 && p[1] > y && p[1] < y + arrow;
                    let down = p[0] > x && p[0] < x1 && p[1] < y1 && p[1] > y1 - arrow;
                    let on_thumb = p[0] > x
                        && p[1] > ty
                        && p[0] < x + (bar.thumb[0] - 1) as f32
                        && p[1] < ty + (bar.thumb[1] - 1) as f32;
                    if on_thumb {
                        bar.drag = Some((p[1], bar.pos));
                    } else if up || down {
                        let dir = if up { -1 } else { 1 };
                        bar.pos = (bar.pos + dir * bar.step).clamp(0, bar.max);
                        bar.held = Some(dir);
                        bar.last_repeat = now;
                    } else {
                        let dir = if p[1] < ty { -1 } else { 1 };
                        bar.pos = (bar.pos + dir * bar.page).clamp(0, bar.max);
                    }
                    true
                }
                Kind::Slider(s) => {
                    let (tx, ty) = slider_thumb(w.x, w.y, s);
                    let (tx, ty) = (tx as f32, ty as f32);
                    let on_thumb = p[0] > tx
                        && p[1] > ty
                        && p[0] < tx + (s.thumb[0] - 1) as f32
                        && p[1] < ty + (s.thumb[1] - 1) as f32;
                    if on_thumb {
                        s.drag = Some((p[0], s.pos));
                    }
                    on_thumb
                }
                Kind::ComboBox(c) => {
                    if !c.lines.is_empty() {
                        c.open = !c.open;
                        c.light = c.cur;
                        self.modal = c.open.then_some(i);
                    }
                    true
                }
                Kind::TextViewer(t) => {
                    t.focus = true;
                    true
                }
                Kind::Universal(u) => {
                    let group = u.group;
                    u.state = match (group, u.state) {
                        (Some(_), 0) => 1,
                        (Some(_), 1) => 2,
                        (Some(_), _) => 1,
                        (None, 0) => 1,
                        (None, _) => 0,
                    };
                    if let Some(g) = group {
                        for (j, other) in self.widgets.iter_mut().enumerate() {
                            if let Kind::Universal(o) = &mut other.kind
                                && j != i
                                && o.group == Some(g)
                            {
                                o.state = 0;
                            }
                        }
                    }
                    true
                }
                Kind::InputBox(b) => {
                    b.focus = true;
                    true
                }
                _ => false,
            };
            if consumed {
                break;
            }
        }
        self.sync_scrolls();
        out
    }

    pub fn release(&mut self) {
        self.held = false;
        for w in &mut self.widgets {
            match &mut w.kind {
                Kind::ScrollBar(bar) => {
                    bar.drag = None;
                    bar.held = None;
                }
                Kind::Slider(s) => s.drag = None,
                _ => {}
            }
        }
    }

    /// Mouse wheel over a list or text with a scroll bar scrolls it by one row per notch.
    /// The original widgets ignore the wheel; this is a convenience.
    pub fn wheel(&mut self, notches: f32) {
        let Some(p) = self.mouse else { return };
        let rows = -notches.round() as i32;
        let target = self.widgets.iter().find_map(|w| match &w.kind {
            Kind::ListBox(l) if w.rect.contains(p) => l.scroll,
            Kind::TextViewer(t) if w.rect.contains(p) => t.scroll,
            _ => None,
        });
        if let Some(sb) = target
            && let Kind::ScrollBar(bar) = &mut self.widgets[sb].kind
        {
            bar.pos = (bar.pos + rows * UNITS_PER_ROW).clamp(0, bar.max.max(0));
        }
        self.sync_scrolls();
    }

    /// A key: focused widgets take it first, otherwise Enter and Escape choose [`OK`] and
    /// [`CANCEL`].
    pub fn key(&mut self, key: Key) -> Outcome {
        let mut out = Outcome::default();
        let mut scroll_to: Option<(WidgetId, i32)> = None;
        let mut handled = false;
        for w in &mut self.widgets {
            match &mut w.kind {
                // A focused, non-empty list swallows every key (0x4737c4).
                Kind::ListBox(l) if l.focus && !l.items.is_empty() => {
                    let delta = match key {
                        Key::Down => 1,
                        Key::Up => -1,
                        Key::PageDown => 3,
                        Key::PageUp => -3,
                        _ => return out,
                    };
                    l.cur = (l.cur as i32 + delta).clamp(0, l.items.len() as i32 - 1) as usize;
                    if l.cur < l.first {
                        l.first = l.cur;
                    }
                    if l.cur >= l.first + l.rows {
                        l.first = l.cur + 1 - l.rows;
                    }
                    scroll_to = l.scroll.map(|sb| (sb, l.first as i32));
                    handled = true;
                }
                Kind::InputBox(b) if b.focus => {
                    handled = true;
                    match key {
                        Key::Left => b.caret = b.caret.saturating_sub(1),
                        Key::Right => b.caret = (b.caret + 1).min(b.text.len()),
                        Key::Home => b.caret = 0,
                        Key::End => b.caret = b.text.len(),
                        Key::Backspace if b.caret > 0 => {
                            b.text.remove(b.caret - 1);
                            b.caret -= 1;
                        }
                        Key::Delete if b.caret < b.text.len() => {
                            b.text.remove(b.caret);
                        }
                        Key::Backspace | Key::Delete => {}
                        Key::Char(c) if c >= 0x20 => {
                            if b.text.len() + 1 < b.capacity {
                                b.text.insert(b.caret, c);
                                b.caret += 1;
                            }
                        }
                        _ => handled = false,
                    }
                }
                Kind::TextViewer(t) if t.focus => {
                    let page = t.page.saturating_sub(1) as i32;
                    let delta = match key {
                        Key::Up => -1,
                        Key::Down => 1,
                        Key::PageUp => -page,
                        Key::PageDown => page,
                        _ => 0,
                    };
                    if delta != 0 {
                        let max = (t.lines.len() as i32 - t.page as i32).max(0);
                        t.line = (t.line as i32 + delta).clamp(0, max) as usize;
                        scroll_to = t.scroll.map(|sb| (sb, t.line as i32));
                        handled = true;
                    }
                }
                _ => {}
            }
            if handled {
                break;
            }
        }
        if let Some((sb, row)) = scroll_to
            && let Kind::ScrollBar(bar) = &mut self.widgets[sb].kind
        {
            bar.pos = row * UNITS_PER_ROW;
        }
        if !handled {
            out.chosen = match key {
                Key::Enter => Some(OK),
                Key::Escape => Some(CANCEL),
                _ => None,
            };
        }
        out
    }

    /// The hint of the hovered widget, if any (the last one wins).
    pub fn hint(&self) -> Option<&[u8]> {
        self.widgets.iter().rev().find(|w| w.hover && w.visible && w.hint.is_some()).and_then(|w| w.hint.as_deref())
    }

    pub fn draw(&self, lib: &Library, fonts: &Fonts, out: &mut Vec<Draw>) {
        for w in self.widgets.iter().filter(|w| w.visible) {
            let start = out.len();
            self.draw_widget(lib, fonts, w, out);
            let dy = self.owner_dy(w);
            if dy != 0 {
                for d in &mut out[start..] {
                    d.y += dy;
                }
            }
            for d in &mut out[start..] {
                d.clip = d.clip.intersect(&w.clip);
            }
        }
        // Drop-downs on top of everything (`OnDrawActive`).
        if let Some(id) = self.modal {
            self.draw_drop_down(&self.widgets[id], out);
        }
    }

    fn draw_widget(&self, lib: &Library, fonts: &Fonts, w: &Widget, out: &mut Vec<Draw>) {
        match &w.kind {
            Kind::Area => {}
            Kind::Outline(rgba) => {
                let r = w.rect;
                for edge in [
                    Rect::new(r.x0, r.y0, r.width(), 1),
                    Rect::new(r.x0, r.y1 - 1, r.width(), 1),
                    Rect::new(r.x0, r.y0, 1, r.height()),
                    Rect::new(r.x1 - 1, r.y0, 1, r.height()),
                ] {
                    out.push(Draw::fill(*rgba, edge));
                }
            }
            Kind::Picture { path } => out.push(Draw::bitmap(path, w.x, w.y)),
            Kind::GpPicture { gp, frame } => out.push(Draw::frame(gp, *frame, w.x, w.y)),
            Kind::GpButton { gp, active, passive } => {
                out.push(Draw::frame(gp, if w.hover { *active } else { *passive }, w.x, w.y))
            }
            Kind::TextButton(b) => {
                let font = if !w.enabled {
                    b.disabled
                } else if w.hover {
                    b.active
                } else {
                    b.passive
                };
                let width = fonts.width(lib, font, &b.text);
                let left = if b.align == Align::Center { b.anchor - width / 2 } else { b.anchor };
                out.push(Draw::text(font, &b.text, left, w.y));
            }
            Kind::GpTextButton(b) => {
                let (x1, y1) = (w.rect.x1 - 1, w.rect.y1 - 1);
                let clip = Rect::inclusive(w.x, w.y, x1 + 1, y1 + 10);
                let ty = (w.y + y1) / 2 - fonts.height(lib, b.active) / 2 + b.font_dy;
                let lit = w.enabled && w.hover;
                if lit {
                    out.push(Draw::frame(&b.gp, b.sprite, w.x, w.y).clipped(clip));
                }
                let font = if lit { b.active } else { b.passive };
                let d = fonts.width(lib, font, &b.text) + w.x - x1;
                out.push(Draw::text(font, &b.text, w.x - (d >> 1), ty).clipped(clip));
            }
            Kind::ListBox(l) => {
                let ty = ((l.row_h - fonts.height(lib, l.active)) >> 1) + l.dy;
                for r in 0..l.rows {
                    let idx = l.first + r;
                    let Some(text) = l.items.get(idx) else { continue };
                    let ry = w.y + r as i32 * l.row_h;
                    let font = if idx == l.cur {
                        let frame = l.selected_base + (r % 3) as i32;
                        if frame >= 0 {
                            out.push(Draw::frame(&l.gp, frame as usize, w.x, ry));
                        }
                        l.active
                    } else {
                        l.passive
                    };
                    out.push(Draw::text(font, text, w.x + l.dx, ry + ty));
                }
            }
            Kind::ScrollBar(bar) => {
                if bar.max > 0 && w.enabled {
                    self.draw_scroll_bar(lib, w, bar, out);
                }
            }
            Kind::Slider(s) => {
                let (tx, ty) = slider_thumb(w.x, w.y, s);
                out.push(Draw::frame(&s.gp, s.frame, tx, ty));
            }
            Kind::ComboBox(c) => {
                let Some(line) = c.lines.get(c.cur) else { return };
                if w.enabled {
                    let frame = c.up + 1 - usize::from(w.hover || c.open);
                    combo_frame(c, frame, w.x, w.y, out);
                }
                let font = if c.open && w.enabled { c.active } else { c.passive };
                out.push(Draw::text(font, line, w.x + 24, w.y + c.one_dy));
            }
            Kind::TextViewer(t) => {
                for i in 0..t.page {
                    let Some(line) = t.lines.get(t.line + i) else { continue };
                    let yy = w.y + t.line_h * i as i32;
                    let text = &t.text[line.start..line.start + line.len];
                    let extra =
                        if line.spaces > 0 && line.justify { ((t.width - line.width) / line.spaces).min(5) } else { 0 };
                    // Word by word so that justified lines can widen their spaces.
                    let mut xx = 0;
                    for word in text.split_inclusive(|&c| c == b' ') {
                        let visible: Vec<u8> = word.iter().copied().filter(|&c| c != b'~' && c != b'\n').collect();
                        out.push(Draw::text(t.font, &visible, w.x + xx, yy));
                        xx += fonts.width(lib, t.font, &visible);
                        if word.last() == Some(&b' ') {
                            xx += extra;
                        }
                    }
                }
            }
            Kind::Universal(u) => {
                // Draw 0x46f9f1: middle pieces between the caps, then the caps, then the text.
                let (gp, width) = (&u.gp, u.width);
                let frames = &u.frames[usize::from(w.hover) * 3 + u.state.min(2)];
                let size = |f: usize| lib.frame_size(gp, f)[0];
                let (mut left, right) = (w.x + size(frames[0]), w.x + width - size(frames[4]));
                let clip = Rect::inclusive(left, 0, right, 767);
                let mut seed = (left * 7 + w.y * 17) as usize;
                let mut n = 0;
                while left < right && n < 1000 {
                    let f = frames[1 + (self.random(seed & 0x1fff).rem_euclid(3)) as usize];
                    out.push(Draw::frame(gp, f, left, w.y).clipped(clip));
                    left += size(f).max(1);
                    seed += 1;
                    n += 1;
                }
                let mid = (w.x + w.x + width - 1) / 2;
                out.push(Draw::frame(gp, frames[0], w.x, w.y).clipped(Rect::inclusive(0, 0, mid, 767)));
                out.push(Draw::frame(gp, frames[4], w.x + width - 1 - size(frames[4]), w.y).clipped(Rect::inclusive(
                    mid + 1,
                    0,
                    1023,
                    767,
                )));
                let pick = |selected: bool| if selected { u.selected } else { u.passive };
                let font = if u.active == u.passive {
                    pick(u.state > 0)
                } else if w.hover {
                    u.active
                } else {
                    pick(u.state > 0)
                };
                if let Some(font) = font {
                    let (x1, y1) = (w.rect.x1 - 1, w.rect.y1 - 1);
                    let ty = (w.y + y1 - fonts.height(lib, font)) / 2 - 1;
                    let tx = (w.x + x1 - fonts.width(lib, font, &u.text)) / 2;
                    out.push(Draw::text(font, &u.text, tx, ty));
                }
            }
            Kind::InputBox(b) => {
                let width = w.rect.width() - 1;
                let caret_x = fonts.width(lib, b.passive, &b.text[..b.caret.min(b.text.len())]);
                let scroll = (caret_x - width + 32).max(0);
                if b.focus {
                    out.push(Draw::text(b.active, &b.text, w.x + 8 - scroll, w.y + 4).clipped(w.rect));
                    if (self.now as i64 / 250) & 1 == 1 {
                        out.push(Draw::text(b.passive, b"|", w.x + caret_x + 8 - scroll, w.y + 4).clipped(w.rect));
                    }
                } else {
                    out.push(Draw::text(b.passive, &b.text, w.x + 8, w.y + 4).clipped(w.rect));
                }
            }
        }
    }

    /// `DrawVBar` 0x47feaf: track tiles between the arrows, the arrows, then the thumb.
    fn draw_scroll_bar(&self, lib: &Library, w: &Widget, bar: &ScrollBar, out: &mut Vec<Draw>) {
        let p = self.mouse.unwrap_or([-1.0, -1.0]);
        let (x, y) = (w.rect.x0 as f32, w.rect.y0 as f32);
        let (x1, y1) = ((w.rect.x1 - 1) as f32, (w.rect.y1 - 1) as f32);
        let arrow = bar.arrow_h as f32;
        let up = p[0] > x && p[0] < x1 && p[1] > y && p[1] < y + arrow;
        let down = p[0] > x && p[0] < x1 && p[1] < y1 && p[1] > y1 - arrow;
        let height = w.rect.height();
        let h2 = bar.arrow_h / 2;
        let mid = bar.base + 5;
        let mh = lib.frame_size(&bar.gp, mid)[1];
        let clip = Rect::inclusive(w.x - 64, w.y + h2, w.x + 64, w.y + height - 1 - h2);
        if mh > 0 {
            for i in 0..=height / mh {
                out.push(Draw::frame(&bar.gp, mid + (i % 3) as usize, w.x, w.y + i * mh + h2).clipped(clip));
            }
        }
        out.push(Draw::frame(&bar.gp, bar.base + usize::from(up), w.x, w.y));
        let bottom = bar.base + 2 + usize::from(down);
        out.push(Draw::frame(&bar.gp, bottom, w.x, w.y + height - lib.frame_size(&bar.gp, bottom)[1]));
        let ty = w.y + bar.arrow_h + bar.pos * (bar.track - bar.thumb[1]) / bar.max;
        out.push(Draw::frame(&bar.gp, bar.base + 4, w.x, ty));
    }

    /// `ComboBox::OnDrawActive` 0x4798ca: header, alternating row backgrounds, footer, the
    /// hovered row highlight and the texts.
    fn draw_drop_down(&self, w: &Widget, out: &mut Vec<Draw>) {
        let Kind::ComboBox(c) = &w.kind else { return };
        let variant = |i: usize| (self.random(i * 17) & 1) as usize;
        let (dx, dy) = drop_origin(w, c);
        let dy = dy + self.owner_dy(w);
        combo_frame(c, c.up + 2, dx, dy, out);
        let mut yy = dy + c.head_h;
        for i in 0..c.lines.len() {
            combo_frame(c, c.up + 5 + variant(i), dx, yy, out);
            yy += c.row_h;
        }
        combo_frame(c, c.up + 7, dx, yy, out);
        for (i, line) in c.lines.iter().enumerate() {
            let ry = dy + c.head_h + i as i32 * c.row_h;
            let font = if i == c.light {
                combo_frame(c, c.up + 3 + variant(i), dx, ry, out);
                c.active
            } else {
                c.passive
            };
            out.push(Draw::text(font, line, w.x + 25, ry + c.font_dy));
        }
    }
}

/// `RANDOM.LST`: the fixed pseudo-random table of the original.
pub const RANDOM_LST: &str = "RANDOM.LST";

/// A combo box frame at (`x`, `y`); with `DLX` its left half up to the middle column and
/// its right half shifted left by `DLX` (0x4787bd).
fn combo_frame(c: &ComboBox, frame: usize, x: i32, y: i32, out: &mut Vec<Draw>) {
    if c.dlx == 0 {
        out.push(Draw::frame(&c.gp, frame, x, y));
        return;
    }
    let mid = x + (c.width - 1) / 2;
    out.push(Draw::frame(&c.gp, frame, x, y).clipped(Rect::inclusive(0, 0, mid, 767)));
    out.push(Draw::frame(&c.gp, frame, x - c.dlx, y).clipped(Rect::inclusive(mid, 0, 1023, 767)));
}

/// Top-left of a combo box's drop-down: `Center` pixels right of the box, on its last row.
fn drop_origin(w: &Widget, c: &ComboBox) -> (i32, i32) {
    (w.x + c.drop_dx, w.rect.y1 - 1)
}

fn slider_thumb(x: i32, y: i32, s: &Slider) -> (i32, i32) {
    let max = s.max.max(1);
    if s.size[0] > s.size[1] {
        (x + s.pos * (s.size[0] - s.thumb[0]) / max + s.offset[0], y + s.offset[1])
    } else {
        (x + s.offset[0], y + s.offset[1] + s.pos * (s.size[1] - s.thumb[1]) / max)
    }
}

/// Splits text into lines narrower than `width` pixels (`TextViewer::GetNextLine` 0x476386).
///
/// Words are separated by spaces; a line ends at CR (the following LF is skipped) or
/// before a word that does not fit, which makes it justified. A word wider than a whole
/// line is broken. Empty lines are dropped, as in the original. `~` has no width.
fn wrap(text: &[u8], width: i32, char_width: impl Fn(u8) -> i32) -> Vec<Line> {
    let cw = |c: u8| if c == b'~' || c == b'\n' { 0 } else { char_width(c) };
    let n = text.len();
    let mut lines = Vec::new();
    let mut start = 0;
    while start < n {
        let (mut offs, mut lw, mut ns) = (start, 0, 0);
        let (line, next) = loop {
            if offs >= n {
                break (Line { start, len: n - start, width: lw, spaces: ns, justify: false }, n);
            }
            let mut i = offs;
            let mut wid = 0;
            while i < n && text[i] == b' ' {
                wid += cw(b' ');
                i += 1;
            }
            let spaces = (i - offs) as i32;
            while i < n && text[i] != b' ' && text[i] != b'\r' {
                wid += cw(text[i]);
                i += 1;
            }
            if lw + wid < width {
                if text.get(i) == Some(&b'\r') {
                    break (
                        Line { start, len: i - start, width: lw + wid, spaces: ns, justify: false },
                        (i + 2).min(n),
                    );
                }
                lw += wid;
                ns += spaces;
                offs = i;
                continue;
            }
            if offs == start {
                // A word wider than the line: break it.
                let (mut j, mut w) = (offs, 0);
                while j < i && w + cw(text[j]) < width {
                    w += cw(text[j]);
                    j += 1;
                }
                let j = j.max(offs + 1);
                break (Line { start, len: j - start, width: w, spaces: 0, justify: true }, j);
            }
            break (Line { start, len: offs - start, width: lw, spaces: ns, justify: true }, offs + spaces as usize);
        };
        let content = &text[line.start..line.start + line.len];
        if !content.iter().all(|&c| c == b'\n') {
            lines.push(line);
        }
        start = next.max(start + 1);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: &str, width: i32) -> Vec<String> {
        wrap(text.as_bytes(), width, |_| 10)
            .iter()
            .map(|l| String::from_utf8_lossy(&text.as_bytes()[l.start..l.start + l.len]).into_owned())
            .collect()
    }

    #[test]
    fn wraps_words_and_paragraphs() {
        assert_eq!(lines("aaa bbb ccc", 75), ["aaa bbb", "ccc"]);
        assert_eq!(lines("aaa\r\nbbb", 200), ["aaa", "bbb"]);
        assert_eq!(lines("aaa\r\n\r\nbbb", 200), ["aaa", "bbb"]);
        assert_eq!(lines("abcdefghij", 45), ["abcd", "efgh", "ij"]);
    }

    #[test]
    fn justification_flags() {
        let l = wrap(b"aa bb cc\r\ndd", 75, |_| 10);
        assert!(l[0].justify);
        assert_eq!(l[0].spaces, 1);
        assert!(!l[1].justify);
    }

    #[test]
    fn clicks_report_params_with_sound() {
        let mut d = Dialog::default();
        let a = d.add_area(Rect::new(10, 10, 10, 10));
        d.choose(a, 7);
        let out = d.press([12.0, 12.0]);
        assert_eq!(out.chosen, Some(7));
        assert_eq!(out.sounds, [sound::CLICK]);
        assert_eq!(d.press([30.0, 30.0]).chosen, None);
        assert_eq!(d.key(Key::Escape).chosen, Some(CANCEL));
    }

    #[test]
    fn clip_limits_hits() {
        let mut d = Dialog::default();
        d.set_clip(Some(Rect::new(0, 0, 15, 15)));
        let a = d.add_area(Rect::new(10, 10, 10, 10));
        d.choose(a, 1);
        assert_eq!(d.press([12.0, 12.0]).chosen, Some(1));
        assert_eq!(d.press([17.0, 17.0]).chosen, None);
    }
}
