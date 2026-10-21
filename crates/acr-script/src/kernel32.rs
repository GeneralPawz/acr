//! KERNEL32 functions the C runtime of the original DLLs needs.
//!
//! The DLLs were linked against the static MSVC 6 C runtime, so `KERNEL32.dll` imports are
//! almost all CRT plumbing: heap, TLS, locale/code page queries, environment and standard
//! handles. The shims emulate a single-threaded Windows XP process with an empty
//! environment, code page 1252, no files and a fixed clock, so runs are reproducible.
//! All functions are `stdcall`: the shim table records how many argument bytes the callee
//! pops.

use std::collections::BTreeMap;

use crate::heap::Heap;
use crate::loader::{LoadedImage, THUNK_AREA, THUNK_STRIDE};
use crate::memory::{Memory, PAGE_SIZE};
use crate::runtime::Error;

/// Fake module handle of the game executable (`GetModuleHandleA(NULL)`).
pub const EXE_HANDLE: u32 = 0x0040_0000;
/// Fake module handle of KERNEL32.
pub const KERNEL32_HANDLE: u32 = 0x7C80_0000;
/// Path reported for the game executable.
pub const EXE_PATH: &str = "C:\\American Conquest\\dmcr.exe";

const STD_HANDLES: [u32; 3] = [0x0000_0F00, 0x0000_0F04, 0x0000_0F08];
const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_FILE_NOT_FOUND: u32 = 2;
const ERROR_ENVVAR_NOT_FOUND: u32 = 203;
const ERROR_PROC_NOT_FOUND: u32 = 127;
const ERROR_MOD_NOT_FOUND: u32 = 126;
const ERROR_INSUFFICIENT_BUFFER: u32 = 122;

/// What a thunk is bound to.
#[derive(Debug, Clone, Copy)]
pub enum Binding {
    Shim(&'static Shim),
    /// Forwarded to the [`crate::runtime::Host`] (game API, `cdecl`).
    Host,
    /// A KERNEL32 function without a shim; calling it is an error.
    Missing,
}

/// An imported function as seen by the guest.
#[derive(Debug, Clone)]
pub struct Thunk {
    pub dll: String,
    pub name: String,
    pub binding: Binding,
}

/// Emulated Win32 process state shared by the shims.
#[derive(Debug, Clone)]
pub struct Process {
    pub heap: Heap,
    pub thunks: Vec<Thunk>,
    /// Path reported for the DLL itself.
    pub module_path: String,
    pub last_error: u32,
    tls: Vec<Option<u32>>,
    /// VirtualAlloc regions: base -> size.
    regions: BTreeMap<u32, u32>,
    virtual_next: u32,
    virtual_end: u32,
    next_heap_handle: u32,
    /// `OutputDebugStringA` and console output, in order.
    pub output: Vec<String>,
    /// Number of calls per KERNEL32 function.
    pub calls: BTreeMap<&'static str, u64>,
    command_line: u32,
    environment: u32,
    environment_w: u32,
}

/// Arguments and state available to a shim.
pub struct Ctx<'a> {
    pub p: &'a mut Process,
    pub mem: &'a mut Memory,
    pub image: &'a LoadedImage,
}

type ShimFn = fn(&mut Ctx, &[u32]) -> Result<u32, Error>;

/// A KERNEL32 function implemented in Rust.
pub struct Shim {
    pub name: &'static str,
    /// Number of 32-bit arguments (popped by the callee).
    pub args: u8,
    pub f: ShimFn,
}

impl std::fmt::Debug for Shim {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}@{}", self.name, self.args * 4)
    }
}

/// Finds the shim for a KERNEL32 export.
pub fn shim(name: &str) -> Option<&'static Shim> {
    SHIMS.iter().find(|s| s.name == name)
}

/// Names of all implemented KERNEL32 functions.
pub fn shim_names() -> impl Iterator<Item = &'static str> {
    SHIMS.iter().map(|s| s.name)
}

/// Process layout parameters.
pub struct Layout {
    /// Page for process strings (command line, environment).
    pub strings: u32,
    pub heap: (u32, u32),
    pub virtual_range: (u32, u32),
}

