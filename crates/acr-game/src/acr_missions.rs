//! ACR's own single missions (#417), listed after the original's in the single missions
//! screen. The original's missions are data (an `.m3d` map, a script DLL, a text file) that
//! ACR must not ship; these are code: the map is made here, the rules and the computer
//! player's orders are Rust, run by [`crate::mission::MissionLink`] before every tick like
//! the scripts of the original's missions. `docs/acr-missions.md` says how to add one.
//!
//! The only mission so far is the Battle of Lake Erie (10 September 1813).

use acr_formats::M3dMap;
use acr_formats::m3d::{Grid, MapSize, Unit, Water};
use acr_sim::{Command, EntityId, World};

use crate::ui::setup::{Control, Difficulty, Ground, PlayerSetup};

/// The missions ACR authors, in menu order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcrMission {
    /// Perry's squadron against Barclay's off Put-in-Bay; the player commands the Americans.
    LakeErie1813,
}

/// The ticks between two rounds of the computer player's orders.
const ORDER_PERIOD: u64 = 200;
/// No verdict before this tick: the world has started and every ship is on the water.
const GRACE: u64 = 16;

impl AcrMission {
    pub const ALL: [AcrMission; 1] = [AcrMission::LakeErie1813];

    /// The identifier kept in savegames.
    pub fn id(self) -> &'static str {
        match self {
            AcrMission::LakeErie1813 => "lake_erie_1813",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.id() == id)
    }

    /// The name in the mission list (CP1252).
    pub fn name(self) -> Vec<u8> {
        match self {
            AcrMission::LakeErie1813 => encode("Seeschlacht auf dem Eriesee"),
        }
    }

    /// The briefing the single missions screen shows (CP1252, paragraphs split by a blank
    /// line).
    pub fn briefing(self) -> Vec<u8> {
        encode(match self {
            AcrMission::LakeErie1813 => BRIEFING_LAKE_ERIE,
        })
    }

    /// The players: the human first (owner 0), then the computer's.
    pub fn players(self) -> Vec<PlayerSetup> {
        match self {
            AcrMission::LakeErie1813 => vec![
                // `TEXT\NATIONS.TXT` order: MAY SPAIN ENGLAND FRANCE USA ...
                PlayerSetup { name: Vec::new(), nation: 4, color: 0, team: 1, control: Control::Human },
                PlayerSetup {
                    name: encode("Barclay"),
                    nation: 2,
                    color: 1,
                    team: 2,
                    control: Control::Computer(Difficulty::Normal),
                },
            ],
        }
    }

    /// The map: water and land as the `SEA2` layers, the fleets as units.
    pub fn map(self) -> M3dMap {
        match self {
            AcrMission::LakeErie1813 => lake_erie_map(),
        }
    }

    /// `Some(true)` when the player has won, `Some(false)` when lost, `None` while both
    /// sides still have ships. A side is beaten when all its ships are sunk (no surrender
    /// rule: the original has none either).
    pub fn verdict(self, world: &World) -> Option<bool> {
        if world.tick.0 < GRACE {
            return None;
        }
        match self {
            AcrMission::LakeErie1813 => {
                let own = ships(world, 0).count();
                let enemy = ships(world, 1).count();
                if own == 0 {
                    Some(false)
                } else if enemy == 0 {
                    Some(true)
                } else {
                    None
                }
            }
        }
    }

    /// The computer player's orders for this tick: every `ORDER_PERIOD` ticks (the first at
    /// tick 0) its idle ships attack-move to the middle of the player's fleet.
    pub fn orders(self, world: &World) -> Vec<Command> {
        if !world.tick.0.is_multiple_of(ORDER_PERIOD) {
            return Vec::new();
        }
        match self {
            AcrMission::LakeErie1813 => {
                let target: Vec<_> = ships(world, 0).map(|u| u.pos).collect();
                if target.is_empty() {
                    return Vec::new();
                }
                let n = target.len() as i64;
                let x = target.iter().map(|p| i64::from(p.x)).sum::<i64>() / n;
                let y = target.iter().map(|p| i64::from(p.y)).sum::<i64>() / n;
                let idle: Vec<EntityId> = ships(world, 1).filter(|u| u.idle()).map(|u| u.id).collect();
                if idle.is_empty() {
                    return Vec::new();
                }
                vec![Command::AttackMove { player: 1, units: idle, x: (x >> 4) as i32, y: (y >> 4) as i32 }]
            }
        }
    }
}

/// The living ships of `owner`.
fn ships(world: &World, owner: u8) -> impl Iterator<Item = &acr_sim::Unit> {
    world
        .units
        .iter()
        .filter(move |u| u.owner == owner && u.living() && world.catalog.get(u.kind).id.starts_with("Shp"))
}

fn encode(text: &str) -> Vec<u8> {
    acr_formats::CodePage::Cp1252.encode(text).unwrap_or_else(|| text.bytes().filter(u8::is_ascii).collect())
}

