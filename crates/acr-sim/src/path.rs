//! The original fine-level path search (`Motion.cpp` of the engine), on 16 px lock cells.
//!
//! It is not A*: `CreatePrePath` (0x534031) draws a Bresenham line to the target and, if the
//! line is blocked, sends two wall followers around the obstacle in lock-step (one keeps the
//! wall on its left, one on its right) until one of them crosses the line behind the
//! obstacle. The winner's trace is string-pulled (tangent point, then every second cell).
//! `CreateFullPath` (0x533f1a) refines the first leg up to four times; `CreatePath`
//! (0x538d85) caches the path and only re-plans when the target changes, the next waypoint
//! becomes blocked or the path runs out. All coordinates here are lock cells; a unit's cell
//! is the top-left cell of its `lx × lx` footprint (`lx` = 1 for land units).

use crate::map::LockMap;
use crate::rng::SyncRandom;
use crate::trig::norma;

/// Steps per wall follower before the search gives up (0x534b4a).
const MAX_TRACE: usize = 0xff8;

/// The eight neighbour steps, 0 = north, clockwise (tables 0x6454ac / 0x6454cc).
const DX: [i32; 8] = [0, 1, 1, 1, 0, -1, -1, -1];
const DY: [i32; 8] = [-1, -1, 0, 1, 1, 1, 0, -1];

/// Direction index of a unit step (0x645488); `(0, 0)` maps to 0.
fn dir_of(dx: i32, dy: i32) -> usize {
    match (dx.signum(), dy.signum()) {
        (0, -1) => 0,
        (1, -1) => 1,
        (1, 0) => 2,
        (1, 1) => 3,
        (0, 1) => 4,
        (-1, 1) => 5,
        (-1, 0) => 6,
        (-1, -1) => 7,
        _ => 0,
    }
}

/// Path state of a unit (OneObject `PathX/PathY`, `NIPoints`, `PathDest`, `PathDelay`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct PathState {
    /// Waypoints, reversed: the last element is the next waypoint.
    pub points: Vec<(i32, i32)>,
    /// Whether a path array exists (`PathX != NULL`), even if all points were consumed.
    pub allocated: bool,
    /// Target cell the path was built for.
    pub target: Option<(i32, i32)>,
    /// Search cooldown in ticks (OB+0x8e).
    pub delay: u8,
}

impl PathState {
    /// `ClearPath` (0x523383).
    pub fn clear(&mut self) {
        self.points.clear();
        self.allocated = false;
        self.target = None;
    }
}

/// The searching unit: footprint cell and size.
#[derive(Debug, Clone, Copy)]
pub struct Mover {
    pub x: i32,
    pub y: i32,
    pub lx: i32,
    /// Takes the water pre-path (`CreateWaterPrePath` 0x535b04): ships (lock type 1, OB+0x54)
    /// and `MOTIONSTYLE COMPLEXROTATE` units (NM+0x28e1 == 3), see 0x534056.
    pub water: bool,
}

impl Mover {
    /// A land-path mover (`water` false).
    pub fn land(x: i32, y: i32, lx: i32) -> Self {
        Mover { x, y, lx, water: false }
    }

    /// The mover of unit `u` of type `t`.
    pub fn of(u: &crate::world::Unit, t: &crate::catalog::ObjectType) -> Self {
        let water =
            t.medium == crate::catalog::Medium::Water || t.motion_style == crate::catalog::MotionStyle::ComplexRotate;
        Mover { x: u.cell.0, y: u.cell.1, lx: u.lx, water }
    }
}

/// `GetNeighbourMask` (0x533854) for the box `(x, y, l, l)`, `l > 2`: whether a step in each
/// of the 8 directions enters a blocked cell. Entries 8..16 repeat 0..8.
fn neighbour_mask(m: &LockMap, x: i32, y: i32, l: i32) -> [bool; 16] {
    let mut r = [false; 16];
    r[0] = m.check_hline(x, y - 1, l);
    r[4] = m.check_hline(x, y + l, l);
    r[2] = m.check_vline(x + l, y, l);
    r[6] = m.check_vline(x - 1, y, l);
    r[1] = m.check_hline(x + 1, y - 1, l) || m.check_vline(x + l, y, l - 1);
    r[3] = m.check_hline(x + 1, y + l, l) || m.check_vline(x + l, y + 1, l - 1);
    r[5] = m.check_hline(x - 1, y + l, l) || m.check_vline(x - 1, y + 1, l - 1);
    r[7] = m.check_hline(x - 1, y - 1, l) || m.check_vline(x - 1, y, l - 1);
    for i in 0..8 {
        r[i + 8] = r[i];
    }
    r
}

