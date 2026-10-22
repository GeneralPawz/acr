//! Blood trails of the bleeding (#254, render only): drops behind those who walk, a smear
//! behind those who crawl on lost legs (#262). Deterministic from unit id and tick.
//!
//! A crawler leaves one smear segment every [`EVERY`] ticks, from where he was to where he
//! is, so the segments of one unit join into a ribbon the width of a body. Slow segments are
//! wide and heavy, fast ones thin and broken; where he lay still the last segment gathers a
//! wider patch. Everything dries from red to brown-black and fades at the end of [`LIFE`].

use std::collections::{HashMap, VecDeque};

use acr_render::model;
use acr_render::model_renderer::{BloodPool, Smear};
use acr_render::view3d::PIXELS_PER_METRE;

/// Ticks a trail stays (five minutes of game time), and between two drops or segments.
pub const LIFE: u64 = 3000;
pub const EVERY: u64 = 10;

/// Half the width of a body dragged along the ground (world pixels).
const HALF_WIDTH: f32 = 0.26 * PIXELS_PER_METRE;
/// Moved less than this between two samples: he lay still (world pixels).
const STILL: f32 = 2.0;

#[derive(Debug, Clone, Copy)]
struct Mark {
    unit: u32,
    at: [f32; 3],
    tick: u64,
    piece: Option<Piece>,
}

/// A smear segment from `Mark::at`.
#[derive(Debug, Clone, Copy)]
struct Piece {
    to: [f32; 3],
    start_width: f32,
    width: f32,
    density: f32,
    /// Samples he lay still at its end (a wider patch gathers there).
    still: u8,
}

/// Where a crawler was last sampled and how wide his smear ended.
#[derive(Debug, Clone, Copy)]
struct Crawler {
    at: [f32; 3],
    tick: u64,
    width: f32,
}

#[derive(Default)]
pub struct Trails {
    marks: VecDeque<Mark>,
    crawlers: HashMap<u32, Crawler>,
}

/// 0..1 from unit and tick.
fn hash(unit: u32, tick: u64) -> f32 {
    let mut h = u64::from(unit).wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ tick.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    h ^= h >> 31;
    h = h.wrapping_mul(0x94d0_49bb_1331_11eb);
    h ^= h >> 29;
    (h >> 40) as f32 / (1u64 << 24) as f32
}

impl Trails {
    /// Samples a bleeding unit at `now` (a multiple of [`EVERY`], once per tick).
    pub fn sample(&mut self, unit: u32, at: [f32; 3], crawling: bool, now: u64) {
        if !crawling {
            self.marks.push_back(Mark { unit, at, tick: now, piece: None });
            return;
        }
        let last = self.crawlers.get(&unit).copied().filter(|c| c.tick + EVERY == now);
        let Some(last) = last else {
            self.crawlers.insert(unit, Crawler { at, tick: now, width: HALF_WIDTH });
            return;
        };
        let (dx, dy) = (at[0] - last.at[0], at[1] - last.at[1]);
        let len = (dx * dx + dy * dy).sqrt();
        if len < STILL {
            // He lay still: the blood gathers at the end of his last segment.
            let piece = self
                .marks
                .iter_mut()
                .rev()
                .find(|m| m.unit == unit && m.piece.is_some())
                .and_then(|m| m.piece.as_mut());
            if let Some(p) = piece.filter(|p| p.to == last.at) {
                p.still = p.still.saturating_add(1);
            }
            self.crawlers.insert(unit, Crawler { tick: now, ..last });
            return;
        }
        // Slow: a wide, heavy smear; quick: a narrow, broken one.
        let density = 1.0 - 0.75 * model::ramp(len, 4.0, 30.0);
        let width = HALF_WIDTH * (0.75 + 0.4 * density) * (0.88 + 0.24 * hash(unit, now));
        self.marks.push_back(Mark {
            unit,
            at: last.at,
            tick: now,
            piece: Some(Piece { to: at, start_width: last.width, width, density, still: 0 }),
        });
        self.crawlers.insert(unit, Crawler { at, tick: now, width });
    }

    /// Drops what has faded by `now`.
    pub fn expire(&mut self, now: u64) {
        while self.marks.front().is_some_and(|m| m.tick + LIFE < now) {
            self.marks.pop_front();
        }
        self.crawlers.retain(|_, c| c.tick + 2 * EVERY >= now);
    }

