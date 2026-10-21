//! Static disassembly of a loaded DLL to measure interpreter coverage.
//!
//! Follows control flow recursively from the entry point and the exports. Further roots
//! are function pointers: relocated words outside the code sections that point into code
//! (vtables, CRT initialiser tables) and relocated immediates in decoded instructions
//! (`push offset callback`). `jmp [table + reg*4]` switch tables are followed entry by
//! entry. Decoding stops at `ret`, `jmp`, `int3` and `hlt`.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use crate::loader::LoadedImage;
use crate::memory::Memory;
use crate::x86::decode::{MAX_INSN_LEN, Mem, Op, Operand};
use crate::x86::{DecodeError, decode};

/// Result of [`reachable_code`].
#[derive(Debug, Clone, Default)]
pub struct Coverage {
    /// Distinct reachable instructions decoded.
    pub instructions: usize,
    /// Reachable instructions per mnemonic.
    pub mnemonics: BTreeMap<String, usize>,
    /// Reachable code the decoder rejects (first error per address).
    pub unsupported: Vec<DecodeError>,
    /// Direct call targets and other roots (approximate function count).
    pub functions: usize,
}

/// Disassembles all code reachable from the entry point and exports.
pub fn reachable_code(mem: &Memory, image: &LoadedImage) -> Coverage {
    let relocs: HashSet<u32> = image.relocations.iter().copied().collect();
    let read = |a: u32| mem.read_u32(a).ok();
    let mut roots: BTreeSet<u32> = BTreeSet::new();
    roots.insert(image.entry_point);
    roots.extend(image.exports.iter().map(|&(_, a)| a));
    for &r in &image.relocations {
        if !image.is_code(r)
            && let Some(v) = read(r).filter(|&v| image.is_code(v))
        {
            roots.insert(v);
        }
    }
    let mut functions: BTreeSet<u32> = roots.clone();
    let mut work: Vec<u32> = roots.into_iter().collect();
    let mut seen: HashSet<u32> = HashSet::new();
    let mut cov = Coverage::default();
    let mut buf = [0u8; MAX_INSN_LEN];
    while let Some(start) = work.pop() {
        let mut pc = start;
        while image.is_code(pc) && seen.insert(pc) {
            let n = (0..MAX_INSN_LEN).take_while(|&k| mem.read_u8(pc + k as u32).map(|b| buf[k] = b).is_ok()).count();
            let insn = match decode(&buf[..n], pc) {
                Ok(i) => i,
                Err(e) => {
                    cov.unsupported.push(e);
                    break;
                }
            };
            cov.instructions += 1;
            *cov.mnemonics.entry(insn.mnemonic()).or_default() += 1;
            let len = u32::from(insn.len);
            // Relocated immediates that point into code are function pointers.
            if (pc..pc + len).any(|a| relocs.contains(&a)) {
                for op in [insn.src, insn.src2] {
                    if let Operand::Imm(v) = op
                        && image.is_code(v)
                        && insn.branch_target().is_none()
                    {
                        functions.insert(v);
                        work.push(v);
                    }
                }
            }
            if let Some(t) = insn.branch_target() {
                if insn.op == Op::Call {
                    functions.insert(t);
                }
                work.push(t);
            }
            if let (Op::Jmp, Operand::Mem(Mem { base: None, index: Some(_), scale: 4, disp, .. })) = (insn.op, insn.src)
            {
                let mut entry = disp;
                while relocs.contains(&entry)
                    && let Some(t) = read(entry).filter(|&t| image.is_code(t))
                {
                    work.push(t);
                    entry += 4;
                }
            }
            if insn.ends_block() {
                break;
            }
            pc += len;
        }
    }
    cov.functions = functions.len();
    cov
}
