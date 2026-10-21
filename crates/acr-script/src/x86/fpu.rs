//! x87 floating-point unit on `f64` registers.
//!
//! MSVC's C runtime sets the x87 precision control to 53 bits, so `f64` gives the same
//! results as the hardware for `+ - * /` and `fsqrt`; only the exponent range differs.
//! Transcendental functions come from the pure-Rust `libm` crate so that results are
//! identical on every host. Exceptions are always masked: invalid operations produce NaN
//! and set the sticky status bits, as the original runs with the default control word.

use super::decode::{FArith, FConst, Fpu as Op, Insn, MemFmt, Operand};
use super::{CF, Cpu, Exception, OF, PF, SF, Size, ZF};
use crate::memory::Memory;

/// Status word bits.
const C0: u16 = 1 << 8;
const C1: u16 = 1 << 9;
const C2: u16 = 1 << 10;
const C3: u16 = 1 << 14;
const IE: u16 = 1 << 0;
const ZE: u16 = 1 << 2;
const STACK_FAULT: u16 = 1 << 6;
const TOP_SHIFT: u16 = 11;

/// x87 register stack and control/status words.
#[derive(Debug, Clone, PartialEq)]
pub struct Fpu {
    /// Physical registers; `st(i)` is `regs[(top + i) & 7]`.
    pub regs: [f64; 8],
    pub top: u8,
    /// Bit `n` set: physical register `n` is empty.
    pub empty: u8,
    pub control: u16,
    /// Status word without the TOP field.
    pub status: u16,
}

impl Default for Fpu {
    fn default() -> Self {
        Self::new()
    }
}

impl Fpu {
    pub fn new() -> Self {
        Self { regs: [0.0; 8], top: 0, empty: 0xFF, control: 0x037F, status: 0 }
    }

    fn phys(&self, i: u8) -> usize {
        usize::from((self.top + i) & 7)
    }

    pub fn st(&self, i: u8) -> f64 {
        self.regs[self.phys(i)]
    }

    pub fn set_st(&mut self, i: u8, v: f64) {
        let p = self.phys(i);
        self.regs[p] = v;
        self.empty &= !(1 << p);
    }

    pub fn push(&mut self, v: f64) {
        self.top = (self.top + 7) & 7;
        if self.empty & (1 << self.top) == 0 {
            self.status |= IE | STACK_FAULT | C1;
        }
        self.set_st(0, v);
    }

    pub fn pop(&mut self) -> f64 {
        let v = self.st(0);
        self.empty |= 1 << self.top;
        self.top = (self.top + 1) & 7;
        v
    }

    /// Status word as stored by `fnstsw`.
    pub fn status_word(&self) -> u16 {
        (self.status & !(7 << TOP_SHIFT)) | (u16::from(self.top) << TOP_SHIFT)
    }

    fn set_cc(&mut self, c3: bool, c2: bool, c1: bool, c0: bool) {
        self.status &= !(C0 | C1 | C2 | C3);
        for (on, bit) in [(c3, C3), (c2, C2), (c1, C1), (c0, C0)] {
            if on {
                self.status |= bit;
            }
        }
    }

    /// Rounds to an integer with the rounding control of the control word.
    fn round(&self, v: f64) -> f64 {
        match (self.control >> 10) & 3 {
            0 => libm::rint(v),
            1 => libm::floor(v),
            2 => libm::ceil(v),
            _ => libm::trunc(v),
        }
    }

    fn compare(&mut self, a: f64, b: f64) {
        if a.is_nan() || b.is_nan() {
            self.status |= IE;
            self.set_cc(true, true, false, true);
        } else {
            self.set_cc(a == b, false, false, a < b);
        }
    }
}

