//! What the original draws and plays around the simulation without changing it:
//! projectiles, smoke and explosions (`WEAPON.NDS` objects, drawn by 0x557cb0), the fire
//! and smoke of damaged buildings (0x50618e, buildings with sections 0x5052b5), the `#WORK`
//! animations of finished buildings (0x50927a) and the weapon sounds (0x5583a2).
//!
//! Everything here only reads the world; the fires use their own random numbers like the
//! original's C `rand()` (0x616961), not the synchronised ones.

use std::collections::BTreeMap;

use acr_formats::UnitDef;
use acr_render::objects_view::ObjectsView;
use acr_render::scene::{Layer, SceneSprite};
use acr_sim::projectile::Projectile;
use acr_sim::upgrade::{section, section_state};
use acr_sim::weapon::{FireAnims, transparency};
use acr_sim::world::{EntityId, Unit};
use acr_sim::{Rng, World};

use crate::unit_sounds::{self, SoundEvent};

/// Rows a `[HILAYER]` weapon is drawn later (0x557f17).
const HI_LAYER: i32 = 300;
/// Rows a building's fires are drawn after the building (0x505ab7).
const FIRE_LAYER: i32 = 400;
/// The engine's frame step (`FrmDec`, 0x668f10): animation sounds start when
/// `frame / 2` reaches the sound's `frame / 2` (0x5295eb).
const FRM_DEC: usize = 2;

/// The direction of a projectile's animation of `dirs` directions (0x557e02): the angle
/// of its screen velocity (`vx`, `vy − 2·vz`), or of the way to its destination while it
/// stands, folded to 0..=π; `true` when it is mirrored (moving right, or a standing one
/// facing a destination to the right: 0x557e58 tests the `x` it fed to `fpatan`, the
/// destination's `dx` for a standing object, #242).
///
/// Seen from the free camera turned by `yaw` (radians, `Orbit::yaw`; 0 for the original
/// view) the flight's ground part is turned into the camera's frame first (#309).
pub fn direction(p: &Projectile, dirs: usize, yaw: f32) -> (usize, bool) {
    if dirs <= 1 {
        return (0, false);
    }
    let (s, c) = f64::from(yaw).sin_cos();
    let turn = |x: i32, y: i32| {
        let (x, y) = (f64::from(x), f64::from(y));
        if yaw == 0.0 { (x, y) } else { (x * c + y * s, y * c - x * s) }
    };
    let (vx, vy) = turn(p.vel.0, p.vel.1);
    let vz = f64::from(p.vel.2);
    let (a, right) = if p.vel.0 == 0 && p.vel.1 == 0 {
        let (dx, dy) = turn(p.dest.0 - p.pos.0, p.dest.1 - p.pos.1);
        (dx.atan2(dy), dx > 0.0)
    } else {
        ((-vx).atan2(vy - 2.0 * vz), vx > 0.0)
    };
    let a = if a >= 3.1415297 { a - 6.28305854 } else { a };
    let n = (2 * dirs - 2) as i64;
    let d = ((a.abs() * n as f64 * 0.318_316_308_413_704_5) as i64).min(n - 1);
    (((d + 1) >> 1) as usize, right)
}

/// The fires of one kind (`FIRES` or `SMOKE` points) of one building (OB+0xfd, 13 bytes).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Fire {
    /// Per point: the animation frame and the state (1 starting, 2 burning).
    pub frames: Vec<u8>,
    pub states: Vec<u8>,
    /// Ticks until the next point catches fire.
    pub delay: u8,
    /// Points that should burn (+9) and that do (+0xb).
    pub target: usize,
    pub active: usize,
}

impl Fire {
    fn new(points: usize, target: usize, rng: &mut Rng) -> Self {
        Self { frames: vec![0; points], states: vec![0; points], delay: (rng.below(4) + 1) as u8, target, active: 0 }
    }

