//! Trees and stones put on a running map (#429), and terrain edits taken into the world
//! (#430): what a loaded map derives from a sprite, added incrementally.

use std::sync::Arc;

use acr_formats::defs::UnitDef;
use acr_formats::m3d::{Grid, M3dMap, MapSize, Sprite, SpriteGroup, Water};
use acr_sim::catalog::{Catalog, ObjectType, TypeId};
use acr_sim::formation::Formations;
use acr_sim::map::{LockMap, SpriteLock, SpriteShapes};
use acr_sim::sprite::{ObjChar, SpriteKinds};
use acr_sim::{MapGrid, ObjectRefusal, World, construction};

const TREE: u16 = 0;
const WOOD: u8 = 0;

const CENTRE: &str = "NAME Cen\r\nBUILDING\r\nLIFE 1000\r\nLOCATION 0 0 64 64\r\nBUILDSTAGES 10\r\n\
    LOCKPOINTS 9 0 0 1 0 2 0 0 1 1 1 2 1 0 2 1 2 2 2\r\nBUILDLOCKPOINTS 9 0 0 1 0 2 0 0 1 1 1 2 1 0 2 1 2 2 2\r\n\
    CHECKPOINTS 9 0 0 1 0 2 0 0 1 1 1 2 1 0 2 1 2 2 2\r\n[END]\r\n";

fn kind() -> ObjChar {
    ObjChar {
        res_type: None,
        per_work: 0,
        radius: 32,
        work_amount: 0,
        work_next: None,
        time_amount: 0,
        time_next: None,
        deposit: false,
        list_radius: 0,
    }
}

/// A tree (wood, 4 per work within 30 px) and a rock (a polygon), as both lists would hold.
fn shapes() -> SpriteShapes {
    let mut s = SpriteShapes::default();
    let names = ["AG1".to_string(), "ST1".to_string()];
    let kinds = |n: &[String]| SpriteKinds { chars: n.iter().map(|_| kind()).collect(), names: n.to_vec() };
    s.kinds.insert(SpriteGroup::Trees, kinds(&names[..1]));
    s.kinds.insert(SpriteGroup::Stones, kinds(&names[1..]));
    s.work.insert(SpriteGroup::Trees, vec![Some((WOOD, 4, 30))]);
    let square = SpriteLock { dx: 0, dy: 0, polygon: vec![(0, 0), (96, 0), (96, 64), (0, 64)], source: false };
    s.groups.insert(SpriteGroup::Stones, vec![Some(square)]);
    s
}

fn tree(x: i32, y: i32) -> Sprite {
    Sprite { group: SpriteGroup::Trees, x, y, index: TREE, extra: None }
}

fn rock(x: i32, y: i32) -> Sprite {
    Sprite { group: SpriteGroup::Stones, x, y, index: 0, extra: None }
}

/// A flat map with a lake in its north-west corner.
fn map() -> M3dMap {
    let size = MapSize { shift: 1 };
    let (w, h) = (size.vertex_width(), size.vertex_height());
    let heights = Grid { width: w, height: h, cells: vec![200i16; w * h] };
    let side = (size.playable_size() / 32) as usize + 2;
    let mut depth = Grid { width: side, height: side, cells: vec![0u8; side * side] };
    for y in 10..30 {
        for x in 10..30 {
            depth.cells[y * side + x] = 200;
        }
    }
    let brightness = Grid { width: side, height: side, cells: vec![0u8; side * side] };
    M3dMap { size: Some(size), heights: Some(heights), water: Some(Water { depth, brightness }), ..M3dMap::default() }
}

fn world_of(map: &M3dMap) -> World {
    World::from_map(map, &shapes(), Arc::new(Catalog::new()), Arc::new(Formations::default())).0
}

fn bits(layer: &LockMap) -> Vec<bool> {
    (0..layer.height).flat_map(|y| (0..layer.width).map(move |x| layer.check_pt(x, y))).collect()
}

