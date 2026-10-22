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
    /// `SETACTIVEPOINT #ANIM f ...`: the frame at which a strike hits or a shot leaves.
    pub active: u16,
}

impl AnimInfo {
    fn of(a: Option<&Animation>) -> Option<Self> {
        let a = a?;
        Some(Self {
            directions: a.directions.clamp(0, 255) as u8,
            frames: a.frame_refs().len().min(0xFFFF) as u16,
            active: 0,
        })
    }
}

/// Resources in the engine's order (`NRES.DAT [RESTYPES]`): wood, gold, stone, food, iron,
/// coal. Indices 6 and 7 exist in the stock arrays but are unused.
pub const RESOURCES: [&str; 6] = ["WOOD", "GOLD", "STONE", "FOOD", "IRON", "COAL"];

/// Index of a resource name (`WOOD` = 0 ... `COAL` = 5, or `#Rn` for raw index `n`, e.g. `#R8`
/// for meat sources).
pub fn resource_index(name: &str) -> Option<usize> {
    if let Some(n) = name.strip_prefix("#R").or_else(|| name.strip_prefix("#r")) {
        return n.parse().ok().filter(|&n: &usize| n < 16);
    }
    RESOURCES.iter().position(|r| r.eq_ignore_ascii_case(name))
}

/// One `TAKERESSTAGES RES a b c` entry: gathering source type `a`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TakeRes {
    /// Stance while carrying (`b`).
    pub carry: u8,
    /// Stance while working (`c`): the work animation is `#ATTACK{c-1}`.
    pub work: u8,
    /// Resource credited on delivery (`RES`).
    pub credited: usize,
}

/// Economy data of an object type.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct EconomyDef {
    /// `PRICE n (RES v)*n`, by resource index.
    pub price: [i32; 8],
    /// `COSTPERCENT p` (default 100; the last line wins): each existing object of the type
    /// multiplies the price by `p/100`.
    pub cost_percent: i32,
    /// `LOWCOSTRADIUS r` (px): without another object of the type that close, the base price.
    pub low_cost_radius: i32,
    /// `BUILDSTAGES n` (default 64): construction stages of a building, production time of
    /// a unit (`n/2` ticks).
    pub build_stages: i32,
    /// `BORNPOINTS`, `CONCENTRATOR`, `BUILDPOINTS`: 16 px cells relative to the lock base cell
    /// (stored as signed bytes).
    pub born_points: Vec<(i32, i32)>,
    pub concentrator: Vec<(i32, i32)>,
    pub build_points: Vec<(i32, i32)>,
    /// Housing: `FARM` gives 15, `MFARM n` gives n.
    pub farm: i32,
    /// `RESCONSUMER RES v`: upkeep `v/10000` of the resource per tick.
    pub consumer: Option<(usize, i32)>,
    /// `PRODUCER n RES*n free per_peasant`: mine output.
    pub producer: Option<Producer>,
    /// `PEASANTABSORBER n` / `UNITABSORBER n`: capacity for peasants / other units inside.
    pub peasant_absorber: i32,
    pub unit_absorber: i32,
    pub no_hungry: bool,
    pub peasant_convertor: bool,
    /// `NAEMNIK`: deserts when the player runs out of gold.
    pub mercenary: bool,
    /// `TURNOFF`: stops working when the player runs out of gold.
    pub turn_off: bool,
    /// `BUILDLOCKPOINTS`: cells locked while under construction (bytes, like `LOCKPOINTS`).
    pub build_lock_points: Vec<(i32, i32)>,
    /// `TAKERESSTAGES` by source type (0..16).
    pub take_res: Vec<Option<TakeRes>>,
    /// `PORTION n (RES v)*n`: load carried home, by resource index.
    pub portion: [i32; 8],
    /// `RESOURCEBASE n RES*n`: source types that may be delivered here (bit per type).
    pub resource_base: u16,
}

/// A mine's production (`PRODUCER`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Producer {
    /// Bit per resource index it can mine.
    pub mask: u8,
    pub free: i32,
    pub per_peasant: i32,
}