    /// One tick (0x505d3b): the next point catches fire after a delay of 8..=23 ticks,
    /// points beyond the target go out at once, and the burning ones advance their
    /// animation (`start`/`cycle`: frames per variant, 0 without one) from the start
    /// animation into the loop.
    pub fn advance(&mut self, start: &[usize], cycle: &[usize], rng: &mut Rng) {
        let nvar = start.len().max(1);
        if self.target > self.active && self.active < self.states.len() {
            let i = self.active;
            if self.delay != 0 {
                self.delay -= 1;
            } else {
                self.active += 1;
                self.states[i] = if start.get(i % nvar).is_some_and(|&n| n > 0) { 1 } else { 2 };
                self.frames[i] = 0;
                self.delay = (rng.below(16) + 8) as u8;
            }
            for j in 0..i {
                if self.states[j] == 3 {
                    (self.states[j], self.frames[j]) = (2, 0);
                }
            }
        }
        self.active = self.active.min(self.target);
        for j in 0..self.active {
            let v = j % nvar;
            self.frames[j] = self.frames[j].wrapping_add(1);
            let f = usize::from(self.frames[j]);
            match self.states[j] {
                1 if f == start.get(v).copied().unwrap_or(0) => (self.frames[j], self.states[j]) = (0, 2),
                2 if f == cycle.get(v).copied().unwrap_or(0) => self.frames[j] = 0,
                _ => {}
            }
        }
    }
}

/// How many of `points` fire points burn (0x505b8a): a burning building shows one per
/// lost share of its type's life `max_life` (the owner's `LIFE`, not the object's maximum),
/// at least 4; a damaged one starts below three quarters of it (`M/2 + M/4`). `None`: no
/// fire is drawn and the state is kept.
pub fn burning_points(points: usize, life: i32, max_life: i32, burning: bool) -> Option<usize> {
    let n = points as i64;
    if n == 0 {
        return None;
    }
    let (life, max) = (i64::from(life), i64::from(max_life));
    if burning {
        if max == 0 {
            return None;
        }
        return Some((n * (max - life) / max).max(4).min(n) as usize);
    }
    let t = (max >> 1) + (max >> 2);
    if life >= t || t == 0 {
        return None;
    }
    Some((n * (t - life) / t) as usize)
}

/// The life band of a building with sections (0x5052b5): the last done section `c - 1`
/// (`c` = 0: none) spans `lo..hi`, from the type's life plus the done sections before it
/// to that plus its own life; without a done section `0..type life`.
pub fn section_band(stages: u16, type_life: i32, sections: &[(u8, i32)]) -> (u8, i32, i32) {
    let (mut c, mut lo, mut hi, mut sum) = (0, 0, type_life, type_life);
    for s in 0..5u8 {
        let prev = sum;
        if section_state(stages, s) == section::DONE {
            sum += sections.iter().find(|x| x.0 == s).map_or(0, |x| i32::from(x.1 as u16));
            (c, lo, hi) = (s + 1, prev, sum);
        }
    }
    (c, lo, hi)
}

/// How many of `points` fire points of a building with sections burn (0x50540a): fires
/// start below three quarters of the band `lo..hi` and all burn at its bottom, whether the
/// building burns or not. `None`: no fire is drawn and the state is kept.
pub fn section_points(points: usize, life: i32, lo: i32, hi: i32) -> Option<usize> {
    let n = points as i64;
    let (life, lo, hi) = (i64::from(life), i64::from(lo), i64::from(hi));
    let t = (hi * 3 + lo) >> 2;
    if n == 0 || life >= t {
        return None;
    }
    let range = ((hi - lo) * 3) >> 2;
    // The engine divides by zero here (a done section without life).
    let k = if range == 0 { n } else { n * (t - life) / range };
    Some(k.min(n) as usize)
}

/// Where a building's fires (`k` 0) or smoke (1) burn and how many of the points.
struct Layout {
    xy: Vec<(i32, i32)>,
    target: usize,
    /// A building with sections: its last done section + 1, whose change resets both kinds
    /// (OB+0xfd byte 0, 0x505487).
    tag: Option<u8>,
}

