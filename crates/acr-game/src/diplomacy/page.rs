//! One CML page laid out like intExplorer (see `docs/ui.md`, "Diplomacy panel"): every box
//! (`#ebox`, `#sbox`) is a [`Dialog`] of its own; texts become rows of text buttons, pictures
//! `GP_Button`s.

use acr_formats::font::{font_like, named_font};

use super::cml::{self, Tag};
use crate::ui::Ctx;
use crate::ui::dialog::{Align, Dialog, Kind, WidgetId};
use crate::ui::draw::{Draw, Rect};
use crate::ui::fonts::FontId;

/// Inclusive rectangle of the original (`x0, y0, x1, y1`).
type Area = [i32; 4];

/// Arrow step of a box's scroll bar in pixels (0x100105bb sets `+0xa6` to 8).
const SCROLL_STEP: i32 = 8;

/// An element of a box: its name and rectangle after creation (for `%NAME` references).
struct Element {
    name: Vec<u8>,
    area: Area,
}

struct PageBox {
    name: Vec<u8>,
    inner: Area,
    outer: Area,
    dialog: Dialog,
    /// `#sbox`: the scroll bar and the first content widget.
    scroll: Option<WidgetId>,
    content: usize,
    elements: Vec<Element>,
}

/// Settings of `#def_sbox` (0x1000c0bb) and `#def_scroll` (0x1000c4d1).
struct SboxStyle {
    /// Insets left, top, right, bottom (n9..n12).
    insets: [i32; 4],
    /// Scroll bar offset from the inner right edge and top, extra length (n13..n15).
    scroll: [i32; 3],
    gp: Option<String>,
    base: usize,
}

impl Default for SboxStyle {
    /// Defaults of 0x10012680; the default scroll pack `Interface\_Slider` is not in the
    /// game data, so a page without `#def_scroll` gets no bar.
    fn default() -> Self {
        Self { insets: [1, 1, 16, 2], scroll: [1, -1, 3], gp: None, base: 0 }
    }
}

pub struct Page {
    text: Vec<u8>,
    /// The browser window the boxes are placed in ([`super::window`]).
    window: Rect,
    built: bool,
    boxes: Vec<PageBox>,
    /// Commands of the links, by `UserParam`.
    commands: Vec<Vec<u8>>,
    /// `#time(ms, "com:...")`: seconds left and the command (one-shot, 0x10009070).
    timer: Option<(f64, Vec<u8>)>,
}

impl Page {
    pub fn new(text: &[u8], window: Rect) -> Self {
        Self { text: text.to_vec(), window, built: false, boxes: Vec::new(), commands: Vec::new(), timer: None }
    }

    /// The page's CML text.
    pub fn text(&self) -> &[u8] {
        &self.text
    }

    /// Builds the widgets once the fonts and pictures are loaded.
    fn build(&mut self, ctx: &Ctx) -> bool {
        if self.built {
            return true;
        }
        let tags = cml::parse(&self.text);
        let pictures: Vec<String> = tags
            .iter()
            .filter(|t| t.name == "pix" || t.name == "cpix")
            .filter_map(|t| t.args.get(2).map(|a| gp_path(&cml::unquote(a))))
            .chain(tags.iter().filter(|t| t.name == "def_scroll").filter_map(|t| t.args.first().map(|a| gp_path(a))))
            .collect();
        if !ctx.fonts.ready(ctx.lib) || !pictures.iter().all(|p| ctx.lib.settled(p)) {
            return false;
        }
        let mut fonts = [FontId("YellowFont"); 3];
        let mut nfonts: Vec<(Vec<u8>, FontId)> = Vec::new();
        let mut style = SboxStyle::default();
        for tag in &tags {
            match tag.name.as_str() {
                "font" => {
                    for (i, a) in tag.args.iter().take(3).enumerate() {
                        fonts[i] = font_name(a, &nfonts);
                    }
                }
                "nfont" => {
                    if let Some(f) = nfont(tag) {
                        nfonts.push((tag.args[0].clone(), f));
                    }
                }
                "def_sbox" => {
                    let n: Vec<i32> = tag.args.iter().skip(1).map(|a| atoi(a)).collect();
                    let at = |i: usize| n.get(i).copied().unwrap_or(0);
                    style.insets = [at(8), at(9), at(10), at(11)];
                    style.scroll = [at(12), at(13), at(14)];
                }
                "def_scroll" => {
                    style.gp = tag.args.first().map(|a| gp_path(a)).filter(|p| ctx.lib.exists(p));
                    style.base = tag.args.get(1).map_or(0, |a| atoi(a).max(0) as usize);
                }
                "ebox" | "box" | "sbox" => self.add_box(ctx, tag, &style),
                "txt" | "ctxt" | "rtxt" => self.add_text(ctx, tag, fonts),
                "pix" | "cpix" => self.add_picture(ctx, tag),
                "pan" => {
                    // Draws only a style defined with `#def_panel`, which no page has.
                    self.add_element(tag, |_, _, a| *a);
                }
                "time" => {
                    let ms = tag.args.first().map_or(0, |a| atoi(a));
                    let command = tag.args.get(1).map(|a| cml::unquote(a)).unwrap_or_default();
                    self.timer = Some((f64::from(ms.max(0)) / 1000.0, command));
                }
                _ => {}
            }
        }
        self.built = true;
        true
    }

