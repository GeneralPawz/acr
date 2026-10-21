//! Interpreter for 32-bit x86 code.
//!
//! [`decode`] turns bytes into [`Insn`]s; [`Cpu::execute`] runs one decoded instruction
//! against a [`Memory`]. Flags are computed eagerly and exactly for every flag the
//! conditional instructions read (CF, PF, ZF, SF, OF; AF for arithmetic). Flags that the
//! architecture leaves undefined keep their previous value, so execution is deterministic.
//! The x87 FPU is emulated with `f64` registers (see [`fpu`]).
//!
//! Nothing here knows about Windows; imported functions are dispatched by
//! [`crate::runtime`].

pub mod decode;
pub mod fpu;

use std::fmt;

use decode::{Alu, BitOp, Cond, Mem, Rep, Shift, reg};
pub use decode::{DecodeError, Insn, Op, Operand, Size, decode};

use crate::memory::{Fault, Memory};

/// EFLAGS bits.
pub mod flags {
    pub const CF: u32 = 1 << 0;
    pub const PF: u32 = 1 << 2;
    pub const AF: u32 = 1 << 4;
    pub const ZF: u32 = 1 << 6;
    pub const SF: u32 = 1 << 7;
    pub const IF: u32 = 1 << 9;
    pub const DF: u32 = 1 << 10;
    pub const OF: u32 = 1 << 11;
    /// Arithmetic status flags.
    pub const STATUS: u32 = CF | PF | AF | ZF | SF | OF;
}
use flags::*;

/// Why execution stopped inside an instruction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Exception {
    /// Memory access to an unmapped address.
    Fault(Fault),
    /// `div`/`idiv` by zero or with a quotient that does not fit.
    DivideError,
    /// `int3` (MSVC pads functions with it and uses it for `DebugBreak`).
    Breakpoint,
    /// `hlt`
    Halt,
}

impl fmt::Display for Exception {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Fault(e) => e.fmt(f),
            Self::DivideError => f.write_str("divide error"),
            Self::Breakpoint => f.write_str("breakpoint (int3)"),
            Self::Halt => f.write_str("hlt"),
        }
    }
}

impl From<Fault> for Exception {
    fn from(e: Fault) -> Self {
        Self::Fault(e)
    }
}

type Result<T> = std::result::Result<T, Exception>;

/// Architectural state of one x86 thread.
#[derive(Debug, Clone, PartialEq)]
pub struct Cpu {
    /// `eax ecx edx ebx esp ebp esi edi`
    pub regs: [u32; 8],
    pub eip: u32,
    pub eflags: u32,
    /// Linear address of `fs:[0]` (the thread information block).
    pub fs_base: u32,
    pub fpu: fpu::Fpu,
}

impl Default for Cpu {
    fn default() -> Self {
        Self::new()
    }
}

fn parity(v: u32) -> bool {
    (v as u8).count_ones().is_multiple_of(2)
}

impl Cpu {
    pub fn new() -> Self {
        Self { regs: [0; 8], eip: 0, eflags: IF | 2, fs_base: 0, fpu: fpu::Fpu::new() }
    }

    pub fn flag(&self, f: u32) -> bool {
        self.eflags & f != 0
    }

    pub fn set_flag(&mut self, f: u32, on: bool) {
        if on {
            self.eflags |= f;
        } else {
            self.eflags &= !f;
        }
    }

    /// Reads a register with the given operand size (`4` is `ah` for bytes).
    pub fn reg(&self, r: u8, size: Size) -> u32 {
        match size {
            Size::Dword => self.regs[usize::from(r)],
            Size::Word => self.regs[usize::from(r)] & 0xFFFF,
            Size::Byte if r < 4 => self.regs[usize::from(r)] & 0xFF,
            Size::Byte => (self.regs[usize::from(r - 4)] >> 8) & 0xFF,
        }
    }

    pub fn set_reg(&mut self, r: u8, size: Size, v: u32) {
        let (i, mask, shift) = match size {
            Size::Dword => (r, u32::MAX, 0),
            Size::Word => (r, 0xFFFF, 0),
            Size::Byte if r < 4 => (r, 0xFF, 0),
            Size::Byte => (r - 4, 0xFF00, 8),
        };
        let slot = &mut self.regs[usize::from(i)];
        *slot = (*slot & !mask) | ((v << shift) & mask);
    }

