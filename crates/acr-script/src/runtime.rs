//! Runs a mission/AI DLL: process setup, the interpreter loop and import dispatch.
//!
//! Guest address space (one [`Machine`] per DLL):
//!
//! | range | contents |
//! |---|---|
//! | `0x0001_0000` | thread information block (`fs:`), process strings |
//! | `0x0020_0000..0x0030_0000` | stack (1 MiB) |
//! | `0x0100_0000..0x1000_0000` | heap (`HeapAlloc`, `_ExMalloc`) |
//! | `0x1000_0000` | the DLL at its preferred image base |
//! | `0x2000_0000..0x7000_0000` | `VirtualAlloc` regions |
//! | `0x7FF0_0000..0x7FF1_0000` | import thunks, return sentinels for calls and SEH |
//!
//! Calls into an import land on its thunk address. The loop then calls the bound Rust
//! function: a KERNEL32 [`Shim`](crate::kernel32::Shim) (`stdcall`, callee pops) or the
//! [`Host`] for the game API (`dmcr.exe`, `DipServer.dll`: `cdecl`, caller pops).

use std::fmt;

use crate::heap::Heap;
use crate::kernel32::{Binding, Ctx, Layout, Process};
use crate::loader::{self, LoadedImage, THUNK_AREA, THUNK_BASE, THUNK_STRIDE};
use crate::memory::{Fault, Memory};
use crate::pe::{PeError, PeImage};
use crate::seh;
use crate::x86::decode::{MAX_INSN_LEN, reg};
use crate::x86::{Cpu, DecodeError, Exception, Insn, decode};

pub const TEB: u32 = 0x0001_0000;
const STRINGS: u32 = 0x0001_1000;
pub const STACK_BOTTOM: u32 = 0x0020_0000;
pub const STACK_TOP: u32 = 0x0030_0000;
pub const HEAP: (u32, u32) = (0x0100_0000, 0x1000_0000);
pub const VIRTUAL: (u32, u32) = (0x2000_0000, 0x7000_0000);
/// Return address pushed by [`Machine::call`]; reaching it ends the call.
pub const RETURN_SENTINEL: u32 = THUNK_BASE + THUNK_AREA - THUNK_STRIDE;
/// Default instruction budget per [`Machine::call`].
pub const DEFAULT_STEP_LIMIT: u64 = 200_000_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Load(PeError),
    /// The CPU raised an exception at `eip`.
    Cpu {
        exception: Exception,
        eip: u32,
    },
    Decode(DecodeError),
    /// The guest called an import nobody implements.
    UnknownImport {
        dll: String,
        name: String,
    },
    /// A shim was called in a way that is not emulated.
    Unsupported {
        function: &'static str,
        detail: String,
    },
    /// `ExitProcess`/`TerminateProcess` with this exit code.
    Exit(u32),
    /// The instruction budget of one call was exhausted (likely an endless loop).
    StepLimit(u64),
    /// Error reported by the host.
    Host(String),
    /// Memory fault inside a shim or host function.
    Fault(Fault),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Load(e) => write!(f, "cannot load image: {e}"),
            Self::Cpu { exception, eip } => write!(f, "{exception} at {eip:#010x}"),
            Self::Decode(e) => write!(f, "unsupported instruction: {e}"),
            Self::UnknownImport { dll, name } => write!(f, "call to unimplemented import {dll}!{name}"),
            Self::Unsupported { function, detail } => write!(f, "{function}: {detail}"),
            Self::Exit(code) => write!(f, "guest exited with code {code}"),
            Self::StepLimit(n) => write!(f, "instruction limit of {n} reached"),
            Self::Host(e) => write!(f, "host: {e}"),
            Self::Fault(e) => write!(f, "in import: {e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<Fault> for Error {
    fn from(e: Fault) -> Self {
        Self::Fault(e)
    }
}