fn byte_points(c: &acr_formats::defs::Command) -> Vec<(i32, i32)> {
    c.points(0).into_iter().map(|(x, y)| (i32::from(x as i8), i32::from(y as i8))).collect()
}

fn economy(def: &UnitDef) -> EconomyDef {
    let mut e = EconomyDef { cost_percent: 100, build_stages: 64, take_res: vec![None; 16], ..EconomyDef::default() };
    for c in &def.commands {
        match c.keyword.as_str() {
            "PRICE" => {
                e.price = [0; 8];
                for (r, v) in c.word_int_pairs(0) {
                    if let Some(i) = resource_index(r) {
                        e.price[i] = v;
                    }
                }
            }
            "COSTPERCENT" => e.cost_percent = c.int(0).unwrap_or(100),
            "LOWCOSTRADIUS" => e.low_cost_radius = c.int(0).unwrap_or(0),
            "BUILDSTAGES" => e.build_stages = c.int(0).unwrap_or(64),
            "BORNPOINTS" => e.born_points = byte_points(c),
            "CONCENTRATOR" => e.concentrator = byte_points(c),
            "BUILDPOINTS" => e.build_points = byte_points(c),
            "FARM" => e.farm = 15,
            "MFARM" => e.farm = i32::from(c.int(0).unwrap_or(0) as u8),
            "RESCONSUMER" => {
                e.consumer = c.word(0).and_then(resource_index).map(|r| (r, i32::from(c.int(1).unwrap_or(0) as i16)))
            }
            "PRODUCER" => {
                let n = c.int(0).unwrap_or(0).max(0) as usize;
                let mask = (0..n).filter_map(|i| c.word(1 + i).and_then(resource_index)).fold(0u8, |m, r| m | 1 << r);
                e.producer =
                    Some(Producer { mask, free: c.int(1 + n).unwrap_or(0), per_peasant: c.int(2 + n).unwrap_or(0) });
            }
            "PEASANTABSORBER" => e.peasant_absorber = c.int(0).unwrap_or(0),
            "UNITABSORBER" => e.unit_absorber = c.int(0).unwrap_or(0),
            "NO_HUNGRY" => e.no_hungry = true,
            "PEASANTCONVERTOR" => e.peasant_convertor = true,
            "NAEMNIK" => e.mercenary = true,
            "TURNOFF" => e.turn_off = true,
            "BUILDLOCKPOINTS" => {
                e.build_lock_points =
                    c.points(0).into_iter().map(|(x, y)| (i32::from(x as u8), i32::from(y as u8))).collect()
            }
            "TAKERESSTAGES" => {
                if let (Some(res), Some(a), Some(b), Some(w)) =
                    (c.word(0).and_then(resource_index), c.int(1), c.int(2), c.int(3))
                    && (0..16).contains(&a)
                {
                    e.take_res[a as usize] = Some(TakeRes { carry: b as u8, work: w as u8, credited: res });
                }
            }
            "PORTION" => {
                for (r, v) in c.word_int_pairs(0) {
                    if let Some(i) = resource_index(r).filter(|&i| i < 8) {
                        e.portion[i] = i32::from(v as u8);
                    }
                }
            }
            "RESOURCEBASE" => {
                let n = c.int(0).unwrap_or(0).max(0) as usize;
                e.resource_base =
                    (0..n).filter_map(|i| c.word(1 + i).and_then(resource_index)).fold(0, |m, r| m | 1 << r);
            }
            _ => {}
        }
    }
    e
}

/// Number of attack indices (`#ATTACK`, `#ATTACK1`, ... `#ATTACK11`).
pub const ATTACK_SLOTS: usize = 12;

/// Material bits (`MATHERIAL`, `CANKILL`, `ATTMASK`; table 0x4f363b).
pub fn material_bit(name: &str) -> u8 {
    match name.to_ascii_uppercase().as_str() {
        "BODY" => 1,
        "STONE" => 2,
        "WOOD" => 4,
        "IRON" => 8,
        "FLY" => 0x10,
        "BUILDING" => 0x20,
        "WOOD_BUILDING" => 0x40,
        "STENA" => 0x80,
        _ => 0,
    }
}

