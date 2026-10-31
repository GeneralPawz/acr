//! The sea battle map (#408), an ACR addition: the original's landscape styles
//! (`Terrain\Res.dat`, `LT_Land` to `LT_Continent`) always have land, so the menu offers one
//! more style after them. The whole map is open water (`SEA2` deeper than the 130 at which
//! ships float and land units stop, `docs/water.md`) and every player's ships start at
//! their own end of it; no port, no territory, no peace time.

use acr_formats::M3dMap;
use acr_formats::m3d::{Grid, MapSize, Unit, Water};

use crate::{Error, Generated, Settings, Source};

/// The landscape index of the sea map: the first one after the original's six.
pub const LANDSCAPE: u8 = 6;

/// `SEA2` depth of open water.
const DEEP: u8 = 200;
/// `SEA2` brightness of open water (as the blank field's).
const BRIGHT: u8 = 0x60;

/// The ships of a nation (the `(XX)` of its type ids, as in `AI\AI.DAT`), flagship first:
/// the European nations have the frigate and caravels, the native ones war canoes. The
/// other ship types (`ShpParom`, `ShpLodka`) carry no guns.
fn fleet(nation: &str) -> Result<[String; 3], Error> {
    let ships: [&str; 3] = match nation {
        "EN" | "FR" | "SP" | "US" => ["ShpFregat", "ShpPinta", "ShpPinta"],
        "AC" | "DE" | "HU" | "IN" | "IR" | "MA" | "PU" | "SI" => ["ShpKanoe3"; 3],
        _ => return Err(Error::Setting("nation")),
    };
    Ok(ships.map(|s| format!("{s}({nation})")))
}

/// The `SEA2` layers of a map of size shift `shift` that are open water everywhere.
pub fn open_water(shift: u8) -> Water {
    let n = (240usize << shift) + 2;
    Water {
        depth: Grid { width: n, height: n, cells: vec![DEEP; n * n] },
        brightness: Grid { width: n, height: n, cells: vec![BRIGHT; n * n] },
    }
}

/// Where the fleet of the `k`-th of `n` players starts, in world pixels, and its facing
/// (0 east, 128 west): two players face each other across the map, more stand on a circle
/// facing its middle.
fn start(k: usize, n: usize, playable: i32) -> (i32, i32, u8) {
    let (c, r) = (playable / 2, playable * 2 / 5);
    if n <= 2 {
        let west = k == 0;
        return (if west { c - r } else { c + r }, c, if west { 0 } else { 128 });
    }
    let a = std::f64::consts::TAU * k as f64 / n as f64 + std::f64::consts::PI;
    let (x, y) = (c + (r as f64 * a.cos()) as i32, c + (r as f64 * a.sin()) as i32);
    let towards = (f64::from(c - y)).atan2(f64::from(c - x));
    (x, y, (towards / std::f64::consts::TAU * 256.0).rem_euclid(256.0) as u8)
}

