//! Fire in the 3D view (#256 follow-up), render only: flames, embers, a warm glow on the
//! ground and smoke over every burning shell fire (`World::ground_fires`, rule
//! `hit_locations`), a scorched patch that grows under it and stays after it has burnt out,
//! and flames and trailing smoke on burning men (`Unit::burning`, from shell fires or the
//! original's `X`/`F` weapons) and on the corpses of the burned for a while. Rain shrinks the
//! flames and makes the smoke thicker and paler (steam). Variety comes from hashes of the
//! fire or unit and an emission counter, never the simulation's RNG; nothing goes back to
//! the simulation. The particles are the `acr_render::particles` kinds `Flame`, `Ember`,
//! `Glow` and `Smoke`.
//!
//! [`FireView::burn`] is the per-unit burn amount (0..1) for the charring tint of the model
//! shader (#207): it grows while a man burns and stays on his corpse.

use std::collections::HashMap;

use acr_render::ground_state::{GroundState, Stamp, metres};
use acr_render::particles::{Burst, ParticleKind, ParticleSink, hash, unit};
use acr_render::terrain::TerrainData;
use acr_sim::World;
use acr_sim::combat::DeathCause;
use acr_sim::ground_fire::{BURN_TICKS, GroundFire};

use super::particle_view::ground_z;

/// Flame colour (sRGB, before the shader's heat ramp).
const FLAME: [u8; 3] = [255, 120, 30];
const EMBER: [u8; 3] = [255, 150, 60];
const GLOW: [u8; 3] = [255, 130, 50];
/// Smoke of a dry fire, and of one in the rain (steam and smoke).
const SMOKE: [u8; 3] = [58, 54, 50];
const STEAM: [u8; 3] = [150, 148, 144];
/// Seconds a man must burn to be fully charred.
const CHAR_SECONDS: f32 = 8.0;
/// Seconds the corpse of a burned man keeps flames, and smoulders (smoke).
const CORPSE_FLAMES: f32 = 8.0;
const CORPSE_SMOKE: f32 = 40.0;
/// Seconds a shell fire takes to flare up; it dies down over three times that at its end.
const FLARE: f32 = 2.0;
/// Ticks per second at normal speed (the fires' ages are ticks).
const TICKS_PER_SECOND: f32 = 10.0;
/// Source keys of units (the fires' keys never set this bit).
const UNIT_KEY: u64 = 1 << 63;
/// Source keys of buildings (#303): this bit, the unit, the anchor and the emitter.
const BUILDING_KEY: u64 = 1 << 62;
/// Dust of a collapse, and the colour of its debris.
const DUST: [u8; 3] = [112, 102, 88];
const DEBRIS: [u8; 3] = [60, 46, 34];

#[derive(Default)]
pub struct FireView {
    /// Burn amount by unit index (0..1).
    burn: HashMap<u32, f32>,
    /// Emission carried over between frames per source (fractional particles) and the
    /// frame it was last used.
    carry: HashMap<u64, (f32, u32)>,
    /// Frames so far.
    frame: u32,
    /// Bursts emitted so far (seeds).
    count: u32,
    /// The tick of the last update: going back means another game.
    tick: u64,
    /// Second of its age each fire's scorch was last stamped at.
    scorched: HashMap<u64, u32>,
}

fn fire_key(f: &GroundFire) -> u64 {
    // Bits 0-3 tell a fire's emitters apart.
    ((f.start << 4) ^ (u64::from(f.x as u32) << 24) ^ (u64::from(f.y as u32) << 44)) & !(UNIT_KEY | 0xF)
}

impl FireView {
    /// The burn amount of unit `index` (0 none, 1 fully charred): for the model tint (#207).
    pub fn burn(&self, index: u32) -> f32 {
        self.burn.get(&index).copied().unwrap_or(0.0)
    }

    /// Emits `rate * dt` particles like `burst` (carrying the fraction) for the source `key`.
    fn emit(&mut self, sink: &mut dyn ParticleSink, key: u64, dt: f32, rate: f32, burst: Burst) {
        let frame = self.frame;
        let have = self.carry.entry(key).or_insert((0.0, frame));
        have.0 += rate.max(0.0) * dt;
        have.1 = frame;
        let n = have.0.floor();
        if n < 1.0 {
            return;
        }
        have.0 -= n;
        self.count = self.count.wrapping_add(1);
        let seed = hash(self.count ^ hash(key as u32 ^ (key >> 32) as u32));
        sink.emit(&Burst { count: n.min(64.0) as u16, seed, ..burst });
    }

