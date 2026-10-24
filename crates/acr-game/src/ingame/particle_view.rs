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
use acr_sim::body::{Outcome, Torn};
use acr_sim::combat::{DeathCause, HitRecord, ImpactRecord};

use super::ground_marks::{Footfall, Gait};

/// Soil colour (sRGB) of each material class (`ground::CLASSES` order), for clods and dust.
const SOIL: [[u8; 3]; 15] = [
    [96, 76, 52],    // grass: the soil under the turf
    [100, 80, 54],   // meadow
    [118, 94, 66],   // dirt
    [64, 50, 36],    // mud
    [178, 152, 112], // sand
    [122, 116, 106], // rock
    [74, 58, 42],    // forest
    [232, 234, 240], // snow
    [110, 86, 58],   // field
    [150, 124, 92],  // cracked
    [126, 76, 44],   // red
    [128, 120, 106], // gravel
    [176, 150, 108], // desert
    [118, 100, 66],  // steppe
    [226, 228, 232], // snowy
];
const OTHER_SOIL: [u8; 3] = [104, 86, 64];
const GRASS: [u8; 3] = [78, 104, 44];
const WATER_SPRAY: [u8; 3] = [226, 234, 238];
const WATER_DROP: [u8; 3] = [196, 212, 222];
/// Blood: the mist of a body torn by a ball, its drops, flesh and bone scraps (#262).
const BLOOD_MIST: [u8; 3] = [150, 14, 12];
const BLOOD_DROP: [u8; 3] = [118, 6, 6];
const FLESH: [u8; 3] = [132, 28, 24];
const BONE: [u8; 3] = [214, 204, 182];
/// True height (world px) of a man's middle, where a ball tears him (0.9 m), and of his
/// legs (0.45 m).
const MIDDLE: f32 = 27.0;
const KNEES: f32 = 13.5;

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
            dusty: !wet
                && matches!(name, "dirt" | "sand" | "field" | "cracked" | "red" | "gravel" | "desert" | "steppe"),
            turf: matches!(name, "grass" | "meadow" | "field" | "steppe"),
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
    /// The tick of the last hits shredded and the victims done at it (a scripted hit can
    /// come between two frames of one tick).
    last_hit: u64,
    hits_done: Vec<u32>,
    /// Shell fragments up to this tick have been thrown (`World::fragments`).
    last_fragment: u64,
    /// The tick of the last update: going back means another game.
    tick: u64,
}

/// The brown-grey of a cloud that hangs after a strike: the dust, greyer and darker.
fn haze(soil: Soil) -> [u8; 3] {
    soil.dust().map(|c| ((u16::from(c) + 120) * 2 / 5) as u8)
}

