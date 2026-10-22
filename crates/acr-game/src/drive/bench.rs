//! The close-up bench (#277, `docs/testing.md`): soldiers placed where the test wants them,
//! scripted hits through the simulation's body path, and the camera close up.

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
                "unknown cause {other:?}: musket, arrow, cannonball, grapeshot, shell, sabre, bayonet or axe"
            ));
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
    let i = id.0 as usize;
    match pose {
        Pose::Stand => {}
        Pose::Walk => g.push_command(Command::Patrol { player: owner, units: vec![id], x: x + walk, y }),
        Pose::Crawl => {
            let leg = Injury { part: Part::Limb(LimbId::LegL), level: KNEE, outcome: Outcome::Off, energy: 0 };
            body::apply(&mut g.world_mut().ok_or("no world")?.units[i].body, &leg);
        }
    }
    let world = g.world().ok_or("no world")?;
    Ok(json!({"id": id.0, "type": world.catalog.get(kind).id, "pose": format!("{pose:?}").to_lowercase()}))
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
    let (cause, mounted) = cause(v.get("cause").and_then(Value::as_str).unwrap_or("musket"))?;
    let region = v.get("region").and_then(Value::as_str).map(region).transpose()?;
    let energy = v.get("energy").and_then(Value::as_u64).unwrap_or(100).min(100) as u8;
    let damage = v.get("damage").and_then(Value::as_i64).unwrap_or(1) as i32;
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
    let index = scripted_hit(world, unit, ScriptedHit { cause, mounted, region, dir, energy, dmg: damage })
        .ok_or_else(|| format!("unit {unit} is not alive"))?;
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
        })),
        "killed": h.killed,
        "life": world.units[unit].life,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_of_causes_regions_and_poses() {
        assert_eq!(cause("sabre"), Ok((DeathCause::Melee, true)));
        assert_eq!(cause("axe"), Ok((DeathCause::Thrown, false)));
        assert!(cause("laser").is_err());
        assert_eq!(region("head"), Ok(HitRegion::Head));
        assert!(region("tail").is_err());
        assert_eq!(Pose::parse("crawling"), Ok(Pose::Crawl));
    }
}
