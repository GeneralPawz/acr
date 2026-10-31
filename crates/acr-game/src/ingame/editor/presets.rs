//! Army presets of the test editor (not in the original, whose editor places single units):
//! a right click on the nation combo box opens a small menu, "Klein", "Mittel" or "Gross"
//! (and the century, where the nation spans more than one), and the next left click on the
//! map places that nation's army around the clicked point, facing the editor's Richtung.
//!
//! A preset is a [`Doctrine`] (rows of bodies, front to back, in [`Size`] steps) and a
//! [`ROSTERS`] line that names the nation's unit type for each [`Role`]. The rows are laid
//! out from the real formation shapes ([`plan`]): bodies side by side with a gap, rows one
//! behind the other, the whole army centred on the clicked point, so nothing overlaps
//! whatever the unit types are. [`place_preset`] is the API the battle tab uses.
//! docs/controls.md, "Army presets", has the layouts and their historical sources.

use acr_formats::m3d::MapSize;
use acr_sim::fixed::Pos;
use acr_sim::{EntityId, TypeId, World};

use super::{Act as EditorAct, ENTRIES, Editor, FIRST_EXTRA, FORMATION_GAP, FormChoice, Item, Look, OX, OY, ROW};
use super::{formations, grid, place_formations};
use crate::ui::draw::Rect;

/// How far from the map's edge an army stays, world pixels (the border locks and the units
/// pushed out of the formation slots).
pub const MAP_MARGIN: i32 = 320;
/// Space a soldier takes around its slot, and in front of or behind a formation for its
/// officer, drummer and flag bearer.
const SLOT_MARGIN: i32 = 24;

/// How big an army is (formation bodies; counts in docs/controls.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Size {
    Small,
    Medium,
    Large,
}

impl Size {
    pub const ALL: [Size; 3] = [Size::Small, Size::Medium, Size::Large];

    /// The size's number in a savegame (its place in [`Size::ALL`]).
    pub fn code(self) -> u8 {
        Self::ALL.iter().position(|&s| s == self).expect("listed") as u8
    }

    pub fn from_code(code: u8) -> Option<Self> {
        Self::ALL.get(usize::from(code)).copied()
    }

    pub fn label(self) -> &'static str {
        match self {
            Size::Small => "Klein",
            Size::Medium => "Mittel",
            Size::Large => "Gross",
        }
    }
}

/// The century a European army fights in (the unit lists span the 17th to the early 19th).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Era {
    C17,
    C18,
    C19,
}

impl Era {
    pub const ALL: [Era; 3] = [Era::C17, Era::C18, Era::C19];

    /// The century's number in a savegame (its place in [`Era::ALL`]).
    pub fn code(self) -> u8 {
        Self::ALL.iter().position(|&e| e == self).expect("listed") as u8
    }

    pub fn from_code(code: u8) -> Option<Self> {
        Self::ALL.get(usize::from(code)).copied()
    }

    pub fn label(self) -> &'static str {
        match self {
            Era::C17 => "17. Jahrhundert",
            Era::C18 => "18. Jahrhundert",
            Era::C19 => "Fruehes 19. Jhd.",
        }
    }
}

/// What a body does in an army; the roster says which unit type fills it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// The main infantry (musketeers, warriors).
    Front,
    /// Pikemen, standing in a block between the shot.
    Pike,
    /// The elite body in the centre.
    Elite,
    /// Skirmishers and riflemen in front.
    Screen,
    /// Archers behind the main body.
    Back,
    /// Cavalry on the flanks.
    Wing,
    /// Militia (the USA), loose on the flanks of the second line.
    Militia,
    /// Cannons.
    Guns,
}

/// How a body stands: loose, or in a formation group of `ORD_GROUPS.LST`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Form {
    Loose,
    Line,
    /// `SQUARE`, "Kolonne".
    Column,
    /// `KARE`, "Karree".
    Karree,
    /// The cavalry line `PRUS`.
    Heavy,
    /// The cavalry wedge `TRI`.
    Wedge,
    /// The thin cavalry line `SHER`.
    Thin,
}

impl Form {
    fn group(self) -> Option<&'static str> {
        match self {
            Form::Loose => None,
            Form::Line => Some("LINE"),
            Form::Column => Some("SQUARE"),
            Form::Karree => Some("KARE"),
            Form::Heavy => Some("PRUS"),
            Form::Wedge => Some("TRI"),
            Form::Thin => Some("SHER"),
        }
    }
}

/// One entry of a row: `n` bodies of `men` soldiers (a formation size, or the loose count)
/// side by side.
#[derive(Debug, Clone, Copy)]
pub struct Entry {
    pub role: Role,
    pub men: u16,
    pub form: Form,
    pub n: u8,
}

const fn at(role: Role, men: u16, form: Form, n: u8) -> Entry {
    Entry { role, men, form, n }
}

/// A row of bodies side by side, `gap` pixels behind the row before it.
#[derive(Debug, Clone, Copy)]
pub struct Row {
    pub gap: i32,
    pub entries: &'static [Entry],
}

macro_rules! rows {
    ($($gap:expr => [$($e:expr),* $(,)?]),* $(,)?) => { &[$(Row { gap: $gap, entries: &[$($e),*] }),*] };
}

use Form::{Column, Heavy, Karree, Line, Loose, Thin, Wedge};
use Role::{Back, Elite, Front, Guns, Militia, Pike, Screen, Wing};

