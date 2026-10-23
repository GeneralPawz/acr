//! The game state the driver reports (`state`), the conditions `wait` checks and the
//! simulation commands `command` pushes.

use std::collections::BTreeMap;

use acr_sim::world::EntityId;
use acr_sim::{Command, TypeId, Unit, World};
use serde_json::{Map, Value, json};

use super::protocol::{Cond, normalize};
use super::tree;
use crate::{Game, Mode};

const RESOURCES: [&str; 6] = ["wood", "gold", "stone", "food", "iron", "coal"];

pub fn mode_name(game: &Game) -> &'static str {
    match &game.mode {
        Mode::Intro => "intro",
        Mode::Menus => "menus",
        Mode::InGame(g) if g.ready() => "game",
        Mode::InGame(_) => "loading",
    }
}

/// The running game's world, if it has one.
pub fn world(game: &Game) -> Option<&World> {
    match &game.mode {
        Mode::InGame(g) => g.world(),
        _ => None,
    }
}

pub fn tick(game: &Game) -> Option<u64> {
    world(game).map(|w| w.tick.0)
}

fn finished(world: &World, u: &Unit) -> bool {
    u.building && u.stage >= world.max_stage(u)
}

fn type_name(world: &World, u: &Unit) -> String {
    world.catalog.get(u.kind).id.clone()
}

fn kind_matches(world: &World, u: &Unit, kind: &str) -> bool {
    world.catalog.get(u.kind).id.to_lowercase().contains(&kind.to_lowercase())
}

/// The player's formations: brigade index and living members.
fn formations(world: &World, player: u8) -> BTreeMap<u16, u32> {
    let mut out = BTreeMap::new();
    for u in world.units.iter().filter(|u| u.living() && u.owner == player) {
        if let Some(b) = u.brigade {
            *out.entry(b).or_default() += 1;
        }
    }
    out
}

/// `state`: mode, tick, the player's resources and selection, objects per owner and type,
/// the mission's end; with `units`, every object (of `owner`, of a type containing `kind`);
/// with `detail`, also what each one is doing and a summary of the listed units' activity.
pub fn state(
    game: &Game,
    units: bool,
    owner: Option<u8>,
    visible_only: bool,
    detail: bool,
    kind: Option<&str>,
) -> Value {
    let mut v = json!({"mode": mode_name(game), "screen": tree::screen_name(game)});
    let m = v.as_object_mut().unwrap();
    let Mode::InGame(g) = &game.mode else { return v };
    let Some(world) = g.world() else { return v };
    let me = g.player();
    let player = &world.players[usize::from(me)];
    let resources: Map<String, Value> =
        RESOURCES.iter().enumerate().map(|(i, n)| ((*n).to_owned(), player.resources[i].into())).collect();
    m.insert("tick".into(), world.tick.0.into());
    m.insert("player".into(), me.into());
    m.insert("resources".into(), resources.into());
    m.insert("population".into(), player.population.into());
    m.insert("housing".into(), player.housing.into());
    let selection: Vec<Value> = g
        .selection()
        .iter()
        .filter_map(|&id| world.unit(id))
        .map(|u| json!({"id": u.id.0, "type": type_name(world, u)}))
        .collect();
    m.insert("selection".into(), selection.into());
    let brigades: Vec<Value> =
        formations(world, me).into_iter().map(|(b, n)| json!({"brigade": b, "members": n})).collect();
    m.insert("formations".into(), brigades.into());
    // Counts per owner: units, finished buildings, building sites and burning objects by type.
    let mut counts: BTreeMap<u8, [BTreeMap<String, u32>; 4]> = BTreeMap::new();
    for u in world.units.iter().filter(|u| u.living()) {
        if visible_only && u.owner != me && !g.sees(u.pos.x >> 4, u.pos.y >> 4) {
            continue;
        }
        let group = if !u.building {
            0
        } else if finished(world, u) {
            1
        } else {
            2
        };
        let c = counts.entry(u.owner).or_default();
        *c[group].entry(type_name(world, u)).or_default() += 1;
        if u.burning {
            *c[3].entry(type_name(world, u)).or_default() += 1;
        }
    }
    let counts: Map<String, Value> = counts
        .into_iter()
        .map(|(o, [u, b, s, f])| (o.to_string(), json!({"units": u, "buildings": b, "sites": s, "burning": f})))
        .collect();
    m.insert("counts".into(), counts.into());
    let ended = g.mission_result().map(|w| if w { "won" } else { "lost" });
    m.insert("mission".into(), json!({"ended": ended, "objectives": g.has_objectives()}));
    if units {
        let window = g.window_size_px();
        let list: Vec<Value> = world
            .units
            .iter()
            .filter(|u| u.living() && owner.is_none_or(|o| u.owner == o))
            .filter(|u| kind.is_none_or(|k| kind_matches(world, u, k)))
            .filter(|u| !visible_only || u.owner == me || g.sees(u.pos.x >> 4, u.pos.y >> 4))
            .map(|u| {
                let mut e = json!({
                    "id": u.id.0, "type": type_name(world, u), "owner": u.owner,
                    "x": u.pos.x >> 4, "y": u.pos.y >> 4, "life": u.life,
                });
                let o = e.as_object_mut().unwrap();
                if u.building {
                    o.insert("building".into(), true.into());
                    o.insert("finished".into(), finished(world, u).into());
                }
                if u.inside_of.is_some() {
                    o.insert("inside".into(), true.into());
                }
                if u.burning {
                    o.insert("burning".into(), true.into());
                }
                if let Some(b) = u.brigade {
                    o.insert("brigade".into(), b.into());
                }
                if detail {
                    o.insert("dir".into(), u.dir.into());
                    o.insert(
                        "orders".into(),
                        u.orders.iter().rev().map(|o| format!("{o:?}")).collect::<Vec<_>>().into(),
                    );
                    o.insert("anim".into(), format!("{:?}", u.anim).into());
                    o.insert("frame".into(), json!([u.frame, u.anim_frames]));
                    o.insert("stance".into(), json!([u.stance, u.local_stance, u.ground_state]));
                    o.insert("delay".into(), json!([u.delay, u.max_delay]));
                    o.insert("moving".into(), u.in_motion.into());
                    let flags: Vec<&str> = [
                        (u.stand_ground, "stand_ground"),
                        (u.melee_only, "melee_only"),
                        (u.no_ff_avoid, "friendly_fire"),
                        (u.no_search_victim, "no_search"),
                    ]
                    .into_iter()
                    .filter_map(|(on, name)| on.then_some(name))
                    .collect();
                    if !flags.is_empty() {
                        o.insert("flags".into(), flags.into());
                    }
                }
                let b = g.unit_window_box(u);
                let c = [b[0] + b[2] / 2.0, b[1] + b[3] / 2.0];
                if c[0] >= 0.0 && c[1] >= 0.0 && c[0] < window[0] && c[1] < window[1] {
                    o.insert("screen".into(), json!([c[0].round(), c[1].round()]));
                }
                e
            })
            .collect();
        if detail {
            let ids: Vec<u32> = list.iter().filter_map(|e| e.get("id")?.as_u64()).map(|id| id as u32).collect();
            m.insert("activity".into(), activity(world, &ids));
        }
        m.insert("units".into(), list.into());
    }
    v
}

