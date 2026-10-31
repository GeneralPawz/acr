//! The widget tree the driver reports: the top screen's widgets (from its [`Dialog`]), the
//! HUD's icons, mission messages and the cursor, all in window pixels; and the lookup of
//! click targets in it.

use serde_json::{Value, json};

use super::protocol::{Target, UnitFilter, pick_label};
use crate::canvas::Canvas;
use crate::ui::Screen;
use crate::ui::dialog::{Dialog, Kind, OK};
use crate::ui::draw::{Rect, Sprite};
use crate::{Game, Mode};

/// Screens that are modal messages the dialog policy handles: mission pages, questions
/// and objectives (`MessageWindow`), multiple choices (`ChoiceWindow`).
const MODAL: [&str; 2] = ["MessageWindow", "ChoiceWindow"];

/// One entry of the tree.
#[derive(Debug, Clone)]
pub struct Item {
    pub id: String,
    pub kind: &'static str,
    pub label: String,
    /// Window pixels: x, y, width, height of the visible part.
    pub rect: [f32; 4],
    pub enabled: bool,
    pub visible: bool,
    pub hover: bool,
    pub selected: bool,
    pub clickable: bool,
    /// The value a click reports (`OK` = 245, cancel = 246), if any.
    pub param: Option<i32>,
    /// HUD icons: the number in the corner (queued units).
    pub count: Option<i32>,
    /// HUD icons: a right click does something (cancels production).
    pub right: bool,
    /// HUD icons: the number at the bottom right (a formation's number).
    pub number: Option<i32>,
}

impl Item {
    pub fn centre(&self) -> [f32; 2] {
        [self.rect[0] + self.rect[2] / 2.0, self.rect[1] + self.rect[3] / 2.0]
    }

    pub fn to_json(&self) -> Value {
        let r = self.rect.map(|v| (v * 10.0).round() / 10.0);
        let mut v = json!({"id": self.id, "kind": self.kind, "label": self.label, "rect": r});
        let m = v.as_object_mut().unwrap();
        if !self.enabled {
            m.insert("enabled".into(), false.into());
        }
        if !self.visible {
            m.insert("visible".into(), false.into());
        }
        if self.hover {
            m.insert("hover".into(), true.into());
        }
        if self.selected {
            m.insert("selected".into(), true.into());
        }
        if self.clickable {
            m.insert("clickable".into(), true.into());
        }
        if let Some(n) = self.count {
            m.insert("count".into(), n.into());
        }
        if self.right {
            m.insert("right_click".into(), true.into());
        }
        if let Some(n) = self.number {
            m.insert("number".into(), n.into());
        }
        v
    }
}

/// Game text (CP1252, CR LF line ends) as UTF-8 with LF line ends.
pub fn decode(bytes: &[u8]) -> String {
    acr_formats::CodePage::Cp1252.decode(bytes).replace("\r\n", "\n")
}

fn area(r: &Rect) -> i64 {
    if r.is_empty() { 0 } else { i64::from(r.width()) * i64::from(r.height()) }
}

fn window_rect(canvas: Canvas, r: &Rect) -> [f32; 4] {
    canvas.to_window([r.x0 as f32, r.y0 as f32, r.width() as f32, r.height() as f32])
}