/// Torn sods on dry turf, else `earth`.
fn sod_or(soil: Soil, earth: ParticleKind) -> ParticleKind {
    if soil.turf && !soil.wet { ParticleKind::Sod } else { earth }
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
            // A 6-12 pdr ball grazing turf throws a sheet of earth 2-4 m up and forward: a fan
            // of mid-size clods, torn sods, crumbs, a quick dust burst and a cloud that hangs.
            let s = e.sqrt();
            let [dx, dy] = dir;
            for (k, yaw) in [-0.55f32, -0.27, 0.0, 0.27, 0.55].into_iter().enumerate() {
                let (sy, cy) = yaw.sin_cos();
                let h = [dx * cy - dy * sy, dx * sy + dy * cy];
                // The sides of the fan fly lower and shorter than its middle.
                let side = 1.0 - yaw.abs() * 0.5;
                sink.emit(&Burst {
                    dir: [h[0] * 0.8, h[1] * 0.8, side],
                    spread: 0.2,
                    speed: [160.0 * s * side, 320.0 * s * side],
                    count: (8.0 + 10.0 * e) as u16,
                    life: [2.5, 4.5],
                    size: [1.6, 3.4],
                    color: soil.color,
                    scatter: 7.0,
                    ..burst(at, earth, seed ^ (k as u32).wrapping_mul(0x2545_f491))
                });
            }
            // Torn sods (20-40 cm) on turf, big lumps elsewhere.
            sink.emit(&Burst {
                dir: fwd(0.9),
                spread: 0.4,
                speed: [120.0 * s, 250.0 * s],
                count: (3.0 + 4.0 * e) as u16,
                life: [3.0, 5.0],
                size: [3.2, 6.0],
                color: soil.color,
                scatter: 6.0,
                ..burst(at, sod_or(soil, earth), seed ^ 0x50d)
            });
            // Crumbs, higher up.
            sink.emit(&Burst {
                dir: fwd(0.35),
                spread: 0.4,
                speed: [180.0 * s, 360.0 * s],
                count: (14.0 * e) as u16 + 4,
                life: [2.0, 4.0],
                size: [0.6, 1.3],
                color: soil.color,
                scatter: 5.0,
                ..burst(at, earth, seed ^ 0x77)
            });
            if soil.turf {
                sink.emit(&Burst {
                    dir: fwd(0.6),
                    spread: 0.7,
                    speed: [80.0, 220.0 * s],
                    count: (14.0 * e) as u16 + 2,
                    life: [2.5, 4.0],
                    size: [1.6, 2.6],
                    color: GRASS,
                    scatter: 6.0,
                    ..burst(at, ParticleKind::Grass, seed ^ 0x99)
                });
            }
            let wet = if soil.wet { 0.5 } else { 1.0 };
            // The burst of dust thrown with the sheet.
            sink.emit(&Burst {
                dir: fwd(0.8),
                spread: 0.6,
                speed: [60.0, 170.0 * s],
                count: ((8.0 + 6.0 * e) * wet) as u16,
                life: [1.8, 3.2],
                size: [8.0, 15.0],
                color: soil.dust(),
                scatter: 8.0,
                ..burst([at[0], at[1], at[2] + 6.0], ParticleKind::Dust, seed ^ 0x1234)
            });
            // A short brown-grey cloud that hangs over the strike.
            sink.emit(&Burst {
                dir: fwd(0.4),
                spread: 1.1,
                speed: [10.0, 45.0],
                count: ((4.0 + 3.0 * e) * wet) as u16,
                life: [5.0, 8.0],
                size: [14.0, 24.0],
                color: haze(soil),
                scatter: 16.0,
                ..burst([at[0], at[1], at[2] + 10.0], ParticleKind::Dust, seed ^ 0x4321)
            });
        }
        DeathCause::Explosion => {
            // A ring of earth: a wide cone up, the heavy clods low and far, sods among them.
            sink.emit(&Burst {
                spread: 1.25,
                speed: [140.0, 360.0],
                count: 130,
                life: [2.0, 4.0],
                size: [1.0, 4.0],
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
                spread: 1.0,
                speed: [150.0, 300.0],
                count: 10,
                life: [3.0, 5.0],
                size: [3.0, 6.0],
                color: soil.color,
                scatter: 12.0,
                ..burst(at, sod_or(soil, earth), seed ^ 0x50d)
            });
            let dark = soil.dust().map(|c| (u16::from(c) * 3 / 10 + 8) as u8);
            // The dark column of earth and smoke straight up from the burst.
            sink.emit(&Burst {
                spread: 0.16,
                speed: [140.0, 400.0],
                count: 26,
                life: [3.0, 5.0],
                size: [9.0, 18.0],
                color: dark,
                scatter: 6.0,
                ..burst([at[0], at[1], at[2] + 8.0], ParticleKind::Dust, seed ^ 0x1234)
            });
            // The ring rolling out along the ground.
            sink.emit(&Burst {
                spread: 1.5,
                speed: [110.0, 220.0],
                count: 24,
                life: [2.5, 4.5],
                size: [12.0, 22.0],
                color: soil.dust().map(|c| (u16::from(c) * 3 / 4) as u8),
                scatter: 12.0,
                ..burst([at[0], at[1], at[2] + 5.0], ParticleKind::Dust, seed ^ 0x2468)
            });
            // What hangs after it.
            sink.emit(&Burst {
                spread: 1.2,
                speed: [10.0, 40.0],
                count: 8,
                life: [6.0, 9.0],
                size: [20.0, 32.0],
                color: haze(soil),
                scatter: 26.0,
                ..burst([at[0], at[1], at[2] + 14.0], ParticleKind::Dust, seed ^ 0x4321)
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

/// How hard a hit tears a body (#262, `docs/damage-model.md` "Mist"): 2 pulverized, 1 torn
/// in two or a limb or the head torn off by a ball, 0 nothing to spray.
pub fn shred_level(hit: &HitRecord) -> u8 {
    let Some(injury) = hit.injury else { return 0 };
    let ball = matches!(hit.cause, DeathCause::Cannonball | DeathCause::Explosion);
    match injury.outcome {
        Outcome::Torn(Torn::Pulverized) => 2,
        Outcome::Torn(Torn::InTwo { .. }) => 1,
        Outcome::Off | Outcome::Torn(Torn::Headless) if ball && injury.energy >= acr_sim::body::BALL_SEVERS => 1,
        _ => 0,
    }
}

/// The burst of a body torn by a ball at `at` (the ground under the victim, true height in
/// z) along `dir` (unit, horizontal): a red mist that hangs and drifts, blood drops flung
/// along the flight, and scraps of flesh and bone that tumble and lie. `level` from
/// [`shred_level`]; `low`: the ball took the legs.
pub fn shred(sink: &mut dyn ParticleSink, at: [f32; 3], dir: [f32; 2], level: u8, low: bool, seed: u32) {
    if level == 0 {
        return;
    }
    let k = f32::from(level);
    let at = [at[0], at[1], at[2] + if low { KNEES } else { MIDDLE }];
    let fwd = |up: f32| [dir[0], dir[1], up];
    // The mist: a cloud thrown forward and all around, slow to fall.
    sink.emit(&Burst {
        dir: fwd(0.6),
        spread: 1.3,
        speed: [30.0, 140.0 * k],
        count: (30.0 * k) as u16,
        life: [0.8, 1.8],
        size: [4.0, 9.0 + 3.0 * k],
        color: BLOOD_MIST,
        scatter: 6.0,
        ..burst(at, ParticleKind::Spray, seed)
    });
    // Drops flung far along the flight.
    sink.emit(&Burst {
        dir: fwd(0.35),
        spread: 0.8,
        speed: [90.0, 260.0 + 80.0 * k],
        count: (45.0 * k) as u16,
        life: [1.5, 2.5],
        size: [0.6, 1.6],
        color: BLOOD_DROP,
        scatter: 4.0,
        ..burst(at, ParticleKind::Droplet, seed ^ 0x0b1d)
    });
    // Scraps of flesh, and a few of bone.
    sink.emit(&Burst {
        dir: fwd(0.5),
        spread: 1.0,
        speed: [60.0, 200.0 + 60.0 * k],
        count: (16.0 * k) as u16,
        life: [2.0, 3.0],
        size: [1.0, 3.0],
        color: FLESH,
        scatter: 5.0,
        ..burst(at, ParticleKind::Gore, seed ^ 0x9e57)
    });
    sink.emit(&Burst {
        dir: fwd(0.5),
        spread: 1.0,
        speed: [60.0, 220.0],
        count: (5.0 * k) as u16,
        life: [2.0, 3.0],
        size: [0.6, 1.6],
        color: BONE,
        scatter: 4.0,
        ..burst(at, ParticleKind::Gore, seed ^ 0xb0e5)
    });
}

/// Wood and plaster of a building's wall, for what a round knocks off it (#226).
const WOOD: [u8; 3] = [150, 112, 70];
const WALL_DUST: [u8; 3] = [170, 156, 134];
const STONE: [u8; 3] = [128, 120, 108];

/// What a round of `cause` striking a building's wall at `at` (true height in z) throws,
/// flying along `dir` (unit, horizontal): splinters and chips knocked back out of the wall
/// towards the shooter and a cloud of dust off the wall (#226). Wood and dust, not gore:
/// shown with every gore setting.
pub fn wall_hit(sink: &mut dyn ParticleSink, cause: DeathCause, at: [f32; 3], dir: [f32; 2], seed: u32) {
    let k = match cause {
        DeathCause::Cannonball => 1.0,
        DeathCause::Explosion => 0.8,
        DeathCause::Grapeshot => 0.45,
        DeathCause::Gunshot => 0.2,
        DeathCause::Arrow => 0.1,
        _ => return,
    };
    let back = |up: f32| [-dir[0], -dir[1], up];
    // Splinters: thrown back out of the hole, tumbling down to lie at the wall's foot.
    sink.emit(&Burst {
        dir: back(0.45),
        spread: 0.9,
        speed: [60.0, 120.0 + 180.0 * k],
        count: 2 + (22.0 * k) as u16,
        life: [1.6, 2.6],
        size: [1.0, 2.0 + 3.0 * k],
        color: WOOD,
        scatter: 2.0 + 4.0 * k,
        ..burst(at, ParticleKind::Splinter, seed)
    });
    // Chips of stone and plaster from a ball or a shell.
    if k >= 0.8 {
        sink.emit(&Burst {
            dir: back(0.3),
            spread: 0.8,
            speed: [80.0, 260.0],
            count: (10.0 * k) as u16,
            life: [2.0, 3.0],
            size: [1.2, 2.8],
            color: STONE,
            scatter: 5.0,
            ..burst(at, ParticleKind::Clod, seed ^ 0xc41b)
        });
    }
    // Dust puffed off the wall, drifting and sinking.
    sink.emit(&Burst {
        dir: back(0.25),
        spread: 1.2,
        speed: [15.0, 40.0 + 70.0 * k],
        count: 2 + (12.0 * k) as u16,
        life: [1.4, 2.8 + k],
        size: [5.0, 8.0 + 12.0 * k],
        color: WALL_DUST,
        scatter: 3.0 + 8.0 * k,
        ..burst(at, ParticleKind::Dust, seed ^ 0xd057)
    });
}

/// The blood of a melee blow or a thrown weapon (a spurt along the blow, from the chest) or
/// of a shell burst that did not tear the body (drops all around) at the victim standing on
/// `at` (true height in z), `dir` the blow's direction (unit, may be zero) (#226).
pub fn spurt(sink: &mut dyn ParticleSink, cause: DeathCause, at: [f32; 3], dir: [f32; 2], killed: bool, seed: u32) {
    let (count, spread, up) = match cause {
        DeathCause::Melee => (8.0, 0.8, 0.35),
        DeathCause::Thrown => (6.0, 0.6, 0.3),
        // Up and all around.
        DeathCause::Explosion => (14.0, 1.35, 0.0),
        _ => return,
    };
    let dir = if cause == DeathCause::Explosion { [0.0, 0.0, 1.0] } else { [dir[0], dir[1], up] };
    let k = if killed { 1.6 } else { 1.0 };
    let at = [at[0], at[1], at[2] + 36.0];
    sink.emit(&Burst {
        dir,
        spread,
        speed: [30.0, 90.0 + 30.0 * k],
        count: (count * k) as u16,
        life: [0.6, 1.2],
        size: [0.5, 1.3],
        color: BLOOD_DROP,
        scatter: 3.0,
        ..burst(at, ParticleKind::Droplet, seed)
    });
}

/// Seconds a shell fragment's streak flies: it reaches its end and is gone.
const FRAGMENT_FLIGHT: f32 = 0.18;

/// A shell fragment's streak: a few glowing specks one behind the other along its flight
/// from the burst (about knee to chest high) to where it struck or fell, bigger for a
/// bigger piece. They vanish at the end of the flight instead of landing.
fn fragment(sink: &mut dyn ParticleSink, f: &acr_sim::shrapnel::Fragment, ground: f32, blood: bool, seed: u32) {
    let (dx, dy) = ((f.to.0 - f.from.0) as f32, (f.to.1 - f.from.1) as f32);
    let len = (dx * dx + dy * dy).sqrt().max(1.0);
    use acr_sim::shrapnel::Kind;
    // Earth and bone fly up from the ground and fall back; a casing's fragment flies flat.
    let rise = match (f.kind, f.victim.is_some()) {
        (_, true) => 6.0,
        (Kind::Casing, false) => -24.0,
        (_, false) => 10.0,
    };
    let dir = [dx / len, dy / len, rise / len];
    let size = match f.kind {
        Kind::Casing => 1.5 + 2.5 * f32::from(f.mass) / 100.0,
        // A clod or stone, a splinter of bone: bigger than its mass for a casing's.
        Kind::Earth | Kind::Bone => 2.0 + 6.0 * f32::from(f.mass) / 100.0,
    };
    let speed = len / FRAGMENT_FLIGHT;
    let specks = 3 + u32::from(f.mass) / 34;
    for i in 0..specks {
        // Each speck starts a little behind the one before: a short streak.
        let back = i as f32 * 4.0;
        let lift = if f.kind == Kind::Casing { 22.0 } else { 6.0 };
        let at = [f.from.0 as f32 - dir[0] * back, f.from.1 as f32 - dir[1] * back, ground + lift];
        sink.emit(&Burst {
            at,
            dir,
            spread: 0.0,
            speed: [speed, speed],
            count: 1,
            kind: ParticleKind::Splinter,
            life: [FRAGMENT_FLIGHT + back / speed; 2],
            size: [size * (1.0 - 0.15 * i as f32); 2],
            color: match f.kind {
                Kind::Casing => [255, 196 - 30 * i.min(3) as u8, 90],
                Kind::Earth => [92, 70, 48],
                Kind::Bone => [226, 214, 190],
            },
            scatter: 0.0,
            seed: seed.wrapping_add(i),
        });
    }
    // Where it struck a man: a spurt of blood, and scraps of flesh from a big piece (the
    // Gore particles of `shred`, full gore only).
    if blood && f.victim.is_some() {
        let at = [f.to.0 as f32, f.to.1 as f32, ground + 30.0];
        let k = f32::from(f.mass) / 100.0;
        sink.emit(&Burst {
            dir: [dir[0], dir[1], 0.4],
            spread: 0.7,
            speed: [40.0, 90.0 + 120.0 * k],
            count: 3 + (10.0 * k) as u16,
            life: [0.6, 1.2],
            size: [0.5, 1.2],
            color: BLOOD_DROP,
            scatter: 2.0,
            ..burst(at, ParticleKind::Droplet, seed ^ 0xf7a9)
        });
        if f.mass > 40 {
            sink.emit(&Burst {
                dir: [dir[0], dir[1], 0.5],
                spread: 0.8,
                speed: [50.0, 160.0],
                count: (6.0 * k) as u16,
                life: [2.0, 3.0],
                size: [0.8, 2.0],
                color: FLESH,
                scatter: 3.0,
                ..burst(at, ParticleKind::Gore, seed ^ 0x6a5e)
            });
        }
    }
}

impl ParticleView {
    /// One frame: new impacts and footfalls throw particles (`earth`: the detailed ground is
    /// on; `wet`: the detailed water is), then everything moves by `dt` seconds. `shots` are
    /// where balls fell into water this frame (`WaterView::shots`, a splash each); `ring`
    /// starts a water ring for an impact on water and says whether it is new (no splash
    /// twice for one ball). `blood`: bodies torn by balls burst into mist, drops and scraps
    /// ([`shred`], the full gore setting), melee blows spurt ([`spurt`]); rounds striking
    /// a building knock splinters and dust off the wall where they struck ([`wall_hit`]).
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        world: &World,
        blood: bool,
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
            self.last_hit = 0;
            self.hits_done.clear();
            self.last_fragment = 0;
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
        {
            let since = self.last_hit;
            for hit in world.hits.iter().filter(|h| h.tick >= since) {
                if hit.tick > self.last_hit {
                    self.last_hit = hit.tick;
                    self.hits_done.clear();
                }
                if self.hits_done.contains(&hit.victim) {
                    continue;
                }
                self.hits_done.push(hit.victim);
                let at = [hit.pos.0 as f32, hit.pos.1 as f32];
                let (dx, dy) = (hit.dir.0 as f32, hit.dir.1 as f32);
                let len = (dx * dx + dy * dy).sqrt();
                let dir = if len > 0.0 { [dx / len, dy / len] } else { [0.0; 2] };
                let seed = hash(hit.victim ^ hash(hit.tick as u32 ^ 0x5eed));
                if hit.building {
                    // Where the round struck (`World::hit_at`), between waist and eaves height.
                    let z = ground_z(terrain, at[0], at[1]) + 30.0 + 45.0 * unit(seed, 1);
                    wall_hit(&mut self.particles, hit.cause, [at[0], at[1], z], dir, seed);
                    continue;
                }
                if !blood {
                    continue;
                }
                let ground = [at[0], at[1], ground_z(terrain, at[0], at[1])];
                let level = shred_level(hit);
                if level == 0 {
                    spurt(&mut self.particles, hit.cause, ground, dir, hit.killed, seed);
                    continue;
                }
                let low = hit.injury.is_some_and(|i| i.level == acr_sim::body::BALL_HIPS);
                shred(&mut self.particles, ground, dir, level, low, seed);
            }
        }
        for (k, f) in world.fragments.iter().filter(|f| f.tick > self.last_fragment).enumerate() {
            let z = ground_z(terrain, f.from.0 as f32, f.from.1 as f32);
            let seed = hash(f.from.0 as u32 ^ hash(f.to.1 as u32) ^ hash(f.tick as u32 + k as u32));
            fragment(&mut self.particles, f, z, blood, seed);
        }
        if let Some(last) = world.fragments.back() {
            self.last_fragment = self.last_fragment.max(last.tick);
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
        assert!(count(&fast, ParticleKind::Sod) > 0, "turf tears sods");
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
    fn a_torn_body_bursts_into_mist_drops_and_scraps() {
        let (mut pulp, mut half, mut none) = (Record::default(), Record::default(), Record::default());
        shred(&mut pulp, [0.0; 3], [1.0, 0.0], 2, false, 1);
        shred(&mut half, [0.0; 3], [1.0, 0.0], 1, true, 1);
        shred(&mut none, [0.0; 3], [1.0, 0.0], 0, false, 1);
        assert!(none.0.is_empty());
        for kind in [ParticleKind::Spray, ParticleKind::Droplet, ParticleKind::Gore] {
            assert!(count(&pulp, kind) > count(&half, kind) && count(&half, kind) > 0, "{kind:?}");
        }
        assert!(pulp.0.iter().all(|b| b.dir[0] > 0.5 && b.at[2] > half.0[0].at[2]));
        assert!(pulp.0.iter().any(|b| b.color == BLOOD_MIST) && pulp.0.iter().any(|b| b.color == BONE));
    }

    #[test]
    fn a_ball_knocks_splinters_and_dust_back_off_a_wall() {
        let mut ball = Record::default();
        wall_hit(&mut ball, DeathCause::Cannonball, [0.0, 0.0, 50.0], [1.0, 0.0], 3);
        let mut shot = Record::default();
        wall_hit(&mut shot, DeathCause::Gunshot, [0.0, 0.0, 50.0], [1.0, 0.0], 3);
        for kind in [ParticleKind::Splinter, ParticleKind::Dust] {
            assert!(count(&ball, kind) > count(&shot, kind) && count(&shot, kind) > 0, "{kind:?}");
        }
        // Back towards the shooter (the round flew east).
        assert!(ball.0.iter().all(|b| b.dir[0] < 0.0));
        let mut none = Record::default();
        wall_hit(&mut none, DeathCause::Melee, [0.0; 3], [1.0, 0.0], 3);
        assert!(none.0.is_empty());
    }

    #[test]
    fn blows_spurt_along_and_bursts_all_around() {
        let mut blow = Record::default();
        spurt(&mut blow, DeathCause::Melee, [0.0; 3], [0.0, 1.0], false, 1);
        assert!(count(&blow, ParticleKind::Droplet) > 0 && blow.0[0].dir[1] > 0.5);
        let mut kill = Record::default();
        spurt(&mut kill, DeathCause::Melee, [0.0; 3], [0.0, 1.0], true, 1);
        assert!(count(&kill, ParticleKind::Droplet) > count(&blow, ParticleKind::Droplet));
        let mut shell = Record::default();
        spurt(&mut shell, DeathCause::Explosion, [0.0; 3], [0.0, 1.0], false, 1);
        assert!(shell.0[0].dir == [0.0, 0.0, 1.0] && shell.0[0].spread > 1.0);
        let mut musket = Record::default();
        spurt(&mut musket, DeathCause::Gunshot, [0.0; 3], [0.0, 1.0], false, 1);
        assert!(musket.0.is_empty());
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
