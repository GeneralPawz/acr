//! Artillery (the cannon `ArtPus`): canister (`WEAPONINSECTOR`, sector damage 0x52d6b8 and
//! the automatic volley in `SearchVictim` 0x526349), attacks on a point (`NewAttackPoint`
//! 0x51b4aa, link 0x51bc5b) and the cannon's motion (`MOTIONSTYLE COMPLEXROTATE`,
//! 0x50f7cb): it drives in stance 1 and turns in 22.5° steps with its turn animations in
//! stance 2. See `docs/simulation.md`.

use crate::catalog::{ObjectType, SectorWeapon};
use crate::fixed::Pos;
use crate::motion::{anim_frames, set_anim, try_to_stand};
use crate::trig::{cos, get_dir, norma, sin};
use crate::world::{AnimKind, FRM_DEC, Order, Unit, World};

/// `InSector` (0x52d0b8): an offset (px) within `r1..=r2` and the sector around `dir`.
fn in_sector(s: &SectorWeapon, r2: i32, dir: u8, dx: i32, dy: i32) -> bool {
    let d = norma(dx, dy);
    s.r1 <= d && d <= r2 && i32::from((get_dir(dx, dy).wrapping_sub(dir) as i8).unsigned_abs()) <= s.sector
}

/// The objects of the 128 px cells a sector touches (`SectorIter` 0x52d212): cells with a
/// corner inside the sector (`r2` capped at 1600 px), row by row, living objects in cell order.
fn sector_objects(w: &World, x: i32, y: i32, s: &SectorWeapon, r2: i32, dir: u8) -> Vec<usize> {
    let r = r2.min(1600);
    let h = (r + 256) >> 7;
    let n = 2 * h + 1;
    let (cx0, cy0) = ((x >> 7) - h, (y >> 7) - h);
    let corner: Vec<bool> = (0..n)
        .flat_map(|j| (0..n).map(move |i| (i, j)))
        .map(|(i, j)| in_sector(s, r2, dir, ((cx0 + i) << 7) - x, ((cy0 + j) << 7) - y))
        .collect();
    let mut out = Vec::new();
    for j in 0..2 * h {
        for i in 0..2 * h {
            let k = (j * n + i) as usize;
            let nn = n as usize;
            if corner[k] || corner[k + 1] || corner[k + nn] || corner[k + nn + 1] {
                // The engine's cell lists carry a one-cell border: (cx0 + i + 1) is cell cx0 + i.
                out.extend(w.buckets.all(cx0 + i, cy0 + j).map(|u| u as usize).filter(|&u| w.units[u].living()));
            }
        }
    }
    out
}

/// Canister (0x52d6b8): every object in the sector, friend or foe, takes `d1` falling
/// linearly to `d2` from `r1` to `r2`, scaled down to `pct` % at the sector's edge.
pub fn sector_damage(w: &mut World, i: usize) {
    let u = &w.units[i];
    let Some(s) = w.catalog.get(u.kind).sector_weapon else { return };
    let (x, y) = u.pos.pixels();
    let dir = u.dir;
    for k in sector_objects(w, x, y, &s, s.r2, dir) {
        let o = &w.units[k];
        let (dx, dy) = ((o.pos.x >> 4) - x, (o.pos.y >> 4) - y);
        let d = norma(dx, dy);
        if d < s.r1 || d > s.r2 || s.r2 == s.r1 || s.sector == 0 {
            continue;
        }
        let ad = i32::from((get_dir(dx, dy).wrapping_sub(dir) as i8).unsigned_abs());
        if ad > s.sector {
            continue;
        }
        let v = s.d1 - (d - s.r1) * (s.d1 - s.d2) / (s.r2 - s.r1);
        let dmg = v * (100 - ad * (100 - s.pct) / s.sector) / 100;
        if dmg != 0 {
            crate::combat::make_damage(w, k, dmg, Some(i), s.att);
        }
    }
}

