//! The close-up bench (#277, `docs/testing.md`): soldiers placed where the test wants them,
//! scripted hits through the simulation's body path, and the camera close up.

use acr_formats::m3d::SpriteGroup;
use acr_sim::Pos;
use acr_sim::body::{self, Injury, KNEE, LimbId, Outcome, Part};
use acr_sim::combat::{DeathCause, ScriptedHit, scripted_hit};
use acr_sim::wounds::HitRegion;
use acr_sim::{Command, world::EntityId};
use serde_json::{Value, json};

use crate::{Game, Mode};

/// What a spawned soldier does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pose {
    Stand,
    /// Walks back and forth (a patrol) over `walk` world pixels along x.
    Walk,
    /// One leg lost below the knee: he crawls.
    Crawl,
}

impl Pose {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "stand" | "standing" => Ok(Self::Stand),
            "walk" | "walking" => Ok(Self::Walk),
            "crawl" | "crawling" => Ok(Self::Crawl),
            other => Err(format!("unknown pose {other:?}: stand, walk or crawl")),
        }
    }
}

/// The weapon of a scripted hit: the death cause and whether the blow is a rider's.
pub fn cause(name: &str) -> Result<(DeathCause, bool), String> {
    Ok(match name {
        "musket" | "gunshot" => (DeathCause::Gunshot, false),
        "arrow" => (DeathCause::Arrow, false),
        "cannonball" | "ball" => (DeathCause::Cannonball, false),
        "grapeshot" => (DeathCause::Grapeshot, false),
        "shell" | "explosion" => (DeathCause::Explosion, false),
        "sabre" | "saber" => (DeathCause::Melee, true),
        "bayonet" | "pike" => (DeathCause::Melee, false),
        "axe" | "tomahawk" | "thrown" => (DeathCause::Thrown, false),
        other => {
            return Err(format!(
                "unknown cause {other:?}: musket, arrow, cannonball, grapeshot, shell, sabre, bayonet, axe, claw, bite or gore"
            ));
        }
    })
}

/// An animal's blow for a scripted hit (`claw`, `bite`, `gore`), `None` for a weapon.
pub fn maul(name: &str) -> Option<body::Cause> {
    match name {
        "claw" => Some(body::Cause::Claw),
        "bite" => Some(body::Cause::Bite),
        "gore" => Some(body::Cause::Gore),
        _ => None,
    }
}

/// A body part to aim at (`hit`'s `part`).
pub fn part(name: &str) -> Result<Part, String> {
    Ok(match name {
        "arm_l" => Part::Limb(LimbId::ArmL),
        "arm_r" => Part::Limb(LimbId::ArmR),
        "leg_l" => Part::Limb(LimbId::LegL),
        "leg_r" => Part::Limb(LimbId::LegR),
        "head" => Part::Head,
        "chest" => Part::Chest,
        "belly" => Part::Belly,
        "pelvis" => Part::Pelvis,
        other => {
            return Err(format!("unknown part {other:?}: arm_l, arm_r, leg_l, leg_r, head, chest, belly or pelvis"));
        }
    })
}

pub fn region(name: &str) -> Result<HitRegion, String> {
    Ok(match name {
        "legs" | "leg" => HitRegion::Legs,
        "arms" | "arm" => HitRegion::Arms,
        "torso" | "body" | "chest" => HitRegion::Torso,
        "head" => HitRegion::Head,
        other => return Err(format!("unknown region {other:?}: legs, arms, torso or head")),
    })
}

