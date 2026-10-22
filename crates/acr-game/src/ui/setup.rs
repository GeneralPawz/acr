//! What the menus hand to the game when the player starts one.

/// Difficulty of computer opponents, in the order of the menus' combo boxes
/// (`@RMID_EASY` .. `@RMID_IMPOSIBLE`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Difficulty {
    Easy,
    #[default]
    Normal,
    Hard,
    VeryHard,
    Impossible,
}

impl Difficulty {
    pub const ALL: [Difficulty; 5] =
        [Difficulty::Easy, Difficulty::Normal, Difficulty::Hard, Difficulty::VeryHard, Difficulty::Impossible];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn from_index(i: usize) -> Self {
        Self::ALL[i.min(4)]
    }
}

/// A mission from `Missions\Missions.txt`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissionSetup {
    /// Identifier such as `#SINGLE1` or `#columb1@`.
    pub id: String,
    /// Display name (CP1252).
    pub name: Vec<u8>,
    /// Map file (`.m3d`).
    pub map: String,
    /// Mission script (`Mission.dll`), run by the x86 interpreter.
    pub script: String,
    /// Mission text file with briefing and in-game messages.
    pub texts: String,
    pub difficulty: Difficulty,
    /// Campaign index and mission index within it, for campaign missions.
    pub campaign: Option<(usize, usize)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MapSize {
    #[default]
    Normal,
    Large,
    Huge,
}

/// Settings of the random map generator, as indices into the menu's choices.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RandomMapSetup {
    pub seed: u32,
    /// `INTF_CROOM_O0` Landschaftstyp: map style from `Terrain\Res.dat`.
    pub landscape: usize,
    /// `INTF_CROOM_O1` Bodentyp.
    pub ground: usize,
    /// `INTF_CROOM_O2` Ressourcen: starting resources.
    pub resources: usize,
    /// `INTF_CROOM_O3` Mineralien: mine density.
    pub minerals: usize,
    pub size: MapSize,
    /// `INTF_CROOM_O5` Klimazone: 0 nordisch, 1 gemäßigt, 2 tropisch, 3 trocken.
    pub climate: usize,
}

/// The blank test field of the driver (`docs/testing.md`, "The blank field"): a map built in
/// code like the editor's new map, see [`crate::blank_map`]. Lengths in world pixels.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BlankSetup {
    pub size: MapSize,
    pub ground: Ground,
    /// A ramp along x: `(tenths of a degree, x0, x1)`, flat west of `x0`, rising to `x1`.
    pub slope: Option<(i32, i32, i32)>,
    /// A round hill: `(x, y, radius, height)`.
    pub hill: Option<(i32, i32, i32, i32)>,
    /// Water: `(x, y, width, height)`.
    pub water: Option<(i32, i32, i32, i32)>,
}

/// The one ground texture of a blank field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Ground {
    #[default]
    Grass,
    Dirt,
    Snow,
    /// Pure white, no texture, light or water: a backdrop for checking animations.
    White,
    /// Pure greenscreen green (0, 255, 0), like `White`.
    Greenscreen,
}

/// Who controls a player.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    Human,
    Computer(Difficulty),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerSetup {
    pub name: Vec<u8>,
    /// Index into the nations of `NATIONS.TXT`.
    pub nation: usize,
    /// Player colour 0..8.
    pub color: u8,
    /// Team 1.., or 0 for none.
    pub team: u8,
    pub control: Control,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameKind {
    Mission(MissionSetup),
    RandomMap(RandomMapSetup),
    /// The driver's blank test field: no objects, no start units, no AI, no fog.
    Blank(BlankSetup),
}

/// A game to start: produced by "start" in the menus, consumed by the in-game screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameSetup {
    pub kind: GameKind,
    /// Players of a random map; missions define their own players.
    pub players: Vec<PlayerSetup>,
    /// Index into `players` of the player at this computer.
    pub local_player: usize,
    /// Optional rules that are not in the original game (all off by default), such as hit
    /// locations (#225). Saved with the game.
    pub rules: acr_sim::Rules,
}