/// Pseudo-angle with 2048 steps per turn (0x4ef9c8), y down.
fn pseudo_angle(dx: i32, dy: i32) -> i32 {
    if dx.abs() > dy.abs() {
        let a = (dy << 8) / dx.abs();
        if dx < 0 { 1024 - a } else { a }
    } else if dy == 0 {
        0
    } else {
        let a = 512 - (dx << 8) / dy.abs();
        if dy < 0 { 2048 - a } else { a }
    }
}

fn normalise_angle(mut r: i32) -> i32 {
    while r < 0 {
        r += 2048;
    }
    if r > 1792 {
        r -= 2048;
    }
    r
}

/// `FindBestPosition` (0x533bc6): if the target box (with a one-cell margin) is blocked,
/// searches square rings around it up to `radius` and takes the free cell closest (Norma) to
/// the unit. Returns false if none is found.
pub fn find_best_position(m: &LockMap, u: Mover, target: &mut (i32, i32), radius: i32) -> bool {
    let l = u.lx + 2;
    if !m.check_bar(target.0 - 1, target.1 - 1, l, l) {
        return true;
    }
    let (mut x0, mut y0, mut side) = (target.0 - 1, target.1 - 1, 2);
    for _ in 0..radius {
        let mut best = 100_000;
        let mut found = None;
        let mut cand = |cx: i32, cy: i32| {
            if !m.check_bar(cx - 1, cy - 1, l, l) {
                let d = norma(cx - u.x, cy - u.y);
                if d < best {
                    best = d;
                    found = Some((cx, cy));
                }
            }
        };
        for i in 0..=side {
            cand(x0 + i, y0);
        }
        for i in 0..=side {
            cand(x0 + i, y0 + side);
        }
        for i in 0..side - 1 {
            cand(x0, y0 + 1 + i);
        }
        for i in 0..side - 1 {
            cand(x0 + side, y0 + 1 + i);
        }
        if let Some(c) = found {
            *target = c;
            return true;
        }
        side += 2;
        x0 -= 1;
        y0 -= 1;
    }
    false
}

/// Result of [`create_pre_path`]: whether points were added, and whether a search ran
/// (`g_searched`, 0x2778864).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrePath {
    pub found: bool,
    pub searched: bool,
}

