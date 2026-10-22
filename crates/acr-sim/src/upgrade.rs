//! Researching upgrades in buildings (`PerformUpgrade` 0x54f25b, `PerformUpgradeLink`
//! 0x54f6bb, `StopUpgrade` 0x4e19ad) and their effects (`PerformNewUpgrade` 0x532281 with
//! the category functions 0x5310cf..0x5319f1). The definitions come from the nation files
//! (`tech.rs`). See `docs/simulation.md`.
//!
//! Effects change the player's copy of a type's characteristics ([`Adv`]), so they apply to
//! existing and future objects of that type at once; `LIFE`, `MORELIFE`, `BUILD` and
//! `HEALING` also touch the objects. The `SINGLE_*`, `PERSONALLIFE`, `MOREFARMS_SINGLE` and
//! `UPSTAGEn` kinds act on the building that researched the upgrade.

use std::sync::Arc;

use crate::catalog::{Adv, TypeId};
use crate::economy::stages_done;
use crate::tech::{Effect, Select, UpgradeId, Value, ctg};
use crate::world::{SPEED_SH, World};

/// An upgrade being researched in a building (the `PerformUpgradeLink` order).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Research {
    pub upgrade: UpgradeId,
    /// Progress, from 1, +2 per tick (`Ustage`); done at `stages` (`#TIME`).
    pub stage: i32,
    pub stages: i32,
}

/// Whether building `b` may start researching `up` now (the checks of `PerformUpgrade`):
/// finished and idle (`Ready`), the upgrade is in its `[UPGRADEPLACE]` list and enabled for
/// the owner, the building's stages cover the upgrade's `[STAGEDEPENDENT]` mask, the owner
/// can pay, a `[PRIVATE]` upgrade's `#LEVEL` equals the building's upgrade count, and a
/// stage upgrade's stage is not done yet.
pub fn can_research(w: &World, b: usize, up: UpgradeId) -> bool {
    let tech = &w.catalog.tech;
    let Some(def) = tech.upgrades.get(usize::from(up.0)) else { return false };
    let u = &w.units[b];
    if !u.living() || !u.building || u.unlimited || u.research.is_some() || u.stage < w.max_stage(u) {
        return false;
    }
    let p = &w.players[usize::from(u.owner)];
    if !tech.of(u.kind).is_some_and(|t| t.upgrades.contains(&up)) || !p.upgrades[usize::from(up.0)].enabled {
        return false;
    }
    if !stages_done(u.stages, def.stage_mask) || p.resources.iter().zip(&def.cost).any(|(s, c)| s < c) {
        return false;
    }
    if def.individual && i32::from(u.upgrade_level) != def.level {
        return false;
    }
    if let Effect::Building { category } = def.effect
        && def.stage_up
        && (ctg::UPSTAGE0..ctg::UPSTAGE0 + 5).contains(&category)
        && (u.stages >> (3 * (category - ctg::UPSTAGE0))) & 7 == 2
    {
        return false;
    }
    true
}

/// The upgrades building `b` offers now, in `[UPGRADEPLACE]` order (for the command panel).
pub fn available(w: &World, b: usize) -> Vec<UpgradeId> {
    let u = &w.units[b];
    let Some(t) = w.catalog.tech.of(u.kind) else { return Vec::new() };
    let p = &w.players[usize::from(u.owner)];
    t.upgrades
        .iter()
        .copied()
        .filter(|&up| {
            let def = w.catalog.tech.upgrade(up);
            p.upgrades[usize::from(up.0)].enabled && (!def.individual || i32::from(u.upgrade_level) == def.level)
        })
        .collect()
}

/// `PerformUpgrade` (0x54f25b): pays and starts the research. The research order goes in
/// front of the building's queue: production and mining pause until it is done. A
/// nation-wide upgrade is no longer offered meanwhile (`IsDoing`).
pub fn start(w: &mut World, b: usize, up: UpgradeId) -> bool {
    if !can_research(w, b, up) {
        return false;
    }
    let catalog = w.catalog.clone();
    let def = catalog.tech.upgrade(up);
    let owner = usize::from(w.units[b].owner);
    let p = &mut w.players[owner];
    for (s, c) in p.resources.iter_mut().zip(&def.cost) {
        *s -= c;
    }
    if !(def.individual || def.stage_up) {
        let s = &mut p.upgrades[usize::from(up.0)];
        (s.enabled, s.permanent, s.doing) = (false, false, true);
    }
    w.units[b].research = Some(Research { upgrade: up, stage: 1, stages: def.time });
    true
}

