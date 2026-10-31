//! The random map generator of American Conquest (`dmcr.exe` 1.46, 0x45CD31), reproduced.
//!
//! Pure and deterministic: data files come in through [`Source`], the result is an
//! [`acr_formats::M3dMap`]. Same settings and data give the same map, and the algorithm
//! (including the engine's random number generator) follows the original so that a seed
//! gives the original's map. See `docs/mapgen.md` for the algorithm and the evidence.
//!
//! Outline:
//! 1. a land/water mask from a `.MSP` shape file (chosen by landscape and player count),
//!    blurred; water depth and shore textures follow from it;
//! 2. start points and territories (one region per player on the coarse grid);
//! 3. map pieces (`PIECES\*.SMP`): mines and nature around every start point, then relief,
//!    forests and settlements grown as chains of linked pieces (`*.SMP.LNK`);
//! 4. inland textures from fractal noise, land elevation from the mask;
//! 5. start units from `Data\Start%d.dat`.

mod files;
mod generator;
mod rng;
mod sea;

pub use files::{CostTable, LandShape, ResDat, StartUnits, TerrLine};
pub use rng::Rng;
pub use sea::{LANDSCAPE as SEA_LANDSCAPE, open_water};

use acr_formats::M3dMap;

/// Access to game files by their path in the archives (`Terrain\Res.dat`). Returns `None`
/// for missing files, which the generator treats like the original does (usually: skip).
pub trait Source {
    fn read(&mut self, path: &str) -> Option<Vec<u8>>;
}

impl<F: FnMut(&str) -> Option<Vec<u8>>> Source for F {
    fn read(&mut self, path: &str) -> Option<Vec<u8>> {
        self(path)
    }
}

/// Everything that determines a random map. The original encodes it in the map name
/// (`RN1 21A6 323454 01148139 4002013.m3d`, see [`Settings::from_name`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// Only the low 13 bits matter (they seed the random number generator).
    pub seed: u32,
    /// Map size shift: 1 normal, 2 large, 3 huge.
    pub size: u8,
    /// Index into the landscape styles of `Terrain\Res.dat` ("Landschaftstyp").
    pub landscape: u8,
    /// Index into the style's land shapes; shape `i` is made for `i + 2` players.
    pub shape: u8,
    /// Relief type ("Bodentyp"): 0 plain, 1 low mountains, 2 high mountains, 3 highlands,
    /// 4 plateaus.
    pub relief: u8,
    /// Start resources index.
    pub resources: u8,
    /// Mine density index ("Mineralien").
    pub minerals: u8,
    /// Climate: 0 northern, 1 temperate, 2 tropical, 3 dry.
    pub climate: u8,
    /// Nation of each colour slot as an index into `AI\AI.DAT`, `None` for no player.
    pub nations: [Option<u8>; 8],
    /// Decimal game options of the map name: digit 0 start units (`Data\Start%d.dat`),
    /// digit 3 peace time, digits 1, 2, 4, 5, 6 other options kept in `CAEP`.
    pub options: u32,
}

/// Peace time choices in seconds (0x45DBCF), selected by digit 3 of [`Settings::options`].
pub const PEACE_TIMES: [u32; 10] = [300, 600, 1200, 1800, 2700, 3600, 5400, 7200, 10800, 14400];

impl Settings {
    /// Parses a random map name: `RN<size-1> <seed hex> <menu choices hex> <8 nation
    /// digits> <options>`, as 0x456E14 does. The menu word holds one nibble each for
    /// landscape, shape, relief, resources, minerals and climate (lowest first); nation
    /// digits are `0` (empty) or the AI.DAT index + 1 in base 36, rightmost for slot 0.
    pub fn from_name(name: &str) -> Option<Self> {
        let name = name.trim_end_matches(".m3d").trim_end_matches(".M3D");
        let mut parts = name.split_ascii_whitespace();
        let head = parts.next()?;
        let size = head.as_bytes().get(2)?.checked_sub(b'/')?;
        let seed = u32::from_str_radix(parts.next()?, 16).ok()?;
        let menu = u32::from_str_radix(parts.next()?, 16).ok()?;
        let digits = parts.next()?.as_bytes();
        let options = parts.next().map_or(Some(0), |p| p.parse().ok())?;
        let mut nations = [None; 8];
        for (slot, n) in nations.iter_mut().enumerate() {
            let c = *digits.get(7usize.checked_sub(slot)?)?;
            let v = match c {
                b'0'..=b'9' => c - b'0',
                b'A'..=b'Z' => c - b'A' + 10,
                _ => 0,
            };
            *n = v.checked_sub(1);
        }
        let nib = |i: u32| ((menu >> (4 * i)) & 0xF) as u8;
        Some(Self {
            seed,
            size,
            landscape: nib(0),
            shape: nib(1),
            relief: nib(2),
            resources: nib(3),
            minerals: nib(4),
            climate: nib(5),
            nations,
            options,
        })
    }

    /// The map name the original would give these settings (inverse of [`Settings::from_name`]).
    pub fn name(&self) -> String {
        let menu = u32::from(self.landscape)
            | u32::from(self.shape) << 4
            | u32::from(self.relief) << 8
            | u32::from(self.resources) << 12
            | u32::from(self.minerals) << 16
            | u32::from(self.climate) << 20;
        let digits: String = (0..8)
            .rev()
            .map(|slot| match self.nations[slot] {
                None => '0',
                Some(n) => char::from_digit(u32::from(n) + 1, 36).unwrap_or('0').to_ascii_uppercase(),
            })
            .collect();
        format!("RN{} {:X} {:X} {} {}", self.size.saturating_sub(1), self.seed, menu, digits, self.options)
    }

    fn digit(&self, i: u32) -> u32 {
        self.options / 10u32.pow(i) % 10
    }
}

/// A generated map plus what the game needs besides it.
#[derive(Debug, Clone)]
pub struct Generated {
    pub map: M3dMap,
    /// Start point (world pixels) of each colour slot that has a player.
    pub starts: [Option<(i32, i32)>; 8],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// A file the generator cannot do without is missing or malformed.
    File(String),
    /// A setting is out of range for the data (e.g. no such landscape).
    Setting(&'static str),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::File(p) => write!(f, "missing or invalid file {p}"),
            Error::Setting(s) => write!(f, "invalid setting: {s}"),
        }
    }
}

impl std::error::Error for Error {}

/// Generates the map for `settings`.
pub fn generate(settings: &Settings, source: &mut dyn Source) -> Result<Generated, Error> {
    if settings.landscape == SEA_LANDSCAPE {
        return sea::generate(settings, source);
    }
    generator::run(settings, source)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip() {
        let s = Settings::from_name("RN1 21A6 323454 01148139 4002013.m3d").unwrap();
        assert_eq!((s.size, s.seed, s.landscape, s.shape, s.relief), (2, 0x21A6, 4, 5, 4));
        assert_eq!((s.resources, s.minerals, s.climate), (3, 2, 3));
        assert_eq!(s.nations, [Some(8), Some(2), Some(0), Some(7), Some(3), Some(0), Some(0), None]);
        assert_eq!(s.digit(3), 2);
        assert_eq!(s.name(), "RN1 21A6 323454 01148139 4002013");
    }
}
