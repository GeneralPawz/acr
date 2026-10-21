//! Decoder for the 32-bit x86 instruction subset used by the original DLLs.
//!
//! Covers the general-purpose integer instructions an MSVC 6/7 compiler emits (including
//! its C runtime) and the x87 FPU. Segment, I/O, BCD, far-pointer, system and SIMD
//! instructions are rejected with [`DecodeError`]; none of them is reachable in the 73
//! original mission/AI DLLs.

use std::fmt;

/// Operand size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Size {
    Byte,
    Word,
    Dword,
}

impl Size {
    pub const fn bytes(self) -> u32 {
        match self {
            Self::Byte => 1,
            Self::Word => 2,
            Self::Dword => 4,
        }
    }

    pub const fn bits(self) -> u32 {
        self.bytes() * 8
    }

    pub const fn mask(self) -> u32 {
        match self {
            Self::Byte => 0xFF,
            Self::Word => 0xFFFF,
            Self::Dword => 0xFFFF_FFFF,
        }
    }

    pub const fn sign_bit(self) -> u32 {
        1 << (self.bits() - 1)
    }
}

/// General-purpose register numbers in encoding order.
pub mod reg {
    pub const EAX: u8 = 0;
    pub const ECX: u8 = 1;
    pub const EDX: u8 = 2;
    pub const EBX: u8 = 3;
    pub const ESP: u8 = 4;
    pub const EBP: u8 = 5;
    pub const ESI: u8 = 6;
    pub const EDI: u8 = 7;
}

/// Memory operand `seg:[base + index * scale + disp]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mem {
    pub base: Option<u8>,
    pub index: Option<u8>,
    /// 1, 2, 4 or 8.
    pub scale: u8,
    pub disp: u32,
    /// `fs:` override (the thread information block); other segments are flat.
    pub fs: bool,
}

/// Instruction operand. Registers are interpreted with the instruction's operand size
/// (`Reg(4)` is `esp`, `sp` or `ah`); FPU instructions use `Reg(i)` for `st(i)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operand {
    None,
    Reg(u8),
    Mem(Mem),
    Imm(u32),
}

/// Two-operand arithmetic/logic group (`/r` field of opcodes 0x80-0x83).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alu {
    Add,
    Or,
    Adc,
    Sbb,
    And,
    Sub,
    Xor,
    Cmp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shift {
    Rol,
    Ror,
    Rcl,
    Rcr,
    Shl,
    Shr,
    Sar,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BitOp {
    Bt,
    Bts,
    Btr,
    Btc,
}

/// Condition code in encoding order (`o`, `no`, `b`, `ae`, ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cond(pub u8);

impl Cond {
    pub const NAMES: [&str; 16] =
        ["o", "no", "b", "ae", "e", "ne", "be", "a", "s", "ns", "p", "np", "l", "ge", "le", "g"];
}

/// String instruction repeat prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rep {
    None,
    /// `rep` / `repe`
    Rep,
    Repne,
}

/// FPU memory operand format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemFmt {
    F32,
    F64,
    F80,
    I16,
    I32,
    I64,
}

impl MemFmt {
    fn is_int(self) -> bool {
        matches!(self, Self::I16 | Self::I32 | Self::I64)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FArith {
    Add,
    Mul,
    Sub,
    Subr,
    Div,
    Divr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FConst {
    One,
    L2t,
    L2e,
    Pi,
    Lg2,
    Ln2,
    Zero,
}

/// x87 instruction. `fmt: None` means the operand is `st(i)` in [`Insn::src`]/[`Insn::dst`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fpu {
    /// `st0 = st0 op src`
    Arith {
        op: FArith,
        fmt: Option<MemFmt>,
    },
    /// `st(i) = st(i) op st0`, optionally popping.
    ArithTo {
        op: FArith,
        pop: bool,
    },
    /// Compares `st0` with `src`, sets C0/C2/C3, pops `pops` times.
    Com {
        fmt: Option<MemFmt>,
        pops: u8,
        unordered: bool,
    },
    /// Compares `st0` with `st(i)` into ZF/PF/CF.
    Comi {
        pop: bool,
        unordered: bool,
    },
    /// Pushes `src`.
    Ld(Option<MemFmt>),
    /// Stores `st0` into `dst`.
    St {
        fmt: Option<MemFmt>,
        pop: bool,
    },
    Const(FConst),
    Xch,
    Chs,
    Abs,
    Tst,
    Xam,
    Sqrt,
    Sin,
    Cos,
    Sincos,
    Ptan,
    Patan,
    Prem,
    Prem1,
    Rndint,
    Scale,
    F2xm1,
    Yl2x,
    Yl2xp1,
    Xtract,
    /// `fnstsw` to `dst` (memory or `ax`).
    Nstsw,
    Nstcw,
    Ldcw,
    Nclex,
    Ninit,
    Free,
    Incstp,
    Decstp,
}

/// Operation of a decoded instruction; operands live in [`Insn`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Alu(Alu),
    Test,
    Mov,
    /// Zero-extends a `Size`-sized source.
    Movzx(Size),
    /// Sign-extends a `Size`-sized source.
    Movsx(Size),
    Lea,
    Xchg,
    Inc,
    Dec,
    Not,
    Neg,
    /// One-operand forms on `al`/`ax`/`eax` (`edx:eax`) with `src`.
    Mul,
    Imul1,
    Div,
    Idiv,
    /// `dst = src * src2` (two- and three-operand `imul`).
    Imul,
    /// `dst` shifted by `src` (immediate or `cl`).
    Shift(Shift),
    /// `dst`, `src` register, count in `src2`.
    Shld,
    Shrd,
    Push,
    Pop,
    Pushad,
    Popad,
    Pushfd,
    Popfd,
    /// `cbw` / `cwde`
    Cbw,
    /// `cwd` / `cdq`
    Cwd,
    /// Branch to `src` (an absolute immediate for relative forms).
    Jcc(Cond),
    Jmp,
    Call,
    /// Pops `src` extra bytes after the return address.
    Ret,
    Leave,
    /// `enter src, src2`
    Enter,
    Loop,
    Loope,
    Loopne,
    Jecxz,
    Setcc(Cond),
    Cmovcc(Cond),
    Bt(BitOp),
    Bsf,
    Bsr,
    Bswap,
    Xadd,
    Cmpxchg,
    Movs,
    Cmps,
    Stos,
    Lods,
    Scas,
    Xlat,
    Cld,
    Std,
    Clc,
    Stc,
    Cmc,
    Sahf,
    Lahf,
    Nop,
    Int3,
    Hlt,
    Wait,
    Fpu(Fpu),
}

/// A decoded instruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Insn {
    pub op: Op,
    pub size: Size,
    pub dst: Operand,
    pub src: Operand,
    pub src2: Operand,
    pub rep: Rep,
    pub len: u8,
}