/// `StopUpgrade` (0x4e19ad): cancels the research with a full refund.
pub fn cancel(w: &mut World, b: usize) {
    let Some(r) = w.units[b].research.take() else { return };
    let catalog = w.catalog.clone();
    let def = catalog.tech.upgrade(r.upgrade);
    let p = &mut w.players[usize::from(w.units[b].owner)];
    for (s, c) in p.resources.iter_mut().zip(&def.cost) {
        *s = s.saturating_add(*c);
    }
    if !(def.individual || def.stage_up) {
        let s = &mut p.upgrades[usize::from(r.upgrade.0)];
        (s.enabled, s.permanent, s.doing) = (true, true, false);
    }
}

/// `PerformUpgradeLink` (0x54f6bb), every tick while researching: +2 progress; a stage
/// upgrade marks its stage as in progress (1); at `#TIME` the upgrade is performed with
/// this building as its maker and the building's upgrade count rises.
pub fn research_link(w: &mut World, b: usize) {
    let Some(mut r) = w.units[b].research else { return };
    let catalog = w.catalog.clone();
    let def = catalog.tech.upgrade(r.upgrade);
    r.stage += 1 << SPEED_SH;
    let u = &mut w.units[b];
    if let Effect::Building { category } = def.effect
        && (ctg::UPSTAGE0..ctg::UPSTAGE0 + 5).contains(&category)
    {
        let s = category - ctg::UPSTAGE0;
        if catalog.get(u.kind).economy.sections.iter().any(|x| x.0 == s) {
            let sh = 3 * u16::from(s);
            u.stages = (u.stages & !(7 << sh)) | (1 << sh);
        }
    }
    if r.stage < r.stages {
        u.research = Some(r);
        return;
    }
    u.research = None;
    let owner = u.owner;
    if !(def.individual || def.stage_up) {
        w.players[usize::from(owner)].upgrades[usize::from(r.upgrade.0)].done = true;
    }
    perform(w, owner, r.upgrade, Some(b));
    let u = &mut w.units[b];
    u.upgrade_level = u.upgrade_level.wrapping_add(1);
}

/// The attack indices or protection kinds an effect touches on a type (`ApplyToOneUnit`
/// 0x531d74).
enum Slots {
    Indices(Vec<usize>),
    Kinds(Vec<String>),
}

fn slots(adv: &Adv, select: &Select) -> Slots {
    let n = adv.attacks.len();
    match select {
        Select::None => Slots::Indices(vec![0]),
        Select::All => Slots::Indices((0..n).collect()),
        Select::AttIndex(v) => Slots::Indices(v.iter().copied().filter(|&i| i < n).collect()),
        Select::WeaponKinds(kinds) => Slots::Indices(
            kinds.iter().flat_map(|k| (0..n).filter(move |&i| adv.attacks[i].kind.eq_ignore_ascii_case(k))).collect(),
        ),
        Select::Protection(kinds) => Slots::Kinds(kinds.iter().map(|k| k.to_ascii_uppercase()).collect()),
    }
}

/// The player's characteristics of `kind`, to change.
fn adv_mut(w: &mut World, player: u8, kind: TypeId) -> &mut Adv {
    let base = w.catalog.get(kind).adv.clone();
    Arc::make_mut(w.players[usize::from(player)].adv.entry(kind).or_insert(base))
}

/// The objects of `player` of type `kind`, existing or hidden.
fn objects_of(w: &World, player: u8, kind: TypeId) -> Vec<usize> {
    (0..w.units.len()).filter(|&i| w.units[i].alive && w.units[i].owner == player && w.units[i].kind == kind).collect()
}