/// Places a unit of `kind` (a catalog id or a part of one) for `owner` at world pixels.
pub fn spawn(game: &mut Game, v: &Value) -> Result<Value, String> {
    let Mode::InGame(g) = &mut game.mode else { return Err("no game".into()) };
    let name = v.get("type").and_then(Value::as_str).ok_or("spawn: missing \"type\"")?;
    let x = v.get("x").and_then(Value::as_f64).ok_or("spawn: missing \"x\"")? as i32;
    let y = v.get("y").and_then(Value::as_f64).ok_or("spawn: missing \"y\"")? as i32;
    let owner = v.get("owner").and_then(Value::as_u64).unwrap_or(0) as u8;
    let dir = v.get("dir").and_then(Value::as_u64).unwrap_or(0) as u8;
    let pose = Pose::parse(v.get("pose").and_then(Value::as_str).unwrap_or("stand"))?;
    let walk = v.get("walk").and_then(Value::as_i64).unwrap_or(200) as i32;
    let world = g.world_mut().ok_or("no world yet")?;
    let kind = find_type(world, name)?;
    let id = world.spawn(kind, owner, Pos::from_pixels(x, y), dir);
    // `resources`: the owner's stock of every resource set to this (a cannon pays per shot).
    if let Some(n) = v.get("resources").and_then(Value::as_i64)
        && let Some(p) = world.players.get_mut(usize::from(owner))
    {
        p.resources = [n.clamp(0, i64::from(i32::MAX)) as i32; 8];
    }
    let i = id.0 as usize;
    match pose {
        Pose::Stand => {}
        Pose::Walk => g.push_command(Command::Patrol { player: owner, units: vec![id], x: x + walk, y }),
        Pose::Crawl => {
            let leg = Injury {
                part: Part::Limb(LimbId::LegL),
                level: KNEE,
                outcome: Outcome::Off,
                energy: 0,
                cause: body::Cause::Cannonball,
            };
            body::apply(&mut g.world_mut().ok_or("no world")?.units[i].body, &leg);
        }
    }
    // Bench bodies (#279): `lost` limbs (`leg_l`, `arm_r`; `foot_l` keeps the leg to the
    // ankle) and one flesh wound per entry of `wounds`; `patrol` makes any pose move.
    let limb = |n: &str| match n {
        "arm_l" => Some(LimbId::ArmL),
        "arm_r" => Some(LimbId::ArmR),
        "leg_l" | "foot_l" => Some(LimbId::LegL),
        "leg_r" | "foot_r" => Some(LimbId::LegR),
        _ => None,
    };
    let names = |k: &str| -> Vec<String> {
        v.get(k)
            .and_then(Value::as_array)
            .map_or(Vec::new(), |a| a.iter().filter_map(Value::as_str).map(String::from).collect())
    };
    let body = &mut g.world_mut().ok_or("no world")?.units[i].body;
    for n in names("lost") {
        let l = limb(&n).ok_or_else(|| format!("unknown limb {n:?}: arm_l, arm_r, leg_l, leg_r, foot_l, foot_r"))?;
        let level = if n.starts_with("foot") { body::ANKLE } else { KNEE };
        body::apply(
            body,
            &Injury { part: Part::Limb(l), level, outcome: Outcome::Off, energy: 0, cause: body::Cause::Cannonball },
        );
    }
    for n in names("wounds") {
        let l = limb(&n).ok_or_else(|| format!("unknown limb {n:?}"))?;
        body.limbs[l as usize].wounds = body.limbs[l as usize].wounds.saturating_add(1);
    }
    // `broken`: a closed break at the ankle (`leg_l`, `leg_r`): he still stands on it (#279 hop).
    for n in names("broken") {
        let l = limb(&n).ok_or_else(|| format!("unknown limb {n:?}"))?;
        body.limbs[l as usize].fracture =
            Some(body::Fracture { level: body::ANKLE, kind: body::FractureKind::Closed, bend: 0 });
    }
    if pose != Pose::Walk && v.get("patrol").and_then(Value::as_bool) == Some(true) {
        if g.world().is_some_and(|w| w.units[i].body.out_of_fight()) {
            // Crawlers take no orders: sent straight on their way.
            let u = &mut g.world_mut().ok_or("no world")?.units[i];
            acr_sim::motion::send_to(u, Pos::from_pixels(x + walk, y), acr_sim::motion::PLAYER_PRIO, true);
        } else {
            g.push_command(Command::Patrol { player: owner, units: vec![id], x: x + walk, y });
        }
    }
    // `life`: life and max life set to this (a victim that lasts several blows);
    // `hold_fire`: melee only (the interface's melee-only state, `Unit::melee_only`): a
    // musketeer fights with the bayonet, a dragoon with the sabre, and neither fires
    // point-blank at a man on the ground.
    let u = &mut g.world_mut().ok_or("no world")?.units[i];
    if let Some(n) = v.get("life").and_then(Value::as_i64) {
        u.life = n.clamp(1, i64::from(i32::MAX)) as i32;
        u.max_life = u.life;
    }
    // `max_life`: the full life alone, after `life` (a building spawned damaged, #303).
    if let Some(n) = v.get("max_life").and_then(Value::as_i64) {
        u.max_life = n.clamp(1, i64::from(i32::MAX)) as i32;
    }
    if v.get("hold_fire").and_then(Value::as_bool) == Some(true) {
        u.melee_only = true;
    }
    // A building under construction (#302): `progress` 0..1 of its `BUILDSTAGES`, shown with
    // its `#BUILDLO_k` (`construction::stage_anim`); `sections`: the section states, one per
    // `ADDSECTION` in order (0 none, 1 being built, 2 done, 3 ruin).
    if let Some(p) = v.get("progress").and_then(Value::as_f64) {
        let world = g.world().ok_or("no world")?;
        let max = world.max_stage(&world.units[i]);
        let stage = ((p.clamp(0.0, 1.0) * f64::from(max)) as i32).min(max);
        let u = &mut g.world_mut().ok_or("no world")?.units[i];
        u.stage = stage;
        if stage < max {
            u.anim = acr_sim::world::AnimKind::BuildLo((stage * 4 / max.max(1)).clamp(0, 3) as u8);
            u.frame = 0;
        }
    }
    if let Some(list) = v.get("sections").and_then(Value::as_array) {
        let u = &mut g.world_mut().ok_or("no world")?.units[i];
        for (s, state) in list.iter().enumerate().take(5) {
            let state = state.as_u64().unwrap_or(0).min(3) as u16;
            u.stages = acr_sim::upgrade::with_section(u.stages, s as u8, state);
        }
    }
    let world = g.world().ok_or("no world")?;
    Ok(
        json!({"id": id.0, "type": world.catalog.get(kind).id, "pose": format!("{pose:?}").to_lowercase(), "life": world.units[i].life}),
    )
}