    pub fn esp(&self) -> u32 {
        self.regs[usize::from(reg::ESP)]
    }

    /// Effective (linear) address of a memory operand.
    pub fn ea(&self, m: &Mem) -> u32 {
        let mut a = m.disp;
        if let Some(b) = m.base {
            a = a.wrapping_add(self.regs[usize::from(b)]);
        }
        if let Some(x) = m.index {
            a = a.wrapping_add(self.regs[usize::from(x)].wrapping_mul(u32::from(m.scale)));
        }
        if m.fs {
            a = a.wrapping_add(self.fs_base);
        }
        a
    }

    fn read_mem(mem: &Memory, addr: u32, size: Size) -> Result<u32> {
        Ok(match size {
            Size::Byte => u32::from(mem.read_u8(addr)?),
            Size::Word => u32::from(mem.read_u16(addr)?),
            Size::Dword => mem.read_u32(addr)?,
        })
    }

    fn write_mem(mem: &mut Memory, addr: u32, size: Size, v: u32) -> Result<()> {
        match size {
            Size::Byte => mem.write_u8(addr, v as u8)?,
            Size::Word => mem.write_u16(addr, v as u16)?,
            Size::Dword => mem.write_u32(addr, v)?,
        }
        Ok(())
    }

    /// Reads an operand of `size` bits.
    pub fn get(&self, mem: &Memory, op: &Operand, size: Size) -> Result<u32> {
        match op {
            Operand::Reg(r) => Ok(self.reg(*r, size)),
            Operand::Mem(m) => Self::read_mem(mem, self.ea(m), size),
            Operand::Imm(v) => Ok(*v & size.mask()),
            Operand::None => unreachable!("read of missing operand"),
        }
    }

    pub fn set(&mut self, mem: &mut Memory, op: &Operand, size: Size, v: u32) -> Result<()> {
        match op {
            Operand::Reg(r) => {
                self.set_reg(*r, size, v);
                Ok(())
            }
            Operand::Mem(m) => Self::write_mem(mem, self.ea(m), size, v),
            _ => unreachable!("write to non-lvalue operand"),
        }
    }

    pub fn push(&mut self, mem: &mut Memory, v: u32) -> Result<()> {
        self.push_sized(mem, v, Size::Dword)
    }

    fn push_sized(&mut self, mem: &mut Memory, v: u32, size: Size) -> Result<()> {
        let sp = self.esp().wrapping_sub(size.bytes());
        Self::write_mem(mem, sp, size, v)?;
        self.regs[usize::from(reg::ESP)] = sp;
        Ok(())
    }

    pub fn pop(&mut self, mem: &Memory) -> Result<u32> {
        self.pop_sized(mem, Size::Dword)
    }

    fn pop_sized(&mut self, mem: &Memory, size: Size) -> Result<u32> {
        let v = Self::read_mem(mem, self.esp(), size)?;
        self.regs[usize::from(reg::ESP)] = self.esp().wrapping_add(size.bytes());
        Ok(v)
    }

    /// Evaluates a condition code against the current flags.
    pub fn cond(&self, c: Cond) -> bool {
        let f = |b| self.flag(b);
        let r = match c.0 >> 1 {
            0 => f(OF),
            1 => f(CF),
            2 => f(ZF),
            3 => f(CF) || f(ZF),
            4 => f(SF),
            5 => f(PF),
            6 => f(SF) != f(OF),
            _ => f(ZF) || f(SF) != f(OF),
        };
        r ^ (c.0 & 1 != 0)
    }

    fn set_szp(&mut self, r: u32, size: Size) {
        self.set_flag(ZF, r & size.mask() == 0);
        self.set_flag(SF, r & size.sign_bit() != 0);
        self.set_flag(PF, parity(r));
    }

    /// `a + b + carry` with all arithmetic flags.
    fn add(&mut self, a: u32, b: u32, carry: u32, size: Size) -> u32 {
        let wide = u64::from(a) + u64::from(b) + u64::from(carry);
        let r = wide as u32 & size.mask();
        self.set_flag(CF, wide > u64::from(size.mask()));
        self.set_flag(OF, (a ^ r) & (b ^ r) & size.sign_bit() != 0);
        self.set_flag(AF, (a ^ b ^ r) & 0x10 != 0);
        self.set_szp(r, size);
        r
    }

