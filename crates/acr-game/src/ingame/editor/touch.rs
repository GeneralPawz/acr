//! The test editor as the touch skin's drawer shows it (docs/webui.md, "Test editor"): the
//! window's meaning without its pixel layout. [`Editor::touch_view`] lists what the drawer
//! draws, every cell of the list whatever the window's scroll, and [`Editor::intent`] turns
//! the ids of that view back into the [`Act`] a click on the classic window would send.
//! Everything the drawer can do ends in [`Editor::apply`]; a refusal is a [`Reject`] and
//! changes nothing. The mirrors here hold no protocol types (as `hud/touch_skin.rs` does):
//! strings are the game's CP1252 bytes until `webui` decodes them.

use acr_sim::{TypeId, World};

pub use super::CELL;

use super::{
    Act, ENTRIES, Editor, FACINGS, FIRST_EXTRA, GELAENDE, Item, NEUTRAL, Pick, View, battle, formations, group_name,
    groups, levels, presets, sides,
};
use crate::hud;
use crate::ui::Ctx;
use crate::ui::dialog::Reject;
use crate::ui::draw::{Draw, GAME_PALETTE, Rect};

/// A button with a text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TouchButton {
    pub id: String,
    pub text: Vec<u8>,
    pub on: bool,
}

/// A player colour.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TouchSwatch {
    pub id: String,
    pub label: String,
    pub colour: [u8; 3],
    pub team: Option<u8>,
    pub on: bool,
    pub teams: bool,
}

/// A cell of a list: the draws of its original icon in a `CELL` square.
#[derive(Debug, Clone, PartialEq)]
pub struct TouchCell {
    pub id: String,
    pub name: Vec<u8>,
    pub draws: Vec<Draw>,
    pub on: bool,
    pub levels: bool,
}

/// A category of a list.
#[derive(Debug, Clone, PartialEq)]
pub struct TouchGroup {
    pub header: String,
    pub cells: Vec<TouchCell>,
}

/// A control of the placing or the battle view.
#[derive(Debug, Clone, PartialEq)]
pub enum TouchControl {
    Stepper { less: String, more: String, text: Vec<u8> },
    Button { id: String, text: Vec<u8>, on: bool },
    Icon { id: String, text: Vec<u8>, on: bool, draws: Vec<Draw> },
    Line { text: Vec<u8> },
}

/// The drawer's content of one frame.
#[derive(Debug, Clone, PartialEq)]
pub struct TouchEditor {
    pub title: Vec<u8>,
    pub toggle: TouchButton,
    pub battle: bool,
    pub status: Vec<u8>,
    pub armed: Option<Vec<u8>>,
    pub swatches: Vec<TouchSwatch>,
    pub entries: Vec<TouchButton>,
    pub army: Vec<TouchButton>,
    pub groups: Vec<TouchGroup>,
    pub levels: Vec<TouchButton>,
    pub controls: Vec<TouchControl>,
}

/// What an id of the drawer asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ingame) enum Target {
    /// What a click on the classic window's item sends.
    Act(Act),
    /// A long press on a unit: its level list opens.
    Levels(TypeId),
}

/// The id of what `pick` places, in the list of its entry.
fn pick_id(world: &World, pick: Pick) -> String {
    match pick {
        Pick::Unit(kind) => format!("editor/unit/{}", world.catalog.get(kind).id),
        Pick::Nature(group, index) => {
            format!("editor/nature/{}/{index}", String::from_utf8_lossy(&group.code()))
        }
        Pick::Ground(tool) => format!("editor/ground/{}", super::ground_name(tool).to_ascii_lowercase()),
    }
}

fn button(id: impl Into<String>, text: impl Into<Vec<u8>>, on: bool) -> TouchButton {
    TouchButton { id: id.into(), text: text.into(), on }
}

fn stepper(name: &str, text: String) -> TouchControl {
    TouchControl::Stepper {
        less: format!("editor/{name}/-"),
        more: format!("editor/{name}/+"),
        text: text.into_bytes(),
    }
}

fn line(text: impl Into<Vec<u8>>) -> TouchControl {
    TouchControl::Line { text: text.into() }
}

