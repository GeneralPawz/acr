//! Direct object access for the AI DLLs: `GetOBJ` and the `OBJ_*` orders.

use acr_script::runtime::Result;
use acr_sim::Command;

use super::{Api, ApiFn, Call, Status};
use crate::guest;

pub const FUNCTIONS: &[ApiFn] = &[
    ApiFn { name: "GetOBJ", original: 0x56f07f, status: Status::Done, handler: get_obj },
    ApiFn {
        name: "OBJ_SendTo",
        original: 0x56f0a1,
        status: Status::Partial("a single-unit Command::Move; priority and order type need acr-sim orders"),
        handler: obj_send_to,
    },
    ApiFn {
        name: "OBJ_AttackObj",
        original: 0x56f13f,
        status: Status::Partial("AttackObj with the given priority; nTimes is not modelled"),
        handler: obj_attack_obj,
    },
    ApiFn { name: "OBJ_AttackPoint", original: 0x56f3a2, status: Status::Done, handler: obj_attack_point },
    ApiFn {
        name: "OBJ_LeaveMine",
        original: 0x56f1d6,
        status: Status::Partial("everybody leaves (acr-sim releases all units at once)"),
        handler: obj_leave_mine,
    },
    ApiFn { name: "GetTargetObject", original: 0x56f4b7, status: Status::Done, handler: get_target_object },
    ApiFn { name: "GetOrderType", original: 0x56f3f5, status: Status::Done, handler: get_order_type },
];

/// `bool OBJ_LeaveMine(word index, word type)`.
fn obj_leave_mine(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(b) = api.object(c.u16(0)?).map(|u| u.id.0 as usize) else { return Ok(0) };
    if !api.world.units[b].inside.is_empty() {
        acr_sim::economy::leave(api.world, b);
    }
    Ok(1)
}

/// `OrderType` of a unit's current order (`Order1+5`): 12 send-to, 3 attack, 0xF2 enter,
/// 0x59 build, 0xEE gather, 0x8E panic, 0x51 patrol.
pub fn order_type(u: &acr_sim::Unit) -> u32 {
    use acr_sim::world::Order;
    match u.orders.last() {
        None => 0,
        Some(Order::SendTo { .. } | Order::SmartSendTo { .. }) => 12,
        Some(Order::Attack { .. }) => 3,
        Some(Order::AttackPoint { .. }) => 0x4C,
        Some(Order::Enter { .. }) => 0xF2,
        Some(Order::Build { .. }) => 0x59,
        Some(Order::Gather { .. }) => 0xEE,
        Some(Order::Panic { .. }) => 0x8E,
        Some(Order::Patrol { .. }) => 0x51,
        Some(Order::ClearUnlimited) => 0,
    }
}

/// `OneObject* GetOBJ(int index)`: the guest mirror of the object, NULL for an empty slot.
/// Dead units stay visible (with `Sdoxlo` set) like dying units in the original.
fn get_obj(api: &mut Api, c: &mut Call) -> Result<u32> {
    let i = c.i32(0)?;
    if !(0..0xFFFF).contains(&i) {
        return Ok(0);
    }
    let Some(u) = api.object(i as u16) else { return Ok(0) };
    let id = u.id;
    let p = guest::write_mirror(c.raw.mem, api.st, api.world, id)?;
    api.st.mirrors.entry(api.vm).or_default().insert(i as u16);
    Ok(p)
}

/// The living unit behind a `word index` argument.
fn unit_arg(api: &Api, c: &Call, i: u32) -> Result<Option<(acr_sim::world::EntityId, u8)>> {
    let idx = c.u16(i)?;
    Ok(api.object(idx).filter(|u| u.living()).map(|u| (u.id, u.owner)))
}

/// `void OBJ_SendTo(word index, int x, int y, byte prio, byte ordType)` (pixels).
fn obj_send_to(api: &mut Api, c: &mut Call) -> Result<u32> {
    if let Some((id, owner)) = unit_arg(api, c, 0)? {
        api.commands.push(Command::Move { player: owner, units: vec![id], x: c.i32(1)?, y: c.i32(2)? });
    }
    Ok(0)
}

/// `void OBJ_AttackObj(word index, word target, byte prio, word nTimes)`.
fn obj_attack_obj(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some((id, owner)) = unit_arg(api, c, 0)? else { return Ok(0) };
    let Some(target) = api.object(c.u16(1)?).filter(|u| u.living()).map(|u| u.id) else { return Ok(0) };
    let _ = owner;
    api.attack(id, target, c.u8(2)?, false);
    // TODO(acr-sim): nTimes (shots) of an attack on an object.
    Ok(1)
}

/// `void OBJ_AttackPoint(word index, int x, int y, byte prio, byte ordType, word nTimes)`.
fn obj_attack_point(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some((id, _)) = unit_arg(api, c, 0)? else { return Ok(0) };
    let (x, y, prio, typ, times) = (c.i32(1)?, c.i32(2)?, c.u8(3)?, c.u8(4)?, c.u16(5)?);
    let p = acr_sim::combat::Placement::from_type(typ);
    acr_sim::artillery::attack_point(api.world, id.0 as usize, x, y, prio, p, times);
    Ok(1)
}

/// `void GetTargetObject(word index, word* outIndex, word* outSerial)`: the attack target
/// (`EnemyID`/`EnemySN`), 0xFFFF when not attacking or the target is dying.
fn get_target_object(api: &mut Api, c: &mut Call) -> Result<u32> {
    let target = api.object(c.u16(0)?).and_then(|u| u.target());
    match target.and_then(|t| api.object(t.0 as u16)) {
        Some(t) if t.living() => {
            let (id, serial) = (t.id.0 as u16, api.st.serial(t.id));
            c.out_u16(1, id)?;
            c.out_u16(2, serial)?;
        }
        Some(t) => {
            let serial = api.st.serial(t.id);
            c.out_u16(1, 0xFFFF)?;
            c.out_u16(2, serial)?;
        }
        None => c.out_u16(1, 0xFFFF)?,
    }
    Ok(0)
}

/// `int GetOrderType(OneObject*)`: the current order's type (12 = move).
fn get_order_type(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(i) = guest::object_index(c, c.u32(0)?)? else { return Ok(0) };
    Ok(api.object(i).map_or(0, order_type))
}
