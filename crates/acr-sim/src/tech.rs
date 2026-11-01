//! The tech tree of the nation files (`*.NDS`): what each building produces
//! (`[FIXED_PRODUCE]`), what is available from the start (`[ENABLED]`, `[UPGRADEENABLE]`),
//! the conditions that make objects and upgrades available (`[ACCESSCONTROL]`), and the
//! upgrades themselves (`[UPGRADE]`, `[UPGRADEPLACE]`, `[UPGRADELINKS]`, `[AUTOUPGRADE]`,
//! `[PRIVATE]`, `[UPGRADESTAGE]`, `[STAGEDEPENDENT]`, `[UNITLOCK]`, `[CANSETDEST]`).
//!
//! The engine's nation loader (0x60e7f0, `stRecog.cpp`) reads every nation file into all
//! eight `Nation`s, so object and upgrade ids form one namespace across nations; the same
//! holds here: types are [`TypeId`]s of the [`crate::Catalog`] and upgrades are
//! [`UpgradeId`]s in load order. The per-player state lives in [`crate::world::Player`].
//! The upgrade parser is 0x52f798 (`UnderstandUpgrade`). See `docs/simulation.md`.

use acr_formats::defs::{NationDef, Upgrade};

use crate::catalog::{Catalog, TypeId, resource_index};

/// Index of an upgrade in [`Tech::upgrades`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UpgradeId(pub u16);

/// Upgrade categories (`CtgUpgrade`, NewUpgrade+0x20), numbered as the parser 0x52f798 sets
/// them.
pub mod ctg {
    pub const SPEED: u8 = 0;
    pub const SHIELD: u8 = 1;
    pub const PROTECTION: u8 = 2;
    pub const ATTPAUSE: u8 = 3;
    pub const WEAPON: u8 = 4;
    pub const BUILD: u8 = 5;
    pub const LIFE: u8 = 6;
    pub const PRECISE: u8 = 7;
    pub const ATTRANGE: u8 = 8;
    pub const INSIDE: u8 = 9;
    pub const COST: u8 = 10;
    pub const ENABLE: u8 = 11;
    pub const DAMAGE: u8 = 12;
    pub const GETRES: u8 = 13;
    pub const SINGLE_INSIDE: u8 = 14;
    pub const SINGLE_ATTPAUSE: u8 = 15;
    pub const FISHING: u8 = 16;
    pub const RAZBROS: u8 = 17;
    pub const SHAR: u8 = 18;
    pub const EFFECT_FOOD: u8 = 19;
    pub const EFFECT_WOOD: u8 = 20;
    pub const EFFECT_STONE: u8 = 21;
    pub const GEOLOGY: u8 = 22;
    pub const HEALING: u8 = 23;
    /// `UPSTAGE0` … `UPSTAGE4` are 24 … 28.
    pub const UPSTAGE0: u8 = 24;
    pub const MORELIFE: u8 = 29;
    pub const MOREFARMS: u8 = 30;
    pub const UNCONSTRUCT: u8 = 31;
    pub const MOREFARMS_SINGLE: u8 = 32;
    pub const FEAR: u8 = 33;
    /// `FEAR0` … `FEAR5` are 34 … 39.
    pub const FEAR0: u8 = 34;
    pub const PERSONALLIFE: u8 = 40;
}

/// An upgrade value (`ReadValue`): `n`, `+n`, `-n`, `n%`, `+n%`, `-n%`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Value {
    /// `ValueType`: 0 set, 1 add, 2 subtract, 4 percent of, 5 add percent, 6 subtract
    /// percent (bit 2 = `%`).
    pub op: u8,
    pub n: i32,
}

impl Value {
    /// Parses `+10%` style values: a leading `+`/`-` and a trailing `%`.
    pub fn parse(s: &str) -> Option<Self> {
        let (mut op, rest) = match s.as_bytes().first()? {
            b'+' => (1, &s[1..]),
            b'-' => (2, &s[1..]),
            _ => (0, s),
        };
        let rest = match rest.strip_suffix('%') {
            Some(r) => {
                op |= 4;
                r
            }
            None => rest,
        };
        Some(Self { op, n: rest.trim().parse().ok()? })
    }