    /// `a - b - borrow` with all arithmetic flags.
    fn sub(&mut self, a: u32, b: u32, borrow: u32, size: Size) -> u32 {
        let r = a.wrapping_sub(b).wrapping_sub(borrow) & size.mask();
        self.set_flag(CF, u64::from(a) < u64::from(b) + u64::from(borrow));
        self.set_flag(OF, (a ^ b) & (a ^ r) & size.sign_bit() != 0);
        self.set_flag(AF, (a ^ b ^ r) & 0x10 != 0);
        self.set_szp(r, size);
        r
    }

    fn logic(&mut self, r: u32, size: Size) -> u32 {
        self.eflags &= !(CF | OF | AF);
        self.set_szp(r, size);
        r & size.mask()
    }

    fn alu(&mut self, op: Alu, a: u32, b: u32, size: Size) -> u32 {
        let cf = u32::from(self.flag(CF));
        match op {
            Alu::Add => self.add(a, b, 0, size),
            Alu::Adc => self.add(a, b, cf, size),
            Alu::Sub | Alu::Cmp => self.sub(a, b, 0, size),
            Alu::Sbb => self.sub(a, b, cf, size),
            Alu::And => self.logic(a & b, size),
            Alu::Or => self.logic(a | b, size),
            Alu::Xor => self.logic(a ^ b, size),
        }
    }

    fn shift(&mut self, op: Shift, a: u32, count: u32, size: Size) -> u32 {
        let count = count & 31;
        if count == 0 {
            return a;
        }
        let (bits, mask, sign) = (size.bits(), size.mask(), size.sign_bit());
        let msb = |v: u32| v & sign != 0;
        match op {
            Shift::Shl => {
                let wide = u64::from(a) << count;
                let r = wide as u32 & mask;
                let cf = (wide >> bits) & 1 != 0;
                self.set_flag(CF, cf);
                self.set_flag(OF, msb(r) != cf);
                self.set_szp(r, size);
                r
            }
            Shift::Shr => {
                let r = ((u64::from(a) >> count) as u32) & mask;
                self.set_flag(CF, (u64::from(a) >> (count - 1)) & 1 != 0);
                self.set_flag(OF, msb(a));
                self.set_szp(r, size);
                r
            }
            Shift::Sar => {
                let sa = i64::from(((a << (32 - bits)) as i32) >> (32 - bits));
                let r = (sa >> count) as u32 & mask;
                self.set_flag(CF, (sa >> (count - 1)) & 1 != 0);
                self.set_flag(OF, false);
                self.set_szp(r, size);
                r
            }
            Shift::Rol | Shift::Ror => {
                let n = count % bits;
                let r = if op == Shift::Rol {
                    ((a << n) | (u64::from(a) >> (bits - n)) as u32) & mask
                } else {
                    ((a >> n) | (u64::from(a) << (bits - n)) as u32) & mask
                };
                if op == Shift::Rol {
                    self.set_flag(CF, r & 1 != 0);
                    self.set_flag(OF, msb(r) != (r & 1 != 0));
                } else {
                    self.set_flag(CF, msb(r));
                    self.set_flag(OF, msb(r) != msb(r << 1));
                }
                r
            }
            Shift::Rcl | Shift::Rcr => {
                let n = count % (bits + 1);
                let (mut r, mut cf) = (a, self.flag(CF));
                for _ in 0..n {
                    if op == Shift::Rcl {
                        let out = msb(r);
                        r = ((r << 1) | u32::from(cf)) & mask;
                        cf = out;
                    } else {
                        let out = r & 1 != 0;
                        r = (r >> 1) | if cf { sign } else { 0 };
                        cf = out;
                    }
                }
                self.set_flag(CF, cf);
                if op == Shift::Rcl {
                    self.set_flag(OF, msb(r) != cf);
                } else {
                    self.set_flag(OF, msb(r) != msb(r << 1));
                }
                r
            }
        }
    }