impl Process {
    pub fn new(mem: &mut Memory, image: &LoadedImage, module_path: &str, layout: &Layout) -> Self {
        mem.map(layout.strings, 0x1000);
        let mut at = layout.strings;
        let mut put = |bytes: &[u8]| {
            let addr = at;
            mem.write_bytes(addr, bytes).expect("strings page is mapped");
            at += (bytes.len() as u32).next_multiple_of(16);
            addr
        };
        let command_line = put(format!("\"{EXE_PATH}\"\0").as_bytes());
        let environment = put(b"\0\0");
        let environment_w = put(&[0; 4]);
        let thunks = image
            .imports
            .iter()
            .map(|i| {
                let name = i.name.to_string();
                let binding = if i.dll.eq_ignore_ascii_case("kernel32.dll") {
                    shim(&name).map_or(Binding::Missing, Binding::Shim)
                } else {
                    Binding::Host
                };
                Thunk { dll: i.dll.clone(), name, binding }
            })
            .collect();
        Self {
            heap: Heap::new(layout.heap.0, layout.heap.1),
            thunks,
            module_path: module_path.to_owned(),
            last_error: 0,
            tls: Vec::new(),
            regions: BTreeMap::new(),
            virtual_next: layout.virtual_range.0,
            virtual_end: layout.virtual_range.1,
            next_heap_handle: 0x0050_0000,
            output: Vec::new(),
            calls: BTreeMap::new(),
            command_line,
            environment,
            environment_w,
        }
    }
}

fn unsupported(function: &'static str, detail: impl Into<String>) -> Error {
    Error::Unsupported { function, detail: detail.into() }
}

impl Ctx<'_> {
    fn cstr(&self, addr: u32) -> Result<Vec<u8>, Error> {
        Ok(self.mem.read_cstr(addr, 0x10000)?)
    }

    /// Reads a string argument with explicit length (`-1` = NUL-terminated, without the NUL).
    fn str_arg(&self, addr: u32, len: u32) -> Result<(Vec<u8>, bool), Error> {
        if len as i32 == -1 {
            return Ok((self.cstr(addr)?, true));
        }
        let mut v = vec![0; len as usize];
        self.mem.read_bytes(addr, &mut v)?;
        Ok((v, false))
    }

    fn wstr_arg(&self, addr: u32, len: u32) -> Result<(Vec<u16>, bool), Error> {
        if len as i32 == -1 {
            return Ok((self.mem.read_wstr(addr, 0x10000)?, true));
        }
        let mut v = Vec::with_capacity(len as usize);
        for i in 0..len {
            v.push(self.mem.read_u16(addr + 2 * i)?);
        }
        Ok((v, false))
    }

    /// Writes `items` to a counted output buffer following the `cch`/`cb` conventions of
    /// the NLS functions: `cap == 0` queries the size, too small fails.
    fn put_counted<T: Copy>(
        &mut self,
        buf: u32,
        cap: u32,
        items: &[T],
        write: fn(&mut Memory, u32, T) -> crate::memory::Result<()>,
        width: u32,
    ) -> Result<u32, Error> {
        if cap == 0 {
            return Ok(items.len() as u32);
        }
        if (items.len() as u32) > cap {
            self.p.last_error = ERROR_INSUFFICIENT_BUFFER;
            return Ok(0);
        }
        for (i, &v) in items.iter().enumerate() {
            write(self.mem, buf + width * i as u32, v)?;
        }
        Ok(items.len() as u32)
    }

    fn virtual_alloc(&mut self, addr: u32, size: u32, kind: u32) -> u32 {
        const MEM_RESERVE: u32 = 0x2000;
        let size = size.max(1).next_multiple_of(PAGE_SIZE);
        if addr != 0 {
            // Committing inside an existing region: pages are already mapped.
            if let Some((&base, &len)) = self.p.regions.range(..=addr).next_back()
                && addr - base < len
            {
                return addr & !0xFFF;
            }
            if kind & MEM_RESERVE == 0 || self.mem.is_range_mapped(addr, 1) {
                self.p.last_error = ERROR_INVALID_PARAMETER;
                return 0;
            }
            let base = addr & !(PAGE_SIZE - 1);
            self.mem.map(base, size);
            self.p.regions.insert(base, size);
            return base;
        }
        let base = self.p.virtual_next;
        if base.checked_add(size).is_none_or(|e| e > self.p.virtual_end) {
            self.p.last_error = 8; // ERROR_NOT_ENOUGH_MEMORY
            return 0;
        }
        self.p.virtual_next = base + size;
        self.mem.map(base, size);
        self.p.regions.insert(base, size);
        base
    }

    fn virtual_free(&mut self, addr: u32, size: u32, kind: u32) -> Result<u32, Error> {
        const MEM_DECOMMIT: u32 = 0x4000;
        const MEM_RELEASE: u32 = 0x8000;
        if kind & MEM_RELEASE != 0 {
            let Some(len) = self.p.regions.remove(&addr) else {
                self.p.last_error = ERROR_INVALID_PARAMETER;
                return Ok(0);
            };
            self.mem.unmap(addr, len);
        } else if kind & MEM_DECOMMIT != 0 {
            // Recommitted pages must read as zero again.
            let start = addr & !0xFFF;
            self.mem.fill(start, (addr + size).next_multiple_of(0x1000) - start, 0)?;
        }
        Ok(1)
    }

    fn module_path(&self, handle: u32) -> Option<String> {
        if handle == 0 || handle == EXE_HANDLE {
            Some(EXE_PATH.to_owned())
        } else if handle == self.image.base {
            Some(self.p.module_path.clone())
        } else {
            None
        }
    }

    fn get_proc_address(&mut self, module: u32, name: u32) -> Result<u32, Error> {
        if name < 0x1_0000 {
            self.p.last_error = ERROR_PROC_NOT_FOUND;
            return Ok(0);
        }
        let name = String::from_utf8_lossy(&self.cstr(name)?).into_owned();
        if module == self.image.base
            && let Some(addr) = self.image.export(&name)
        {
            return Ok(addr);
        }
        let Some(s) = (module == KERNEL32_HANDLE).then(|| shim(&name)).flatten() else {
            self.p.last_error = ERROR_PROC_NOT_FOUND;
            return Ok(0);
        };
        if let Some(i) = self.p.thunks.iter().position(|t| t.name == name && matches!(t.binding, Binding::Shim(_))) {
            return Ok(LoadedImage::thunk(i));
        }
        if (self.p.thunks.len() as u32 + 2) * THUNK_STRIDE >= THUNK_AREA {
            return Err(unsupported("GetProcAddress", "out of thunks"));
        }
        self.p.thunks.push(Thunk { dll: "KERNEL32.dll".into(), name, binding: Binding::Shim(s) });
        Ok(LoadedImage::thunk(self.p.thunks.len() - 1))
    }
}