#[test]
fn added_rock_locks_like_a_loaded_one() {
    let mut with = map();
    with.sprites.push(rock(6000, 6000));
    let loaded = world_of(&with);

    let mut edited = map();
    let mut w = world_of(&edited);
    let before = bits(&w.map.land);
    edited.sprites.push(rock(6000, 6000));
    let i = w.add_map_object(&edited, &edited.sprites[0]);

    assert_eq!(i, 0);
    assert_eq!(w.objects, loaded.objects);
    assert!(bits(&w.map.land) != before, "the rock locks cells");
    assert!(bits(&w.map.land) == bits(&loaded.map.land));
    assert!(bits(&w.map.water) == bits(&loaded.map.water));
    assert_eq!(*w.spacing, *loaded.spacing);
}

#[test]
fn added_tree_is_a_wood_source() {
    let mut edited = map();
    let mut w = world_of(&edited);
    assert_eq!(w.sources.nearest(4000, 4000), None);
    let before = bits(&w.map.land);
    edited.sprites.push(tree(4000, 4000));
    w.add_map_object(&edited, &edited.sprites[0]);

    let mut with = map();
    with.sprites.push(tree(4000, 4000));
    let loaded = world_of(&with);
    assert_eq!(w.sources.list, loaded.sources.list);
    assert_eq!(w.sources.nearest(4010, 4000), Some((10, WOOD)));
    assert_eq!(w.sources.nearest(4000, 4500), None, "only the 3 x 3 cells around count");
    assert!(bits(&w.map.land) == before, "a tree does not block");
}

#[test]
fn objects_refused_on_deep_water_and_off_map() {
    let w = world_of(&map());
    let at = |g, i, x, y| w.object_refusal(g, i, x, y);
    assert_eq!(at(SpriteGroup::Trees, TREE, 4000, 4000), None);
    assert_eq!(at(SpriteGroup::Stones, 0, 4000, 4000), None);
    assert_eq!(at(SpriteGroup::Trees, TREE, 500, 500), Some(ObjectRefusal::Water));
    assert_eq!(at(SpriteGroup::Trees, TREE, 0, 4000), Some(ObjectRefusal::OffMap));
    assert_eq!(at(SpriteGroup::Trees, TREE, 4000, -5), Some(ObjectRefusal::OffMap));
    assert_eq!(at(SpriteGroup::Trees, TREE, 480 * 32 * 2, 4000), Some(ObjectRefusal::OffMap));
    assert_eq!(at(SpriteGroup::Trees, 7, 4000, 4000), Some(ObjectRefusal::UnknownType));
    assert_eq!(at(SpriteGroup::Complex, 0, 4000, 4000), Some(ObjectRefusal::UnknownType));
}

fn with_centre(w: &mut World) -> TypeId {
    let mut c = Catalog::new();
    let cen = c.add(ObjectType::from_def("Cen(EN)", &UnitDef::parse(CENTRE.as_bytes())));
    w.catalog = Arc::new(c);
    cen
}

#[test]
fn sync_terrain_keeps_building_locks() {
    let mut edited = map();
    edited.sprites.push(rock(9000, 9000));
    let mut w = world_of(&edited);
    let cen = with_centre(&mut w);
    let b = construction::place_free(&mut w, 0, cen, 2048, 2048, None).expect("free ground").0 as usize;
    let cells: Vec<_> = w.catalog.get(cen).lock_cells(w.units[b].pos).collect();
    let with_building = bits(&w.map.land);

    // Raise the ground around the building and drown a patch elsewhere.
    let heights = edited.heights.as_mut().unwrap();
    let stride = heights.width;
    for row in 50..80 {
        for col in 50..80 {
            heights.cells[row * stride + col] = 205;
        }
    }
    let water = edited.water.as_mut().unwrap();
    let side = water.depth.width;
    for y in 98..103 {
        for x in 98..103 {
            water.depth.cells[y * side + x] = 220;
        }
    }
    w.sync_terrain(&edited, [1500, 1500, 2600, 2600]);
    assert!(cells.iter().all(|&(x, y)| w.map.land.check_pt(x, y)), "the building's cells stay locked");
    assert!(bits(&w.map.land) == with_building, "nothing else changed there");

    w.sync_terrain(&edited, [3000, 3000, 3400, 3400]);
    assert!(bits(&w.map.land) != with_building, "deep water locks land");
    assert!(cells.iter().all(|&(x, y)| w.map.land.check_pt(x, y)));
    assert_eq!(w.map.water_depth.as_ref().unwrap().get(100, 100), Some(220));
    let full = MapGrid::from_m3d(&edited, &shapes());
    assert!(bits(&w.map.water) == bits(&full.water));
}
