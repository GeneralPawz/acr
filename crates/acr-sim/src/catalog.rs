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
    /// `MaxInside` (NM+0x175a): the capacity of the last of the two keywords.
    pub max_inside: i32,
    /// `FARM` or `MFARM` given: finished objects house units.
    pub is_farm: bool,
    /// `ADDSECTION s a life …`: building stages that exist, and the life each adds once done.
    pub sections: Vec<(u8, i32)>,
    /// `SPRITEOBJECT`: placing the type plants sprites (fields) instead of an object.
    pub sprite_object: bool,
    /// `EXSPRITES name variants n step spread`: the sprites it plants.
    pub ex_sprites: Option<ExSprites>,
    /// `TRANSFORMTOMEAT i`: after its death animation the unit becomes `MEAT` sprite `i`.
    pub meat: Option<u8>,
    /// `USAGE MELNICA`: a mill; finished, it plants its first product (fields) around it.
    pub mill: bool,
    pub no_hungry: bool,
    pub peasant_convertor: bool,
    /// `MAXINDEPO g n` (NM+0x173a = g + 1, NM+0x1737 = n): at most `n` objects of depot
    /// group `g` (< 4) per finished `ARTDEPO` building.
    pub depot_group: Option<(usize, i32)>,
    /// `ARTDEPO`: a depot for [`EconomyDef::depot_group`] limits.
    pub art_depot: bool,
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

/// `EXSPRITES name variants n step spread` (NM+0x28da, +0x181a, +0x183d, +0x1845,
/// +0x1841): a field type plants a random one of `variants` sprite types from `name`; mills
/// plant them on a `(2n + 1)²` diamond grid `step` px apart with up to `spread` px of jitter.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExSprites {
    pub name: String,
    pub variants: i32,
    pub n: i32,
    pub step: i32,
    pub spread: i32,
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
            "FARM" => (e.farm, e.is_farm) = (15, true),
            "MFARM" => (e.farm, e.is_farm) = (i32::from(c.int(0).unwrap_or(0) as u8), true),
            "ADDSECTION" => {
                if let Some(s) = c.int(0).filter(|s| (0..5).contains(s)) {
                    e.sections.retain(|x| x.0 != s as u8);
                    e.sections.push((s as u8, c.int(2).unwrap_or(0)));
                }
            }
            "RESCONSUMER" => {
                e.consumer = c.word(0).and_then(resource_index).map(|r| (r, i32::from(c.int(1).unwrap_or(0) as i16)))
            }
            "PRODUCER" => {
                let n = c.int(0).unwrap_or(0).max(0) as usize;
                let mask = (0..n).filter_map(|i| c.word(1 + i).and_then(resource_index)).fold(0u8, |m, r| m | 1 << r);
                e.producer =
                    Some(Producer { mask, free: c.int(1 + n).unwrap_or(0), per_peasant: c.int(2 + n).unwrap_or(0) });
            }
            "PEASANTABSORBER" => {
                e.peasant_absorber = c.int(0).unwrap_or(0);
                e.max_inside = e.peasant_absorber;
            }
            "UNITABSORBER" => {
                e.unit_absorber = c.int(0).unwrap_or(0);
                e.max_inside = e.unit_absorber;
            }
            "NO_HUNGRY" => e.no_hungry = true,
            "SPRITEOBJECT" => e.sprite_object = true,
            "EXSPRITES" => {
                e.ex_sprites = c.word(0).map(|name| ExSprites {
                    name: name.to_ascii_uppercase(),
                    variants: c.int(1).unwrap_or(0),
                    n: c.int(2).unwrap_or(0),
                    step: c.int(3).unwrap_or(0),
                    spread: c.int(4).unwrap_or(0),
                })
            }
            "TRANSFORMTOMEAT" => e.meat = c.int(0).map(|v| v as u8).filter(|&v| v != 0xFF),
            "USAGE" => e.mill = c.word(0).is_some_and(|u| u.eq_ignore_ascii_case("MELNICA")),
            "PEASANTCONVERTOR" => e.peasant_convertor = true,
            "MAXINDEPO" => {
                e.depot_group = c.int(0).filter(|g| (0..4).contains(g)).map(|g| (g as usize, c.int(1).unwrap_or(0)))
            }
            "ARTDEPO" => e.art_depot = true,
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

