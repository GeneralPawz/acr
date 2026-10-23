//! What the game state throws into the air (docs/ground.md phase 3), render only: earth
//! fountains thrown forward along the flight where cannonballs strike or bounce, a ring of
//! earth and dust where shells burst, dust puffs where musket balls strike the ground, mud
//! flicked up by horses on wet ground, a light dust haze over marching columns on dry
//! ground, and a splash column with droplets where shot falls into water. Variety comes from
//! hashes of the impact or footfall, never from the simulation's RNG; nothing goes back to
//! the simulation. The particles live in `acr_render::particles`.

use acr_render::ground::{self, CLASSES};
use acr_render::ground_state::{GroundState, Stamp, metres};
use acr_render::particles::{Burst, Landing, ParticleKind, ParticleSink, Particles, hash, unit};
use acr_render::terrain::TerrainData;
use acr_render::view3d::COS_E;
use acr_render::water;
use acr_sim::World;
use acr_sim::combat::{DeathCause, ImpactRecord};

use super::ground_marks::{Footfall, Gait};

/// Soil colour (sRGB) of each material class (`ground::CLASSES` order), for clods and dust.
const SOIL: [[u8; 3]; 9] = [
    [96, 76, 52],    // grass: the soil under the turf
    [100, 80, 54],   // meadow
    [118, 94, 66],   // dirt
    [64, 50, 36],    // mud
    [178, 152, 112], // sand
    [122, 116, 106], // rock
    [74, 58, 42],    // forest
    [232, 234, 240], // snow
    [110, 86, 58],   // field
];
const OTHER_SOIL: [u8; 3] = [104, 86, 64];
const GRASS: [u8; 3] = [78, 104, 44];
const WATER_SPRAY: [u8; 3] = [226, 234, 238];
const WATER_DROP: [u8; 3] = [196, 212, 222];

/// The ground under a point, as the particles see it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Soil {
    pub color: [u8; 3],
    /// Mud: the mud class, or wet from the ground state.
    pub wet: bool,
    /// Dry and loose (dirt, sand, field): dust.
    pub dusty: bool,
    /// Turf (grass, meadow, field): grass bits.
    pub turf: bool,
}

impl Soil {
    pub fn of_class(class: u8, wet: bool) -> Self {
        let name = CLASSES.get(usize::from(class)).copied().unwrap_or("");
        let wet = wet || name == "mud";
        Self {
            color: if wet { SOIL[3] } else { SOIL.get(usize::from(class)).copied().unwrap_or(OTHER_SOIL) },
            wet,
            dusty: !wet && matches!(name, "dirt" | "sand" | "field"),
            turf: matches!(name, "grass" | "meadow" | "field"),
        }
    }

    fn dust(self) -> [u8; 3] {
        // Dust is the soil, lighter and greyer.
        self.color.map(|c| ((u16::from(c) * 3 + 150 * 2) / 5) as u8)
    }
}

#[derive(Default)]
pub struct ParticleView {
    pub particles: Particles,
    last_impact: u64,
    /// The tick of the last update: going back means another game.
    tick: u64,
}

/// A burst with the common defaults.
fn burst(at: [f32; 3], kind: ParticleKind, seed: u32) -> Burst {
    Burst {
        at,
        dir: [0.0, 0.0, 1.0],
        spread: 0.5,
        speed: [50.0, 100.0],
        count: 1,
        kind,
        life: [2.0, 3.0],
        size: [2.0, 3.0],
        color: OTHER_SOIL,
        scatter: 0.0,
        seed,
    }
}