    /// Executes one decoded instruction located at `self.eip`.
    ///
    /// On an exception `eip` still points at the instruction; side effects that happened
    /// before the exception are kept.
    pub fn execute(&mut self, mem: &mut Memory, i: &Insn) -> Result<()> {
        let at = self.eip;
        self.eip = at.wrapping_add(u32::from(i.len));
        let r = self.exec(mem, i);
        if r.is_err() {
            self.eip = at;
        }
        r
    }

    fn exec(&mut self, mem: &mut Memory, i: &Insn) -> Result<()> {
        let size = i.size;
        match i.op {
            Op::Alu(op) => {
                let a = self.get(mem, &i.dst, size)?;
                let b = self.get(mem, &i.src, size)?;
                let r = self.alu(op, a, b, size);
                if op != Alu::Cmp {
                    self.set(mem, &i.dst, size, r)?;
                }
            }
            Op::Test => {
                let r = self.get(mem, &i.dst, size)? & self.get(mem, &i.src, size)?;
                self.logic(r, size);
            }
            Op::Mov => {
                let v = self.get(mem, &i.src, size)?;
                self.set(mem, &i.dst, size, v)?;
            }
            Op::Movzx(from) => {
                let v = self.get(mem, &i.src, from)?;
                self.set(mem, &i.dst, size, v)?;
            }
            Op::Movsx(from) => {
                let v = self.get(mem, &i.src, from)?;
                let v = if from == Size::Byte { v as u8 as i8 as u32 } else { v as u16 as i16 as u32 };
                self.set(mem, &i.dst, size, v)?;
            }
            Op::Lea => {
                let Operand::Mem(m) = &i.src else { unreachable!() };
                self.set(mem, &i.dst, size, self.ea(m))?;
            }
            Op::Xchg => {
                let a = self.get(mem, &i.dst, size)?;
                let b = self.get(mem, &i.src, size)?;
                self.set(mem, &i.dst, size, b)?;
                self.set(mem, &i.src, size, a)?;
            }
            Op::Inc | Op::Dec => {
                let a = self.get(mem, &i.dst, size)?;
                let cf = self.flag(CF);
                let r = if i.op == Op::Inc { self.add(a, 1, 0, size) } else { self.sub(a, 1, 0, size) };
                self.set_flag(CF, cf);
                self.set(mem, &i.dst, size, r)?;
            }
            Op::Not => {
                let a = self.get(mem, &i.dst, size)?;
                self.set(mem, &i.dst, size, !a)?;
            }
            Op::Neg => {
                let a = self.get(mem, &i.dst, size)?;
                let r = self.sub(0, a, 0, size);
                self.set(mem, &i.dst, size, r)?;
            }
            Op::Mul | Op::Imul1 => self.mul(mem, i)?,
            Op::Div | Op::Idiv => self.div(mem, i)?,
            Op::Imul => {
                let a = self.get(mem, &i.src, size)?;
                let b = match i.src2 {
                    Operand::None => self.get(mem, &i.dst, size)?,
                    other => self.get(mem, &other, size)?,
                };
                let sx = |v: u32| i64::from(((v << (32 - size.bits())) as i32) >> (32 - size.bits()));
                let full = sx(a) * sx(b);
                let r = full as u32 & size.mask();
                let overflow = sx(r) != full;
                self.set_flag(CF, overflow);
                self.set_flag(OF, overflow);
                self.set(mem, &i.dst, size, r)?;
            }
            Op::Shift(op) => {
                let a = self.get(mem, &i.dst, size)?;
                let count = self.get(mem, &i.src, Size::Byte)?;
                let r = self.shift(op, a, count, size);
                self.set(mem, &i.dst, size, r)?;
            }
            Op::Shld | Op::Shrd => {
                let count = self.get(mem, &i.src2, Size::Byte)? & 31;
                if count != 0 {
                    let a = self.get(mem, &i.dst, size)?;
                    let b = self.get(mem, &i.src, size)?;
                    let bits = size.bits();
                    // Treat dst:src (shld) or src:dst (shrd) as one wide value.
                    let (r, cf) = if i.op == Op::Shld {
                        let wide = (u64::from(a) << bits) | u64::from(b);
                        (((wide << count) >> bits) as u32 & size.mask(), (wide >> (2 * bits - count)) & 1 != 0)
                    } else {
                        let wide = (u64::from(b) << bits) | u64::from(a);
                        ((wide >> count) as u32 & size.mask(), (wide >> (count - 1)) & 1 != 0)
                    };
                    self.set_flag(CF, cf);
                    self.set_flag(OF, (r ^ a) & size.sign_bit() != 0);
                    self.set_szp(r, size);
                    self.set(mem, &i.dst, size, r)?;
                }
            }
            Op::Push => {
                let v = self.get(mem, &i.src, size)?;
                self.push_sized(mem, v, size)?;
            }
            Op::Pop => {
                let v = self.pop_sized(mem, size)?;
                // The destination address is computed after esp was incremented.
                self.set(mem, &i.dst, size, v)?;
            }
            Op::Pushad => {
                let sp = self.esp();
                for r in 0..8u8 {
                    let v = if r == reg::ESP { sp } else { self.regs[usize::from(r)] };
                    self.push(mem, v)?;
                }
            }
            Op::Popad => {
                for r in (0..8u8).rev() {
                    let v = self.pop(mem)?;
                    if r != reg::ESP {
                        self.regs[usize::from(r)] = v;
                    }
                }
            }
            Op::Pushfd => self.push(mem, self.eflags & 0x00FC_FFFF)?,
            Op::Popfd => {
                let v = self.pop(mem)?;
                self.eflags = (v & (STATUS | DF)) | IF | 2;
            }
            Op::Cbw => {
                let v = if size == Size::Word {
                    self.reg(0, Size::Byte) as u8 as i8 as u32
                } else {
                    self.regs[0] as i16 as u32
                };
                self.set_reg(reg::EAX, size, v);
            }
            Op::Cwd => {
                let sign = self.reg(reg::EAX, size) & size.sign_bit() != 0;
                self.set_reg(reg::EDX, size, if sign { 0xFFFF_FFFF } else { 0 });
            }
            Op::Jcc(c) => {
                if self.cond(c) {
                    self.eip = self.get(mem, &i.src, Size::Dword)?;
                }
            }
            Op::Jmp => self.eip = self.get(mem, &i.src, Size::Dword)?,
            Op::Call => {
                let target = self.get(mem, &i.src, Size::Dword)?;
                self.push(mem, self.eip)?;
                self.eip = target;
            }
            Op::Ret => {
                let extra = self.get(mem, &i.src, Size::Dword)?;
                self.eip = self.pop(mem)?;
                self.regs[usize::from(reg::ESP)] = self.esp().wrapping_add(extra);
            }
            Op::Leave => {
                self.regs[usize::from(reg::ESP)] = self.regs[usize::from(reg::EBP)];
                self.regs[usize::from(reg::EBP)] = self.pop(mem)?;
            }
            Op::Enter => {
                let alloc = self.get(mem, &i.src, Size::Dword)?;
                let level = self.get(mem, &i.src2, Size::Dword)? & 31;
                self.push(mem, self.regs[usize::from(reg::EBP)])?;
                let frame = self.esp();
                if level > 0 {
                    let mut bp = self.regs[usize::from(reg::EBP)];
                    for _ in 1..level {
                        bp = bp.wrapping_sub(4);
                        let v = mem.read_u32(bp)?;
                        self.push(mem, v)?;
                    }
                    self.push(mem, frame)?;
                }
                self.regs[usize::from(reg::EBP)] = frame;
                self.regs[usize::from(reg::ESP)] = self.esp().wrapping_sub(alloc);
            }
            Op::Loop | Op::Loope | Op::Loopne => {
                let ecx = self.regs[usize::from(reg::ECX)].wrapping_sub(1);
                self.regs[usize::from(reg::ECX)] = ecx;
                let take = ecx != 0
                    && match i.op {
                        Op::Loope => self.flag(ZF),
                        Op::Loopne => !self.flag(ZF),
                        _ => true,
                    };
                if take {
                    self.eip = self.get(mem, &i.src, Size::Dword)?;
                }
            }
            Op::Jecxz => {
                if self.regs[usize::from(reg::ECX)] == 0 {
                    self.eip = self.get(mem, &i.src, Size::Dword)?;
                }
            }
            Op::Setcc(c) => self.set(mem, &i.dst, Size::Byte, u32::from(self.cond(c)))?,
            Op::Cmovcc(c) => {
                let v = self.get(mem, &i.src, size)?;
                if self.cond(c) {
                    self.set(mem, &i.dst, size, v)?;
                }
            }
            Op::Bt(op) => self.bit_test(mem, i, op)?,
            Op::Bsf | Op::Bsr => {
                let v = self.get(mem, &i.src, size)?;
                self.set_flag(ZF, v == 0);
                if v != 0 {
                    let idx = if i.op == Op::Bsf { v.trailing_zeros() } else { 31 - v.leading_zeros() };
                    self.set(mem, &i.dst, size, idx)?;
                }
            }
            Op::Bswap => {
                let v = self.get(mem, &i.dst, size)?;
                self.set(mem, &i.dst, size, v.swap_bytes())?;
            }
            Op::Xadd => {
                let a = self.get(mem, &i.dst, size)?;
                let b = self.get(mem, &i.src, size)?;
                let r = self.add(a, b, 0, size);
                self.set(mem, &i.src, size, a)?;
                self.set(mem, &i.dst, size, r)?;
            }
            Op::Cmpxchg => {
                let acc = self.reg(reg::EAX, size);
                let d = self.get(mem, &i.dst, size)?;
                self.sub(acc, d, 0, size);
                if acc == d {
                    let s = self.get(mem, &i.src, size)?;
                    self.set(mem, &i.dst, size, s)?;
                } else {
                    self.set_reg(reg::EAX, size, d);
                }
            }
            Op::Movs | Op::Cmps | Op::Stos | Op::Lods | Op::Scas => self.string(mem, i)?,
            Op::Xlat => {
                let a = self.regs[usize::from(reg::EBX)].wrapping_add(self.reg(reg::EAX, Size::Byte));
                let v = mem.read_u8(a)?;
                self.set_reg(reg::EAX, Size::Byte, u32::from(v));
            }
            Op::Cld => self.set_flag(DF, false),
            Op::Std => self.set_flag(DF, true),
            Op::Clc => self.set_flag(CF, false),
            Op::Stc => self.set_flag(CF, true),
            Op::Cmc => self.set_flag(CF, !self.flag(CF)),
            Op::Sahf => {
                let ah = self.reg(4, Size::Byte);
                self.eflags = (self.eflags & !(SF | ZF | AF | PF | CF)) | (ah & (SF | ZF | AF | PF | CF));
            }
            Op::Lahf => self.set_reg(4, Size::Byte, (self.eflags & 0xFF) | 2),
            Op::Nop | Op::Wait => {}
            Op::Int3 => return Err(Exception::Breakpoint),
            Op::Hlt => return Err(Exception::Halt),
            Op::Fpu(f) => self.fpu_exec(mem, i, f)?,
        }
        Ok(())
    }

