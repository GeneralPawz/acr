//! Registration, texts and pages, questions, the end of the game, players, difficulty,
//! random numbers, light spots and the camera (`ActiveScenary.cpp` in the original).

use acr_script::runtime::{Error, Result};

use super::call::magic;
use super::{Api, ApiFn, Call, Caller, Status};
use crate::events::MissionEvent;
use crate::state::{Group, LIGHT_SPOTS};

/// Returned through the interpreter when a question has no answer yet; the runtime rolls the
/// script call back and waits for the player (see `Scripts::answer`).
pub const QUESTION_PENDING: &str = "question pending";

const fn done(name: &'static str, original: u32, handler: super::Handler) -> ApiFn {
    ApiFn { name, original, status: Status::Done, handler }
}

pub const FUNCTIONS: &[ApiFn] = &[
    done("RegisterUnitType", 0x55be77, register_unit_type),
    done("RegisterUpgrade", 0x55bf0c, register_upgrade),
    done("RegisterDynGroup", 0x56780c, register_dyn_group),
    done("RegisterDynGroupEx", 0x5678e9, register_dyn_group),
    ApiFn {
        name: "RegisterVar",
        original: 0x55bac8,
        status: Status::Partial("recorded for savegames (#78); the engine never touches the variable otherwise"),
        handler: register_var,
    },
    done("RegisterZone", 0x55bb53, register_zone),
    done("RegisterVisibleZone", 0x55bce5, register_visible_zone),
    done("RegisterUnits", 0x55b73d, register_units),
    done("RegisterFormation", 0x55c67c, register_formation),
    done("GetDiff", 0x56957d, get_diff),
    done("GetDifficulty", 0x569555, get_difficulty),
    done("RandNew", 0x584891, rand_new),
    done("GetRND", 0x570700, get_rnd),
    done("ShowPage", 0x56255a, show_page),
    done("ShowPageParam", 0x56261d, show_page_param),
    done("ShowCentralText", 0x5624a6, show_central_text),
    done("ShowVictory", 0x5623df, show_victory),
    done("LooseGame", 0x562456, loose_game),
    done("AskQuestion", 0x562765, ask_question),
    done("AskQuestionParam", 0x562868, ask_question_param),
    done("AskMultilineQuestion", 0x5662de, ask_multiline_question),
    done("EnableMission", 0x562318, enable_mission),
    done("DisableMission", 0x5622fb, disable_mission),
    done("MissErrorMessage", 0x56bc22, miss_error_message),
    done("?GetTextByID@@YAPADPAD@Z", 0x60a020, get_text_by_id),
    done("?AssignHint@@YAXPADH@Z", 0x494283, assign_hint),
    done("?AssignHint1@@YAXPADH@Z", 0x4942db, assign_hint1),
    ApiFn { name: "SetPlayerName", original: 0x5697eb, status: Status::Done, handler: set_player_name },
    ApiFn { name: "ChangeFriends", original: 0x561df9, status: Status::Done, handler: change_friends },
    ApiFn {
        name: "StartAI",
        original: 0x565a1a,
        status: Status::Partial("the AI DLL is loaded by the runtime (Scripts::ai_requests); island research not done"),
        handler: start_ai,
    },
    done("GetMyNation", 0x5658b6, get_my_nation),
    done("GetAINation", 0x56827b, get_ai_nation),
    ApiFn { name: "NationIsErased", original: 0x5695ce, status: Status::Done, handler: nation_is_erased },
    ApiFn { name: "SetLightSpot", original: 0x564017, status: Status::Done, handler: set_light_spot },
    ApiFn { name: "ClearLightSpot", original: 0x564121, status: Status::Done, handler: clear_light_spot },
    ApiFn { name: "GetFogSpotVisibility", original: 0x56e6a4, status: Status::Done, handler: get_fog_spot_visibility },
    done("SetStartPoint", 0x564192, set_start_point),
    done("GetPeaceTimeLeft", 0x569882, get_peace_time_left),
    done("GetLandType", 0x5695ae, get_land_type),
];

// --- registration ------------------------------------------------------------------------

/// `bool RegisterUnitType(GAMEOBJ*, char* name)`: exact (case-sensitive) match against the
/// unit ids; the index is the type index shared by all nations.
fn register_unit_type(api: &mut Api, c: &mut Call) -> Result<u32> {
    let name = c.str(1)?;
    let found = api.world.catalog.find(&name).filter(|&t| api.world.catalog.get(t).id == name);
    match found {
        Some(t) => {
            c.set_handle(c.u32(0)?, t.0, 0, magic::UNIT_TYPE)?;
            Ok(1)
        }
        None => {
            api.st.error(format!("RegisterUnitType : Unknown unit type : {name}"));
            Ok(0)
        }
    }
}

