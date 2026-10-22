//! Triggers, timers, time and memory.

use acr_script::runtime::Result;

use super::{Api, ApiFn, Call, Status};
use crate::state::{TIMERS, TRIGGERS};

pub const FUNCTIONS: &[ApiFn] = &[
    ApiFn { name: "Trigg", original: 0x563117, status: Status::Done, handler: trigg },
    ApiFn { name: "SetTrigg", original: 0x563160, status: Status::Done, handler: set_trigg },
    ApiFn { name: "RunTimer", original: 0x563598, status: Status::Done, handler: run_timer },
    ApiFn { name: "TimerDone", original: 0x5635f6, status: Status::Done, handler: timer_done },
    ApiFn { name: "TimerDoneFirst", original: 0x563669, status: Status::Done, handler: timer_done_first },
    ApiFn { name: "FreeTimer", original: 0x563739, status: Status::Done, handler: free_timer },
    ApiFn { name: "GetGlobalTime", original: 0x5637e7, status: Status::Done, handler: global_time },
    ApiFn { name: "?_ExMalloc@@YAPAXH@Z", original: 0x46b1c7, status: Status::Done, handler: ex_malloc },
    ApiFn { name: "?_ExFree@@YAXPAX@Z", original: 0x46b11e, status: Status::Done, handler: ex_free },
    ApiFn { name: "?_ExRealloc@@YAPAXPAXH@Z", original: 0x46b220, status: Status::Done, handler: ex_realloc },
];

/// `bool Trigg(byte n)`: the trigger word is stored inverted; returns `!stored` (al).
fn trigg(api: &mut Api, c: &mut Call) -> Result<u32> {
    let n = usize::from(c.u8(0)?);
    Ok(u32::from(!api.st.triggers[n % TRIGGERS]))
}

/// `void SetTrigg(byte n, byte value)`: stores `!value`.
fn set_trigg(api: &mut Api, c: &mut Call) -> Result<u32> {
    let n = usize::from(c.u8(0)?);
    api.st.triggers[n % TRIGGERS] = !u16::from(c.u8(1)?);
    Ok(0)
}

/// Index of a timer argument; out of range counts as an initialisation error.
fn timer(api: &mut Api, c: &Call) -> Result<Option<usize>> {
    let n = usize::from(c.u8(0)?);
    if n >= TIMERS {
        api.st.error(format!("{}: timer index {n} out of range", c.name()));
        return Ok(None);
    }
    Ok(Some(n))
}

/// `void RunTimer(byte n, int time)`: starts timer `n`; it counts down by 16 per tick.
fn run_timer(api: &mut Api, c: &mut Call) -> Result<u32> {
    if let Some(n) = timer(api, c)? {
        let t = &mut api.st.timers[n];
        t.left = c.i32(1)?;
        t.used = true;
        t.first = false;
    }
    Ok(0)
}

/// `bool TimerDone(byte n)`: the timer has run out (and was started).
fn timer_done(api: &mut Api, c: &mut Call) -> Result<u32> {
    Ok(timer(api, c)?.is_some_and(|n| api.st.timers[n].done()).into())
}

/// `bool TimerDoneFirst(byte n)`: like `TimerDone`, but true only once per run.
fn timer_done_first(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(n) = timer(api, c)? else { return Ok(0) };
    let t = &mut api.st.timers[n];
    if t.done() && t.first {
        t.first = false;
        return Ok(1);
    }
    Ok(0)
}

/// `void FreeTimer(byte n)`: stops and clears the timer.
fn free_timer(api: &mut Api, c: &mut Call) -> Result<u32> {
    if let Some(n) = timer(api, c)? {
        api.st.timers[n] = Default::default();
    }
    Ok(0)
}

/// `int GetGlobalTime()`: `GlobalTime` (0x9091c4), advanced by `FrmDec` = 2 per tick.
fn global_time(api: &mut Api, _: &mut Call) -> Result<u32> {
    Ok((api.world.tick.0 as u32).wrapping_mul(acr_sim::world::FRM_DEC as u32))
}

fn ex_malloc(_: &mut Api, c: &mut Call) -> Result<u32> {
    let size = c.u32(0)?;
    Ok(c.raw.heap.alloc(c.raw.mem, size, true).unwrap_or(0))
}

fn ex_free(_: &mut Api, c: &mut Call) -> Result<u32> {
    let p = c.u32(0)?;
    c.raw.heap.free(p);
    Ok(0)
}

fn ex_realloc(_: &mut Api, c: &mut Call) -> Result<u32> {
    let (p, size) = (c.u32(0)?, c.u32(1)?);
    Ok(if p == 0 { c.raw.heap.alloc(c.raw.mem, size, true) } else { c.raw.heap.realloc(c.raw.mem, p, size, true) }
        .unwrap_or(0))
}