/// `PerformNewUpgrade` (0x532281): applies an upgrade's effect for `player`, then enables
/// its `[UPGRADELINKS]` and performs its `[AUTOUPGRADE]`s. `maker` is the building that
/// researched it.
pub fn perform(w: &mut World, player: u8, up: UpgradeId, maker: Option<usize>) {
    let catalog = w.catalog.clone();
    let def = catalog.tech.upgrade(up);
    match &def.effect {
        Effect::Units { category, units, select, value } => {
            let v = value.unwrap_or(Value { op: 3, n: 0 });
            for &kind in units {
                let slots = slots(&w.adv(player, kind), select);
                apply_unit(w, player, kind, *category, &slots, v, maker);
            }
        }
        Effect::Cost { units, values } => {
            for &kind in units {
                for &(r, v) in values {
                    let a = adv_mut(w, player, kind);
                    a.cost[r] = v.apply(a.cost[r]);
                }
            }
        }
        Effect::Enable { units, .. } => {
            // `UpgradeEnable` (0x531801); the engine cannot enable upgrades this way.
            for &kind in units {
                let p = &mut w.players[usize::from(player)];
                if let (Some(e), Some(c)) =
                    (p.enabled.get_mut(usize::from(kind.0)), p.cond_enabled.get_mut(usize::from(kind.0)))
                {
                    (*e, *c) = (true, true);
                }
            }
            crate::economy::census_all(w);
        }
        Effect::Nation { category, value } => {
            let p = &mut w.players[usize::from(player)];
            let v = value.unwrap_or(Value { op: 3, n: 0 });
            // u16 fields, without the caps of Cossacks.
            match *category {
                ctg::EFFECT_FOOD => p.food_eff = i32::from(v.apply(p.food_eff) as u16),
                ctg::EFFECT_WOOD => p.wood_eff = i32::from(v.apply(p.wood_eff) as u16),
                ctg::EFFECT_STONE => p.stone_eff = i32::from(v.apply(p.stone_eff) as u16),
                _ => p.geology = true,
            }
        }
        Effect::Building { category } => {
            if let Some(b) = maker.filter(|_| (ctg::UPSTAGE0..ctg::UPSTAGE0 + 5).contains(category)) {
                upstage(w, b, category - ctg::UPSTAGE0);
            }
            // UNCONSTRUCT (0x4a0f76) is not reproduced.
        }
        Effect::None => {}
    }
    for &next in &def.auto_enable {
        let s = &mut w.players[usize::from(player)].upgrades[usize::from(next.0)];
        (s.enabled, s.permanent, s.done) = (true, true, false);
    }
    for &next in &def.auto_perform {
        perform(w, player, next, maker);
    }
}