/// `CreatePrePath` (0x534031): appends a detour around the first obstacle on the straight
/// line to `(x1, y1)` to the unit's path (new points become the next waypoints).
///
/// A [`Mover::water`] unit takes `CreateWaterPrePath` (0x535b04) instead, the same search
/// with three changes: a follower's cell counts as visible when the neighbour mask of its
/// previous cell is open towards the target, and the end of the detour is the start of the
/// winner's last visible run (or its crossing cell); the trace keeps every fourth cell
/// instead of every second; a failed search waits 64..79 ticks instead of 200..215. It never
/// sets `g_searched` (cleared by `CreatePrePath` before it dispatches), so `searched` is
/// always false.
pub fn create_pre_path(m: &LockMap, u: Mover, st: &mut PathState, rnd: &mut SyncRandom, x1: i32, y1: i32) -> PrePath {
    let water = u.water;
    let no = |searched: bool| PrePath { found: false, searched: searched && !water };
    if st.delay != 0 {
        return no(false);
    }
    let (x, y, lx) = (u.x, u.y, u.lx);
    if (x - x1).abs() < 2 && (y - y1).abs() < 2 {
        return no(true);
    }
    let start_locked = m.check_bar(x, y, lx, lx);
    if start_locked && st.allocated {
        return no(true);
    }
    let (sdx, sdy) = (x1 - x, y1 - y);
    let (sx, sy) = (if sdx > 0 { 1 } else { -1 }, if sdy > 0 { 1 } else { -1 });
    let (adx, ady) = (sdx.abs(), sdy.abs());
    let ang0 = normalise_angle(pseudo_angle(sdx, sdy));

    // Bresenham line, error term starting at 0; points 0..=n, the last is the target.
    let mut line = vec![(x, y)];
    let (mut px, mut py, mut e) = (x, y, 0);
    if ady < adx {
        for _ in 0..adx {
            px += sx;
            e += ady;
            if e >= adx {
                e -= adx;
                py += sy;
            }
            line.push((px, py));
        }
    } else {
        for _ in 0..ady {
            py += sy;
            e += adx;
            if e >= ady {
                e -= ady;
                px += sx;
            }
            line.push((px, py));
        }
    }
    let n = adx.max(ady) as usize;

    if start_locked {
        // Escape: the first free point on the line becomes the only waypoint.
        for &(lx0, ly0) in &line[..n] {
            if !m.check_bar(lx0, ly0, lx, lx) {
                st.points = vec![(lx0, ly0)];
                st.allocated = true;
                return PrePath { found: true, searched: !water };
            }
        }
        return no(true);
    }
    if (1..n).all(|i| !m.check_bar(line[i].0, line[i].1, lx, lx)) {
        return no(true); // the straight line is free (the target cell itself is not tested)
    }
    let l2 = lx + 2;
    let margin_blocked = |p: (i32, i32)| m.check_bar(p.0 - 1, p.1 - 1, l2, l2);
    let mut i = 1;
    while i < n && !margin_blocked(line[i]) {
        i += 1;
    }
    let i_lock = i;
    if line[i] == (x1, y1) {
        return no(true);
    }
    i = i.saturating_sub(1);
    let mut j = i + 1;
    while margin_blocked(line[j]) && j < n {
        j += 1;
    }
    let exit = line[j - 1];
    let past = |p: (i32, i32)| {
        if adx > ady {
            if sx > 0 { p.0 >= exit.0 } else { p.0 <= exit.0 }
        } else if sy > 0 {
            p.1 >= exit.1
        } else {
            p.1 <= exit.1
        }
    };
    let visible = |p: (i32, i32), dir: usize| {
        let t = ((x1 - p.0).clamp(-1, 1), (y1 - p.1).clamp(-1, 1));
        let diff = (dir as i32 - dir_of(t.0, t.1) as i32) & 7;
        matches!(diff, 0 | 1 | 7) && !m.check_bar(p.0 + 2 * t.0 - 1, p.1 + 2 * t.1 - 1, l2, l2)
    };

    let mut r = line[i];
    let mut l = r;
    let (mut base_r, mut base_l) = (i_lock, i_lock);
    let (mut max_ar, mut max_al) = (0, 0);
    let (mut l_alive, mut r_alive, mut choose_r) = (true, true, true);
    let d0 = dir_of(line[i + 1].0 - line[i].0, line[i + 1].1 - line[i].1) as i32;
    let mask = neighbour_mask(m, r.0 - 1, r.1 - 1, l2);
    let mut d = (d0 + 1) & 7;
    let mut c = 0;
    while mask[d as usize] && c < 7 {
        d += 1;
        c += 1;
    }
    if !mask[((d & 7) + 6) as usize] {
        d += 1;
    }
    let mut dir_r = (d & 7) as usize;
    let mut d = ((d0 + 7) & 7) + 8;
    let mut c = 0;
    while mask[d as usize] && c < 7 {
        d -= 1;
        c += 1;
    }
    if !mask[((d & 7) + 2) as usize] {
        d -= 1;
    }
    let mut dir_l = (d & 7) as usize;

    let mut trace_r = vec![(0, 0); MAX_TRACE + 2];
    let mut trace_l = vec![(0, 0); MAX_TRACE + 2];
    let (mut vis_r, mut vis_l) = (Vec::new(), Vec::new());
    let (mut cum_r, mut cum_l, mut prev_r, mut prev_l) = (0i32, 0i32, 0i32, 0i32);
    // Water: whether the follower's last cell was visible, whether a visible run is on, and
    // the start of its last run (-0x9/-0x80c1/-0x8078 right, -0x15/-0x80c2/-0x2038 left).
    let (mut seen_r, mut run_r, mut end_r) = (false, false, 0usize);
    let (mut seen_l, mut run_l, mut end_l) = (false, false, 0usize);
    // The mask of the follower's previous cell, open towards the target (0x536845).
    let open_to_target =
        |mask: &[bool; 16], p: (i32, i32)| !mask[dir_of((x1 - p.0).clamp(-1, 1), (y1 - p.1).clamp(-1, 1))];
    let track = |open: bool, seen: &mut bool, run: &mut bool, end: &mut usize, i: usize| {
        *seen = open;
        if !open {
            *run = false;
        } else if !*run {
            *end = i;
            *run = true;
        }
    };
    while i < MAX_TRACE && l_alive && r_alive {
        // Right follower: obstacle on its left, goes round clockwise.
        let mask = neighbour_mask(m, r.0 - 1, r.1 - 1, l2);
        let mut d = (dir_r + 6) & 7;
        let mut c = 0;
        while c < 6 && mask[d] {
            d += 1;
            c += 1;
        }
        dir_r = d & 7;
        i += 1;
        let (dx, dy) = (DX[dir_r], DY[dir_r]);
        cum_r += sdx * dy - sdy * dx;
        r = (r.0 + dx, r.1 + dy);
        trace_r[i] = r;
        let a = normalise_angle(pseudo_angle(r.0 - x, r.1 - y) - ang0);
        if a > max_ar {
            max_ar = a;
            base_r = i;
        }
        if water {
            track(open_to_target(&mask, r), &mut seen_r, &mut run_r, &mut end_r, i);
        } else if visible(r, dir_r) {
            vis_r.push(i);
        }
        // Left follower: the mirror image.
        let mask = neighbour_mask(m, l.0 - 1, l.1 - 1, l2);
        let mut d = ((dir_l + 2) & 7) + 8;
        let mut c = 0;
        while c < 6 && mask[d] {
            d -= 1;
            c += 1;
        }
        dir_l = d & 7;
        let (dx, dy) = (DX[dir_l], DY[dir_l]);
        cum_l += sdy * dx - sdx * dy;
        l = (l.0 + dx, l.1 + dy);
        trace_l[i] = l;
        let a = normalise_angle(ang0 - pseudo_angle(l.0 - x, l.1 - y));
        if a > max_al {
            max_al = a;
            base_l = i;
        }
        if water {
            track(open_to_target(&mask, l), &mut seen_l, &mut run_l, &mut end_l, i);
        } else if visible(l, dir_l) {
            vis_l.push(i);
        }
        if cum_r <= 0 && prev_r >= 0 && past(r) {
            if !seen_r {
                end_r = i;
            }
            r_alive = false;
            choose_r = true;
        }
        prev_r = cum_r;
        if cum_l <= 0 && prev_l >= 0 && past(l) {
            if !seen_l {
                end_l = i;
            }
            l_alive = false;
            choose_r = false;
        }
        prev_l = cum_l;
    }
    if i >= MAX_TRACE - 1 {
        st.delay = ((if water { 64 } else { 200 }) + (rnd.rando() & 15)) as u8;
        return no(true);
    }
    let (trace, base, vis) = if choose_r { (&trace_r, base_r, &vis_r) } else { (&trace_l, base_l, &vis_l) };
    let mut end = if water {
        if choose_r { end_r } else { end_l }
    } else {
        vis.iter().copied().find(|&v| v >= base).unwrap_or(i.saturating_sub(1))
    };
    if end < i {
        end += 1;
    }
    end = end.max(base);
    let cnt = end - base + 1;
    // Every second traced cell on land, every fourth on water (0x5354c9 / 0x536f54).
    let sh = if water { 2 } else { 1 };
    let mut added: Vec<(i32, i32)> = Vec::new();
    if cnt <= 2 {
        added.push(trace[base + cnt - 1]);
        if cnt == 2 {
            added.push(trace[base]);
        }
    } else {
        let h = ((cnt - 2) >> sh) + 1;
        added.resize(h + 1, (0, 0));
        for k in 0..h {
            added[h - k] = trace[base + (k << sh)];
        }
        added[0] = trace[end];
    }
    st.points.extend(added);
    st.allocated = true;
    PrePath { found: true, searched: !water }
}