/// How an army is drawn up: three sizes of rows, front to back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Doctrine {
    /// Pike and shot (17th century): pike blocks between lines of shot, horse on the wings.
    PikeShot,
    /// Linear tactics (18th century): lines of foot, guns between and in front, horse on the
    /// wings and in reserve, a second line behind.
    Linear,
    /// Napoleonic columns (French and Spanish, early 19th century) under a skirmish screen
    /// and a grand battery.
    Column,
    /// Lines and squares behind a skirmish screen (British and American, early 19th century).
    Lines19,
    /// A formed native army (Mesoamerica): lines of warriors, archers behind, an elite body.
    Warband,
    /// A loose native army (Woodland, Pueblos): clouds of warriors, archers behind.
    Clans,
    /// Mounted nations (Plains): wedges of horse, mounted archers behind, hunters in front.
    Riders,
}

impl Doctrine {
    pub fn rows(self, size: Size) -> &'static [Row] {
        let i = size as usize;
        match self {
            Doctrine::PikeShot => PIKE_SHOT[i],
            Doctrine::Linear => LINEAR[i],
            Doctrine::Column => COLUMN[i],
            Doctrine::Lines19 => LINES19[i],
            Doctrine::Warband => WARBAND[i],
            Doctrine::Clans => CLANS[i],
            Doctrine::Riders => RIDERS[i],
        }
    }
}

const PIKE_SHOT: [&[Row]; 3] = [
    rows![0 => [at(Guns, 2, Loose, 1)], 120 => [at(Front, 72, Line, 1), at(Pike, 36, Column, 1)]],
    rows![
        0 => [at(Screen, 15, Loose, 1)],
        120 => [at(Guns, 2, Loose, 2)],
        160 => [at(Wing, 40, Heavy, 1), at(Front, 72, Line, 1), at(Pike, 72, Column, 1),
                at(Front, 72, Line, 1), at(Wing, 40, Heavy, 1)],
    ],
    rows![
        0 => [at(Screen, 36, Loose, 1), at(Guns, 2, Loose, 3), at(Screen, 36, Loose, 1)],
        200 => [at(Wing, 90, Heavy, 1), at(Front, 72, Line, 1), at(Pike, 72, Column, 1), at(Front, 72, Line, 2),
                at(Pike, 72, Column, 1), at(Front, 72, Line, 1), at(Wing, 90, Heavy, 1)],
        300 => [at(Front, 120, Line, 1), at(Pike, 120, Column, 1), at(Front, 120, Line, 1)],
        300 => [at(Wing, 90, Heavy, 1)],
    ],
];

const LINEAR: [&[Row]; 3] = [
    rows![0 => [at(Guns, 2, Loose, 1)], 120 => [at(Front, 72, Line, 1), at(Wing, 40, Heavy, 1)]],
    rows![
        0 => [at(Screen, 15, Loose, 1)],
        120 => [at(Guns, 2, Loose, 2)],
        160 => [at(Wing, 40, Heavy, 1), at(Front, 120, Line, 3), at(Wing, 40, Heavy, 1)],
    ],
    rows![
        0 => [at(Screen, 36, Loose, 1), at(Guns, 2, Loose, 3), at(Screen, 36, Loose, 1)],
        200 => [at(Wing, 90, Heavy, 1), at(Front, 120, Line, 2), at(Guns, 2, Loose, 1),
                at(Front, 120, Line, 2), at(Wing, 90, Heavy, 1)],
        300 => [at(Militia, 72, Loose, 1), at(Front, 120, Line, 3), at(Militia, 72, Loose, 1)],
        300 => [at(Wing, 90, Heavy, 2)],
    ],
];

const COLUMN: [&[Row]; 3] = [
    rows![0 => [at(Guns, 2, Loose, 1)], 120 => [at(Front, 72, Column, 1), at(Wing, 40, Heavy, 1)]],
    rows![
        0 => [at(Screen, 15, Loose, 1), at(Guns, 2, Loose, 2), at(Screen, 15, Loose, 1)],
        200 => [at(Wing, 40, Heavy, 1), at(Front, 120, Column, 3), at(Wing, 40, Heavy, 1)],
    ],
    rows![
        0 => [at(Screen, 36, Loose, 1), at(Guns, 2, Loose, 4), at(Screen, 36, Loose, 1)],
        200 => [at(Wing, 90, Heavy, 1), at(Front, 120, Column, 6), at(Wing, 90, Heavy, 1)],
        300 => [at(Front, 72, Column, 2), at(Wing, 90, Heavy, 2)],
    ],
];

const LINES19: [&[Row]; 3] = [
    rows![0 => [at(Guns, 2, Loose, 1)], 120 => [at(Front, 72, Line, 1), at(Front, 36, Karree, 1)]],
    rows![
        0 => [at(Screen, 15, Loose, 1), at(Guns, 2, Loose, 2), at(Screen, 15, Loose, 1)],
        160 => [at(Front, 120, Line, 2), at(Front, 72, Karree, 1), at(Front, 120, Line, 1), at(Wing, 40, Heavy, 1)],
    ],
    rows![
        0 => [at(Screen, 36, Loose, 1), at(Guns, 2, Loose, 3), at(Screen, 36, Loose, 1)],
        200 => [at(Wing, 90, Heavy, 1), at(Front, 120, Line, 3), at(Guns, 2, Loose, 1),
                at(Front, 120, Line, 3), at(Wing, 90, Heavy, 1)],
        300 => [at(Militia, 72, Loose, 1), at(Front, 72, Karree, 1), at(Front, 120, Line, 1),
                at(Front, 72, Karree, 1), at(Militia, 72, Loose, 1)],
        300 => [at(Wing, 90, Heavy, 1)],
    ],
];