impl From<PeError> for Error {
    fn from(e: PeError) -> Self {
        Self::Load(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// A call from guest code into a host (game API) import.
pub struct HostCall<'a> {
    pub dll: &'a str,
    pub name: &'a str,
    pub cpu: &'a mut Cpu,
    pub mem: &'a mut Memory,
    /// The guest heap, for functions that allocate (`_ExMalloc`).
    pub heap: &'a mut Heap,
}

impl HostCall<'_> {
    /// The `i`th 32-bit stack argument.
    pub fn arg(&self, i: u32) -> Result<u32> {
        Ok(self.mem.read_u32(self.cpu.esp().wrapping_add(4 + 4 * i))?)
    }

    /// Return address of the call.
    pub fn return_address(&self) -> Result<u32> {
        Ok(self.mem.read_u32(self.cpu.esp())?)
    }

    /// Reads a NUL-terminated string from guest memory.
    pub fn cstr(&self, addr: u32) -> Result<String> {
        Ok(String::from_utf8_lossy(&self.mem.read_cstr(addr, 0x10000)?).into_owned())
    }
}

/// Implements the game API the DLLs import (everything but KERNEL32).
///
/// The functions use the `cdecl` convention: the host only computes the return value
/// (`eax`); the caller removes the arguments.
pub trait Host {
    fn call(&mut self, call: &mut HostCall<'_>) -> Result<u32>;
}

/// A host that implements nothing; every game API call fails with [`Error::UnknownImport`].
pub struct NoHost;

impl Host for NoHost {
    fn call(&mut self, call: &mut HostCall<'_>) -> Result<u32> {
        Err(Error::UnknownImport { dll: call.dll.to_owned(), name: call.name.to_owned() })
    }
}

/// Decoded instructions of the image, indexed by address.
#[derive(Debug, Clone)]
struct CodeCache {
    base: u32,
    /// `slot[addr - base]`: 0 = not decoded, else index + 1 into `insns`.
    slot: Vec<u32>,
    insns: Vec<Insn>,
}

/// One loaded DLL with its own address space and CPU.
#[derive(Debug, Clone)]
pub struct Machine {
    pub cpu: Cpu,
    pub mem: Memory,
    pub image: LoadedImage,
    pub process: Process,
    cache: CodeCache,
    /// Instructions executed so far.
    pub steps: u64,
    /// Instruction budget for each [`Machine::call`].
    pub step_limit: u64,
    /// Where execution stopped when the last [`Machine::call`] failed.
    pub last_error_eip: Option<u32>,
}

impl Machine {
    /// Loads a DLL at its preferred base. `module_path` is what `GetModuleFileNameA`
    /// reports for it.
    pub fn new(data: &[u8], module_path: &str) -> Result<Self> {
        let pe = PeImage::parse(data)?;
        let mut mem = Memory::new();
        let image = loader::load(&mut mem, data, &pe, pe.image_base)?;

        mem.map(STACK_BOTTOM, STACK_TOP - STACK_BOTTOM);
        // Thread information block: no SEH frame, stack bounds, self pointer.
        mem.map(TEB, 0x1000);
        for (off, v) in [(0x00, u32::MAX), (0x04, STACK_TOP), (0x08, STACK_BOTTOM), (0x18, TEB), (0x24, 1)] {
            mem.write_u32(TEB + off, v)?;
        }
        let layout = Layout { strings: STRINGS, heap: HEAP, virtual_range: VIRTUAL };
        let process = Process::new(&mut mem, &image, module_path, &layout);

        let mut cpu = Cpu::new();
        cpu.fs_base = TEB;
        cpu.regs[usize::from(reg::ESP)] = STACK_TOP - 16;
        let cache = CodeCache { base: image.base, slot: vec![0; image.size as usize], insns: Vec::new() };
        Ok(Self { cpu, mem, image, process, cache, steps: 0, step_limit: DEFAULT_STEP_LIMIT, last_error_eip: None })
    }

    /// Runs the entry point (`_DllMainCRTStartup`) with `DLL_PROCESS_ATTACH`; this
    /// initialises the C runtime and static data. Returns the `BOOL` result.
    pub fn attach(&mut self, host: &mut dyn Host) -> Result<bool> {
        const DLL_PROCESS_ATTACH: u32 = 1;
        let r = self.call(host, self.image.entry_point, &[self.image.base, DLL_PROCESS_ATTACH, 0])?;
        Ok(r != 0)
    }