    /// `#ebox`/`#sbox[%NAME](x:..,y:..,w:..,h:..)`: a box relative to the window (0x1000ab75).
    fn add_box(&mut self, ctx: &Ctx, tag: &Tag, style: &SboxStyle) {
        let Some(name) = tag.id.clone() else { return };
        let w = self.window;
        let window = [w.x0, w.y0, w.x1 - 1, w.y1 - 1];
        let outer = rect(&tag.args.join(&b','), window, &[]);
        let mut dialog = Dialog::default();
        let mut scroll = None;
        let mut inner = outer;
        if tag.name == "sbox" {
            let [l, t, r, b] = style.insets;
            inner = [outer[0] + l, outer[1] + t, outer[2] - r, outer[3] - b];
            // The border (`addCustomBorder` with the `#def_sbox` frames) draws nothing with
            // the frames -1 of the diplomacy pages.
            if let Some(gp) = &style.gp {
                let [dx, dy, extra] = style.scroll;
                let len = inner[3] - inner[1] + extra + 1;
                let id = dialog.add_scroll_bar(ctx.lib, inner[2] + dx, inner[1] + dy, len, 10, 0, gp, style.base);
                if let Kind::ScrollBar(bar) = &mut dialog.widgets[id].kind {
                    bar.step = SCROLL_STEP;
                    bar.page = outer[3] - outer[1] - 24;
                    bar.max = 0;
                }
                dialog.widgets[id].visible = false;
                scroll = Some(id);
            }
            dialog.set_clip(Some(Rect::inclusive(inner[0], inner[1], inner[2], inner[3])));
        }
        let content = dialog.widgets.len();
        self.boxes.push(PageBox { name, inner, outer, dialog, scroll, content, elements: Vec::new() });
    }

    /// Creates an element in its parent box: `create(dialog, commands, area)` adds the widgets
    /// and returns the element's rectangle after creation.
    fn add_element(&mut self, tag: &Tag, create: impl FnOnce(&mut Dialog, &mut Vec<Vec<u8>>, &Area) -> Area) {
        let Some(spec) = tag.args.first() else { return };
        let Some(open) = spec.iter().position(|&b| b == b'[') else { return };
        let parent = &spec[..open];
        let Some(b) =
            self.boxes.iter_mut().find(|b| b.name.eq_ignore_ascii_case(parent.strip_prefix(b"%").unwrap_or(parent)))
        else {
            return;
        };
        let inside = spec[open + 1..].strip_suffix(b"]").unwrap_or(&spec[open + 1..]);
        let area = rect(inside, b.inner, &b.elements);
        let first = b.dialog.widgets.len();
        let area = create(&mut b.dialog, &mut self.commands, &area);
        if let Some(sb) = b.scroll {
            for w in &mut b.dialog.widgets[first..] {
                w.scroll_owner = Some(sb);
            }
        }
        let name = tag.id.clone().map(|n| n.strip_prefix(b"%").map(<[u8]>::to_vec).unwrap_or(n)).unwrap_or_default();
        b.elements.push(Element { name, area });
    }

