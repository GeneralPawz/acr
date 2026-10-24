//! Mortars with engagement zones (#301): an optional rule that is not in the original game.
//! With [`crate::Rules::mortars`] on, [`append_types`] adds an `ArtMrt(XX)` type after the
//! original types for every `ArtPus(XX)` field gun: the same gun without canister, firing the
//! original's unused mortar shell `NUCL3M` (`WEAPON.NDS`, weapon kind `IADROMOR`) on a high arc
//! ([`crate::artillery::GunMode::Mortar`]) from [`MIN_RANGE`] to [`MAX_RANGE`] px, with a slower
//! reload. Scatter grows with the range like every gun's (`RAZBROS`, `CreateRazbros` 0x51b917).
//!
//! A mortar can be given up to [`MAX_ZONES`] engagement zones ([`crate::Command::MortarZones`])
//! and told to shell them ([`crate::Command::MortarFire`]): whenever it is idle, it lays one
//! shot ([`crate::artillery::attack_point`], one round) on the centre of the enemies in the
//! next zone that holds any, taking the zones in turn and skipping those nearer than its
//! minimum range or beyond its reach.
//!
//! Off (the default), no type is added, nothing here runs, no random number is drawn and the
//! state hash is unchanged. See `docs/simulation.md`, "Mortar".

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::World;
use crate::catalog::{Attack, Catalog, ObjectType, TypeId};
use crate::tech::{Access, Element};
use crate::trig::norma;
use crate::world::EntityId;

/// The `.MD` name of the field gun a mortar is made from.
pub const GUN: &str = "ArtPus";
/// The member id prefix of the mortars (`ArtMrt(EN)` from `ArtPus(EN)`).
pub const MORTAR: &str = "ArtMrt";
/// The shell (`WEAPON.NDS` line `NUCL3M #NUCL2 800 60 NONE HI_GRAVITY3 ANGLE 75 -1`): round
/// damage 800; at the burst its sync weapons throw the `NUCLM` fragments and the `PU2` blast.
pub const WEAPON: &str = "NUCL3M";
/// The weapon kind (`NRES.DAT [WEAPONS]`): a bursting shell
/// ([`crate::combat::DeathCause::Explosion`]; with hit locations also on, fragments and ground fire).
pub const KIND: &str = "IADROMOR";
/// The nearest a mortar fires (world px): closer the shell would fall on its own crew.
pub const MIN_RANGE: i32 = 600;
/// The farthest (world px; the field gun's ball reaches 6000).
pub const MAX_RANGE: i32 = 4000;
/// The reload in percent of the field gun's `ATTACK_PAUSE`.
pub const RELOAD_PCT: i32 = 150;
/// The elevation of the shell's flight (degrees; the weapon's own `ANGLE` is 75).
pub const ELEVATION: i32 = 60;
/// Zones per mortar at most.
pub const MAX_ZONES: usize = 8;
/// A zone's radius (world px): at least, at most.
pub const ZONE_RADIUS: (i32, i32) = (32, 800);

/// One engagement zone: a circle on the ground (world px).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Zone {
    pub x: i32,
    pub y: i32,
    pub r: i32,
}

/// A mortar's zones and whether it shells them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Mortar {
    pub zones: Vec<Zone>,
    /// The zone it looks at first next time.
    pub next: u8,
    pub firing: bool,
}

/// The zones of every mortar that has some, by its id (`World::mortar_zones`, hashed only
/// while not empty).
pub type Zones = BTreeMap<EntityId, Mortar>;

/// The member id of the mortar made from field gun `id` (`ArtPus(EN)` → `ArtMrt(EN)`).
pub fn mortar_id(id: &str) -> String {
    match id.get(..GUN.len()) {
        Some(p) if p.eq_ignore_ascii_case(GUN) => format!("{MORTAR}{}", &id[GUN.len()..]),
        _ => format!("{MORTAR}({id})"),
    }
}

/// The attack index of a mortar's shell.
pub fn shell_index(t: &ObjectType) -> Option<usize> {
    if !t.mortar {
        return None;
    }
    t.attacks.iter().position(|a| a.kind == KIND)
}

/// Appends an `ArtMrt(XX)` type for every `ArtPus(XX)` field gun of the catalog (once): a copy
/// without canister whose ball attack fires [`WEAPON`] from [`MIN_RANGE`] to [`MAX_RANGE`] px
/// with [`RELOAD_PCT`] % of the reload. It gets the gun's tech entry and access rules and
/// follows it in every build list that holds it. Call it only with the rule on, after the
/// weapons are set and before the world is made: the original types keep their ids.
pub fn append_types(c: &mut Catalog) {
    let guns: Vec<TypeId> = c.iter().filter(|(_, t)| t.md_name.eq_ignore_ascii_case(GUN)).map(|(id, _)| id).collect();
    let weapon = c.weapons.find(WEAPON);
    for g in guns {
        let mut t = c.get(g).clone();
        let id = mortar_id(&t.id);
        if c.find(&id).is_some() {
            continue;
        }
        let Some(ball) = crate::artillery::ball_index(&t) else { continue };
        let can = t.sector_weapon.map(|s| usize::from(s.att)).filter(|&k| k != ball);
        t.id = id;
        t.mortar = true;
        t.sector_weapon = None;
        patch(&mut t.attacks, ball, can, weapon);
        patch(&mut Arc::make_mut(&mut t.adv).attacks, ball, can, weapon);
        let m = c.add(t);
        let tech = &mut c.tech;
        if let Some(tt) = tech.types.get(usize::from(g.0)).cloned() {
            let i = usize::from(m.0);
            if tech.types.len() <= i {
                tech.types.resize(i + 1, Default::default());
            }
            tech.types[i] = crate::tech::TypeTech { produce: Vec::new(), upgrades: Vec::new(), ..tt };
        }
        for tt in &mut tech.types {
            if let Some(p) = tt.produce.iter().position(|&x| x == g) {
                tt.produce.insert(p + 1, m);
            }
        }
        let more: Vec<Access> = tech
            .access
            .iter()
            .filter(|a| a.target == Element::Type(g))
            .map(|a| Access { target: Element::Type(m), requires: a.requires.clone() })
            .collect();
        tech.access.extend(more);
    }
}