fn register_upgrade(api: &mut Api, c: &mut Call) -> Result<u32> {
    let name = c.str(1)?;
    let Some(i) = api.upgrade_named(&name) else {
        api.st.error(format!("RegisterUpgrade : Unknown upgrade : {name}"));
        return Ok(0);
    };
    c.set_handle(c.u32(0)?, i, 0, magic::UPGRADE)?;
    Ok(1)
}

/// `void RegisterDynGroup(GAMEOBJ*)` / `RegisterDynGroupEx(GAMEOBJ*, file, line)`: a new empty
/// group (the extra arguments are `__FILE__`/`__LINE__`, debug only).
fn register_dyn_group(api: &mut Api, c: &mut Call) -> Result<u32> {
    let i = api.st.groups.len();
    api.st.groups.push(Group::default());
    c.set_handle(c.u32(0)?, i as u16, 0, magic::GROUP)?;
    Ok(0)
}

fn register_var(api: &mut Api, c: &mut Call) -> Result<u32> {
    api.st.vars.push((api.vm, c.u32(0)?, c.u32(1)?));
    Ok(0)
}

/// All map zones with the name form a new zone group.
fn register_zone_as(api: &mut Api, c: &mut Call, visible: bool) -> Result<u32> {
    let name = c.str(1)?;
    let ids: Vec<usize> =
        api.st.zones.iter().enumerate().filter(|(_, z)| z.name == name).map(|(i, _)| i).take(64).collect();
    if ids.is_empty() {
        api.st.error(format!("RegisterZone : Unknown zone name : {name}"));
        return Ok(0);
    }
    for &i in &ids {
        api.st.zones[i].visible = visible;
    }
    let g = api.st.zone_groups.len();
    api.st.zone_groups.push(ids);
    c.set_handle(c.u32(0)?, g as u16, 0, magic::ZONE)?;
    Ok(1)
}

fn register_zone(api: &mut Api, c: &mut Call) -> Result<u32> {
    register_zone_as(api, c, false)
}

fn register_visible_zone(api: &mut Api, c: &mut Call) -> Result<u32> {
    register_zone_as(api, c, true)
}

/// `bool RegisterUnits(GAMEOBJ*, char* name)`: a snapshot of a map group's members; an
/// unknown name sets the type to 0 without an error message.
fn register_units(api: &mut Api, c: &mut Call) -> Result<u32> {
    let name = c.str(1)?;
    let h = c.u32(0)?;
    let Some(mg) = api.st.map_groups.iter().find(|g| g.name == name) else {
        c.write_u32(h + 4, 0)?;
        return Ok(0);
    };
    let members = mg.members.clone();
    let i = api.st.groups.len();
    api.st.groups.push(Group { members, ..Group::default() });
    c.set_handle(h, i as u16, 0, magic::GROUP)?;
    Ok(1)
}

/// `bool RegisterFormation(GAMEOBJ*, char* name)`: a formation of `ORDERS.LST` (`#ODIN`).
fn register_formation(api: &mut Api, c: &mut Call) -> Result<u32> {
    let name = c.str(1)?;
    match api.world.formations.shapes.iter().position(|s| s.name == name) {
        Some(i) => {
            c.set_handle(c.u32(0)?, i as u16, 0, magic::FORMATION)?;
            Ok(1)
        }
        None => {
            api.st.error(format!("RegisterFormation : Unknown formation type :{name}  (see ORDERS.LST)"));
            Ok(0)
        }
    }
}

// --- difficulty, random numbers ------------------------------------------------------------

/// `int GetDiff(byte nation)`: `CITY[n].Difficulty` (no range check in the original).
fn get_diff(api: &mut Api, c: &mut Call) -> Result<u32> {
    Ok(u32::from(api.st.difficulty[usize::from(c.u8(0)? & 7)]))
}

/// `int GetDifficulty()`: difficulty of the AI being run.
fn get_difficulty(api: &mut Api, _: &mut Call) -> Result<u32> {
    let n = match api.caller {
        Caller::Ai(n) => n,
        _ => 0,
    };
    Ok(u32::from(api.st.difficulty[usize::from(n & 7)]))
}