const BRIEFING_LAKE_ERIE: &str = "\
Eriesee, 10. September 1813. Im Krieg von 1812 hängt die Versorgung der britischen Posten im Westen, \
vor allem Fort Malden, vom Wasserweg über den See ab. Seit Wochen liegt das amerikanische Geschwader unter \
Kommandant Oliver Hazard Perry vor Put-in-Bay auf South Bass Island und sperrt den Briten die Zufahrt.\n\
\n\
Im Morgengrauen sichtet der Ausguck sechs britische Schiffe im Nordwesten, jenseits von Rattlesnake Island. \
Kommandant Robert Barclay hat keine Vorräte mehr und muss kämpfen. Er führt die Detroit und die Queen Charlotte, \
dazu Lady Prevost, General Hunter und Chippewa. Die Briten haben Langrohrgeschütze mit großer Reichweite; \
die Amerikaner führen vor allem Karronaden, die nur aus der Nähe wirken.\n\
\n\
Ihr Auftrag: Sie führen Perrys Geschwader, die Brigs Lawrence und Niagara mit den kleineren Schiffen \
Caledonia, Scorpion, Ariel und Somers. Versenken Sie alle britischen Schiffe. Gehen alle Ihre Schiffe \
unter, ist die Schlacht verloren.\n\
\n\
Hinweis: Die Fregatten des Spiels vertreten die Brigs und Schiffe, die Pinassen die Schoner und Schaluppen. \
Historisch standen neun amerikanische gegen sechs britische Schiffe; Porcupine, Tigress, Trippe und Little Belt fehlen hier.";

// --- the map --------------------------------------------------------------------------

/// The side shift of the map (the menu's normal size, 15 360 px a side).
const SHIFT: u8 = 1;
/// `SEA2` depth of the open water at some distance from the shore, and the brightness.
const DEEP: f64 = 200.0;
const BRIGHT: u8 = 0x60;

/// Lake Erie's western basin, north up. The lake is open water; the Ohio shore lies at the
/// south edge, Pelee Island and the Canadian shore at the north, the Fort Malden headland at
/// the north-west, and the Bass Islands off Put-in-Bay in the south-east.
const ISLANDS: [(f64, f64, f64); 7] = [
    // (x, y, radius) in world pixels
    (9800.0, 12600.0, 950.0), // South Bass, with Put-in-Bay on its north-west side
    (9650.0, 11450.0, 230.0), // Gibraltar Island
    (8300.0, 13750.0, 600.0), // Middle Bass
    (8900.0, 10050.0, 550.0), // North Bass (Isle St George)
    (7250.0, 11150.0, 330.0), // Rattlesnake Island
    (8000.0, 2300.0, 1300.0), // Pelee Island
    (500.0, 900.0, 2300.0),   // the Malden headland
];

/// The signed distance in world pixels from `(x, y)` to the nearest land: negative on land.
fn shore(x: f64, y: f64) -> f64 {
    // The south (Ohio) and north (Ontario) shores.
    let south = 14_350.0 + 220.0 * (x * 0.0011).sin() + 120.0 * (x * 0.0031 + 1.0).sin();
    let north = 520.0 + 180.0 * (x * 0.0014 + 2.0).sin() + 90.0 * (x * 0.0037).sin();
    let mut d = (south - y).min(y - north);
    for (i, &(cx, cy, r)) in ISLANDS.iter().enumerate() {
        let (dx, dy) = (x - cx, y - cy);
        // A wobbling outline, a different one per island.
        let angle = dy.atan2(dx);
        let k = i as f64;
        let wobble = 1.0 + 0.12 * (3.0 * angle + k).sin() + 0.06 * (5.0 * angle + 2.0 * k).sin();
        d = d.min(dx.hypot(dy) - r * wobble);
    }
    d
}

/// A ship of `nation` ("US", "EN") at `(x, y)` world pixels.
fn ship(owner: u8, name: &str, x: f64, y: f64, direction: u8) -> Unit {
    Unit {
        nation: owner,
        type_index: 0,
        x: (x as i32) << 4,
        y: (y as i32) << 4,
        life: 0,
        stage: 0,
        unknown: [0; 2],
        direction,
        flags: 0,
        name: name.to_owned(),
    }
}

/// A line of `types` abreast of `(x, y)`, across the heading `(hx, hy)` (a unit vector),
/// 220 px apart.
fn line(owner: u8, types: &[&str], (x, y): (f64, f64), (hx, hy): (f64, f64), direction: u8) -> Vec<Unit> {
    let n = types.len() as f64;
    types
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let off = (i as f64 - (n - 1.0) / 2.0) * 220.0;
            ship(owner, name, x - hy * off, y + hx * off, direction)
        })
        .collect()
}