    /// The trails' decals at `now`.
    pub fn pools(&self, now: u64, out: &mut Vec<BloodPool>) {
        for m in &self.marks {
            let t = now.saturating_sub(m.tick) as f32;
            let age = model::ramp(t, 40.0, 900.0);
            let fade = 1.0 - model::ramp(t, LIFE as f32 * 0.8, LIFE as f32);
            let seed = (m.unit as f32 * 0.618).fract() * 97.0;
            let Some(p) = m.piece else {
                out.push(BloodPool {
                    centre: m.at,
                    radius: 0.18 * PIXELS_PER_METRE,
                    age,
                    fade,
                    seed: (m.unit as f32 * 0.618 + m.tick as f32 * 0.1).fract() * 97.0,
                    ..Default::default()
                });
                continue;
            };
            let (dx, dy) = (p.to[0] - m.at[0], p.to[1] - m.at[1]);
            let len = (dx * dx + dy * dy).sqrt().max(1e-3);
            out.push(BloodPool {
                centre: m.at,
                radius: p.width,
                along: [dx / len, dy / len],
                age,
                fade,
                seed,
                smear: Some(Smear { length: len, start_width: p.start_width / p.width, density: p.density }),
                ..Default::default()
            });
            if p.still > 0 {
                // A wider, heavier patch where he lay still.
                let grow = 1.0 + 0.12 * f32::from(p.still.min(8));
                out.push(BloodPool {
                    centre: p.to,
                    radius: p.width * grow,
                    along: [dx / len, dy / len],
                    age,
                    fade,
                    seed,
                    smear: Some(Smear { length: 0.0, start_width: 1.0, density: 1.0 }),
                    ..Default::default()
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crawl(trails: &mut Trails, unit: u32, path: &[[f32; 2]]) {
        for (i, p) in path.iter().enumerate() {
            trails.sample(unit, [p[0], p[1], 0.0], true, 100 + i as u64 * EVERY);
        }
    }

    #[test]
    fn crawler_segments_join_into_a_ribbon() {
        let mut t = Trails::default();
        crawl(&mut t, 7, &[[0.0, 0.0], [10.0, 0.0], [20.0, 5.0], [30.0, 5.0]]);
        let mut pools = Vec::new();
        t.pools(200, &mut pools);
        assert_eq!(pools.len(), 3);
        for w in pools.windows(2) {
            let (a, b) = (w[0], w[1]);
            let s = a.smear.unwrap();
            let end = [a.centre[0] + a.along[0] * s.length, a.centre[1] + a.along[1] * s.length];
            assert!((end[0] - b.centre[0]).abs() < 1e-3 && (end[1] - b.centre[1]).abs() < 1e-3);
            // The next segment starts as wide as this one ended.
            assert!((b.smear.unwrap().start_width * b.radius - a.radius).abs() < 1e-3);
        }
    }

    #[test]
    fn slow_crawling_is_heavier_and_lying_still_gathers_a_patch() {
        let mut slow = Trails::default();
        crawl(&mut slow, 1, &[[0.0, 0.0], [5.0, 0.0]]);
        let mut fast = Trails::default();
        crawl(&mut fast, 1, &[[0.0, 0.0], [30.0, 0.0]]);
        let (mut a, mut b) = (Vec::new(), Vec::new());
        slow.pools(200, &mut a);
        fast.pools(200, &mut b);
        assert!(a[0].smear.unwrap().density > b[0].smear.unwrap().density);
        crawl(&mut slow, 1, &[[0.0, 0.0], [5.0, 0.0], [5.5, 0.0], [5.5, 0.0]]);
        let mut c = Vec::new();
        slow.pools(200, &mut c);
        let patch = c.last().unwrap();
        assert_eq!(patch.smear.unwrap().length, 0.0);
        assert!(patch.radius > c[c.len() - 2].radius);
    }

    #[test]
    fn walkers_leave_drops_and_everything_fades() {
        let mut t = Trails::default();
        t.sample(3, [0.0, 0.0, 0.0], false, 10);
        let mut pools = Vec::new();
        t.pools(20, &mut pools);
        assert!(pools[0].smear.is_none());
        t.expire(10 + LIFE + 1);
        pools.clear();
        t.pools(10 + LIFE + 1, &mut pools);
        assert!(pools.is_empty());
    }
}
