//! The editor's inputs (#427). Every click that changes the world, not only the window, is an
//! [`Input`]: the game applies it between two ticks ([`apply`]) and logs it with its tick
//! ([`crate::savegame::Log::editor`]), so a savegame replays it at the same tick. They are not
//! `acr_sim::Command`s (the original's command set, and the ACRR format, stay as they are).
//!
//! An input names what it needs by value (a type by its id, a formation by its index in
//! [`formations`]), never by the window's state: replaying it needs no window.

use acr_formats::M3dMap;
use acr_formats::m3d::{Sprite, SpriteGroup};
use acr_mapgen::edit::{self, Dab, EditError, Tool};
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
    /// One stamp of the terrain brush ([`acr_mapgen::edit`]); a stroke is a run of them.
    Ground(Dab),
}

/// What applying an input gave.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applied {
    /// The line under the window.
    pub status: Vec<u8>,
    /// The map got an object: the scene is built again (and the minimap, unless a tree).
    pub objects: bool,
    /// The terrain changed in this box of world pixels `[x0, y0, x1, y1)`: the game's render
    /// data, the GPU copy, the minimap and the scene follow.
    pub ground: Option<[i32; 4]>,
}

/// Applies `input` to the world (and the editor's own state: the Kampf rule, the battle
/// view's placed units). `map` is the map the world was made from, for the inputs that
/// change it.
pub fn apply(world: &mut World, map: &mut M3dMap, editor: &mut Editor, input: &Input) -> Applied {
    let mut objects = false;
    let mut ground = None;
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
        Input::Ground(dab) => match edit::apply(map, dab) {
            Ok(area) => {
                if let Some(area) = area {
                    world.sync_terrain(map, area);
                }
                ground = area;
                tool_name(dab.tool).as_bytes().to_vec()
            }
            Err(e) => match e {
                EditError::NoTerrain => "Karte ohne Gelaende",
                EditError::NoSea => "Karte ohne Wasser",
                EditError::Areas => "Karte mit Wegegebieten: nicht editierbar",
                EditError::Radius => "Pinsel ungueltig",
                EditError::Texture => "Unbekannte Textur",
            }
            .as_bytes()
            .to_vec(),
        },
    };
    Applied { status, objects, ground }
}

/// The status line's word for a brush tool.
fn tool_name(tool: Tool) -> &'static str {
    match tool {
        Tool::Raise(_) => "Angehoben",
        Tool::Lower(_) => "Abgesenkt",
        Tool::Flatten(_) => "Geebnet",
        Tool::Texture(_) => "Boden gemalt",
        Tool::Water => "Wasser gemalt",
        Tool::Land => "Land gemalt",
    }
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
        Some(choice) => place_formations(world, kind, &choice, n, owner, at, dir).map(|(placed, _)| placed),
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

    /// A flat map of size shift 1 with sea cells, and the world made from it.
    fn flat_world() -> (World, M3dMap) {
        use acr_formats::m3d::{Grid, MapSize, Water};
        let size = MapSize { shift: 1 };
        let (vw, vh) = (size.vertex_width(), size.vertex_height());
        let n = (240usize << 1) + 2;
        let map = M3dMap {
            size: Some(size),
            header: [vw as u32, vh as u32],
            heights: Some(Grid { width: vw, height: vh, cells: vec![0; vw * vh] }),
            textures: Some(Grid { width: vw, height: vh, cells: vec![0; vw * vh] }),
            water: Some(Water {
                depth: Grid { width: n, height: n, cells: vec![0; n * n] },
                brightness: Grid { width: n, height: n, cells: vec![0; n * n] },
            }),
            ..M3dMap::default()
        };
        let mut w = crate::ingame::editor::tests::nature_world();
        w.map = std::sync::Arc::new(acr_sim::MapGrid::from_m3d(&map, &w.shapes));
        (w, map)
    }

    #[test]
    fn a_ground_input_edits_the_map_and_the_worlds_copy() {
        let (mut w, mut map) = flat_world();
        let mut editor = Editor::default();
        let dab = |x, y, tool| Input::Ground(Dab { x, y, radius: 128, tool });

        let raised = apply(&mut w, &mut map, &mut editor, &dab(3200, 3200, Tool::Raise(edit::RAISE)));
        let area = raised.ground.expect("the dab changed the ground");
        assert!(area[0] <= 3200 - 128 && area[2] >= 3200 + 128);
        assert!(w.map.heights.at(3200, 3200) > 0, "the world sees the hill");
        assert_eq!(raised.status, b"Angehoben");

        let flooded = apply(&mut w, &mut map, &mut editor, &dab(6400, 6400, Tool::Water));
        assert!(flooded.ground.is_some());
        assert!(w.map.land.check_pt(6400 >> 4, 6400 >> 4), "land is locked on the water");
        assert!(!w.map.water.check_pt(6400 >> 4, 6400 >> 4), "ships float there");
        let dried = apply(&mut w, &mut map, &mut editor, &dab(6400, 6400, Tool::Land));
        assert!(dried.ground.is_some());
        assert!(!w.map.land.check_pt(6400 >> 4, 6400 >> 4));

        // A bad brush is refused with a text and changes nothing.
        let before = map.heights.clone();
        let bad = apply(
            &mut w,
            &mut map,
            &mut editor,
            &Input::Ground(Dab { x: 3200, y: 3200, radius: 7, tool: Tool::Raise(8) }),
        );
        assert_eq!((bad.ground, bad.status), (None, b"Pinsel ungueltig".to_vec()));
        assert_eq!(map.heights, before);
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