impl Insn {
    /// True if execution never continues with the next instruction (`ret`, `jmp`, `int3`, ...).
    pub fn ends_block(&self) -> bool {
        matches!(self.op, Op::Ret | Op::Jmp | Op::Int3 | Op::Hlt)
    }

    /// Absolute target of a direct branch or call.
    pub fn branch_target(&self) -> Option<u32> {
        match (self.op, self.src) {
            (Op::Jcc(_) | Op::Jmp | Op::Call | Op::Loop | Op::Loope | Op::Loopne | Op::Jecxz, Operand::Imm(t)) => {
                Some(t)
            }
            _ => None,
        }
    }
}

/// An instruction the decoder does not support.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodeError {
    pub addr: u32,
    /// The first bytes of the instruction.
    pub bytes: Vec<u8>,
    pub reason: &'static str,
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at {:#010x} (bytes", self.reason, self.addr)?;
        for b in &self.bytes {
            write!(f, " {b:02x}")?;
        }
        f.write_str(")")
    }
}

impl std::error::Error for DecodeError {}

/// Longest legal x86 instruction.
pub const MAX_INSN_LEN: usize = 15;

struct Cursor<'a> {
    code: &'a [u8],
    pos: usize,
}

type DResult<T> = Result<T, &'static str>;

impl Cursor<'_> {
    fn u8(&mut self) -> DResult<u8> {
        let b = *self.code.get(self.pos).ok_or("truncated instruction")?;
        self.pos += 1;
        if self.pos > MAX_INSN_LEN {
            return Err("instruction too long");
        }
        Ok(b)
    }

    fn i8(&mut self) -> DResult<u32> {
        Ok(self.u8()? as i8 as i32 as u32)
    }

    fn u16(&mut self) -> DResult<u32> {
        Ok(u32::from(self.u8()?) | u32::from(self.u8()?) << 8)
    }

    fn u32(&mut self) -> DResult<u32> {
        Ok(self.u16()? | self.u16()? << 16)
    }

    /// Immediate of the operand size (imm8/imm16/imm32).
    fn imm(&mut self, size: Size) -> DResult<u32> {
        match size {
            Size::Byte => self.u8().map(u32::from),
            Size::Word => self.u16(),
            Size::Dword => self.u32(),
        }
    }
}

/// Decoded ModRM byte: `reg` field and the r/m operand.
struct ModRm {
    reg: u8,
    rm: Operand,
}

impl ModRm {
    fn mem(&self) -> DResult<Operand> {
        match self.rm {
            Operand::Mem(_) => Ok(self.rm),
            _ => Err("register operand where memory is required"),
        }
    }
}

fn modrm(c: &mut Cursor, fs: bool) -> DResult<ModRm> {
    let b = c.u8()?;
    let (md, reg, rm) = (b >> 6, (b >> 3) & 7, b & 7);
    if md == 3 {
        return Ok(ModRm { reg, rm: Operand::Reg(rm) });
    }
    let mut m = Mem { base: Some(rm), index: None, scale: 1, disp: 0, fs };
    if rm == 4 {
        let sib = c.u8()?;
        let (ss, index, base) = (sib >> 6, (sib >> 3) & 7, sib & 7);
        if index != 4 {
            m.index = Some(index);
            m.scale = 1 << ss;
        }
        m.base = Some(base);
        if base == 5 && md == 0 {
            m.base = None;
            m.disp = c.u32()?;
        }
    } else if rm == 5 && md == 0 {
        m.base = None;
        m.disp = c.u32()?;
    }
    match md {
        1 => m.disp = c.i8()?,
        2 => m.disp = c.u32()?,
        _ => {}
    }
    Ok(ModRm { reg, rm: Operand::Mem(m) })
}