const WARBAND: [&[Row]; 3] = [
    rows![0 => [at(Front, 72, Line, 1)], 120 => [at(Back, 36, Line, 1)]],
    rows![
        0 => [at(Screen, 36, Loose, 1)],
        160 => [at(Front, 120, Line, 1), at(Elite, 72, Column, 1), at(Front, 120, Line, 1)],
        160 => [at(Back, 72, Line, 2)],
    ],
    rows![
        0 => [at(Screen, 36, Loose, 4)],
        160 => [at(Front, 72, Line, 4), at(Elite, 120, Column, 1), at(Front, 72, Line, 4)],
        160 => [at(Back, 72, Line, 4)],
    ],
];

const CLANS: [&[Row]; 3] = [
    rows![0 => [at(Front, 72, Loose, 1)], 120 => [at(Back, 36, Loose, 1)]],
    rows![
        0 => [at(Screen, 36, Loose, 2)],
        160 => [at(Wing, 40, Loose, 1), at(Front, 72, Loose, 3), at(Wing, 40, Loose, 1)],
        160 => [at(Back, 72, Loose, 1)],
    ],
    rows![
        0 => [at(Screen, 72, Loose, 3)],
        160 => [at(Wing, 90, Loose, 1), at(Front, 120, Loose, 6), at(Wing, 90, Loose, 1)],
        160 => [at(Back, 120, Loose, 6)],
    ],
];

const RIDERS: [&[Row]; 3] = [
    rows![0 => [at(Front, 40, Wedge, 1)], 120 => [at(Back, 40, Thin, 1)]],
    rows![
        0 => [at(Screen, 36, Loose, 2)],
        160 => [at(Wing, 40, Wedge, 1), at(Front, 90, Wedge, 2), at(Wing, 40, Wedge, 1)],
        160 => [at(Back, 90, Thin, 1)],
    ],
    rows![
        0 => [at(Screen, 72, Loose, 3)],
        160 => [at(Wing, 90, Wedge, 2), at(Front, 90, Wedge, 4), at(Wing, 90, Wedge, 2)],
        160 => [at(Back, 90, Thin, 4)],
    ],
];