/// The fleets: the Americans in Put-in-Bay heading north-west (the Lawrence and Niagara in
/// the middle), the British to the north-west heading south-east (the Detroit and Queen
/// Charlotte in the middle).
fn fleets() -> Vec<Unit> {
    let s = std::f64::consts::FRAC_1_SQRT_2;
    let mut units = line(
        0,
        &["ShpPinta(US)", "ShpPinta(US)", "ShpFregat(US)", "ShpFregat(US)", "ShpPinta(US)", "ShpPinta(US)"],
        (8300.0, 11400.0),
        (-s, -s),
        160,
    );
    units.extend(line(
        1,
        &["ShpPinta(EN)", "ShpFregat(EN)", "ShpFregat(EN)", "ShpPinta(EN)", "ShpPinta(EN)"],
        (5200.0, 6800.0),
        (s, s),
        32,
    ));
    units
}

fn lake_erie_map() -> M3dMap {
    let size = MapSize { shift: SHIFT };
    let (vw, vh) = (size.vertex_width(), size.vertex_height());
    // Terrain vertices: the land rises from the shore to some 50 px; the shallows and beach
    // are bare earth (`TILES3.BMP`), the land inland green.
    let beach = crate::blank_map::texture(Ground::Dirt);
    let mut heights = vec![0i16; vw * vh];
    let mut textures = vec![0u8; vw * vh];
    for v in 0..vw * vh {
        let (col, row) = ((v % vw) as i32, (v / vw) as i32);
        let (x, y) = (col << 5, (row << 5) - ((col & 1) << 4));
        let d = shore(f64::from(x), f64::from(y));
        heights[v] = ((-d).clamp(0.0, 600.0) / 12.0) as i16;
        textures[v] = if d > -160.0 { beach } else { 0 };
    }
    // The water on the 32 px grid: none on land, above the 130 at which ships float
    // beyond the first cell, deepening to the open lake (`docs/water.md`).
    let n = (240usize << SHIFT) + 2;
    let mut depth = vec![0u8; n * n];
    for (i, cell) in depth.iter_mut().enumerate() {
        let (cx, cy) = ((i % n) as f64, (i / n) as f64);
        let d = shore(cx * 32.0, cy * 32.0);
        if d > 0.0 {
            *cell = (131.0 + d / 32.0 * 14.0).min(DEEP) as u8;
        }
    }
    M3dMap {
        size: Some(size),
        header: [vw as u32, vh as u32],
        heights: Some(Grid { width: vw, height: vh, cells: heights }),
        textures: Some(Grid { width: vw, height: vh, cells: textures }),
        units: fleets(),
        water: Some(Water {
            depth: Grid { width: n, height: n, cells: depth },
            brightness: Grid { width: n, height: n, cells: vec![BRIGHT; n * n] },
        }),
        resources: Some([crate::blank_map::BLANK_STOCK; acr_formats::m3d::PLAYERS]),
        mission_dll: Some(String::new()),
        ..M3dMap::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip() {
        for m in AcrMission::ALL {
            assert_eq!(AcrMission::from_id(m.id()), Some(m));
        }
        assert_eq!(AcrMission::from_id("nope"), None);
    }

    #[test]
    fn the_fleets_float_clear_of_the_shore() {
        let map = AcrMission::LakeErie1813.map();
        let us = map.units.iter().filter(|u| u.nation == 0).count();
        let british = map.units.iter().filter(|u| u.nation == 1).count();
        assert_eq!((us, british), (6, 5));
        let frigates =
            |name: &str| map.units.iter().filter(|u| u.name.starts_with("ShpFregat") && u.name.ends_with(name)).count();
        assert_eq!((frigates("(US)"), frigates("(EN)")), (2, 2));
        let depth = &map.water.as_ref().unwrap().depth;
        for u in &map.units {
            let (x, y) = (u.x >> 4, u.y >> 4);
            assert!(shore(f64::from(x), f64::from(y)) > 300.0, "{} at {x},{y} is near land", u.name);
            let cell = (y as usize >> 5) * depth.width + (x as usize >> 5);
            assert!(depth.cells[cell] > 130);
        }
    }

    #[test]
    fn the_map_has_land_and_open_water() {
        let map = AcrMission::LakeErie1813.map();
        let depth = &map.water.as_ref().unwrap().depth;
        let n = depth.width;
        let land = depth.cells.iter().filter(|&&d| d <= 128).count();
        assert!(land > n * n / 50 && land < n * n / 3, "{land} land cells of {}", n * n);
        // The Bass Islands are land, the lake between the fleets is deep.
        let at = |x: usize, y: usize| depth.cells[(y >> 5) * n + (x >> 5)];
        assert!(at(9800, 12600) <= 128 && at(8900, 10050) <= 128);
        assert_eq!(at(7000, 9000), 200);
        // The terrain grids match the vertex grid.
        let h = map.heights.as_ref().unwrap();
        assert_eq!(h.cells.len(), h.width * h.height);
        assert!(h.cells.iter().any(|&c| c > 20));
    }

    #[test]
    fn players_and_texts() {
        let m = AcrMission::LakeErie1813;
        let players = m.players();
        assert_eq!(players.len(), 2);
        assert!(matches!(players[0].control, Control::Human));
        assert!(matches!(players[1].control, Control::Computer(_)));
        assert!(m.briefing().len() > 800);
        assert!(!m.name().is_empty());
    }
}
