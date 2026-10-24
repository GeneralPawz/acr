//! Corpse piles (#221, docs/rendering-3d.md): 3D corpses that overlap lie on top of each
//! other instead of through each other. Render only: each body is a rectangle on the ground
//! along its facing with a rounded top, bodies are stacked in the order they died, and each
//! one is lifted onto the bodies beneath it and the ground, tilted to rest on them and bent
//! over them (#229). As the bodies below decay (deflate, sink) they get thinner and the pile
//! settles. [`Piles`] keeps the rests and stacks again only when the corpses change (#230).

/// A lying body as seen from above, world pixels (`docs/rendering-3d.md`, camera math).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Footprint {
    /// Centre of the body on the ground (x, y) and the ground's height there (z).
    pub centre: [f32; 3],
    /// Unit vector along the body, feet to head (the facing).
    pub along: [f32; 2],
    pub half_length: f32,
    pub half_width: f32,
    /// Height of the body's top over its underside, already thinned by its decay.
    pub thickness: f32,
    /// Stacking order: smaller lies further down (died earlier). Also the body's identity
    /// in [`Piles`]: the tick of death and the unit id.
    pub order: (u32, u32),
    /// The terrain at the [`SAMPLES`] over the ground point's height (`centre[2]`).
    pub ground: [f32; 8],
}

/// Where a body rests: lifted over its own ground point and tilted (height change per pixel
/// along the body, towards the head, and across it, towards its right = `along` turned
/// clockwise seen from above with y south).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Rest {
    pub lift: f32,
    pub pitch: f32,
    pub roll: f32,
    /// The bend at the four [`STATIONS`] (world pixels, along the tilted body's up): where
    /// the supports there lie off the tilted plane, the body follows them.
    pub drape: [f32; 4],
}

/// The highest a body is lifted (world pixels, 1.1 m: about four bodies).
pub const MAX_LIFT: f32 = 33.0;
/// The steepest tilt (height per pixel, about 27°).
const MAX_SLOPE: f32 = 0.5;
/// Grid cell (world pixels).
const CELL: f32 = 64.0;
/// The most a body bends at a station off its tilted plane (world pixels, 0.4 m).
pub const MAX_DRAPE: f32 = 12.0;
/// How far a body's thickness or ground point may drift (decay, sink) before the pile is
/// stacked again (world pixels).
const DRIFT: f32 = 0.5;
/// The most new bodies [`Piles`] places alone; more are stacked with all.
const ALONE: usize = 16;

/// Stations along a body (fractions of the half length): two samples each, left and right.
pub const STATIONS: [f32; 4] = [-0.75, -0.25, 0.25, 0.75];
/// Sample points under a body (fractions of the half length and half width).
pub const SAMPLES: [(f32, f32); 8] =
    [(-0.75, -0.5), (-0.75, 0.5), (-0.25, -0.5), (-0.25, 0.5), (0.25, -0.5), (0.25, 0.5), (0.75, -0.5), (0.75, 0.5)];

/// The bend at `a` (world pixels along the body from its centre): linear between the
/// stations, held beyond the ends.
fn drape_at(f: &Footprint, drape: &[f32; 4], a: f32) -> f32 {
    let s = (a / (0.5 * f.half_length) + 1.5).clamp(0.0, 3.0);
    let k = (s as usize).min(2);
    drape[k] + (drape[k + 1] - drape[k]) * (s - k as f32)
}

fn across(along: [f32; 2]) -> [f32; 2] {
    // Turned clockwise on screen (y south): east → south.
    [-along[1], along[0]]
}

/// The bodies by grid cell over the bounding box of all of them, each body in every cell its
/// rectangle touches (cells in one flat array, compressed rows).
struct Grid {
    origin: [f32; 2],
    cell: f32,
    size: [usize; 2],
    starts: Vec<u32>,
    bodies: Vec<u32>,
}