    /// `UseValue` (0x530fe7): applies the value to `v` with truncating division, like `div()`.
    pub fn apply(self, v: i32) -> i32 {
        match self.op {
            0 => self.n,
            1 => v.wrapping_add(self.n),
            2 => v.wrapping_sub(self.n),
            4 => v.wrapping_mul(self.n) / 100,
            5 => v.wrapping_add(v.wrapping_mul(self.n) / 100),
            6 => v.wrapping_sub(v.wrapping_mul(self.n) / 100),
            _ => v,
        }
    }
}

/// Which attack indices or protection kinds a unit upgrade touches (`CtgType`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Select {
    /// No category: the effect function gets index 0.
    None,
    /// `WEAPONKIND k` / `WEAPONKIND GRP n k*n`: every attack index whose `WEAPONKIND` is
    /// one of these (compared on the type's `.MD`, as 0x531d74 does).
    WeaponKinds(Vec<String>),
    /// `ATTINDEX i` / `ATTINDEX GRP n i*n` / `ATTINDEX ALL`.
    AttIndex(Vec<usize>),
    /// `ALL`: every attack index.
    All,
    /// `PROTECTION` kinds (weapon kind names).
    Protection(Vec<String>),
}

/// What an upgrade does when it completes (`PerformNewUpgrade` 0x532281).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Effect {
    /// A category applied to the per-player characteristics (`AdvCharacter`) of unit
    /// types, or to the researching building for the `SINGLE_*`/`PERSONALLIFE` kinds.
    Units { category: u8, units: Vec<TypeId>, select: Select, value: Option<Value> },
    /// `COST`: one value per resource (`GRP n (RES v)*n`).
    Cost { units: Vec<TypeId>, values: Vec<(usize, Value)> },
    /// `ENABLE UNITS|UPGRADES|UNITS&UPGRADES`.
    Enable { units: Vec<TypeId>, upgrades: Vec<UpgradeId> },
    /// `EFFECT_FOOD`, `EFFECT_WOOD`, `EFFECT_STONE` (nation factors) and `GEOLOGY`.
    Nation { category: u8, value: Option<Value> },
    /// `UPSTAGEn`, `UNCONSTRUCT`: act on the researching building.
    Building { category: u8 },
    /// Empty or not understood.
    None,
}

/// One upgrade (`NewUpgrade`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UpgradeDef {
    pub id: String,
    pub icon: String,
    /// Button position in the building's command panel.
    pub position: (i32, i32),
    /// Text id of the hint.
    pub message: String,
    /// `#LEVEL`: for `[PRIVATE]` upgrades the building's own upgrade count must equal it.
    pub level: i32,
    /// `#COST`, by resource index (u16 in the engine, NewUpgrade+0x10).
    pub cost: [i32; 8],
    /// `#TIME` (`NStages`, u16): research takes `ceil((time − 1) / 2)` ticks.
    pub time: i32,
    pub effect: Effect,
    /// `[PRIVATE]` (`Individual`): researched per building, may be repeated.
    pub individual: bool,
    /// `[UPGRADESTAGE]` (`StageUp`).
    pub stage_up: bool,
    /// `[STAGEDEPENDENT]` mask (NewUpgrade+0x4d): building stages that must be done.
    pub stage_mask: u8,
    /// `[UPGRADEENABLE]`: `Enabled` and `PermanentEnabled` from the start.
    pub start_enabled: bool,
    /// `[DISABLED_UPGRADES]` (`Gray`).
    pub gray: bool,
    /// `[UPGRADELINKS]`: enabled when this one is done.
    pub auto_enable: Vec<UpgradeId>,
    /// `[AUTOUPGRADE]`: performed at once when this one is done.
    pub auto_perform: Vec<UpgradeId>,
}

/// A player's state of one upgrade (NewUpgrade+0x3c bits).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct UpgradeState {
    /// Can be researched now.
    pub enabled: bool,
    /// Enabled once its conditions hold (`[UPGRADEENABLE]`, `[UPGRADELINKS]`).
    pub permanent: bool,
    pub done: bool,
    /// Being researched somewhere.
    pub doing: bool,
    /// `NewUpgrade+0x3c` bit 0x40, set by `DisableUpgrade` and `SetUpgradeEnableStatus(<0)`:
    /// the command panel hides the upgrade unless `manual_enable` is set.
    pub manual_disable: bool,
    /// Bit 0x80, set by `SetUpgradeEnableStatus(>0)`: the panel offers the upgrade.
    pub manual_enable: bool,
}