    fn mul(&mut self, mem: &Memory, i: &Insn) -> Result<()> {
        let size = i.size;
        let b = self.get(mem, &i.src, size)?;
        let a = self.reg(reg::EAX, size);
        let bits = size.bits();
        let (lo, hi, overflow) = if i.op == Op::Mul {
            let full = u64::from(a) * u64::from(b);
            let hi = (full >> bits) as u32 & size.mask();
            (full as u32 & size.mask(), hi, hi != 0)
        } else {
            let sx = |v: u32| i64::from(((v << (32 - bits)) as i32) >> (32 - bits));
            let full = sx(a) * sx(b);
            let lo = full as u32 & size.mask();
            (lo, (full >> bits) as u32 & size.mask(), sx(lo) != full)
        };
        if size == Size::Byte {
            self.set_reg(reg::EAX, Size::Word, lo | (hi << 8));
        } else {
            self.set_reg(reg::EAX, size, lo);
            self.set_reg(reg::EDX, size, hi);
        }
        self.set_flag(CF, overflow);
        self.set_flag(OF, overflow);
        Ok(())
    }

    fn div(&mut self, mem: &Memory, i: &Insn) -> Result<()> {
        let size = i.size;
        let bits = size.bits();
        let divisor = self.get(mem, &i.src, size)?;
        if divisor == 0 {
            return Err(Exception::DivideError);
        }
        let dividend = if size == Size::Byte {
            u64::from(self.reg(reg::EAX, Size::Word))
        } else {
            (u64::from(self.reg(reg::EDX, size)) << bits) | u64::from(self.reg(reg::EAX, size))
        };
        let (q, r) = if i.op == Op::Div {
            let q = dividend / u64::from(divisor);
            if q > u64::from(size.mask()) {
                return Err(Exception::DivideError);
            }
            (q as u32, (dividend % u64::from(divisor)) as u32)
        } else {
            let n = i128::from(((dividend << (64 - 2 * bits)) as i64) >> (64 - 2 * bits));
            let d = i128::from(((divisor << (32 - bits)) as i32) >> (32 - bits));
            let (q, r) = (n / d, n % d);
            let limit = 1i128 << (bits - 1);
            if q < -limit || q >= limit {
                return Err(Exception::DivideError);
            }
            (q as u32 & size.mask(), r as u32 & size.mask())
        };
        if size == Size::Byte {
            self.set_reg(reg::EAX, Size::Word, q | (r << 8));
        } else {
            self.set_reg(reg::EAX, size, q);
            self.set_reg(reg::EDX, size, r);
        }
        Ok(())
    }