    /// `#txt`, `#ctxt`, `#rtxt` (0x100136e0).
    fn add_text(&mut self, ctx: &Ctx, tag: &Tag, fonts: [FontId; 3]) {
        // `{cmd}{cmd}...` up to the comma, then the text (0x1000d9d2..0x1000e47b).
        let commands = tag.args.get(1).map(|a| groups(a)).unwrap_or_default();
        let text = tag.args.get(2).map(|a| cml::unquote(a)).unwrap_or_default();
        let align = match tag.name.as_str() {
            "ctxt" => 1,
            "rtxt" => 2,
            _ => 0,
        };
        self.add_element(tag, |dialog, all, area| {
            let lines = layout_text(ctx, dialog, all, area, &text, &commands, fonts, align);
            let lh = ctx.fonts.glyph_height(ctx.lib, fonts[0], b'y') + 1;
            [area[0], area[1], area[2], area[1] + lines * lh]
        });
    }

    /// `#pix` / `#cpix(rect, {cmd}, "gp", f1, f2, f3, f4)` (0x10019390): a `GP_Button` with
    /// the passive frame f1 and f3 under the mouse; `#cpix` is centred by the width of f3.
    fn add_picture(&mut self, ctx: &Ctx, tag: &Tag) {
        let Some(gp) = tag.args.get(2).map(|a| gp_path(&cml::unquote(a))) else { return };
        let frame = |i: usize| tag.args.get(i).map_or(0, |a| atoi(a).max(0) as usize);
        let (f1, f3) = (frame(3), frame(5));
        let centre = tag.name == "cpix";
        self.add_element(tag, |dialog, _, area| {
            if ctx.lib.sprites(&gp).is_none() {
                return *area;
            }
            let [w, h] = ctx.lib.frame_size(&gp, f3);
            let x = if centre { area[0] + (area[2] - area[0] - w) / 2 } else { area[0] };
            dialog.add_gp_button(ctx.lib, x, area[1], &gp, f3, f1);
            [x, area[1], area[2], area[1] + h]
        });
    }

    /// Left button at canvas point `p`: the request of a clicked link (`DS|perform&..`
    /// without the server name) and the click sound.
    pub fn press(&mut self, ctx: &Ctx, p: [f32; 2]) -> (Option<Vec<u8>>, Vec<String>) {
        if !self.build(ctx) {
            return (None, Vec::new());
        }
        let mut sounds = Vec::new();
        let mut chosen = None;
        for b in &mut self.boxes {
            let out = b.dialog.press(p);
            sounds.extend(out.sounds);
            chosen = chosen.or(out.chosen);
        }
        let command = chosen.and_then(|i| self.commands.get(i as usize)).cloned();
        (command.and_then(|c| request(&c)), sounds)
    }

    pub fn release(&mut self) {
        for b in &mut self.boxes {
            b.dialog.release();
        }
    }

    pub fn mouse_moved(&mut self, p: [f32; 2]) {
        for b in &mut self.boxes {
            b.dialog.mouse_move(p);
        }
    }

    /// Time passes: scroll bars and a due `#time` request.
    pub fn update(&mut self, seconds: f64) -> Option<Vec<u8>> {
        for b in &mut self.boxes {
            b.dialog.tick(seconds);
        }
        let (left, command) = self.timer.as_mut()?;
        *left -= seconds;
        if *left > 0.0 {
            return None;
        }
        let command = command.clone();
        self.timer = None;
        // `com:X` sends X like a link; other timer kinds do not occur in the pages.
        command.strip_prefix(b"com:").and_then(request)
    }

    pub fn draw(&mut self, ctx: &Ctx, out: &mut Vec<Draw>) {
        if !self.build(ctx) {
            return;
        }
        for b in &mut self.boxes {
            update_scroll(b);
            let mut draws = Vec::new();
            b.dialog.draw(ctx.lib, ctx.fonts, &mut draws);
            out.extend(draws.into_iter().map(Draw::in_game));
        }
    }
}