/// Converts an 80-bit extended value to `f64` (rounding to nearest).
pub fn f80_to_f64(bytes: [u8; 10]) -> f64 {
    let mant = u64::from_le_bytes(bytes[..8].try_into().unwrap());
    let se = u16::from_le_bytes([bytes[8], bytes[9]]);
    let sign = if se & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exp = i32::from(se & 0x7FFF);
    let v = if exp == 0x7FFF {
        if mant << 1 == 0 { f64::INFINITY } else { f64::NAN }
    } else if mant == 0 {
        0.0
    } else {
        libm::scalbn(mant as f64, exp.max(1) - 16383 - 63)
    };
    sign * v
}

/// Converts an `f64` to the 80-bit extended format (exact).
pub fn f64_to_f80(v: f64) -> [u8; 10] {
    let bits = v.to_bits();
    let sign = ((bits >> 63) as u16) << 15;
    let exp = ((bits >> 52) & 0x7FF) as i32;
    let frac = bits & ((1 << 52) - 1);
    let (e, mant) = if exp == 0 && frac == 0 {
        (0, 0)
    } else if exp == 0x7FF {
        (0x7FFF, (1 << 63) | (frac << 11))
    } else if exp == 0 {
        let lz = frac.leading_zeros() as i32;
        (-1074 + 16383 + 63 - lz, frac << lz)
    } else {
        (exp - 1023 + 16383, (1 << 63) | (frac << 11))
    };
    let mut out = [0; 10];
    out[..8].copy_from_slice(&mant.to_le_bytes());
    out[8..].copy_from_slice(&(sign | e as u16).to_le_bytes());
    out
}

fn arith(op: FArith, a: f64, b: f64) -> f64 {
    match op {
        FArith::Add => a + b,
        FArith::Mul => a * b,
        FArith::Sub => a - b,
        FArith::Subr => b - a,
        FArith::Div => a / b,
        FArith::Divr => b / a,
    }
}

impl Cpu {
    fn fpu_load(&self, mem: &Memory, op: &Operand, fmt: Option<MemFmt>) -> Result<f64, Exception> {
        let (Some(fmt), Operand::Mem(m)) = (fmt, op) else {
            let Operand::Reg(i) = op else { unreachable!("FPU operand") };
            return Ok(self.fpu.st(*i));
        };
        let a = self.ea(m);
        Ok(match fmt {
            MemFmt::F32 => f64::from(f32::from_bits(mem.read_u32(a)?)),
            MemFmt::F64 => f64::from_bits(mem.read_u64(a)?),
            MemFmt::F80 => {
                let mut b = [0; 10];
                mem.read_bytes(a, &mut b)?;
                f80_to_f64(b)
            }
            MemFmt::I16 => f64::from(mem.read_u16(a)? as i16),
            MemFmt::I32 => f64::from(mem.read_u32(a)? as i32),
            MemFmt::I64 => mem.read_u64(a)? as i64 as f64,
        })
    }

    fn fpu_store(&mut self, mem: &mut Memory, op: &Operand, fmt: Option<MemFmt>, v: f64) -> Result<(), Exception> {
        let (Some(fmt), Operand::Mem(m)) = (fmt, op) else {
            let Operand::Reg(i) = op else { unreachable!("FPU operand") };
            self.fpu.set_st(*i, v);
            return Ok(());
        };
        let a = self.ea(m);
        // Out-of-range or NaN integers store the "integer indefinite" value.
        let int = |fpu: &mut Fpu, bits: u32| -> u64 {
            let r = fpu.round(v);
            let limit = 2f64.powi(bits as i32 - 1);
            if r.is_nan() || r < -limit || r >= limit {
                fpu.status |= IE;
                1 << (bits - 1)
            } else {
                r as i64 as u64
            }
        };
        match fmt {
            MemFmt::F32 => mem.write_u32(a, (v as f32).to_bits())?,
            MemFmt::F64 => mem.write_u64(a, v.to_bits())?,
            MemFmt::F80 => mem.write_bytes(a, &f64_to_f80(v))?,
            MemFmt::I16 => mem.write_u16(a, int(&mut self.fpu, 16) as u16)?,
            MemFmt::I32 => mem.write_u32(a, int(&mut self.fpu, 32) as u32)?,
            MemFmt::I64 => mem.write_u64(a, int(&mut self.fpu, 64))?,
        }
        Ok(())
    }