/// One attack index of an object type; index `i` is stance `i + 1` and animation
/// `#ATTACKi` (`#ATTACK` for 0).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Attack {
    /// `ATTACK_RADIUS i min max` (world pixels); also sets `detect`.
    pub radius: (i32, i32),
    /// `DET_RADIUS i min max`: the range at which this index is chosen.
    pub detect: (i32, i32),
    /// `ADD_ATTACK_RADIUS i v`: extra range at the moment of the hit.
    pub add_radius: i32,
    /// `ATTACK_PAUSE i p`: reload in animation frames.
    pub pause: i32,
    /// `DAMAGE i v` (`MaxDamage`).
    pub damage: i32,
    /// `DAMAGEDEC i d`: shot damage halves every `d` px (default 0xFFFF).
    pub damage_dec: i32,
    /// `WEAPONKIND i KIND` (`NRES.DAT [WEAPONS]`; default `MECH`).
    pub kind: String,
    /// `WEAPON i ID` (`WEAPON.NDS`): the attack shoots.
    pub weapon: Option<String>,
    /// Materials this index may attack (`CANKILL` for all, `ATTMASK i` per index).
    pub mask: u8,
    /// `NOPAUSEDATTACK i`: usable while reloading, and does not reset the reload.
    pub no_pause: bool,
}

impl Default for Attack {
    fn default() -> Self {
        Self {
            radius: (0, 0),
            detect: (0, 0),
            add_radius: 0,
            pause: 0,
            damage: 0,
            damage_dec: 0xFFFF,
            kind: "MECH".into(),
            weapon: None,
            mask: 0,
            no_pause: false,
        }
    }
}