const ALU: [Alu; 8] = [Alu::Add, Alu::Or, Alu::Adc, Alu::Sbb, Alu::And, Alu::Sub, Alu::Xor, Alu::Cmp];
const SHIFT: [Option<Shift>; 8] = [
    Some(Shift::Rol),
    Some(Shift::Ror),
    Some(Shift::Rcl),
    Some(Shift::Rcr),
    Some(Shift::Shl),
    Some(Shift::Shr),
    Some(Shift::Shl), // undocumented `sal` alias
    Some(Shift::Sar),
];
const FARITH: [Option<FArith>; 8] = [
    Some(FArith::Add),
    Some(FArith::Mul),
    None, // fcom
    None, // fcomp
    Some(FArith::Sub),
    Some(FArith::Subr),
    Some(FArith::Div),
    Some(FArith::Divr),
];

/// Decodes one instruction at guest address `addr` from `code` (which may be longer).
pub fn decode(code: &[u8], addr: u32) -> Result<Insn, DecodeError> {
    let mut c = Cursor { code, pos: 0 };
    decode_inner(&mut c, addr).map_err(|reason| DecodeError {
        addr,
        bytes: code[..code.len().min(c.pos.max(1)).min(MAX_INSN_LEN)].to_vec(),
        reason,
    })
}