fn control(id: &str, text: impl Into<Vec<u8>>, on: bool) -> TouchControl {
    TouchControl::Button { id: id.to_owned(), text: text.into(), on }
}

/// The draws of one icon cell: the classic window's art in a `CELL` square at the origin.
pub(super) fn cell_draws(item: &Item, ctx: &Ctx) -> Vec<Draw> {
    let mut out = Vec::new();
    let rect = Rect::new(0, 0, CELL, CELL);
    Editor::draws(std::slice::from_ref(item), ctx, rect, &mut out);
    out
}

fn not_a_number(_: std::num::ParseIntError) -> Reject {
    Reject::NoWidget
}

impl Editor {
    /// The drawer's content, or none while the game's palette is not loaded.
    pub fn touch_view(&self, ctx: &Ctx, world: &World, view: &View, local: u8) -> Option<TouchEditor> {
        let palette = ctx.lib.palette(GAME_PALETTE)?;
        let battle = self.battle.on;
        let toggle = button("editor/battle", if battle { "Einheiten" } else { "Schlacht" }, battle);
        let levels = self.menu.map_or_else(Vec::new, |menu| {
            let names = levels::names(world, menu.kind, view.upgrade_name);
            names
                .into_iter()
                .enumerate()
                .map(|(n, text)| {
                    button(
                        format!("editor/level/{n}"),
                        text,
                        self.pick == Some(Pick::Unit(menu.kind)) && self.level == n,
                    )
                })
                .collect()
        });
        let armed = match (self.pick, self.army.armed) {
            (Some(pick), _) => Some([b"Typ: ".to_vec(), Self::pick_name(world, view, pick)].concat()),
            (None, Some(size)) => Some(format!("Armee {}", size.label()).into_bytes()),
            (None, None) => None,
        };
        let mut out = TouchEditor {
            title: view.title.clone(),
            toggle,
            battle,
            status: self.status.clone(),
            armed,
            swatches: Vec::new(),
            entries: Vec::new(),
            army: Vec::new(),
            groups: Vec::new(),
            levels,
            controls: Vec::new(),
        };
        if battle {
            out.controls = self.battle_touch(ctx);
            return Some(out);
        }
        let teams = self.sides == sides::Mode::Teams;
        out.swatches = (0..8u8)
            .map(|p| {
                let [r, g, b] = palette.rgb(hud::color::PLAYER.wrapping_add(4 * p));
                let label = if p == NEUTRAL { "Neutral".to_owned() } else { format!("Spieler {}", p + 1) };
                let team = teams && p != NEUTRAL;
                TouchSwatch {
                    id: format!("editor/owner/{p}"),
                    label,
                    colour: [r, g, b],
                    team: team.then(|| self.team[usize::from(p)]),
                    on: self.owner == p,
                    teams: team,
                }
            })
            .collect();
        out.entries = ENTRIES
            .iter()
            .enumerate()
            .map(|(i, (label, _))| button(format!("editor/nation/{i}"), label.as_bytes(), i == self.entry))
            .collect();
        if self.entry < FIRST_EXTRA {
            let current = self.army.era_for(self.entry);
            let eras = presets::eras(self.entry);
            if eras.len() > 1 {
                for e in eras {
                    out.army.push(button(format!("editor/army/era/{}", e.code()), e.label(), current == Some(e)));
                }
            }
            for s in presets::Size::ALL {
                out.army.push(button(format!("editor/army/size/{}", s.code()), s.label(), self.army.armed == Some(s)));
            }
        }
        let cell = Rect::new(0, 0, CELL, CELL);
        out.groups = groups(world, self.entry)
            .into_iter()
            .map(|g| TouchGroup {
                header: g.header.to_owned(),
                cells: g
                    .kinds
                    .into_iter()
                    .map(|pick| {
                        let on = self.pick == Some(pick);
                        let item = Self::cell_item(world, view, pick, cell, on);
                        let name = if matches!(pick, Pick::Unit(_)) {
                            item.text.clone()
                        } else {
                            item.name.clone().into_bytes()
                        };
                        TouchCell {
                            id: pick_id(world, pick),
                            name,
                            draws: cell_draws(&item, ctx),
                            on,
                            levels: matches!(pick, Pick::Unit(_)),
                        }
                    })
                    .collect(),
            })
            .collect();
        out.controls = self.placing_touch(world, view, local);
        Some(out)
    }