/// `LW_file` with a file that starts with a marker such as `<FRM1>` (0x100040b0) replaces
/// only that marked section of the current page (DipServer refreshes the list of allied
/// tribes every 4 s this way); any other file replaces the page.
pub fn merge_section(current: &[u8], file: &[u8]) -> Vec<u8> {
    let start = file.iter().position(|b| !b.is_ascii_whitespace()).unwrap_or(0);
    let body = &file[start..];
    let marker_end = match body.first() {
        Some(b'<') => body.iter().position(|&b| b == b'>'),
        _ => None,
    };
    let Some(end) = marker_end else { return file.to_vec() };
    let marker = &body[..=end];
    let find = |hay: &[u8], from: usize| {
        hay.get(from..).and_then(|h| h.windows(marker.len()).position(|w| w == marker)).map(|i| i + from)
    };
    // The section: from the marker to the end of its second occurrence.
    let section = |hay: &[u8]| {
        let a = find(hay, 0)?;
        let b = find(hay, a + marker.len()).map_or(hay.len(), |b| b + marker.len());
        Some((a, b))
    };
    let (Some((a, b)), Some((fa, fb))) = (section(current), section(body)) else { return file.to_vec() };
    let mut out = current[..a].to_vec();
    out.extend_from_slice(&body[fa..fb]);
    out.extend_from_slice(&current[b..]);
    out
}

/// The scroll bar of an `#sbox` (0x100105bb..0x1001081e): shown while the content reaches
/// below the inner rectangle, with the overflow + 10 pixels as its range.
fn update_scroll(b: &mut PageBox) {
    let Some(sb) = b.scroll else { return };
    let bottom = b.dialog.widgets[b.content..].iter().map(|w| w.rect.y1 - 1).max().unwrap_or(0);
    let overflow = bottom > b.inner[3];
    let w = &mut b.dialog.widgets[sb];
    w.visible = overflow;
    if let Kind::ScrollBar(bar) = &mut w.kind {
        bar.max = if overflow { bottom - b.inner[3] + 10 } else { 0 };
        bar.pos = bar.pos.clamp(0, bar.max);
    }
    let _ = b.outer;
}

/// The `{...}` groups of a command argument; `{}` is one empty command.
fn groups(a: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut rest = a;
    while let Some(open) = rest.iter().position(|&b| b == b'{') {
        let Some(close) = rest[open..].iter().position(|&b| b == b'}') else { break };
        out.push(rest[open + 1..open + close].to_vec());
        rest = &rest[open + close + 1..];
    }
    out
}

/// `SERVER|cmd&args` of a link: the part after `DS|` for DipServer.
fn request(command: &[u8]) -> Option<Vec<u8>> {
    let bar = command.iter().position(|&b| b == b'|')?;
    command[..bar].eq_ignore_ascii_case(b"DS").then(|| command[bar + 1..].to_vec())
}

/// The game path of a picture (0x10011ad0): `.gp` appended unless present, separators
/// normalised.
fn gp_path(name: &[u8]) -> String {
    let mut s = String::from_utf8_lossy(name).replace("\\\\", "\\").replace("//", "/");
    if !s.contains(".gp") && !s.contains(".GP") {
        s.push_str(".gp");
    }
    s
}

fn atoi(a: &[u8]) -> i32 {
    let s = String::from_utf8_lossy(a);
    let s = s.trim();
    let end =
        s.char_indices().find(|&(i, c)| !(c.is_ascii_digit() || (i == 0 && c == '-'))).map_or(s.len(), |(i, _)| i);
    s[..end].parse().unwrap_or(0)
}

/// Font names of `#font` (0x1000a610): the page's `#nfont`s, then the built-in names;
/// unknown names are `YellowFont`.
fn font_name(name: &[u8], nfonts: &[(Vec<u8>, FontId)]) -> FontId {
    if let Some((_, f)) = nfonts.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)) {
        return *f;
    }
    let s = String::from_utf8_lossy(name).to_ascii_uppercase();
    FontId(match s.as_str() {
        "F10" => "fn10",
        "F8" => "fn8",
        "WF" => "WhiteFont",
        "RF" => "RedFont",
        "BF" => "BlackFont",
        "SWF" => "SmallWhiteFont",
        "SYF" => "SmallYellowFont",
        "SRF" => "SmallRedFont",
        "SBF" => "SmallBlackFont",
        "BWF" | "SPWF" => "BigWhiteFont",
        "BYF" | "SPYF" => "BigYellowFont",
        "BRF" | "SPRF" => "BigRedFont",
        "BBF" | "SPBF" => "BigBlackFont",
        "XXWF" => "SpecialWhiteFont",
        "XXYF" => "SpecialYellowFont",
        "XXRF" => "SpecialRedFont",
        "XXBF" => "SpecialBlackFont",
        _ => "YellowFont",
    })
}