/// What the listed units are doing: `attacking` (an attack is their current order), `aside`
/// (a move in front of an attack they will resume: stepping aside to shoot, walking around),
/// `idle` (no orders) and `reloading`.
fn activity(world: &World, ids: &[u32]) -> Value {
    use acr_sim::world::Order;
    let attack = |o: &Order| matches!(o, Order::Attack { .. } | Order::AttackPoint { .. });
    let moving = |o: &Order| matches!(o, Order::SendTo { .. } | Order::SmartSendTo { .. });
    let (mut attacking, mut aside, mut idle, mut reloading) = (0, 0, 0, 0);
    for u in ids.iter().filter_map(|&id| world.unit(EntityId(id))) {
        match u.orders.split_last() {
            None => idle += 1,
            Some((cur, _)) if attack(cur) => attacking += 1,
            Some((cur, below)) if moving(cur) && below.iter().any(attack) => aside += 1,
            Some(_) => {}
        }
        if u.delay > 0 {
            reloading += 1;
        }
    }
    json!({"units": ids.len(), "attacking": attacking, "aside": aside, "idle": idle, "reloading": reloading})
}

/// Whether a `wait` condition holds.
pub fn check(game: &Game, cond: &Cond) -> bool {
    let top_built = tree::top(game).is_none_or(|(_, b)| b);
    match cond {
        Cond::Screen(name) => top_built && tree::screen_name(game).eq_ignore_ascii_case(name),
        Cond::Dialog(None) => tree::modal(game).is_some(),
        Cond::Dialog(Some(name)) => {
            game.ui.dialog_open() && tree::top(game).is_some_and(|(s, b)| b && s.name().eq_ignore_ascii_case(name))
        }
        Cond::NoDialog => tree::modal(game).is_none(),
        Cond::InGame => matches!(&game.mode, Mode::InGame(g) if g.ready()),
        Cond::Menus => matches!(game.mode, Mode::Menus) && top_built,
        Cond::Label(text) => {
            let want = normalize(text);
            tree::items(game, false).iter().any(|i| normalize(&i.label).contains(&want))
                || tree::messages(game).iter().any(|t| normalize(t).contains(&want))
        }
        Cond::Tick(t) => tick(game).is_some_and(|now| now >= *t),
        Cond::Units { owner, kind, building, finished: done, min } => {
            let Some(world) = world(game) else { return false };
            world
                .units
                .iter()
                .filter(|u| u.living())
                .filter(|u| owner.is_none_or(|o| u.owner == o))
                .filter(|u| kind.as_deref().is_none_or(|k| kind_matches(world, u, k)))
                .filter(|u| building.is_none_or(|b| u.building == b))
                .filter(|u| done.is_none_or(|f| finished(world, u) == f))
                .count()
                >= *min
        }
        Cond::Formations(n) => match &game.mode {
            Mode::InGame(g) => g.world().is_some_and(|w| formations(w, g.player()).len() >= *n),
            _ => false,
        },
        Cond::Selected(n) => matches!(&game.mode, Mode::InGame(g) if g.selection().len() >= *n),
        Cond::Ended(won) => match &game.mode {
            Mode::InGame(g) => g.mission_result().is_some_and(|r| won.is_none_or(|w| w == r)),
            _ => false,
        },
    }
}

