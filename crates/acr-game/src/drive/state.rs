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
    if let Some(o) = g.free_orbit() {
        // The free camera's turn and tilt in degrees (Shift + right drag, `,`/`.`, Page Up/Down).
        m.insert("orbit".into(), json!({"yaw_deg": o.yaw.to_degrees(), "pitch_deg": o.pitch.to_degrees()}));
    }
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
    // Trees and stones struck by balls (rule `ball_terrain`), and the impacts kept.
    if !world.struck.is_empty() {
        let struck: Vec<Value> =
            world.struck.iter().map(|(k, s)| json!({"object": k, "struck": format!("{s:?}")})).collect();
        m.insert("struck".into(), struck.into());
    }
    m.insert(
        "impacts".into(),
        world.impacts.iter().rev().take(8).map(|i| json!([i.tick, i.pos.0, i.pos.1])).collect::<Vec<_>>().into(),
    );
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
                // Mortars rule: the zones ([x, y, r]) and whether it shells them.
                if let Some(m) = world.mortar_zones.get(&u.id) {
                    let zones: Vec<Value> = m.zones.iter().map(|z| json!([z.x, z.y, z.r])).collect();
                    o.insert("mortar".into(), json!({"zones": zones, "firing": m.firing}));
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

/// Builds the simulation command `v` describes (see [`build`]).
pub fn command(game: &Game, v: &Value) -> Result<Vec<Command>, String> {
    let Mode::InGame(g) = &game.mode else { return Err("no game".into()) };
    let world = g.world().ok_or("no world yet")?;
    build(world, g.player(), g.selection(), v)
}

/// The player a command is given for: `"player": n`, else the one owner of the objects it
/// names (`units`, else `officer` / `building`, else the selection), else the local player.
/// The sim drops units its player does not own (`World::owned`), so this lets a test order
/// either side with the same commands the HUD sends.
fn command_player(world: &World, local: u8, sel: &[EntityId], v: &Value) -> Result<u8, String> {
    if let Some(p) = v.get("player") {
        return p.as_u64().and_then(|p| u8::try_from(p).ok()).ok_or("player: 0..255".into());
    }
    let named = if v.get("units").is_some() {
        ids(world, v, "units", sel)?
    } else {
        let one: Vec<EntityId> = ["officer", "building"].iter().filter_map(|k| id(v, k).ok()).collect();
        if one.is_empty() { sel.to_vec() } else { one }
    };
    let mut owners = named.iter().filter_map(|&i| world.unit(i)).map(|u| u.owner);
    Ok(match owners.next() {
        Some(o) if owners.all(|p| p == o) => o,
        _ => local,
    })
}

fn dir(v: &Value) -> Result<u8, String> {
    v.get("dir").and_then(Value::as_u64).and_then(|d| u8::try_from(d).ok()).ok_or("missing \"dir\" (0..255)".into())
}

/// The commands for `face`: one `Face` per formation among `units` (any member stands for
/// its officer, `formation::face` turns the member's brigade), and for a free unit a
/// `MoveFacing` onto its own place (it stays and turns, 0x4ac249). Like any move, that ends
/// the unit's stand ground: face first, then stand ground.
fn face(world: &World, player: u8, units: &[EntityId], dir: u8) -> Vec<Command> {
    let mut brigades = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for u in units.iter().filter_map(|&i| world.unit(i)).filter(|u| u.living()) {
        match u.brigade {
            Some(b) => {
                if brigades.insert(b) {
                    out.push(Command::Face { player, officer: u.id, dir });
                }
            }
            None => {
                let (x, y) = (u.pos.x >> 4, u.pos.y >> 4);
                out.push(Command::MoveFacing { player, units: vec![u.id], x, y, dir });
            }
        }
    }
    out
}

/// Builds the simulation commands `v` describes; `player` is the local player, `sel` its
/// selection.
pub fn build(world: &World, player: u8, sel: &[EntityId], v: &Value) -> Result<Vec<Command>, String> {
    let player = command_player(world, player, sel, v)?;
    let kind = v.get("type").and_then(Value::as_str).ok_or("command: missing \"type\"")?;
    let units = || ids(world, v, "units", sel);
    let object_type = || find_type(world, v.get("kind").and_then(Value::as_str).ok_or("missing \"kind\"")?);
    let count = v.get("count").and_then(Value::as_u64).unwrap_or(1).max(1) as usize;
    let one = |c: Command| Ok(vec![c]);
    let on = || v.get("on").and_then(Value::as_bool).unwrap_or(true);
    match kind {
        "move" => one(Command::Move { player, units: units()?, x: int(v, "x")?, y: int(v, "y")? }),
        "attack_move" => one(Command::AttackMove { player, units: units()?, x: int(v, "x")?, y: int(v, "y")? }),
        "patrol" => one(Command::Patrol { player, units: units()?, x: int(v, "x")?, y: int(v, "y")? }),
        "stop" => one(Command::Stop { player, units: units()? }),
        "attack" => one(Command::Attack { player, units: units()?, target: id(v, "target")? }),
        "attack_ground" => one(Command::AttackGround { player, units: units()?, x: int(v, "x")?, y: int(v, "y")? }),
        "canister" => one(Command::Canister { player, units: units()? }),
        // Mortars rule: "zones": [[x, y, r], ...] (world px); none clears them.
        "mortar_zones" => {
            let zones = v
                .get("zones")
                .and_then(Value::as_array)
                .ok_or("mortar_zones: missing \"zones\" ([[x, y, r], ...])")?
                .iter()
                .map(|z| {
                    let n = |k: usize| z.get(k).and_then(Value::as_i64).map(|v| v as i32);
                    match (n(0), n(1), n(2)) {
                        (Some(x), Some(y), Some(r)) => Ok(acr_sim::mortar::Zone { x, y, r }),
                        _ => Err("mortar_zones: a zone is [x, y, r]".to_string()),
                    }
                })
                .collect::<Result<Vec<_>, _>>()?;
            one(Command::MortarZones { player, units: units()?, zones })
        }
        "mortar_fire" => one(Command::MortarFire {
            player,
            units: units()?,
            on: v.get("on").and_then(Value::as_bool).unwrap_or(true),
        }),
        // The HUD's `STANDGR` / `NOSTAND`, `DATTACK` / `EATTACK` and `ARMATTACK` / `ANYATTACK`
        // buttons (`interface.rs`); `on` defaults to true.
        "stand_ground" => one(Command::StandGround { player, units: units()?, on: on() }),
        "hold_fire" => one(Command::HoldFire { player, units: units()?, hold: on() }),
        "melee_only" => one(Command::MeleeOnly { player, units: units()?, on: on() }),
        // The right drag (`interface.rs` `right_release`): a move that ends facing `dir`.
        "move_facing" => {
            one(Command::MoveFacing { player, units: units()?, x: int(v, "x")?, y: int(v, "y")?, dir: dir(v)? })
        }
        "face" => match v.get("officer") {
            Some(_) => one(Command::Face { player, officer: id(v, "officer")?, dir: dir(v)? }),
            None => Ok(face(world, player, &units()?, dir(v)?)),
        },
        "friendly_fire" => one(Command::FriendlyFire {
            player,
            units: units()?,
            shoot_own: v.get("on").and_then(Value::as_bool).unwrap_or(true),
        }),
        "gather" => one(Command::Gather { player, units: units()?, x: int(v, "x")?, y: int(v, "y")? }),
        "repair" => one(Command::Repair { player, units: units()?, building: id(v, "building")? }),
        "enter" => one(Command::Enter { player, units: units()?, building: id(v, "building")? }),
        "leave" => {
            // One unit of `kind` leaves; without a kind everybody does.
            let kind = if v.get("kind").is_some() { Some(object_type()?) } else { None };
            one(Command::Leave { player, building: id(v, "building")?, kind })
        }
        "build" => {
            one(Command::Build { player, units: units()?, kind: object_type()?, x: int(v, "x")?, y: int(v, "y")? })
        }
        "produce" => {
            let (building, kind) = (id(v, "building")?, object_type()?);
            Ok((0..count).map(|_| Command::Produce { player, building, kind, endless: false }).collect())
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use acr_formats::defs::{OrderGroups, OrderList, UnitDef};
    use acr_sim::catalog::{Catalog, ObjectType};
    use acr_sim::formation::Formations;
    use acr_sim::map::MapGrid;
    use acr_sim::{Pos, TypeId};

    use super::*;

    // The pikeman of `acr-sim/tests/states.rs`; `ARMATTACK` lets it take the melee-only switch.
    const PIKEMAN: &str = "NAME Pik\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n\
        @MOTION_R 9 0 10 19\r\n@PATTACK 9 0 0 35\r\n@UATTACK 9 0 35 0\r\n@ATTACK 9 0 0 20\r\n\
        @DEATH 9 0 0 39\r\n#DEATHLIE1 9 1 0 39\r\nLIFE 100\r\nGEOMETRY 1 10 40\r\nDAMAGE 0 10\r\n\
        ATTACK_RADIUS 0 40 100\r\nATTACK_PAUSE 0 1\r\nSEARCH_ENEMY_RADIUS 700\r\nSEARCH_ENEMY_RADIUS_SHIFT 70\r\n\
        DET_RADIUS 0 0 7000\r\nMATHERIAL 1 BODY\r\nCANKILL 1 BODY\r\nARMATTACK\r\n[END]\r\n";
    const OFFICER: &str = "NAME Off\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n\
        @MOTION_R 9 0 10 19\r\nLIFE 100\r\nGEOMETRY 1 10 40\r\nOFFICER\r\nMATHERIAL 1 BODY\r\n[END]\r\n";
    const ORDERS: &[u8] =
        b"#LINE15 SYM2 19 2 9 3 1 2\r\n   C   C   C\r\n      \r\n***************\r\n#END\r\n#EXIT\r\n";
    const GROUPS: &[u8] = b"1\r\nLINE 0 1 #LINE15\r\n";

    fn setup() -> (World, TypeId, TypeId) {
        let mut c = Catalog::new();
        let pik = c.add(ObjectType::from_def("Pik(EN)", &UnitDef::parse(PIKEMAN.as_bytes())));
        let off = c.add(ObjectType::from_def("Off(EN)", &UnitDef::parse(OFFICER.as_bytes())));
        let f = Formations::new(&OrderList::parse(ORDERS), &OrderGroups::parse(GROUPS));
        (World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(f)), pik, off)
    }

    /// The driver's command `json` as the local player 0 with nothing selected, applied.
    fn order(w: &mut World, json: Value) -> Vec<Command> {
        let c = build(w, 0, &[], &json).unwrap();
        w.step(&c);
        c
    }

    fn raw(ids: &[EntityId]) -> Vec<u32> {
        ids.iter().map(|i| i.0).collect()
    }

    fn run(w: &mut World, ticks: u32) {
        for _ in 0..ticks {
            w.step(&[]);
        }
    }

    #[test]
    fn the_orders_parse_into_the_huds_commands_for_either_side() {
        let (mut w, pik, _) = setup();
        let a = w.spawn(pik, 0, Pos::from_pixels(2000, 2000), 0);
        let b = w.spawn(pik, 1, Pos::from_pixels(2300, 2000), 128);
        let cmd = |j: Value| build(&w, 0, &[a], &j).unwrap();
        assert_eq!(
            cmd(json!({"type": "stand_ground", "units": [b.0]})),
            [Command::StandGround { player: 1, units: vec![b], on: true }]
        );
        assert_eq!(
            cmd(json!({"type": "hold_fire", "units": {"owner": 1}, "on": false})),
            [Command::HoldFire { player: 1, units: vec![b], hold: false }]
        );
        assert_eq!(cmd(json!({"type": "melee_only"})), [Command::MeleeOnly { player: 0, units: vec![a], on: true }]);
        assert_eq!(
            cmd(json!({"type": "move_facing", "units": [a.0], "x": 10, "y": 20, "dir": 64})),
            [Command::MoveFacing { player: 0, units: vec![a], x: 10, y: 20, dir: 64 }]
        );
        assert_eq!(
            cmd(json!({"type": "face", "units": [b.0], "dir": 0})),
            [Command::MoveFacing { player: 1, units: vec![b], x: 2300, y: 2000, dir: 0 }]
        );
        // `player` wins; mixed owners fall back to the local player.
        assert_eq!(
            cmd(json!({"type": "stop", "units": [b.0], "player": 0})),
            [Command::Stop { player: 0, units: vec![b] }]
        );
        assert_eq!(cmd(json!({"type": "stop", "units": [a.0, b.0]})), [Command::Stop { player: 0, units: vec![a, b] }]);
        assert!(build(&w, 0, &[], &json!({"type": "face", "units": [a.0]})).is_err(), "face needs a dir");
        assert!(build(&w, 0, &[], &json!({"type": "face", "units": [a.0], "dir": 256})).is_err());
    }

    #[test]
    fn a_standing_enemy_does_not_walk_into_melee() {
        let (mut w, pik, _) = setup();
        let a = w.spawn(pik, 0, Pos::from_pixels(2000, 2000), 0);
        let b = w.spawn(pik, 1, Pos::from_pixels(2300, 2000), 128);
        order(&mut w, json!({"type": "hold_fire", "units": [a.0]}));
        order(&mut w, json!({"type": "stand_ground", "units": [b.0]}));
        let start = w.unit(b).unwrap().pos;
        run(&mut w, 400);
        let u = w.unit(b).unwrap();
        assert!(u.stand_ground);
        assert_eq!(u.pos, start, "owner 1's standing pikeman stayed");
        assert_eq!((w.unit(a).unwrap().life, u.life), (100, 100));
    }

    #[test]
    fn units_holding_fire_do_not_attack() {
        let (mut w, pik, _) = setup();
        let a = w.spawn(pik, 0, Pos::from_pixels(2000, 2000), 0);
        let b = w.spawn(pik, 1, Pos::from_pixels(2150, 2000), 128);
        order(&mut w, json!({"type": "hold_fire", "units": {"owner": 0}}));
        order(&mut w, json!({"type": "hold_fire", "units": {"owner": 1}}));
        run(&mut w, 300);
        assert_eq!((w.unit(a).unwrap().life, w.unit(b).unwrap().life), (100, 100), "nobody fought");
        assert!(w.unit(b).unwrap().no_search_victim);
        // Without the order they fight.
        order(&mut w, json!({"type": "hold_fire", "units": [b.0], "on": false}));
        run(&mut w, 400);
        assert!(w.unit(a).unwrap().life < 100, "b attacked once free to");
    }

    #[test]
    fn melee_only_and_face_take_effect() {
        let (mut w, pik, off) = setup();
        let free = w.spawn(pik, 1, Pos::from_pixels(3000, 3000), 0);
        order(&mut w, json!({"type": "melee_only", "units": [free.0]}));
        assert!(w.unit(free).unwrap().melee_only);
        order(&mut w, json!({"type": "face", "units": [free.0], "dir": 128}));
        run(&mut w, 60);
        let u = w.unit(free).unwrap();
        assert_eq!((u.dir, u.pos), (128, Pos::from_pixels(3000, 3000)), "turned in place");

        let officer = w.spawn(off, 1, Pos::from_pixels(1500, 1000), 0);
        let men: Vec<EntityId> =
            (0..15).map(|i| w.spawn(pik, 1, Pos::from_pixels(1400 + 20 * (i % 5), 1100 + 20 * (i / 5)), 0)).collect();
        let c = order(&mut w, json!({"type": "form", "officer": officer.0, "units": raw(&men), "kind": "LINE"}));
        assert_eq!(c[0], Command::Form { player: 1, officer, units: men.clone(), kind: "LINE".into() });
        let b = usize::from(w.unit(men[0]).unwrap().brigade.expect("formed"));
        // Any member stands for the formation: one `Face`.
        let c = order(&mut w, json!({"type": "face", "units": raw(&men), "dir": 64}));
        assert_eq!(c, [Command::Face { player: 1, officer: men[0], dir: 64 }]);
        assert_eq!(w.brigades[b].dir, 64);
    }
}
