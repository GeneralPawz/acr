//! Object types: the parts of the `.MD` definitions the simulation uses, compiled into
//! plain integers once so the per-tick code never looks at text.

use std::collections::BTreeMap;

use acr_formats::defs::{Animation, UnitDef};

/// Index of an [`ObjectType`] in the [`Catalog`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TypeId(pub u16);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Medium {
    Land,
    /// `MEDIA WATER`: ships.
    Water,
}

/// `MOTIONSTYLE` (absent = [`MotionStyle::Default`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MotionStyle {
    Default,
    FastRotateAndMove,
    SlowRotate,
    ComplexRotate,
    RotateAndMove,
    Ships,
    NewShips,
}

impl MotionStyle {
    fn parse(word: &str) -> Self {
        match word.to_ascii_uppercase().as_str() {
            "FASTROTATE&MOVE" => Self::FastRotateAndMove,
            "SLOWROTATE" => Self::SlowRotate,
            "COMPLEXROTATE" => Self::ComplexRotate,
            "ROTATE&MOVE" => Self::RotateAndMove,
            "SHEEPS" => Self::Ships,
            "NEWSHEEPS" => Self::NewShips,
            _ => Self::Default,
        }
    }
}

/// Playback data of one animation slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct AnimInfo {
    /// Stored directions (1, 9, 16 or 65).
    pub directions: u8,
    /// Frames per direction.
    pub frames: u16,
}

impl AnimInfo {
    fn of(a: Option<&Animation>) -> Option<Self> {
        let a = a?;
        Some(Self { directions: a.directions.clamp(0, 255) as u8, frames: a.frame_refs().len().min(0xFFFF) as u16 })
    }
}

/// One unit or building type.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ObjectType {
    /// Nation member id, e.g. `EngMus(EN)` (what maps reference).
    pub id: String,
    /// `.MD` `NAME`, e.g. `EngMus`.
    pub md_name: String,
    pub building: bool,
    pub medium: Medium,
    pub life: i32,
    /// `GEOMETRY r1 r2 step`: the two radii (world pixels) and the motion step.
    pub radius1: i32,
    pub radius2: i32,
    pub step: i32,
    /// `UNITRADIUS` (default 16; used by projectile hit tests, not by motion).
    pub unit_radius: i32,
    /// `ROTATE`: turn per decision step (`MinRotator`, NM+0x2932, default 16).
    pub rotate: i32,
    /// `STOPDISTANCE` in 1/16 px (NM+0x176d, default 256 = one 16 px cell).
    pub stop_distance: i32,
    pub motion_style: MotionStyle,
    pub officer: bool,
    pub drummer: bool,
    pub peasant: bool,
    /// Buildings: `LOCATION a b` (picture offset); the lock base cell is
    /// `((RealX + 16a) >> 8, (RealY + 32b) >> 8)` (0x516fde).
    pub location: (i32, i32),
    /// Buildings: `LOCKPOINTS`, 16 px cells relative to the lock base cell. The engine
    /// stores them as bytes (NM+0x16c6), so only the low 8 bits of each value count.
    pub lock_points: Vec<(i32, i32)>,
    pub stand: Option<AnimInfo>,
    pub motion_l: Option<AnimInfo>,
    pub motion_r: Option<AnimInfo>,
    pub rest: Option<AnimInfo>,
    pub rest1: Option<AnimInfo>,
}

impl ObjectType {
    pub fn from_def(id: &str, def: &UnitDef) -> Self {
        let geometry = def.get("GEOMETRY");
        let g = |i| geometry.and_then(|c| c.int(i)).unwrap_or(0);
        let int = |k: &str, default| def.get(k).and_then(|c| c.int(0)).unwrap_or(default);
        Self {
            id: id.to_owned(),
            md_name: def.name().unwrap_or_default().to_owned(),
            building: def.is_building(),
            medium: match def.get("MEDIA").and_then(|c| c.word(0)) {
                Some(w) if w.eq_ignore_ascii_case("WATER") => Medium::Water,
                _ => Medium::Land,
            },
            life: def.life().unwrap_or(0),
            radius1: g(0),
            radius2: g(1),
            step: g(2),
            unit_radius: int("UNITRADIUS", 16),
            rotate: int("ROTATE", 16),
            stop_distance: int("STOPDISTANCE", 256),
            motion_style: def
                .get("MOTIONSTYLE")
                .and_then(|c| c.word(0))
                .map_or(MotionStyle::Default, MotionStyle::parse),
            officer: def.has("OFFICER"),
            drummer: def.has("BARABAN"),
            peasant: def.has("PEASANT"),
            location: def.get("LOCATION").map_or((0, 0), |c| (c.int(0).unwrap_or(0), c.int(1).unwrap_or(0))),
            lock_points: def
                .get("LOCKPOINTS")
                .map(|c| c.points(0).into_iter().map(|(x, y)| (i32::from(x as u8), i32::from(y as u8))).collect())
                .unwrap_or_default(),
            stand: AnimInfo::of(def.animation("STAND")),
            motion_l: AnimInfo::of(def.animation("MOTION_L")),
            motion_r: AnimInfo::of(def.animation("MOTION_R")),
            rest: AnimInfo::of(def.animation("REST")),
            rest1: AnimInfo::of(def.animation("REST1")),
        }
    }