fn decode_inner(c: &mut Cursor, addr: u32) -> DResult<Insn> {
    let mut opsize16 = false;
    let mut rep = Rep::None;
    let mut fs = false;
    let mut b;
    loop {
        b = c.u8()?;
        match b {
            0x66 => opsize16 = true,
            0xF3 => rep = Rep::Rep,
            0xF2 => rep = Rep::Repne,
            0xF0 => {} // lock: single-threaded, no effect
            0x26 | 0x2E | 0x36 | 0x3E => {}
            0x64 => fs = true,
            0x65 => return Err("gs segment override"),
            0x67 => return Err("address size override"),
            _ => break,
        }
    }
    let osz = if opsize16 { Size::Word } else { Size::Dword };
    let mut i =
        Insn { op: Op::Nop, size: osz, dst: Operand::None, src: Operand::None, src2: Operand::None, rep, len: 0 };
    let size_of = |b: u8| if b & 1 == 0 { Size::Byte } else { osz };

    match b {
        // add/or/adc/sbb/and/sub/xor/cmp in their six encodings
        0x00..=0x3F if b & 7 < 6 => {
            i.op = Op::Alu(ALU[usize::from(b >> 3)]);
            i.size = size_of(b);
            match b & 7 {
                0 | 1 => {
                    let m = modrm(c, fs)?;
                    (i.dst, i.src) = (m.rm, Operand::Reg(m.reg));
                }
                2 | 3 => {
                    let m = modrm(c, fs)?;
                    (i.dst, i.src) = (Operand::Reg(m.reg), m.rm);
                }
                _ => (i.dst, i.src) = (Operand::Reg(0), Operand::Imm(c.imm(i.size)?)),
            }
        }
        0x40..=0x47 => (i.op, i.dst) = (Op::Inc, Operand::Reg(b & 7)),
        0x48..=0x4F => (i.op, i.dst) = (Op::Dec, Operand::Reg(b & 7)),
        0x50..=0x57 => (i.op, i.src) = (Op::Push, Operand::Reg(b & 7)),
        0x58..=0x5F => (i.op, i.dst) = (Op::Pop, Operand::Reg(b & 7)),
        0x60 => i.op = Op::Pushad,
        0x61 => i.op = Op::Popad,
        0x68 => (i.op, i.src) = (Op::Push, Operand::Imm(c.imm(osz)?)),
        0x6A => (i.op, i.src) = (Op::Push, Operand::Imm(c.i8()?)),
        0x69 | 0x6B => {
            let m = modrm(c, fs)?;
            i.op = Op::Imul;
            (i.dst, i.src) = (Operand::Reg(m.reg), m.rm);
            i.src2 = Operand::Imm(if b == 0x69 { c.imm(osz)? } else { c.i8()? });
        }
        0x70..=0x7F => {
            let d = c.i8()?;
            i.op = Op::Jcc(Cond(b & 15));
            i.src = Operand::Imm(addr.wrapping_add(c.pos as u32).wrapping_add(d));
        }
        0x80..=0x83 => {
            let m = modrm(c, fs)?;
            i.op = Op::Alu(ALU[usize::from(m.reg)]);
            i.size = if b == 0x81 || b == 0x83 { osz } else { Size::Byte };
            i.dst = m.rm;
            i.src = Operand::Imm(match b {
                0x81 => c.imm(osz)?,
                0x83 => c.i8()? & osz.mask(),
                _ => u32::from(c.u8()?),
            });
        }
        0x84..=0x89 => {
            let m = modrm(c, fs)?;
            i.op = match b {
                0x84 | 0x85 => Op::Test,
                0x86 | 0x87 => Op::Xchg,
                _ => Op::Mov,
            };
            i.size = size_of(b);
            (i.dst, i.src) = (m.rm, Operand::Reg(m.reg));
        }
        0x8A | 0x8B => {
            let m = modrm(c, fs)?;
            i.op = Op::Mov;
            i.size = size_of(b);
            (i.dst, i.src) = (Operand::Reg(m.reg), m.rm);
        }
        0x8D => {
            let m = modrm(c, false)?;
            i.op = Op::Lea;
            (i.dst, i.src) = (Operand::Reg(m.reg), m.mem()?);
        }
        0x8F => {
            let m = modrm(c, fs)?;
            if m.reg != 0 {
                return Err("invalid 8F encoding");
            }
            (i.op, i.dst) = (Op::Pop, m.rm);
        }
        0x90 => i.op = Op::Nop,
        0x91..=0x97 => (i.op, i.dst, i.src) = (Op::Xchg, Operand::Reg(0), Operand::Reg(b & 7)),
        0x98 => i.op = Op::Cbw,
        0x99 => i.op = Op::Cwd,
        0x9B => i.op = Op::Wait,
        0x9C => i.op = Op::Pushfd,
        0x9D => i.op = Op::Popfd,
        0x9E => i.op = Op::Sahf,
        0x9F => i.op = Op::Lahf,
        0xA0..=0xA3 => {
            let m = Operand::Mem(Mem { base: None, index: None, scale: 1, disp: c.u32()?, fs });
            i.op = Op::Mov;
            i.size = size_of(b);
            (i.dst, i.src) = if b < 0xA2 { (Operand::Reg(0), m) } else { (m, Operand::Reg(0)) };
        }
        0xA4..=0xA7 | 0xAA..=0xAF => {
            if fs {
                return Err("segment override on string instruction");
            }
            i.op = match b & !1 {
                0xA4 => Op::Movs,
                0xA6 => Op::Cmps,
                0xAA => Op::Stos,
                0xAC => Op::Lods,
                _ => Op::Scas,
            };
            i.size = size_of(b);
        }
        0xA8 | 0xA9 => {
            i.op = Op::Test;
            i.size = size_of(b);
            (i.dst, i.src) = (Operand::Reg(0), Operand::Imm(c.imm(i.size)?));
        }
        0xB0..=0xB7 => {
            i.op = Op::Mov;
            i.size = Size::Byte;
            (i.dst, i.src) = (Operand::Reg(b & 7), Operand::Imm(c.imm(Size::Byte)?));
        }
        0xB8..=0xBF => (i.op, i.dst, i.src) = (Op::Mov, Operand::Reg(b & 7), Operand::Imm(c.imm(osz)?)),
        0xC0 | 0xC1 | 0xD0..=0xD3 => {
            let m = modrm(c, fs)?;
            i.op = Op::Shift(SHIFT[usize::from(m.reg)].ok_or("invalid shift")?);
            i.size = size_of(b);
            i.dst = m.rm;
            i.src = match b {
                0xC0 | 0xC1 => Operand::Imm(u32::from(c.u8()?)),
                0xD0 | 0xD1 => Operand::Imm(1),
                _ => Operand::Reg(reg::ECX),
            };
        }
        0xC2 => (i.op, i.src) = (Op::Ret, Operand::Imm(c.u16()?)),
        0xC3 => (i.op, i.src) = (Op::Ret, Operand::Imm(0)),
        0xC6 | 0xC7 => {
            let m = modrm(c, fs)?;
            if m.reg != 0 {
                return Err("invalid C6/C7 encoding");
            }
            i.op = Op::Mov;
            i.size = size_of(b);
            (i.dst, i.src) = (m.rm, Operand::Imm(c.imm(i.size)?));
        }
        0xC8 => {
            i.op = Op::Enter;
            i.src = Operand::Imm(c.u16()?);
            i.src2 = Operand::Imm(u32::from(c.u8()?));
        }
        0xC9 => i.op = Op::Leave,
        0xCC => i.op = Op::Int3,
        0xD7 => i.op = Op::Xlat,
        0xD8..=0xDF => decode_fpu(c, b, fs, &mut i)?,
        0xE0..=0xE3 => {
            let d = c.i8()?;
            i.op = [Op::Loopne, Op::Loope, Op::Loop, Op::Jecxz][usize::from(b - 0xE0)];
            i.src = Operand::Imm(addr.wrapping_add(c.pos as u32).wrapping_add(d));
        }
        0xE8 | 0xE9 => {
            let d = c.u32()?;
            i.op = if b == 0xE8 { Op::Call } else { Op::Jmp };
            i.src = Operand::Imm(addr.wrapping_add(c.pos as u32).wrapping_add(d));
        }
        0xEB => {
            let d = c.i8()?;
            i.op = Op::Jmp;
            i.src = Operand::Imm(addr.wrapping_add(c.pos as u32).wrapping_add(d));
        }
        0xF4 => i.op = Op::Hlt,
        0xF5 => i.op = Op::Cmc,
        0xF6 | 0xF7 => {
            let m = modrm(c, fs)?;
            i.size = size_of(b);
            match m.reg {
                0 | 1 => (i.op, i.dst, i.src) = (Op::Test, m.rm, Operand::Imm(c.imm(i.size)?)),
                2 => (i.op, i.dst) = (Op::Not, m.rm),
                3 => (i.op, i.dst) = (Op::Neg, m.rm),
                r => {
                    i.op = [Op::Mul, Op::Imul1, Op::Div, Op::Idiv][usize::from(r - 4)];
                    i.src = m.rm;
                }
            }
        }
        0xF8 => i.op = Op::Clc,
        0xF9 => i.op = Op::Stc,
        0xFC => i.op = Op::Cld,
        0xFD => i.op = Op::Std,
        0xFE | 0xFF => {
            let m = modrm(c, fs)?;
            i.size = size_of(b);
            match (b, m.reg) {
                (_, 0) => (i.op, i.dst) = (Op::Inc, m.rm),
                (_, 1) => (i.op, i.dst) = (Op::Dec, m.rm),
                (0xFF, 2) => (i.op, i.size, i.src) = (Op::Call, Size::Dword, m.rm),
                (0xFF, 4) => (i.op, i.size, i.src) = (Op::Jmp, Size::Dword, m.rm),
                (0xFF, 6) => (i.op, i.src) = (Op::Push, m.rm),
                _ => return Err("unsupported FE/FF encoding (far call/jmp)"),
            }
        }
        0x0F => decode_0f(c, addr, fs, &mut i)?,
        _ => return Err("unsupported opcode"),
    }
    if i.rep != Rep::None && !matches!(i.op, Op::Movs | Op::Cmps | Op::Stos | Op::Lods | Op::Scas) {
        // `rep ret` and similar padding idioms ignore the prefix.
        i.rep = Rep::None;
    }
    i.len = c.pos as u8;
    Ok(i)
}