/// The automatic canister of `SearchVictim` (0x526349): a deployed cannon without orders,
/// reloaded and at the end of its animation fires one volley ahead when more than 3 enemies
/// and more than twice as many enemies as friends are in the cells of its sector out to
/// `SEARCH_ENEMY_RADIUS`.
pub fn auto_canister(w: &mut World, i: usize) {
    let u = &w.units[i];
    let t = w.catalog.get(u.kind);
    let Some(s) = t.sector_weapon else { return };
    if !u.orders.is_empty() || u.prio >= 15 || u.local_stance == 0 || u.delay != 0 || u.frame < u.anim_frames - FRM_DEC
    {
        return;
    }
    let (x, y) = u.pos.pixels();
    let (mut enemies, mut friends) = (0, 0);
    for k in sector_objects(w, x, y, &s, t.search_radius >> 4, u.dir) {
        if w.units[k].nmask & u.nmask != 0 {
            friends += 1;
        } else {
            enemies += 1;
        }
    }
    if enemies > 3 && enemies > 2 * friends {
        let (px, py) = (x + cos(u.dir), y + sin(u.dir));
        attack_point(w, i, px, py, 0x90, crate::combat::Placement::Replace, 1);
    }
}

/// The `KARTECH` button's executor (0x4dc8fa): a unit with canister clears its orders and
/// attacks the point 256 px ahead once (`NewAttackPoint(.., 0x90, 0, 1)`).
pub fn fire_canister(w: &mut World, i: usize) {
    let u = &w.units[i];
    if w.catalog.get(u.kind).sector_weapon.is_none() {
        return;
    }
    let (x, y) = u.pos.pixels();
    let dir = u.dir;
    let u = &mut w.units[i];
    u.orders.clear();
    u.prio = 0;
    attack_point(w, i, x + cos(dir), y + sin(dir), 0x90, crate::combat::Placement::Replace, 1);
}

/// `NewAttackPoint` (0x51b4aa): an attack on the ground at `(x, y)` (px) for `times` shots
/// (0: until stopped). Refused for objects that cannot act, garrison buildings, a priority
/// below the unit's or a unit that is producing.
pub fn attack_point(
    w: &mut World,
    i: usize,
    x: i32,
    y: i32,
    prio: u8,
    placement: crate::combat::Placement,
    times: u16,
) -> bool {
    let Some(u) = w.units.get(i) else { return false };
    let t = w.catalog.get(u.kind);
    if !u.active() || u.unlimited || (u.building && !u.queue.is_empty()) || w.adv(u.owner, u.kind).max_inside != 0 {
        return false;
    }
    if !t.attacks.iter().any(|a| a.usable()) || prio < u.prio && !u.orders.is_empty() {
        return false;
    }
    let u = &mut w.units[i];
    let prio = if prio == 0xFE {
        u.stand_ground = false;
        0x90
    } else {
        prio
    } & 0x7f;
    let placement = if u.in_patrol && placement == crate::combat::Placement::Replace {
        crate::combat::Placement::Front
    } else {
        placement
    };
    placement.push(&mut u.orders, Order::AttackPoint { x, y, prio, times });
    u.prio = prio;
    u.stand_time = 0;
    u.path.delay = 0;
    true
}

/// `q(d)` (0x50f7af): the nearest of 16 directions.
fn q16(d: u8) -> i32 {
    ((i32::from(d) + 0x3c8) >> 4) & 15
}