/// `int RandNew(char* file, int line)`: the synchronised random table (`rando`, 0..32767);
/// scripts draw from the same sequence as the simulation.
fn rand_new(api: &mut Api, _: &mut Call) -> Result<u32> {
    Ok(api.world.random.rando() as u32)
}

/// `int GetRND(int n)`: `rando() * n >> 15`.
fn get_rnd(api: &mut Api, c: &mut Call) -> Result<u32> {
    let n = c.i32(0)?;
    Ok((api.world.random.rando().wrapping_mul(n) >> 15) as u32)
}

// --- pages, texts, questions ---------------------------------------------------------------

/// The pages of the mission text file with this id (the id includes `#`; every match is
/// shown, as the original loops over all pages).
fn pages(api: &Api, id: &str) -> Vec<(String, Vec<u8>)> {
    let key = id.strip_prefix('#').unwrap_or(id);
    api.st.texts.entries.iter().filter(|e| e.id == key).map(|e| (e.header.clone(), e.body.clone())).collect()
}

fn show_page(api: &mut Api, c: &mut Call) -> Result<u32> {
    let id = c.str(0)?;
    let found = pages(api, &id);
    if found.is_empty() {
        api.st.error(format!("ShowPage : Unknown page: {id}"));
    }
    for (header, text) in found {
        api.events.push(MissionEvent::Page { id: id.clone(), header, text });
    }
    Ok(0)
}

/// `void ShowPageParam(char* id, ...)`: the page text is a `printf` format for the variadic
/// arguments that follow on the guest stack.
fn show_page_param(api: &mut Api, c: &mut Call) -> Result<u32> {
    let id = c.str(0)?;
    let found = pages(api, &id);
    if found.is_empty() {
        api.st.error(format!("ShowPage : Unknown page: {id}"));
    }
    for (header, text) in found {
        let text = sprintf(c, &text, 1)?;
        api.events.push(MissionEvent::Page { id: id.clone(), header, text });
    }
    Ok(0)
}

/// Formats `fmt` like MSVC's `vsprintf` with the arguments from stack slot `first` on.
pub fn sprintf(c: &Call, fmt: &[u8], first: u32) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut arg = first;
    let mut i = 0;
    while i < fmt.len() {
        let b = fmt[i];
        i += 1;
        if b != b'%' {
            out.push(b);
            continue;
        }
        let start = i;
        while i < fmt.len() && b"-+ #0123456789.lhI".contains(&fmt[i]) {
            i += 1;
        }
        let Some(&conv) = fmt.get(i) else { break };
        i += 1;
        let spec = &fmt[start..i - 1];
        let left = spec.contains(&b'-');
        let zero = spec.first() == Some(&b'0');
        let width: usize = spec
            .iter()
            .skip_while(|b| b"-+ #0".contains(b))
            .take_while(|b| b.is_ascii_digit())
            .fold(0, |w, d| w * 10 + usize::from(d - b'0'));
        let body: Vec<u8> = match conv {
            b'%' => b"%".to_vec(),
            b'd' | b'i' => {
                arg += 1;
                (c.u32(arg - 1)? as i32).to_string().into_bytes()
            }
            b'u' => {
                arg += 1;
                c.u32(arg - 1)?.to_string().into_bytes()
            }
            b'x' => {
                arg += 1;
                format!("{:x}", c.u32(arg - 1)?).into_bytes()
            }
            b'X' => {
                arg += 1;
                format!("{:X}", c.u32(arg - 1)?).into_bytes()
            }
            b'c' => {
                arg += 1;
                vec![c.u8(arg - 1)?]
            }
            b's' => {
                arg += 1;
                c.bytes(arg - 1)?
            }
            other => vec![b'%', other],
        };
        let pad = width.saturating_sub(body.len());
        if left {
            out.extend_from_slice(&body);
            out.extend(std::iter::repeat_n(b' ', pad));
        } else {
            out.extend(std::iter::repeat_n(if zero { b'0' } else { b' ' }, pad));
            out.extend_from_slice(&body);
        }
    }
    Ok(out)
}

/// `void ShowCentralText(char* id, int time)`: a banner for `time` (−16 per script call).
fn show_central_text(api: &mut Api, c: &mut Call) -> Result<u32> {
    let text = api.text(&c.bytes(0)?);
    let time = c.i32(1)?;
    api.st.central_text = Some((text.clone(), time));
    api.events.push(MissionEvent::CentralText { text, ticks: (time.max(0) / 2) as u32 });
    Ok(0)
}