/// `USAGE` names by value (0x4ff3b6..0x4ff96f); 26 and 29 are unused.
pub const USAGES: [&str; 35] = [
    "",
    "MELNICA",
    "FARM",
    "CENTER",
    "SKLAD",
    "TOWER",
    "FIELD",
    "MINE",
    "FASTHORSE",
    "MORTIRA",
    "PUSHKA",
    "GRENADER",
    "HARDWALL",
    "WEAKWALL",
    "LINKOR",
    "WEAK",
    "FISHER",
    "ARTDEPO",
    "SUPERMORTIRA",
    "PORT",
    "LIGHTINFANTRY",
    "STRELOK",
    "HARDHORCE",
    "PEASANT",
    "HORSE-STRELOK",
    "FREGAT",
    "",
    "GALERA",
    "IAXTA",
    "",
    "SHEBEKA",
    "PAROM",
    "ARCHER",
    "MCANNON",
    "DIPLOMAT",
];

/// `USAGE` values the engine tests.
pub mod usage {
    pub const MELNICA: u8 = 1;
    pub const CENTER: u8 = 3;
    pub const SKLAD: u8 = 4;
    pub const TOWER: u8 = 5;
    pub const MINE: u8 = 7;
    pub const MORTIRA: u8 = 9;
    pub const PUSHKA: u8 = 10;
    pub const SUPERMORTIRA: u8 = 18;
    pub const PORT: u8 = 19;
    pub const PEASANT: u8 = 23;
    pub const MCANNON: u8 = 33;
}

/// Script and AI data of a type: `USAGE`, the force values and the capture flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Role {
    /// `USAGE NAME` (u8 NM+0x294a, [`USAGES`]).
    pub usage: u8,
    /// `FORCE v` (u8 NM+0x2939): strength for the AI's force sums.
    pub force: i32,
    /// `STORMFORCE v` (u8 NM+0x293c): strength when storming or defending a building.
    pub storm_force: i32,
    /// `BATTLEFORCE v` (u8 NM+0x294b, default 32).
    pub battle_force: i32,
    /// `OPPOSIT a b` (u8 NM+0x293a/+0x293b): defence scale of a building's garrison.
    pub opposit: (i32, i32),
    /// `CATEGORY NAME` (u8 NM+0x1736) through `CATEGORIES.DAT` (`STRELOK 1`).
    pub category: u8,
    /// `CAPTURE` (NM+0x172a bit1).
    pub capture: bool,
    /// `VES v` (NM+0x1767, default 10; `USAGE MELNICA` sets 100, `USAGE SKLAD` 30, `BUILDING` 500).
    pub ves: i32,
    /// A `BUILDING` token in `ATTMASK` or `MATHERIAL` (NM+0x172b bit2): attacks buildings.
    pub attacks_buildings: bool,
    /// `AUTO_NO_ATTACK` / `AUTO_STAND_GROUND` (NM+0x172b bits 0/1): initial unit states.
    pub auto_no_attack: bool,
    pub auto_stand_ground: bool,
    /// `STANDGROUND` (NM+0x172b bit3): the interface offers the stand-ground button.
    pub stand_ground_button: bool,
    /// Artillery (NM+0x172a bit5, set by `USAGE MORTIRA/PUSHKA/SUPERMORTIRA/MCANNON`).
    pub artillery: bool,
    /// `ARCHER` (NM+0x172b bit7).
    pub archer: bool,
    /// `ARMATTACK` (NM+0x172d bit7): may be switched to melee only.
    pub arm_attack: bool,
}