/// One line of the roster: the unit type of a role, and whether it stands loose whatever the
/// doctrine says (it has no formation: skirmishers, militia, some riders).
type Slot = (Role, &'static str, bool);

/// A nation's army in one century: its doctrine and the unit type of each role. A role the
/// nation does not have (no cavalry, no guns) leaves its bodies out.
pub struct Roster {
    /// The index in [`ENTRIES`].
    pub nation: usize,
    /// `None` for a nation with one army (the natives).
    pub era: Option<Era>,
    pub doctrine: Doctrine,
    pub roles: &'static [Slot],
}

const fn s(role: Role, id: &'static str) -> Slot {
    (role, id, false)
}

const fn l(role: Role, id: &'static str) -> Slot {
    (role, id, true)
}

/// Every army, per nation and century. The type ids are those of the nations' files; a role
/// is filled by the nearest unit of the nation's list (the sources are in docs/controls.md).
#[rustfmt::skip]
pub const ROSTERS: &[Roster] = &[
    // England: the New Model Army, Marlborough, Wellington.
    Roster { nation: 2, era: Some(Era::C17), doctrine: Doctrine::PikeShot, roles: &[
        s(Front, "EngMus(EN)"), s(Pike, "EngPik(EN)"), l(Screen, "EngTap(EN)"), s(Wing, "EngD17(EN)"), l(Guns, "ArtPus(EN)")] },
    Roster { nation: 2, era: Some(Era::C18), doctrine: Doctrine::Linear, roles: &[
        s(Front, "EngM18(EN)"), l(Screen, "EngTap(EN)"), s(Wing, "EngD18(EN)"), l(Guns, "ArtPus(EN)")] },
    Roster { nation: 2, era: Some(Era::C19), doctrine: Doctrine::Lines19, roles: &[
        s(Front, "EngM18(EN)"), l(Screen, "EngTap(EN)"), s(Wing, "EngD18(EN)"), l(Guns, "ArtPus(EN)")] },
    // Spain: the tercios, Almansa, Bailen.
    Roster { nation: 1, era: Some(Era::C17), doctrine: Doctrine::PikeShot, roles: &[
        s(Front, "SpnMus(SP)"), s(Pike, "SpnAlb(SP)"), l(Screen, "SpnS15(SP)"), s(Wing, "SpnD17(SP)"), l(Guns, "ArtPus(SP)")] },
    Roster { nation: 1, era: Some(Era::C18), doctrine: Doctrine::Linear, roles: &[
        s(Front, "SpnM18(SP)"), l(Screen, "SpnS15(SP)"), s(Wing, "SpnD18(SP)"), l(Guns, "ArtPus(SP)")] },
    Roster { nation: 1, era: Some(Era::C19), doctrine: Doctrine::Column, roles: &[
        s(Front, "SpnM18(SP)"), l(Screen, "SpnS15(SP)"), s(Wing, "SpnD18(SP)"), l(Guns, "ArtPus(SP)")] },
    // France: Rocroi, Fontenoy, the Grande Armee.
    Roster { nation: 3, era: Some(Era::C17), doctrine: Doctrine::PikeShot, roles: &[
        s(Front, "FrnS17(FR)"), s(Pike, "FrnPik(FR)"), l(Screen, "FrnBuk(FR)"), s(Wing, "FrnD17(FR)"), l(Guns, "ArtPus(FR)")] },
    Roster { nation: 3, era: Some(Era::C18), doctrine: Doctrine::Linear, roles: &[
        s(Front, "FrnM18(FR)"), l(Screen, "FrnBuk(FR)"), s(Wing, "FrnD18(FR)"), l(Guns, "ArtPus(FR)")] },
    Roster { nation: 3, era: Some(Era::C19), doctrine: Doctrine::Column, roles: &[
        s(Front, "FrnM18(FR)"), l(Screen, "FrnBuk(FR)"), s(Wing, "FrnD18(FR)"), l(Guns, "ArtPus(FR)")] },
    // USA: Saratoga, New Orleans.
    Roster { nation: 4, era: Some(Era::C18), doctrine: Doctrine::Linear, roles: &[
        s(Front, "UsaM18(US)"), l(Militia, "UsaMil(US)"), l(Screen, "EngTap(US)"), s(Wing, "UsaD18(US)"), l(Guns, "ArtPus(US)")] },
    Roster { nation: 4, era: Some(Era::C19), doctrine: Doctrine::Lines19, roles: &[
        s(Front, "UsaM18(US)"), l(Militia, "UsaMil(US)"), l(Screen, "EngTap(US)"), s(Wing, "UsaD18(US)"), l(Guns, "ArtPus(US)")] },
    // Mesoamerica: Maya, Inka, Aztecs.
    Roster { nation: 0, era: None, doctrine: Doctrine::Warband, roles: &[
        s(Front, "MayPik(MA)"), s(Elite, "MayVoi(MA)"), s(Back, "MayLut(MA)")] },
    Roster { nation: 5, era: None, doctrine: Doctrine::Warband, roles: &[
        s(Front, "IncPik(IN)"), s(Elite, "IncPik(IN)"), s(Back, "IncLut(IN)"), l(Screen, "IncPra(IN)")] },
    Roster { nation: 6, era: None, doctrine: Doctrine::Warband, roles: &[
        s(Front, "AtcMet(AC)"), s(Elite, "AtcMet(AC)"), s(Back, "AtcLut(AC)")] },
    // The Southwest: Pueblos with captured arms and horses.
    Roster { nation: 7, era: None, doctrine: Doctrine::Clans, roles: &[
        l(Front, "PueS15(PU)"), s(Back, "PueLut(PU)"), l(Wing, "PueVsdA(PU)")] },
    // Woodland: Delaware, Huron, Iroquois.
    Roster { nation: 8, era: None, doctrine: Doctrine::Clans, roles: &[
        l(Front, "DelPik(DE)"), l(Back, "DelLut(DE)"), l(Screen, "DelS15(DE)")] },
    Roster { nation: 9, era: None, doctrine: Doctrine::Clans, roles: &[
        l(Front, "IroTam(HU)"), l(Back, "HurLut(HU)"), l(Screen, "HurS15(HU)")] },
    Roster { nation: 10, era: None, doctrine: Doctrine::Clans, roles: &[
        l(Front, "IroTam(IR)"), l(Back, "IroLut(IR)"), l(Screen, "IroStr(IR)")] },
    // Plains: the Sioux.
    Roster { nation: 11, era: None, doctrine: Doctrine::Riders, roles: &[
        s(Front, "SiuKPik(SI)"), s(Back, "SiuKLut(SI)"), l(Screen, "SiuLut(SI)"), s(Wing, "SiuKStr(SI)")] },
];

/// The centuries a nation's armies are offered in, oldest first; empty for nations with one
/// army (the natives) and for the animals and cannons.
pub fn eras(nation: usize) -> Vec<Era> {
    ROSTERS.iter().filter(|r| r.nation == nation).filter_map(|r| r.era).collect()
}

/// The century a menu starts on: the 18th, where the original's units are most numerous.
pub fn default_era(nation: usize) -> Option<Era> {
    let all = eras(nation);
    all.iter().copied().find(|&e| e == Era::C18).or_else(|| all.first().copied())
}

/// Why a preset was not placed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The list (animals, cannons) has no armies.
    NoArmy(usize),
    /// A century was asked for that the nation has no army in, or none where it needs one.
    Era(usize, Option<Era>),
    /// A unit type of the roster is not in the catalog.
    Type(&'static str),
    /// The unit type has no such formation, or none of that size.
    Formation(&'static str, &'static str, u16),
    /// A body would leave the map.
    OffMap,
    /// A body would stand in water.
    Water,
    /// The formations could not be placed.
    Place(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NoArmy(_) => write!(f, "Keine Armee fuer diese Liste"),
            Error::Era(..) => write!(f, "Dieses Jahrhundert gibt es nicht"),
            Error::Type(id) => write!(f, "Unbekannter Typ {id}"),
            Error::Formation(id, group, men) => write!(f, "{id}: keine Formation {group} mit {men}"),
            Error::OffMap => write!(f, "Armee ragt ueber den Kartenrand"),
            Error::Water => write!(f, "Armee steht im Wasser"),
            Error::Place(e) => f.write_str(e),
        }
    }
}

