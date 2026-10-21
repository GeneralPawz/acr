//! Win32 structured exception handling for `RaiseException` and `RtlUnwind`.
//!
//! MSVC implements C++ `throw` as `RaiseException(0xE06D7363, ...)`; the C runtime's
//! `__CxxFrameHandler` (guest code) finds the `catch` block, calls `RtlUnwind` and then
//! jumps to the continuation without ever returning to the dispatcher. So the dispatcher
//! must not call handlers as nested host calls. Instead it runs as a small state machine
//! in guest memory: each handler is entered with a return address pointing at a
//! dedicated thunk ([`SEH_RETURN`] / [`UNWIND_RETURN`]), and when (if) the handler
//! returns there, the interpreter continues the dispatch from the state stored next to the
//! handler's arguments on the guest stack. A handler that never returns simply abandons
//! that state, exactly as on Windows.
//!
//! Hardware exceptions (access violations, divide errors) are not dispatched to SEH; they
//! stop the interpreter.

use crate::loader::{THUNK_AREA, THUNK_BASE, THUNK_STRIDE};
use crate::runtime::{Error, Machine, Result};
use crate::x86::decode::reg;

/// Return address for exception handlers called during dispatch.
pub const SEH_RETURN: u32 = THUNK_BASE + THUNK_AREA - 2 * THUNK_STRIDE;
/// Return address for exception handlers called during unwinding.
pub const UNWIND_RETURN: u32 = THUNK_BASE + THUNK_AREA - 3 * THUNK_STRIDE;

const END_OF_CHAIN: u32 = u32::MAX;
const EXCEPTION_NONCONTINUABLE: u32 = 1;
const EXCEPTION_UNWINDING: u32 = 2;
const EXCEPTION_EXIT_UNWIND: u32 = 4;
const STATUS_UNWIND: u32 = 0xC000_0027;
const EXCEPTION_RECORD_SIZE: u32 = 0x50;
const CONTEXT_SIZE: u32 = 0x2CC;
const CONTEXT_FULL: u32 = 0x1_0007;
/// `CONTEXT` offsets of `edi esi ebx edx ecx eax ebp eip`, then `eflags` and `esp`.
const CTX_EDI: u32 = 0x9C;
const CTX_EIP: u32 = 0xB8;
const CTX_EFLAGS: u32 = 0xC0;
const CTX_ESP: u32 = 0xC4;
const CTX_REGS: [(u32, u8); 7] = [
    (0x9C, reg::EDI),
    (0xA0, reg::ESI),
    (0xA4, reg::EBX),
    (0xA8, reg::EDX),
    (0xAC, reg::ECX),
    (0xB0, reg::EAX),
    (0xB4, reg::EBP),
];

/// Words stored after the four handler arguments while unwinding.
const UNWIND_TARGET: u32 = 16;
const UNWIND_CALLER_RET: u32 = 20;
const UNWIND_CALLER_ESP: u32 = 24;
const UNWIND_RETVAL: u32 = 28;
const UNWIND_SAVED: u32 = 32; // ebx esi edi ebp

impl Machine {
    fn fs0(&self) -> Result<u32> {
        Ok(self.mem.read_u32(self.cpu.fs_base)?)
    }

    fn set_fs0(&mut self, v: u32) -> Result<()> {
        Ok(self.mem.write_u32(self.cpu.fs_base, v)?)
    }

    /// Writes a `CONTEXT` for the current registers, with `eip`/`esp` as given.
    fn write_context(&mut self, at: u32, eip: u32, esp: u32) -> Result<()> {
        self.mem.fill(at, CONTEXT_SIZE, 0)?;
        self.mem.write_u32(at, CONTEXT_FULL)?;
        for (off, r) in CTX_REGS {
            self.mem.write_u32(at + off, self.cpu.regs[usize::from(r)])?;
        }
        self.mem.write_u32(at + CTX_EIP, eip)?;
        self.mem.write_u32(at + CTX_EFLAGS, self.cpu.eflags)?;
        self.mem.write_u32(at + CTX_ESP, esp)?;
        Ok(())
    }