/// `#nfont(Name, GP, color[, shadow, dx, dy])` (0x1001b510): the global font built the same
/// way (`FontM16` colour 1 with the `FontM16s` shadow at 1, 1 is `fon16y1`).
fn nfont(tag: &Tag) -> Option<FontId> {
    let gp = String::from_utf8_lossy(tag.args.get(1)?).into_owned();
    let color = atoi(tag.args.get(2)?) as u8;
    let shadow = tag.args.get(3).map(|s| String::from_utf8_lossy(s).into_owned());
    let (dx, dy) = if tag.args.len() >= 6 { (atoi(&tag.args[4]), atoi(&tag.args[5])) } else { (0, 0) };
    let shadow = match shadow.as_deref() {
        None | Some("none") => None,
        Some("std") => Some((gp.as_str(), 1, 1)),
        Some(s) => Some((s, dx, dy)),
    };
    let found = font_like(&gp, color, shadow).or_else(|| font_like(&gp, color, None))?;
    named_font(found.name).map(|f| FontId(f.name))
}

/// A rectangle from `x:..,y:..,w:..,h:..` (0x1000a170), seeded with `parent` (inclusive).
/// Only complete pairs change it: (x, w), (x, x1), (xc, w), (x, xc), (x1, xc), the same for y.
/// `%NAME` in a y expression is the bottom of an element of the same box.
fn rect(spec: &[u8], parent: Area, elements: &[Element]) -> Area {
    let [px0, py0, px1, py1] = parent;
    let mut v: [Option<i32>; 8] = [None; 8];
    for part in spec.split(|&b| b == b',') {
        let Some(colon) = part.iter().position(|&b| b == b':') else { continue };
        let key = String::from_utf8_lossy(&part[..colon]).trim().to_ascii_lowercase();
        let e = &part[colon + 1..];
        let (slot, lo, hi, y) = match key.as_str() {
            "x" => (0, px0, px1, false),
            "x1" => (1, px0, px1, false),
            "xc" => (2, px0, px1, false),
            "w" => (3, 0, px1 - px0, false),
            "y" => (4, py0, py1, true),
            "y1" => (5, py0, py1, true),
            "yc" => (6, py0, py1, true),
            "h" => (7, 0, py1 - py0, true),
            _ => continue,
        };
        v[slot] = Some(expr(e, lo, hi, y, elements));
    }
    let pair = |a: Option<i32>, a1: Option<i32>, c: Option<i32>, w: Option<i32>, lo: i32, hi: i32| match (a, a1, c, w) {
        (Some(a), _, _, Some(w)) => (a, a + w),
        (Some(a), Some(a1), _, _) => (a, a1),
        (None, _, Some(c), Some(w)) => (c - w / 2, c - w / 2 + w),
        (Some(a), None, Some(c), None) => (a, 2 * c - a),
        (None, Some(a1), Some(c), None) => (2 * c - a1, a1),
        _ => (lo, hi),
    };
    let (x0, x1) = pair(v[0], v[1], v[2], v[3], px0, px1);
    let (y0, y1) = pair(v[4], v[5], v[6], v[7], py0, py1);
    [x0, y0, x1, y1]
}