/// The `COMPLEXROTATE` motion handler (0x50f7cb). At the end of each animation cycle: a
/// cannon facing its steering point within a 16th drives (stance 1, `#MOTION_L0/R0`);
/// otherwise it changes to stance 2 and turns 16 per turn animation (`#MOTION_L1`: +16 at
/// its end, `#MOTION_R1`: −16 at its start); without a target it stands.
pub fn complex_rotate(w: &mut World, i: usize) {
    let catalog = w.catalog.clone();
    let map = w.map.clone();
    let adv = w.adv(w.units[i].owner, w.units[i].kind);
    let World { units, occupancy, random, .. } = w;
    let u = &mut units[i];
    let t: &ObjectType = catalog.get(u.kind);
    if u.orders.is_empty() {
        u.dest = None;
    }
    u.path.delay = (i32::from(u.path.delay) - FRM_DEC).max(0) as u8;
    if u.frame < u.anim_frames - FRM_DEC {
        if u.in_motion {
            u.pos.x += u.vel.0 << 1;
            u.pos.y += u.vel.1 << 1;
        }
        u.frame += FRM_DEC;
        return;
    }
    let Some(dest) = u.dest else {
        try_to_stand(u, t, random, true);
        return;
    };
    let gd = get_dir(dest.x - u.pos.x, dest.y - u.pos.y);
    let dd = gd.wrapping_sub(u.dir) as i8;
    if q16(u.dir) != q16(gd) {
        if u.local_stance == 2 {
            if u.anim == AnimKind::MotionLs(1) {
                u.dir = u.dir.wrapping_add(16);
                try_to_stand(u, t, random, false);
                return;
            }
            if u.anim == AnimKind::MotionRs(1) {
                try_to_stand(u, t, random, false);
                return;
            }
            if dd > 0 {
                set_anim(u, t, AnimKind::MotionLs(1));
            } else {
                set_anim(u, t, AnimKind::MotionRs(1));
                u.dir = u.dir.wrapping_sub(16);
            }
            u.frame = 0;
            u.in_motion = false;
        } else {
            (u.stance, u.ground_state) = (2, 2);
            try_to_stand(u, t, random, false);
        }
        return;
    }
    u.dir = gd;
    match u.local_stance {
        2 => {
            (u.stance, u.ground_state) = (0, 0);
            try_to_stand(u, t, random, false);
        }
        1 => {
            // A drive step (`TryToMove` 0x50b021): the cell at the end of the cycle must be
            // free, except in unlimited motion (a cannon leaving the building that made it).
            random.rando();
            let rate = adv.rate[0];
            let (odx, ody) = (((t.step_dx(gd) << 1) * rate) >> 4, ((t.step_dy(gd) << 1) * rate) >> 4);
            let nf = t.motion_frames() >> 1;
            let end = Pos::new(u.pos.x + nf * odx, u.pos.y + nf * ody);
            let (cx, cy) = Unit::cell_of(end, u.lx);
            occupancy.clr_bar(u.cell.0, u.cell.1, u.lx);
            let kind = if u.left_leg { AnimKind::MotionLs(0) } else { AnimKind::MotionRs(0) };
            if !u.unlimited && (occupancy.check_bar(cx, cy, u.lx, u.lx) || map.land.check_bar(cx, cy, u.lx, u.lx))
                || anim_frames(t, kind).is_none()
            {
                occupancy.set_bar(u.cell.0, u.cell.1, u.lx);
                u.in_motion = false;
                return;
            }
            set_anim(u, t, kind);
            u.left_leg = !u.left_leg;
            u.vel = (odx >> 1, ody >> 1);
            u.pos.x += odx;
            u.pos.y += ody;
            u.in_motion = true;
            u.frame = FRM_DEC - 1;
            u.stand_time = 0;
            u.cell = (cx, cy);
            occupancy.set_bar(cx, cy, u.lx);
        }
        _ => {
            (u.stance, u.ground_state) = (1, 1);
            try_to_stand(u, t, random, false);
        }
    }
}

/// A cannon's fire mode with [`crate::Rules::hit_locations`] on (`docs/damage-model.md`,
/// "Gun modes"). Without the rule the original picks the attack index by distance alone
/// (canister `KARTECH` within its 1500 px, the ball `IADRO` from 1500 to 6000 px).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum GunMode {
    /// Canister: the `KARTECH` / `PREKARTECH` buttons, the automatic volley and an attack on
    /// men within the canister's reach. Unchanged from the original.
    Canister = 1,
    /// Ground fire (`ATTACKGROUND`): a ball at low elevation that first grazes at the point
    /// and skips on along the lane behind it.
    Ground = 2,
    /// Direct fire (an attack on a building, or on men beyond canister reach): a flatter
    /// ball straight at the aim point with half the dispersion; a miss still bounces on.
    Direct = 3,
}