/// The catalog type by its exact id, else the first (by id) whose id contains `name`,
/// ignoring case.
fn find_type(world: &acr_sim::World, name: &str) -> Result<acr_sim::TypeId, String> {
    let lower = name.to_lowercase();
    if let Some(t) = world.catalog.find(name) {
        return Ok(t);
    }
    let mut found: Vec<(acr_sim::TypeId, &str)> = world
        .catalog
        .iter()
        .filter(|(_, t)| t.id.to_lowercase().contains(&lower))
        .map(|(id, t)| (id, t.id.as_str()))
        .collect();
    found.sort_by_key(|&(_, s)| (!s.eq_ignore_ascii_case(name), s.len(), s.to_owned()));
    found.first().map(|&(id, _)| id).ok_or_else(|| format!("no object type like {name:?}"))
}

/// A scripted hit (`hit`): the body path of battle, see [`scripted_hit`].
pub fn hit(game: &mut Game, v: &Value) -> Result<Value, String> {
    let Mode::InGame(g) = &mut game.mode else { return Err("no game".into()) };
    let unit = v.get("unit").and_then(Value::as_u64).ok_or("hit: missing \"unit\" (an id)")? as usize;
    let name = v.get("cause").and_then(Value::as_str).unwrap_or("musket");
    // Starved (#207): dies as `economy::starve` kills, no injury.
    if name == "hunger" {
        let world = g.world_mut().ok_or("no world yet")?;
        if !world.unit(EntityId(unit as u32)).is_some_and(|u| u.alive && u.death == 0) {
            return Err(format!("unit {unit} is not alive"));
        }
        acr_sim::combat::record_death(world, unit, acr_sim::combat::DeathRecord::of(DeathCause::Hunger, None));
        acr_sim::combat::die(world, unit);
        return Ok(json!({"unit": unit, "tick": world.tick.0, "cause": "Hunger", "killed": true}));
    }
    let maul = maul(name);
    let (cause, mounted) = if maul.is_some() { (DeathCause::Melee, false) } else { cause(name)? };
    let region = v.get("region").and_then(Value::as_str).map(region).transpose()?;
    let energy = v.get("energy").and_then(Value::as_u64).unwrap_or(100).min(100) as u8;
    let damage = v.get("damage").and_then(Value::as_i64).unwrap_or(1) as i32;
    let aim = v.get("part").and_then(Value::as_str).map(part).transpose()?;
    let level = v.get("level").and_then(Value::as_u64).map(|l| l.min(255) as u8);
    if level.is_some() && aim.is_none() {
        return Err("hit: \"level\" needs a \"part\"".into());
    }
    let aim = aim.map(|p| (p, level));
    let dir = match v.get("dir").and_then(Value::as_array).map(Vec::as_slice) {
        Some([x, y]) => {
            ((x.as_f64().unwrap_or(0.0) * 100.0).round() as i32, (y.as_f64().unwrap_or(0.0) * 100.0).round() as i32)
        }
        _ => (100, 0),
    };
    let world = g.world_mut().ok_or("no world yet")?;
    if world.unit(EntityId(unit as u32)).is_none() {
        return Err(format!("no unit {unit}"));
    }
    let index = scripted_hit(world, unit, ScriptedHit { cause, mounted, region, dir, energy, dmg: damage, aim, maul })
        .ok_or_else(|| format!("unit {unit} is not alive"))?;
    // A fire arrow (`OSTRELA`) or another fire weapon's round: only the record says so.
    world.hits[index].fire = v.get("fire").and_then(Value::as_bool).unwrap_or(false);
    let h = world.hits[index];
    Ok(json!({
        "unit": unit,
        "tick": h.tick,
        "cause": format!("{:?}", h.cause),
        "region": h.region.map(|r| format!("{r:?}")),
        "injury": h.injury.map(|i| json!({
            "part": format!("{:?}", i.part),
            "level": i.level,
            "outcome": format!("{:?}", i.outcome),
            "energy": i.energy,
            "cause": format!("{:?}", i.cause),
        })),
        "down": world.units[unit].body.down,
        "killed": h.killed,
        "life": world.units[unit].life,
    }))
}

