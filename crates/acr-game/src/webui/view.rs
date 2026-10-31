//! The view model of the top screen: each widget's draws, resolved to parts the UI can place
//! (docs/webui.md "View model").
//!
//! `Dialog::widget_draws` gives the engine's own draws; sprites become urls of the asset
//! endpoints with their size and offset, strings become [`Part::Text`] for the UI to lay out
//! from the font atlas (strings with bytes the atlas lacks fall back to a whole-string image).

use std::collections::HashMap;

use acr_formats::CodePage;
use acr_ui_protocol::{CanvasView, CursorView, Part, WidgetKind, WidgetView};

use super::assets::{self, AssetError, FramePalette};
use crate::canvas::Canvas;
use crate::ui::Ui;
use crate::ui::dialog::{Dialog, Kind};
use crate::ui::draw::{Draw, Rect, Sprite};
use crate::ui::fonts::Fonts;
use crate::ui::library::Library;

/// Why a screen cannot be described now.
#[derive(Debug)]
pub enum Failure {
    /// A file is still loading.
    Pending,
    /// The screen draws something the protocol has no part for.
    Unsupported(String),
    /// An asset cannot be made.
    Asset(AssetError),
}

impl From<AssetError> for Failure {
    fn from(e: AssetError) -> Self {
        Failure::Asset(e)
    }
}

/// A sprite as the UI places it.
#[derive(Debug, Clone)]
struct Placed {
    url: String,
    offset: [i32; 2],
    size: [i32; 2],
}

/// Remembers the url, size and offset of each sprite, which cost a decode to find out.
#[derive(Default)]
pub struct Resolver {
    placed: HashMap<Sprite, Option<Placed>>,
}

/// Cached sprites before the cache starts over.
const KEEP: usize = 4000;

fn rect4(r: Rect) -> [i32; 4] {
    [r.x0, r.y0, r.width(), r.height()]
}

fn clip4(clip: Rect) -> Option<[i32; 4]> {
    (clip != Rect::ALL).then(|| rect4(clip))
}

/// Whether the atlas can lay `text` out: bytes `0x20..=0xFF` without the escape (NUL ends a
/// string in the engine).
fn atlas_text(text: &[u8]) -> bool {
    text.iter().all(|&b| b >= 0x20 && b != 0x7f)
}

impl Resolver {
    fn place(&mut self, lib: &Library, fonts: &Fonts, sprite: &Sprite) -> Result<Option<Placed>, Failure> {
        if let Some(placed) = self.placed.get(sprite) {
            return Ok(placed.clone());
        }
        let (image, url) = match sprite {
            Sprite::Bitmap(path) => (assets::bitmap_image(lib, path)?.map(Some), assets::bmp_url(path)),
            Sprite::Frame { gp, frame } => (
                assets::frame_image(lib, FramePalette::Menu, 0, gp, *frame)?.map(Some),
                assets::frame_url(FramePalette::Menu, 0, gp, *frame),
            ),
            Sprite::GameFrame { gp, frame, player } => (
                assets::frame_image(lib, FramePalette::Sprite, *player, gp, *frame)?.map(Some),
                assets::frame_url(FramePalette::Sprite, *player, gp, *frame),
            ),
            Sprite::Text { font, text } => {
                (assets::text_image(lib, fonts, 1, *font, 0, text)?, assets::text_url(1, *font, 0, text))
            }
            Sprite::GameText { font, text } => {
                (assets::text_image(lib, fonts, 0, *font, 0, text)?, assets::text_url(0, *font, 0, text))
            }
            Sprite::GameTextFaded { font, text, fade } => {
                (assets::text_image(lib, fonts, 0, *font, *fade, text)?, assets::text_url(0, *font, *fade, text))
            }
            other => return Err(Failure::Unsupported(format!("{other:?}"))),
        };
        let Some(image) = image else { return Err(Failure::Pending) };
        let placed =
            image.map(|(image, offset)| Placed { url, offset, size: [image.width as i32, image.height as i32] });
        if self.placed.len() >= KEEP {
            self.placed.clear();
        }
        self.placed.insert(sprite.clone(), placed.clone());
        Ok(placed)
    }