impl GunMode {
    /// The mode a projectile carries ([`crate::projectile::Projectile::shot`]), 0 for none.
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            1 => Some(Self::Canister),
            2 => Some(Self::Ground),
            3 => Some(Self::Direct),
            _ => None,
        }
    }

    /// The elevation (degrees) of the ball's `ANGLE` flight, `None` for the weapon's own.
    pub fn elevation(self) -> Option<i32> {
        match self {
            Self::Canister => None,
            Self::Ground => Some(GROUND_ELEVATION),
            Self::Direct => Some(DIRECT_ELEVATION),
        }
    }
}

/// Ground fire: the nearest first graze (world px, 20 m at 30 px/m; on the game's compressed
/// ranges about 120 m of a real 6–12 pdr). Closer the ball would bury itself instead of
/// skipping, so the gun fires canister there.
pub const GROUND_MIN: i32 = 600;
/// Ground fire: elevation of the ball's flight (degrees; the weapon's own `NUCL2P` is 10°).
pub const GROUND_ELEVATION: i32 = 2;
/// Direct fire: elevation of the ball's flight (degrees).
pub const DIRECT_ELEVATION: i32 = 5;
/// Direct fire: the share (percent) of `RAZBROS` dispersion left.
pub const DIRECT_SCATTER: i32 = 50;
/// Direct fire: a ball strikes the building it is laid on within this many px of its
/// position (or its `ADDSHOTRADIUS` if larger)...
pub const DIRECT_BUILDING_REACH: i32 = 64;
/// ...and below this height (px, about 6 m at 30 px/m) over its ground.
pub const DIRECT_BUILDING_TOP: i32 = 180;
/// The lane a ground-fire ball skips along behind its first graze (world px, measured on the
/// blank field with the original weapons; the overlay draws it).
pub const GROUND_LANE: i32 = 1400;
/// How far a direct-fire ball bounces on past a miss (world px, measured likewise).
pub const DIRECT_LANE: i32 = 700;

/// The attack index of a type's ball (`WEAPONKIND IADRO`).
pub fn ball_index(t: &ObjectType) -> Option<usize> {
    t.attacks.iter().position(|a| a.kind == "IADRO")
}

/// The mode, attack index and minimum distance (px) of cannon `i`'s next shot at a target
/// `dst` px away: a point attack (`point`, with `times` shots left, 1 for `KARTECH` and the
/// automatic volley) or an object (`building`). `None` without the hit-locations rule and for
/// anything that is not a gun with canister and ball.
pub fn gun_shot(
    w: &World,
    i: usize,
    point: bool,
    times: u16,
    building: bool,
    dst: i32,
) -> Option<(GunMode, usize, i32)> {
    if !w.rules.hit_locations {
        return None;
    }
    let u = &w.units[i];
    let t = w.catalog.get(u.kind);
    let can = usize::from(t.sector_weapon?.att);
    let ball = ball_index(t)?;
    let ta = w.adv(u.owner, u.kind);
    let cr = ta.attacks.get(can)?.radius;
    ta.attacks.get(ball)?;
    let mode = if point {
        if times != 1 && dst >= GROUND_MIN { GunMode::Ground } else { GunMode::Canister }
    } else if building || dst > cr.1 {
        GunMode::Direct
    } else {
        GunMode::Canister
    };
    Some(match mode {
        GunMode::Canister => (mode, can, cr.0),
        GunMode::Ground => (mode, ball, GROUND_MIN),
        GunMode::Direct => (mode, ball, 0),
    })
}