/// One unit category for one type (the functions behind `ApplyToOneUnitAndOneCategory`),
/// storing results with the engine's field widths.
fn apply_unit(w: &mut World, player: u8, kind: TypeId, category: u8, slots: &Slots, v: Value, maker: Option<usize>) {
    let u16w = |x: i32| i32::from(x as u16);
    let i16w = |x: i32| i32::from(x as i16);
    match (category, slots) {
        (ctg::SHIELD, _) => {
            let a = adv_mut(w, player, kind);
            a.shield = u16w(v.apply(a.shield));
        }
        (ctg::PROTECTION, Slots::Kinds(kinds)) => {
            let a = adv_mut(w, player, kind);
            for k in kinds {
                match a.protection.iter_mut().find(|p| &p.0 == k) {
                    Some(p) => p.1 = i32::from(v.apply(p.1) as u8),
                    None => a.protection.push((k.clone(), i32::from(v.apply(0) as u8))),
                }
            }
        }
        (ctg::ATTPAUSE, Slots::Indices(ix)) => {
            let a = adv_mut(w, player, kind);
            for &i in ix {
                a.attacks[i].pause = u16w(v.apply(a.attacks[i].pause));
            }
        }
        (ctg::DAMAGE, Slots::Indices(ix)) => {
            let a = adv_mut(w, player, kind);
            for &i in ix {
                // `UpgradeDamage` leaves indices without damage alone.
                if a.attacks[i].damage != 0 {
                    a.attacks[i].damage = i16w(v.apply(a.attacks[i].damage));
                }
            }
        }
        (ctg::ATTRANGE, Slots::Indices(ix)) => {
            let a = adv_mut(w, player, kind);
            for &c in ix {
                a.attacks[c].radius.1 = i16w(v.apply(a.attacks[c].radius.1));
                a.attacks[c].detect.1 = i16w(v.apply(a.attacks[c].detect.1));
                if c + 1 < a.attacks.len() {
                    // The next index's minimum range: the engine stores the scaled DetR1 over
                    // the scaled AttR1, leaving DetR1 itself unchanged.
                    a.attacks[c + 1].radius.0 = i16w(v.apply(a.attacks[c + 1].detect.0));
                }
            }
        }
        (ctg::BUILD, _) => {
            let a = adv_mut(w, player, kind);
            let old = a.build_stages;
            a.build_stages = u16w(v.apply(old));
            let new = a.build_stages;
            for b in objects_of(w, player, kind) {
                let u = &mut w.units[b];
                if !u.building {
                    continue;
                }
                if u.stage >= old {
                    u.stage = new;
                } else if u.stage >= new || u.stage > 0 {
                    u.stage = if u.stage >= new { new - 1 } else { u.stage - 1 };
                    crate::construction::next_stage(w, b);
                }
            }
        }
        (ctg::LIFE, _) => {
            let a = adv_mut(w, player, kind);
            a.life = v.apply(a.life).min(65535);
            let life = a.life;
            for b in objects_of(w, player, kind) {
                (w.units[b].max_life, w.units[b].life) = (life, life);
            }
        }
        (ctg::MORELIFE, _) => {
            let a = adv_mut(w, player, kind);
            a.life = u16w(v.apply(a.life));
            let life = a.life;
            for b in objects_of(w, player, kind) {
                w.units[b].max_life = life;
            }
        }
        (ctg::INSIDE, _) => {
            let a = adv_mut(w, player, kind);
            a.max_inside = u16w(v.apply(a.max_inside));
        }
        (ctg::RAZBROS, _) => {
            let a = adv_mut(w, player, kind);
            a.razbros = i16w(v.apply(a.razbros));
        }
        (ctg::MOREFARMS, _) => {
            let a = adv_mut(w, player, kind);
            a.farm = u16w(v.apply(a.farm));
        }
        (ctg::HEALING, _) => {
            for b in objects_of(w, player, kind) {
                let u = &mut w.units[b];
                if u.living() {
                    u.life = (u.life + u.max_life / 10).min(u.max_life);
                }
            }
        }
        (ctg::SINGLE_INSIDE, _) => {
            if let Some(b) = maker {
                w.units[b].add_inside = u16w(v.apply(w.units[b].add_inside));
            }
        }
        (ctg::MOREFARMS_SINGLE, _) => {
            if let Some(b) = maker {
                // `OB+0xf3 = (u8) UseValue(ADC farm)`: the extra is the changed housing.
                w.units[b].farm_extra = i32::from(v.apply(w.adv(player, kind).farm) as u8);
            }
        }
        (ctg::PERSONALLIFE, _) => {
            if let Some(b) = maker {
                let u = &mut w.units[b];
                u.life = v.apply(u.life).min(65535);
                u.max_life = u.life;
            }
        }
        // SPEED (`Rate`, which no motion code here reads), WEAPON, PRECISE, GETRES,
        // SINGLE_ATTPAUSE, FISHING, SHAR and FEAR have no effect in this simulation.
        _ => {}
    }
}

/// `UPSTAGEn` on the researching building (0x53268e): if the building has that stage
/// (`ADDSECTION`), it becomes done (2); life becomes the type's life plus the life of every
/// done stage, and so does the maximum.
fn upstage(w: &mut World, b: usize, s: u8) {
    let catalog = w.catalog.clone();
    let u = &w.units[b];
    let t = catalog.get(u.kind);
    if !t.economy.sections.iter().any(|x| x.0 == s) {
        return;
    }
    let life = w.adv(u.owner, u.kind).life;
    let u = &mut w.units[b];
    let sh = 3 * u16::from(s);
    u.stages = (u.stages & !(7 << sh)) | (2 << sh);
    let mut l = life;
    for &(k, add) in &t.economy.sections {
        if (u.stages >> (3 * u16::from(k))) & 7 == 2 {
            l += add;
        }
    }
    u.life = i32::from(l as u16);
    u.max_life = u.life;
}