impl Grid {
    fn new(bodies: &[Footprint]) -> Self {
        let reach = |f: &Footprint| f.half_length + f.half_width;
        let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
        for f in bodies {
            for k in 0..2 {
                lo[k] = lo[k].min(f.centre[k] - reach(f));
                hi[k] = hi[k].max(f.centre[k] + reach(f));
            }
        }
        // Coarser cells when the bodies are spread wide, so the grid stays small (a few cells
        // per body: clearing a map-sized grid each frame would cost more than the stacking).
        let mut cell = CELL;
        let most = (8 * bodies.len() + 1024) as f32;
        while ((hi[0] - lo[0]) / cell + 1.0) * ((hi[1] - lo[1]) / cell + 1.0) > most {
            cell *= 2.0;
        }
        let size = [((hi[0] - lo[0]) / cell) as usize + 1, ((hi[1] - lo[1]) / cell) as usize + 1];
        let mut grid = Self { origin: lo, cell, size, starts: vec![0; size[0] * size[1] + 1], bodies: Vec::new() };
        let range = |g: &Self, f: &Footprint| {
            let r = reach(f);
            (g.index_xy(f.centre[0] - r, f.centre[1] - r), g.index_xy(f.centre[0] + r, f.centre[1] + r))
        };
        for f in bodies {
            let ((x0, y0), (x1, y1)) = range(&grid, f);
            for y in y0..=y1 {
                for x in x0..=x1 {
                    grid.starts[y * size[0] + x + 1] += 1;
                }
            }
        }
        for i in 1..grid.starts.len() {
            grid.starts[i] += grid.starts[i - 1];
        }
        grid.bodies = vec![0; grid.starts[grid.starts.len() - 1] as usize];
        let mut fill = grid.starts.clone();
        for (i, f) in bodies.iter().enumerate() {
            let ((x0, y0), (x1, y1)) = range(&grid, f);
            for y in y0..=y1 {
                for x in x0..=x1 {
                    let c = &mut fill[y * size[0] + x];
                    grid.bodies[*c as usize] = i as u32;
                    *c += 1;
                }
            }
        }
        grid
    }

    fn index_xy(&self, x: f32, y: f32) -> (usize, usize) {
        let c = |v: f32, o: f32, n: usize| (((v - o) / self.cell).max(0.0) as usize).min(n - 1);
        (c(x, self.origin[0], self.size[0]), c(y, self.origin[1], self.size[1]))
    }

    /// The bodies whose rectangle may cover `(x, y)`.
    fn at(&self, x: f32, y: f32) -> &[u32] {
        let (cx, cy) = self.index_xy(x, y);
        let i = cy * self.size[0] + cx;
        &self.bodies[self.starts[i] as usize..self.starts[i + 1] as usize]
    }
}

/// The top of a placed body at `(x, y)`, if it covers that point.
fn top(f: &Footprint, r: &Rest, x: f32, y: f32) -> Option<f32> {
    let (dx, dy) = (x - f.centre[0], y - f.centre[1]);
    let c = across(f.along);
    let a = dx * f.along[0] + dy * f.along[1];
    let b = dx * c[0] + dy * c[1];
    if a.abs() > f.half_length || b.abs() > f.half_width {
        return None;
    }
    let q = b / f.half_width;
    Some(
        f.centre[2]
            + r.lift
            + r.pitch * a
            + r.roll * b
            + drape_at(f, &r.drape, a)
            + f.thickness * (1.0 - q * q).max(0.0).sqrt(),
    )
}

