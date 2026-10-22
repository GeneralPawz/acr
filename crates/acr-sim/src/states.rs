//! The combat states players and scripts switch on units: stand ground, hold fire
//! (`NoSearchVictim`), melee only, friendly-fire avoidance and the AI guard. See
//! `docs/simulation.md` (Combat, unit states).

use crate::world::{EntityId, World};

/// The living units among `ids`, in the order given, without repeats.
fn living(w: &World, ids: &[EntityId]) -> Vec<usize> {
    let mut seen = Vec::new();
    for id in ids {
        let i = id.0 as usize;
        if w.units.get(i).is_some_and(|u| u.living() && !u.building) && !seen.contains(&i) {
            seen.push(i);
        }
    }
    seen
}

/// The members of unit `i`'s brigade (alive and still in it), or just `i`.
fn with_brigade(w: &World, i: usize) -> Vec<usize> {
    let Some(b) = w.units[i].brigade else { return vec![i] };
    w.brigades[usize::from(b)]
        .members
        .iter()
        .flatten()
        .map(|e| e.0 as usize)
        .filter(|&k| w.units.get(k).is_some_and(|u| u.living() && u.brigade == Some(b)))
        .collect()
}

/// `ClearOrders` (0x4efad1) for a unit that is attacking (`EnemyID != 0xFFFF`).
fn stop_attack(w: &mut World, i: usize) {
    let u = &mut w.units[i];
    if u.target().is_some() {
        u.orders.clear();
        u.path.clear();
        u.prio = 0;
    }
}

/// `MakeStandGround` / `CancelStandGround` (0x4e09c7 / 0x4e0e70): free units get the flag;
/// a formation member switches its whole formation (`MakeStandGroundTemp` 0x4dfe94,
/// `CancelStandGround` 0x4e0ad7). A standing unit attacks only what is in range.
pub fn set_stand_ground(w: &mut World, ids: &[EntityId], on: bool) {
    for i in living(w, ids) {
        for k in with_brigade(w, i) {
            w.units[k].stand_ground = on;
        }
    }
}

/// `SetSearchVictim(nat, allow ? 2 : 3)` (0x4e0330), the script's `AllowAttack`: free units
/// get `NoSearchVictim = !allow`, and attacking units stop either way; a formation member
/// sets `NoSearchVictim` on its whole formation even when `allow` is true (`SetAttState`
/// 0x4e0212 is passed `Val != 0`, an engine quirk) and its attacking members stop.
pub fn allow_attack(w: &mut World, ids: &[EntityId], allow: bool) {
    for i in living(w, ids) {
        if w.units[i].brigade.is_some() {
            for k in with_brigade(w, i) {
                w.units[k].no_search_victim = true;
                stop_attack(w, k);
            }
        } else {
            w.units[i].no_search_victim = !allow;
            stop_attack(w, i);
        }
    }
}

/// The interface's hold-fire toggle (`SetSearchVictim(nat, 0/1)`): `NoSearchVictim = hold`,
/// and attacking units stop; formation members switch their formation.
pub fn hold_fire(w: &mut World, ids: &[EntityId], hold: bool) {
    for i in living(w, ids) {
        for k in with_brigade(w, i) {
            w.units[k].no_search_victim = hold;
            stop_attack(w, k);
        }
    }
}

/// `SetUnitsState` (0x578c09): melee only (`ARMATTACK` units), friendly-fire avoidance off
/// (`FRIENDLYFIRE` units), `NoSearchVictim = !allow_attack` (attacks go on), and stand
/// ground: on for free units or the whole formation, off only for free units.
pub fn set_units_state(
    w: &mut World,
    ids: &[EntityId],
    allow_attack: bool,
    stand_ground: bool,
    melee_only: bool,
    no_ff_avoid: bool,
) {
    let catalog = w.catalog.clone();
    for i in living(w, ids) {
        let t = catalog.get(w.units[i].kind);
        let u = &mut w.units[i];
        if t.role.arm_attack {
            u.melee_only = melee_only;
        }
        if t.avoid_friendly_fire {
            u.no_ff_avoid = no_ff_avoid;
        }
        u.no_search_victim = !allow_attack;
        if stand_ground {
            for k in with_brigade(w, i) {
                w.units[k].stand_ground = true;
            }
        } else if w.units[i].brigade.is_none() {
            w.units[i].stand_ground = false;
        }
    }
}

/// `SetAI_Guard` (0x570251): guards never search targets, others' searches skip them unless
/// they attack, and units that try to take a guarded building die.
pub fn set_guard(w: &mut World, ids: &[EntityId], on: bool) {
    for id in ids {
        if let Some(u) = w.units.get_mut(id.0 as usize).filter(|u| u.living()) {
            u.guard = on;
        }
    }
}