/// The [`Layout`] of fires or smoke, `None` when none is drawn. A building with sections
/// (OB+0xaa not 0, 0x505af5) burns at the `STAGE_FIRES` / `STAGE_SMOKE` points of its last
/// done section ([`section_points`]), or at its `FIRES` / `SMOKE` without one; others at
/// their `FIRES` / `SMOKE` ([`burning_points`]).
fn layout(world: &World, def: &UnitDef, u: &Unit, k: usize) -> Option<Layout> {
    let type_life = world.adv(u.owner, u.kind).life;
    let plain = || def.get(["FIRES", "SMOKE"][k]).map(|c| c.points(0)).unwrap_or_default();
    if u.stages == 0 {
        let xy = plain();
        let target = burning_points(xy.len(), u.life, type_life, u.burning)?;
        return Some(Layout { xy, target, tag: None });
    }
    let sections = &world.catalog.get(u.kind).economy.sections;
    let (c, lo, hi) = section_band(u.stages, type_life, sections);
    let xy = match c {
        0 => plain(),
        _ => def.stage_points(["STAGE_FIRES", "STAGE_SMOKE"][k], c - 1).unwrap_or_default(),
    };
    let target = section_points(xy.len(), u.life, lo, hi)?;
    Some(Layout { xy, target, tag: Some(c) })
}

/// The effects of a running game.
pub struct Effects {
    /// Fires (0) and smoke (1) per building, in id order (a stable draw order), with the
    /// [`Layout::tag`] of a building with sections.
    fires: BTreeMap<EntityId, (u8, [Option<Fire>; 2])>,
    rng: Rng,
    /// The frame counters (OB+0xec) of the `#WORK` animations of finished buildings
    /// ([`acr_render::scene::work_part`]): from 0 when the building is done, one step per
    /// tick while it is in view (the original steps it when it draws it, 0x509480).
    work: BTreeMap<EntityId, usize>,
}

impl Default for Effects {
    fn default() -> Self {
        Self { fires: BTreeMap::new(), rng: Rng::new(0xF12E, 7), work: BTreeMap::new() }
    }
}

impl Effects {
    /// One tick of the fires of the buildings in `view` (world pixels `[x0, y0, x1, y1]`;
    /// the original only advances fires it draws). Returns the animation sounds started.
    pub fn tick(&mut self, world: &World, objects: &ObjectsView, view: [i32; 4]) -> Vec<(String, i32, i32)> {
        let mut sounds = Vec::new();
        let assets = objects.objects();
        let in_view = |x: i32, y: i32| x >= view[0] && x <= view[2] && y >= view[1] && y <= view[3];
        // A building that is not done (any more) starts its `#WORK` over when it is
        // (0x52393b).
        self.work.retain(|id, _| world.unit(*id).is_some_and(|u| u.living() && u.stage >= world.max_stage(u)));
        for u in world.units.iter().filter(|u| u.building && u.living() && u.stage >= world.max_stage(u)) {
            if !in_view(u.pos.x >> 4, u.pos.y >> 4) {
                continue;
            }
            if let Some(n) = assets.work_frames(&world.catalog.get(u.kind).id) {
                // Drawn at the counter, then stepped; at the frame count it starts over
                // (0x5092c1).
                let step = self.work.entry(u.id).or_insert(usize::MAX);
                *step = step.wrapping_add(1) % n;
            }
        }
        let Some(effects) = assets.effects() else { return sounds };
        let frames = |a: &str| effects.animation(a).map_or(0, |a| a.frame_refs().len());
        let fires = &world.catalog.weapons.fires;
        let lens: Vec<(Vec<usize>, Vec<usize>)> = fires
            .iter()
            .map(|f| (f.start.iter().map(|a| frames(a)).collect(), f.cycle.iter().map(|a| frames(a)).collect()))
            .collect();
        self.fires.retain(|id, _| world.unit(*id).is_some_and(|u| u.living()));
        for u in world.units.iter().filter(|u| u.building && u.living() && u.inside_of.is_none()) {
            if !in_view(u.pos.x >> 4, u.pos.y >> 4) {
                continue;
            }
            // Only a finished building (OB+0x93 == NM+0xda, 0x5061ae).
            if u.stage < world.max_stage(u) {
                continue;
            }
            let Some(def) = assets.definition(&world.catalog.get(u.kind).id) else { continue };
            for (k, (start, cycle)) in lens.iter().enumerate().take(2) {
                let Some(look) = layout(world, def, u, k) else { continue };
                let entry = self.fires.entry(u.id).or_default();
                if let Some(tag) = look.tag
                    && tag != entry.0
                {
                    *entry = (tag, [None, None]);
                }
                let fire = entry.1[k].get_or_insert_with(|| Fire::new(look.xy.len(), look.target, &mut self.rng));
                fire.target = look.target;
                fire.advance(start, cycle, &mut self.rng);
            }
        }
        // Their animation sounds (`SOUND 1 #DFIRE2 FIRE1`), at the point they burn.
        sounds.extend(self.burning(world, objects).into_iter().filter_map(|look| look.sound));
        sounds
    }