impl UpgradeState {
    pub fn new(u: &UpgradeDef) -> Self {
        Self {
            enabled: u.start_enabled,
            permanent: u.start_enabled,
            done: false,
            doing: false,
            manual_disable: false,
            manual_enable: false,
        }
    }
}

/// An object or upgrade in an `[ACCESSCONTROL]` line (the engine adds 8192 to upgrades).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Element {
    Type(TypeId),
    Upgrade(UpgradeId),
}

/// `[ACCESSCONTROL]` line: `target` is available only while every element of `requires`
/// is (finished objects of a type exist / an upgrade is done).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Access {
    pub target: Element,
    pub requires: Vec<Element>,
}

/// Per-type tech data.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct TypeTech {
    /// `[FIXED_PRODUCE]`: what the object produces or builds (`PAble`), in menu order.
    pub produce: Vec<TypeId>,
    /// `[UPGRADEPLACE]`: upgrades researched here (`GO->Upg`).
    pub upgrades: Vec<UpgradeId>,
    /// `[ENABLED]`: `Enabled` and `CondEnabled` from the start.
    pub start_enabled: bool,
    /// `[STAGEDEPENDENT]` mask (GeneralObject+0xad): stages the producing building needs.
    pub stage_mask: u8,
    /// `[UNITLOCK]`: disabled while the player has this many objects of the type.
    pub lock: Option<(TypeId, u16)>,
    /// `[CANSETDEST]`: the building can have a rally point.
    pub can_set_dest: bool,
}

/// The tech tree of all loaded nations.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Tech {
    /// Whether nation files were loaded. Without them (synthetic test catalogs) every
    /// type is enabled and buildings produce anything.
    pub loaded: bool,
    /// Indexed by `TypeId`; may be shorter than the catalog (missing = default).
    pub types: Vec<TypeTech>,
    pub upgrades: Vec<UpgradeDef>,
    pub access: Vec<Access>,
}

impl Tech {
    pub fn of(&self, t: TypeId) -> Option<&TypeTech> {
        self.types.get(usize::from(t.0))
    }

    /// Looks an upgrade up by id (case-sensitive, like `GetUpgradeID`); the first wins.
    pub fn find_upgrade(&self, id: &str) -> Option<UpgradeId> {
        self.upgrades.iter().position(|u| u.id == id).map(|i| UpgradeId(i as u16))
    }

    pub fn upgrade(&self, u: UpgradeId) -> &UpgradeDef {
        &self.upgrades[usize::from(u.0)]
    }

    fn type_mut(&mut self, t: TypeId) -> &mut TypeTech {
        let i = usize::from(t.0);
        if self.types.len() <= i {
            self.types.resize(i + 1, TypeTech::default());
        }
        &mut self.types[i]
    }

    fn element(&self, catalog: &Catalog, id: &str) -> Option<Element> {
        catalog.find(id).map(Element::Type).or_else(|| self.find_upgrade(id).map(Element::Upgrade))
    }