/// Where body `f` rests, given the highest top of the bodies under it at a point
/// (`f32::MIN` where none): on them and the ground, tilted to the highest support of each
/// half and bent to the supports off that plane.
fn place(f: &Footprint, support: impl Fn(f32, f32) -> f32) -> Rest {
    let c = across(f.along);
    let mut h = [0.0f32; 8];
    for (k, (sa, sb)) in SAMPLES.into_iter().enumerate() {
        let (a, b) = (sa * f.half_length, sb * f.half_width);
        let (x, y) = (f.centre[0] + a * f.along[0] + b * c[0], f.centre[1] + a * f.along[1] + b * c[1]);
        h[k] = (support(x, y) - f.centre[2]).max(f.ground[k]);
    }
    if h.iter().all(|&v| v == 0.0) {
        return Rest::default();
    }
    // Highest support under each half (back/front, left/right) over the own ground point.
    let half = |pick: fn(f32, f32) -> bool| {
        SAMPLES.iter().zip(h).filter(|((sa, sb), _)| pick(*sa, *sb)).map(|(_, v)| v).fold(f32::MIN, f32::max)
    };
    let (back, front) = (half(|sa, _| sa < 0.0), half(|sa, _| sa > 0.0));
    let (left, right) = (half(|_, sb| sb < 0.0), half(|_, sb| sb > 0.0));
    let pitch = ((front - back) / f.half_length).clamp(-MAX_SLOPE, MAX_SLOPE);
    // Half the slope between the side samples (half a width apart): bodies give.
    let roll = ((right - left) / (2.0 * f.half_width)).clamp(-MAX_SLOPE, MAX_SLOPE);
    // The supports off the tilted plane through the ground point; the plane is lifted onto
    // the highest of them (never below the ground point) and the body bends down to the rest.
    let off: [f32; 8] = std::array::from_fn(|i| {
        let (sa, sb) = SAMPLES[i];
        h[i] - (pitch * sa * f.half_length + roll * sb * f.half_width)
    });
    let lift = off.iter().fold(0.0f32, |m, &v| m.max(v)).min(MAX_LIFT);
    let drape = [0, 1, 2, 3].map(|k| (off[2 * k].max(off[2 * k + 1]) - lift).clamp(-MAX_DRAPE, MAX_DRAPE));
    Rest { lift, pitch, roll, drape }
}

/// Stacks the bodies: each rests on those that died before it (its `order` smaller) and
/// overlap it. Returns one `Rest` per footprint, in the given order.
pub fn stack(bodies: &[Footprint]) -> Vec<Rest> {
    let mut rests = vec![Rest::default(); bodies.len()];
    if bodies.is_empty() {
        return rests;
    }
    let mut by_order: Vec<usize> = (0..bodies.len()).collect();
    by_order.sort_by_key(|&i| bodies[i].order);
    // Bodies placed so far: those of a lower rank.
    let mut rank = vec![0u32; bodies.len()];
    for (r, &i) in by_order.iter().enumerate() {
        rank[i] = r as u32;
    }
    let grid = Grid::new(bodies);
    for i in by_order {
        let support = |x: f32, y: f32| {
            grid.at(x, y)
                .iter()
                .map(|&j| j as usize)
                .filter(|&j| rank[j] < rank[i])
                .filter_map(|j| top(&bodies[j], &rests[j], x, y))
                .fold(f32::MIN, f32::max)
        };
        rests[i] = place(&bodies[i], support);
    }
    rests
}

/// The rests of the last frame (#230): corpses do not move and decay slowly, so the pile is
/// stacked again only when one goes, or a thickness or ground point has drifted more than
/// [`DRIFT`] since. A body that died after all the others only rests on them: it is placed
/// alone, against the bodies within its reach.
#[derive(Default)]
pub struct Piles {
    /// The bodies as last stacked (in the order given then) and their rests.
    bodies: Vec<Footprint>,
    rests: Vec<Rest>,
    /// Full stackings and bodies placed alone since the last [`Piles::take_counts`].
    full: u32,
    added: u32,
}