fn role(def: &UnitDef) -> Role {
    let mut r = Role {
        usage: 0,
        force: 0,
        storm_force: 0,
        battle_force: 32,
        opposit: (0, 0),
        category: 0,
        capture: false,
        ves: 10,
        attacks_buildings: false,
        auto_no_attack: false,
        auto_stand_ground: false,
        stand_ground_button: false,
        artillery: false,
        archer: false,
        arm_attack: false,
    };
    let byte = |c: &acr_formats::defs::Command, i| i32::from(c.int(i).unwrap_or(0) as u8);
    for c in &def.commands {
        match c.keyword.as_str() {
            "USAGE" => {
                let name = c.word(0).unwrap_or("").to_ascii_uppercase();
                if let Some(v) = USAGES.iter().position(|u| !u.is_empty() && *u == name) {
                    r.usage = v as u8;
                    match r.usage {
                        usage::MELNICA => r.ves = 100,
                        usage::SKLAD => r.ves = 30,
                        usage::MORTIRA | usage::PUSHKA | usage::SUPERMORTIRA | usage::MCANNON => r.artillery = true,
                        _ => {}
                    }
                }
            }
            "FORCE" => r.force = byte(c, 0),
            "STORMFORCE" => r.storm_force = byte(c, 0),
            "BATTLEFORCE" => r.battle_force = byte(c, 0),
            "OPPOSIT" => r.opposit = (byte(c, 0), byte(c, 1)),
            "CATEGORY" => r.category = u8::from(c.word(0).is_some_and(|w| w.eq_ignore_ascii_case("STRELOK"))),
            "CAPTURE" => r.capture = true,
            "VES" => r.ves = c.int(0).unwrap_or(10),
            "BUILDING" => r.ves = 500,
            "ATTMASK" | "MATHERIAL" => {
                let first = if c.keyword == "ATTMASK" { 2 } else { 1 };
                r.attacks_buildings |=
                    (first..first + 8).filter_map(|k| c.word(k)).any(|w| w.eq_ignore_ascii_case("BUILDING"));
            }
            "AUTO_NO_ATTACK" => r.auto_no_attack = true,
            "AUTO_STAND_GROUND" => r.auto_stand_ground = true,
            "STANDGROUND" => r.stand_ground_button = true,
            "ARCHER" => r.archer = true,
            "ARMATTACK" => r.arm_attack = true,
            _ => {}
        }
    }
    r
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
    /// `MinDamage` (`EXDAMAGE i min max`; `DAMAGE` sets both): the damage of a garrisoned
    /// building without shooters inside.
    pub min_damage: i32,
    /// `DAMAGEDEC i d`: shot damage halves every `d` px (default 0xFFFF).
    pub damage_dec: i32,
    /// `WEAPONKIND i KIND` (`NRES.DAT [WEAPONS]`; default `MECH`).
    pub kind: String,
    /// `WEAPON i ID` (`WEAPON.NDS`): the attack shoots.
    pub weapon: Option<String>,
    /// The index of `weapon` in [`Catalog::weapons`].
    pub weapon_id: Option<u16>,
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
            min_damage: 0,
            damage_dec: 0xFFFF,
            kind: "MECH".into(),
            weapon: None,
            weapon_id: None,
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
    /// `SETACTIVEPOINT` muzzle points by animation (upper-case name).
    pub muzzles: BTreeMap<String, Muzzle>,
    /// `EXPLMEDIA NAME r` (NM+0x28de/+0x28df): the media a projectile hitting the object
    /// explodes in, and the object's size for round damage (px; buildings 5).
    pub expl_media: (i32, i32),
    /// `EXPLRADIUS r` (NM+0x2920): more size for round damage (px).
    pub expl_radius: i32,
    /// `EX_SHOTS` / `EX_SHOTS2`: a building's shot points.
    pub shot_points: Vec<ShotPoint>,
    /// `3DBARS n (x y l w h)*n` (NM+0x2892): the boxes of a building that block shots and
    /// that shooters aim at.
    pub bars: Vec<[i32; 5]>,
    /// `RESSUBST` (NM+0x172b bit6): a mine that becomes the mine of the deposit it is built
    /// on.
    pub res_subst: bool,
    /// `ARTPODGOTOVKA` (NM+0x172c bit1): the player may order attacks on the ground.
    pub ground_attack: bool,
    /// `RASTRATA_NA_VISTREL i n (RES v)*n` / `..VISTREL2 i1 i2 n ..` (NM+0x2927..): shots of
    /// attack index `i1` or `i2` cost these resources (cannons: iron and coal).
    pub shot_cost: Option<(u8, u8, [i32; 8])>,
    /// `WEAPONINSECTOR a r1 r2 d1 d2 sector pct` (NM+0x2940): canister. Attack index `a`
    /// hurts everything from `r1` to `r2` px within `sector` of the facing, `d1` falling to
    /// `d2` with distance and to `pct` % at the sector's edge.
    pub sector_weapon: Option<SectorWeapon>,
    /// `FIRELIMIT v` (u8 NM+0x1762, buildings 15): hits of slow-burning weapons before the
    /// object burns.
    pub fire_limit: i32,
    /// `EVROSHOT k` (NM+0x293f; buildings 100, `EVROUNIT` units 1): with a European unit
    /// first inside, an Indian building fires only its attack index `k` points.
    pub evro_shot: i32,
    /// `PROMAX v` / `PROMAX2 k v` (NM+0x1771[8]): the miss roll (out of 101) of attackers of
    /// column `k` against the garrison; `PROMAXCTG c` (NM+0x1779): this unit's column.
    pub promax: [i32; 8],
    pub promax_ctg: u8,
    /// `VISION` (0..=8, default 0): the fog reveal pattern.
    pub vision: u8,
    /// Prices, production, housing, mines.
    pub economy: EconomyDef,
    /// Morale: what kills by this unit frighten, and how easily it panics.
    pub fear: FearDef,
    /// `USAGE`, force values, capture flags.
    pub role: Role,
    /// `COMMANDCENTER` (NM+0x172d bit6): fleeing units of the nation run to it, and being near
    /// one calms them.
    pub command_center: bool,
    /// `FASTUNIT` (NM+0x2925): never walks in the stance-1 motion.
    pub fast_unit: bool,
    /// `FORMLIKESHOOTERS` (NM+0x172f bit0): its formations are shooting formations, whose
    /// members attack one by one instead of starting formation melee.
    pub shooters_formation: bool,
    /// The characteristics upgrades can change, as every player starts with them.
    pub adv: std::sync::Arc<Adv>,
}