    /// Enters the handler of registration `frame` with `(record, frame, context, dc)`.
    fn enter_handler(&mut self, args_at: u32, frame: u32, ret: u32) -> Result<()> {
        self.mem.write_u32(args_at + 4, frame)?;
        self.mem.write_u32(args_at - 4, ret)?;
        self.cpu.regs[usize::from(reg::ESP)] = args_at - 4;
        self.cpu.eip = self.mem.read_u32(frame + 4)?;
        Ok(())
    }

    /// `RaiseException(code, flags, argc, argv)`, entered with `esp` at the return address.
    pub(crate) fn raise_exception(&mut self) -> Result<()> {
        let esp = self.cpu.esp();
        let rd = |m: &Self, i: u32| m.mem.read_u32(esp + 4 * i);
        let (ret, code, flags, argc, argv) = (rd(self, 0)?, rd(self, 1)?, rd(self, 2)?, rd(self, 3)?, rd(self, 4)?);
        // Exception record, context and dispatcher context below the caller's stack.
        let record = (esp - 0x100 - EXCEPTION_RECORD_SIZE - CONTEXT_SIZE - 4) & !0xF;
        let context = record + EXCEPTION_RECORD_SIZE;
        let dc = context + CONTEXT_SIZE;
        self.mem.fill(record, EXCEPTION_RECORD_SIZE, 0)?;
        let argc = argc.min(15);
        for (off, v) in [(0, code), (4, flags & EXCEPTION_NONCONTINUABLE), (0xC, ret), (0x10, argc)] {
            self.mem.write_u32(record + off, v)?;
        }
        for i in 0..argc {
            let v = self.mem.read_u32(argv + 4 * i)?;
            self.mem.write_u32(record + 0x14 + 4 * i, v)?;
        }
        self.write_context(context, ret, esp + 20)?;
        self.mem.write_u32(dc, 0)?;
        let frame = self.fs0()?;
        if frame == END_OF_CHAIN {
            return Err(unhandled(code));
        }
        let args = record - 16;
        for (i, v) in [record, frame, context, dc].into_iter().enumerate() {
            self.mem.write_u32(args + 4 * i as u32, v)?;
        }
        self.enter_handler(args, frame, SEH_RETURN)
    }

    /// A handler returned to [`SEH_RETURN`]; `esp` points at its arguments.
    pub(crate) fn seh_return(&mut self) -> Result<()> {
        let args = self.cpu.esp();
        let disposition = self.cpu.regs[usize::from(reg::EAX)];
        let record = self.mem.read_u32(args)?;
        let frame = self.mem.read_u32(args + 4)?;
        let context = self.mem.read_u32(args + 8)?;
        let code = self.mem.read_u32(record)?;
        match disposition {
            // ExceptionContinueExecution: resume with the (possibly modified) context.
            0 => {
                if self.mem.read_u32(record + 4)? & EXCEPTION_NONCONTINUABLE != 0 {
                    return Err(Error::Unsupported {
                        function: "RaiseException",
                        detail: format!("handler continued non-continuable exception {code:#010x}"),
                    });
                }
                for (off, r) in CTX_REGS {
                    self.cpu.regs[usize::from(r)] = self.mem.read_u32(context + off)?;
                }
                self.cpu.eflags = (self.mem.read_u32(context + CTX_EFLAGS)? & 0xCD5) | 0x202;
                self.cpu.regs[usize::from(reg::ESP)] = self.mem.read_u32(context + CTX_ESP)?;
                self.cpu.eip = self.mem.read_u32(context + CTX_EIP)?;
                debug_assert_eq!(CTX_EDI, CTX_REGS[0].0);
                Ok(())
            }
            // ExceptionContinueSearch: try the next registration.
            1 => {
                let next = self.mem.read_u32(frame)?;
                if next == END_OF_CHAIN {
                    return Err(unhandled(code));
                }
                self.enter_handler(args, next, SEH_RETURN)
            }
            d => Err(Error::Unsupported {
                function: "RaiseException",
                detail: format!("handler disposition {d} for exception {code:#010x}"),
            }),
        }
    }