/// The earth (or water) thrown by a round of `cause` striking at `at` (true height in z),
/// flying along `dir` (unit, horizontal, may be zero) with `energy` (1 for a 12 px/tick
/// ball). `soil` is the ground there, `None` for water.
pub fn impact(
    sink: &mut dyn ParticleSink,
    cause: DeathCause,
    at: [f32; 3],
    dir: [f32; 2],
    energy: f32,
    soil: Option<Soil>,
    seed: u32,
) {
    let e = energy.clamp(0.3, 2.0);
    let fwd = |k: f32| [dir[0] * k, dir[1] * k, 1.0];
    let Some(soil) = soil else {
        match cause {
            DeathCause::Cannonball | DeathCause::Explosion | DeathCause::Grapeshot => {
                let big = if cause == DeathCause::Grapeshot { 0.3 } else { e };
                // A tall column, droplets falling from it, and spray spreading at its foot.
                let column = Burst {
                    dir: fwd(0.15),
                    spread: 0.16,
                    speed: [200.0 * big.sqrt(), 430.0 * big.sqrt()],
                    count: (34.0 * big) as u16 + 6,
                    life: [1.0, 1.9],
                    size: [7.0, 15.0],
                    color: WATER_SPRAY,
                    scatter: 6.0,
                    ..burst(at, ParticleKind::Spray, seed)
                };
                sink.emit(&column);
                sink.emit(&Burst {
                    spread: 0.45,
                    speed: [120.0, 380.0 * big.sqrt()],
                    count: (60.0 * big) as u16 + 6,
                    life: [2.0, 3.0],
                    size: [0.9, 2.0],
                    color: WATER_DROP,
                    ..burst(at, ParticleKind::Droplet, seed ^ 0x5151)
                });
                sink.emit(&Burst {
                    spread: 1.35,
                    speed: [40.0, 120.0],
                    count: (14.0 * big) as u16 + 2,
                    life: [0.8, 1.4],
                    size: [7.0, 13.0],
                    color: WATER_SPRAY,
                    scatter: 8.0,
                    ..burst(at, ParticleKind::Spray, seed ^ 0xa3a3)
                });
            }
            _ => sink.emit(&Burst {
                spread: 0.3,
                speed: [60.0, 140.0],
                count: 8,
                life: [0.5, 0.9],
                size: [0.5, 1.0],
                color: WATER_DROP,
                ..burst(at, ParticleKind::Droplet, seed)
            }),
        }
        return;
    };
    let earth = if soil.wet { ParticleKind::Mud } else { ParticleKind::Clod };
    match cause {
        DeathCause::Cannonball => {
            // The fountain is thrown forward along the flight, sized by the ball's energy.
            sink.emit(&Burst {
                dir: fwd(0.7),
                spread: 0.5,
                speed: [110.0 * e.sqrt(), 300.0 * e.sqrt()],
                count: (35.0 + 35.0 * e) as u16,
                life: [2.0, 4.0],
                size: [1.3, 5.0],
                color: soil.color,
                scatter: 6.0,
                ..burst(at, earth, seed)
            });
            // Fine earth higher up and back.
            sink.emit(&Burst {
                dir: fwd(0.25),
                spread: 0.35,
                speed: [160.0 * e.sqrt(), 360.0 * e.sqrt()],
                count: (12.0 * e) as u16 + 4,
                life: [2.0, 4.0],
                size: [0.6, 1.4],
                color: soil.color,
                scatter: 4.0,
                ..burst(at, earth, seed ^ 0x77)
            });
            if soil.turf {
                sink.emit(&Burst {
                    dir: fwd(0.6),
                    spread: 0.7,
                    speed: [80.0, 220.0 * e.sqrt()],
                    count: (14.0 * e) as u16 + 2,
                    life: [2.5, 4.0],
                    size: [1.6, 2.6],
                    color: GRASS,
                    scatter: 6.0,
                    ..burst(at, ParticleKind::Grass, seed ^ 0x99)
                });
            }
            let wet = if soil.wet { 0.5 } else { 1.0 };
            sink.emit(&Burst {
                dir: fwd(0.5),
                spread: 0.9,
                speed: [25.0, 80.0 * e.sqrt()],
                count: ((6.0 + 6.0 * e) * wet) as u16,
                life: [2.5, 4.5],
                size: [9.0, 18.0],
                color: soil.dust(),
                scatter: 10.0,
                ..burst([at[0], at[1], at[2] + 6.0], ParticleKind::Dust, seed ^ 0x1234)
            });
        }
        DeathCause::Explosion => {
            // A ring of earth: a wide cone up, the heavy clods low and far.
            sink.emit(&Burst {
                spread: 1.25,
                speed: [140.0, 360.0],
                count: 130,
                life: [2.0, 4.0],
                size: [1.0, 5.0],
                color: soil.color,
                scatter: 14.0,
                ..burst(at, earth, seed)
            });
            sink.emit(&Burst {
                spread: 0.4,
                speed: [200.0, 420.0],
                count: 30,
                life: [2.0, 4.0],
                size: [0.6, 1.6],
                color: soil.color,
                scatter: 8.0,
                ..burst(at, earth, seed ^ 0x77)
            });
            sink.emit(&Burst {
                spread: 1.45,
                speed: [50.0, 150.0],
                count: 26,
                life: [3.0, 5.5],
                size: [14.0, 30.0],
                color: soil.dust().map(|c| (u16::from(c) * 3 / 4) as u8),
                scatter: 20.0,
                ..burst([at[0], at[1], at[2] + 8.0], ParticleKind::Dust, seed ^ 0x1234)
            });
        }
        DeathCause::Grapeshot => {
            sink.emit(&Burst {
                dir: fwd(0.6),
                spread: 0.6,
                speed: [60.0, 150.0],
                count: 6,
                life: [1.0, 2.0],
                size: [0.6, 1.8],
                color: soil.color,
                ..burst(at, earth, seed)
            });
            sink.emit(&Burst {
                spread: 0.8,
                speed: [10.0, 40.0],
                count: 2,
                life: [1.5, 2.5],
                size: [5.0, 9.0],
                color: soil.dust(),
                ..burst([at[0], at[1], at[2] + 3.0], ParticleKind::Dust, seed ^ 0x1234)
            });
        }
        DeathCause::Gunshot | DeathCause::Arrow | DeathCause::Thrown => {
            // A musket ball: a small puff and a few crumbs.
            sink.emit(&Burst {
                dir: fwd(0.4),
                spread: 0.7,
                speed: [8.0, 35.0],
                count: if soil.wet { 1 } else { 3 },
                life: [1.2, 2.2],
                size: [3.0, 6.0],
                color: soil.dust(),
                ..burst([at[0], at[1], at[2] + 2.0], ParticleKind::Dust, seed)
            });
            sink.emit(&Burst {
                dir: fwd(0.5),
                spread: 0.5,
                speed: [50.0, 110.0],
                count: 4,
                life: [0.6, 1.2],
                size: [0.4, 0.9],
                color: soil.color,
                ..burst(at, earth, seed ^ 0x77)
            });
        }
        _ => {}
    }
}

