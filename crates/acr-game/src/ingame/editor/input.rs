//! The editor's inputs (#427). Every click that changes the world, not only the window, is an
//! [`Input`]: the game applies it between two ticks ([`apply`]) and logs it with its tick
//! ([`crate::savegame::Log::editor`]), so a savegame replays it at the same tick. They are not
//! `acr_sim::Command`s (the original's command set, and the ACRR format, stay as they are).
//!
//! An input names what it needs by value (a type by its id, a formation by its index in
//! [`formations`]), never by the window's state: replaying it needs no window.

use acr_formats::M3dMap;
use acr_formats::m3d::{Sprite, SpriteGroup};
use acr_sim::World;
use acr_sim::fixed::Pos;
use acr_sim::world::ObjectRefusal;

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
    /// A tree or stone of the Natur list: type `index` of the group's sprite list at `at`.
    Nature { group: SpriteGroup, index: u16, at: (i32, i32) },
}

/// What applying an input gave.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applied {
    /// The line under the window.
    pub status: Vec<u8>,
    /// The map got an object: the scene is built again (and the minimap, unless a tree).
    pub objects: bool,
}

/// Applies `input` to the world (and the editor's own state: the Kampf rule, the battle
/// view's placed units). `map` is the map the world was made from, for the inputs that
/// change it.
pub fn apply(world: &mut World, map: &mut M3dMap, editor: &mut Editor, input: &Input) -> Applied {
    let mut objects = false;
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
        Input::Nature { group, index, at } => {
            let (status, added) = nature(world, map, *group, *index, *at);
            objects = added;
            status
        }
    };
    Applied { status, objects }
}

/// Puts a tree or stone on the map and into the world; the text for the status line says
/// why not. A deposit next to a mine feeds it from the next `HandleMines` (every 256 ticks,
/// `economy::bind_mines`).
fn nature(world: &mut World, map: &mut M3dMap, group: SpriteGroup, index: u16, (x, y): (i32, i32)) -> (Vec<u8>, bool) {
    let why = match world.object_refusal(group, index, x, y) {
        None => {
            let sprite = Sprite { group, x, y, index, extra: None };
            map.sprites.push(sprite.clone());
            world.add_map_object(map, &sprite);
            let name = world.shapes.kinds.get(&group).and_then(|k| k.names.get(usize::from(index)));
            return (format!("{} gesetzt", name.map_or("Objekt", String::as_str)).into_bytes(), true);
        }
        Some(ObjectRefusal::UnknownType) => "Unbekanntes Objekt",
        Some(ObjectRefusal::OffMap) => "Ausserhalb der Karte",
        Some(ObjectRefusal::Water) => "Nicht im tiefen Wasser",
    };
    (why.as_bytes().to_vec(), false)
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
    fn a_nature_input_puts_a_tree_and_a_stone_and_refuses_deep_water() {
        use acr_formats::m3d::Grid;
        let mut w = crate::ingame::editor::tests::nature_world();
        // The sea west of x = 1600 (32 px cells).
        let side = 512 << 1;
        let mut cells = vec![0u8; side * side];
        for row in cells.chunks_mut(side) {
            row[..50].fill(200);
        }
        std::sync::Arc::make_mut(&mut w.map).water_depth = Some(Grid { width: side, height: side, cells });
        let mut editor = Editor::default();
        let mut map = M3dMap::default();
        let nature = |group, index, at| Input::Nature { group, index, at };

        let tree = apply(&mut w, &mut map, &mut editor, &nature(SpriteGroup::Trees, 0, (3000, 3000)));
        assert!(tree.objects);
        assert_eq!(tree.status, b"AG1 gesetzt");
        let stone = apply(&mut w, &mut map, &mut editor, &nature(SpriteGroup::Stones, 2, (3200, 3000)));
        assert!(stone.objects);
        assert_eq!((map.sprites.len(), w.objects.len()), (2, 2));
        assert_eq!(map.sprites[1].group, SpriteGroup::Stones);
        assert_eq!(w.deposits.len(), 1, "a deposit of the Vorkommen list");
        assert!(w.sources.nearest(3000, 3000).is_some(), "a tree is a wood source");

        for (input, text) in [
            (nature(SpriteGroup::Trees, 0, (800, 3000)), &b"Nicht im tiefen Wasser"[..]),
            (nature(SpriteGroup::Trees, 0, (-5, 3000)), b"Ausserhalb der Karte"),
            (nature(SpriteGroup::Trees, 9, (3000, 3000)), b"Unbekanntes Objekt"),
            (nature(SpriteGroup::Complex, 0, (3000, 3000)), b"Unbekanntes Objekt"),
        ] {
            let refused = apply(&mut w, &mut map, &mut editor, &input);
            assert!(!refused.objects);
            assert_eq!(refused.status, text);
        }
        assert_eq!((map.sprites.len(), w.objects.len()), (2, 2), "a refusal changes nothing");
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