/// The items of a screen's dialog on the menu canvas.
pub fn dialog_items(name: &str, d: &Dialog, canvas: Canvas, all: bool) -> Vec<Item> {
    let mut out = Vec::new();
    for (i, w) in d.widgets.iter().enumerate() {
        let shown = d.shown_rect(i);
        let vis = shown.intersect(&w.clip);
        // Mostly inside its clip: the main menu's other pages lie outside it.
        let visible = w.visible && area(&vis) > 0 && area(&vis) * 2 >= area(&shown);
        let (kind, label, clickable, selected) = match &w.kind {
            Kind::TextButton(b) => ("text", decode(&b.text), w.param.is_some(), false),
            Kind::GpTextButton(b) => ("button", decode(&b.text), true, false),
            Kind::GpButton { .. } => ("button", w.hint.as_deref().map(decode).unwrap_or_default(), true, false),
            Kind::Universal(u) => ("button", decode(&u.text), true, u.state > 0),
            Kind::ListBox(_) => ("list", String::new(), true, false),
            Kind::ComboBox(c) => ("combo", c.lines.get(c.cur).map(|l| decode(l)).unwrap_or_default(), true, c.open),
            Kind::TextViewer(t) => ("viewer", decode(t.text()), false, false),
            Kind::InputBox(b) => ("input", decode(&b.text), true, b.focus),
            Kind::Slider(s) => ("slider", s.pos.to_string(), true, false),
            Kind::ScrollBar(_) => ("scrollbar", String::new(), true, false),
            Kind::Area | Kind::Outline(_) | Kind::Picture { .. } | Kind::GpPicture { .. } => {
                if w.param.is_none() && !all {
                    continue;
                }
                ("picture", w.hint.as_deref().map(decode).unwrap_or_default(), w.param.is_some(), false)
            }
        };
        if !all && !visible {
            continue;
        }
        // Static texts without any text say nothing.
        if !all && !clickable && label.trim().is_empty() {
            continue;
        }
        out.push(Item {
            id: format!("{name}/{i}"),
            kind,
            label,
            rect: window_rect(canvas, &vis),
            enabled: w.enabled,
            visible,
            hover: w.hover,
            selected,
            clickable: clickable && w.enabled,
            param: w.param,
            count: None,
            right: false,
            number: None,
        });
        if let (Kind::ListBox(l), true) = (&w.kind, visible) {
            for row in 0..l.rows {
                let idx = l.first + row;
                let Some(text) = l.items.get(idx) else { break };
                let r = Rect::new(shown.x0, shown.y0 + row as i32 * l.row_h, shown.width(), l.row_h).intersect(&w.clip);
                out.push(Item {
                    id: format!("{name}/{i}#{idx}"),
                    kind: "row",
                    label: decode(text),
                    rect: window_rect(canvas, &r),
                    enabled: w.enabled,
                    visible: true,
                    hover: l.hovered == Some(idx),
                    selected: l.cur == idx,
                    clickable: w.enabled,
                    param: None,
                    count: None,
                    right: false,
                    number: None,
                });
            }
        }
    }
    for (id, k, r) in d.dropdown_rows() {
        let Kind::ComboBox(c) = &d.widgets[id].kind else { continue };
        out.push(Item {
            id: format!("{name}/{id}~{k}"),
            kind: "option",
            label: decode(&c.lines[k]),
            rect: window_rect(canvas, &r),
            enabled: true,
            visible: true,
            hover: false,
            selected: c.cur == k,
            clickable: true,
            param: None,
            count: None,
            right: false,
            number: None,
        });
    }
    out
}

/// The top screen and whether it is built.
pub fn top(game: &Game) -> Option<(&dyn Screen, bool)> {
    game.ui.screens().last()
}

/// The game is shown with nothing over it.
pub fn game_has_input(game: &Game) -> bool {
    matches!(&game.mode, Mode::InGame(g) if g.ready()) && !game.ui.dialog_open()
}

/// Name of what is shown: the top screen, `game`, or `intro`.
pub fn screen_name(game: &Game) -> String {
    match (&game.mode, top(game)) {
        (Mode::Intro, _) => "intro".into(),
        (_, Some((s, _))) => s.name().into(),
        (Mode::InGame(_), None) => "game".into(),
        (Mode::Menus, None) => "none".into(),
    }
}

/// The items of the top screen (built ones only) and, over the game, the HUD icons.
pub fn items(game: &Game, all: bool) -> Vec<Item> {
    let mut out = Vec::new();
    if let (Some((screen, true)), Some(canvas)) = (top(game), game.canvas())
        && let Some(d) = screen.dialog()
    {
        out = dialog_items(screen.name(), d, canvas, all);
    }
    if game_has_input(game)
        && let Mode::InGame(g) = &game.mode
    {
        for icon in g.hud_icons().into_iter().chain(g.editor_icons(&game.ui.ctx())) {
            out.push(Item {
                id: icon.id.clone(),
                kind: "icon",
                label: decode(&icon.hint),
                rect: icon.rect,
                enabled: !icon.disabled && !icon.gray,
                visible: true,
                hover: false,
                selected: icon.selected,
                clickable: icon.clickable,
                param: None,
                count: icon.count,
                right: icon.right_clickable,
                number: icon.number,
            });
        }
    }
    out
}

/// Texts drawn over the map: mission hints, messages and the central box.
pub fn messages(game: &Game) -> Vec<String> {
    let Mode::InGame(g) = &game.mode else { return Vec::new() };
    if !g.ready() {
        return Vec::new();
    }
    let mut out: Vec<String> = Vec::new();
    for d in g.overlay(&game.ui.ctx()) {
        if let Sprite::Text { text, .. } | Sprite::GameText { text, .. } | Sprite::GameTextFaded { text, .. } =
            &d.sprite
        {
            let t = decode(text);
            // Each line comes twice: shadow and text.
            if out.last() != Some(&t) && !t.trim().is_empty() {
                out.push(t);
            }
        }
    }
    out
}