    /// Adds a nation file. Its members must already be in the catalog; sections are applied
    /// in the loader's order (upgrades first, so later sections can refer to them).
    /// Unknown ids are skipped (the engine stops with an error message).
    pub fn add_nation(&mut self, catalog: &Catalog, def: &NationDef) {
        self.loaded = true;
        for u in &def.upgrades {
            let effect = compile_effect(self, catalog, u);
            let mut cost = [0; 8];
            for (r, v) in &u.cost {
                if let Some(i) = resource_index(r).filter(|&i| i < 8) {
                    cost[i] = i32::from(*v as u16);
                }
            }
            self.upgrades.push(UpgradeDef {
                id: u.id.clone(),
                icon: u.icon.clone(),
                position: u.position,
                message: u.message.clone(),
                level: u.level,
                cost,
                time: i32::from(u.time as u16),
                effect,
                individual: false,
                stage_up: false,
                stage_mask: 0,
                start_enabled: false,
                gray: false,
                auto_enable: Vec::new(),
                auto_perform: Vec::new(),
            });
        }
        let upgrades = |t: &Tech, ids: &[String]| ids.iter().filter_map(|i| t.find_upgrade(i)).collect::<Vec<_>>();
        for l in &def.auto_upgrade {
            if let Some(u) = self.find_upgrade(&l.head) {
                self.upgrades[usize::from(u.0)].auto_perform = upgrades(self, &l.items);
            }
        }
        for l in &def.upgrade_links {
            if let Some(u) = self.find_upgrade(&l.head) {
                self.upgrades[usize::from(u.0)].auto_enable = upgrades(self, &l.items);
            }
        }
        for p in &def.fixed_produce {
            if let Some(t) = catalog.find(&p.id) {
                self.type_mut(t).produce = p.items.iter().filter_map(|i| catalog.find(&i.id)).collect();
            }
        }
        for id in &def.upgrade_enable {
            if let Some(u) = self.find_upgrade(id) {
                self.upgrades[usize::from(u.0)].start_enabled = true;
            }
        }
        for id in &def.disabled_upgrades {
            if let Some(u) = self.find_upgrade(id) {
                self.upgrades[usize::from(u.0)].gray = true;
            }
        }
        for id in &def.private {
            if let Some(u) = self.find_upgrade(id) {
                self.upgrades[usize::from(u.0)].individual = true;
            }
        }
        for l in &def.upgrade_place {
            if let Some(t) = catalog.find(&l.head) {
                self.type_mut(t).upgrades = upgrades(self, &l.items);
            }
        }
        for id in &def.enabled {
            if let Some(t) = catalog.find(id) {
                self.type_mut(t).start_enabled = true;
            }
        }
        for l in &def.access_control {
            let Some(target) = self.element(catalog, &l.head) else { continue };
            let requires = l.items.iter().filter_map(|i| self.element(catalog, i)).collect();
            self.access.push(Access { target, requires });
        }
        for (id, n, lock) in &def.unit_lock {
            if let (Some(t), Some(l)) = (catalog.find(id), catalog.find(lock)) {
                self.type_mut(t).lock = Some((l, *n as u8 as u16));
            }
        }
        for id in &def.can_set_dest {
            if let Some(t) = catalog.find(id) {
                self.type_mut(t).can_set_dest = true;
            }
        }
        for id in &def.upgrade_stage {
            if let Some(u) = self.find_upgrade(id) {
                self.upgrades[usize::from(u.0)].stage_up = true;
            }
        }
        for (id, n) in &def.stage_dependent {
            match self.element(catalog, id) {
                Some(Element::Type(t)) => self.type_mut(t).stage_mask = *n as u8,
                Some(Element::Upgrade(u)) => self.upgrades[usize::from(u.0)].stage_mask = *n as u8,
                None => {}
            }
        }
    }
}

/// `City::RefreshAbility` (0x42ffed): first every `[ACCESSCONTROL]` target is disabled,
/// then each whose required objects exist (finished) and upgrades are done is enabled again
/// (a type only if it is `CondEnabled` or a building, an upgrade only if it is
/// `PermanentEnabled` and not done). Last, over every type (0x430265): `ManualDisable`
/// switches it off, otherwise `[UNITLOCK]` types are disabled while the player has enough
/// objects of the lock type and otherwise follow `CondEnabled`; `ManualEnable` then
/// switches it on.
pub fn refresh_ability(w: &mut crate::World, player: u8) {
    let catalog = w.catalog.clone();
    let tech = &catalog.tech;
    if !tech.loaded {
        return;
    }
    let p = &mut w.players[usize::from(player)];
    for a in &tech.access {
        match a.target {
            Element::Type(t) => set(&mut p.enabled, t, false),
            Element::Upgrade(u) => p.upgrades[usize::from(u.0)].enabled = false,
        }
    }
    for a in &tech.access {
        let ok = a.requires.iter().all(|r| match *r {
            Element::Type(t) => p.ready_count.get(usize::from(t.0)).is_some_and(|&n| n != 0),
            Element::Upgrade(u) => p.upgrades[usize::from(u.0)].done,
        });
        if !ok {
            continue;
        }
        match a.target {
            Element::Type(t) => {
                if get(&p.cond_enabled, t) || catalog.get(t).building {
                    set(&mut p.enabled, t, true);
                }
            }
            Element::Upgrade(u) => {
                let s = &mut p.upgrades[usize::from(u.0)];
                if s.permanent && !s.done {
                    s.enabled = true;
                }
            }
        }
    }
    for (i, tt) in tech.types.iter().enumerate() {
        let t = TypeId(i as u16);
        if get(&p.manual_disable, t) {
            set(&mut p.enabled, t, false);
        } else if let Some((lock, n)) = tt.lock {
            let count = p.type_count.get(usize::from(lock.0)).copied().unwrap_or(0);
            let on = count < n && get(&p.cond_enabled, t);
            set(&mut p.enabled, t, on);
        }
        if get(&p.manual_enable, t) {
            set(&mut p.enabled, t, true);
        }
    }
}