impl Attack {
    /// Whether the index can attack at all (`AttR2 != 0`).
    pub fn usable(&self) -> bool {
        self.radius.1 != 0
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
    /// Attack slots, always [`ATTACK_SLOTS`] long, indexed by attack index.
    pub attacks: Vec<Attack>,
    /// `PROTECTION n (KIND v)*n`: armour per weapon kind; the engine keeps the low byte.
    pub protection: Vec<(String, i32)>,
    /// `MATHERIAL`: own material bits.
    pub materials: u8,
    /// `SEARCH_ENEMY_RADIUS r` / `_SHIFT p` in 1/16 px: the auto-target circle has radius
    /// `search_radius` and its centre lies `search_shift` ahead (0x4fd6b7, 0x4fd743).
    pub search_radius: i32,
    pub search_shift: i32,
    /// `ADDSHOTRADIUS`: extra range attackers get against this object.
    pub add_shot_radius: i32,
    /// `SLOWRECHARGE`: reloads only by playing `#ATTACK3` (musketeers).
    pub slow_recharge: bool,
    /// `SKILLDAMAGEBONUS` / `SKILLDAMAGEMASK`: damage bonus per kill.
    pub skill_bonus: i32,
    pub skill_mask: i32,
    /// `SHIELD`: flat damage reduction.
    pub shield: i32,
    /// `RAZBROS`: shot dispersion.
    pub razbros: i32,
    /// `ZPOINTS src dst`: shot height above ground and aim height.
    pub zpoints: (i32, i32),
    /// `FRIENDLYFIRE`: the unit *avoids* hitting friends.
    pub avoid_friendly_fire: bool,
    /// `NO25`: immune to the 4 % instant kill.
    pub no25: bool,
    /// `USETRANSX i`: stances using `#TRANSX3`/`#TRANS3X` (bit per attack index).
    pub trans_x: u16,
    /// All animations by upper-case name (`#ATTACK1`, `#DEATH`, ...).
    pub anims: BTreeMap<String, AnimInfo>,
    /// `VISION` (0..=8, default 0): the fog reveal pattern.
    pub vision: u8,
    /// Prices, production, housing, mines.
    pub economy: EconomyDef,
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
            attacks: attacks(def),
            protection: def
                .get("PROTECTION")
                .map(|c| c.word_int_pairs(0).into_iter().map(|(k, v)| (k.to_ascii_uppercase(), v & 0xFF)).collect())
                .unwrap_or_default(),
            materials: def.get("MATHERIAL").map_or(0, materials),
            search_radius: search(def).0,
            search_shift: search(def).1,
            add_shot_radius: int("ADDSHOTRADIUS", 0),
            slow_recharge: def.has("SLOWRECHARGE"),
            skill_bonus: int("SKILLDAMAGEBONUS", 0),
            skill_mask: int("SKILLDAMAGEMASK", 0),
            shield: int("SHIELD", 0),
            razbros: int("RAZBROS", 0),
            zpoints: def.get("ZPOINTS").map_or((0, 0), |c| (c.int(0).unwrap_or(0), c.int(1).unwrap_or(0))),
            avoid_friendly_fire: def.has("FRIENDLYFIRE"),
            no25: def.has("NO25"),
            trans_x: def
                .all("USETRANSX")
                .filter_map(|c| c.int(0))
                .filter(|i| (0..16).contains(i))
                .fold(0, |m, i| m | 1 << i),
            anims: anims(def),
            vision: int("VISION", 0).clamp(0, 255) as u8,
            economy: economy(def),
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

fn materials(c: &acr_formats::defs::Command) -> u8 {
    let n = c.int(0).unwrap_or(0).max(0) as usize;
    (0..n).filter_map(|i| c.word(1 + i)).fold(0, |m, w| m | material_bit(w))
}

/// `SEARCH_ENEMY_RADIUS r` then `SEARCH_ENEMY_RADIUS_SHIFT p`, applied in file order.
fn search(def: &UnitDef) -> (i32, i32) {
    let (mut sr, mut ss) = (0, 0);
    for c in &def.commands {
        match c.keyword.as_str() {
            "SEARCH_ENEMY_RADIUS" => sr = c.int(0).unwrap_or(0) << 4,
            "SEARCH_ENEMY_RADIUS_SHIFT" => {
                let p = c.int(0).unwrap_or(0);
                sr = sr * 100 / (p + 100).max(1);
                ss = sr * p / 100;
            }
            _ => {}
        }
    }
    (sr, ss)
}

/// Animations by name, with the active frames of `SETACTIVEPOINT`.
fn anims(def: &UnitDef) -> BTreeMap<String, AnimInfo> {
    let mut m: BTreeMap<String, AnimInfo> = BTreeMap::new();
    for a in &def.animations {
        if let Some(info) = AnimInfo::of(Some(a)) {
            m.insert(a.name.to_ascii_uppercase(), info);
        }
    }
    for c in def.all("SETACTIVEPOINT").chain(def.all("SETACTIVEPOINT2")) {
        if let (Some(name), Some(f)) = (c.word(0), c.int(1))
            && let Some(info) = m.get_mut(&name.to_ascii_uppercase())
        {
            info.active = f.clamp(0, 0xFFFF) as u16;
        }
    }
    m
}

/// The per-attack-index keywords in file order (`ATTACK_RADIUS` overwrites `DET_RADIUS`,
/// `CANKILL` overwrites every `ATTMASK`, as in the loader 0x4f372c).
fn attacks(def: &UnitDef) -> Vec<Attack> {
    let mut v = vec![Attack::default(); ATTACK_SLOTS];
    for c in &def.commands {
        if c.keyword == "CANKILL" {
            let mask = materials(c);
            v.iter_mut().for_each(|a| a.mask = mask);
            continue;
        }
        let Some(i) = c.int(0).and_then(|i| usize::try_from(i).ok()).filter(|&i| i < ATTACK_SLOTS) else { continue };
        let (a1, a2) = (c.int(1).unwrap_or(0), c.int(2).unwrap_or(0));
        let a = &mut v[i];
        match c.keyword.as_str() {
            "ATTACK_RADIUS" => (a.radius, a.detect) = ((a1, a2), (a1, a2)),
            "DET_RADIUS" => a.detect = (a1, a2),
            "ADD_ATTACK_RADIUS" => a.add_radius = a1,
            "ATTACK_PAUSE" => a.pause = a1,
            "DAMAGE" => a.damage = a1,
            "EXDAMAGE" => a.damage = a2,
            "DAMAGEDEC" => a.damage_dec = a1,
            "WEAPONKIND" => a.kind = c.word(1).unwrap_or("MECH").to_ascii_uppercase(),
            "WEAPON" => a.weapon = c.word(1).map(str::to_owned),
            "ATTMASK" => {
                let n = a1.max(0) as usize;
                a.mask = (0..n).filter_map(|k| c.word(2 + k)).fold(0, |m, w| m | material_bit(w));
            }
            "NOPAUSEDATTACK" => a.no_pause = true,
            _ => {}
        }
    }
    v
}

/// All object types of a match, indexed by [`TypeId`] in insertion order.
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    types: Vec<ObjectType>,
    by_id: BTreeMap<String, TypeId>,
    /// `NRES.DAT [WEAPONS]` flags per weapon kind name.
    weapon_flags: BTreeMap<String, u8>,
    /// The nations' tech tree (build lists, upgrades, availability).
    pub tech: crate::tech::Tech,
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

    /// Sets the weapon kinds of `NRES.DAT` (their flags decide range bonuses, instant kills,
    /// formation bonuses).
    pub fn set_weapon_kinds(&mut self, kinds: &[acr_formats::defs::nres::WeaponKind]) {
        self.weapon_flags = kinds.iter().map(|k| (k.name.clone(), k.flags)).collect();
    }

    /// Flags of a weapon kind (`acr_formats::defs::nres::weapon_flags`), 0 if unknown.
    pub fn weapon_flags(&self, kind: &str) -> u8 {
        self.weapon_flags.get(kind).copied().unwrap_or(0)
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
        assert_eq!(t.motion_l, Some(AnimInfo { directions: 9, frames: 10, active: 0 }));
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
    fn collects_attacks_and_protection() {
        let def = UnitDef::parse(
            b"NAME M\r\nUSERLC 0 G SHADOW 0 0\r\n@ATTACK1 9 0 0 67\r\nSETACTIVEPOINT #ATTACK1 13 1 2\r\n\
              DAMAGE 0 15\r\nDAMAGE 1 380\r\nATTACK_RADIUS 0 40 80\r\nATTACK_RADIUS 1 250 850\r\n\
              DET_RADIUS 1 250 7000\r\nATTACK_PAUSE 1 550\r\nWEAPONKIND 1 VISTREL\r\nWEAPON 1 SHOT\r\n\
              CANKILL 1 BODY\r\nMATHERIAL 1 BODY\r\nNOPAUSEDATTACK 0\r\nPROTECTION 2 PIKA 1 VISTREL 300\r\n\
              SEARCH_ENEMY_RADIUS 850\r\nSEARCH_ENEMY_RADIUS_SHIFT 70\r\n[END]\r\n",
        );
        let t = ObjectType::from_def("M(EN)", &def);
        assert_eq!(t.attacks.len(), ATTACK_SLOTS);
        let shot = &t.attacks[1];
        assert_eq!((shot.radius, shot.detect, shot.pause, shot.damage), ((250, 850), (250, 7000), 550, 380));
        assert_eq!((shot.kind.as_str(), shot.weapon.as_deref(), shot.mask), ("VISTREL", Some("SHOT"), 1));
        assert!(t.attacks[0].no_pause && !shot.no_pause && !t.attacks[3].usable());
        assert_eq!(t.protection, [("PIKA".into(), 1), ("VISTREL".into(), 300 & 0xFF)]);
        assert_eq!((t.search_radius, t.search_shift), (8000, 5600));
        assert_eq!(t.anims["#ATTACK1"], AnimInfo { directions: 9, frames: 68, active: 13 });
        assert_eq!(t.materials, 1);
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