/// `CreateFullPath` (0x533f1a): builds a new path to `(x, y)`, refining the first leg up to
/// four times; consumes one synced random number per refinement.
pub fn create_full_path(m: &LockMap, u: Mover, st: &mut PathState, rnd: &mut SyncRandom, x: i32, y: i32) {
    st.clear();
    let mut t = (x, y);
    if !find_best_position(m, u, &mut t, 40) {
        return;
    }
    let mut n = 0;
    let mut searched;
    loop {
        let r = create_pre_path(m, u, st, rnd, t.0, t.1);
        searched = r.searched;
        if r.found {
            t = *st.points.last().expect("pre-path adds points");
        }
        n += 1;
        rnd.rando();
        if !(r.found && n < 4) {
            break;
        }
    }
    if searched {
        // Also overwrites the failure cooldown set by CreatePrePath (an original bug).
        st.delay = (rnd.rando() & 7) as u8;
    }
}

/// Centre of a footprint cell in 1/16 px (`cell·256 + lx·128`).
pub fn cell_center(c: i32, lx: i32) -> i32 {
    (c << 8) + (lx << 7)
}

/// `DistTo` (0x52e230): Chebyshev distance in cells.
pub fn dist_to(u: Mover, x: i32, y: i32) -> i32 {
    (x - u.x).abs().max((y - u.y).abs())
}

