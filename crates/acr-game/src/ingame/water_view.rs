//! What the game state does to the water surface (docs/water.md, phases 1b and 2), render
//! only: ships ride the waves (heave, pitch and roll from the same Gerstner sum as the
//! shader, `acr_render::water::Waves`), ships under way leave a wake, and shot that falls into
//! the water leaves a ring. Nothing here goes back to the simulation.

use std::collections::HashMap;

use acr_render::math::{Mat4, Vec3};
use acr_render::terrain::TerrainData;
use acr_render::view3d;
use acr_render::water::{self, Disturbances, Ring, Wake, Waves};
use acr_sim::World;
use acr_sim::world::Unit;

const TPS: f32 = acr_sim::ORIGINAL_TICKS_PER_SECOND as f32;
/// Depth units from which a ship counts as afloat (ships float above 130, i.e. 3 units).
const AFLOAT: f32 = 2.0;
/// Shot lower than this (true px above the ground) over water has landed in it.
const SPLASH_HEIGHT: i32 = 8;

/// A ship's last positions, for its speed and heading.
#[derive(Clone, Copy, Default)]
struct Track {
    at: [f32; 2],
    tick: u64,
    vel: [f32; 2],
    /// The tick the ship was last drawn (stale tracks are dropped).
    seen: u64,
}

#[derive(Default)]
pub struct WaterView {
    tracks: HashMap<u32, Track>,
    pub disturbances: Disturbances,
    waves: Option<(u32, Waves)>,
}

impl WaterView {
    /// Starts a frame's update at `tick`: forgets ships no longer drawn and old rings.
    pub fn begin(&mut self, tick: u64, seconds: f32) {
        self.tracks.retain(|_, t| t.seen + 20 >= tick && t.seen <= tick);
        self.disturbances.wakes.clear();
        self.disturbances.rings.retain(|r| seconds - r.time < water::RING_SECONDS && r.time <= seconds);
    }

    fn waves(&mut self, sea: (usize, usize, &[u8])) -> Waves {
        let seed = water::seed(sea);
        match self.waves {
            Some((s, w)) if s == seed => w,
            _ => {
                let w = Waves::new(seed);
                self.waves = Some((seed, w));
                w
            }
        }
    }

    /// The transform of ship `u` (model radius `radius` px) standing at `ground` and turned by
    /// `yaw`, riding the waves at `seconds` on the water's clock; also notes its wake. `None`
    /// when it is not afloat (a gun on land, a wreck on the shore, no water on the map).
    #[allow(clippy::too_many_arguments)]
    pub fn ship(
        &mut self,
        u: &Unit,
        tick: u64,
        terrain: &TerrainData,
        ground: Vec3,
        yaw: f32,
        radius: f32,
        seconds: f32,
    ) -> Option<Mat4> {
        let sea = terrain.water()?;
        if water::depth_units(sea, ground[0], ground[1]) < AFLOAT {
            return None;
        }
        let waves = self.waves(sea);
        // The hull: half the model's radius each way (the radius holds the masts too), a
        // quarter of that abeam.
        let half = radius * 0.5;
        let beam = half * 0.25;
        // Model facing at yaw (view3d::model_matrix): glTF +z → world (sin, cos).
        let (s, c) = yaw.sin_cos();
        let fwd = [s, c];
        let right = [-c, s];
        let at = |a: f32, r: f32| {
            let p = [ground[0] + fwd[0] * a + right[0] * r, ground[1] + fwd[1] * a + right[1] * r];
            let scale = water::wave_scale(water::depth_units(sea, p[0], p[1]));
            waves.sample(p, seconds % 3600.0, scale).height
        };
        // Bow, stern, both sides at the bow and the stern quarters: the hull spans many short
        // waves, so their heights average out and the hull answers only to the long ones.
        let (bow, stern) = (at(half * 0.7, 0.0) + at(half * 0.35, 0.0), at(-half * 0.7, 0.0) + at(-half * 0.35, 0.0));
        let (port, starboard) =
            (at(half * 0.3, -beam) + at(-half * 0.3, -beam), at(half * 0.3, beam) + at(-half * 0.3, beam));
        let heave = (bow + stern + port + starboard) / 8.0 + at(0.0, 0.0) * 0.5;
        // Slopes, damped: a hull of this mass lags and answers with less than the water's slope.
        let pitch = (bow - stern) / 2.0 / (half * 1.05) * 0.8;
        let roll = (starboard - port) / 2.0 / (2.0 * beam) * 0.6;
        let placed = [ground[0], ground[1], ground[2] + heave * 0.7];

        // Speed and heading from the position over the ticks (the simulation moves ships by
        // whole pixels; average over a few ticks).
        let id = u.id.0;
        let track = self.tracks.entry(id).or_insert(Track { at: [ground[0], ground[1]], tick, ..Default::default() });
        track.seen = tick;
        if tick >= track.tick + 3 {
            let dt = (tick - track.tick) as f32 / TPS;
            let v = [(ground[0] - track.at[0]) / dt, (ground[1] - track.at[1]) / dt];
            let k = if dt > 2.0 { 1.0 } else { 0.6 };
            track.vel = [track.vel[0] + (v[0] - track.vel[0]) * k, track.vel[1] + (v[1] - track.vel[1]) * k];
            track.at = [ground[0], ground[1]];
            track.tick = tick;
        } else if tick < track.tick {
            *track = Track { at: [ground[0], ground[1]], tick, seen: tick, ..Default::default() };
        }
        let speed = (track.vel[0] * track.vel[0] + track.vel[1] * track.vel[1]).sqrt();
        if u.alive {
            self.disturbances.wakes.push(Wake {
                at: [ground[0], ground[1]],
                // The bow's direction (ships sail forwards; the velocity lags in turns).
                dir: fwd,
                // At anchor (or crawling along): only the waterline's foam.
                speed: if speed > 3.0 { speed } else { 0.0 },
                half_length: half,
            });
        }
        Some(view3d::model_matrix_tilted(placed, yaw, 1.0, pitch, roll))
    }

    /// Notes shot that falls into the water this tick: a ring and a boil of foam where it
    /// lands (`projectile.rs` bursts on `media::WATER` the same way).
    pub fn shots(&mut self, world: &World, terrain: &TerrainData, seconds: f32) {
        let Some(sea) = terrain.water() else { return };
        for p in world.projectiles.iter() {
            let (x, y, z) = (p.pos.0 >> 14, p.pos.1 >> 14, p.pos.2 >> 14);
            if p.vel.2 > 0 || z > SPLASH_HEIGHT + (terrain.height_at(x, y)).max(0) {
                continue;
            }
            let at = [x as f32, ((y >> 1) * 2) as f32];
            if water::depth_units(sea, at[0], at[1]) < 1.0 {
                continue;
            }
            // One ring per landing: the ball may show for a tick or two at the surface.
            let fresh = self.disturbances.rings.iter().all(|r| {
                let d = [r.at[0] - at[0], r.at[1] - at[1]];
                d[0] * d[0] + d[1] * d[1] > 40.0 * 40.0 || seconds - r.time > 0.6
            });
            if fresh {
                self.disturbances.rings.push(Ring { at, time: seconds, strength: 1.0 });
            }
        }
    }
}
