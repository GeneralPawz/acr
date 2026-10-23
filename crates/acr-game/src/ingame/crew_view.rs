//! Gun crews (#287, docs/rendering-3d.md "Gun crew"): the sim keeps a gun one unit with one
//! life, so its crew's losses are read from that life. With `k = ceil(4 · life / max_life)`
//! men standing, a man falls each time `k` drops (firer, loader, rammer, layer in that order):
//! his bones are collapsed in the gun's pose, and he is drawn on his own (the gun model posed
//! in its `DEATH` clip with only him shown) where he stood, as a corpse that bleeds a pool
//! and rots like any other; the men still standing when the gun is destroyed become such
//! corpses too. Uniforms follow the gun owner's nation. Render only.

use std::collections::HashMap;

use acr_render::math::{self, Mat4};
use acr_render::model::{self, CREW, ModelData, Palette};
use acr_render::model_renderer::{BloodPool, ModelInstance, Wounds};
use acr_render::view3d::PIXELS_PER_METRE;
use acr_sim::World;
use acr_sim::catalog::ObjectType;
use acr_sim::world::{AnimKind, FRM_DEC, Unit};

use crate::ui::settings::Gore;

/// The order the men fall in: the firer, the loader, the rammer, the layer (who stays
/// longest: he serves the vent and can fire).
pub const FALL_ORDER: [u8; CREW] = [3, 1, 0, 2];
/// The bit of the gun itself in a shown mask ([`model::Crew::mask`]).
const GUN: u8 = 1 << CREW;
/// Where each man stands in battery (glTF metres: x, up, z = −Blender y), `guncrew.HOME`.
const HOME: [[f32; 3]; CREW] = [[1.25, 0.0, 0.2], [-1.25, 0.0, 0.2], [0.0, 0.0, -3.15], [1.3, 0.0, -0.9]];

/// Men standing at `life` of `max_life`: `ceil(4 · life / max_life)`, 0..=4.
pub fn standing(life: i32, max_life: i32) -> usize {
    if max_life <= 0 {
        return CREW;
    }
    let life = i64::from(life.clamp(0, max_life));
    ((CREW as i64 * life + i64::from(max_life) - 1) / i64::from(max_life)) as usize
}

/// The men lost first when `lost` have fallen.
fn fallen(lost: usize) -> &'static [u8] {
    &FALL_ORDER[..lost.min(CREW)]
}

/// The shown mask of a gun that lost `lost` men: the gun and the men left.
pub fn shown(lost: usize) -> u8 {
    fallen(lost).iter().fold(GUN | ((1 << CREW) - 1), |m, &f| m & !(1 << f))
}

/// The uniform of a nation's artillery (linear, as the model's palette): coat, waistcoat.
/// The prefixes are the nations' unit names (`EngM18`, `FrnKf18`, ...).
pub fn uniform(prefix: &str) -> Option<[[f32; 3]; 2]> {
    Some(match prefix {
        // Royal Artillery: dark blue faced red, red waistcoat.
        "Eng" => [[0.085, 0.10, 0.24], [0.70, 0.11, 0.07]],
        // Corps royal de l'artillerie: bleu de roi throughout (red only at the cuffs).
        "Frn" => [[0.07, 0.11, 0.38], [0.09, 0.13, 0.40]],
        // Real Cuerpo de Artillería: blue faced red, red waistcoat, a lighter blue.
        "Spn" => [[0.13, 0.17, 0.36], [0.62, 0.08, 0.06]],
        // Continental artillery: blue faced red, buff waistcoat.
        "Usa" => [[0.075, 0.09, 0.21], [0.76, 0.64, 0.40]],
        _ => return None,
    })
}

/// The nation of a gun type: the catalog keeps one `ArtPus` per nation, `ArtPus(EN)`,
/// `(FR)`, `(SP)`, `(US)`.
pub fn nation_of_type(id: &str) -> Option<&'static str> {
    let code = id.rsplit_once('(')?.1.strip_suffix(')')?;
    Some(match code.to_ascii_uppercase().as_str() {
        "EN" => "Eng",
        "FR" => "Frn",
        "SP" => "Spn",
        "US" => "Usa",
        _ => return None,
    })
}