fn get(v: &[bool], t: TypeId) -> bool {
    v.get(usize::from(t.0)).copied().unwrap_or(false)
}

fn set(v: &mut [bool], t: TypeId, on: bool) {
    if let Some(b) = v.get_mut(usize::from(t.0)) {
        *b = on;
    }
}

/// The ids of `GetUnits` (0x52e8b1): `GRP n ID*n` or one id; returns the rest.
fn units<'a>(catalog: &Catalog, w: &'a [String]) -> (Vec<TypeId>, &'a [String]) {
    match w.first().map(String::as_str) {
        Some("GRP") => {
            let n = w.get(1).and_then(|n| n.parse::<usize>().ok()).unwrap_or(0).min(w.len().saturating_sub(2));
            (w[2..2 + n].iter().filter_map(|id| catalog.find(id)).collect(), &w[2 + n..])
        }
        Some(id) => (catalog.find(id).into_iter().collect(), &w[1..]),
        None => (Vec::new(), w),
    }
}

/// A counted or single word list (`GRP n x*n` or `x`); returns the rest.
fn words(w: &[String]) -> (Vec<String>, &[String]) {
    match w.first().map(String::as_str) {
        Some("GRP") => {
            let n = w.get(1).and_then(|n| n.parse::<usize>().ok()).unwrap_or(0).min(w.len().saturating_sub(2));
            (w[2..2 + n].to_vec(), &w[2 + n..])
        }
        Some(x) => (vec![x.to_owned()], &w[1..]),
        None => (Vec::new(), w),
    }
}

/// `GetCtgParam01`: `ATTINDEX …`, `WEAPONKIND …` or `ALL`.
fn select01(w: &[String]) -> (Select, &[String]) {
    match w.first().map(String::as_str) {
        Some("ALL") => (Select::All, &w[1..]),
        Some("WEAPONKIND") => {
            let (k, rest) = words(&w[1..]);
            (Select::WeaponKinds(k), rest)
        }
        Some("ATTINDEX") => {
            if w.get(1).map(String::as_str) == Some("ALL") {
                return (Select::All, &w[2..]);
            }
            let (k, rest) = words(&w[1..]);
            (Select::AttIndex(k.iter().filter_map(|i| i.parse().ok()).collect()), rest)
        }
        _ => (Select::None, w),
    }
}