/// `void ShowVictory()`: the game stops at once and shows "WINNER".
fn show_victory(api: &mut Api, _: &mut Call) -> Result<u32> {
    if !api.st.victory {
        api.st.victory = true;
        api.events.push(MissionEvent::Victory);
    }
    Ok(0)
}

/// `void LooseGame()`: the game stops at once and shows "LOOSER".
fn loose_game(api: &mut Api, _: &mut Call) -> Result<u32> {
    if !api.st.defeat {
        api.st.defeat = true;
        api.events.push(MissionEvent::Defeat);
    }
    Ok(0)
}

/// The answer to the next question of this call, or a rollback request.
fn next_answer(api: &mut Api) -> Result<Option<u32>> {
    let i = api.st.answers_used;
    api.st.answers_used += 1;
    match api.st.answers.get(i) {
        Some(&a) => Ok(Some(a)),
        None => Ok(None),
    }
}

fn ask(api: &mut Api, c: &mut Call, formatted: bool) -> Result<u32> {
    let id = c.str(0)?;
    let Some((header, text)) = pages(api, &id).into_iter().next() else {
        api.st.error(format!("ShowPage : Unknown page: {id}"));
        return Ok(0);
    };
    match next_answer(api)? {
        Some(a) => Ok(u32::from(a != 0)),
        None => {
            let text = if formatted { sprintf(c, &text, 1)? } else { text };
            api.events.push(MissionEvent::Question { id, header, text });
            Err(Error::Host(QUESTION_PENDING.into()))
        }
    }
}

/// `bool AskQuestion(char* id)`: a modal OK/No page; 1 = OK.
fn ask_question(api: &mut Api, c: &mut Call) -> Result<u32> {
    ask(api, c, false)
}

fn ask_question_param(api: &mut Api, c: &mut Call) -> Result<u32> {
    ask(api, c, true)
}

/// `int AskMultilineQuestion(int nx, char* id, byte orient, char* quest)`: the choices are
/// `|`-separated text ids; returns the index of the chosen line.
fn ask_multiline_question(api: &mut Api, c: &mut Call) -> Result<u32> {
    let id = c.str(1)?;
    let Some((header, text)) = pages(api, &id).into_iter().next() else {
        api.st.error(format!("AskMultilineQuestion : Unknown page : {id}"));
        return Ok(0);
    };
    match next_answer(api)? {
        Some(a) => Ok(a),
        None => {
            let choices = c.bytes(3)?.split(|&b| b == b'|').map(|t| api.text(t)).collect();
            let (width, orient) = (c.i32(0)?.max(0) as u32, c.u8(2)?);
            api.events.push(MissionEvent::Choice { id, header, text, choices, width, orient });
            Err(Error::Host(QUESTION_PENDING.into()))
        }
    }
}

/// `GetCPos`: `{A}`..`{Z}` → 0..25, `{a}`..`{z}` → 26..51, anything else 0.
pub fn section(c: u8) -> usize {
    match c {
        b'A'..=b'Z' => usize::from(c - b'A'),
        b'a'..=b'z' => usize::from(c - b'a') + 26,
        _ => 0,
    }
}

fn enable_mission(api: &mut Api, c: &mut Call) -> Result<u32> {
    api.st.text_disable[section(c.u8(0)?)] = false;
    api.events.push(MissionEvent::Briefing);
    Ok(0)
}

fn disable_mission(api: &mut Api, c: &mut Call) -> Result<u32> {
    api.st.text_disable[section(c.u8(0)?)] = true;
    api.events.push(MissionEvent::Briefing);
    Ok(0)
}

fn miss_error_message(api: &mut Api, c: &mut Call) -> Result<u32> {
    let (title, text) = (c.str(0)?, c.str(1)?);
    api.events.push(MissionEvent::Error { title, text });
    Ok(0)
}

/// `char* GetTextByID(char* id)`: the text, or the id pointer itself if it is unknown. Known
/// texts are copied into guest memory once per machine.
fn get_text_by_id(api: &mut Api, c: &mut Call) -> Result<u32> {
    let id = c.bytes(0)?;
    let text = api.text(&id);
    if text == id {
        return c.u32(0);
    }
    let key = (api.vm, id);
    if let Some(&p) = api.st.text_cache.get(&key) {
        return Ok(p);
    }
    let p = c.alloc_str(&text)?;
    api.st.text_cache.insert(key, p);
    Ok(p)
}

/// `void AssignHint(char* text, int time)`: the hint line (time in frames).
fn assign_hint(api: &mut Api, c: &mut Call) -> Result<u32> {
    let text = c.bytes(0)?;
    api.events.push(MissionEvent::Hint { text, ticks: c.i32(1)?.max(0) as u32, list: false });
    Ok(0)
}

