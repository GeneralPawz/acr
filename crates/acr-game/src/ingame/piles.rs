//! Corpse piles (#221, docs/rendering-3d.md): 3D corpses that overlap lie on top of each
//! other instead of through each other. Render only: each body is a rectangle on the ground
//! along its facing with a rounded top, bodies are stacked in the order they died, and each
//! one is lifted onto the bodies beneath it and tilted to rest on them. As the bodies below
//! decay (deflate, sink) they get thinner and the pile settles.

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
    /// Stacking order: smaller lies further down (died earlier).
    pub order: (u32, u32),
}

/// Where a body rests: lifted over its own ground point and tilted (height change per pixel
/// along the body, towards the head, and across it, towards its right = `along` turned
/// clockwise seen from above with y south).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Rest {
    pub lift: f32,
    pub pitch: f32,
    pub roll: f32,
}

/// The highest a body is lifted (world pixels, 1.1 m: about four bodies).
pub const MAX_LIFT: f32 = 33.0;
/// The steepest tilt (height per pixel, about 27°).
const MAX_SLOPE: f32 = 0.5;
/// Grid cell (world pixels).
const CELL: f32 = 64.0;

/// Sample points under a body (fractions of the half length and half width).
const SAMPLES: [(f32, f32); 8] =
    [(-0.75, -0.5), (-0.75, 0.5), (-0.25, -0.5), (-0.25, 0.5), (0.25, -0.5), (0.25, 0.5), (0.75, -0.5), (0.75, 0.5)];

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
    Some(f.centre[2] + r.lift + r.pitch * a + r.roll * b + f.thickness * (1.0 - q * q).max(0.0).sqrt())
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
        let f = &bodies[i];
        let c = across(f.along);
        // Highest support under each half (back/front, left/right) over the own ground.
        let (mut back, mut front, mut left, mut right) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        let mut any = false;
        for (sa, sb) in SAMPLES {
            let (a, b) = (sa * f.half_length, sb * f.half_width);
            let (x, y) = (f.centre[0] + a * f.along[0] + b * c[0], f.centre[1] + a * f.along[1] + b * c[1]);
            let h = grid
                .at(x, y)
                .iter()
                .map(|&j| j as usize)
                .filter(|&j| rank[j] < rank[i])
                .filter_map(|j| top(&bodies[j], &rests[j], x, y))
                .fold(f32::MIN, f32::max)
                - f.centre[2];
            if h <= 0.0 {
                continue;
            }
            any = true;
            if sa < 0.0 {
                back = back.max(h)
            } else {
                front = front.max(h)
            }
            if sb < 0.0 { left = left.max(h) } else { right = right.max(h) }
        }
        if any {
            let lift = ((back + front) / 2.0).max((left + right) / 2.0).min(MAX_LIFT);
            rests[i] = Rest {
                lift,
                pitch: ((front - back) / f.half_length).clamp(-MAX_SLOPE, MAX_SLOPE),
                // Half the slope between the side samples (half a width apart): bodies give.
                roll: ((right - left) / (2.0 * f.half_width)).clamp(-MAX_SLOPE, MAX_SLOPE),
            };
        }
    }
    rests
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(x: f32, y: f32, along: [f32; 2], order: u32) -> Footprint {
        Footprint { centre: [x, y, 0.0], along, half_length: 25.0, half_width: 7.5, thickness: 7.5, order: (order, 0) }
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