/// `CreatePath` (0x538d85): steers towards the target cell; returns the new steering point
/// (`DestX/DestY`) in 1/16 px, or `None` to keep the current one.
pub fn create_path(
    m: &LockMap,
    u: Mover,
    st: &mut PathState,
    rnd: &mut SyncRandom,
    x1: i32,
    y1: i32,
    stop_distance: i32,
) -> Option<(i32, i32)> {
    let center = |c: (i32, i32)| (cell_center(c.0, u.lx), cell_center(c.1, u.lx));
    let thr = (stop_distance >> 8) + 1;
    let mut dest = None;
    if st.target == Some((x1, y1)) {
        if !st.allocated {
            dest = Some(center((x1, y1)));
        } else if !st.points.is_empty() {
            let next = *st.points.last().unwrap();
            if dist_to(u, next.0, next.1) < thr {
                st.points.pop();
            }
            if let Some(&next) = st.points.last() {
                if m.check_bar(next.0, next.1, u.lx, u.lx) {
                    create_full_path(m, u, st, rnd, x1, y1);
                } else {
                    dest = Some(center(next));
                }
            } else {
                create_full_path(m, u, st, rnd, x1, y1);
                if st.points.len() == 1 {
                    let p = st.points[0];
                    if norma(p.0 - u.x, p.1 - u.y) < 3 {
                        st.clear();
                    }
                }
            }
        } else {
            create_full_path(m, u, st, rnd, x1, y1);
        }
    } else {
        create_full_path(m, u, st, rnd, x1, y1);
    }
    st.target = Some((x1, y1));
    if dest.is_none() {
        dest = Some(match st.points.last() {
            Some(&p) => center(p),
            None => center((x1, y1)),
        });
    }
    dest
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_map() -> LockMap {
        LockMap::new(64, 64)
    }

    #[test]
    fn straight_line_needs_no_path() {
        let m = open_map();
        let mut st = PathState::default();
        let mut rnd = SyncRandom::new(0);
        let r = create_pre_path(&m, Mover::land(5, 5, 1), &mut st, &mut rnd, 40, 20);
        assert!(!r.found && r.searched);
        assert!(st.points.is_empty());
    }

    #[test]
    fn detours_around_a_wall() {
        let mut m = open_map();
        m.set_rect(20, 5, 2, 30); // vertical wall x = 20..22, y = 5..35
        let u = Mover::land(10, 20, 1);
        let mut st = PathState::default();
        let mut rnd = SyncRandom::new(0);
        create_full_path(&m, u, &mut st, &mut rnd, 30, 20);
        assert!(!st.points.is_empty(), "a detour was found");
        // Every waypoint is free and the route ends past the wall's end.
        for &(x, y) in &st.points {
            assert!(!m.check_bar(x, y, 1, 1), "waypoint ({x},{y}) blocked");
        }
        let next = *st.points.last().unwrap();
        assert!(next.1 <= 4 || next.1 >= 35, "goes around the wall end: {next:?}");
    }

    /// `CreateWaterPrePath` (0x535b04): ships keep every fourth traced cell, never report a
    /// search (so `CreateFullPath` draws no closing `rando`) and wait 64..79 ticks after a
    /// failed one.
    #[test]
    fn water_pre_path_is_sparser_and_never_searched() {
        let mut m = open_map();
        m.set_rect(20, 5, 24, 40);
        let land = Mover::land(10, 30, 1);
        let water = Mover { water: true, ..land };
        let (mut a, mut b) = (PathState::default(), PathState::default());
        let (mut ra, mut rb) = (SyncRandom::new(3), SyncRandom::new(3));
        let pa = create_pre_path(&m, land, &mut a, &mut ra, 54, 30);
        let pb = create_pre_path(&m, water, &mut b, &mut rb, 54, 30);
        assert!(pa.found && pa.searched);
        assert!(pb.found && !pb.searched);
        assert!(!b.points.is_empty() && b.points.len() < a.points.len(), "{:?} vs {:?}", b.points, a.points);
        for &(x, y) in &b.points {
            assert!(!m.check_bar(x, y, 1, 1), "waypoint ({x},{y}) blocked");
        }
        // Round the block's lower side, four cells apart; the first point is where the target
        // comes into view past the corner.
        assert_eq!(b.points[1..], [(42, 46), (38, 46), (34, 46), (30, 46), (26, 46), (22, 46), (18, 45)]);
        assert_eq!(b.points[0], (45, 44));

        // CreateFullPath: the land search draws one more `rando` (the closing delay).
        let (mut a, mut b) = (PathState::default(), PathState::default());
        let (mut ra, mut rb) = (SyncRandom::new(3), SyncRandom::new(3));
        create_full_path(&m, land, &mut a, &mut ra, 54, 30);
        create_full_path(&m, water, &mut b, &mut rb, 54, 30);
        assert_eq!(b.delay, 0);
        rb.rando();
        assert_eq!(ra.rando(), rb.rando());
    }

    #[test]
    fn water_pre_path_failure_waits_less() {
        // The unit is shut in a ring: the followers circle inside it until the trace ends.
        let mut m = open_map();
        m.set_rect(10, 10, 20, 1);
        m.set_rect(10, 29, 20, 1);
        m.set_rect(10, 10, 1, 20);
        m.set_rect(29, 10, 1, 20);
        for (water, lo) in [(false, 200), (true, 64)] {
            let mut st = PathState::default();
            let mut rnd = SyncRandom::new(1);
            let r = create_pre_path(&m, Mover { water, ..Mover::land(20, 20, 1) }, &mut st, &mut rnd, 50, 20);
            assert!(!r.found);
            assert!((lo..lo + 16).contains(&i32::from(st.delay)), "water {water}: delay {}", st.delay);
        }
    }

    #[test]
    fn best_position_moves_target_out_of_obstacle() {
        let mut m = open_map();
        m.set_rect(30, 30, 6, 6);
        let mut t = (32, 32);
        assert!(find_best_position(&m, Mover::land(10, 32, 1), &mut t, 10));
        assert!(!m.check_bar(t.0 - 1, t.1 - 1, 3, 3));
        assert!(t.0 < 30, "closest to the unit: {t:?}");
    }

    #[test]
    fn pseudo_angle_quadrants() {
        assert_eq!(pseudo_angle(10, 0), 0);
        assert_eq!(pseudo_angle(0, 10), 512);
        assert_eq!(pseudo_angle(-10, 0), 1024);
        assert_eq!(pseudo_angle(0, -10), 1536);
    }
}