/// Windows-1252 code points for 0x80..=0x9F (undefined bytes map to themselves).
const CP1252_HIGH: [u16; 32] = [
    0x20AC, 0x0081, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039, 0x0152, 0x008D,
    0x017D, 0x008F, 0x0090, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0x02DC, 0x2122, 0x0161, 0x203A,
    0x0153, 0x009D, 0x017E, 0x0178,
];

pub fn cp1252_to_unicode(b: u8) -> u16 {
    if (0x80..0xA0).contains(&b) { CP1252_HIGH[usize::from(b - 0x80)] } else { u16::from(b) }
}

pub fn unicode_to_cp1252(c: u16) -> Option<u8> {
    if c < 0x80 || (0xA0..0x100).contains(&c) {
        return Some(c as u8);
    }
    CP1252_HIGH.iter().position(|&x| x == c).map(|i| 0x80 + i as u8)
}

/// `CT_CTYPE1` classification.
fn ctype1(c: u16) -> u16 {
    const UPPER: u16 = 0x1;
    const LOWER: u16 = 0x2;
    const DIGIT: u16 = 0x4;
    const SPACE: u16 = 0x8;
    const PUNCT: u16 = 0x10;
    const CNTRL: u16 = 0x20;
    const BLANK: u16 = 0x40;
    const XDIGIT: u16 = 0x80;
    const ALPHA: u16 = 0x100;
    let Some(ch) = char::from_u32(u32::from(c)) else { return 0 };
    let mut t = 0;
    if ch.is_uppercase() {
        t |= UPPER;
    }
    if ch.is_lowercase() {
        t |= LOWER;
    }
    if ch.is_ascii_digit() {
        t |= DIGIT;
    }
    if ch.is_whitespace() {
        t |= SPACE;
    }
    if ch.is_control() {
        t |= CNTRL;
    }
    if ch == ' ' || ch == '\t' {
        t |= BLANK;
    }
    if ch.is_ascii_hexdigit() {
        t |= XDIGIT;
    }
    if ch.is_alphabetic() {
        t |= ALPHA;
    }
    if t & (UPPER | LOWER | DIGIT | SPACE | CNTRL | ALPHA) == 0 && !ch.is_whitespace() && c > 0x20 {
        t |= PUNCT;
    }
    t
}

