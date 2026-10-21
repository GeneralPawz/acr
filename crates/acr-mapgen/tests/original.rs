//! Regenerates the random maps the original saved in the game folder (`RN* *.m3d`) and
//! compares them chunk by chunk. Needs the game: `ACR_GAME_DIR=... cargo test -p acr-mapgen
//! -- --ignored`.

use acr_formats::{M3dMap, Vfs};
use acr_mapgen::{Settings, generate};

#[test]
#[ignore]
fn reproduces_the_saved_random_maps() {
    let dir = std::env::var("ACR_GAME_DIR").expect("ACR_GAME_DIR");
    let mut vfs = Vfs::open_game_dir(std::path::Path::new(&dir)).unwrap();
    let names: Vec<String> = vfs
        .list()
        .map(|(p, _)| p.to_owned())
        .filter(|p| p.to_ascii_uppercase().starts_with("RN") && p.to_ascii_uppercase().ends_with(".M3D"))
        .collect();
    assert!(!names.is_empty());
    for name in names {
        let original = M3dMap::parse(&vfs.read(&name).unwrap()).unwrap();
        let settings = Settings::from_name(&name).unwrap();
        let ours = generate(&settings, &mut |p: &str| vfs.read(p).ok()).unwrap().map;
        assert_eq!(ours.heights, original.heights, "{name}: SURF");
        assert_eq!(ours.textures, original.textures, "{name}: TILE");
        assert_eq!(ours.water, original.water, "{name}: SEA2");
        assert_eq!(ours.sprites.len(), original.sprites.len(), "{name}: TRE1");
        for (a, b) in ours.sprites.iter().zip(&original.sprites) {
            assert_eq!((a.group, a.x, a.y, a.index), (b.group, b.x, b.y, b.index), "{name}: TRE1");
        }
        let unit = |u: &acr_formats::m3d::Unit| (u.nation, u.x, u.y, u.name.clone());
        assert!(ours.units.iter().map(unit).eq(original.units.iter().map(unit)), "{name}: UNI3");
        assert_eq!(ours.locks, original.locks, "{name}: LOC1");
        assert_eq!(ours.resources, original.resources, "{name}: RESU");
        let (p, q) = (ours.peace.unwrap(), original.peace.unwrap());
        assert_eq!((p.cells, p.time, p.players, p.flags), (q.cells, q.time, q.players, q.flags), "{name}: CAEP");
    }
}