    /// `RtlUnwind(target_frame, target_ip, record, return_value)`.
    pub(crate) fn rtl_unwind(&mut self) -> Result<()> {
        let esp = self.cpu.esp();
        let rd = |m: &Self, i: u32| m.mem.read_u32(esp + 4 * i);
        let (ret, target, _ip, record_in, retval) =
            (rd(self, 0)?, rd(self, 1)?, rd(self, 2)?, rd(self, 3)?, rd(self, 4)?);
        let record = (esp - 0x100 - EXCEPTION_RECORD_SIZE - CONTEXT_SIZE - 4) & !0xF;
        let context = record + EXCEPTION_RECORD_SIZE;
        let dc = context + CONTEXT_SIZE;
        if record_in == 0 {
            self.mem.fill(record, EXCEPTION_RECORD_SIZE, 0)?;
            self.mem.write_u32(record, STATUS_UNWIND)?;
            self.mem.write_u32(record + 0xC, ret)?;
        } else {
            let mut buf = vec![0; EXCEPTION_RECORD_SIZE as usize];
            self.mem.read_bytes(record_in, &mut buf)?;
            self.mem.write_bytes(record, &buf)?;
        }
        let flags =
            self.mem.read_u32(record + 4)? | EXCEPTION_UNWINDING | if target == 0 { EXCEPTION_EXIT_UNWIND } else { 0 };
        self.mem.write_u32(record + 4, flags)?;
        self.write_context(context, ret, esp + 20)?;
        self.mem.write_u32(dc, 0)?;
        // Handler arguments followed by the unwind state.
        let args = record - 64;
        let r = |m: &Self, i: u8| m.cpu.regs[usize::from(i)];
        let words = [
            record,
            0,
            context,
            dc,
            target,
            ret,
            esp + 20,
            retval,
            r(self, reg::EBX),
            r(self, reg::ESI),
            r(self, reg::EDI),
            r(self, reg::EBP),
        ];
        for (i, v) in words.into_iter().enumerate() {
            self.mem.write_u32(args + 4 * i as u32, v)?;
        }
        self.unwind_next(args, self.fs0()?)
    }

    /// Calls the unwind handler of `frame` or finishes when `target` is reached.
    fn unwind_next(&mut self, args: u32, frame: u32) -> Result<()> {
        let target = self.mem.read_u32(args + UNWIND_TARGET)?;
        if frame != target && frame != END_OF_CHAIN {
            return self.enter_handler(args, frame, UNWIND_RETURN);
        }
        if frame == END_OF_CHAIN && target != 0 {
            return Err(Error::Unsupported { function: "RtlUnwind", detail: "target frame not on the chain".into() });
        }
        // Done: return from RtlUnwind (stdcall, 16 bytes of arguments).
        for (i, r) in [reg::EBX, reg::ESI, reg::EDI, reg::EBP].into_iter().enumerate() {
            self.cpu.regs[usize::from(r)] = self.mem.read_u32(args + UNWIND_SAVED + 4 * i as u32)?;
        }
        self.cpu.regs[usize::from(reg::EAX)] = self.mem.read_u32(args + UNWIND_RETVAL)?;
        self.cpu.regs[usize::from(reg::ESP)] = self.mem.read_u32(args + UNWIND_CALLER_ESP)?;
        self.cpu.eip = self.mem.read_u32(args + UNWIND_CALLER_RET)?;
        Ok(())
    }

    /// An unwind handler returned to [`UNWIND_RETURN`]: unlink its frame and continue.
    pub(crate) fn unwind_return(&mut self) -> Result<()> {
        let args = self.cpu.esp();
        let frame = self.mem.read_u32(args + 4)?;
        let next = self.mem.read_u32(frame)?;
        self.set_fs0(next)?;
        self.unwind_next(args, next)
    }
}

fn unhandled(code: u32) -> Error {
    let what = if code == 0xE06D_7363 { " (C++ exception)" } else { "" };
    Error::Unsupported { function: "RaiseException", detail: format!("unhandled exception {code:#010x}{what}") }
}