/// What one footfall throws: mud flicked back from a horse's hooves on wet ground, dust
/// under men and horses on dry ground, now and then a blade of grass. `seed` decides.
pub fn footfall(sink: &mut dyn ParticleSink, f: &Footfall, at: [f32; 3], soil: Soil, seed: u32) {
    let back = [-f.along[0] * 0.7, -f.along[1] * 0.7, 1.0];
    let gallop = f.speed > 4.0;
    match f.gait {
        Gait::Hooves => {
            if soil.wet {
                sink.emit(&Burst {
                    dir: back,
                    spread: 0.5,
                    speed: [50.0, if gallop { 170.0 } else { 100.0 }],
                    count: if gallop { 6 } else { 3 },
                    life: [1.0, 2.0],
                    size: [0.6, 1.8],
                    color: soil.color,
                    scatter: 6.0,
                    ..burst(at, ParticleKind::Mud, seed)
                });
            } else if soil.dusty {
                sink.emit(&Burst {
                    dir: back,
                    spread: 0.8,
                    speed: [10.0, 30.0],
                    count: 1,
                    life: [2.0, 3.5],
                    size: [8.0, 14.0],
                    color: soil.dust(),
                    scatter: 6.0,
                    ..burst([at[0], at[1], at[2] + 4.0], ParticleKind::Dust, seed)
                });
            }
            if soil.turf && !soil.wet && gallop && unit(seed, 3) < 0.4 {
                sink.emit(&Burst {
                    dir: back,
                    spread: 0.6,
                    speed: [50.0, 110.0],
                    count: 2,
                    life: [1.0, 2.0],
                    size: [1.2, 2.0],
                    color: GRASS,
                    ..burst(at, ParticleKind::Grass, seed ^ 0x99)
                });
            }
        }
        Gait::Boots => {
            // A column's haze: one soft puff for about every third step on dry ground.
            if soil.dusty && unit(seed, 1) < 0.35 {
                sink.emit(&Burst {
                    spread: 1.2,
                    speed: [4.0, 14.0],
                    count: 1,
                    life: [3.0, 5.0],
                    size: [9.0, 16.0],
                    color: soil.dust(),
                    scatter: 4.0,
                    ..burst([at[0], at[1], at[2] + 3.0], ParticleKind::Dust, seed)
                });
            } else if soil.wet && unit(seed, 2) < 0.25 {
                sink.emit(&Burst {
                    dir: back,
                    spread: 0.5,
                    speed: [30.0, 60.0],
                    count: 1,
                    life: [0.6, 1.0],
                    size: [0.5, 1.0],
                    color: soil.color,
                    ..burst(at, ParticleKind::Mud, seed)
                });
            }
        }
        _ => {}
    }
}