    /// Calls an exported function by name.
    pub fn call_export(&mut self, host: &mut dyn Host, name: &str, args: &[u32]) -> Result<u32> {
        let addr = self.image.export(name).ok_or_else(|| Error::Host(format!("no export named {name}")))?;
        self.call(host, addr, args)
    }

    /// Calls guest code at `addr` with 32-bit stack arguments and returns `eax`.
    ///
    /// Works for `cdecl` and `stdcall` callees: the stack pointer is restored afterwards,
    /// also after an error, so the next call starts from a clean stack.
    pub fn call(&mut self, host: &mut dyn Host, addr: u32, args: &[u32]) -> Result<u32> {
        let saved_esp = self.cpu.esp();
        let saved_eip = self.cpu.eip;
        let saved_seh = self.mem.read_u32(self.cpu.fs_base)?;
        for &a in args.iter().rev() {
            self.cpu.push(&mut self.mem, a).map_err(|e| self.cpu_error(e))?;
        }
        self.cpu.push(&mut self.mem, RETURN_SENTINEL).map_err(|e| self.cpu_error(e))?;
        self.cpu.eip = addr;
        let result = self.run(host);
        if result.is_err() {
            self.last_error_eip = Some(self.cpu.eip);
            // Leave the machine usable for the next call: drop the aborted frames.
            self.mem.write_u32(self.cpu.fs_base, saved_seh)?;
        }
        self.cpu.regs[usize::from(reg::ESP)] = saved_esp;
        self.cpu.eip = saved_eip;
        result.map(|()| self.cpu.regs[usize::from(reg::EAX)])
    }

    fn cpu_error(&self, exception: Exception) -> Error {
        Error::Cpu { exception, eip: self.cpu.eip }
    }

    /// Executes until the return sentinel is reached.
    fn run(&mut self, host: &mut dyn Host) -> Result<()> {
        let limit = self.steps.saturating_add(self.step_limit);
        loop {
            let eip = self.cpu.eip;
            if eip.wrapping_sub(THUNK_BASE) < THUNK_AREA {
                match eip {
                    RETURN_SENTINEL => return Ok(()),
                    seh::SEH_RETURN => self.seh_return()?,
                    seh::UNWIND_RETURN => self.unwind_return()?,
                    _ => self.dispatch(host, eip)?,
                }
                continue;
            }
            let insn = self.fetch(eip)?;
            self.cpu.execute(&mut self.mem, &insn).map_err(|exception| Error::Cpu { exception, eip })?;
            self.steps += 1;
            if self.steps >= limit {
                return Err(Error::StepLimit(self.step_limit));
            }
        }
    }

    /// Decodes (or looks up) the instruction at `eip`.
    fn fetch(&mut self, eip: u32) -> Result<Insn> {
        let off = eip.wrapping_sub(self.cache.base) as usize;
        if let Some(&s) = self.cache.slot.get(off)
            && s != 0
        {
            return Ok(self.cache.insns[s as usize - 1]);
        }
        let mut buf = [0u8; MAX_INSN_LEN];
        let mut n = 0;
        while n < MAX_INSN_LEN {
            match self.mem.read_u8(eip.wrapping_add(n as u32)) {
                Ok(b) => buf[n] = b,
                Err(_) if n > 0 => break,
                Err(e) => return Err(Error::Cpu { exception: Exception::Fault(e), eip }),
            }
            n += 1;
        }
        let insn = decode(&buf[..n], eip).map_err(Error::Decode)?;
        if off < self.cache.slot.len() {
            self.cache.insns.push(insn);
            self.cache.slot[off] = self.cache.insns.len() as u32;
        }
        Ok(insn)
    }