fn decode_0f(c: &mut Cursor, addr: u32, fs: bool, i: &mut Insn) -> DResult<()> {
    let b = c.u8()?;
    let osz = i.size;
    match b {
        0x40..=0x4F => {
            let m = modrm(c, fs)?;
            i.op = Op::Cmovcc(Cond(b & 15));
            (i.dst, i.src) = (Operand::Reg(m.reg), m.rm);
        }
        0x80..=0x8F => {
            let d = c.imm(osz)?;
            let d = if osz == Size::Word { d as u16 as i16 as u32 } else { d };
            i.op = Op::Jcc(Cond(b & 15));
            i.size = Size::Dword;
            i.src = Operand::Imm(addr.wrapping_add(c.pos as u32).wrapping_add(d));
        }
        0x90..=0x9F => {
            let m = modrm(c, fs)?;
            (i.op, i.size, i.dst) = (Op::Setcc(Cond(b & 15)), Size::Byte, m.rm);
        }
        0xA3 | 0xAB | 0xB3 | 0xBB => {
            let m = modrm(c, fs)?;
            i.op = Op::Bt([BitOp::Bt, BitOp::Bts, BitOp::Btr, BitOp::Btc][usize::from((b >> 3) & 3)]);
            (i.dst, i.src) = (m.rm, Operand::Reg(m.reg));
        }
        0xBA => {
            let m = modrm(c, fs)?;
            if m.reg < 4 {
                return Err("invalid 0F BA encoding");
            }
            i.op = Op::Bt([BitOp::Bt, BitOp::Bts, BitOp::Btr, BitOp::Btc][usize::from(m.reg - 4)]);
            (i.dst, i.src) = (m.rm, Operand::Imm(u32::from(c.u8()?)));
        }
        0xA4 | 0xA5 | 0xAC | 0xAD => {
            let m = modrm(c, fs)?;
            i.op = if b < 0xA8 { Op::Shld } else { Op::Shrd };
            (i.dst, i.src) = (m.rm, Operand::Reg(m.reg));
            i.src2 = if b & 1 == 0 { Operand::Imm(u32::from(c.u8()?)) } else { Operand::Reg(reg::ECX) };
        }
        0xAF => {
            let m = modrm(c, fs)?;
            i.op = Op::Imul;
            (i.dst, i.src) = (Operand::Reg(m.reg), m.rm);
        }
        0xB0 | 0xB1 | 0xC0 | 0xC1 => {
            let m = modrm(c, fs)?;
            i.op = if b < 0xC0 { Op::Cmpxchg } else { Op::Xadd };
            i.size = if b & 1 == 0 { Size::Byte } else { osz };
            (i.dst, i.src) = (m.rm, Operand::Reg(m.reg));
        }
        0xB6 | 0xB7 | 0xBE | 0xBF => {
            let m = modrm(c, fs)?;
            let from = if b & 1 == 0 { Size::Byte } else { Size::Word };
            i.op = if b < 0xB8 { Op::Movzx(from) } else { Op::Movsx(from) };
            (i.dst, i.src) = (Operand::Reg(m.reg), m.rm);
        }
        0xBC | 0xBD => {
            let m = modrm(c, fs)?;
            i.op = if b == 0xBC { Op::Bsf } else { Op::Bsr };
            (i.dst, i.src) = (Operand::Reg(m.reg), m.rm);
        }
        0xC8..=0xCF => (i.op, i.size, i.dst) = (Op::Bswap, Size::Dword, Operand::Reg(b & 7)),
        _ => return Err("unsupported 0F opcode"),
    }
    Ok(())
}