impl Piles {
    /// The rests of `bodies`, one per footprint in the given order.
    pub fn rests(&mut self, bodies: &[Footprint]) -> &[Rest] {
        let same = |a: &Footprint, b: &Footprint| {
            a.order == b.order
                && a.centre[..2] == b.centre[..2]
                && a.along == b.along
                && a.half_length == b.half_length
                && a.ground == b.ground
                && (a.centre[2] - b.centre[2]).abs() <= DRIFT
                && (a.thickness - b.thickness).abs() <= DRIFT
        };
        // The old bodies in the same order, with ones that died after all of them in between.
        let newest = self.bodies.iter().map(|f| f.order).max();
        let mut kept = 0;
        let mut fresh = Vec::new();
        let mut appended = true;
        for (i, f) in bodies.iter().enumerate() {
            if self.bodies.get(kept).is_some_and(|o| same(o, f)) {
                kept += 1;
            } else if newest.is_none_or(|n| f.order > n) {
                fresh.push(i);
            } else {
                appended = false;
                break;
            }
        }
        // Many new ones at once (the first frame, a volley): the grid is quicker.
        if !appended || kept != self.bodies.len() || fresh.len() > ALONE {
            self.rests = stack(bodies);
            self.bodies = bodies.to_vec();
            self.full += 1;
            return &self.rests;
        }
        if fresh.is_empty() {
            return &self.rests;
        }
        // The old footprints stay as stacked (their drift counts from then).
        let mut all: Vec<Footprint> = Vec::with_capacity(bodies.len());
        let mut rests: Vec<Rest> = Vec::with_capacity(bodies.len());
        let mut old = self.bodies.iter().zip(&self.rests);
        let mut next = fresh.iter().peekable();
        for (i, f) in bodies.iter().enumerate() {
            if next.peek() == Some(&&i) {
                next.next();
                all.push(*f);
                rests.push(Rest::default());
            } else if let Some((o, r)) = old.next() {
                all.push(*o);
                rests.push(*r);
            }
        }
        fresh.sort_by_key(|&i| bodies[i].order);
        for (n, &i) in fresh.iter().enumerate() {
            let f = &all[i];
            let reach = f.half_length + f.half_width;
            let under: Vec<usize> = (0..all.len())
                .filter(|&j| j != i && !fresh[n..].contains(&j))
                .filter(|&j| {
                    let g = &all[j];
                    let r = reach + g.half_length + g.half_width;
                    let (dx, dy) = (g.centre[0] - f.centre[0], g.centre[1] - f.centre[1]);
                    dx * dx + dy * dy < r * r
                })
                .collect();
            let support =
                |x: f32, y: f32| under.iter().filter_map(|&j| top(&all[j], &rests[j], x, y)).fold(f32::MIN, f32::max);
            rests[i] = place(f, support);
            self.added += 1;
        }
        self.bodies = all;
        self.rests = rests;
        &self.rests
    }