    pub(super) fn fpu_exec(&mut self, mem: &mut Memory, i: &Insn, op: Op) -> Result<(), Exception> {
        let st_index = |o: &Operand| match o {
            Operand::Reg(n) => *n,
            _ => unreachable!("FPU register operand"),
        };
        match op {
            Op::Arith { op, fmt } => {
                let b = self.fpu_load(mem, &i.src, fmt)?;
                let a = self.fpu.st(0);
                if matches!(op, FArith::Div) && b == 0.0 || matches!(op, FArith::Divr) && a == 0.0 {
                    self.fpu.status |= ZE;
                }
                self.fpu.set_st(0, arith(op, a, b));
            }
            Op::ArithTo { op, pop } => {
                let n = st_index(&i.dst);
                let v = arith(op, self.fpu.st(n), self.fpu.st(0));
                self.fpu.set_st(n, v);
                if pop {
                    self.fpu.pop();
                }
            }
            Op::Com { fmt, pops, .. } => {
                let b = self.fpu_load(mem, &i.src, fmt)?;
                self.fpu.compare(self.fpu.st(0), b);
                for _ in 0..pops {
                    self.fpu.pop();
                }
            }
            Op::Comi { pop, .. } => {
                let (a, b) = (self.fpu.st(0), self.fpu.st(st_index(&i.src)));
                let (zf, pf, cf) = if a.is_nan() || b.is_nan() { (true, true, true) } else { (a == b, false, a < b) };
                self.set_flag(ZF, zf);
                self.set_flag(PF, pf);
                self.set_flag(CF, cf);
                self.eflags &= !(OF | SF | super::AF);
                if pop {
                    self.fpu.pop();
                }
            }
            Op::Ld(fmt) => {
                let v = self.fpu_load(mem, &i.src, fmt)?;
                self.fpu.push(v);
            }
            Op::St { fmt, pop } => {
                self.fpu_store(mem, &i.dst, fmt, self.fpu.st(0))?;
                if pop {
                    self.fpu.pop();
                }
            }
            Op::Const(c) => self.fpu.push(match c {
                FConst::One => 1.0,
                FConst::Zero => 0.0,
                FConst::Pi => std::f64::consts::PI,
                FConst::L2t => std::f64::consts::LOG2_10,
                FConst::L2e => std::f64::consts::LOG2_E,
                FConst::Lg2 => std::f64::consts::LOG10_2,
                FConst::Ln2 => std::f64::consts::LN_2,
            }),
            Op::Xch => {
                let n = st_index(&i.src);
                let (a, b) = (self.fpu.st(0), self.fpu.st(n));
                self.fpu.set_st(0, b);
                self.fpu.set_st(n, a);
            }
            Op::Chs => self.fpu.set_st(0, -self.fpu.st(0)),
            Op::Abs => self.fpu.set_st(0, self.fpu.st(0).abs()),
            Op::Tst => self.fpu.compare(self.fpu.st(0), 0.0),
            Op::Xam => {
                let v = self.fpu.st(0);
                let neg = v.is_sign_negative();
                let (c3, c2, c0) = if self.fpu.empty & (1 << self.fpu.top) != 0 {
                    (true, false, true)
                } else if v.is_nan() {
                    (false, false, true)
                } else if v.is_infinite() {
                    (false, true, true)
                } else if v == 0.0 {
                    (true, false, false)
                } else if v.is_subnormal() {
                    (true, true, false)
                } else {
                    (false, true, false)
                };
                self.fpu.set_cc(c3, c2, neg, c0);
            }
            Op::Sqrt => self.fpu.set_st(0, self.fpu.st(0).sqrt()),
            Op::Sin => self.trig(libm::sin),
            Op::Cos => self.trig(libm::cos),
            Op::Sincos => {
                let v = self.fpu.st(0);
                self.fpu.set_st(0, libm::sin(v));
                self.fpu.push(libm::cos(v));
                self.fpu.status &= !C2;
            }
            Op::Ptan => {
                self.trig(libm::tan);
                self.fpu.push(1.0);
            }
            Op::Patan => {
                let v = libm::atan2(self.fpu.st(1), self.fpu.st(0));
                self.fpu.set_st(1, v);
                self.fpu.pop();
            }
            Op::Prem | Op::Prem1 => {
                let (a, b) = (self.fpu.st(0), self.fpu.st(1));
                let (r, q) = if op == Op::Prem {
                    let r = libm::fmod(a, b);
                    (r, libm::trunc((a - r) / b) as i64)
                } else {
                    let (r, q) = libm::remquo(a, b);
                    (r, i64::from(q))
                };
                self.fpu.set_st(0, r);
                // Reduction is always complete; C0/C3/C1 hold the low quotient bits.
                self.fpu.set_cc(q & 2 != 0, false, q & 1 != 0, q & 4 != 0);
            }
            Op::Rndint => self.fpu.set_st(0, self.fpu.round(self.fpu.st(0))),
            Op::Scale => {
                let n = libm::trunc(self.fpu.st(1)).clamp(-10000.0, 10000.0) as i32;
                self.fpu.set_st(0, libm::scalbn(self.fpu.st(0), n));
            }
            Op::F2xm1 => self.fpu.set_st(0, libm::exp2(self.fpu.st(0)) - 1.0),
            Op::Yl2x | Op::Yl2xp1 => {
                let x = self.fpu.st(0);
                let l = if op == Op::Yl2x { libm::log2(x) } else { libm::log1p(x) / std::f64::consts::LN_2 };
                self.fpu.set_st(1, self.fpu.st(1) * l);
                self.fpu.pop();
            }
            Op::Xtract => {
                let v = self.fpu.st(0);
                let (m, e) = libm::frexp(v);
                self.fpu.set_st(0, f64::from(e - 1));
                self.fpu.push(m * 2.0);
            }
            Op::Nstsw => {
                let sw = u32::from(self.fpu.status_word());
                self.set(mem, &i.dst, Size::Word, sw)?;
            }
            Op::Nstcw => self.set(mem, &i.dst, Size::Word, u32::from(self.fpu.control))?,
            Op::Ldcw => self.fpu.control = self.get(mem, &i.src, Size::Word)? as u16,
            Op::Nclex => self.fpu.status &= 0x7F00,
            Op::Ninit => self.fpu = Fpu::new(),
            Op::Free => {
                let p = self.fpu.phys(st_index(&i.dst));
                self.fpu.empty |= 1 << p;
            }
            Op::Incstp => self.fpu.top = (self.fpu.top + 1) & 7,
            Op::Decstp => self.fpu.top = (self.fpu.top + 7) & 7,
        }
        Ok(())
    }

    /// Applies a trigonometric function to `st0`; like the hardware, arguments beyond
    /// 2^63 are left unchanged with C2 set.
    fn trig(&mut self, f: fn(f64) -> f64) {
        let v = self.fpu.st(0);
        if v.abs() >= 9.223_372_036_854_776e18 {
            self.fpu.status |= C2;
        } else {
            self.fpu.set_st(0, f(v));
            self.fpu.status &= !C2;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f80_round_trips() {
        for v in [0.0, -0.0, 1.0, -2.5, 1e300, 1e-310, f64::INFINITY, std::f64::consts::PI] {
            assert_eq!(f80_to_f64(f64_to_f80(v)).to_bits(), v.to_bits(), "{v}");
        }
        // 1.0 in extended precision: exponent 0x3FFF, explicit integer bit.
        assert_eq!(f64_to_f80(1.0), [0, 0, 0, 0, 0, 0, 0, 0x80, 0xFF, 0x3F]);
        assert!(f80_to_f64(f64_to_f80(f64::NAN)).is_nan());
    }
}