/// The nation prefix of player `owner`: the first of his units with a European nation's name.
fn nation(world: &World, owner: u8) -> Option<&'static str> {
    const NATIONS: [&str; 4] = ["Eng", "Frn", "Spn", "Usa"];
    world.units.iter().filter(|u| u.owner == owner && u.alive).find_map(|u| {
        let id = &world.catalog.get(u.kind).id;
        NATIONS.iter().copied().find(|n| id.len() > 3 && id[..3].eq_ignore_ascii_case(n))
    })
}

/// A man fallen from a gun's crew.
#[derive(Debug, Clone, Copy)]
struct Corpse {
    man: u8,
    tick: u64,
    model: usize,
    transform: Mat4,
    player: u8,
    uniform: Option<[[f32; 3]; 2]>,
    seed: f32,
}

/// The crews' state between frames.
#[derive(Debug, Default)]
pub struct Crews {
    /// Men lost per gun (unit id).
    lost: HashMap<u32, usize>,
    corpses: Vec<Corpse>,
    /// Nation prefix per player, refreshed every 256 ticks.
    nations: HashMap<u8, Option<&'static str>>,
    nations_tick: u64,
    /// The palettes of this frame's gun and corpse instances (instance index → palette).
    pub poses: HashMap<usize, Palette>,
    /// The yaw each gun (unit id) is drawn at, following its facing smoothly.
    yaw: HashMap<u32, f32>,
}

/// A cannon's turn step: the sim turns it a 16th (16 of 256) per `#MOTION_L1`/`#MOTION_R1`
/// cycle (`artillery::complex_rotate`, 0x50f7cb).
const TURN_STEP: f32 = 16.0;

/// The facing (in 256ths, fractional) gun `u` shows in its turn animation: the sim adds the
/// 16th at the end of `#MOTION_L1` and takes it off at the start of `#MOTION_R1`, so the
/// drawn facing moves through the step with the animation's frame instead.
pub fn turning_dir(u: &Unit) -> f32 {
    turning(u.dir, u.anim, u.frame, u.anim_frames)
}

fn turning(dir: u8, anim: AnimKind, frame: i32, frames: i32) -> f32 {
    let p = (frame.max(0) as f32 / frames.max(1) as f32).clamp(0.0, 1.0);
    let step = match anim {
        AnimKind::MotionLs(1) => TURN_STEP * p,
        AnimKind::MotionRs(1) => TURN_STEP * (1.0 - p),
        _ => 0.0,
    };
    f32::from(dir) + step
}

/// The gun's real turning rate (radians per second): a 16th per turn cycle of `t`, whose
/// frames advance [`FRM_DEC`] per tick.
pub fn turn_rate(t: &ObjectType) -> f32 {
    let frames = t.anims.get("#MOTION_L1").map_or(20.0, |a| f32::from(a.frames).max(2.0));
    let cycle = frames / (FRM_DEC as f32 * acr_sim::ORIGINAL_TICKS_PER_SECOND as f32);
    TURN_STEP / 256.0 * std::f32::consts::TAU / cycle
}

/// The clip and time a crewed gun shows in the trail's lift and drop around a turn
/// (`#PATTACK1`/`#UATTACK1`, 71 frames, the sprites' `ArtPus_Pdg_pvrt`): the model has no
/// clip of its own for them, so instead of standing still the crew works the trail in the
/// `MOTION` clip. `None` in every other animation.
pub fn trail_clip(model: &ModelData, u: &Unit) -> Option<(usize, f32)> {
    let name = match u.anim {
        AnimKind::PAttack(1) => "MOTION_L",
        AnimKind::UAttack(1) => "MOTION_R",
        _ => return None,
    };
    let c = model.clip(name)?;
    let duration = model.clips[c].duration.max(1e-3);
    Some((c, (u.frame.max(0) as f32 / model.fps) % duration))
}