    /// Full stackings and bodies placed alone since the last call.
    pub fn take_counts(&mut self) -> (u32, u32) {
        (std::mem::take(&mut self.full), std::mem::take(&mut self.added))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(x: f32, y: f32, along: [f32; 2], order: u32) -> Footprint {
        Footprint {
            centre: [x, y, 0.0],
            along,
            half_length: 25.0,
            half_width: 7.5,
            thickness: 7.5,
            order: (order, 0),
            ground: [0.0; 8],
        }
    }

    #[test]
    fn apart_bodies_lie_on_the_ground() {
        let rests = stack(&[body(0.0, 0.0, [1.0, 0.0], 0), body(200.0, 0.0, [1.0, 0.0], 1)]);
        assert_eq!(rests, vec![Rest::default(); 2]);
    }

    #[test]
    fn the_later_body_lies_on_the_earlier_one() {
        // Given later first: the order, not the slice order, decides.
        let rests = stack(&[body(0.0, 2.0, [1.0, 0.0], 1), body(0.0, 0.0, [1.0, 0.0], 0)]);
        assert_eq!(rests[1], Rest::default());
        assert!(rests[0].lift > 5.0 && rests[0].lift <= 7.5, "{:?}", rests[0]);
        assert!(rests[0].pitch.abs() < 1e-3);
    }

    #[test]
    fn a_body_half_on_another_tilts_down_to_the_ground() {
        // The second body's head end lies on the first, its feet on the ground.
        let rests = stack(&[body(0.0, 0.0, [0.0, 1.0], 0), body(-20.0, 0.0, [1.0, 0.0], 1)]);
        let r = rests[1];
        assert!(r.lift > 0.0 && r.pitch > 0.0, "{r:?}");
        assert!(r.pitch <= MAX_SLOPE);
    }

    #[test]
    fn piles_are_capped_and_settle_with_decay() {
        let pile: Vec<Footprint> = (0..12).map(|i| body(0.0, 0.0, [1.0, 0.0], i)).collect();
        let rests = stack(&pile);
        for w in rests.windows(2) {
            assert!(w[1].lift >= w[0].lift);
        }
        assert_eq!(rests[11].lift, MAX_LIFT);
        // Decayed bodies below are thinner: the pile is lower.
        let settled: Vec<Footprint> =
            pile.iter().map(|f| Footprint { thickness: if f.order.0 < 11 { 2.0 } else { 7.5 }, ..*f }).collect();
        let low = stack(&settled);
        assert!(low[11].lift < rests[11].lift);
        let flat: Vec<Footprint> = pile.iter().map(|f| Footprint { thickness: 0.0, ..*f }).collect();
        assert!(stack(&flat).iter().all(|r| r.lift == 0.0));
    }

    #[test]
    fn a_body_across_another_bends_over_it() {
        // The second lies across the first's middle: lifted there, its ends bent down.
        let rests = stack(&[body(0.0, 0.0, [0.0, 1.0], 0), body(0.0, 0.0, [1.0, 0.0], 1)]);
        let r = rests[1];
        assert!(r.lift > 3.0, "{r:?}");
        assert!(r.drape[0] < -3.0 && r.drape[3] < -3.0, "{r:?}");
        assert!(r.drape[1].abs() < 1.0 && r.drape[2].abs() < 1.0, "{r:?}");
        // Its ends rest on the ground, not under it.
        assert!(r.lift + r.drape[0] >= -1e-3 && r.lift + r.drape[3] >= -1e-3, "{r:?}");
    }

    #[test]
    fn bodies_lie_along_the_slope() {
        // Ground rising 0.2 per pixel towards the head (+x).
        let mut f = body(0.0, 0.0, [1.0, 0.0], 0);
        for (k, (sa, _)) in SAMPLES.into_iter().enumerate() {
            f.ground[k] = 0.2 * sa * f.half_length;
        }
        let r = stack(&[f])[0];
        assert!((r.pitch - 0.2).abs() < 1e-3 && r.lift.abs() < 1e-3, "{r:?}");
        assert!(r.drape.iter().all(|d| d.abs() < 1e-3), "{r:?}");
    }

    #[test]
    fn the_cache_matches_a_full_stack() {
        let mut piles = Piles::default();
        let mut bodies: Vec<Footprint> = (0..6).map(|i| body(i as f32 * 9.0, 0.0, [1.0, 0.0], i)).collect();
        assert_eq!(piles.rests(&bodies), stack(&bodies));
        assert_eq!(piles.take_counts(), (0, 6));
        // Nothing changed: nothing stacked.
        piles.rests(&bodies);
        assert_eq!(piles.take_counts(), (0, 0));
        // Two die later (one listed first): placed alone, as a full stack would.
        bodies.insert(0, body(20.0, 3.0, [0.0, 1.0], 9));
        bodies.push(body(5.0, -2.0, [0.6, 0.8], 7));
        assert_eq!(piles.rests(&bodies), stack(&bodies));
        assert_eq!(piles.take_counts(), (0, 2));
        // A little decay waits; more stacks again; one gone stacks again.
        bodies[3].thickness -= 0.3;
        piles.rests(&bodies);
        assert_eq!(piles.take_counts(), (0, 0));
        bodies[3].thickness -= 0.3;
        assert_eq!(piles.rests(&bodies), stack(&bodies));
        assert_eq!(piles.take_counts(), (1, 0));
        bodies.remove(2);
        assert_eq!(piles.rests(&bodies), stack(&bodies));
        assert_eq!(piles.take_counts(), (1, 0));
    }

    #[test]
    fn hundreds_of_bodies_stay_fast() {
        // 800 bodies on a 40 × 20 m field, heavily overlapping.
        let bodies: Vec<Footprint> = (0..800u32)
            .map(|i| {
                let t = i as f32 * 0.618_034;
                let a = t * 7.0;
                body((t.fract() * 1200.0).floor(), ((i * 37 % 600) as f32).floor(), [a.cos(), a.sin()], i)
            })
            .collect();
        let start = std::time::Instant::now();
        let rests = stack(&bodies);
        assert!(rests.iter().any(|r| r.lift > 0.0));
        assert!(rests.iter().all(|r| r.lift <= MAX_LIFT));
        // Generous for debug builds on slow machines.
        assert!(start.elapsed().as_millis() < 200, "{:?}", start.elapsed());
    }
}