    /// The frame of a finished building's `#WORK` animation, as last stepped.
    pub fn work_step(&self, building: EntityId) -> usize {
        self.work.get(&building).copied().unwrap_or(0)
    }

    /// The sprites of the fires of the buildings, as last advanced; none for a building
    /// `modelled` (drawn in 3D with its own fire, #303).
    pub fn fire_sprites(
        &self,
        world: &World,
        objects: &ObjectsView,
        modelled: &dyn Fn(EntityId) -> bool,
        out: &mut Vec<SceneSprite>,
    ) {
        out.extend(
            self.burning(world, objects).into_iter().filter(|look| !modelled(look.unit)).map(|look| look.sprite),
        );
    }

    /// The burning points (0x505e5f): drawn while the building is damaged enough.
    fn burning(&self, world: &World, objects: &ObjectsView) -> Vec<FireLook> {
        let mut out = Vec::new();
        let assets = objects.objects();
        let Some(effects) = assets.effects() else { return out };
        for (&id, (tag, kinds)) in &self.fires {
            let Some(u) = world.unit(id) else { continue };
            let Some(def) = assets.definition(&world.catalog.get(u.kind).id) else { continue };
            let t = world.catalog.get(u.kind);
            let h = objects_height(world, u.pos.x >> 4, u.pos.y >> 4);
            for (k, fire) in kinds.iter().enumerate() {
                let Some(fire) = fire else { continue };
                let anims: &FireAnims = &world.catalog.weapons.fires[k];
                let Some(look) = layout(world, def, u, k).filter(|l| l.tag.is_none_or(|c| c == *tag)) else {
                    continue;
                };
                for j in 0..fire.active.min(look.xy.len()) {
                    let nvar = anims.start.len().max(1);
                    let v = j % nvar;
                    let anim = match fire.states[j] {
                        1 => anims.start.get(v),
                        2 => anims.cycle.get(v),
                        3 => world.catalog.weapons.fires[0].end.get(v),
                        _ => None,
                    };
                    let Some(anim) = anim else { continue };
                    let step = usize::from(fire.frames[j]);
                    let Some((frame, (fx, fy))) = assets.effect_look(anim, step, 0) else { continue };
                    let (px, py) = look.xy[j];
                    let (dx, dy) = (t.location.0 + px, t.location.1 + py);
                    let (x, y) = (u.pos.x >> 4, u.pos.y >> 4);
                    let sound = anim_sound(effects, anim, step).map(|s| (s, x + dx, (y >> 1) - h + dy));
                    let sprite = SceneSprite {
                        frame,
                        layer: Layer::Sorted,
                        x,
                        y,
                        dx: dx + fx,
                        dy: dy + fy - h,
                        sort: (y >> 1) + FIRE_LAYER + j as i32,
                        player: 0,
                        transparent: false,
                        mirrored: false,
                        white: false,
                    };
                    out.push(FireLook { sprite, sound, unit: id });
                }
            }
        }
        out
    }
}