/// `AssignHint1(text, time)` (0x4942db): a new line of the message list.
fn assign_hint1(api: &mut Api, c: &mut Call) -> Result<u32> {
    let text = c.bytes(0)?;
    api.events.push(MissionEvent::Hint { text, ticks: c.i32(1)?.max(0) as u32, list: true });
    Ok(0)
}

// --- players ----------------------------------------------------------------------------

fn set_player_name(api: &mut Api, c: &mut Call) -> Result<u32> {
    let n = c.u8(0)?;
    if n < 8 {
        let mut name = api.text(&c.bytes(1)?);
        name.truncate(31);
        api.st.player_names[usize::from(n)] = Some(name);
    }
    Ok(0)
}

/// `void ChangeFriends(byte nation, byte mask)`: the nation's alliance mask and that of all
/// its units (bit k = nation k is a friend; one-sided).
fn change_friends(api: &mut Api, c: &mut Call) -> Result<u32> {
    let n = api.assign(c.u8(0)?);
    if n >= 8 {
        api.st.error("ChangeFriends(Nat,Flags) : Invalid parameter <Nat>");
        return Ok(0);
    }
    let mask = c.u8(1)?;
    api.st.nmask[usize::from(n)] = mask;
    let ids: Vec<u32> = api.world.units.iter().filter(|u| u.owner == n).map(|u| u.id.0).collect();
    for id in ids {
        if let Some(e) = api.st.extra.get_mut(&id) {
            e.nmask = None;
        }
    }
    Ok(0)
}

/// `void StartAI(byte nation, char* aiTextId, int land, int money, int resOnMap, int diff)`:
/// switches the AI of a nation on; the DLL path is the text of `aiTextId`.
fn start_ai(api: &mut Api, c: &mut Call) -> Result<u32> {
    let n = api.assign(c.u8(0)?);
    let path = crate::texts::cp1252(&api.text(&c.bytes(1)?));
    let diff = c.i32(5)?;
    if n < 8 {
        if diff != -1 {
            api.st.difficulty[usize::from(n)] = diff as u8;
        }
        api.st.land_type[usize::from(n)] = c.u8(2)?;
        api.st.ai_started.push((n, path));
    }
    Ok(0)
}

fn get_my_nation(api: &mut Api, _: &mut Call) -> Result<u32> {
    Ok(u32::from(api.st.setup.local_player))
}

fn get_ai_nation(api: &mut Api, _: &mut Call) -> Result<u32> {
    match api.caller {
        Caller::Ai(n) => Ok(u32::from(n)),
        _ => {
            api.st.error("ERROR: AI function must be called only from AI section.");
            Ok(0)
        }
    }
}

/// `int GetPeaceTimeLeft()`: the minutes of peace left (0x6e45d8), refreshed every 30 s.
fn get_peace_time_left(api: &mut Api, _: &mut Call) -> Result<u32> {
    Ok(api.world.peace.minutes as u32)
}

fn get_land_type(api: &mut Api, _: &mut Call) -> Result<u32> {
    let n = match api.caller {
        Caller::Ai(n) => n,
        _ => 0,
    };
    Ok(u32::from(api.st.land_type[usize::from(n & 7)]))
}

/// `bool NationIsErased(byte nation)`: no land units left, or fewer than 10 and neither
/// peasants nor a town centre.
fn nation_is_erased(api: &mut Api, c: &mut Call) -> Result<u32> {
    let n = c.u8(0)?;
    if n >= 8 {
        return Ok(1);
    }
    let n = api.assign(n);
    let (mut peasants, mut centres, mut units) = (0, 0, 0);
    // Objects alive or inside buildings; peasants and centres by `USAGE` (23, 3).
    for u in api.world.units.iter().filter(|u| u.living() && u.owner == n) {
        let t = api.world.catalog.get(u.kind);
        if t.role.usage == acr_sim::catalog::usage::PEASANT {
            peasants += 1;
        } else if t.role.usage == acr_sim::catalog::usage::CENTER {
            centres += 1;
        }
        if !u.building && t.medium == acr_sim::catalog::Medium::Land {
            units += 1;
        }
    }
    if peasants == 0 && centres == 0 && units < 10 {
        units = 0;
    }
    Ok(u32::from(units == 0))
}

// --- light spots, camera -------------------------------------------------------------------