impl std::error::Error for Error {}

/// The roster of a nation and century.
pub fn roster(nation: usize, era: Option<Era>) -> Result<&'static Roster, Error> {
    if nation >= FIRST_EXTRA {
        return Err(Error::NoArmy(nation));
    }
    ROSTERS.iter().find(|r| r.nation == nation && r.era == era).ok_or(Error::Era(nation, era))
}

/// One body of an army, laid out: where its centre stands (in the army's frame, forward and
/// to the right-hand side, world pixels) and how it is turned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub role: Role,
    pub kind: TypeId,
    /// Soldiers of a formation, or the loose men.
    pub men: usize,
    /// The formation and its shape, or `None` for a loose block.
    pub form: Option<(FormChoice, usize)>,
    pub forward: i32,
    pub side: i32,
    /// The row of the roster the body stands in, 0 the front row.
    pub row: u8,
    /// The facing relative to the army's, in 1/256 turns.
    pub turn: u8,
    /// What the body covers around its centre: forward, backward, left and right edges
    /// relative to the army's frame.
    pub reach: Reach,
}

/// The extent of a body around its centre, world pixels, in the army's frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reach {
    pub front: i32,
    pub back: i32,
    pub left: i32,
    pub right: i32,
}

impl Block {
    /// The block's centre on the map for an army at `at` facing `facing`.
    pub fn spot(&self, at: (i32, i32), facing: u8) -> (i32, i32) {
        let (x, y) = frame(facing, self.forward, self.side);
        (at.0 + x, at.1 + y)
    }

    /// The facing of the block's soldiers.
    pub fn dir(&self, facing: u8) -> u8 {
        facing.wrapping_add(self.turn)
    }

    /// The gap of a loose block.
    fn gap(world: &World, kind: TypeId, role: Role) -> i32 {
        let r = world.catalog.get(kind).radius2;
        if role == Role::Guns { r * 2 + 20 } else { (r * 4).max(32) }
    }
}

/// The army's frame (forward `f`, right-hand `s`) in world offsets for an army facing `dir`.
fn frame(dir: u8, f: i32, s: i32) -> (i32, i32) {
    let a = f64::from(dir) * std::f64::consts::TAU / 256.0;
    let (c, sn) = (a.cos(), a.sin());
    let (f, s) = (f64::from(f), f64::from(s));
    ((f * c - s * sn).round() as i32, (f * sn + s * c).round() as i32)
}

/// The slot (and commander) positions of a shape turned `turn` in the army's frame: forward is
/// +x, the right-hand side +y (1/16 px).
fn shape_reach(world: &World, shape: usize, turn: u8) -> Reach {
    let sh = &world.formations.shapes[shape];
    let mut pts = sh.positions(0, 0, turn);
    pts.extend((0..sh.commanders.len()).map(|i| sh.commander_position(i, 0, 0, turn)));
    reach_of(pts.iter().map(|&(x, y)| (x / 16, y / 16)), SLOT_MARGIN)
}

fn reach_of(points: impl Iterator<Item = (i32, i32)>, margin: i32) -> Reach {
    let mut r = Reach { front: i32::MIN, back: i32::MAX, left: i32::MAX, right: i32::MIN };
    for (x, y) in points {
        r.front = r.front.max(x);
        r.back = r.back.min(x);
        r.left = r.left.min(y);
        r.right = r.right.max(y);
    }
    Reach { front: r.front + margin, back: r.back - margin, left: r.left - margin, right: r.right + margin }
}

/// The formation of `kind` in group `group` with exactly `men` soldiers.
fn pick_shape(
    world: &World,
    kind: TypeId,
    id: &'static str,
    form: Form,
    men: u16,
) -> Result<(FormChoice, usize), Error> {
    let group = form.group().unwrap_or("");
    let err = || Error::Formation(id, group, men);
    let choice = formations(world, kind)
        .into_iter()
        .find(|c| world.formations.groups.get(c.group).is_some_and(|g| g.name.eq_ignore_ascii_case(group)))
        .ok_or_else(err)?;
    let shape = choice.shapes.iter().copied().find(|&s| world.formations.shapes[s].slots() == usize::from(men));
    shape.map(|s| (choice, s)).ok_or_else(err)
}