fn compile_effect(tech: &Tech, catalog: &Catalog, u: &Upgrade) -> Effect {
    let e = &u.effect;
    let Some(name) = e.first() else { return Effect::None };
    let rest = &e[1..];
    let category = match name.as_str() {
        "SPEED" => ctg::SPEED,
        "SHIELD" => ctg::SHIELD,
        "PROTECTION" => ctg::PROTECTION,
        "ATTPAUSE" => ctg::ATTPAUSE,
        "WEAPON" => ctg::WEAPON,
        "BUILD" => ctg::BUILD,
        "LIFE" => ctg::LIFE,
        "PRECISE" => ctg::PRECISE,
        "ATTRANGE" => ctg::ATTRANGE,
        "INSIDE" => ctg::INSIDE,
        "COST" => ctg::COST,
        "ENABLE" => ctg::ENABLE,
        "DAMAGE" => ctg::DAMAGE,
        "GETRES" => ctg::GETRES,
        "SINGLE_INSIDE" => ctg::SINGLE_INSIDE,
        "SINGLE_ATTPAUSE" => ctg::SINGLE_ATTPAUSE,
        "FISHING" => ctg::FISHING,
        "RAZBROS" => ctg::RAZBROS,
        "SHAR" => ctg::SHAR,
        "EFFECT_FOOD" => ctg::EFFECT_FOOD,
        "EFFECT_WOOD" => ctg::EFFECT_WOOD,
        "EFFECT_STONE" => ctg::EFFECT_STONE,
        "GEOLOGY" => ctg::GEOLOGY,
        "HEALING" => ctg::HEALING,
        "UPSTAGE0" => ctg::UPSTAGE0,
        "UPSTAGE1" => ctg::UPSTAGE0 + 1,
        "UPSTAGE2" => ctg::UPSTAGE0 + 2,
        "UPSTAGE3" => ctg::UPSTAGE0 + 3,
        "UPSTAGE4" => ctg::UPSTAGE0 + 4,
        "MORELIFE" => ctg::MORELIFE,
        "MOREFARMS" => ctg::MOREFARMS,
        "UNCONSTRUCT" => ctg::UNCONSTRUCT,
        "MOREFARMS_SINGLE" => ctg::MOREFARMS_SINGLE,
        "FEAR" => ctg::FEAR,
        "FEAR0" => ctg::FEAR0,
        "FEAR1" => ctg::FEAR0 + 1,
        "FEAR2" => ctg::FEAR0 + 2,
        "FEAR3" => ctg::FEAR0 + 3,
        "FEAR4" => ctg::FEAR0 + 4,
        "FEAR5" => ctg::FEAR0 + 5,
        "PERSONALLIFE" => ctg::PERSONALLIFE,
        _ => return Effect::None,
    };
    let value = |w: &[String]| w.first().and_then(|v| Value::parse(v));
    match category {
        ctg::EFFECT_FOOD | ctg::EFFECT_WOOD | ctg::EFFECT_STONE => Effect::Nation { category, value: value(rest) },
        ctg::GEOLOGY => Effect::Nation { category, value: None },
        ctg::UNCONSTRUCT => Effect::Building { category },
        c if (ctg::UPSTAGE0..ctg::UPSTAGE0 + 5).contains(&c) => Effect::Building { category },
        ctg::ENABLE => {
            let (use_units, use_upgrades) = match rest.first().map(String::as_str) {
                Some("UNITS") => (true, false),
                Some("UPGRADES") => (false, true),
                Some("UNITS&UPGRADES") => (true, true),
                _ => return Effect::None,
            };
            let (ids, _) = words(&rest[1..]);
            let (mut units, mut upgrades) = (Vec::new(), Vec::new());
            for id in &ids {
                // `GetNTElement`: a unit first, then an upgrade.
                match catalog.find(id).filter(|_| use_units) {
                    Some(t) => units.push(t),
                    None => upgrades.extend(tech.find_upgrade(id).filter(|_| use_upgrades)),
                }
            }
            Effect::Enable { units, upgrades }
        }
        ctg::COST => {
            let (units, rest) = units(catalog, rest);
            let mut values = Vec::new();
            let mut add = |r: Option<&String>, v: Option<&String>| {
                if let (Some(r), Some(v)) = (r.and_then(|r| resource_index(r)), v.and_then(|v| Value::parse(v))) {
                    values.push((r & 7, v));
                }
            };
            if rest.first().map(String::as_str) == Some("GRP") {
                let n = rest.get(1).and_then(|n| n.parse::<usize>().ok()).unwrap_or(0);
                for k in 0..n {
                    add(rest.get(2 + 2 * k), rest.get(3 + 2 * k));
                }
            } else {
                add(rest.first(), rest.get(1));
            }
            Effect::Cost { units, values }
        }
        _ => {
            let (select, rest) = match category {
                ctg::PROTECTION => {
                    let (k, rest) = words(rest);
                    (Select::Protection(k), rest)
                }
                ctg::ATTPAUSE | ctg::WEAPON | ctg::PRECISE | ctg::ATTRANGE | ctg::DAMAGE | ctg::SINGLE_ATTPAUSE => {
                    select01(rest)
                }
                _ => (Select::None, rest),
            };
            let (units, rest) = units(catalog, rest);
            let value = match category {
                ctg::HEALING | ctg::SHAR | ctg::WEAPON => None,
                _ => value(rest),
            };
            Effect::Units { category, units, select, value }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::ObjectType;
    use acr_formats::defs::UnitDef;

    #[test]
    fn values_follow_use_value() {
        let v = |s| Value::parse(s).unwrap();
        assert_eq!(v("+10%"), Value { op: 5, n: 10 });
        assert_eq!(v("-25%").apply(64), 48);
        assert_eq!(v("+1").apply(7), 8);
        assert_eq!(v("400%").apply(30), 120);
        assert_eq!(v("1010").apply(5), 1010);
        assert_eq!(v("-90%").apply(15), 2);
    }

    #[test]
    fn compiles_a_nation() {
        let mut c = Catalog::new();
        for id in ["EngPik(EN)", "EngCen(EN)", "EngKri(EN)"] {
            c.add(ObjectType::from_def(id, &UnitDef::parse(b"NAME X\r\n[END]\r\n")));
        }
        let def = NationDef::parse(
            b"[UPGRADE]\r\nUP1 ICO #POSITION 2 3 #LEVEL 1 MSG #COST 2 GOLD 25 WOOD 70000 #TIME 200\r\n\
              DAMAGE WEAPONKIND PIKA EngPik(EN) +1\r\n\
              UP2 ICO #POSITION 0 0 #LEVEL 2 MSG #COST 0 #TIME 5\r\nPROTECTION GRP 2 MECH PIKA EngPik(EN) +2\r\n\
              UP3 ICO #POSITION 0 0 #LEVEL 2 MSG #COST 0 #TIME 5\r\nCOST EngPik(EN) GRP 2 IRON -5% GOLD -7%\r\n\
              UP4 ICO #POSITION 0 0 #LEVEL 2 MSG #COST 0 #TIME 5\r\nENABLE UNITS GRP 2 EngKri(EN) EngPik(EN)\r\n\
              [FIXED_PRODUCE]\r\nEngCen(EN) 1\r\nEngKri(EN) 0 0\r\n[ENABLED]\r\nEngCen(EN)\r\n\
              [UPGRADEPLACE]\r\nEngCen(EN) 2 UP1\r\nUP2\r\n[UPGRADELINKS]\r\nUP1 1 UP2\r\n[UPGRADEENABLE]\r\nUP1\r\n\
              [ACCESSCONTROL]\r\nEngPik(EN) 2 EngCen(EN) UP1\r\n[STAGEDEPENDENT]\r\nUP2 1\r\n[END]\r\n",
        );
        let mut t = Tech::default();
        t.add_nation(&c, &def);
        let (pik, cen, kri) =
            (c.find("EngPik(EN)").unwrap(), c.find("EngCen(EN)").unwrap(), c.find("EngKri(EN)").unwrap());
        assert_eq!(t.of(cen).unwrap().produce, [kri]);
        assert!(t.of(cen).unwrap().start_enabled);
        assert_eq!(t.of(cen).unwrap().upgrades, [UpgradeId(0), UpgradeId(1)]);
        let up1 = t.upgrade(UpgradeId(0));
        assert_eq!(up1.cost[crate::economy::WOOD], 70000 & 0xFFFF);
        assert!(up1.start_enabled && up1.auto_enable == [UpgradeId(1)]);
        assert_eq!(
            up1.effect,
            Effect::Units {
                category: ctg::DAMAGE,
                units: vec![pik],
                select: Select::WeaponKinds(vec!["PIKA".into()]),
                value: Some(Value { op: 1, n: 1 })
            }
        );
        assert!(
            matches!(&t.upgrade(UpgradeId(1)).effect, Effect::Units { select: Select::Protection(k), .. } if k.len() == 2)
        );
        assert_eq!(t.upgrade(UpgradeId(1)).stage_mask, 1);
        assert!(matches!(&t.upgrade(UpgradeId(2)).effect, Effect::Cost { values, .. } if values.len() == 2));
        assert_eq!(t.upgrade(UpgradeId(3)).effect, Effect::Enable { units: vec![kri, pik], upgrades: vec![] });
        assert_eq!(
            t.access,
            [Access { target: Element::Type(pik), requires: vec![Element::Type(cen), Element::Upgrade(UpgradeId(0))] }]
        );
    }
}