    /// The controls under the list, in the classic window's order.
    fn placing_touch(&self, world: &World, view: &View, local: u8) -> Vec<TouchControl> {
        let mut out = vec![stepper("count", format!("Anzahl: {}", self.count()))];
        let form = match self.unit() {
            Some(kind) if self.form > 0 => {
                formations(world, kind).get(self.form - 1).map_or("lose".into(), |c| group_name(world, c.group))
            }
            _ => "lose".into(),
        };
        out.push(stepper("form", format!("Formation: {form}")));
        if self.entry == GELAENDE {
            out.push(stepper("brush", format!("Pinsel: {}", self.radius())));
        }
        out.push(line(self.level_text(world, view)));
        out.push(stepper("facing", format!("Richtung: {}", FACINGS[usize::from(self.facing() / 32) & 7])));
        let owner = if self.owner == NEUTRAL { "Neutral".to_owned() } else { format!("Spieler {}", self.owner + 1) };
        out.push(line(format!("Besitzer: {owner}")));
        let fight = match self.sides {
            sides::Mode::Off => "Kampf: AUS (klicken: an)",
            sides::Mode::Free => "Kampf: AN, alle gegen alle (klicken: Teams)",
            sides::Mode::Teams => "Kampf: Teams (Farbe lange druecken: Team)",
        };
        out.push(control("editor/peace", fight, self.sides != sides::Mode::Off));
        out.push(control("editor/reload", "Kanonen sofort laden", false));
        let auto = if self.auto_reload { "Kanonen: sofort nachladen AN" } else { "Kanonen: sofort nachladen AUS" };
        out.push(control("editor/auto_reload", auto, self.auto_reload));
        let me = if local == NEUTRAL { "Neutral".to_owned() } else { format!("Spieler {}", local + 1) };
        out.push(control("editor/control", format!("Ich steuere: {me}"), false));
        if let Some(pick) = self.pick {
            out.push(line([b"Typ: ".to_vec(), Self::pick_name(world, view, pick)].concat()));
        }
        out.push(line("Symbol lange druecken: Ausbau"));
        out
    }