pub(crate) fn generate(settings: &Settings, source: &mut dyn Source) -> Result<Generated, Error> {
    if !(1..=3).contains(&settings.size) {
        return Err(Error::Setting("size"));
    }
    let res = source.read("Terrain\\Res.dat").ok_or_else(|| Error::File("Terrain\\Res.dat".into()))?;
    let res = crate::ResDat::parse(&res).ok_or_else(|| Error::File("Terrain\\Res.dat".into()))?;
    let ai = source.read("AI\\AI.DAT").ok_or_else(|| Error::File("AI\\AI.DAT".into()))?;
    let nations = crate::files::parse_ai_nations(&ai);
    let amounts = res.resources.get(settings.resources as usize).map_or([0; 8], |r| r.1);
    let size = MapSize { shift: settings.size };
    let (vw, vh) = (size.vertex_width(), size.vertex_height());
    let playable = size.playable_size();
    let slots: Vec<usize> = (0..8).filter(|&s| settings.nations[s].is_some()).collect();
    let mut starts = [None; 8];
    let mut units = Vec::new();
    for (k, &slot) in slots.iter().enumerate() {
        // The nation's suffix: the `(XX)` of its start peasant.
        let peasant = &nations.get(usize::from(settings.nations[slot].unwrap_or(0))).ok_or(Error::Setting("nation"))?.1;
        let suffix = peasant.rsplit_once('(').and_then(|(_, s)| s.strip_suffix(')')).ok_or(Error::Setting("nation"))?;
        let ships = fleet(suffix)?;
        let (x, y, direction) = start(k, slots.len(), playable);
        starts[slot] = Some((x, y));
        for (i, name) in ships.iter().enumerate() {
            // Ships side by side across the facing, 160 px apart.
            let off = (i as i32 - 1) * 160;
            let (dx, dy) = if slots.len() <= 2 { (0, off) } else { (off, 0) };
            units.push(Unit {
                nation: slot as u8,
                type_index: 0,
                x: (x + dx) << 4,
                y: (y + dy) << 4,
                life: 0,
                stage: 0,
                unknown: [0; 2],
                direction,
                flags: 0,
                name: name.clone(),
            });
        }
    }
    let map = M3dMap {
        size: Some(size),
        header: [vw as u32, vh as u32],
        heights: Some(Grid { width: vw, height: vh, cells: vec![0; vw * vh] }),
        textures: Some(Grid { width: vw, height: vh, cells: vec![0; vw * vh] }),
        units,
        water: Some(open_water(settings.size)),
        resources: Some([amounts; acr_formats::m3d::PLAYERS]),
        mission_dll: Some(String::new()),
        ..M3dMap::default()
    };
    Ok(Generated { map, starts })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(players: usize) -> Settings {
        let mut s = Settings::from_name("RN0 1 6 00000000 5002012").unwrap();
        s.landscape = LANDSCAPE;
        for (i, n) in s.nations.iter_mut().enumerate() {
            *n = (i < players).then_some((i % 2) as u8);
        }
        s
    }

    fn files(path: &str) -> Option<Vec<u8>> {
        if path == "AI\\AI.DAT" {
            return Some(b"2 2\nSPAIN Spain SpnKri(SP) 9 1 1 - -\nMAY May MayKri(MA) 9 1 1 - -\n".to_vec());
        }
        (path == "Terrain\\Res.dat")
            .then(|| b"1 0\nA 1 1 1\n1 0\nRA 6 5 5 5 5 5 5\n1 0\nRT\n1 0\n#0 LT 000.STY 1 2 a.msp\n".to_vec())
    }

    #[test]
    fn two_fleets_face_each_other_across_open_water() {
        let g = crate::generate(&settings(2), &mut files).unwrap();
        let map = g.map;
        assert!(map.water.as_ref().unwrap().depth.cells.iter().all(|&d| d > 130));
        assert_eq!(map.units.len(), 6);
        assert_eq!(map.units[0].name, "ShpFregat(SP)");
        assert_eq!(map.units[3].name, "ShpKanoe3(MA)");
        let west: Vec<_> = map.units.iter().filter(|u| u.nation == 0).collect();
        let east: Vec<_> = map.units.iter().filter(|u| u.nation == 1).collect();
        assert!(west.iter().all(|u| u.x >> 4 < 4000 && u.direction == 0));
        assert!(east.iter().all(|u| u.x >> 4 > 11000 && u.direction == 128));
        assert_eq!(g.starts[1].map(|s| s.1), Some(7680));
    }

    #[test]
    fn more_players_stand_apart() {
        let g = crate::generate(&settings(4), &mut files).unwrap();
        let mut seen: Vec<_> = g.starts.iter().flatten().collect();
        seen.dedup();
        assert_eq!(seen.len(), 4);
        assert_eq!(g.map.units.len(), 12);
    }

    #[test]
    fn a_missing_resource_file_is_an_error() {
        assert!(crate::generate(&settings(2), &mut |_: &str| None).is_err());
    }
}