/// The muzzle points of an attack animation (`SETACTIVEPOINT #ANIM f (x y)*R`, anim+0xa/+0xe;
/// `SETACTIVEPOINT2` gives two sets and two shots).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Muzzle {
    /// `(x, y)` per stored direction, two sets after each other with `SETACTIVEPOINT2`.
    pub points: Vec<(i32, i32)>,
    pub sets: u8,
    /// Stored directions of the animation.
    pub directions: i32,
    /// `USERLC` offset (`dx dy`) of the sprite pack of each frame (`NF.dx`, `NF.dy`).
    pub frame_offsets: Vec<(i32, i32)>,
}

/// `WEAPONINSECTOR` (the struct at NM+0x2940).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SectorWeapon {
    pub att: u8,
    pub r1: i32,
    pub r2: i32,
    pub d1: i32,
    pub d2: i32,
    pub sector: i32,
    pub pct: i32,
}

/// One `EX_SHOTS`/`EX_SHOTS2` point of a building (NM+0x17ae..): where a shot leaves, the
/// ranges and sector it covers, its reload and attack index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ShotPoint {
    /// x in the picture (+0x17ae), screen y (+0x17b2), height (+0x17ba) and the visual
    /// offset (+0x17b6).
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub extra: i32,
    /// Range from this point in px (+0x17ce, +0x17d2).
    pub r_min: i32,
    pub r_max: i32,
    /// Direction (+0x17c6) and half-width (+0x17ca) of the sector it fires into.
    pub dir: u8,
    pub sector: i32,
    /// Reload of this point (+0x17be, counts 2 per tick) and attack index (+0x17c2).
    pub reload: i32,
    pub att: u8,
}