/// `void SetLightSpot(GAMEOBJ* zone, int kind, byte index)` (0x564017): a point zone's
/// `(Index, Serial)` or a `'ZONE'` group's first zone centre goes into slot `index` (64
/// slots at 0x28b7cbf, 12 bytes each, saved with the game); anything else, or an index
/// above 63, is a script error. Every frame 0x55a2f1 stamps the used slots (`x != 0`) into
/// the fog with the `VISION` pattern `kind` (above 7 or negative: 8); acr-game does it in
/// `MissionLink::update_fog`, and the fog overlay and minimap show it.
fn set_light_spot(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some((x, y)) = (if c.u32(0)? == 0 { None } else { api.zone_point(&c.handle(0)?) }) else {
        api.st.error("SetLightSpot : Invalid  <Zone>");
        return Ok(0);
    };
    let i = usize::from(c.u8(2)?);
    if i >= LIGHT_SPOTS {
        api.st.error("SetLightSpot : Invalid  <Index>");
        return Ok(0);
    }
    api.st.light_spots[i] = crate::state::LightSpot { x, y, kind: c.i32(1)? };
    Ok(0)
}

/// `void ClearLightSpot(byte index)` (0x564121): zeroes slot `index`; above 63 is a script
/// error.
fn clear_light_spot(api: &mut Api, c: &mut Call) -> Result<u32> {
    let i = usize::from(c.u8(0)?);
    if i >= LIGHT_SPOTS {
        api.st.error("ClearLightSpot : Invalid  <Index>");
        return Ok(0);
    }
    api.st.light_spots[i] = Default::default();
    Ok(0)
}

/// `bool GetFogSpotVisibility(int x, int y)` (0x56e6a4): with fog on, whether the local
/// player's fog at the projected point reaches the visible value (900); without fog (none
/// handed in) everything is visible.
fn get_fog_spot_visibility(api: &mut Api, c: &mut Call) -> Result<u32> {
    let (x, y) = (c.i32(0)?, c.i32(1)?);
    Ok(api.st.fog.as_ref().is_none_or(|fog| fog.visible(api.world, x, y)).into())
}

/// `void SetStartPoint(GAMEOBJ* zone)`: centres the camera on the zone.
fn set_start_point(api: &mut Api, c: &mut Call) -> Result<u32> {
    match if c.u32(0)? == 0 { None } else { api.zone_point(&c.handle(0)?) } {
        Some((x, y)) => api.events.push(MissionEvent::Camera { x, y }),
        None => api.st.error("SetStartPoint : Invalid <Zone>"),
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use acr_script::heap::Heap;
    use acr_script::memory::Memory;
    use acr_script::runtime::HostCall;
    use acr_script::x86::Cpu;
    use acr_sim::World;
    use acr_sim::catalog::Catalog;
    use acr_sim::formation::Formations;
    use acr_sim::map::MapGrid;

    use super::super::{Api, Call, Caller, function};
    use crate::state::{ScriptSetup, ScriptState};
    use crate::texts::MissionTexts;

    fn call(name: &str, st: &mut ScriptState, caller: Caller) -> u32 {
        let mut world = World::new(
            Arc::new(MapGrid::open(1)),
            Arc::new(Catalog::new()),
            Arc::new(Formations::new(&Default::default(), &Default::default())),
        );
        let mut mem = Memory::new();
        mem.map(0x1_0000, 0x1_0000);
        let mut cpu = Cpu::new();
        cpu.regs[usize::from(acr_script::x86::decode::reg::ESP)] = 0x1_8000;
        let mut heap = Heap::new(0x2_0000, 0x3_0000);
        let mut raw = HostCall { dll: "test", name, cpu: &mut cpu, mem: &mut mem, heap: &mut heap };
        let (mut commands, mut events) = (Vec::new(), Vec::new());
        let mut api =
            Api { st, world: &mut world, commands: &mut commands, events: &mut events, caller, vm: 0, dip: None };
        (function(name).expect("known").handler)(&mut api, &mut Call { raw: &mut raw }).expect("call")
    }

    /// Random maps give every computer player its own difficulty (`City+0x183`).
    #[test]
    fn get_difficulty_is_per_nation() {
        let setup = ScriptSetup { difficulty: [0, 3, 2, 4, 1, 0, 2, 3], local_player: 0 };
        let mut st = ScriptState::new(setup, MissionTexts::default());
        for n in 0..8u8 {
            assert_eq!(call("GetDifficulty", &mut st, Caller::Ai(n)), u32::from(setup.difficulty[usize::from(n)]));
        }
    }
}