fn decode_fpu(c: &mut Cursor, b: u8, fs: bool, i: &mut Insn) -> DResult<()> {
    let m = modrm(c, fs)?;
    let r = m.reg;
    let st = |n: u8| Operand::Reg(n);
    let fpu = if let Operand::Mem(_) = m.rm {
        let mem = m.rm;
        // Memory forms.
        let arith = |fmt: MemFmt, i: &mut Insn| -> Fpu {
            i.src = mem;
            match FARITH[usize::from(r)] {
                Some(op) => Fpu::Arith { op, fmt: Some(fmt) },
                None => Fpu::Com { fmt: Some(fmt), pops: r - 2, unordered: false },
            }
        };
        match (b, r) {
            (0xD8, _) => arith(MemFmt::F32, i),
            (0xDC, _) => arith(MemFmt::F64, i),
            (0xDA, _) => arith(MemFmt::I32, i),
            (0xDE, _) => arith(MemFmt::I16, i),
            (0xD9, 0) | (0xDD, 0) | (0xDB, 0) | (0xDF, 0) | (0xDB, 5) | (0xDF, 5) | (0xDF, 7) => {
                // fld / fild in their formats; DF /7 is fistp m64 (handled below).
                if (b, r) == (0xDF, 7) {
                    i.dst = mem;
                    Fpu::St { fmt: Some(MemFmt::I64), pop: true }
                } else {
                    i.src = mem;
                    Fpu::Ld(Some(match (b, r) {
                        (0xD9, _) => MemFmt::F32,
                        (0xDD, _) => MemFmt::F64,
                        (0xDB, 0) => MemFmt::I32,
                        (0xDF, 0) => MemFmt::I16,
                        (0xDB, _) => MemFmt::F80,
                        _ => MemFmt::I64,
                    }))
                }
            }
            (0xD9 | 0xDD | 0xDB | 0xDF, 2 | 3) | (0xDB, 7) => {
                i.dst = mem;
                let fmt = match b {
                    0xD9 => MemFmt::F32,
                    0xDD => MemFmt::F64,
                    0xDB if r == 7 => MemFmt::F80,
                    0xDB => MemFmt::I32,
                    _ => MemFmt::I16,
                };
                Fpu::St { fmt: Some(fmt), pop: r != 2 }
            }
            (0xD9, 5) => {
                i.src = mem;
                Fpu::Ldcw
            }
            (0xD9, 7) => {
                i.dst = mem;
                Fpu::Nstcw
            }
            (0xDD, 7) => {
                i.dst = mem;
                Fpu::Nstsw
            }
            _ => return Err("unsupported FPU memory instruction"),
        }
    } else {
        let Operand::Reg(n) = m.rm else { unreachable!() };
        match (b, r) {
            (0xD8, _) => {
                i.src = st(n);
                match FARITH[usize::from(r)] {
                    Some(op) => Fpu::Arith { op, fmt: None },
                    None => Fpu::Com { fmt: None, pops: r - 2, unordered: false },
                }
            }
            (0xDC | 0xDE, 0 | 1 | 4..=7) => {
                i.dst = st(n);
                // Register forms with st(i) as destination swap sub/subr and div/divr.
                let op = [
                    FArith::Add,
                    FArith::Mul,
                    FArith::Add,
                    FArith::Add,
                    FArith::Subr,
                    FArith::Sub,
                    FArith::Divr,
                    FArith::Div,
                ][usize::from(r)];
                Fpu::ArithTo { op, pop: b == 0xDE }
            }
            (0xDE, 3) if n == 1 => {
                i.src = st(1);
                Fpu::Com { fmt: None, pops: 2, unordered: false }
            }
            (0xD9, 0) => {
                i.src = st(n);
                Fpu::Ld(None)
            }
            (0xD9, 1) => {
                i.src = st(n);
                Fpu::Xch
            }
            (0xDD, 2 | 3) => {
                i.dst = st(n);
                Fpu::St { fmt: None, pop: r == 3 }
            }
            (0xDD, 0) => {
                i.dst = st(n);
                Fpu::Free
            }
            (0xDD, 4 | 5) => {
                i.src = st(n);
                Fpu::Com { fmt: None, pops: r - 4, unordered: true }
            }
            (0xDA, 5) if n == 1 => {
                i.src = st(1);
                Fpu::Com { fmt: None, pops: 2, unordered: true }
            }
            (0xDB, 5) | (0xDF, 5) | (0xDB, 6) | (0xDF, 6) => {
                i.src = st(n);
                Fpu::Comi { pop: b == 0xDF, unordered: r == 5 }
            }
            (0xDF, 4) if n == 0 => {
                i.dst = Operand::Reg(reg::EAX);
                i.size = Size::Word;
                Fpu::Nstsw
            }
            (0xDB, 4) if n == 2 => Fpu::Nclex,
            (0xDB, 4) if n == 3 => Fpu::Ninit,
            (0xD9, 4) => match n {
                0 => Fpu::Chs,
                1 => Fpu::Abs,
                4 => Fpu::Tst,
                5 => Fpu::Xam,
                _ => return Err("unsupported FPU instruction"),
            },
            (0xD9, 5) => Fpu::Const(match n {
                0 => FConst::One,
                1 => FConst::L2t,
                2 => FConst::L2e,
                3 => FConst::Pi,
                4 => FConst::Lg2,
                5 => FConst::Ln2,
                6 => FConst::Zero,
                _ => return Err("unsupported FPU instruction"),
            }),
            (0xD9, 2) if n == 0 => {
                i.op = Op::Nop; // fnop
                return Ok(());
            }
            (0xD9, 6) => {
                [Fpu::F2xm1, Fpu::Yl2x, Fpu::Ptan, Fpu::Patan, Fpu::Xtract, Fpu::Prem1, Fpu::Decstp, Fpu::Incstp]
                    [usize::from(n)]
            }
            (0xD9, 7) => [Fpu::Prem, Fpu::Yl2xp1, Fpu::Sqrt, Fpu::Sincos, Fpu::Rndint, Fpu::Scale, Fpu::Sin, Fpu::Cos]
                [usize::from(n)],
            _ => return Err("unsupported FPU instruction"),
        }
    };
    if let Fpu::Arith { fmt: Some(f), .. } | Fpu::Com { fmt: Some(f), .. } = fpu {
        debug_assert!(f.is_int() || matches!(f, MemFmt::F32 | MemFmt::F64));
    }
    i.op = Op::Fpu(fpu);
    Ok(())
}