fn map_case(c: u16, flags: u32) -> u16 {
    const LCMAP_LOWERCASE: u32 = 0x100;
    const LCMAP_UPPERCASE: u32 = 0x200;
    let Some(ch) = char::from_u32(u32::from(c)) else { return c };
    let mapped = if flags & LCMAP_LOWERCASE != 0 {
        ch.to_lowercase().next()
    } else if flags & LCMAP_UPPERCASE != 0 {
        ch.to_uppercase().next()
    } else {
        Some(ch)
    };
    mapped.and_then(|m| u16::try_from(u32::from(m)).ok()).unwrap_or(c)
}

fn compare(a: &[u16], b: &[u16], flags: u32) -> u32 {
    let key =
        |s: &[u16]| -> Vec<u16> { s.iter().map(|&c| if flags & 1 != 0 { map_case(c, 0x100) } else { c }).collect() };
    match key(a).cmp(&key(b)) {
        std::cmp::Ordering::Less => 1,
        std::cmp::Ordering::Equal => 2,
        std::cmp::Ordering::Greater => 3,
    }
}

fn locale_info(lctype: u32) -> Option<&'static str> {
    Some(match lctype & 0xFFFF {
        0x0001 => "0409",          // LOCALE_ILANGUAGE
        0x0003 => "ENU",           // LOCALE_SABBREVLANGNAME
        0x0005 => "1",             // LOCALE_ICOUNTRY
        0x0007 => "USA",           // LOCALE_SABBREVCTRYNAME
        0x000B => "437",           // LOCALE_IDEFAULTCODEPAGE
        0x000E => ".",             // LOCALE_SDECIMAL
        0x000F => ",",             // LOCALE_STHOUSAND
        0x0014 => "$",             // LOCALE_SCURRENCY
        0x1001 => "English",       // LOCALE_SENGLANGUAGE
        0x1002 => "United States", // LOCALE_SENGCOUNTRY
        0x1004 => "1252",          // LOCALE_IDEFAULTANSICODEPAGE
        _ => return None,
    })
}

fn write_u16(mem: &mut Memory, a: u32, v: u16) -> crate::memory::Result<()> {
    mem.write_u16(a, v)
}

fn write_u8(mem: &mut Memory, a: u32, v: u8) -> crate::memory::Result<()> {
    mem.write_u8(a, v)
}

/// Fixed wall clock: Tuesday, 1 January 2002, 00:00:00.
fn write_systemtime(mem: &mut Memory, addr: u32) -> Result<u32, Error> {
    for (i, v) in [2002u16, 1, 2, 1, 0, 0, 0, 0].into_iter().enumerate() {
        mem.write_u16(addr + 2 * i as u32, v)?;
    }
    Ok(0)
}

macro_rules! shims {
    ($($name:literal $args:literal $f:expr;)*) => {
        static SHIMS: &[Shim] = &[$(Shim { name: $name, args: $args, f: $f }),*];
    };
}