/// Lays out the bodies of the preset around the army's centre (front row first).
pub fn blocks(world: &World, nation: usize, size: Size, era: Option<Era>) -> Result<Vec<Block>, Error> {
    let roster = roster(nation, era)?;
    // The bodies of each row, as a copy of one body per `n`: (block, copies).
    let mut rows: Vec<(i32, Vec<(Block, u8)>)> = Vec::new();
    for row in roster.doctrine.rows(size) {
        let mut items = Vec::new();
        for e in row.entries {
            let Some(&(_, id, loose)) = roster.roles.iter().find(|r| r.0 == e.role) else { continue };
            let kind = world.catalog.find(id).ok_or(Error::Type(id))?;
            let (form, reach) = match e.form.group().filter(|_| !loose) {
                Some(_) => {
                    let (choice, shape) = pick_shape(world, kind, id, e.form, e.men)?;
                    let reach = shape_reach(world, shape, 0);
                    (Some((choice, shape)), reach)
                }
                None => {
                    let gap = Block::gap(world, kind, e.role);
                    let pts = grid(usize::from(e.men), (0, 0), 0, gap);
                    (None, reach_of(pts.into_iter(), gap / 2))
                }
            };
            let block = Block {
                role: e.role,
                kind,
                men: usize::from(e.men),
                form,
                forward: 0,
                side: 0,
                row: 0,
                turn: 0,
                reach,
            };
            items.push((block, e.n));
        }
        if !items.is_empty() {
            rows.push((if rows.is_empty() { 0 } else { row.gap }, items));
        }
    }
    // Depth of each row, front to back with its gap; the army's depth centres on zero.
    let depth = |items: &[(Block, u8)]| items.iter().map(|(b, _)| b.reach.front - b.reach.back).max().unwrap_or(0);
    let total: i32 = rows.iter().map(|(gap, items)| gap + depth(items)).sum();
    let mut front = total / 2;
    let mut out = Vec::new();
    for (row, (gap, items)) in rows.into_iter().enumerate() {
        front -= gap;
        let width: i32 = items.iter().map(|(b, n)| i32::from(*n) * (b.reach.right - b.reach.left)).sum::<i32>()
            + FORMATION_GAP * (items.iter().map(|(_, n)| i32::from(*n)).sum::<i32>() - 1);
        let mut left = -width / 2;
        let row_depth = depth(&items);
        for (b, n) in items {
            for _ in 0..n {
                let mut b = b.clone();
                // The body's front edge on the row's front, its left edge at the cursor.
                b.forward = front - b.reach.front;
                b.side = left - b.reach.left;
                b.row = row as u8;
                left += b.reach.right - b.reach.left + FORMATION_GAP;
                out.push(b);
            }
        }
        front -= row_depth;
    }
    Ok(out)
}

/// A block's centre is the formation's own origin, so a block's covered rectangle on the map
/// is `reach` around it; this checks that all of it stands on the map and on land.
fn check_bounds(world: &World, b: &Block, at: (i32, i32), facing: u8) -> Result<(), Error> {
    let side = MapSize { shift: world.map.shift }.playable_size();
    let (c, dir) = (b.spot(at, facing), b.dir(facing));
    let r = b.reach;
    let corners = [(r.front, r.left), (r.front, r.right), (r.back, r.left), (r.back, r.right), (0, 0)];
    for (f, s) in corners {
        let (x, y) = frame(dir, f, s);
        let (x, y) = (c.0 + x, c.1 + y);
        if x < MAP_MARGIN || y < MAP_MARGIN || x > side - MAP_MARGIN || y > side - MAP_MARGIN {
            return Err(Error::OffMap);
        }
        if !world.sea_allows(b.kind, Pos::from_pixels(x, y)) {
            return Err(Error::Water);
        }
    }
    Ok(())
}

/// Places the army of `nation` (an index in [`ENTRIES`]) in century `era` and `size` for
/// player `owner`, centred on world pixel `at` and facing `facing` (0 east, 64 south, ...).
/// Returns the units placed per body, in the order of [`blocks`] (soldiers, officers, drummers
/// and flag bearers). Nothing is placed when any body would leave the map or stand in water. Upgrade levels are left as
/// they are; the caller sets the alliances (`Editor::apply_sides`) afterwards.
pub fn place_preset(
    world: &mut World,
    nation: usize,
    size: Size,
    era: Option<Era>,
    owner: u8,
    at: (i32, i32),
    facing: u8,
) -> Result<Vec<Vec<EntityId>>, Error> {
    let plan = blocks(world, nation, size, era)?;
    for b in &plan {
        check_bounds(world, b, at, facing)?;
    }
    let mut placed = Vec::new();
    for b in &plan {
        let (c, dir) = (b.spot(at, facing), b.dir(facing));
        match &b.form {
            Some((choice, _)) => {
                let (_, ids) = place_formations(world, b.kind, choice, b.men, owner, c, dir).map_err(Error::Place)?;
                placed.push(ids);
            }
            None => {
                let gap = Block::gap(world, b.kind, b.role);
                let ids = grid(b.men, c, dir, gap)
                    .into_iter()
                    .map(|p| world.spawn(b.kind, owner, Pos::from_pixels(p.0, p.1), dir))
                    .collect();
                placed.push(ids);
            }
        }
    }
    Ok(placed)
}

/// What a click on the army menu does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Act {
    /// Opens or closes the menu (a right click on the nation combo box).
    Toggle,
    Era(Era),
    /// Arms a size: the next left click on the map places the army.
    Size(Size),
}

/// The army menu's state.
#[derive(Debug, Clone, Default)]
pub struct Menu {
    pub open: bool,
    era: Option<Era>,
    /// The size the next click on the map places.
    pub armed: Option<Size>,
}

impl Menu {
    /// The century in use for `nation`: the chosen one if the nation has it, else the default.
    pub fn era_for(&self, nation: usize) -> Option<Era> {
        let all = eras(nation);
        self.era.filter(|e| all.contains(e)).or_else(|| default_era(nation))
    }
}