struct FireLook {
    sprite: SceneSprite,
    unit: EntityId,
    sound: Option<(String, i32, i32)>,
}

fn objects_height(world: &World, x: i32, y: i32) -> i32 {
    world.map.heights.at(x, y)
}

/// The `SOUND frame #ANIM name` of an effect animation that starts at `step` (0x5295eb).
fn anim_sound(effects: &acr_formats::UnitDef, anim: &str, step: usize) -> Option<String> {
    effects.all("SOUND").find_map(|c| {
        let (frame, on, name) = (c.int(0)?, c.word(1)?, c.word(2)?);
        (on.eq_ignore_ascii_case(anim) && frame >= 0 && frame as usize / FRM_DEC == step / FRM_DEC)
            .then(|| name.to_owned())
    })
}

/// The sprites of the projectiles, smoke and explosions in flight (0x557cb0): frame
/// `frame` of the weapon's animation (clamped to the last), facing its flight, at
/// `(x, y/2 − z)`, sorted at the ground row plus the shot point's height (and 300 for
/// `[HILAYER]`). `WHITE` weapons lighten. With `rounds_3d` the rounds drawn as models
/// (`missile_view::round`, #308) are left out.
pub fn projectile_sprites(world: &World, objects: &ObjectsView, rounds_3d: bool, yaw: f32, out: &mut Vec<SceneSprite>) {
    let assets = objects.objects();
    let Some(effects) = assets.effects() else { return };
    for p in world.projectiles.iter() {
        if rounds_3d && super::missile_view::round(world, p).is_some() {
            continue;
        }
        let wp = world.catalog.weapons.get(p.weapon);
        let Some(anim) = effects.animation(&wp.anim) else { continue };
        let (dir, mirrored) = direction(p, anim.directions.max(1) as usize, yaw);
        let Some((frame, (dx, dy))) = assets.effect_look(&wp.anim, p.frame.max(0) as usize, dir) else { continue };
        let (x, y, z) = (p.pos.0 >> 14, p.pos.1 >> 14, p.pos.2 >> 14);
        out.push(SceneSprite {
            frame,
            layer: Layer::Sorted,
            x,
            y,
            // A mirrored frame is placed at `x − dx` (0x557eab).
            dx: if mirrored { -dx } else { dx },
            dy: dy - z,
            sort: (y >> 1) + p.extra + if wp.hi_layer { HI_LAYER } else { 0 },
            player: 0,
            transparent: false,
            mirrored,
            white: wp.transparency == transparency::WHITE,
        });
    }
}

/// Where the flames of a shell fire stand, relative to its centre (in units of its radius).
const GROUND_FLAMES: [(i32, i32); 3] = [(0, 0), (-1, 1), (1, -1)];