    /// What the drawer's id `id` asks for, with the right button for a long press. Refused
    /// as the classic window would refuse the click; nothing is clamped.
    pub(in crate::ingame) fn intent(&self, world: &World, id: &str, right: bool) -> Result<Target, Reject> {
        let rest = id.strip_prefix("editor/").ok_or(Reject::NoWidget)?;
        let parts: Vec<&str> = rest.split('/').collect();
        let left = |act: Act| if right { Err(Reject::Kind("a left-click target")) } else { Ok(Target::Act(act)) };
        // These work in both views.
        match parts.as_slice() {
            ["close"] => {
                return if !self.menu_open() && !self.army.open { Err(Reject::Hidden) } else { left(Act::Close) };
            }
            ["stop"] => {
                return if self.pick.is_none() && self.army.armed.is_none() {
                    Err(Reject::Disabled)
                } else {
                    left(Act::Stop)
                };
            }
            ["battle"] => return left(Act::Battle(battle::Act::Toggle)),
            _ => {}
        }
        // The battle view hides the list and its controls, and the list hides the battle's.
        if (parts[0] == "battle") != self.battle.on {
            return Err(Reject::Hidden);
        }
        match parts.as_slice() {
            ["battle", tail @ ..] => left(Act::Battle(self.battle_intent(tail)?)),
            ["nation", i] => {
                let i: usize = i.parse().map_err(not_a_number)?;
                if i >= ENTRIES.len() { Err(Reject::Range) } else { left(Act::Entry(i)) }
            }
            ["owner", p] => {
                let p: u8 = p.parse().map_err(not_a_number)?;
                if p >= 8 {
                    return Err(Reject::Range);
                }
                if !right {
                    return Ok(Target::Act(Act::Owner(p)));
                }
                if self.sides == sides::Mode::Teams && p != NEUTRAL {
                    Ok(Target::Act(Act::Team(p)))
                } else {
                    Err(Reject::Disabled)
                }
            }
            ["army", kind, code] => {
                if self.entry >= FIRST_EXTRA {
                    return Err(Reject::Hidden);
                }
                let code: u8 = code.parse().map_err(not_a_number)?;
                match *kind {
                    "era" => {
                        let era = presets::Era::from_code(code).ok_or(Reject::Range)?;
                        if !presets::eras(self.entry).contains(&era) {
                            return Err(Reject::Hidden);
                        }
                        left(Act::Army(presets::Act::Era(era)))
                    }
                    "size" => left(Act::Army(presets::Act::Size(presets::Size::from_code(code).ok_or(Reject::Range)?))),
                    _ => Err(Reject::NoWidget),
                }
            }
            ["unit" | "nature" | "ground", ..] => {
                let pick = groups(world, self.entry)
                    .into_iter()
                    .flat_map(|g| g.kinds)
                    .find(|&p| pick_id(world, p) == id)
                    .ok_or(Reject::NoWidget)?;
                match (pick, right) {
                    (pick, false) => Ok(Target::Act(Act::Pick(pick))),
                    (Pick::Unit(kind), true) => Ok(Target::Levels(kind)),
                    (_, true) => Err(Reject::Kind("a unit")),
                }
            }
            ["level", n] => {
                let n: usize = n.parse().map_err(not_a_number)?;
                let menu = self.menu.ok_or(Reject::Hidden)?;
                if n > levels::steps(world, menu.kind) { Err(Reject::Range) } else { left(Act::SetLevel(n)) }
            }
            [what @ ("count" | "form" | "facing" | "brush"), sign @ ("-" | "+")] => {
                let d = if *sign == "-" { -1 } else { 1 };
                match *what {
                    "count" => left(Act::Count(d)),
                    "form" => left(Act::Form(d)),
                    "facing" => left(Act::Facing(d)),
                    _ if self.entry == GELAENDE => left(Act::Brush(d)),
                    _ => Err(Reject::Hidden),
                }
            }
            ["peace"] => left(Act::Peace),
            ["reload"] => left(Act::Reload),
            ["auto_reload"] => left(Act::AutoReload),
            ["control"] => left(Act::Control),
            _ => Err(Reject::NoWidget),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use acr_formats::defs::{NationDef, UnitDef};
    use acr_sim::catalog::{Catalog, ObjectType};
    use acr_sim::formation::Formations;
    use acr_sim::map::MapGrid;
    use acr_sim::tech::Tech;

    use super::super::levels::tests::{NATION, PIKEMAN};
    use super::super::tests::nature_world;
    use super::super::{GroundTool, NATUR};
    use super::*;
    use crate::ui::Ui;

    /// England's list: a pikeman with three training levels and eighty plain copies, and the
    /// nature world's trees and stones.
    fn world() -> (World, TypeId) {
        let mut c = Catalog::new();
        let def = UnitDef::parse(PIKEMAN.as_bytes());
        let pik = c.add(ObjectType::from_def("EngPik(EN)", &def));
        for n in 0..80 {
            c.add(ObjectType::from_def(&format!("EngX{n}(EN)"), &def));
        }
        let mut tech = Tech::default();
        tech.add_nation(&c, &NationDef::parse(NATION));
        c.tech = tech;
        let mut w = World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default()));
        w.shapes = nature_world().shapes;
        (w, pik)
    }

    /// A `Ui` whose library holds a game palette (colour `n` is the grey `n`).
    fn ui() -> Ui {
        let mut ui = Ui::new(&[GAME_PALETTE.to_owned()]);
        ui.ctx().lib.want(GAME_PALETTE);
        ui.requests();
        let bytes = (0..768).map(|i| (i / 3) as u8).collect();
        assert!(ui.on_loaded(GAME_PALETTE, Ok(bytes)));
        ui
    }