    /// Runs the import bound to the thunk at `eip` and returns to the caller.
    fn dispatch(&mut self, host: &mut dyn Host, eip: u32) -> Result<()> {
        let index = ((eip - THUNK_BASE) / THUNK_STRIDE) as usize;
        let Some(thunk) = self.process.thunks.get(index).filter(|_| (eip - THUNK_BASE).is_multiple_of(THUNK_STRIDE))
        else {
            return Err(Error::Cpu { exception: Exception::Fault(Fault { addr: eip }), eip });
        };
        let esp = self.cpu.esp();
        let ret = self.mem.read_u32(esp)?;
        let (eax, pop) = match thunk.binding {
            Binding::Shim(shim) if matches!(shim.name, "RaiseException" | "RtlUnwind") => {
                *self.process.calls.entry(shim.name).or_default() += 1;
                // Exception dispatch transfers control itself (see `seh`).
                return if shim.name == "RaiseException" { self.raise_exception() } else { self.rtl_unwind() };
            }
            Binding::Shim(shim) => {
                let mut args = [0u32; 8];
                for (i, a) in args.iter_mut().enumerate().take(usize::from(shim.args)) {
                    *a = self.mem.read_u32(esp + 4 + 4 * i as u32)?;
                }
                *self.process.calls.entry(shim.name).or_default() += 1;
                let mut ctx = Ctx { p: &mut self.process, mem: &mut self.mem, image: &self.image };
                ((shim.f)(&mut ctx, &args[..usize::from(shim.args)])?, 4 * u32::from(shim.args))
            }
            Binding::Host => {
                let (dll, name) = (thunk.dll.clone(), thunk.name.clone());
                let mut call = HostCall {
                    dll: &dll,
                    name: &name,
                    cpu: &mut self.cpu,
                    mem: &mut self.mem,
                    heap: &mut self.process.heap,
                };
                (host.call(&mut call)?, 0)
            }
            Binding::Missing => {
                return Err(Error::UnknownImport { dll: thunk.dll.clone(), name: thunk.name.clone() });
            }
        };
        self.cpu.regs[usize::from(reg::EAX)] = eax;
        self.cpu.regs[usize::from(reg::ESP)] = esp + 4 + pop;
        self.cpu.eip = ret;
        Ok(())
    }