/// The soil at world pixel `(x, y)`: its class, wet where the ground state holds mud.
pub fn soil_at(terrain: &TerrainData, state: Option<&GroundState>, x: f32, y: f32) -> Soil {
    let class = ground::class_of(terrain.tile_at(x as i32, y as i32));
    let wet = state.is_some_and(|s| s.texel(x, y)[1] > 90);
    Soil::of_class(class, wet)
}

/// True height of the ground (or the water level) at world pixel `(x, y)`.
fn ground_z(terrain: &TerrainData, x: f32, y: f32) -> f32 {
    terrain.height_at(x as i32, y as i32).max(0) as f32 / COS_E
}

fn on_water(terrain: &TerrainData, x: f32, y: f32) -> bool {
    terrain.water().is_some_and(|sea| water::depth_units(sea, x, y) >= 1.0)
}

impl ParticleView {
    /// One frame: new impacts and footfalls throw particles (`earth`: the detailed ground is
    /// on; `wet`: the detailed water is), then everything moves by `dt` seconds. `shots` are
    /// where balls fell into water this frame (`WaterView::shots`, a splash each); `ring`
    /// starts a water ring for an impact on water and says whether it is new (no splash
    /// twice for one ball).
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        world: &World,
        terrain: &TerrainData,
        state: Option<&GroundState>,
        footfalls: &[Footfall],
        earth: bool,
        wet: bool,
        dt: f32,
        shots: &[[f32; 2]],
        ring: &mut dyn FnMut([f32; 2]) -> bool,
    ) {
        let now = world.tick.0;
        if now < self.tick {
            self.particles.clear();
            self.last_impact = 0;
        }
        self.tick = now;
        let fresh = |i: &&ImpactRecord| i.tick > self.last_impact;
        for (k, i) in world.impacts.iter().filter(fresh).enumerate() {
            let at = [i.pos.0 as f32, i.pos.1 as f32];
            let (dx, dy) = (i.dir.0 as f32, i.dir.1 as f32);
            let len = (dx * dx + dy * dy).sqrt();
            let dir = if len > 0.0 { [dx / len, dy / len] } else { [0.0; 2] };
            // `flight_dir` is 1/64 px per tick; a field gun's ball flies about 12 px per tick.
            let energy = if len > 0.0 { len / 64.0 / 12.0 } else { 1.0 };
            let seed = hash(i.pos.0 as u32 ^ hash(i.pos.1 as u32) ^ hash(i.tick as u32 + k as u32));
            let z = ground_z(terrain, at[0], at[1]);
            if on_water(terrain, at[0], at[1]) {
                if wet && ring(at) {
                    impact(&mut self.particles, i.cause, [at[0], at[1], z], dir, energy, None, seed);
                }
            } else if earth {
                let soil = soil_at(terrain, state, at[0], at[1]);
                impact(&mut self.particles, i.cause, [at[0], at[1], z], dir, energy, Some(soil), seed);
            }
        }
        if let Some(last) = world.impacts.back() {
            self.last_impact = self.last_impact.max(last.tick);
        }
        if wet {
            for (k, &at) in shots.iter().enumerate() {
                let seed = hash(at[0] as u32 ^ hash(at[1] as u32 ^ hash(now as u32 + k as u32)));
                let z = ground_z(terrain, at[0], at[1]);
                impact(&mut self.particles, DeathCause::Cannonball, [at[0], at[1], z], [0.0; 2], 1.0, None, seed);
            }
        }
        if earth {
            for f in footfalls {
                if on_water(terrain, f.at[0], f.at[1]) {
                    continue;
                }
                let soil = soil_at(terrain, state, f.at[0], f.at[1]);
                let seed = hash(f.unit ^ hash(f.tick as u32 ^ (f.at[0] as u32).rotate_left(16)));
                footfall(&mut self.particles, f, [f.at[0], f.at[1], ground_z(terrain, f.at[0], f.at[1])], soil, seed);
            }
        }
        let surface = |x: f32, y: f32| ground_z(terrain, x, y);
        self.particles.step(dt, &surface);
    }

    /// Small marks where clods and mud fell (the ground state of the detailed ground).
    pub fn stamp_landings(&mut self, state: &mut GroundState) {
        for Landing { at, size, kind } in self.particles.take_landings() {
            let r = size.max(metres(0.06));
            let mud = if kind == ParticleKind::Mud { 0.45 } else { 0.08 };
            state.stamp(&Stamp { mud, ..Stamp::print(at, [1.0, 0.0], [r, r], -0.006, 0.18) });
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

    fn count(r: &Record, kind: ParticleKind) -> u32 {
        r.0.iter().filter(|b| b.kind == kind).map(|b| u32::from(b.count)).sum()
    }

    #[test]
    fn a_ball_throws_earth_forward_by_its_energy() {
        let grass = Soil::of_class(0, false);
        let (mut slow, mut fast) = (Record::default(), Record::default());
        impact(&mut slow, DeathCause::Cannonball, [0.0; 3], [1.0, 0.0], 0.5, Some(grass), 1);
        impact(&mut fast, DeathCause::Cannonball, [0.0; 3], [1.0, 0.0], 1.5, Some(grass), 1);
        assert!(count(&fast, ParticleKind::Clod) > count(&slow, ParticleKind::Clod));
        assert!(count(&fast, ParticleKind::Grass) > 0 && count(&fast, ParticleKind::Dust) > 0);
        assert!(fast.0[0].dir[0] > 0.5);
        let mut mud = Record::default();
        impact(&mut mud, DeathCause::Cannonball, [0.0; 3], [1.0, 0.0], 1.0, Some(Soil::of_class(3, false)), 1);
        assert!(count(&mud, ParticleKind::Mud) > 0 && count(&mud, ParticleKind::Clod) == 0);
        let mut water = Record::default();
        impact(&mut water, DeathCause::Cannonball, [0.0; 3], [1.0, 0.0], 1.0, None, 1);
        assert!(count(&water, ParticleKind::Droplet) > 20 && count(&water, ParticleKind::Spray) > 10);
        let mut shell = Record::default();
        impact(&mut shell, DeathCause::Explosion, [0.0; 3], [0.0, 0.0], 1.0, Some(grass), 1);
        assert!(count(&shell, ParticleKind::Clod) >= 150);
    }

    #[test]
    fn hooves_flick_mud_only_on_wet_ground() {
        let f = Footfall { gait: Gait::Hooves, at: [0.0; 2], along: [1.0, 0.0], speed: 4.0, unit: 3, tick: 10 };
        let (mut wet, mut dry) = (Record::default(), Record::default());
        footfall(&mut wet, &f, [0.0; 3], Soil::of_class(0, true), 5);
        footfall(&mut dry, &f, [0.0; 3], Soil::of_class(0, false), 5);
        assert!(count(&wet, ParticleKind::Mud) > 0);
        assert_eq!(count(&dry, ParticleKind::Mud), 0);
        // Flicked back, against the way it goes.
        assert!(wet.0[0].dir[0] < 0.0);
    }
}