    /// One frame of `dt` seconds; `rain` when it rains. `state` is the ground state of the
    /// detailed ground (scorch), if any.
    pub fn update(
        &mut self,
        world: &World,
        terrain: &TerrainData,
        sink: &mut dyn ParticleSink,
        mut state: Option<&mut GroundState>,
        dt: f32,
        rain: bool,
    ) {
        let now = world.tick.0;
        if now < self.tick {
            self.burn.clear();
            self.carry.clear();
            self.scorched.clear();
        }
        self.tick = now;
        self.frame = self.frame.wrapping_add(1);
        let dt = dt.clamp(0.0, 0.1);
        // Rain: smaller, fewer flames; more, paler smoke.
        let (flame_k, smoke_k, smoke_color) = if rain { (0.6, 2.0, STEAM) } else { (1.0, 1.0, SMOKE) };
        for f in &world.ground_fires.list {
            let key = fire_key(f);
            let age = f.age(now) as f32 / TICKS_PER_SECOND;
            let left = (BURN_TICKS as f32 / TICKS_PER_SECOND - age).max(0.0);
            // Flares up, burns, dies down over its last seconds.
            let strength = (age / FLARE).min(1.0) * (left / (FLARE * 3.0)).min(1.0);
            let r = f.radius as f32;
            let at = [f.x as f32, f.y as f32, ground_z(terrain, f.x as f32, f.y as f32)];
            let flames = Burst {
                at,
                dir: [0.0, 0.0, 1.0],
                spread: 0.25,
                speed: [20.0, 60.0],
                count: 1,
                kind: ParticleKind::Flame,
                life: [0.35, 0.7],
                size: [metres(0.2) * flame_k, metres(0.42) * flame_k],
                color: FLAME,
                scatter: r * 0.75,
                seed: 0,
            };
            self.emit(sink, key, dt, 40.0 * strength * flame_k, flames);
            let embers = Burst {
                spread: 0.5,
                speed: [30.0, 90.0],
                kind: ParticleKind::Ember,
                life: [0.8, 1.8],
                size: [0.5, 1.0],
                color: EMBER,
                scatter: r * 0.6,
                ..flames
            };
            self.emit(sink, key | 1, dt, 10.0 * strength * flame_k, embers);
            let glow = Burst {
                at: [at[0], at[1], at[2] + 1.0],
                spread: 0.0,
                speed: [0.0, 0.0],
                kind: ParticleKind::Glow,
                life: [0.45, 0.6],
                size: [r * 1.6, r * 1.9],
                color: GLOW,
                scatter: r * 0.15,
                ..flames
            };
            self.emit(sink, key | 2, dt, 4.0 * strength, glow);
            let smoke = Burst {
                at: [at[0], at[1], at[2] + metres(0.8)],
                spread: 0.3,
                speed: [15.0, 35.0],
                kind: ParticleKind::Smoke,
                life: [3.5, 6.0],
                size: [metres(0.35), metres(0.6)],
                color: smoke_color,
                scatter: r * 0.5,
                ..flames
            };
            // Smoke keeps rising a little while the flames die down.
            let smoke_strength = (age / FLARE).min(1.0) * (left / (FLARE * 6.0)).clamp(0.25, 1.0);
            self.emit(sink, key | 3, dt, 6.0 * smoke_strength * smoke_k, smoke);
            // The ground chars under it: blotches that darken over its first ten seconds and
            // stay (the ground state keeps scorch).
            if let Some(state) = state.as_deref_mut() {
                let second = age as u32;
                let done = self.scorched.entry(key).or_insert(u32::MAX);
                if *done != second && second <= 10 {
                    *done = second;
                    let k = (0.25 + 0.075 * second as f32).min(1.0);
                    let s = hash(key as u32 ^ 0x5c0c);
                    for b in 0..4u32 {
                        let (sa, ca) = (std::f32::consts::TAU * unit(s, b * 4)).sin_cos();
                        let off = r * 0.35 * unit(s, b * 4 + 1);
                        let half = r * (0.55 + 0.35 * unit(s, b * 4 + 2));
                        state.stamp(&Stamp {
                            scorch: k * (0.8 + 0.2 * unit(s, b * 4 + 3)),
                            ..Stamp::print([at[0] + ca * off, at[1] + sa * off], [ca, sa], [half, half * 0.8], 0.0, 0.1)
                        });
                    }
                }
            }
        }
        let fires = &world.ground_fires.list;
        self.scorched.retain(|k, _| fires.iter().any(|f| fire_key(f) == *k));
        // Burning men, and the corpses of the burned.
        for (i, u) in world.units.iter().enumerate() {
            let index = i as u32;
            let living = u.living();
            let death = if living { None } else { world.deaths.get(&index).filter(|d| d.cause == DeathCause::Burned) };
            if living && u.burning {
                let b = self.burn.entry(index).or_insert(0.0);
                *b = (*b + dt / CHAR_SECONDS).min(1.0);
            } else if death.is_some() {
                // Burned to death: at least well charred.
                let b = self.burn.entry(index).or_insert(0.0);
                *b = b.max(0.6);
            }
            let (on, smoulder) = match death {
                _ if living => (if u.burning { 1.0 } else { 0.0 }, 0.0),
                Some(d) => {
                    let since = now.saturating_sub(d.tick) as f32 / TICKS_PER_SECOND;
                    ((1.0 - since / CORPSE_FLAMES).max(0.0), (1.0 - since / CORPSE_SMOKE).max(0.0))
                }
                None => (0.0, 0.0),
            };
            if on <= 0.0 && smoulder <= 0.0 {
                continue;
            }
            let (x, y) = u.pos.pixels();
            let (x, y) = (x as f32, y as f32);
            let z = ground_z(terrain, x, y);
            let key = UNIT_KEY | (u64::from(index) << 4);
            // A standing man burns from his legs to his head (three bands of body points);
            // a corpse burns low, spread over his length.
            let (mid, scatter, half) = if living {
                let top = if u.body.mounted { metres(2.4) } else { metres(1.6) };
                (z + top * 0.5, metres(0.18), top * 0.5)
            } else {
                (z + metres(0.1), metres(0.6), 0.0)
            };
            let flames = Burst {
                at: [x, y, mid],
                dir: [0.0, 0.0, 1.0],
                spread: 0.3,
                speed: [15.0, 45.0],
                count: 1,
                kind: ParticleKind::Flame,
                life: [0.25, 0.5],
                size: [metres(0.12) * flame_k, metres(0.26) * flame_k],
                color: FLAME,
                scatter,
                seed: 0,
            };
            for band in 0..3u64 {
                let h = (band as f32 - 1.0) * half * 0.6;
                let b = Burst { at: [x, y, mid + h], ..flames };
                self.emit(sink, key | band, dt, 18.0 * on * flame_k, b);
            }
            let embers = Burst {
                speed: [25.0, 70.0],
                kind: ParticleKind::Ember,
                life: [0.6, 1.4],
                size: [0.4, 0.9],
                color: EMBER,
                ..flames
            };
            self.emit(sink, key | 4, dt, 4.0 * on * flame_k, embers);
            let glow = Burst {
                at: [x, y, z + 1.0],
                spread: 0.0,
                speed: [0.0, 0.0],
                kind: ParticleKind::Glow,
                life: [0.45, 0.6],
                size: [metres(1.0), metres(1.3)],
                color: GLOW,
                scatter: 0.0,
                ..flames
            };
            self.emit(sink, key | 5, dt, 4.0 * on, glow);
            // Smoke is left behind where he walks: a trail.
            let smoke = Burst {
                at: [x, y, mid + half],
                spread: 0.3,
                speed: [10.0, 25.0],
                kind: ParticleKind::Smoke,
                life: [2.5, 4.5],
                size: [metres(0.2), metres(0.35)],
                color: smoke_color,
                scatter: metres(0.15),
                ..flames
            };
            self.emit(sink, key | 6, dt, 5.0 * on.max(smoulder) * smoke_k, smoke);
        }
        let units = world.units.len();
        self.burn.retain(|&i, _| (i as usize) < units);
        let frame = self.frame;
        // Buildings emit after this (`buildings`), with the last frame's number.
        self.carry.retain(|k, (_, used)| *used == frame || (k & BUILDING_KEY != 0 && frame.wrapping_sub(*used) <= 1));
    }
}