shims! {
    // Heap: every heap handle shares one allocator.
    "HeapCreate" 3 |c, _| {
        c.p.next_heap_handle += 0x1000;
        Ok(c.p.next_heap_handle)
    };
    "HeapDestroy" 1 |_, _| Ok(1);
    "HeapAlloc" 3 |c, a| Ok(c.p.heap.alloc(c.mem, a[2], a[1] & 8 != 0).unwrap_or(0));
    "HeapFree" 3 |c, a| Ok(u32::from(a[2] == 0 || c.p.heap.free(a[2])));
    "HeapReAlloc" 4 |c, a| {
        const IN_PLACE_ONLY: u32 = 0x10;
        if a[1] & IN_PLACE_ONLY != 0 && c.p.heap.size(a[2]).is_some() {
            // Shrinking always works in place; growing is reported as impossible.
            return Ok(if c.p.heap.size(a[2]).is_some_and(|s| a[3] <= s) {
                c.p.heap.realloc(c.mem, a[2], a[3], false).unwrap_or(0)
            } else {
                0
            });
        }
        Ok(c.p.heap.realloc(c.mem, a[2], a[3], a[1] & 8 != 0).unwrap_or(0))
    };
    "HeapSize" 3 |c, a| Ok(c.p.heap.size(a[2]).unwrap_or(u32::MAX));
    "HeapValidate" 3 |_, _| Ok(1);
    "VirtualAlloc" 4 |c, a| Ok(c.virtual_alloc(a[0], a[1], a[2]));
    "VirtualFree" 3 |c, a| c.virtual_free(a[0], a[1], a[2]);

    // Threads and synchronisation: one thread, so locks are no-ops.
    "InitializeCriticalSection" 1 |_, _| Ok(0);
    "DeleteCriticalSection" 1 |_, _| Ok(0);
    "EnterCriticalSection" 1 |_, _| Ok(0);
    "LeaveCriticalSection" 1 |_, _| Ok(0);
    "InterlockedIncrement" 1 |c, a| {
        let v = c.mem.read_u32(a[0])?.wrapping_add(1);
        c.mem.write_u32(a[0], v)?;
        Ok(v)
    };
    "InterlockedDecrement" 1 |c, a| {
        let v = c.mem.read_u32(a[0])?.wrapping_sub(1);
        c.mem.write_u32(a[0], v)?;
        Ok(v)
    };
    "TlsAlloc" 0 |c, _| {
        let i = c.p.tls.iter().position(Option::is_none).unwrap_or(c.p.tls.len());
        if i == c.p.tls.len() {
            c.p.tls.push(None);
        }
        c.p.tls[i] = Some(0);
        Ok(i as u32)
    };
    "TlsFree" 1 |c, a| Ok(u32::from(c.p.tls.get_mut(a[0] as usize).and_then(Option::take).is_some()));
    "TlsGetValue" 1 |c, a| {
        c.p.last_error = 0;
        Ok(c.p.tls.get(a[0] as usize).copied().flatten().unwrap_or(0))
    };
    "TlsSetValue" 2 |c, a| match c.p.tls.get_mut(a[0] as usize) {
        Some(Some(v)) => {
            *v = a[1];
            Ok(1)
        }
        _ => Ok(0),
    };
    "GetCurrentThreadId" 0 |_, _| Ok(1);
    "GetCurrentThread" 0 |_, _| Ok(0xFFFF_FFFE);
    "GetCurrentProcess" 0 |_, _| Ok(0xFFFF_FFFF);
    "Sleep" 1 |_, _| Ok(0);
    "GetLastError" 0 |c, _| Ok(c.p.last_error);
    "SetLastError" 1 |c, a| {
        c.p.last_error = a[0];
        Ok(0)
    };

    // Process environment.
    "GetVersion" 0 |_, _| Ok(0x0A28_0105); // Windows XP (5.1), build 2600
    "GetVersionExA" 1 |c, a| {
        // OSVERSIONINFOA: size, major, minor, build, platform (NT), empty CSD string.
        for (i, v) in [5u32, 1, 2600, 2].into_iter().enumerate() {
            c.mem.write_u32(a[0] + 4 + 4 * i as u32, v)?;
        }
        c.mem.fill(a[0] + 20, 128, 0)?;
        Ok(1)
    };
    "GetCommandLineA" 0 |c, _| Ok(c.p.command_line);
    "GetEnvironmentStrings" 0 |c, _| Ok(c.p.environment);
    "GetEnvironmentStringsW" 0 |c, _| Ok(c.p.environment_w);
    "FreeEnvironmentStringsA" 1 |_, _| Ok(1);
    "FreeEnvironmentStringsW" 1 |_, _| Ok(1);
    "GetEnvironmentVariableA" 3 |c, _| {
        c.p.last_error = ERROR_ENVVAR_NOT_FOUND;
        Ok(0)
    };
    "SetEnvironmentVariableA" 2 |_, _| Ok(1);
    "GetStartupInfoA" 1 |c, a| {
        c.mem.fill(a[0], 68, 0)?;
        c.mem.write_u32(a[0], 68)?;
        Ok(0)
    };
    "GetStdHandle" 1 |_, a| Ok(match a[0] as i32 {
        -10 => STD_HANDLES[0],
        -11 => STD_HANDLES[1],
        -12 => STD_HANDLES[2],
        _ => u32::MAX,
    });
    "SetStdHandle" 2 |_, _| Ok(1);
    "GetFileType" 1 |_, a| Ok(if STD_HANDLES.contains(&a[0]) { 2 } else { 0 }); // FILE_TYPE_CHAR
    "SetHandleCount" 1 |_, a| Ok(a[0]);
    "SetConsoleCtrlHandler" 2 |_, _| Ok(1);
    "GetModuleFileNameA" 3 |c, a| {
        let Some(path) = c.module_path(a[0]) else {
            c.p.last_error = ERROR_MOD_NOT_FOUND;
            return Ok(0);
        };
        if a[2] == 0 {
            return Ok(0);
        }
        let n = path.len().min(a[2] as usize - 1);
        c.mem.write_bytes(a[1], &path.as_bytes()[..n])?;
        c.mem.write_u8(a[1] + n as u32, 0)?;
        Ok(n as u32)
    };
    "GetModuleHandleA" 1 |c, a| {
        if a[0] == 0 {
            return Ok(EXE_HANDLE);
        }
        let name = String::from_utf8_lossy(&c.cstr(a[0])?).to_ascii_lowercase();
        let own = c.p.module_path.rsplit(['\\', '/']).next().unwrap_or("").to_ascii_lowercase();
        Ok(match name.as_str() {
            "kernel32" | "kernel32.dll" => KERNEL32_HANDLE,
            n if n == own => c.image.base,
            _ => {
                c.p.last_error = ERROR_MOD_NOT_FOUND;
                0
            }
        })
    };
    "LoadLibraryA" 1 |c, a| {
        let name = String::from_utf8_lossy(&c.cstr(a[0])?).to_ascii_lowercase();
        if name == "kernel32.dll" || name == "kernel32" {
            return Ok(KERNEL32_HANDLE);
        }
        // user32 (message boxes) and friends are not available.
        c.p.last_error = ERROR_MOD_NOT_FOUND;
        Ok(0)
    };
    "GetProcAddress" 2 |c, a| c.get_proc_address(a[0], a[1]);

    // Code pages and locale (code page 1252, US English).
    "GetACP" 0 |_, _| Ok(1252);
    "GetOEMCP" 0 |_, _| Ok(437);
    "IsValidCodePage" 1 |_, a| Ok(u32::from(matches!(a[0], 437 | 850 | 1252)));
    "IsValidLocale" 2 |_, _| Ok(1);
    "GetUserDefaultLCID" 0 |_, _| Ok(0x0409);
    "EnumSystemLocalesA" 2 |_, _| Ok(1);
    "GetCPInfo" 2 |c, a| {
        if !matches!(a[0], 0..=3 | 437 | 850 | 1252) {
            c.p.last_error = ERROR_INVALID_PARAMETER;
            return Ok(0);
        }
        // CPINFO: MaxCharSize 1, DefaultChar "?", no lead bytes.
        c.mem.fill(a[1], 20, 0)?;
        c.mem.write_u32(a[1], 1)?;
        c.mem.write_u8(a[1] + 4, b'?')?;
        Ok(1)
    };
    "MultiByteToWideChar" 6 |c, a| {
        let (mut s, nul) = c.str_arg(a[2], a[3])?;
        if nul {
            s.push(0);
        }
        let w: Vec<u16> = s.iter().map(|&b| cp1252_to_unicode(b)).collect();
        c.put_counted(a[4], a[5], &w, write_u16, 2)
    };
    "WideCharToMultiByte" 8 |c, a| {
        let (mut w, nul) = c.wstr_arg(a[2], a[3])?;
        if nul {
            w.push(0);
        }
        let mut used_default = false;
        let s: Vec<u8> = w
            .iter()
            .map(|&ch| {
                unicode_to_cp1252(ch).unwrap_or_else(|| {
                    used_default = true;
                    b'?'
                })
            })
            .collect();
        let n = c.put_counted(a[4], a[5], &s, write_u8, 1)?;
        if a[7] != 0 {
            c.mem.write_u32(a[7], u32::from(used_default))?;
        }
        Ok(n)
    };
    "GetStringTypeA" 5 |c, a| {
        const CT_CTYPE1: u32 = 1;
        if a[1] != CT_CTYPE1 {
            return Err(unsupported("GetStringTypeA", format!("info type {}", a[1])));
        }
        let (s, _) = c.str_arg(a[2], a[3])?;
        for (i, &b) in s.iter().enumerate() {
            c.mem.write_u16(a[4] + 2 * i as u32, ctype1(cp1252_to_unicode(b)))?;
        }
        Ok(1)
    };
    "GetStringTypeW" 4 |c, a| {
        if a[0] != 1 {
            return Err(unsupported("GetStringTypeW", format!("info type {}", a[0])));
        }
        let (w, _) = c.wstr_arg(a[1], a[2])?;
        for (i, &ch) in w.iter().enumerate() {
            c.mem.write_u16(a[3] + 2 * i as u32, ctype1(ch))?;
        }
        Ok(1)
    };
    "LCMapStringA" 6 |c, a| {
        if a[1] & 0x400 != 0 {
            return Err(unsupported("LCMapStringA", "LCMAP_SORTKEY"));
        }
        let (mut s, nul) = c.str_arg(a[2], a[3])?;
        if nul {
            s.push(0);
        }
        let out: Vec<u8> =
            s.iter().map(|&b| unicode_to_cp1252(map_case(cp1252_to_unicode(b), a[1])).unwrap_or(b)).collect();
        c.put_counted(a[4], a[5], &out, write_u8, 1)
    };
    "LCMapStringW" 6 |c, a| {
        if a[1] & 0x400 != 0 {
            return Err(unsupported("LCMapStringW", "LCMAP_SORTKEY"));
        }
        let (mut w, nul) = c.wstr_arg(a[2], a[3])?;
        if nul {
            w.push(0);
        }
        let out: Vec<u16> = w.iter().map(|&ch| map_case(ch, a[1])).collect();
        c.put_counted(a[4], a[5], &out, write_u16, 2)
    };
    "CompareStringA" 6 |c, a| {
        let (x, _) = c.str_arg(a[2], a[3])?;
        let (y, _) = c.str_arg(a[4], a[5])?;
        let w = |s: &[u8]| s.iter().map(|&b| cp1252_to_unicode(b)).collect::<Vec<_>>();
        Ok(compare(&w(&x), &w(&y), a[1]))
    };
    "CompareStringW" 6 |c, a| {
        let (x, _) = c.wstr_arg(a[2], a[3])?;
        let (y, _) = c.wstr_arg(a[4], a[5])?;
        Ok(compare(&x, &y, a[1]))
    };
    "GetLocaleInfoA" 4 |c, a| {
        let Some(v) = locale_info(a[1]) else {
            c.p.last_error = ERROR_INVALID_PARAMETER;
            return Ok(0);
        };
        let mut s = v.as_bytes().to_vec();
        s.push(0);
        c.put_counted(a[2], a[3], &s, write_u8, 1)
    };
    "GetLocaleInfoW" 4 |c, a| {
        let Some(v) = locale_info(a[1]) else {
            c.p.last_error = ERROR_INVALID_PARAMETER;
            return Ok(0);
        };
        let w: Vec<u16> = v.encode_utf16().chain([0]).collect();
        c.put_counted(a[2], a[3], &w, write_u16, 2)
    };

    // Time (fixed, see `write_systemtime`).
    "GetSystemTime" 1 |c, a| write_systemtime(c.mem, a[0]);
    "GetLocalTime" 1 |c, a| write_systemtime(c.mem, a[0]);
    "GetTimeZoneInformation" 1 |c, a| {
        c.mem.fill(a[0], 172, 0)?;
        Ok(0) // TIME_ZONE_ID_UNKNOWN
    };

    // Files: only the standard handles exist; output is collected.
    "WriteFile" 5 |c, a| {
        let mut buf = vec![0; a[2] as usize];
        c.mem.read_bytes(a[1], &mut buf)?;
        if !STD_HANDLES.contains(&a[0]) {
            c.p.last_error = 6; // ERROR_INVALID_HANDLE
            return Ok(0);
        }
        c.p.output.push(String::from_utf8_lossy(&buf).into_owned());
        if a[3] != 0 {
            c.mem.write_u32(a[3], a[2])?;
        }
        Ok(1)
    };
    "ReadFile" 5 |c, _| {
        c.p.last_error = 6;
        Ok(0)
    };
    "CreateFileA" 7 |c, a| {
        let name = String::from_utf8_lossy(&c.cstr(a[0])?).into_owned();
        c.p.output.push(format!("CreateFileA({name}) -> not found"));
        c.p.last_error = ERROR_FILE_NOT_FOUND;
        Ok(u32::MAX)
    };
    "CloseHandle" 1 |_, _| Ok(1);
    "FlushFileBuffers" 1 |_, _| Ok(1);
    "SetFilePointer" 4 |c, _| {
        c.p.last_error = 6;
        Ok(u32::MAX)
    };
    "SetEndOfFile" 1 |_, _| Ok(0);
    "OutputDebugStringA" 1 |c, a| {
        let s = String::from_utf8_lossy(&c.cstr(a[0])?).into_owned();
        c.p.output.push(s);
        Ok(0)
    };

    // Pointer validation: "bad" means not mapped.
    "IsBadReadPtr" 2 |c, a| Ok(u32::from(!c.mem.is_range_mapped(a[0], a[1])));
    "IsBadWritePtr" 2 |c, a| Ok(u32::from(!c.mem.is_range_mapped(a[0], a[1])));
    "IsBadCodePtr" 1 |c, a| Ok(u32::from(!c.mem.is_mapped(a[0])));

    // Termination and exceptions.
    "ExitProcess" 1 |_, a| Err(Error::Exit(a[0]));
    "TerminateProcess" 2 |_, a| Err(Error::Exit(a[1]));
    "FatalAppExitA" 2 |c, a| {
        let msg = String::from_utf8_lossy(&c.cstr(a[1])?).into_owned();
        Err(unsupported("FatalAppExitA", msg))
    };
    "DebugBreak" 0 |_, _| Err(unsupported("DebugBreak", "guest requested a debugger"));
    // Dispatched by the runtime itself (`crate::seh`); never called through the table.
    "RaiseException" 4 |_, _| unreachable!("handled by the runtime");
    "RtlUnwind" 4 |_, _| unreachable!("handled by the runtime");
    "UnhandledExceptionFilter" 1 |_, _| Err(unsupported("UnhandledExceptionFilter", "unhandled guest exception"));
    "SetUnhandledExceptionFilter" 1 |_, _| Ok(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shim_table_is_unique_and_covers_crt_startup() {
        let mut names: Vec<_> = shim_names().collect();
        let n = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), n);
        for f in ["HeapCreate", "GetVersion", "TlsAlloc", "GetCommandLineA", "GetStartupInfoA", "GetCPInfo"] {
            assert!(shim(f).is_some(), "{f}");
        }
    }

    #[test]
    fn code_page_1252_round_trips() {
        for b in 0..=255u8 {
            assert_eq!(unicode_to_cp1252(cp1252_to_unicode(b)), Some(b));
        }
        assert_eq!(cp1252_to_unicode(0x80), 0x20AC);
        assert_eq!(unicode_to_cp1252(0x0416), None);
    }

    #[test]
    fn classifies_characters() {
        assert_eq!(ctype1(u16::from(b'A')), 0x1 | 0x80 | 0x100);
        assert_eq!(ctype1(u16::from(b'z')), 0x2 | 0x100);
        assert_eq!(ctype1(u16::from(b'7')), 0x4 | 0x80);
        assert_eq!(ctype1(u16::from(b' ')), 0x8 | 0x40);
        assert_eq!(ctype1(u16::from(b'!')), 0x10);
        assert_eq!(ctype1(0x0A), 0x8 | 0x20);
        assert_eq!(compare(&[0x61], &[0x41], 1), 2);
        assert_eq!(compare(&[0x61], &[0x41], 0), 3);
    }
}