    fn bit_test(&mut self, mem: &mut Memory, i: &Insn, op: BitOp) -> Result<()> {
        let size = i.size;
        let offset = self.get(mem, &i.src, size)?;
        // A register bit offset can address memory outside the operand (bit strings).
        let (dst, bit) = match (&i.dst, &i.src) {
            (Operand::Mem(m), Operand::Reg(_)) => {
                let bits = size.bits() as i32;
                let signed = if size == Size::Word { offset as i16 as i32 } else { offset as i32 };
                let addr = self.ea(m).wrapping_add((signed.div_euclid(bits) * (bits / 8)) as u32);
                let m = Mem { base: None, index: None, scale: 1, disp: addr, fs: false };
                (Operand::Mem(m), signed.rem_euclid(bits) as u32)
            }
            _ => (i.dst, offset & (size.bits() - 1)),
        };
        let v = self.get(mem, &dst, size)?;
        self.set_flag(CF, (v >> bit) & 1 != 0);
        let r = match op {
            BitOp::Bt => return Ok(()),
            BitOp::Bts => v | (1 << bit),
            BitOp::Btr => v & !(1 << bit),
            BitOp::Btc => v ^ (1 << bit),
        };
        self.set(mem, &dst, size, r)
    }

    fn string(&mut self, mem: &mut Memory, i: &Insn) -> Result<()> {
        let size = i.size;
        let step = if self.flag(DF) { size.bytes().wrapping_neg() } else { size.bytes() };
        let (si, di, cx) = (usize::from(reg::ESI), usize::from(reg::EDI), usize::from(reg::ECX));
        loop {
            if i.rep != Rep::None && self.regs[cx] == 0 {
                break;
            }
            match i.op {
                Op::Movs => {
                    let v = Self::read_mem(mem, self.regs[si], size)?;
                    Self::write_mem(mem, self.regs[di], size, v)?;
                    self.regs[si] = self.regs[si].wrapping_add(step);
                    self.regs[di] = self.regs[di].wrapping_add(step);
                }
                Op::Stos => {
                    Self::write_mem(mem, self.regs[di], size, self.reg(reg::EAX, size))?;
                    self.regs[di] = self.regs[di].wrapping_add(step);
                }
                Op::Lods => {
                    let v = Self::read_mem(mem, self.regs[si], size)?;
                    self.set_reg(reg::EAX, size, v);
                    self.regs[si] = self.regs[si].wrapping_add(step);
                }
                Op::Cmps => {
                    let a = Self::read_mem(mem, self.regs[si], size)?;
                    let b = Self::read_mem(mem, self.regs[di], size)?;
                    self.sub(a, b, 0, size);
                    self.regs[si] = self.regs[si].wrapping_add(step);
                    self.regs[di] = self.regs[di].wrapping_add(step);
                }
                _ => {
                    let b = Self::read_mem(mem, self.regs[di], size)?;
                    self.sub(self.reg(reg::EAX, size), b, 0, size);
                    self.regs[di] = self.regs[di].wrapping_add(step);
                }
            }
            if i.rep == Rep::None {
                break;
            }
            self.regs[cx] = self.regs[cx].wrapping_sub(1);
            if matches!(i.op, Op::Cmps | Op::Scas) && self.flag(ZF) != (i.rep == Rep::Rep) {
                break;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