impl Editor {
    /// A click on the army menu, with the nation shown in the combo box.
    pub(super) fn army_act(&mut self, act: Act) {
        if self.entry >= FIRST_EXTRA {
            self.army.open = false;
            self.status = b"Keine Armeen fuer diese Liste".to_vec();
            return;
        }
        match act {
            Act::Toggle => self.army.open = !self.army.open,
            Act::Era(e) => self.army.era = Some(e),
            Act::Size(s) => {
                self.army.open = false;
                self.army.armed = Some(s);
                self.pick = None;
                self.status = format!("Armee {}: Karte anklicken", s.label()).into_bytes();
            }
        }
    }

    /// The menu's lines under the combo box, over the icons.
    pub(super) fn army_items(&self) -> Vec<Item> {
        if !self.army.open || self.entry >= FIRST_EXTRA {
            return Vec::new();
        }
        let mut out = Vec::new();
        let mut y = OY + 36 + 22;
        let mut line = |text: String, name: String, hot: bool, act: Option<EditorAct>| {
            let look = if act.is_some() { Look::DropRow } else { Look::Text };
            let rect = Rect::new(OX + 10, y, 224, ROW);
            out.push(Item { rect, text: text.into_bytes(), name, hot, act, look, gp: None });
            y += ROW;
        };
        line(format!("Armee: {}", ENTRIES[self.entry].0), String::new(), false, None);
        let current = self.army.era_for(self.entry);
        let all = eras(self.entry);
        if all.len() > 1 {
            for e in all {
                let act = Some(EditorAct::Army(Act::Era(e)));
                line(e.label().to_owned(), format!("Armee {}", e.label()), current == Some(e), act);
            }
        }
        for s in Size::ALL {
            let act = Some(EditorAct::Army(Act::Size(s)));
            line(s.label().to_owned(), format!("Armee {}", s.label()), self.army.armed == Some(s), act);
        }
        out
    }

