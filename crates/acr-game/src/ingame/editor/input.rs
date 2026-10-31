//! The editor's inputs (#427). Every click that changes the world, not only the window, is an
//! [`Input`]: the game applies it between two ticks ([`apply`]) and logs it with its tick
//! ([`crate::savegame::Log::editor`]), so a savegame replays it at the same tick. They are not
//! `acr_sim::Command`s (the original's command set, and the ACRR format, stay as they are).
//!
//! An input names what it needs by value (a type by its id, a formation by its index in
//! [`formations`]), never by the window's state: replaying it needs no window.

use acr_formats::M3dMap;
use acr_sim::World;
use acr_sim::fixed::Pos;

pub(crate) use super::presets::{Era, Size};
pub(crate) use super::sides::Mode;
use super::{Editor, formations, grid, levels, place_formations, reload_guns, sides, world as placing};

/// A change the editor makes to the game.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    /// `count` units of type `kind` for `owner` around `at`; `form` is 0 for loose, else
    /// 1 + the index in [`formations`]; `level` is the training level the owner's types get.
    Place { kind: String, owner: u8, at: (i32, i32), count: u32, form: u32, dir: u8, level: u32 },
    /// The Kampf rule and the team numbers.
    Sides { mode: Mode, teams: [u8; 8] },
    /// Every cannon is loaded once.
    ReloadGuns,
    /// Cannons load at once, every tick, or normally.
    AutoReload(bool),
    /// The army of `nation` (an index in the combo box) of `size` and `era` centred on `at`.
    Preset { nation: usize, size: Size, era: Option<Era>, owner: u8, at: (i32, i32), dir: u8 },
    /// The battle view's "Aufstellen": the nations and centuries of both sides, the army size
    /// and "Kampf sofort". Its marches are commands and sit in the command log.
    Battle { nation: [usize; 2], era: [Option<Era>; 2], size: Size, fight: bool },
}

/// What applying an input gave.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applied {
    /// The line under the window.
    pub status: Vec<u8>,
}

/// Applies `input` to the world (and the editor's own state: the Kampf rule, the battle
/// view's placed units). `map` is the map the world was made from, for the inputs that
/// change it (none yet).
pub fn apply(world: &mut World, _map: &mut M3dMap, editor: &mut Editor, input: &Input) -> Applied {
    let status = match input {
        Input::Place { kind, owner, at, count, form, dir, level } => {
            place(world, editor, kind, *owner, *at, (*count as usize, *form as usize, *dir, *level as usize))
        }
        Input::Sides { mode, teams } => {
            editor.sides = *mode;
            editor.team = *teams;
            sides::apply(world, *mode, teams);
            match mode {
                Mode::Off => b"Kampf aus".to_vec(),
                Mode::Free => b"Kampf an: alle gegen alle".to_vec(),
                Mode::Teams => b"Kampf an: Teams".to_vec(),
            }
        }
        Input::ReloadGuns => format!("{} Kanonen geladen", reload_guns(world)).into_bytes(),
        Input::AutoReload(on) => {
            editor.auto_reload = *on;
            if *on { b"Kanonen laden sofort nach".to_vec() } else { b"Kanonen laden normal".to_vec() }
        }
        Input::Preset { nation, size, era, owner, at, dir } => {
            editor.place_army(world, (*nation, *size, *era), *owner, *at, *dir)
        }
        Input::Battle { nation, era, size, fight } => editor.go_to_battle(world, *nation, *era, *size, *fight),
    };
    Applied { status }
}

/// Puts units or a building on the map; the text for the status line says why not.
fn place(
    world: &mut World,
    editor: &Editor,
    kind: &str,
    owner: u8,
    at: (i32, i32),
    (n, form, dir, level): (usize, usize, u8, usize),
) -> Vec<u8> {
    let Some(kind) = world.catalog.find(kind) else { return b"Unbekannter Typ".to_vec() };
    if let Some(why) = placing::refuse(world, kind, at.0, at.1) {
        return why.as_bytes().to_vec();
    }
    if world.catalog.get(kind).building {
        return match placing::place_building(world, kind, owner, at) {
            Ok(name) => format!("{name}, fertig"),
            Err(e) => e,
        }
        .into_bytes();
    }
    let choice = match form {
        0 => None,
        f => match formations(world, kind).into_iter().nth(f - 1) {
            Some(c) => Some(c),
            None => return b"keine Formation".to_vec(),
        },
    };
    // The level holds for the owner's every object of the placed types, as researched
    // upgrades do: set before spawning, so the new men start at full upgraded life.
    let mut kinds = vec![kind];
    if let Some(c) = &choice {
        let o = world.formations.officers.get(&c.officer).cloned().unwrap_or_default();
        kinds.extend([Some(c.officer), o.drummer, o.flag].into_iter().flatten());
    }
    let ups = levels::steps(world, kind);
    let level = levels::apply(world, owner, kind, level);
    // The officer, drummer and flag bearer stand at the same level, as far as they have it.
    for &k in kinds.iter().skip(1) {
        levels::apply(world, owner, k, level);
    }
    let placed = match choice {
        Some(choice) => place_formations(world, kind, &choice, n, owner, at, dir),
        None => {
            let t = world.catalog.get(kind);
            let gap = (t.radius2 * 4).max(32);
            for p in grid(n, at, dir, gap) {
                world.spawn(kind, owner, Pos::from_pixels(p.0, p.1), dir);
            }
            Ok(n)
        }
    };
    editor.apply_sides(world);
    let name = world.catalog.get(kind).id.clone();
    match placed {
        Ok(k) => format!("{k} x {name}, Ausbau {level}/{ups}").into_bytes(),
        Err(e) => e.into_bytes(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingame::editor::presets::tests::world;

    const KIND: &str = "EngM18(EN)";

    fn count(world: &World, owner: u8) -> usize {
        world.units.iter().filter(|u| u.alive && u.owner == owner).count()
    }

    #[test]
    #[ignore = "needs the original game files (ACR_GAME_DIR)"]
    fn a_placement_by_type_id_puts_the_units_and_a_bad_one_nothing() {
        let mut w = world();
        let mut editor = Editor::default();
        let mut map = M3dMap::default();
        let place = |kind: &str, form| Input::Place {
            kind: kind.into(),
            owner: 2,
            at: (900, 900),
            count: 5,
            form,
            dir: 0,
            level: 0,
        };
        apply(&mut w, &mut map, &mut editor, &place(KIND, 0));
        assert_eq!(count(&w, 2), 5);
        assert_eq!(apply(&mut w, &mut map, &mut editor, &place("Nope", 0)).status, b"Unbekannter Typ");
        assert_eq!(apply(&mut w, &mut map, &mut editor, &place(KIND, 99)).status, b"keine Formation");
        assert_eq!(count(&w, 2), 5);
    }

    #[test]
    #[ignore = "needs the original game files (ACR_GAME_DIR)"]
    fn sides_and_auto_reload_set_the_editors_state() {
        let mut w = world();
        let mut editor = Editor::default();
        let mut map = M3dMap::default();
        let teams = [1, 1, 2, 2, 3, 3, 4, 4];
        apply(&mut w, &mut map, &mut editor, &Input::Sides { mode: Mode::Teams, teams });
        assert_eq!(editor.sides, Mode::Teams);
        assert_eq!(w.nation_mask, sides::masks(Mode::Teams, &teams));
        apply(&mut w, &mut map, &mut editor, &Input::AutoReload(true));
        assert!(editor.auto_reload);
    }
}