/// The per-player characteristics of a type that upgrades change (`AdvCharacter`, built
/// from the `.MD` by `CreateAdvCharacter` 0x532b00). Combat, prices, production and
/// construction read these through [`crate::World::adv`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Adv {
    /// Attack indices: `radius`, `detect`, `pause` and `damage` change (`AttR1/2`, `DetR1/2`,
    /// `AttPause`, `MaxDamage`).
    pub attacks: Vec<Attack>,
    /// Armour per weapon kind (u8).
    pub protection: Vec<(String, i32)>,
    pub shield: i32,
    /// `Life` (u16): the maximum life of new objects.
    pub life: i32,
    /// `MaxStage`: construction stages, and production time of units.
    pub build_stages: i32,
    /// `MaxInside`: capacity for units inside.
    pub max_inside: i32,
    /// `NeedRes`: the base price.
    pub cost: [i32; 8],
    pub razbros: i32,
    /// Housing per finished object (ADC+0xed).
    pub farm: i32,
    /// `FEARFACTOR t v` (u16, ADC+0xf1): the base panic chance for fear type `t`.
    pub fear_factor: [i32; 4],
    /// `RATE i v` (u8 ADC+0x9c, default 16): the walking speed in stance `i + 1`, in 1/16
    /// (`SPEED` upgrades change it).
    pub rate: [i32; ATTACK_SLOTS],
    /// `ResEff` (ADC+0xef, 100): percent of a load a base credits (`GETRES` upgrades).
    pub res_eff: i32,
}

/// Morale keywords of a type (parser 0x4f372c).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FearDef {
    /// `FEARTYPE i t` (NM+0x16ae): the fear type a kill by attack index `i` raises (0xFF: none).
    pub types: [u8; ATTACK_SLOTS],
    /// `FEARRADIUS t r` (NM+0x16ba, px, default 255): this unit only panics from a kill of
    /// fear type `t` closer than `r`.
    pub radius: [u8; 4],
    /// `PSIXOZ v` (NM+0x1765, default 32): chance `v/1024` per flight leg to calm down.
    pub psixoz: i32,
}

impl Default for FearDef {
    fn default() -> Self {
        Self { types: [0xFF; ATTACK_SLOTS], radius: [0xFF; 4], psixoz: 32 }
    }
}