/// An expression (0x10009db0, 0x10009f10): the first term is `lo + n`; later terms add
/// `±n`. `N%` is `N · (hi − lo) / 100`; `L`/`R`/`C` (`U`/`D`/`C`) are the edges and middle;
/// `%NAME` (y only) is the bottom of a named element of the box.
fn expr(e: &[u8], lo: i32, hi: i32, y: bool, elements: &[Element]) -> i32 {
    let mut value = lo;
    let mut sign = 0i32;
    let mut i = 0;
    while i < e.len() {
        let c = e[i];
        let term: Option<(i32, bool)> = match c {
            b' ' => {
                i += 1;
                continue;
            }
            b'+' | b'-' if i > 0 || c == b'+' => {
                sign = if c == b'+' { 1 } else { -1 };
                i += 1;
                continue;
            }
            b'%' if y => {
                let start = i + 1;
                let mut end = start;
                while end < e.len() && !matches!(e[end], b'+' | b'-' | b' ') {
                    end += 1;
                }
                let name = &e[start..end];
                i = end;
                // An unknown name would be 0 (an absolute screen y) in the original, which
                // hides every later line of a list with a gap (DipServer numbers its offer
                // lines with gaps); ACR continues below the box's last element.
                let found = elements.iter().rev().find(|el| el.name.eq_ignore_ascii_case(name)).map(|el| el.area[3]);
                Some((found.or_else(|| elements.last().map(|el| el.area[3])).unwrap_or(0), true))
            }
            b'L' | b'U' => {
                i += 1;
                Some((lo, true))
            }
            b'R' | b'D' => {
                i += 1;
                Some((hi, true))
            }
            b'C' => {
                i += 1;
                Some(((lo + hi) >> 1, true))
            }
            b'0'..=b'9' | b'-' => {
                let neg = c == b'-';
                if neg {
                    i += 1;
                }
                let start = i;
                while i < e.len() && e[i].is_ascii_digit() {
                    i += 1;
                }
                let mut n: i32 = std::str::from_utf8(&e[start..i]).ok().and_then(|s| s.parse().ok()).unwrap_or(0);
                if e.get(i) == Some(&b'%') {
                    n = n * (hi - lo) / 100;
                    i += 1;
                }
                Some((if neg { -n } else { n }, false))
            }
            _ => {
                i += 1;
                None
            }
        };
        let Some((n, absolute)) = term else { continue };
        value = match sign {
            0 if absolute => n,
            0 => lo + n,
            s => value + s * n,
        };
        sign = 1;
    }
    value
}

/// A token: leading spaces and a word, its link state and whether a line break precedes it.
struct Token {
    text: Vec<u8>,
    link: Option<usize>,
    newline: bool,
}

/// Splits a text into tokens (0x100136e0): `{`/`}` start and end link text (the k-th `{`
/// uses the k-th command), CR/LF forces a line break.
fn tokens(text: &[u8]) -> Vec<Token> {
    let mut out = Vec::new();
    let mut cur = Vec::new();
    let mut link = None;
    let mut links = 0;
    let mut newline = false;
    let mut in_word = false;
    let flush = |cur: &mut Vec<u8>, link: Option<usize>, newline: &mut bool, out: &mut Vec<Token>| {
        if !cur.is_empty() {
            out.push(Token { text: std::mem::take(cur), link, newline: *newline });
            *newline = false;
        }
    };
    for &c in text {
        match c {
            b'{' => {
                flush(&mut cur, link, &mut newline, &mut out);
                link = Some(links);
                links += 1;
                in_word = false;
            }
            b'}' => {
                flush(&mut cur, link, &mut newline, &mut out);
                link = None;
                in_word = false;
            }
            b'\r' | b'\n' => {
                flush(&mut cur, link, &mut newline, &mut out);
                newline = true;
                in_word = false;
            }
            b' ' if in_word => {
                flush(&mut cur, link, &mut newline, &mut out);
                cur.push(c);
                in_word = false;
            }
            b' ' => cur.push(c),
            _ => {
                cur.push(c);
                in_word = true;
            }
        }
    }
    flush(&mut cur, link, &mut newline, &mut out);
    out
}