/// A type by its id (`PEASANT(EN)`), exactly or ignoring case.
fn find_type(world: &World, name: &str) -> Result<TypeId, String> {
    world
        .catalog
        .find(name)
        .or_else(|| world.catalog.iter().find(|(_, t)| t.id.eq_ignore_ascii_case(name)).map(|(id, _)| id))
        .ok_or_else(|| format!("no object type {name:?}"))
}

/// The units of a command: a list of ids, `"selection"` (the default) or a filter
/// `{"owner": n, "type": "Mus"}` for every living object that matches.
fn ids(world: &World, v: &Value, key: &str, selection: &[EntityId]) -> Result<Vec<EntityId>, String> {
    match v.get(key) {
        None => Ok(selection.to_vec()),
        Some(Value::String(s)) if s == "selection" => Ok(selection.to_vec()),
        Some(Value::Array(a)) => {
            a.iter().map(|x| x.as_u64().map(|n| EntityId(n as u32)).ok_or(format!("{key}: ids"))).collect()
        }
        Some(Value::Object(f)) => {
            let owner = f.get("owner").and_then(Value::as_u64);
            let kind = f.get("type").and_then(Value::as_str);
            Ok(world
                .units
                .iter()
                .filter(|u| u.living() && owner.is_none_or(|o| u64::from(u.owner) == o))
                .filter(|u| kind.is_none_or(|k| kind_matches(world, u, k)))
                .map(|u| u.id)
                .collect())
        }
        Some(_) => Err(format!("{key}: a list of ids, \"selection\" or {{\"owner\", \"type\"}}")),
    }
}

fn id(v: &Value, key: &str) -> Result<EntityId, String> {
    v.get(key).and_then(Value::as_u64).map(|n| EntityId(n as u32)).ok_or(format!("missing {key:?}"))
}

fn int(v: &Value, key: &str) -> Result<i32, String> {
    v.get(key).and_then(Value::as_i64).map(|n| n as i32).ok_or(format!("missing {key:?}"))
}

/// Builds the simulation command `v` describes for the local player.
pub fn command(game: &Game, v: &Value) -> Result<Vec<Command>, String> {
    let Mode::InGame(g) = &game.mode else { return Err("no game".into()) };
    let world = g.world().ok_or("no world yet")?;
    let player = g.player();
    let sel = g.selection();
    let kind = v.get("type").and_then(Value::as_str).ok_or("command: missing \"type\"")?;
    let units = || ids(world, v, "units", sel);
    let object_type = || find_type(world, v.get("kind").and_then(Value::as_str).ok_or("missing \"kind\"")?);
    let count = v.get("count").and_then(Value::as_u64).unwrap_or(1).max(1) as usize;
    let one = |c: Command| Ok(vec![c]);
    match kind {
        "move" => one(Command::Move { player, units: units()?, x: int(v, "x")?, y: int(v, "y")? }),
        "attack_move" => one(Command::AttackMove { player, units: units()?, x: int(v, "x")?, y: int(v, "y")? }),
        "patrol" => one(Command::Patrol { player, units: units()?, x: int(v, "x")?, y: int(v, "y")? }),
        "stop" => one(Command::Stop { player, units: units()? }),
        "attack" => one(Command::Attack { player, units: units()?, target: id(v, "target")? }),
        "friendly_fire" => one(Command::FriendlyFire {
            player,
            units: units()?,
            shoot_own: v.get("on").and_then(Value::as_bool).unwrap_or(true),
        }),
        "gather" => one(Command::Gather { player, units: units()?, x: int(v, "x")?, y: int(v, "y")? }),
        "repair" => one(Command::Repair { player, units: units()?, building: id(v, "building")? }),
        "enter" => one(Command::Enter { player, units: units()?, building: id(v, "building")? }),
        "leave" => one(Command::Leave { player, building: id(v, "building")? }),
        "build" => {
            one(Command::Build { player, units: units()?, kind: object_type()?, x: int(v, "x")?, y: int(v, "y")? })
        }
        "produce" => {
            let (building, kind) = (id(v, "building")?, object_type()?);
            Ok((0..count).map(|_| Command::Produce { player, building, kind }).collect())
        }
        "rally" => one(Command::Rally { player, building: id(v, "building")?, x: int(v, "x")?, y: int(v, "y")? }),
        "form" => one(Command::Form {
            player,
            officer: id(v, "officer")?,
            units: units()?,
            kind: v.get("kind").and_then(Value::as_str).ok_or("missing \"kind\"")?.to_owned(),
        }),
        "disband" => one(Command::Disband { player, officer: id(v, "officer")? }),
        other => Err(format!("unknown command type {other:?}")),
    }
}