fn fear(def: &UnitDef) -> (FearDef, [i32; 4]) {
    let mut f = FearDef::default();
    let mut factor = [0; 4];
    for c in &def.commands {
        let (a, b) = (c.int(0).unwrap_or(-1), c.int(1).unwrap_or(0));
        match c.keyword.as_str() {
            "FEARTYPE" if (0..ATTACK_SLOTS as i32).contains(&a) && (0..4).contains(&b) => f.types[a as usize] = b as u8,
            "FEARRADIUS" if (0..4).contains(&a) => f.radius[a as usize] = b.clamp(0, 255) as u8,
            "FEARFACTOR" if (0..4).contains(&a) => factor[a as usize] = i32::from(b as u16),
            "PSIXOZ" => f.psixoz = i32::from(c.int(0).unwrap_or(32) as i16),
            _ => {}
        }
    }
    (f, factor)
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
            muzzles: muzzles(def),
            expl_media: def.get("EXPLMEDIA").map_or((0, 0), |c| {
                (c.word(0).and_then(crate::weapon::media::id).unwrap_or(0), i32::from(c.int(1).unwrap_or(0) as u8))
            }),
            expl_radius: def.get("EXPLRADIUS").and_then(|c| c.int(0)).unwrap_or(0),
            shot_points: shot_points(def),
            bars: def
                .get("3DBARS")
                .map(|c| {
                    let n = c.int(0).unwrap_or(0).max(0) as usize;
                    (0..n)
                        .map(|i| std::array::from_fn(|k| i32::from(c.int(1 + 5 * i + k).unwrap_or(0) as i16)))
                        .collect()
                })
                .unwrap_or_default(),
            ground_attack: def.has("ARTPODGOTOVKA"),
            res_subst: def.has("RESSUBST"),
            shot_cost: def.get("RASTRATA_NA_VISTREL2").or_else(|| def.get("RASTRATA_NA_VISTREL")).map(|c| {
                let two = c.keyword == "RASTRATA_NA_VISTREL2";
                let (i1, i2) = (
                    c.int(0).unwrap_or(0) as u8,
                    if two { c.int(1).unwrap_or(0) as u8 } else { c.int(0).unwrap_or(0) as u8 },
                );
                let mut cost = [0; 8];
                for (r, v) in c.word_int_pairs(if two { 2 } else { 1 }) {
                    if let Some(k) = resource_index(r).filter(|&k| k < 8) {
                        cost[k] = v;
                    }
                }
                (i1, i2, cost)
            }),
            sector_weapon: def.get("WEAPONINSECTOR").map(|c| {
                let a = |i| c.int(i).unwrap_or(0);
                SectorWeapon { att: a(0) as u8, r1: a(1), r2: a(2), d1: a(3), d2: a(4), sector: a(5), pct: a(6) }
            }),
            fire_limit: def
                .get("FIRELIMIT")
                .and_then(|c| c.int(0))
                .map_or(if def.is_building() { 15 } else { 0 }, |v| i32::from(v as u8)),
            evro_shot: {
                let mut e = if def.is_building() { 100 } else { 0 };
                for c in &def.commands {
                    match c.keyword.as_str() {
                        "EVROSHOT" => e = i32::from(c.int(0).unwrap_or(0) as u8),
                        "EVROUNIT" => e = 1,
                        _ => {}
                    }
                }
                e
            },
            promax: {
                let mut p = [0; 8];
                for c in &def.commands {
                    match c.keyword.as_str() {
                        "PROMAX" => p = [i32::from(c.int(0).unwrap_or(0) as u8); 8],
                        "PROMAX2" => {
                            if let Some(k) = c.int(0).filter(|k| (0..8).contains(k)) {
                                p[k as usize] = i32::from(c.int(1).unwrap_or(0) as u8);
                            }
                        }
                        _ => {}
                    }
                }
                p
            },
            promax_ctg: def.get("PROMAXCTG").and_then(|c| c.int(0)).unwrap_or(0) as u8 & 7,
            vision: int("VISION", 0).clamp(0, 255) as u8,
            economy: economy(def),
            fear: fear(def).0,
            role: role(def),
            command_center: def.has("COMMANDCENTER"),
            fast_unit: def.get("FASTUNIT").is_some_and(|c| c.int(0).unwrap_or(1) != 0),
            shooters_formation: def.has("FORMLIKESHOOTERS"),
            adv: Default::default(),
        }
        .with_adv(fear(def).1, {
            let mut r = [16; ATTACK_SLOTS];
            for c in def.all("RATE") {
                if let Some(i) = c.int(0).filter(|i| (0..ATTACK_SLOTS as i32).contains(i)) {
                    r[i as usize] = i32::from(c.int(1).unwrap_or(16) as u8);
                }
            }
            r
        })
    }

    /// Fills [`ObjectType::adv`] from the other fields (`CreateAdvCharacter`).
    fn with_adv(mut self, fear_factor: [i32; 4], rate: [i32; ATTACK_SLOTS]) -> Self {
        let e = &self.economy;
        self.adv = std::sync::Arc::new(Adv {
            attacks: self.attacks.clone(),
            protection: self.protection.clone(),
            shield: self.shield,
            life: self.life,
            build_stages: e.build_stages,
            max_inside: e.max_inside,
            cost: e.price,
            razbros: self.razbros,
            farm: e.farm,
            fear_factor,
            rate,
            res_eff: 100,
        });
        self
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

/// The muzzle points of `SETACTIVEPOINT` / `SETACTIVEPOINT2` (parser 0x4fd325 / 0x4fd65b).
fn muzzles(def: &UnitDef) -> BTreeMap<String, Muzzle> {
    let mut m = BTreeMap::new();
    for (key, sets) in [("SETACTIVEPOINT", 1u8), ("SETACTIVEPOINT2", 2)] {
        for c in def.all(key) {
            let Some(name) = c.word(0) else { continue };
            let Some(anim) = def.animation(name) else { continue };
            let r = anim.directions.max(0) as usize;
            let points: Vec<(i32, i32)> =
                (0..r * usize::from(sets)).filter_map(|k| Some((c.int(2 + 2 * k)?, c.int(3 + 2 * k)?))).collect();
            let frame_offsets =
                anim.frame_refs().iter().map(|f| def.sprite(f.sprite).map_or((0, 0), |s| s.offset)).collect();
            m.insert(name.to_ascii_uppercase(), Muzzle { points, sets, directions: anim.directions, frame_offsets });
        }
    }
    m
}

/// `EX_SHOTS n (a1..a9)*n` and `EX_SHOTS2 n (a1..a10)*n` (NM+0x17ac..).
fn shot_points(def: &UnitDef) -> Vec<ShotPoint> {
    let mut v = Vec::new();
    for (key, k) in [("EX_SHOTS", 9usize), ("EX_SHOTS2", 10)] {
        let Some(c) = def.get(key) else { continue };
        let n = c.int(0).unwrap_or(0).max(0) as usize;
        for i in 0..n {
            let a = |j: usize| c.int(1 + i * k + j - 1).unwrap_or(0);
            let (z, extra) = if k == 10 { (a(3) - a(2), a(10)) } else { (a(3), a(2) + a(3)) };
            v.push(ShotPoint {
                x: a(1),
                y: a(2),
                z,
                extra,
                r_min: i32::from(a(4) as u16),
                r_max: i32::from(a(5) as u16),
                dir: a(6) as u8,
                sector: i32::from(a(7) as u8),
                reload: a(8),
                att: a(9) as u8,
            });
        }
    }
    v
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
            "DAMAGE" => (a.damage, a.min_damage) = (a1, a1),
            "EXDAMAGE" => (a.min_damage, a.damage) = (a1, a2),
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
    /// The market's rate table (`ECONOMY.TXT`); without it there is no trade.
    pub market: Option<crate::market::MarketDef>,
    /// The panic factors of `MORALE.DAT`.
    pub morale: crate::morale::MoraleDef,
    /// Projectile types (`WEAPON.NDS`).
    pub weapons: crate::weapon::Weapons,
    /// `ADDRANGE` / `ADDPAUSE` (`NRES.DAT [UNITKINDS]`, globals 0x66ae18/0x66ae1c): an attack on
    /// the ground reaches this percentage of the range and reloads this percentage as long.
    pub add_range: i32,
    pub add_pause: i32,
}

impl Catalog {
    pub fn new() -> Self {
        Self { add_range: 200, add_pause: 150, ..Self::default() }
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

    /// Sets the projectile types and resolves every attack's `WEAPON` to its index.
    pub fn set_weapons(&mut self, weapons: crate::weapon::Weapons) {
        for t in &mut self.types {
            for a in &mut t.attacks {
                a.weapon_id = a.weapon.as_deref().and_then(|n| weapons.find(n));
            }
            let adv = std::sync::Arc::make_mut(&mut t.adv);
            for a in &mut adv.attacks {
                a.weapon_id = a.weapon.as_deref().and_then(|n| weapons.find(n));
            }
        }
        self.weapons = weapons;
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