/// Lays a text out as rows of text buttons in `area` and returns the number of lines.
/// Widths and the line height come from font a; plain runs use (a, a, a), links
/// (c hovered, b, a) (0x10013d40).
#[allow(clippy::too_many_arguments)]
fn layout_text(
    ctx: &Ctx,
    dialog: &mut Dialog,
    all: &mut Vec<Vec<u8>>,
    area: &Area,
    text: &[u8],
    commands: &[Vec<u8>],
    [a, b, c]: [FontId; 3],
    align: u8,
) -> i32 {
    let width = |t: &[u8]| ctx.fonts.width(ctx.lib, a, t);
    let lh = ctx.fonts.glyph_height(ctx.lib, a, b'y') + 1;
    let w = area[2] - area[0];
    let (mut y, mut line_x, mut lines) = (area[1], 0, 1);
    let mut run: Vec<u8> = Vec::new();
    let mut run_link: Option<usize> = None;
    let mut row: Vec<WidgetId> = Vec::new();
    let flush =
        |dialog: &mut Dialog, all: &mut Vec<Vec<u8>>, run: &mut Vec<u8>, link: Option<usize>, x: i32, y: i32| {
            let fonts = if link.is_some() { [c, b, a] } else { [a, a, a] };
            let id = dialog.add_text_button(ctx.lib, ctx.fonts, area[0] + x, y, run, fonts, Align::Left);
            match link.and_then(|k| commands.get(k)) {
                Some(cmd) => {
                    dialog.widgets[id].param = Some(all.len() as i32);
                    all.push(cmd.clone());
                }
                None => dialog.widgets[id].enabled = link.is_some(),
            }
            run.clear();
            id
        };
    let finish = |dialog: &mut Dialog, row: &mut Vec<WidgetId>| {
        let ws = &dialog.widgets;
        let (Some(min), Some(max)) =
            (row.iter().map(|&i| ws[i].rect.x0).min(), row.iter().map(|&i| ws[i].rect.x1 - 1).max())
        else {
            return;
        };
        let shift = match align {
            1 => (w - (max - min)) >> 1,
            2 => w - (max - min),
            _ => 0,
        };
        for &i in row.iter() {
            dialog.shift(i, shift, 0);
        }
        row.clear();
    };
    let toks = tokens(text);
    for (n, t) in toks.iter().enumerate() {
        let mut tok = t.text.as_slice();
        let tw = width(tok);
        let pending = width(&run);
        let fits = line_x + pending + tw <= w || (run.is_empty() && line_x == 0);
        if n > 0 && (t.link != run_link || t.newline || !fits) {
            if !run.is_empty() {
                row.push(flush(dialog, all, &mut run, run_link, line_x, y));
            }
            if t.newline || !fits {
                finish(dialog, &mut row);
                y += lh;
                lines += 1;
                line_x = 0;
                if !t.newline {
                    while let [b' ', rest @ ..] = tok {
                        tok = rest;
                    }
                }
            } else {
                line_x += pending;
            }
        } else if n == 0 {
            while let [b' ', rest @ ..] = tok {
                tok = rest;
            }
        }
        run.extend_from_slice(tok);
        run_link = t.link;
    }
    if !run.is_empty() || toks.is_empty() {
        row.push(flush(dialog, all, &mut run, run_link, line_x, y));
    }
    finish(dialog, &mut row);
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rects() {
        let window = [762, 77, 1016, 470];
        assert_eq!(rect(b"x:0,y:142,w:100%-10,h:100%-188", window, &[]), [762, 219, 1006, 424]);
        assert_eq!(rect(b"x:0,y:100%-40,w:100%,h:45", window, &[]), [762, 430, 1016, 475]);
        let dn = [762, 430, 1016, 475];
        assert_eq!(rect(b"x:10%,y:20,w:80%,h:24", dn, &[]), [787, 450, 990, 474]);
        let els = [Element { name: b"TX0".to_vec(), area: [0, 0, 0, 300] }];
        assert_eq!(rect(b"x:0,y:%TX0+8,w:50%,h:20", [762, 219, 1006, 424], &els)[1], 308);
        // A single field changes nothing.
        assert_eq!(rect(b"x:5", window, &[]), window);
    }

    #[test]
    fn requests_and_paths() {
        assert_eq!(request(b"DS|perform&0&1&0").as_deref(), Some(&b"perform&0&1&0"[..]));
        assert_eq!(request(b""), None);
        assert_eq!(groups(b"{DS|a}{}"), vec![b"DS|a".to_vec(), Vec::new()]);
        assert_eq!(gp_path(b"dip/SevMo"), "dip/SevMo.gp");
        assert_eq!(gp_path(b"Dip\\\\lic.gp"), "Dip\\lic.gp");
    }

    #[test]
    fn sections() {
        let page = b"#ctxt(x)
#time(4000,y)
<FRM1>
<FRM1>
";
        let merged = merge_section(
            page,
            b"<FRM1>
#txt[%TX1](a)
<FRM1>
",
        );
        assert_eq!(
            merged,
            b"#ctxt(x)
#time(4000,y)
<FRM1>
#txt[%TX1](a)
<FRM1>
"
        );
        assert_eq!(merge_section(page, b"#font(a)"), b"#font(a)");
    }

    #[test]
    fn token_split() {
        let t = tokens(b"Hallo Welt {* 1000 Gold}");
        let words: Vec<&[u8]> = t.iter().map(|t| t.text.as_slice()).collect();
        assert_eq!(words, vec![&b"Hallo"[..], b" Welt", b" ", b"*", b" 1000", b" Gold"]);
        assert_eq!(t[3].link, Some(0));
    }
}