/// An open modal message: its screen, texts, buttons and the default button.
pub struct Modal {
    pub screen: String,
    pub texts: Vec<String>,
    pub buttons: Vec<Item>,
    pub default: Option<Item>,
}

impl Modal {
    pub fn to_json(&self) -> Value {
        json!({
            "screen": self.screen,
            "text": self.texts.join("\n"),
            "buttons": self.buttons.iter().filter(|b| !b.label.is_empty()).map(|b| b.label.clone()).collect::<Vec<_>>(),
            "default": self.default.as_ref().map(|b| b.label.clone()),
        })
    }
}

/// The modal message over the game, once it is built.
pub fn modal(game: &Game) -> Option<Modal> {
    if !game.ui.in_game() {
        return None;
    }
    let (screen, built) = top(game)?;
    if !built || !MODAL.contains(&screen.name()) {
        return None;
    }
    let items = dialog_items(screen.name(), screen.dialog()?, game.canvas()?, false);
    let texts = items.iter().filter(|i| !i.clickable && !i.label.trim().is_empty()).map(|i| i.label.clone()).collect();
    let buttons: Vec<Item> = items.into_iter().filter(|i| i.clickable && i.kind != "scrollbar").collect();
    let default = buttons.iter().find(|b| b.param == Some(OK)).or(buttons.first()).cloned();
    Some(Modal { screen: screen.name().into(), texts, buttons, default })
}

/// The whole tree for `ui`.
pub fn describe(game: &Game, all: bool) -> Value {
    let stack: Vec<&str> = game.ui.screens().map(|(s, _)| s.name()).collect();
    let (built, busy) = top(game).map_or((true, false), |(s, b)| (b, s.busy()));
    let mut v = json!({
        "screen": screen_name(game),
        "stack": stack,
        "built": built,
        "busy": busy,
        "in_game": matches!(&game.mode, Mode::InGame(g) if g.ready()),
        "loading": matches!(&game.mode, Mode::InGame(g) if !g.ready()),
        "widgets": items(game, all).iter().map(Item::to_json).collect::<Vec<_>>(),
        "dialog": modal(game).map(|m| m.to_json()),
    });
    let m = v.as_object_mut().unwrap();
    if let Mode::InGame(g) = &game.mode
        && g.ready()
    {
        m.insert("messages".into(), messages(game).into());
        m.insert(
            "cursor".into(),
            json!({
                "pos": [game.cursor.x.round(), game.cursor.y.round()],
                "mode": g.cursor_mode(),
                "outline": g.touch_outline(),
            }),
        );
    }
    if let Some(size) = game.frame_size() {
        m.insert("window".into(), json!(size));
    }
    v
}

