//! Generating a random map in the game (#74): the menu's choices become
//! [`acr_mapgen::Settings`]; the generator's files arrive asynchronously, so it is run
//! until it no longer asks for a file it has not seen (a few rounds: lists, then pieces).

use std::collections::{HashMap, HashSet};

use acr_formats::{M3dMap, normalize_path};
use acr_mapgen::Settings;

use crate::ui::GameSetup;
use crate::ui::setup::{MapSize, RandomMapSetup};

/// `TEXT\NATIONS.TXT` order (the menu's nation list) to `AI\AI.DAT` order (the
/// generator's nation digits): MAY SPAIN ENGLAND FRANCE USA INCAS ACTEC PUEBLO DELAVAR
/// HURONI IROKEZ SIU -> SPAIN MAY USA DELAVAR ENGLAND IROKEZ INCAS HURONI FRANCE ACTEC
/// PUEBLO SIU.
const AI_NATION: [u8; 12] = [1, 0, 4, 8, 2, 6, 9, 10, 3, 7, 5, 11];

/// Game options of the map name: start units of `Data\Start2.dat`, 20 minutes peace
/// (digits as in the original's saved random maps).
const OPTIONS: u32 = 5_002_012;

pub fn settings(map: &RandomMapSetup, setup: &GameSetup) -> Settings {
    let mut nations = [None; 8];
    for p in &setup.players {
        if let Some(slot) = nations.get_mut(usize::from(p.color)) {
            *slot = AI_NATION.get(p.nation).copied();
        }
    }
    let byte = |v: usize| v.min(15) as u8;
    Settings {
        seed: map.seed,
        size: match map.size {
            MapSize::Normal => 1,
            MapSize::Large => 2,
            MapSize::Huge => 3,
        },
        landscape: byte(map.landscape),
        shape: byte(setup.players.len().clamp(2, 7) - 2),
        relief: byte(map.ground),
        resources: byte(map.resources),
        minerals: byte(map.minerals),
        climate: byte(map.climate),
        nations,
        options: OPTIONS,
    }
}

pub struct RandomMapGen {
    settings: Settings,
    /// Files by normalized path; `None` for files that do not exist.
    files: HashMap<String, Option<Vec<u8>>>,
    pending: HashSet<String>,
}

impl RandomMapGen {
    pub fn new(settings: Settings) -> Self {
        Self { settings, files: HashMap::new(), pending: HashSet::new() }
    }

    /// Takes a delivered file if the generator asked for it; returns true then.
    pub fn on_loaded(&mut self, path: &str, result: &Result<Vec<u8>, String>) -> bool {
        let key = normalize_path(path);
        if !self.pending.remove(&key) {
            return false;
        }
        self.files.insert(key, result.as_ref().ok().cloned());
        true
    }

    /// True while requested files are outstanding.
    pub fn waiting(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Runs the generator: `Ok(Some(map))` when done, `Ok(None)` with new requests in
    /// `requests` while files are missing.
    pub fn try_generate(&mut self, requests: &mut Vec<String>) -> Result<Option<M3dMap>, String> {
        let mut missing = Vec::new();
        let files = &self.files;
        let mut source = |path: &str| {
            let key = normalize_path(path);
            match files.get(&key) {
                Some(f) => f.clone(),
                None => {
                    missing.push(path.to_owned());
                    None
                }
            }
        };
        let result = acr_mapgen::generate(&self.settings, &mut source);
        if missing.is_empty() {
            return result.map(|g| Some(g.map)).map_err(|e| e.to_string());
        }
        for path in missing {
            if self.pending.insert(normalize_path(&path)) {
                requests.push(path);
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs the request/deliver loop against the real game data
    /// (`ACR_GAME_DIR=... cargo test -p acr-game -- --ignored`).
    #[test]
    #[ignore]
    fn collects_files_and_generates() {
        let dir = std::env::var("ACR_GAME_DIR").expect("ACR_GAME_DIR");
        let mut vfs = acr_formats::Vfs::open_game_dir(std::path::Path::new(&dir)).unwrap();
        let s = Settings::from_name("RN1 21A6 323454 01148139 4002013").unwrap();
        let mut g = RandomMapGen::new(s.clone());
        let mut rounds = 0;
        let map = loop {
            let mut requests = Vec::new();
            if let Some(map) = g.try_generate(&mut requests).unwrap() {
                break map;
            }
            for r in requests {
                let data = vfs.read(&r).map_err(|e| e.to_string());
                assert!(g.on_loaded(&r, &data));
            }
            rounds += 1;
            assert!(rounds < 10);
        };
        let direct = acr_mapgen::generate(&s, &mut |p: &str| vfs.read(p).ok()).unwrap().map;
        assert_eq!(map.heights, direct.heights);
        assert_eq!(map.sprites, direct.sprites);
        assert_eq!(map.units, direct.units);
    }

    #[test]
    fn menu_choices_become_settings() {
        use crate::ui::setup::{Control, GameKind, PlayerSetup};
        let map = RandomMapSetup { seed: 0x21A6, size: MapSize::Large, landscape: 4, climate: 3, ..Default::default() };
        let player = |nation, color| PlayerSetup { name: Vec::new(), nation, color, team: 0, control: Control::Human };
        let setup = GameSetup {
            kind: GameKind::RandomMap(map.clone()),
            players: vec![player(3, 0), player(1, 2)],
            local_player: 0,
        };
        let s = settings(&map, &setup);
        assert_eq!((s.size, s.shape, s.landscape, s.climate), (2, 0, 4, 3));
        assert_eq!(s.nations[..3], [Some(8), None, Some(0)]);
    }
}