impl Insn {
    /// Lower-case mnemonic, matching the usual Intel names (`jne`, `fstp`, `movsd`, ...).
    pub fn mnemonic(&self) -> String {
        let sfx = |base: &str| {
            format!(
                "{base}{}",
                match self.size {
                    Size::Byte => "b",
                    Size::Word => "w",
                    Size::Dword => "d",
                }
            )
        };
        match self.op {
            Op::Alu(a) => format!("{a:?}").to_ascii_lowercase(),
            Op::Shift(s) => format!("{s:?}").to_ascii_lowercase(),
            Op::Bt(b) => format!("{b:?}").to_ascii_lowercase(),
            Op::Movzx(_) => "movzx".into(),
            Op::Movsx(_) => "movsx".into(),
            Op::Imul1 | Op::Imul => "imul".into(),
            Op::Cbw => if self.size == Size::Word { "cbw" } else { "cwde" }.into(),
            Op::Cwd => if self.size == Size::Word { "cwd" } else { "cdq" }.into(),
            Op::Jcc(c) => format!("j{}", Cond::NAMES[usize::from(c.0)]),
            Op::Setcc(c) => format!("set{}", Cond::NAMES[usize::from(c.0)]),
            Op::Cmovcc(c) => format!("cmov{}", Cond::NAMES[usize::from(c.0)]),
            Op::Movs | Op::Cmps | Op::Stos | Op::Lods | Op::Scas => sfx(&format!("{:?}", self.op).to_ascii_lowercase()),
            Op::Xlat => "xlatb".into(),
            Op::Fpu(f) => fpu_mnemonic(f),
            op => format!("{op:?}").to_ascii_lowercase(),
        }
    }
}

fn fpu_mnemonic(f: Fpu) -> String {
    let arith = |op: FArith| format!("{op:?}").to_ascii_lowercase();
    match f {
        Fpu::Arith { op, fmt } => {
            format!("f{}{}", if fmt.is_some_and(MemFmt::is_int) { "i" } else { "" }, arith(op))
        }
        Fpu::ArithTo { op, pop } => format!("f{}{}", arith(op), if pop { "p" } else { "" }),
        Fpu::Com { fmt, pops, unordered } => format!(
            "f{}{}com{}",
            if fmt.is_some_and(MemFmt::is_int) { "i" } else { "" },
            if unordered { "u" } else { "" },
            ["", "p", "pp"][usize::from(pops)]
        ),
        Fpu::Comi { pop, unordered } => {
            format!("f{}comi{}", if unordered { "u" } else { "" }, if pop { "p" } else { "" })
        }
        Fpu::Ld(fmt) => if fmt.is_some_and(MemFmt::is_int) { "fild" } else { "fld" }.into(),
        Fpu::St { fmt, pop } => {
            format!("f{}st{}", if fmt.is_some_and(MemFmt::is_int) { "i" } else { "" }, if pop { "p" } else { "" })
        }
        Fpu::Const(c) => match c {
            FConst::One => "fld1".into(),
            FConst::Zero => "fldz".into(),
            c => format!("fld{c:?}").to_ascii_lowercase(),
        },
        Fpu::F2xm1 => "f2xm1".into(),
        f => format!("f{f:?}").to_ascii_lowercase(),
    }
}

/// Formats an operand with the register names of `size` (`st(i)` for FPU instructions).
struct Shown(Operand, Size, bool);

impl fmt::Display for Shown {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const R32: [&str; 8] = ["eax", "ecx", "edx", "ebx", "esp", "ebp", "esi", "edi"];
        const R16: [&str; 8] = ["ax", "cx", "dx", "bx", "sp", "bp", "si", "di"];
        const R8: [&str; 8] = ["al", "cl", "dl", "bl", "ah", "ch", "dh", "bh"];
        let Self(op, size, fpu) = *self;
        match op {
            Operand::None => Ok(()),
            Operand::Reg(r) if fpu => write!(f, "st({r})"),
            Operand::Reg(r) => f.write_str(match size {
                Size::Byte => R8[usize::from(r)],
                Size::Word => R16[usize::from(r)],
                Size::Dword => R32[usize::from(r)],
            }),
            Operand::Imm(v) => write!(f, "{v:#x}"),
            Operand::Mem(m) => {
                f.write_str(if m.fs { "fs:[" } else { "[" })?;
                let mut plus = false;
                if let Some(b) = m.base {
                    f.write_str(R32[usize::from(b)])?;
                    plus = true;
                }
                if let Some(x) = m.index {
                    write!(f, "{}{}*{}", if plus { "+" } else { "" }, R32[usize::from(x)], m.scale)?;
                    plus = true;
                }
                if m.disp != 0 || !plus {
                    if plus && (m.disp as i32) < 0 {
                        write!(f, "-{:#x}", (m.disp as i32).unsigned_abs())?;
                    } else {
                        write!(f, "{}{:#x}", if plus { "+" } else { "" }, m.disp)?;
                    }
                }
                f.write_str("]")
            }
        }
    }
}