    /// Places army `size` of `nation` for [`super::Input::Preset`]; the text for the status line.
    pub(super) fn place_army(
        &self,
        world: &mut World,
        (nation, size, era): (usize, Size, Option<Era>),
        owner: u8,
        at: (i32, i32),
        dir: u8,
    ) -> Vec<u8> {
        let result = place_preset(world, nation, size, era, owner, at, dir);
        self.apply_sides(world);
        match result {
            Ok(bodies) => {
                let n: usize = bodies.iter().map(Vec::len).sum();
                format!("Armee {}: {n} Mann", size.label()).into_bytes()
            }
            Err(e) => e.to_string().into_bytes(),
        }
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    /// The three sizes of every doctrine have the bodies the sizes promise: Klein 2, Mittel
    /// 5, Gross 10 to 14 (screens, guns and militia not counted).
    #[test]
    fn doctrines_have_the_promised_bodies() {
        let bodies = |rows: &[Row]| {
            rows.iter()
                .flat_map(|r| r.entries)
                .filter(|e| !matches!(e.role, Screen | Guns | Militia))
                .map(|e| usize::from(e.n))
                .sum::<usize>()
        };
        for d in [
            Doctrine::PikeShot,
            Doctrine::Linear,
            Doctrine::Column,
            Doctrine::Lines19,
            Doctrine::Warband,
            Doctrine::Clans,
            Doctrine::Riders,
        ] {
            let n: Vec<usize> = Size::ALL.iter().map(|&s| bodies(d.rows(s))).collect();
            assert_eq!(n[0], 2, "{d:?} Klein");
            assert!((4..=6).contains(&n[1]), "{d:?} Mittel {}", n[1]);
            assert!((10..=14).contains(&n[2]), "{d:?} Gross {}", n[2]);
        }
    }

    #[test]
    fn every_nation_of_the_original_has_an_army_and_the_extra_lists_none() {
        for (n, (name, _)) in ENTRIES.iter().enumerate() {
            let has = ROSTERS.iter().any(|r| r.nation == n);
            assert_eq!(has, n < FIRST_EXTRA, "{name}");
        }
        assert_eq!(eras(2), [Era::C17, Era::C18, Era::C19]);
        assert_eq!(eras(4), [Era::C18, Era::C19]);
        assert!(eras(11).is_empty() && eras(12).is_empty());
        assert_eq!(default_era(2), Some(Era::C18));
        assert_eq!(default_era(0), None);
    }

    #[test]
    fn roster_type_ids_carry_the_nations_prefix() {
        for r in ROSTERS {
            for (_, id, _) in r.roles {
                assert!(
                    id.starts_with("ArtPus")
                        || id.starts_with("EngTap")
                        || id.starts_with("IroTam")
                        || id.to_ascii_lowercase().starts_with(&ENTRIES[r.nation].1.to_ascii_lowercase()),
                    "{id} in {}",
                    ENTRIES[r.nation].0
                );
            }
        }
    }

    #[test]
    fn a_missing_century_or_list_is_an_error() {
        assert_eq!(roster(2, None).err(), Some(Error::Era(2, None)));
        assert_eq!(roster(11, Some(Era::C18)).err(), Some(Error::Era(11, Some(Era::C18))));
        assert_eq!(roster(4, Some(Era::C17)).err(), Some(Error::Era(4, Some(Era::C17))));
        assert_eq!(roster(13, None).err(), Some(Error::NoArmy(13)));
    }

    /// The game's catalog and formations on an open map (shift 1, 15360 px a side); needs the
    /// original game files, so the tests that use it are ignored without `ACR_GAME_DIR`.
    pub(in crate::ingame::editor) fn world() -> World {
        use acr_formats::Vfs;
        use acr_sim::assets::SimAssets;
        use std::sync::Arc;
        let dir = std::env::var_os("ACR_GAME_DIR").expect("ACR_GAME_DIR");
        let mut vfs = Vfs::open_game_dir(std::path::Path::new(&dir)).unwrap();
        let paths: Vec<String> = vfs.list().map(|(p, _)| p.to_owned()).collect();
        let mut assets = SimAssets::default();
        let mut queue = assets.start(&paths);
        while let Some(p) = queue.pop() {
            let data = vfs.read(&p).ok();
            queue.extend(assets.add_file(&p, data.as_deref()));
        }
        let data = assets.build();
        World::new(Arc::new(acr_sim::MapGrid::open(1)), Arc::new(data.catalog), Arc::new(data.formations))
    }

    /// Every roster, in every century and size: the types exist, the formations have the
    /// sizes asked for, and the army places inside the map without two units overlapping
    /// (the distance of two units is at least 0.9 times the sum of their radii: the engine's
    /// diagonal facings draw a formation about 6 % tighter), facing every way.
    #[test]
    #[ignore = "needs the original game files (ACR_GAME_DIR)"]
    fn every_preset_places_inside_the_map_without_overlap() {
        let world = world();
        let side = MapSize { shift: 1 }.playable_size();
        let centre = (side / 2, side / 2);
        for r in ROSTERS {
            for size in Size::ALL {
                let plan = blocks(&world, r.nation, size, r.era).unwrap();
                assert!(!plan.is_empty());
                for facing in (0..=255u8).step_by(32) {
                    let mut w = world.clone();
                    let name = format!("{} {:?} {size:?} facing {facing}", ENTRIES[r.nation].0, r.era);
                    let ids = place_preset(&mut w, r.nation, size, r.era, 0, centre, facing).expect(&name);
                    assert_eq!(ids.len(), plan.len(), "{name}");
                    for (b, ids) in plan.iter().zip(&ids) {
                        let soldiers = ids.iter().filter(|id| w.units[id.0 as usize].kind == b.kind).count();
                        assert_eq!(soldiers, b.men, "{name}: {:?}", b.role);
                    }
                    let units: Vec<_> = w.units.iter().filter(|u| u.alive).collect();
                    let inside = |v: i32| (MAP_MARGIN..=side - MAP_MARGIN).contains(&v);
                    for (i, a) in units.iter().enumerate() {
                        let (ax, ay) = (a.pos.x >> 4, a.pos.y >> 4);
                        let id = &w.catalog.get(a.kind).id;
                        assert!(inside(ax) && inside(ay), "{name}: {id} at {ax},{ay} off the map");
                        let ra = w.catalog.get(a.kind).radius2;
                        for b in &units[i + 1..] {
                            let (dx, dy) = (ax - (b.pos.x >> 4), ay - (b.pos.y >> 4));
                            let rb = w.catalog.get(b.kind).radius2;
                            let other = &w.catalog.get(b.kind).id;
                            assert!(
                                100 * i64::from(dx * dx + dy * dy) >= 81 * i64::from((ra + rb) * (ra + rb)),
                                "{name}: {id} and {other} overlap at {ax},{ay} (d {dx},{dy} r {ra}+{rb})"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    #[ignore = "needs the original game files (ACR_GAME_DIR)"]
    fn a_preset_near_the_edge_places_nothing() {
        let mut w = world();
        let side = MapSize { shift: 1 }.playable_size();
        let before = w.units.len();
        let e = place_preset(&mut w, 2, Size::Large, Some(Era::C18), 0, (side - 400, side / 2), 0);
        assert_eq!(e, Err(Error::OffMap));
        assert_eq!(w.units.len(), before);
        assert_eq!(place_preset(&mut w, 12, Size::Small, None, 0, (side / 2, side / 2), 0), Err(Error::NoArmy(12)));
        assert_eq!(w.units.len(), before);
    }

    /// Prints each preset's soldiers, bodies and extent (for docs/controls.md).
    #[test]
    #[ignore = "needs the original game files (ACR_GAME_DIR); prints with --nocapture"]
    fn print_the_presets() {
        let world = world();
        for r in ROSTERS {
            for size in Size::ALL {
                let plan = blocks(&world, r.nation, size, r.era).unwrap();
                let men: usize = plan.iter().map(|b| b.men).sum();
                let bodies = plan.iter().filter(|b| !matches!(b.role, Screen | Guns | Militia)).count();
                let guns: usize = plan.iter().filter(|b| b.role == Guns).map(|b| b.men).sum();
                let f = plan.iter().map(|b| b.forward + b.reach.front).max().unwrap();
                let bk = plan.iter().map(|b| b.forward + b.reach.back).min().unwrap();
                let lf = plan.iter().map(|b| b.side + b.reach.left).min().unwrap();
                let rt = plan.iter().map(|b| b.side + b.reach.right).max().unwrap();
                eprintln!(
                    "PRESET {} {:?} {size:?}: {men} units ({guns} guns), {bodies} bodies, {} deep x {} wide",
                    ENTRIES[r.nation].0,
                    r.era,
                    f - bk,
                    rt - lf
                );
            }
        }
    }
}