/// The mortar's ball attack `ball` fires the shell; the canister `can` goes.
fn patch(attacks: &mut [Attack], ball: usize, can: Option<usize>, weapon: Option<u16>) {
    if let Some(a) = attacks.get_mut(ball) {
        a.radius = (MIN_RANGE, MAX_RANGE);
        a.detect = (MIN_RANGE, MAX_RANGE);
        a.pause = a.pause * RELOAD_PCT / 100;
        a.kind = KIND.into();
        a.weapon = Some(WEAPON.into());
        a.weapon_id = weapon;
    }
    if let Some(a) = can.and_then(|k| attacks.get_mut(k)) {
        *a = Attack::default();
    }
}

/// Whether a round of a unit of type `kind` is a mortar's shell. Only the rule adds
/// mortar types, so off this is always false. Its shell starts no ground fire
/// ([`crate::ground_fire::shell_burst`]) and is drawn as a smaller burst (render only).
pub fn fired_by(kind: Option<TypeId>, c: &Catalog) -> bool {
    kind.is_some_and(|k| c.get(k).mortar)
}

/// Whether unit `i` is a mortar the rule lets take zones.
fn is_mortar(w: &World, i: usize) -> bool {
    w.rules.mortars && w.units.get(i).is_some_and(|u| w.catalog.get(u.kind).mortar)
}

/// [`crate::Command::MortarZones`] for unit `i`: its zones (radius clamped, at most
/// [`MAX_ZONES`]); none clears them and stops its fire.
pub fn set_zones(w: &mut World, i: usize, zones: &[Zone]) {
    if !is_mortar(w, i) {
        return;
    }
    let id = w.units[i].id;
    if zones.is_empty() {
        w.mortar_zones.remove(&id);
        return;
    }
    let zones =
        zones.iter().take(MAX_ZONES).map(|z| Zone { r: z.r.clamp(ZONE_RADIUS.0, ZONE_RADIUS.1), ..*z }).collect();
    let m = w.mortar_zones.entry(id).or_default();
    m.zones = zones;
    m.next = 0;
}

/// [`crate::Command::MortarFire`] for unit `i`: shell its zones or stop.
pub fn set_fire(w: &mut World, i: usize, on: bool) {
    if !is_mortar(w, i) {
        return;
    }
    let id = w.units[i].id;
    if let Some(m) = w.mortar_zones.get_mut(&id) {
        m.firing = on;
    }
}

/// The centre (world px) of the enemies of unit `i` standing in zone `z`, by unit index.
fn enemy_centre(w: &World, i: usize, z: Zone) -> Option<(i32, i32)> {
    let u = &w.units[i];
    let (mut sx, mut sy, mut n) = (0i64, 0i64, 0i64);
    for e in &w.units {
        if !e.active() || e.building || !u.hostile(e) {
            continue;
        }
        let (x, y) = e.pos.pixels();
        if norma(x - z.x, y - z.y) <= z.r {
            (sx, sy, n) = (sx + i64::from(x), sy + i64::from(y), n + 1);
        }
    }
    (n > 0).then(|| ((sx / n) as i32, (sy / n) as i32))
}

/// Once per tick, after the commands: every firing mortar that is idle lays a shot on the
/// next zone with enemies in its reach. Drops the zones of mortars that are gone.
pub fn tick(w: &mut World) {
    if !w.rules.mortars || w.mortar_zones.is_empty() {
        return;
    }
    let ids: Vec<EntityId> = w.mortar_zones.keys().copied().collect();
    for id in ids {
        let i = id.0 as usize;
        if !w.units.get(i).is_some_and(|u| u.id == id && u.living()) || !is_mortar(w, i) {
            w.mortar_zones.remove(&id);
            continue;
        }
        let m = &w.mortar_zones[&id];
        let u = &w.units[i];
        if !m.firing || !u.active() || !u.orders.is_empty() {
            continue;
        }
        let t = w.catalog.get(u.kind);
        let adv = w.adv(u.owner, u.kind);
        let Some(reach) = shell_index(t).and_then(|k| adv.attacks.get(k)).map(|a| a.radius) else { continue };
        let (ux, uy) = u.pos.pixels();
        let n = m.zones.len();
        let start = usize::from(m.next);
        let mut aim = None;
        for k in 0..n {
            let zi = (start + k) % n;
            if let Some((x, y)) = enemy_centre(w, i, m.zones[zi]) {
                let d = norma(x - ux, y - uy);
                if d >= reach.0.max(MIN_RANGE) && d <= reach.1 {
                    aim = Some((zi, x, y));
                    break;
                }
            }
        }
        if let Some((zi, x, y)) = aim
            && crate::artillery::attack_point(w, i, x, y, 0x90, crate::combat::Placement::Replace, 1)
            && let Some(m) = w.mortar_zones.get_mut(&id)
        {
            m.next = ((zi + 1) % n) as u8;
        }
    }
}