fn wrap(a: f32) -> f32 {
    (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

impl Crews {
    /// Starts a frame: forgets the last frame's palettes (and everything on a restart).
    pub fn begin(&mut self, now: u64) {
        self.poses.clear();
        if now < self.nations_tick {
            self.lost.clear();
            self.corpses.clear();
            self.nations.clear();
            self.yaw.clear();
        }
        if now >= self.nations_tick + 256 || now < self.nations_tick {
            self.nations.clear();
            self.nations_tick = now;
        }
    }

    /// The yaw gun `u` is drawn at this frame (`seconds` long): its facing through the turn
    /// animation ([`turning_dir`]), followed at up to twice its turning rate, so the model
    /// turns smoothly where the sim's facing steps (the sprite's 16 directions).
    pub fn gun_yaw(&mut self, u: &Unit, t: &ObjectType, seconds: f32) -> f32 {
        let want = -(turning_dir(u) - 64.0) * std::f32::consts::TAU / 256.0;
        if !u.alive || u.death > 0 {
            self.yaw.remove(&u.id.0);
            return want;
        }
        let now = *self.yaw.entry(u.id.0).or_insert(want);
        let max = 2.0 * turn_rate(t) * seconds.clamp(0.0, 0.1);
        let next = now + wrap(want - now).clamp(-max, max);
        self.yaw.insert(u.id.0, next);
        next
    }

    /// The uniform of gun `u`: its type's nation, else its owner's.
    pub fn uniform(&mut self, world: &World, u: &Unit) -> Option<[[f32; 3]; 2]> {
        if let Some(n) = nation_of_type(&world.catalog.get(u.kind).id) {
            return uniform(n);
        }
        let n = *self.nations.entry(u.owner).or_insert_with(|| nation(world, u.owner));
        uniform(n?)
    }

    /// The gun `u` drawn as `inst` (instance `index`): records new losses and masks the
    /// fallen out of its pose.
    pub fn gun(&mut self, u: &Unit, model: &ModelData, index: usize, inst: &ModelInstance, now: u64) {
        let Some(crew) = model.crew.as_ref() else { return };
        let lost = self.lost.entry(u.id.0).or_insert(0);
        // The men left fall with a destroyed gun: corpses of their own too, so they bleed
        // and rot and stay where they fell (the gun's `DEATH` hides them).
        let k = if u.alive { standing(u.life, u.max_life) } else { 0 };
        if u.alive && k == CREW {
            // Unhurt (or a new gun on this id).
            *lost = 0;
        }
        for &man in &FALL_ORDER[(*lost).min(CREW)..CREW - k.min(CREW)] {
            self.corpses.push(Corpse {
                man,
                tick: now,
                model: inst.model,
                transform: inst.transform,
                player: inst.player,
                uniform: inst.uniform,
                seed: ((u.id.0.wrapping_mul(2_654_435_761) >> 8) % 1000) as f32 / 1000.0 + f32::from(man) * 0.13,
            });
        }
        *lost = (*lost).max(CREW - k.min(CREW));
        if *lost > 0 {
            let mut palette = model.pose(inst.clip, inst.time);
            crew.mask(&mut palette, shown(*lost));
            self.poses.insert(index, palette);
        }
    }

    /// The fallen men as corpse instances (appended to `out`, their palettes kept) and their
    /// blood pools; corpses older than `corpse_ticks` are dropped.
    pub fn corpses<'a>(
        &mut self,
        models: impl Fn(usize) -> Option<&'a ModelData>,
        now: u64,
        corpse_ticks: u64,
        gore: Gore,
        out: &mut Vec<ModelInstance>,
        pools: &mut Vec<BloodPool>,
    ) {
        self.corpses.retain(|c| now.saturating_sub(c.tick) < corpse_ticks);
        self.lost.retain(|_, l| *l > 0);
        for c in &self.corpses {
            let Some(model) = models(c.model) else { continue };
            let Some(crew) = model.crew.as_ref() else { continue };
            let age = now.saturating_sub(c.tick);
            let secs = age as f32 / acr_sim::ORIGINAL_TICKS_PER_SECOND as f32;
            let death = model.clip("DEATH");
            let (clip, time, lying) = match death.map(|d| (d, model.clips[d].duration)) {
                Some((d, len)) if secs < len => (Some(d), secs, false),
                _ => match model.clip("DEATHLIE1") {
                    Some(l) => (Some(l), 0.0, true),
                    None => (death, death.map_or(0.0, |d| model.clips[d].duration), true),
                },
            };
            // Two counter steps per tick, as the 3D corpses age (three_d.rs).
            let counter = (2 * age).min(u64::from(u16::MAX)) as u16;
            let removed = (2 * corpse_ticks).min(u64::from(u16::MAX)) as u16;
            let decay = if gore == Gore::Off { 0.0 } else { model::corpse_decay(counter, removed) };
            let (morphs, _) = model::decay_pose(decay, lying);
            let mut palette = model.pose(clip, time);
            crew.mask(&mut palette, 1 << c.man);
            self.poses.insert(out.len(), palette);
            out.push(ModelInstance {
                model: c.model,
                clip,
                time,
                transform: c.transform,
                decay,
                morphs,
                transparent: gore == Gore::Off,
                player: c.player,
                wounds: Wounds {
                    blood: if gore == Gore::Off { 0.0 } else { 0.7 },
                    dirt: 0.35,
                    seed: c.seed,
                    ..Default::default()
                },
                sprayed: Default::default(),
                damage: Default::default(),
                look: None,
                uniform: c.uniform,
            });
            if gore != Gore::Off {
                let d = f32::from(counter);
                let at = math::transform_point(&c.transform, HOME[usize::from(c.man)]);
                pools.push(BloodPool {
                    centre: at,
                    radius: 0.5 * PIXELS_PER_METRE * (0.35 + 0.65 * model::ramp(d, 5.0, 140.0)),
                    age: model::ramp(d, 120.0, 900.0),
                    fade: 1.0 - model::ramp(d, f32::from(removed) * 0.85, f32::from(removed)),
                    seed: c.seed,
                    ..Default::default()
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn men_fall_by_quarters_of_life() {
        assert_eq!(standing(100, 100), 4);
        assert_eq!(standing(76, 100), 4);
        assert_eq!(standing(75, 100), 3);
        assert_eq!(standing(26, 100), 2);
        assert_eq!(standing(1, 100), 1);
        assert_eq!(standing(0, 100), 0);
        assert_eq!(standing(5, 0), 4);
    }

    #[test]
    fn the_layer_stays_longest() {
        assert_eq!(shown(0), 0b1_1111);
        assert_eq!(shown(1), 0b1_0111);
        assert_eq!(shown(2), 0b1_0101);
        assert_eq!(shown(3), 0b1_0100);
        assert_eq!(shown(4), 0b1_0000);
    }

    #[test]
    fn a_turn_moves_through_its_step() {
        assert_eq!(turning(32, AnimKind::MotionLs(1), 10, 20), 40.0);
        // Turning right the sim has already taken the 16th off: it starts at the old facing.
        assert_eq!(turning(32, AnimKind::MotionRs(1), 0, 20), 48.0);
        assert_eq!(turning(32, AnimKind::MotionRs(1), 20, 20), 32.0);
        assert_eq!(turning(32, AnimKind::Stand, 10, 20), 32.0);
    }

    #[test]
    fn nations_wear_their_own_coats() {
        let e = uniform("Eng").unwrap();
        let f = uniform("Frn").unwrap();
        let u = uniform("Usa").unwrap();
        assert!(e[1][0] > 0.5 && f[1][2] > f[1][0] && u[1][1] > 0.5);
        assert!(uniform("Hur").is_none());
        assert_eq!(nation_of_type("ArtPus(FR)"), Some("Frn"));
        assert_eq!(nation_of_type("ArtPus"), None);
    }
}