    /// Whether objects of this type ever move.
    pub fn mobile(&self) -> bool {
        !self.building && self.step > 0
    }

    /// `OneStepDX[dir]` (NM+0x1849): distance per animation frame in 1/16 px, `TCos·step >> 8`.
    pub fn step_dx(&self, dir: u8) -> i32 {
        (crate::trig::cos(dir) * self.step) >> 8
    }

    /// `OneStepDY[dir]` (NM+0x1c49).
    pub fn step_dy(&self, dir: u8) -> i32 {
        (crate::trig::sin(dir) * self.step) >> 8
    }

    /// Lock cells of a finished building standing at `pos` (`RelockAll`, 0x44e4f0).
    pub fn lock_cells(&self, pos: crate::Pos) -> impl Iterator<Item = (i32, i32)> + '_ {
        let bx = (pos.x + (self.location.0 << 4)) >> 8;
        let by = (pos.y + ((2 * self.location.1) << 4)) >> 8;
        self.lock_points.iter().map(move |&(x, y)| (bx + x, by + y))
    }

    /// Frames of the walking cycle used for the look-ahead (always `#MOTION_L`).
    pub fn motion_frames(&self) -> i32 {
        self.motion_l.map_or(1, |a| i32::from(a.frames))
    }
}

/// All object types of a match, indexed by [`TypeId`] in insertion order.
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    types: Vec<ObjectType>,
    by_id: BTreeMap<String, TypeId>,
}

impl Catalog {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a type unless one with the same member id (case-insensitive) exists; returns its id.
    pub fn add(&mut self, t: ObjectType) -> TypeId {
        let key = t.id.to_ascii_uppercase();
        if let Some(&id) = self.by_id.get(&key) {
            return id;
        }
        let id = TypeId(u16::try_from(self.types.len()).expect("too many object types"));
        self.types.push(t);
        self.by_id.insert(key, id);
        id
    }

    /// Looks a type up by nation member id (`EngMus(EN)`), ignoring case like the engine.
    pub fn find(&self, id: &str) -> Option<TypeId> {
        self.by_id.get(&id.to_ascii_uppercase()).copied()
    }

    pub fn get(&self, id: TypeId) -> &ObjectType {
        &self.types[usize::from(id.0)]
    }

    pub fn len(&self) -> usize {
        self.types.len()
    }

    pub fn is_empty(&self) -> bool {
        self.types.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (TypeId, &ObjectType)> {
        self.types.iter().enumerate().map(|(i, t)| (TypeId(i as u16), t))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MUSKETEER: &str = "NAME EngMus\r\nUSERLC 0 EngMusG SHADOW -62 -81\r\n@MOTION_L 9 0 0 9\r\n\
        @MOTION_R 9 0 10 19\r\nLIFE 100\r\nGEOMETRY 1 10 40\r\n[END]\r\n";

    #[test]
    fn compiles_motion_fields() {
        let t = ObjectType::from_def("EngMus(EN)", &UnitDef::parse(MUSKETEER.as_bytes()));
        assert_eq!((t.radius1, t.radius2, t.step, t.life), (1, 10, 40, 100));
        assert_eq!(t.motion_l, Some(AnimInfo { directions: 9, frames: 10 }));
        assert!(t.mobile());
    }

    #[test]
    fn building_lock_cells_use_location_and_bytes() {
        let def = UnitDef::parse(
            b"NAME Hut
BUILDING
LOCATION -405 -450 656 637
LOCKPOINTS 2 7 49 260 80
[END]
",
        );
        let t = ObjectType::from_def("Hut(EN)", &def);
        assert_eq!(t.location, (-405, -450));
        // 260 is stored as a byte: 4.
        let cells: Vec<_> = t.lock_cells(crate::Pos::new(6480 + 256 * 10, 14400 + 256 * 20)).collect();
        assert_eq!(cells, [(17, 69), (14, 100)]);
    }

    #[test]
    fn lookup_ignores_case_and_keeps_first() {
        let mut c = Catalog::new();
        let def = UnitDef::parse(MUSKETEER.as_bytes());
        let a = c.add(ObjectType::from_def("EngMus(EN)", &def));
        let b = c.add(ObjectType::from_def("ENGMUS(en)", &def));
        assert_eq!(a, b);
        assert_eq!(c.find("engmus(EN)"), Some(a));
        assert_eq!(c.len(), 1);
    }
}