/// `{"cmd":"impact","at":[x,y],"dir":[dx,dy],"speed":px_per_tick,"cause":"cannonball"}`: a
/// round striking the ground (or water) at world pixel `at` and hitting no one, as a missed
/// shot is recorded (`World::impacts`, render only): craters, furrows and earth fountains can
/// be filmed without casualties. `dir` is the flight direction (default east), `speed` the
/// ball's speed in px per tick (default 12). A shell `"by"` a howitzer also leaves a shell
/// fire (`ground_fire::shell_burst`, rule `hit_locations`); one by a mortar (`ArtMrt`, rule
/// `mortars`) leaves none and bursts smaller.
/// `{"cmd": "map_object", "group": "tree"|"stone", "index": n, "at": [[x, y], ...],
/// "rock": bool, "struck": "felled", "dir": 0, "source": 2}`: trees (wood sources) or stones put on the map as `World::objects`, so
/// balls meet them under the `ball_terrain` rule; a `rock` locks its ground cells. Drawn as
/// models with `map_models` where the table names `index`. Returns the struck-state keys.
pub fn map_object(game: &mut Game, v: &Value) -> Result<Value, String> {
    let Mode::InGame(g) = &mut game.mode else { return Err("no game".into()) };
    let group = v.get("group").and_then(Value::as_str).unwrap_or("tree");
    let tree = match group {
        "tree" => true,
        "stone" | "source" => false,
        o => return Err(format!("map_object: group {o:?} is not \"tree\", \"stone\" or \"source\"")),
    };
    let bare = group == "source";
    let index = v.get("index").and_then(Value::as_u64).unwrap_or(0) as u16;
    let rock = v.get("rock").and_then(Value::as_bool).unwrap_or(false);
    let at: Vec<(i32, i32)> = v
        .get("at")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|p| Some((p.get(0)?.as_i64()? as i32, p.get(1)?.as_i64()? as i32))).collect())
        .unwrap_or_default();
    let world = g.world_mut().ok_or("no world yet")?;
    let group = if tree { SpriteGroup::Trees } else { SpriteGroup::Stones };
    let ids: Vec<usize> = if bare {
        Vec::new()
    } else {
        at.iter().map(|&(x, y)| acr_sim::ball_terrain::add_object(world, group, index, x, y, tree, rock)).collect()
    };
    // `"source": k`: each also a gathering source of `ResType` k (2 stone, 3 field food; a
    // tree is one of wood already), so peasants can work it on the blank field; group
    // `"source"` places the source alone (an unseen field).
    if let Some(kind) = v.get("source").and_then(Value::as_u64) {
        use acr_sim::gather::{Source, Sources};
        let mut list = world.sources.list.clone();
        list.extend(at.iter().map(|&(x, y)| Source { x, y, kind: kind as u8, per_work: 4, radius: 30 }));
        world.sources = std::sync::Arc::new(Sources::new(list, world.map.land.width * 16));
    }
    // Already struck (render tests and the frame-time bench): `"struck": "felled" |
    // "splintered" | "chipped"` from flight direction `"dir"` (0 east, 64 south), now.
    if let Some(kind) = v.get("struck").and_then(Value::as_str) {
        use acr_sim::ball_terrain::Struck;
        let dir = v.get("dir").and_then(Value::as_u64).unwrap_or(0) as u8;
        let tick = world.tick.0;
        let state = match kind {
            "felled" => Struck::Felled { dir, tick },
            "splintered" => Struck::Splintered { hits: 1, dir, tick },
            "chipped" => Struck::Chipped { chips: 1, dir, tick },
            o => return Err(format!("map_object: struck {o:?} is not felled, splintered or chipped")),
        };
        for &k in &ids {
            world.struck.insert(k as u32, state);
        }
    }
    Ok(json!(ids))
}