/// The sprites of the shell fires on the ground (#256, rule `hit_locations`): a few flames
/// of the burning-building loop (`[FIRES]`, `#DFIRE2`) per fire, each a little out of step,
/// fewer as the fire burns down. Sorted at the ground row like the buildings' fires.
pub fn ground_fire_sprites(world: &World, objects: &ObjectsView, out: &mut Vec<SceneSprite>) {
    if world.ground_fires.list.is_empty() {
        return;
    }
    let assets = objects.objects();
    let Some(effects) = assets.effects() else { return };
    let Some(anim) = world.catalog.weapons.fires[0].cycle.first() else { return };
    let Some(len) = effects.animation(anim).map(|a| a.frame_refs().len()).filter(|&n| n > 0) else { return };
    for f in &world.ground_fires.list {
        let age = f.age(world.tick.0);
        // All flames for the first half, then one goes out per sixth of the rest.
        let left = acr_sim::ground_fire::BURN_TICKS.saturating_sub(age);
        let n = (left * 6 / acr_sim::ground_fire::BURN_TICKS).clamp(1, GROUND_FLAMES.len() as u64) as usize;
        let h = objects_height(world, f.x, f.y);
        for (j, &(ox, oy)) in GROUND_FLAMES.iter().take(n).enumerate() {
            let step = (age as usize + 5 * j) % len;
            let Some((frame, (fx, fy))) = assets.effect_look(anim, step, 0) else { continue };
            let (x, y) = (f.x + ox * f.radius / 2, f.y + oy * f.radius / 2);
            out.push(SceneSprite {
                frame,
                layer: Layer::Sorted,
                x,
                y,
                dx: fx,
                dy: fy - h,
                sort: (y >> 1) + FIRE_LAYER,
                player: 0,
                transparent: false,
                mirrored: false,
                white: false,
            });
        }
    }
}