    fn view<'a>(
        names: &'a dyn Fn(TypeId) -> Vec<u8>,
        upgrade: &'a dyn Fn(acr_sim::tech::UpgradeId) -> Vec<u8>,
        sprite: &'a dyn Fn(acr_formats::m3d::SpriteGroup, u16) -> Option<(String, i32, i32)>,
    ) -> View<'a> {
        View {
            names,
            icon: &|_| None,
            upgrade_name: upgrade,
            sprite,
            texture_colour: &|t| Some([t, t, t]),
            title: b"Einheit".to_vec(),
        }
    }

    /// Runs `f` with the view of a world whose names are the type ids.
    fn with_view<R>(w: &World, f: impl FnOnce(&View) -> R) -> R {
        let names = |k: TypeId| w.catalog.get(k).id.as_bytes().to_vec();
        f(&view(&names, &|_| Vec::new(), &|_, _| None))
    }

    /// Every id the view sends.
    fn ids(v: &TouchEditor) -> Vec<String> {
        let mut out = vec![v.toggle.id.clone()];
        out.extend(v.swatches.iter().map(|s| s.id.clone()));
        out.extend(v.entries.iter().chain(&v.army).chain(&v.levels).map(|b| b.id.clone()));
        out.extend(v.groups.iter().flat_map(|g| &g.cells).map(|c| c.id.clone()));
        for c in &v.controls {
            match c {
                TouchControl::Stepper { less, more, .. } => out.extend([less.clone(), more.clone()]),
                TouchControl::Button { id, .. } | TouchControl::Icon { id, .. } => out.push(id.clone()),
                TouchControl::Line { .. } => {}
            }
        }
        out
    }

    fn built(e: &Editor, w: &World, ui: &Ui) -> TouchEditor {
        with_view(w, |v| e.touch_view(&ui.ctx(), w, v, 0)).expect("the palette is loaded")
    }

    fn accepted(e: &Editor, w: &World, ui: &Ui) {
        for id in ids(&built(e, w, ui)) {
            assert!(e.intent(w, &id, false).is_ok(), "{id}: {:?}", e.intent(w, &id, false));
        }
    }

    #[test]
    fn every_id_of_the_view_is_accepted() {
        let (w, pik) = world();
        let ui = ui();
        let mut e = Editor::default();
        accepted(&e, &w, &ui);
        // The level list, the army menu's lines and the other lists.
        e.open_levels(pik, Rect::new(0, 0, CELL, CELL));
        let v = built(&e, &w, &ui);
        assert_eq!(v.levels.len(), 4);
        accepted(&e, &w, &ui);
        for entry in [FIRST_EXTRA, NATUR, GELAENDE] {
            let e = Editor { entry, ..Editor::default() };
            let v = built(&e, &w, &ui);
            assert!(v.army.is_empty() && !v.groups.is_empty(), "entry {entry}");
            accepted(&e, &w, &ui);
        }
        let mut e = Editor::default();
        assert!(!built(&e, &w, &ui).army.is_empty(), "England has armies");
        e.apply(&w, Act::Battle(battle::Act::Toggle), 0);
        let v = built(&e, &w, &ui);
        assert!(v.battle && v.groups.is_empty() && v.swatches.is_empty());
        assert!(v.controls.iter().any(|c| matches!(c, TouchControl::Icon { id, .. } if id == "editor/battle/fight")));
        accepted(&e, &w, &ui);
        // Teams show their numbers and take a long press.
        let e = Editor { sides: sides::Mode::Teams, ..Editor::default() };
        let v = built(&e, &w, &ui);
        assert!(v.swatches[..7].iter().all(|s| s.teams && s.team.is_some()));
        assert!(!v.swatches[7].teams && v.swatches[7].team.is_none());
        assert_eq!(v.swatches[1].colour, [0xd4, 0xd4, 0xd4]);
    }

    #[test]
    fn without_the_palette_there_is_no_view() {
        let (w, _) = world();
        let ui = Ui::new(&[]);
        assert!(with_view(&w, |v| Editor::default().touch_view(&ui.ctx(), &w, v, 0)).is_none());
    }

    #[test]
    fn an_intent_does_what_the_classic_click_does() {
        let (w, pik) = world();
        let mut classic = Editor::default();
        let mut touch = Editor::default();
        let steps: [(&str, Act); 16] = [
            ("editor/nation/3", Act::Entry(3)),
            ("editor/nation/2", Act::Entry(2)),
            ("editor/owner/1", Act::Owner(1)),
            ("editor/unit/EngPik(EN)", Act::Pick(Pick::Unit(pik))),
            ("editor/count/+", Act::Count(1)),
            ("editor/count/+", Act::Count(1)),
            ("editor/count/-", Act::Count(-1)),
            ("editor/form/+", Act::Form(1)),
            ("editor/facing/+", Act::Facing(1)),
            ("editor/facing/+", Act::Facing(1)),
            ("editor/nation/14", Act::Entry(14)),
            ("editor/nature/ST/2", Act::Pick(Pick::Nature(acr_formats::m3d::SpriteGroup::Stones, 2))),
            ("editor/nation/15", Act::Entry(15)),
            ("editor/brush/-", Act::Brush(-1)),
            ("editor/ground/wasser", Act::Pick(Pick::Ground(GroundTool::Water))),
            ("editor/ground/wasser", Act::Pick(Pick::Ground(GroundTool::Water))),
        ];
        for (id, act) in steps {
            classic.apply(&w, act, 0);
            let Ok(Target::Act(sent)) = touch.intent(&w, id, false) else { panic!("{id}") };
            assert_eq!(sent, act, "{id}");
            touch.apply(&w, sent, 0);
            let state = |e: &Editor| (e.count(), e.owner(), e.facing(), e.pick, e.entry, e.level, e.form, e.radius());
            assert_eq!(state(&touch), state(&classic), "{id}");
        }
        assert_eq!(touch.pick, None, "the second tap on the cell lifts the pick");
        // The stop and the close act like the right click that ends placing.
        touch.apply(&w, Act::Pick(Pick::Unit(pik)), 0);
        assert_eq!(touch.intent(&w, "editor/stop", false), Ok(Target::Act(Act::Stop)));
        touch.apply(&w, Act::Stop, 0);
        assert_eq!(touch.pick, None);
        touch.apply(&w, Act::Entry(2), 0);
        touch.apply(&w, Act::Army(presets::Act::Size(presets::Size::Small)), 0);
        assert_eq!(
            touch.intent(&w, "editor/army/size/0", false),
            Ok(Target::Act(Act::Army(presets::Act::Size(presets::Size::Small))))
        );
        assert!(touch.army.armed.is_some());
        touch.apply(&w, Act::Stop, 0);
        assert!(touch.army.armed.is_none());
    }

    #[test]
    fn refusals_say_why() {
        let (w, pik) = world();
        let mut e = Editor::default();
        let no = |e: &Editor, id: &str, right: bool| e.intent(&w, id, right).expect_err(id);
        let before = format!("{e:?}");
        assert_eq!(no(&e, "editor/nation/16", false), Reject::Range);
        assert_eq!(no(&e, "editor/nation/x", false), Reject::NoWidget);
        assert_eq!(no(&e, "editor/nope", false), Reject::NoWidget);
        assert_eq!(no(&e, "hud/0", false), Reject::NoWidget);
        assert_eq!(no(&e, "editor/owner/8", false), Reject::Range);
        assert_eq!(no(&e, "editor/owner/1", true), Reject::Disabled, "no teams: no team");
        assert_eq!(no(&e, "editor/unit/Nope(EN)", false), Reject::NoWidget);
        assert_eq!(no(&e, "editor/unit/EngPik(FR)", false), Reject::NoWidget);
        assert_eq!(no(&e, "editor/level/1", false), Reject::Hidden, "no menu");
        assert_eq!(no(&e, "editor/close", false), Reject::Hidden);
        assert_eq!(no(&e, "editor/stop", false), Reject::Disabled);
        assert_eq!(no(&e, "editor/brush/+", false), Reject::Hidden, "only on Gelaende");
        assert_eq!(no(&e, "editor/army/era/9", false), Reject::Range);
        assert_eq!(no(&e, "editor/army/size/3", false), Reject::Range);
        assert_eq!(no(&e, "editor/count/+", true), Reject::Kind("a left-click target"));
        assert_eq!(no(&e, "editor/battle/go", false), Reject::Hidden, "not in the battle view");
        e.open_levels(pik, Rect::new(0, 0, CELL, CELL));
        assert_eq!(no(&e, "editor/level/4", false), Reject::Range);
        assert_eq!(
            format!("{e:?}"),
            before.replace("menu: None", &format!("menu: {:?}", e.menu)),
            "only the menu we opened"
        );
        let mut e = Editor { entry: NATUR, ..Editor::default() };
        assert_eq!(no(&e, "editor/nature/ST/2", true), Reject::Kind("a unit"));
        assert_eq!(no(&e, "editor/nature/ST/9", false), Reject::NoWidget);
        e.entry = FIRST_EXTRA;
        assert_eq!(no(&e, "editor/army/size/0", false), Reject::Hidden);
        // Teams: the neutral colour has no team.
        let e = Editor { sides: sides::Mode::Teams, ..Editor::default() };
        assert_eq!(no(&e, "editor/owner/7", true), Reject::Disabled);
        assert_eq!(e.intent(&w, "editor/owner/2", true), Ok(Target::Act(Act::Team(2))));
        // The battle view hides the list; a nation with one army has no century to step.
        let mut e = Editor::default();
        e.apply(&w, Act::Battle(battle::Act::Toggle), 0);
        assert_eq!(no(&e, "editor/count/+", false), Reject::Hidden);
        assert_eq!(no(&e, "editor/nation/3", false), Reject::Hidden);
        assert_eq!(no(&e, "editor/battle/nation/2/+", false), Reject::NoWidget);
        assert_eq!(e.intent(&w, "editor/battle", false), Ok(Target::Act(Act::Battle(battle::Act::Toggle))));
        e.battle_act(battle::Act::Nation(0, 6));
        assert!(presets::eras(8).len() < 2);
        assert_eq!(no(&e, "editor/battle/era/0/+", false), Reject::Hidden);
        assert!(e.intent(&w, "editor/battle/era/1/+", false).is_ok());
    }

    #[test]
    fn a_right_click_on_a_unit_opens_its_levels_and_a_level_picks_it() {
        let (w, pik) = world();
        let mut e = Editor::default();
        let Ok(Target::Levels(kind)) = e.intent(&w, "editor/unit/EngPik(EN)", true) else { panic!() };
        assert_eq!(kind, pik);
        e.open_levels(kind, Rect::new(0, 0, CELL, CELL));
        assert!(e.menu_open());
        assert_eq!(e.intent(&w, "editor/close", false), Ok(Target::Act(Act::Close)));
        let Ok(Target::Act(act)) = e.intent(&w, "editor/level/2", false) else { panic!() };
        assert_eq!(act, Act::SetLevel(2));
        e.apply(&w, act, 0);
        assert_eq!((e.pick, e.level, e.menu_open()), (Some(Pick::Unit(pik)), 2, false));
        // A close drops the menu and nothing else.
        e.open_levels(pik, Rect::new(0, 0, CELL, CELL));
        e.apply(&w, Act::Close, 0);
        assert_eq!((e.pick, e.level, e.menu_open()), (Some(Pick::Unit(pik)), 2, false));
    }

    #[test]
    fn the_view_lists_every_cell_whatever_the_scroll() {
        let (w, _) = world();
        let ui = ui();
        let e = Editor { scroll: 100_000, ..Editor::default() };
        let total: usize = groups(&w, e.entry).iter().map(|g| g.kinds.len()).sum();
        assert_eq!(total, 81);
        let shown = built(&e, &w, &ui).groups.iter().map(|g| g.cells.len()).sum::<usize>();
        assert_eq!(shown, total);
        let names = |k: TypeId| w.catalog.get(k).id.as_bytes().to_vec();
        let classic = e
            .layout(&w, &view(&names, &|_| Vec::new(), &|_, _| None), 0)
            .iter()
            .filter(|i| matches!(i.act, Some(Act::Pick(_))) && !i.hit_rect(e.frame.content()).is_empty())
            .count();
        assert!(classic < total, "the classic window clips to its scroll: {classic}");
        let cell = &built(&e, &w, &ui).groups[0].cells[0];
        assert!(!cell.draws.is_empty());
        assert!(cell.levels && cell.name == b"EngPik(EN)");
    }
}