    /// Name of the import whose thunk is `addr`, if any.
    pub fn import_at(&self, addr: u32) -> Option<(&str, &str)> {
        let off = addr.checked_sub(THUNK_BASE)?;
        let t = self.process.thunks.get((off / THUNK_STRIDE) as usize)?;
        Some((&t.dll, &t.name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: u32 = 0x1000_0000;
    const CODE_RVA: u32 = 0x1000;

    /// Builds a minimal DLL: one RWX section with the code at RVA 0x1000, the given imports
    /// and exports (offsets into the code).
    struct TestDll {
        imports: Vec<(&'static str, Vec<&'static str>)>,
    }

    impl TestDll {
        /// Import address table slots start at RVA 0x2200, one null-terminated table per DLL.
        fn iat(&self, name: &str) -> u32 {
            let mut slot = 0x2200;
            for (_, funcs) in &self.imports {
                for f in funcs {
                    if *f == name {
                        return BASE + slot;
                    }
                    slot += 4;
                }
                slot += 4;
            }
            panic!("no import {name}")
        }

        fn build(&self, code: &[u8], exports: &[(&str, u32)]) -> Vec<u8> {
            let mut d = vec![0u8; 0x2200];
            let put = |d: &mut Vec<u8>, at: usize, v: u32| d[at..at + 4].copy_from_slice(&v.to_le_bytes());
            let file = |rva: u32| (rva - CODE_RVA + 0x200) as usize;
            d[0..2].copy_from_slice(b"MZ");
            put(&mut d, 0x3C, 0x40);
            d[0x40..0x44].copy_from_slice(b"PE\0\0");
            d[0x44..0x46].copy_from_slice(&0x14Cu16.to_le_bytes());
            d[0x46..0x48].copy_from_slice(&1u16.to_le_bytes());
            d[0x54..0x56].copy_from_slice(&224u16.to_le_bytes());
            let opt = 0x58;
            d[opt..opt + 2].copy_from_slice(&0x10Bu16.to_le_bytes());
            put(&mut d, opt + 16, CODE_RVA);
            put(&mut d, opt + 28, BASE);
            put(&mut d, opt + 56, 0x3000);
            put(&mut d, opt + 60, 0x200);
            put(&mut d, opt + 92, 16);
            put(&mut d, opt + 96, 0x2800); // exports
            put(&mut d, opt + 104, 0x2000); // imports
            let s = opt + 224;
            d[s..s + 5].copy_from_slice(b".text");
            for (off, v) in [(8, 0x2000), (12, CODE_RVA), (16, 0x2000), (20, 0x200), (36, 0xE000_0020)] {
                put(&mut d, s + off, v);
            }
            d[file(CODE_RVA)..file(CODE_RVA) + code.len()].copy_from_slice(code);
            // Imports: descriptors at 0x2000, lookup tables at 0x2100, IATs at 0x2200,
            // names from 0x2400.
            let (mut desc, mut slot, mut name_at) = (0x2000, 0x2100, 0x2400);
            for (dll, funcs) in &self.imports {
                put(&mut d, file(desc), slot);
                put(&mut d, file(desc + 12), name_at);
                put(&mut d, file(desc + 16), slot + 0x100);
                d[file(name_at)..file(name_at) + dll.len()].copy_from_slice(dll.as_bytes());
                name_at += 0x20;
                for f in funcs {
                    put(&mut d, file(slot), name_at);
                    put(&mut d, file(slot + 0x100), name_at);
                    d[file(name_at) + 2..file(name_at) + 2 + f.len()].copy_from_slice(f.as_bytes());
                    name_at += 0x20;
                    slot += 4;
                }
                slot += 4;
                desc += 20;
            }
            // Exports: directory at 0x2800, then functions, names, ordinals and strings.
            let n = exports.len() as u32;
            for (off, v) in [(16, 1), (20, n), (24, n), (28, 0x2840), (32, 0x2880), (36, 0x28C0)] {
                put(&mut d, file(0x2800 + off), v);
            }
            for (i, (name, off)) in exports.iter().enumerate() {
                let i = i as u32;
                put(&mut d, file(0x2840 + 4 * i), CODE_RVA + off);
                put(&mut d, file(0x2880 + 4 * i), 0x2900 + 0x20 * i);
                d[file(0x28C0 + 2 * i)..file(0x28C0 + 2 * i) + 2].copy_from_slice(&(i as u16).to_le_bytes());
                let at = file(0x2900 + 0x20 * i);
                d[at..at + name.len()].copy_from_slice(name.as_bytes());
            }
            d
        }
    }

    fn call_iat(code: &mut Vec<u8>, iat: u32) {
        code.extend_from_slice(&[0xFF, 0x15]);
        code.extend_from_slice(&iat.to_le_bytes());
    }

    /// Host that adds its two arguments.
    struct Adder(Vec<(String, u32, u32)>);

    impl Host for Adder {
        fn call(&mut self, call: &mut HostCall<'_>) -> Result<u32> {
            let (a, b) = (call.arg(0)?, call.arg(1)?);
            self.0.push((call.name.to_owned(), a, b));
            Ok(a + b)
        }
    }

    #[test]
    fn dispatches_stdcall_shims_and_cdecl_host_calls() {
        let dll =
            TestDll { imports: vec![("KERNEL32.dll", vec!["GetVersion", "HeapCreate"]), ("dmcr.exe", vec!["Add"])] };
        let mut code = vec![0x53]; // push ebx
        call_iat(&mut code, dll.iat("GetVersion"));
        code.extend_from_slice(&[0x89, 0xC3]); // mov ebx, eax
        code.extend_from_slice(&[0x6A, 0, 0x6A, 0, 0x6A, 0]); // HeapCreate(0, 0, 0): stdcall, pops 12
        call_iat(&mut code, dll.iat("HeapCreate"));
        code.extend_from_slice(&[0xFF, 0x74, 0x24, 0x0C, 0x6A, 40]); // push [esp+12] (2nd argument); push 40
        call_iat(&mut code, dll.iat("Add"));
        code.extend_from_slice(&[0x83, 0xC4, 0x08, 0x01, 0xD8, 0x5B, 0xC3]); // add esp, 8; add eax, ebx; pop ebx; ret
        let mut m = Machine::new(&dll.build(&code, &[("Run", 0)]), "test.dll").unwrap();
        let mut host = Adder(Vec::new());
        let esp = m.cpu.esp();
        let r = m.call_export(&mut host, "Run", &[1, 2]).unwrap();
        assert_eq!(r, 42 + 0x0A28_0105);
        assert_eq!(host.0, [("Add".to_owned(), 40, 2)]);
        assert_eq!(m.cpu.esp(), esp);
        assert_eq!(m.process.calls.get("HeapCreate"), Some(&1));
    }

    #[test]
    fn unknown_imports_fail_with_their_name() {
        let dll = TestDll { imports: vec![("KERNEL32.dll", vec!["CreateThread"])] };
        let mut code = vec![];
        call_iat(&mut code, dll.iat("CreateThread"));
        code.push(0xC3);
        let mut m = Machine::new(&dll.build(&code, &[("Run", 0)]), "test.dll").unwrap();
        let e = m.call_export(&mut NoHost, "Run", &[]).unwrap_err();
        assert_eq!(e, Error::UnknownImport { dll: "KERNEL32.dll".into(), name: "CreateThread".into() });
        assert_eq!(e.to_string(), "call to unimplemented import KERNEL32.dll!CreateThread");
        // The machine stays usable.
        assert!(matches!(m.call_export(&mut NoHost, "Run", &[]), Err(Error::UnknownImport { .. })));
    }

    /// `push handler; push fs:[0]; mov fs:[0], esp; RaiseException(1, 0, 0, 0);
    /// pop fs:[0]; add esp, 4; ret`, followed by a handler that sets the context's `eax` to
    /// 77 and returns `disposition`.
    fn seh_dll(disposition: u8) -> Vec<u8> {
        let dll = TestDll { imports: vec![("KERNEL32.dll", vec!["RaiseException"])] };
        let handler = BASE + CODE_RVA + 44;
        let mut code = vec![0x68];
        code.extend_from_slice(&handler.to_le_bytes());
        code.extend_from_slice(&[0x64, 0xFF, 0x35, 0, 0, 0, 0, 0x64, 0x89, 0x25, 0, 0, 0, 0]);
        code.extend_from_slice(&[0x6A, 0, 0x6A, 0, 0x6A, 0, 0x6A, 0x01]);
        call_iat(&mut code, dll.iat("RaiseException"));
        code.extend_from_slice(&[0x64, 0x8F, 0x05, 0, 0, 0, 0, 0x83, 0xC4, 0x04, 0xC3]);
        assert_eq!(code.len(), 44);
        // handler: mov eax, [esp+12]; mov dword [eax+0xB0], 77; mov eax, disposition; ret
        code.extend_from_slice(&[0x8B, 0x44, 0x24, 0x0C, 0xC7, 0x80, 0xB0, 0, 0, 0, 77, 0, 0, 0]);
        code.extend_from_slice(&[0xB8, disposition, 0, 0, 0, 0xC3]);
        dll.build(&code, &[("Run", 0)])
    }

    #[test]
    fn raise_exception_runs_seh_handlers() {
        let mut m = Machine::new(&seh_dll(0), "test.dll").unwrap();
        assert_eq!(m.call_export(&mut NoHost, "Run", &[]), Ok(77));
        assert_eq!(m.mem.read_u32(TEB).unwrap(), u32::MAX, "frame unlinked");
        // ExceptionContinueSearch with no further handler: unhandled.
        let mut m = Machine::new(&seh_dll(1), "test.dll").unwrap();
        let e = m.call_export(&mut NoHost, "Run", &[]).unwrap_err();
        assert!(e.to_string().contains("unhandled exception 0x00000001"), "{e}");
        assert_eq!(m.mem.read_u32(TEB).unwrap(), u32::MAX, "chain restored after the error");
    }

    #[test]
    fn step_limit_stops_endless_loops() {
        let dll = TestDll { imports: vec![] };
        let mut m = Machine::new(&dll.build(&[0xEB, 0xFE], &[("Run", 0)]), "test.dll").unwrap();
        m.step_limit = 1000;
        assert_eq!(m.call_export(&mut NoHost, "Run", &[]), Err(Error::StepLimit(1000)));
    }
}