impl FireView {
    /// Flames, embers and smoke of burning buildings, and the dust and debris of those
    /// falling in (#303, `ruin_view`), for one frame of `dt` seconds.
    pub fn buildings(&mut self, list: &[super::ruin_view::Burning], sink: &mut dyn ParticleSink, dt: f32, rain: bool) {
        let dt = dt.clamp(0.0, 0.1);
        let (flame_k, smoke_k, smoke_color) = if rain { (0.6, 2.0, STEAM) } else { (1.0, 1.0, SMOKE) };
        for b in list {
            let key = BUILDING_KEY | (u64::from(b.unit) << 12);
            for (j, &at) in b.fire.iter().enumerate().take(255) {
                let k = key | ((j as u64) << 4);
                let flames = Burst {
                    at,
                    dir: [0.0, 0.0, 1.0],
                    spread: 0.25,
                    speed: [25.0, 70.0],
                    count: 1,
                    kind: ParticleKind::Flame,
                    life: [0.4, 0.8],
                    size: [metres(0.35) * flame_k, metres(0.7) * flame_k],
                    color: FLAME,
                    scatter: metres(0.6),
                    seed: 0,
                };
                self.emit(sink, k, dt, 22.0 * b.flames * flame_k, flames);
                let embers = Burst {
                    spread: 0.5,
                    speed: [30.0, 100.0],
                    kind: ParticleKind::Ember,
                    life: [0.8, 2.0],
                    size: [0.5, 1.1],
                    color: EMBER,
                    ..flames
                };
                self.emit(sink, k | 1, dt, 5.0 * b.flames * flame_k, embers);
            }
            for (j, &at) in b.smoke.iter().enumerate().take(255) {
                let smoke = Burst {
                    at: [at[0], at[1], at[2] + metres(0.5)],
                    dir: [0.0, 0.0, 1.0],
                    spread: 0.3,
                    speed: [15.0, 40.0],
                    count: 1,
                    kind: ParticleKind::Smoke,
                    life: [3.5, 6.0],
                    size: [metres(0.7), metres(1.1)],
                    color: smoke_color,
                    scatter: metres(0.6),
                    seed: 0,
                };
                // At most `ruin_view::SMOKE_COLUMNS` columns of slightly larger puffs: the particles
                // pass is fill-bound, so fewer puffs, not bigger ones, make it cheaper.
                self.emit(sink, key | ((j as u64) << 4) | 2, dt, 2.0 * b.smoking * smoke_k, smoke);
            }
            if b.dust > 0.0 {
                // Dust (a pale smoke) rolls out over the footprint, splinters fly from the falling
                // walls.
                let r = b.half[0].max(b.half[1]);
                let dust = Burst {
                    at: [b.ground[0], b.ground[1], b.ground[2] + metres(0.5)],
                    dir: [0.0, 0.0, 1.0],
                    spread: 0.9,
                    speed: [10.0, 35.0],
                    count: 1,
                    kind: ParticleKind::Smoke,
                    life: [1.5, 3.0],
                    size: [metres(0.8), metres(1.6)],
                    color: DUST,
                    scatter: r,
                    seed: 0,
                };
                // Low and short-lived, so the falling walls show through and the ruin is clear
                // once it settles.
                self.emit(sink, key | 3, dt, 18.0 * b.dust, dust);
                let debris = Burst {
                    at: [b.ground[0], b.ground[1], b.ground[2] + b.top * 0.4],
                    spread: 0.7,
                    speed: [40.0, 140.0],
                    kind: ParticleKind::Splinter,
                    life: [1.5, 3.0],
                    size: [metres(0.15), metres(0.45)],
                    color: DEBRIS,
                    scatter: r * 0.8,
                    ..dust
                };
                self.emit(sink, key | 4, dt, 50.0 * b.dust, debris);
            }
            // A puff where a falling piece strikes the ground.
            for (j, &at) in b.landings.iter().enumerate().take(255) {
                let puff = Burst {
                    at: [at[0], at[1], at[2] + metres(0.2)],
                    dir: [0.0, 0.0, 1.0],
                    spread: 1.2,
                    speed: [15.0, 45.0],
                    count: 1,
                    kind: ParticleKind::Smoke,
                    life: [1.0, 2.2],
                    size: [metres(0.4), metres(0.9)],
                    color: DUST,
                    scatter: metres(0.5),
                    seed: 0,
                };
                self.emit(sink, key | ((j as u64) << 4) | 5, dt, 24.0, puff);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Record(Vec<Burst>);
    impl ParticleSink for Record {
        fn emit(&mut self, b: &Burst) {
            self.0.push(*b);
        }
    }

    #[test]
    fn emission_carries_fractions() {
        let mut v = FireView::default();
        let mut r = Record::default();
        let burst = Burst {
            at: [0.0; 3],
            dir: [0.0, 0.0, 1.0],
            spread: 0.0,
            speed: [0.0, 0.0],
            count: 1,
            kind: ParticleKind::Flame,
            life: [1.0, 1.0],
            size: [1.0, 1.0],
            color: FLAME,
            scatter: 0.0,
            seed: 0,
        };
        // One second at 60 frames and 30 per second: 30 particles, whatever the frame rate.
        for _ in 0..60 {
            v.emit(&mut r, 7, 1.0 / 60.0, 30.0, burst);
        }
        let n: u32 = r.0.iter().map(|b| u32::from(b.count)).sum();
        assert!((29..=30).contains(&n), "{n}");
        assert!(r.0.windows(2).all(|w| w[0].seed != w[1].seed));
    }
}