/// Where a pointer command goes (window pixels) and what was hit.
pub fn resolve(game: &mut Game, target: &Target) -> Result<([f32; 2], Value), String> {
    match target {
        Target::Pos(p) => Ok((*p, json!({"pos": p}))),
        Target::Label { text, exact, index } => {
            let all = items(game, false);
            let candidates: Vec<&Item> = all.iter().filter(|i| i.clickable).collect();
            match pick_label(candidates.iter().map(|i| i.label.as_str()), text, *exact, *index) {
                Some(k) => Ok((candidates[k].centre(), candidates[k].to_json())),
                None => {
                    // Also look at disabled ones, to say why.
                    let disabled: Vec<&Item> = all.iter().filter(|i| !i.clickable && !i.enabled).collect();
                    if let Some(k) = pick_label(disabled.iter().map(|i| i.label.as_str()), text, *exact, *index) {
                        return Err(format!("{:?} is disabled", disabled[k].label));
                    }
                    Err(format!("no clickable widget labelled {text:?}"))
                }
            }
        }
        Target::Id(id) => items(game, true)
            .into_iter()
            .find(|i| &i.id == id)
            .map(|i| (i.centre(), i.to_json()))
            .ok_or_else(|| format!("no widget {id}")),
        Target::UnitOf(filter) => {
            let Mode::InGame(g) = &game.mode else { return Err("no game".into()) };
            let world = g.world().ok_or("no world yet")?;
            let id = world
                .units
                .iter()
                .find(|u| filter_matches(world, filter, u))
                .map(|u| u.id.0)
                .ok_or(format!("no object like {filter:?}"))?;
            resolve(game, &Target::Unit(id))
        }
        Target::Unit(id) => {
            let Mode::InGame(g) = &mut game.mode else { return Err("no game".into()) };
            let world = g.world().ok_or("no world yet")?;
            let u = world.unit(acr_sim::world::EntityId(*id)).filter(|u| u.alive).ok_or(format!("no unit {id}"))?;
            let (pos, kind) = ((u.pos.x >> 4, u.pos.y >> 4), world.catalog.get(u.kind).id.clone());
            let mut b = g.unit_window_box(u);
            if !in_view(g.window_size_px(), [b[0] + b[2] / 2.0, b[1] + b[3] / 2.0]) {
                g.centre_on(pos.0, pos.1);
                let world = g.world().ok_or("no world yet")?;
                let u = world.unit(acr_sim::world::EntityId(*id)).ok_or(format!("no unit {id}"))?;
                b = g.unit_window_box(u);
            }
            // Low in the box: the feet, below other units' boxes in front.
            let p = [b[0] + b[2] / 2.0, b[1] + b[3] * 0.6];
            Ok((p, json!({"unit": id, "type": kind, "pos": p})))
        }
        Target::Near(filter, [dx, dy]) => {
            let Mode::InGame(g) = &game.mode else { return Err("no game".into()) };
            let world = g.world().ok_or("no world yet")?;
            let found: Vec<(i64, i64)> = world
                .units
                .iter()
                .filter(|u| filter_matches(world, filter, u))
                .map(|u| (i64::from(u.pos.x >> 4), i64::from(u.pos.y >> 4)))
                .collect();
            if found.is_empty() {
                return Err(format!("no object like {filter:?}"));
            }
            let n = found.len() as i64;
            let (sx, sy) = found.iter().fold((0, 0), |(a, b), (x, y)| (a + x, b + y));
            resolve(game, &Target::World([(sx / n) as i32 + dx, (sy / n) as i32 + dy]))
        }
        Target::World([x, y]) => {
            let Mode::InGame(g) = &mut game.mode else { return Err("no game".into()) };
            let mut p = g.world_to_window(*x, *y);
            if !in_view(g.window_size_px(), p) {
                g.centre_on(*x, *y);
                p = g.world_to_window(*x, *y);
            }
            Ok((p, json!({"world": [x, y], "pos": p})))
        }
    }
}

/// A living object on the map (not inside another) of the filter's owner and type.
fn filter_matches(world: &acr_sim::World, filter: &UnitFilter, u: &acr_sim::Unit) -> bool {
    u.living()
        && u.inside_of.is_none()
        && filter.owner.is_none_or(|o| u.owner == o)
        && filter
            .kind
            .as_deref()
            .is_none_or(|k| world.catalog.get(u.kind).id.to_lowercase().contains(&k.to_lowercase()))
}

/// The corners of a selection rectangle over the matching objects (the first `max` by id),
/// after centring the view on them.
pub fn around(game: &mut Game, filter: &UnitFilter, max: usize) -> Result<([f32; 2], [f32; 2], usize), String> {
    let Mode::InGame(g) = &mut game.mode else { return Err("no game".into()) };
    let world = g.world().ok_or("no world yet")?;
    let ids: Vec<_> = world.units.iter().filter(|u| filter_matches(world, filter, u)).take(max).map(|u| u.id).collect();
    if ids.is_empty() {
        return Err("no matching objects".into());
    }
    let n = ids.len() as i64;
    let (sx, sy) = ids
        .iter()
        .filter_map(|&id| world.unit(id))
        .fold((0i64, 0i64), |(x, y), u| (x + i64::from(u.pos.x >> 4), y + i64::from(u.pos.y >> 4)));
    g.centre_on((sx / n) as i32, (sy / n) as i32);
    let world = g.world().ok_or("no world yet")?;
    let centres: Vec<[f32; 2]> = ids
        .iter()
        .filter_map(|&id| world.unit(id))
        .map(|u| {
            let b = g.unit_window_box(u);
            [b[0] + b[2] / 2.0, b[1] + b[3] / 2.0]
        })
        .collect();
    let lo = centres.iter().fold([f32::MAX; 2], |a, c| [a[0].min(c[0]), a[1].min(c[1])]);
    let hi = centres.iter().fold([f32::MIN; 2], |a, c| [a[0].max(c[0]), a[1].max(c[1])]);
    // At least a drag, not a click.
    let hi = [hi[0].max(lo[0] + 6.0), hi[1].max(lo[1] + 6.0)];
    Ok((lo, hi, ids.len()))
}

/// Inside the map part of the window, away from the edges (which scroll) and the HUD.
fn in_view(window: [f32; 2], p: [f32; 2]) -> bool {
    p[0] > window[0] * 0.1 && p[0] < window[0] * 0.75 && p[1] > window[1] * 0.1 && p[1] < window[1] * 0.7
}