impl fmt::Display for Insn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.rep != Rep::None {
            f.write_str(if self.rep == Rep::Rep { "rep " } else { "repne " })?;
        }
        f.write_str(&self.mnemonic())?;
        let fpu = matches!(self.op, Op::Fpu(f) if f != Fpu::Nstsw);
        let mut sep = " ";
        for (k, op) in [self.dst, self.src, self.src2].into_iter().enumerate() {
            if op != Operand::None {
                // movzx/movsx sources use the narrow register names.
                let size = match self.op {
                    Op::Movzx(s) | Op::Movsx(s) if k == 1 => s,
                    _ => self.size,
                };
                write!(f, "{sep}{}", Shown(op, size, fpu))?;
                sep = ", ";
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(bytes: &[u8]) -> Insn {
        decode(bytes, 0x1000).unwrap()
    }

    #[test]
    fn decodes_modrm_and_sib() {
        // mov eax, [ebp-8]
        let i = d(&[0x8B, 0x45, 0xF8]);
        assert_eq!((i.op, i.len), (Op::Mov, 3));
        assert_eq!(i.src, Operand::Mem(Mem { base: Some(5), index: None, scale: 1, disp: 0xFFFF_FFF8, fs: false }));
        // lea ecx, [eax+edx*4+0x10]
        let i = d(&[0x8D, 0x4C, 0x90, 0x10]);
        assert_eq!(i.dst, Operand::Reg(1));
        assert_eq!(i.src, Operand::Mem(Mem { base: Some(0), index: Some(2), scale: 4, disp: 0x10, fs: false }));
        // mov eax, [0x10005000]
        let i = d(&[0x8B, 0x05, 0x00, 0x50, 0x00, 0x10]);
        assert_eq!(i.src, Operand::Mem(Mem { base: None, index: None, scale: 1, disp: 0x1000_5000, fs: false }));
        // jmp [ecx*4+0x10001234] (jump table, SIB without base)
        let i = d(&[0xFF, 0x24, 0x8D, 0x34, 0x12, 0x00, 0x10]);
        assert_eq!(i.op, Op::Jmp);
        assert_eq!(i.src, Operand::Mem(Mem { base: None, index: Some(1), scale: 4, disp: 0x1000_1234, fs: false }));
        // mov eax, fs:[0]
        let i = d(&[0x64, 0xA1, 0, 0, 0, 0]);
        assert!(matches!(i.src, Operand::Mem(Mem { fs: true, .. })));
        assert_eq!(i.len, 6);
    }

    #[test]
    fn decodes_branches_relative_to_next_instruction() {
        assert_eq!(d(&[0x75, 0xFE]).branch_target(), Some(0x1000));
        assert_eq!(d(&[0xE8, 0x10, 0, 0, 0]).branch_target(), Some(0x1015));
        assert_eq!(d(&[0x0F, 0x84, 0, 1, 0, 0]).branch_target(), Some(0x1106));
        assert_eq!(d(&[0x0F, 0x84, 0, 1, 0, 0]).mnemonic(), "je");
    }

    #[test]
    fn decodes_operand_size_and_immediates() {
        // add word ptr [eax], 0x1234
        let i = d(&[0x66, 0x81, 0x00, 0x34, 0x12]);
        assert_eq!((i.size, i.src, i.len), (Size::Word, Operand::Imm(0x1234), 5));
        // sub esp, -8 (sign-extended imm8)
        assert_eq!(d(&[0x83, 0xEC, 0xF8]).src, Operand::Imm(0xFFFF_FFF8));
        // rep stosd
        let i = d(&[0xF3, 0xAB]);
        assert_eq!((i.op, i.rep, i.mnemonic().as_str()), (Op::Stos, Rep::Rep, "stosd"));
        // imul eax, ecx, 0x64
        let i = d(&[0x6B, 0xC1, 0x64]);
        assert_eq!((i.op, i.dst, i.src, i.src2), (Op::Imul, Operand::Reg(0), Operand::Reg(1), Operand::Imm(0x64)));
    }

    #[test]
    fn decodes_fpu() {
        assert_eq!(d(&[0xDD, 0x45, 0xF8]).mnemonic(), "fld");
        assert_eq!(d(&[0xDE, 0xE9]).mnemonic(), "fsubp");
        assert_eq!(d(&[0xDE, 0xE1]).mnemonic(), "fsubrp");
        assert_eq!(d(&[0xDF, 0xE0]).mnemonic(), "fnstsw");
        assert_eq!(d(&[0xDF, 0x7D, 0xF0]).mnemonic(), "fistp");
        assert_eq!(d(&[0xD9, 0xE8]).mnemonic(), "fld1");
        assert_eq!(d(&[0xDA, 0x4D, 0x08]).mnemonic(), "fimul");
        assert_eq!(d(&[0xDC, 0xC9]).to_string(), "fmul st(1)");
        assert_eq!(d(&[0x0F, 0xB6, 0xC4]).to_string(), "movzx eax, ah");
        assert_eq!(d(&[0x8B, 0x44, 0x24, 0xFC]).to_string(), "mov eax, [esp-0x4]");
    }

    #[test]
    fn rejects_unsupported() {
        let e = decode(&[0xE4, 0x60], 0x1000).unwrap_err();
        assert_eq!(e.reason, "unsupported opcode");
        assert!(decode(&[0x8B], 0).is_err());
        assert!(decode(&[0x8D, 0xC0], 0).is_err());
    }
}