/// The weapon sounds of the last ticks as sound events, placed like the original's world
/// sounds (0x586a48: `x`, `y/2 − z`).
pub fn weapon_sounds(
    world: &World,
    sounds: &[acr_sim::projectile::WeaponSound],
    view: [f32; 4],
    fog: Option<&acr_sim::fog::Fog>,
) -> Vec<SoundEvent> {
    sounds
        .iter()
        .filter_map(|s| {
            let name = world.catalog.weapons.list.get(usize::from(s.weapon))?.sound.clone()?;
            let (volume, pan) = unit_sounds::placed(s.x as f32, s.y as f32, view, fog)?;
            Some(SoundEvent { name, pan, volume })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shot(vel: (i32, i32, i32), dest: (i32, i32)) -> Projectile {
        Projectile {
            weapon: 0,
            pos: (1000 << 14, 1000 << 14, 0),
            vel,
            az: 0,
            dest: (dest.0 << 14, dest.1 << 14, 0),
            extra: 0,
            frame: 0,
            times: 1,
            damage: 0,
            sender: None,
            att: 0,
            plough: Default::default(),
            shot: Default::default(),
            dest_obj: None,
        }
    }

    #[test]
    fn projectiles_face_their_flight() {
        // 17 directions from down the screen (0) to up (16); moving right mirrors.
        assert_eq!(direction(&shot((0, 1 << 14, 0), (0, 0)), 17, 0.0), (0, false));
        assert_eq!(direction(&shot((0, -(1 << 14), 0), (0, 0)), 17, 0.0), (16, false));
        assert_eq!(direction(&shot((-(1 << 14), 0, 0), (0, 0)), 17, 0.0), (8, false));
        assert_eq!(direction(&shot((1 << 14, 0, 0), (0, 0)), 17, 0.0), (8, true));
        // Rising counts as moving up the screen (45° here).
        assert_eq!(direction(&shot((-(1 << 14), 0, 1 << 13), (0, 0)), 17, 0.0).0, 12);
        // A standing object (a musket's smoke) faces its destination, mirrored when it lies
        // to the right (0x557e58, #242): a shot to the north-east puffs north-east.
        assert_eq!(direction(&shot((0, 0, 0), (2000, 1000)), 17, 0.0), (8, true));
        assert_eq!(direction(&shot((0, 0, 0), (0, 1000)), 9, 0.0), (4, false));
        let (ne, nw) = (direction(&shot((0, 0, 0), (2000, 0)), 17, 0.0), direction(&shot((0, 0, 0), (0, 0)), 17, 0.0));
        assert_eq!(ne, (nw.0, !nw.1));
        assert!(ne.1);
        // Like a ball flying the same way.
        assert_eq!(ne, direction(&shot((1 << 14, -(1 << 14), 0), (0, 0)), 17, 0.0));
        assert_eq!(direction(&shot((5, 5, 5), (0, 0)), 1, 0.0), (0, false));
        // The free camera looking east (#309): a shot flying east flies away from it.
        let east = direction(&shot((1 << 14, 0, 0), (0, 0)), 17, std::f32::consts::FRAC_PI_2);
        assert_eq!(east, direction(&shot((0, -(1 << 14), 0), (0, 0)), 17, 0.0));
    }

    #[test]
    fn damaged_buildings_burn_more_points_the_more_they_lose() {
        assert_eq!(burning_points(13, 1000, 1000, false), None);
        // Fires start below three quarters of the life.
        assert_eq!(burning_points(13, 750, 1000, false), None);
        assert_eq!(burning_points(13, 749, 1000, false), Some(0));
        assert_eq!(burning_points(13, 375, 1000, false), Some(6));
        assert_eq!(burning_points(13, 0, 1000, false), Some(13));
        // A burning building: at least 4 points.
        assert_eq!(burning_points(13, 990, 1000, true), Some(4));
        assert_eq!(burning_points(2, 990, 1000, true), Some(2));
        assert_eq!(burning_points(10, 100, 1000, true), Some(9));
        assert_eq!(burning_points(0, 0, 1000, true), None);
    }

    #[test]
    fn fires_catch_one_by_one_and_loop() {
        let mut rng = Rng::new(1, 1);
        let mut f = Fire::new(3, 2, &mut rng);
        let first = f.delay;
        assert!((1..=4).contains(&first));
        let (start, cycle) = ([3usize], [5usize]);
        for _ in 0..first {
            f.advance(&start, &cycle, &mut rng);
        }
        assert_eq!(f.active, 0);
        f.advance(&start, &cycle, &mut rng);
        assert_eq!((f.active, f.states[0], f.frames[0]), (1, 1, 1));
        assert!((8..=23).contains(&f.delay));
        // The start animation (3 frames) runs into the loop (5 frames).
        f.advance(&start, &cycle, &mut rng);
        f.advance(&start, &cycle, &mut rng);
        assert_eq!((f.states[0], f.frames[0]), (2, 0));
        for _ in 0..5 {
            f.advance(&start, &cycle, &mut rng);
        }
        assert_eq!(f.frames[0], 0, "the loop wraps");
        for _ in 0..30 {
            f.advance(&start, &cycle, &mut rng);
        }
        assert_eq!(f.active, 2, "never more than the target");
        // Repairs put points out at once.
        f.target = 1;
        f.advance(&start, &cycle, &mut rng);
        assert_eq!(f.active, 1);
    }

    #[test]
    fn buildings_with_sections_burn_in_the_band_of_their_last_done_section() {
        // PUEGOR: sections of 10000; 0 and 2 done, 1 a ruin (state 3).
        let sections = [(0, 10000), (1, 10000), (2, 10000), (3, 10000)];
        let stages = 0x8000 | 2 | 3 << 3 | 2 << 6;
        assert_eq!(section_band(stages, 5000, &sections), (3, 15000, 25000));
        assert_eq!(section_band(0x8000 | 2, 5000, &sections), (1, 5000, 15000));
        assert_eq!(section_band(0x8000 | 3, 5000, &sections), (0, 0, 5000));
        // Fires start below three quarters of the band; the burning flag does not count.
        assert_eq!(section_points(10, 12500, 5000, 15000), None);
        assert_eq!(section_points(10, 12499, 5000, 15000), Some(0));
        assert_eq!(section_points(10, 8750, 5000, 15000), Some(5));
        assert_eq!(section_points(10, 5000, 5000, 15000), Some(10));
        assert_eq!(section_points(10, 1000, 5000, 15000), Some(10), "never more than the points");
        assert_eq!(section_points(0, 1000, 5000, 15000), None);
        // Without a done section: three quarters of the type's life.
        assert_eq!(section_points(12, 3750, 0, 5000), None);
        assert_eq!(section_points(12, 1875, 0, 5000), Some(6));
        assert_eq!(section_points(4, 100, 900, 900), Some(4), "a section without life");
    }
}