    /// The parts for `draws`, in order.
    pub fn parts(&mut self, lib: &Library, fonts: &Fonts, draws: &[Draw]) -> Result<Vec<Part>, Failure> {
        let mut parts = Vec::with_capacity(draws.len());
        for d in draws {
            let clip = clip4(d.clip);
            let (font, set, text, fade) = match &d.sprite {
                Sprite::Fill(rgba) => {
                    let mut rgba = *rgba;
                    rgba[3] = (f32::from(rgba[3]) * d.alpha).round() as u8;
                    parts.push(Part::Fill { rgba, rect: [d.x, d.y, d.size[0], d.size[1]], clip });
                    continue;
                }
                Sprite::Text { font, text } => (font, 1, text, 0),
                Sprite::GameText { font, text } => (font, 0, text, 0),
                Sprite::GameTextFaded { font, text, fade } => (font, 0, text, *fade),
                _ => {
                    if let Some(p) = self.place(lib, fonts, &d.sprite)? {
                        let [x, y] = [d.x + p.offset[0], d.y + p.offset[1]];
                        parts.push(Part::Sprite { url: p.url, x, y, w: p.size[0], h: p.size[1], alpha: d.alpha, clip });
                    }
                    continue;
                }
            };
            if text.is_empty() {
                continue;
            }
            if atlas_text(text) && fade == 0 {
                parts.push(Part::Text {
                    font: font.0.to_owned(),
                    set,
                    text: CodePage::Cp1252.decode(text),
                    x: d.x,
                    y: d.y,
                    fade,
                    clip,
                });
            } else if let Some(p) = self.place(lib, fonts, &d.sprite)? {
                let [x, y] = [d.x + p.offset[0], d.y + p.offset[1]];
                parts.push(Part::Sprite { url: p.url, x, y, w: p.size[0], h: p.size[1], alpha: d.alpha, clip });
            }
        }
        Ok(parts)
    }

    /// The cursor as an image with its hotspot; `None` while the mouse is outside.
    pub fn cursor(&mut self, ui: &Ui) -> Result<Option<CursorView>, Failure> {
        let Some(draw) = ui.cursor_draw() else { return Ok(None) };
        let (lib, fonts) = ui.assets();
        Ok(self
            .place(lib, fonts, &draw.sprite)?
            .map(|p| CursorView { url: p.url, hotspot: [-(draw.x + p.offset[0]), -(draw.y + p.offset[1])] }))
    }
}

/// The top screen as the UI draws it.
pub struct ScreenView {
    pub name: String,
    pub stack: Vec<String>,
    pub in_game: bool,
    pub busy: bool,
    pub hint: Option<String>,
    pub widgets: Vec<WidgetView>,
}

fn kind(kind: &Kind) -> WidgetKind {
    match kind {
        Kind::Area => WidgetKind::Area,
        Kind::Outline(_) => WidgetKind::Outline,
        Kind::Picture { .. } | Kind::GpPicture { .. } => WidgetKind::Picture,
        Kind::TextButton(_) | Kind::GpButton { .. } | Kind::GpTextButton(_) => WidgetKind::Button,
        Kind::Universal(_) => WidgetKind::Strip,
        Kind::ListBox(_) => WidgetKind::List,
        Kind::ScrollBar(_) => WidgetKind::Scrollbar,
        Kind::Slider(_) => WidgetKind::Slider,
        Kind::ComboBox(_) => WidgetKind::Combo,
        Kind::TextViewer(_) => WidgetKind::Viewer,
        Kind::InputBox(_) => WidgetKind::Input,
    }
}

/// The widgets of `dialog`, then the open drop-down as a widget of its own on top.
pub fn widgets(
    resolver: &mut Resolver,
    lib: &Library,
    fonts: &Fonts,
    screen: &str,
    dialog: &Dialog,
) -> Result<Vec<WidgetView>, Failure> {
    let mut out = Vec::with_capacity(dialog.widgets.len() + 1);
    for (i, w) in dialog.widgets.iter().enumerate() {
        let parts = resolver.parts(lib, fonts, &dialog.widget_draws(lib, fonts, i))?;
        out.push(WidgetView {
            id: format!("{screen}/{i}"),
            kind: kind(&w.kind),
            rect: rect4(w.rect),
            enabled: w.enabled,
            hover: w.hover,
            parts,
        });
    }
    let dropdown = dialog.dropdown_draws();
    if !dropdown.is_empty() {
        out.push(WidgetView {
            id: format!("{screen}/dropdown"),
            kind: WidgetKind::Combo,
            rect: [0, 0, 0, 0],
            enabled: true,
            hover: false,
            parts: resolver.parts(lib, fonts, &dropdown)?,
        });
    }
    Ok(out)
}

/// The screen [`Ui::draws`] would show, as a view.
pub fn screen(resolver: &mut Resolver, ui: &Ui) -> Result<Option<ScreenView>, Failure> {
    let Some(top) = ui.shown_screen() else { return Ok(None) };
    let (lib, fonts) = ui.assets();
    let name = top.name();
    let dialog = top.dialog().ok_or_else(|| Failure::Unsupported(format!("{name} has no dialog")))?;
    Ok(Some(ScreenView {
        name: name.to_owned(),
        stack: ui.screens().map(|(s, _)| s.name().to_owned()).collect(),
        in_game: top.in_game(),
        busy: top.busy(),
        hint: dialog.hint().map(|h| CodePage::Cp1252.decode(h)),
        widgets: widgets(resolver, lib, fonts, name, dialog)?,
    }))
}

pub fn canvas_view(canvas: Canvas) -> CanvasView {
    CanvasView { scale: canvas.scale, origin: canvas.origin, size: canvas.size() }
}