pub fn impact(game: &mut Game, v: &Value) -> Result<Value, String> {
    let Mode::InGame(g) = &mut game.mode else { return Err("no game".into()) };
    let at = match v.get("at").and_then(Value::as_array).map(Vec::as_slice) {
        Some([x, y]) => (x.as_f64().unwrap_or(0.0) as i32, y.as_f64().unwrap_or(0.0) as i32),
        _ => return Err("impact: missing \"at\": [x, y] (world pixels)".into()),
    };
    let (dx, dy) = match v.get("dir").and_then(Value::as_array).map(Vec::as_slice) {
        Some([x, y]) => (x.as_f64().unwrap_or(0.0), y.as_f64().unwrap_or(0.0)),
        _ => (1.0, 0.0),
    };
    let len = (dx * dx + dy * dy).sqrt().max(1e-6);
    // `flight_dir`: the velocity in 1/64 px per tick.
    let speed = v.get("speed").and_then(Value::as_f64).unwrap_or(12.0) * 64.0;
    let dir = ((dx / len * speed).round() as i32, (dy / len * speed).round() as i32);
    let (cause, _) = cause(v.get("cause").and_then(Value::as_str).unwrap_or("cannonball"))?;
    let world = g.world_mut().ok_or("no world yet")?;
    acr_sim::combat::record_impact(world, at, dir, cause);
    // `"by": unit` with a shell attack: the shell's fragments fly (hit-locations rule).
    let by = v.get("by").and_then(Value::as_u64).map(|b| b as usize);
    // A mortar's shell draws a smaller burst and leaves no fire (rule `mortars`).
    let mortar = acr_sim::mortar::fired_by(by.and_then(|b| world.units.get(b)).map(|u| u.kind), &world.catalog);
    if let Some(i) = world.impacts.back_mut() {
        i.mortar = mortar;
    }
    let mut fragments = 0;
    if cause == DeathCause::Explosion
        && let Some(b) = by
    {
        let att = acr_sim::shrapnel::shell_attack(world, b).ok_or("impact: \"by\" has no shell attack")?;
        let before = world.fragments.len();
        acr_sim::shrapnel::burst(world, Some(b), att, at, 0);
        fragments = world.fragments.len() - before;
        // And the fire it leaves on dry ground (hit-locations rule, `ground_fire.rs`).
        let water = v.get("water").and_then(Value::as_bool).unwrap_or(false);
        acr_sim::ground_fire::shell_burst(world, Some(b), att, at, water);
    }
    // A ball of `by`'s cannon grazing there: the earth it throws up (hit-locations rule).
    if cause == DeathCause::Cannonball
        && let Some(b) = by
    {
        let att = acr_sim::shrapnel::attack_of(world, b, cause).ok_or("impact: \"by\" has no cannonball attack")?;
        let before = world.fragments.len();
        acr_sim::shrapnel::graze(world, Some(b), att, at, dir);
        fragments = world.fragments.len() - before;
    }
    Ok(json!({
        "tick": world.tick.0, "at": [at.0, at.1], "dir": [dir.0, dir.1], "cause": format!("{cause:?}"),
        "fragments": fragments,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_of_causes_regions_and_poses() {
        assert_eq!(cause("sabre"), Ok((DeathCause::Melee, true)));
        assert_eq!(cause("axe"), Ok((DeathCause::Thrown, false)));
        assert_eq!(part("arm_l"), Ok(Part::Limb(LimbId::ArmL)));
        assert!(part("tail").is_err());
        assert!(cause("laser").is_err());
        assert_eq!(maul("claw"), Some(body::Cause::Claw));
        assert_eq!(maul("musket"), None);
        assert_eq!(region("head"), Ok(HitRegion::Head));
        assert!(region("tail").is_err());
        assert_eq!(Pose::parse("crawling"), Ok(Pose::Crawl));
    }
}
