//! Interpreter tests on hand-assembled instruction sequences.

use super::decode::reg::*;
use super::*;

const CODE: u32 = 0x1_0000;
const DATA: u32 = 0x2_0000;
const STACK_TOP: u32 = 0x3_0000 - 16;

fn try_run(code: &[u8], setup: impl FnOnce(&mut Cpu, &mut Memory)) -> std::result::Result<(Cpu, Memory), Exception> {
    let mut mem = Memory::new();
    mem.map(CODE, 0x3_0000);
    mem.write_bytes(CODE, code).unwrap();
    let mut cpu = Cpu::new();
    cpu.eip = CODE;
    cpu.regs[usize::from(ESP)] = STACK_TOP;
    setup(&mut cpu, &mut mem);
    let end = CODE + code.len() as u32;
    for _ in 0..10_000 {
        if !(CODE..end).contains(&cpu.eip) {
            return Ok((cpu, mem));
        }
        let mut buf = [0; 15];
        mem.read_bytes(cpu.eip, &mut buf).unwrap();
        let insn = decode(&buf, cpu.eip).unwrap();
        cpu.execute(&mut mem, &insn)?;
    }
    panic!("no termination");
}

/// Runs `code` until execution leaves it; `setup` prepares registers and memory.
fn run_with(code: &[u8], setup: impl FnOnce(&mut Cpu, &mut Memory)) -> (Cpu, Memory) {
    try_run(code, setup).unwrap()
}

fn run(code: &[u8]) -> Cpu {
    run_with(code, |_, _| {}).0
}

fn set_regs(cpu: &mut Cpu, init: &[(u8, u32)]) {
    for &(r, v) in init {
        cpu.regs[usize::from(r)] = v;
    }
}

fn regs(code: &[u8], init: &[(u8, u32)]) -> Cpu {
    run_with(code, |cpu, _| set_regs(cpu, init)).0
}

fn fails(code: &[u8], init: &[(u8, u32)]) -> Exception {
    try_run(code, |cpu, _| set_regs(cpu, init)).unwrap_err()
}

/// `(CF, ZF, SF, OF)`
fn czso(cpu: &Cpu) -> (bool, bool, bool, bool) {
    (cpu.flag(CF), cpu.flag(ZF), cpu.flag(SF), cpu.flag(OF))
}

/// Opcode bytes followed by a 32-bit absolute address.
fn at(op: &[u8], addr: u32) -> Vec<u8> {
    let mut v = op.to_vec();
    v.extend_from_slice(&addr.to_le_bytes());
    v
}

fn put_f64(m: &mut Memory, addr: u32, v: f64) {
    m.write_u64(addr, v.to_bits()).unwrap();
}

#[test]
fn add_and_sub_set_all_flags() {
    // add eax, ebx
    let c = regs(&[0x01, 0xD8], &[(EAX, 0x7FFF_FFFF), (EBX, 1)]);
    assert_eq!(c.regs[0], 0x8000_0000);
    assert_eq!(czso(&c), (false, false, true, true));
    assert!(c.flag(AF));
    let c = regs(&[0x01, 0xD8], &[(EAX, 0xFFFF_FFFF), (EBX, 1)]);
    assert_eq!((c.regs[0], czso(&c)), (0, (true, true, false, false)));
    assert!(c.flag(PF));
    // sub eax, ebx: 1 - 2 borrows
    let c = regs(&[0x29, 0xD8], &[(EAX, 1), (EBX, 2)]);
    assert_eq!((c.regs[0], czso(&c)), (0xFFFF_FFFF, (true, false, true, false)));
    // sub eax, ebx: INT_MIN - 1 overflows
    let c = regs(&[0x29, 0xD8], &[(EAX, 0x8000_0000), (EBX, 1)]);
    assert_eq!(czso(&c), (false, false, false, true));
    // add al, 0x80 leaves the rest of eax alone
    let c = regs(&[0x04, 0x80], &[(EAX, 0x1234_5680)]);
    assert_eq!((c.regs[0], czso(&c)), (0x1234_5600, (true, true, false, true)));
    // add word ptr: add ax, 0xFFFF
    let c = regs(&[0x66, 0x05, 0xFF, 0xFF], &[(EAX, 0xAAAA_0001)]);
    assert_eq!((c.regs[0], czso(&c)), (0xAAAA_0000, (true, true, false, false)));
}

#[test]
fn adc_and_sbb_chain_64_bit_arithmetic() {
    // add eax, ecx; adc edx, ebx  (edx:eax += ebx:ecx)
    let c = regs(&[0x01, 0xC8, 0x11, 0xDA], &[(EAX, 0xFFFF_FFFF), (EDX, 1), (ECX, 1), (EBX, 2)]);
    assert_eq!((c.regs[0], c.regs[2]), (0, 4));
    // sub eax, ecx; sbb edx, ebx
    let c = regs(&[0x29, 0xC8, 0x19, 0xDA], &[(EAX, 0), (EDX, 4), (ECX, 1), (EBX, 2)]);
    assert_eq!((c.regs[0], c.regs[2]), (0xFFFF_FFFF, 1));
    // cmp ecx, ebx; sbb eax, eax: the MSVC idiom for "eax = ecx < ebx ? -1 : 0"
    let c = regs(&[0x39, 0xD9, 0x1B, 0xC0], &[(ECX, 1), (EBX, 2), (EAX, 77)]);
    assert_eq!(c.regs[0], 0xFFFF_FFFF);
    // stc; adc eax, 0xFFFFFFFF carries out again
    let c = regs(&[0xF9, 0x83, 0xD0, 0xFF], &[(EAX, 0)]);
    assert_eq!((c.regs[0], c.flag(CF)), (0, true));
}

#[test]
fn conditions_follow_signed_and_unsigned_compares() {
    for (a, b) in [(1u32, 2u32), (2, 1), (5, 5), (0xFFFF_FFFF, 1), (1, 0xFFFF_FFFF), (0x8000_0000, 0x7FFF_FFFF)] {
        // cmp eax, ebx
        let c = regs(&[0x39, 0xD8], &[(EAX, a), (EBX, b)]);
        let (sa, sb) = (a as i32, b as i32);
        let d = a.wrapping_sub(b);
        let expect = [
            sa.checked_sub(sb).is_none(),
            sa.checked_sub(sb).is_some(),
            a < b,
            a >= b,
            a == b,
            a != b,
            a <= b,
            a > b,
            (d as i32) < 0,
            (d as i32) >= 0,
            (d as u8).count_ones().is_multiple_of(2),
            (d as u8).count_ones() % 2 == 1,
            sa < sb,
            sa >= sb,
            sa <= sb,
            sa > sb,
        ];
        for (cc, &e) in expect.iter().enumerate() {
            assert_eq!(c.cond(Cond(cc as u8)), e, "cmp {a:#x}, {b:#x}: j{}", Cond::NAMES[cc]);
        }
    }
}

#[test]
fn branches_and_loops() {
    // xor eax, eax; mov ecx, 10; L: add eax, ecx; dec ecx; jnz L
    let c = run(&[0x31, 0xC0, 0xB9, 10, 0, 0, 0, 0x01, 0xC8, 0x49, 0x75, 0xFB]);
    assert_eq!((c.regs[0], c.regs[1]), (55, 0));
    // mov ecx, 3; xor eax, eax; L: inc eax; loop L
    let c = run(&[0xB9, 3, 0, 0, 0, 0x31, 0xC0, 0x40, 0xE2, 0xFD]);
    assert_eq!(c.regs[0], 3);
    // jecxz skips when ecx == 0: jecxz +5; mov eax, 1
    let c = regs(&[0xE3, 0x05, 0xB8, 1, 0, 0, 0], &[(ECX, 0)]);
    assert_eq!(c.regs[0], 0);
    // je rel32 not taken / jg rel32 taken: cmp eax, 1; je +5; jg +5; mov ebx, 1; (end)
    let c = regs(&[0x83, 0xF8, 0x01, 0x0F, 0x84, 5, 0, 0, 0, 0x0F, 0x8F, 5, 0, 0, 0, 0xBB, 1, 0, 0, 0], &[(EAX, 2)]);
    assert_eq!(c.regs[3], 0);
}

#[test]
fn inc_dec_keep_carry_and_neg_sets_it() {
    // stc; inc eax
    let c = regs(&[0xF9, 0x40], &[(EAX, 0x7FFF_FFFF)]);
    assert_eq!(czso(&c), (true, false, true, true));
    // clc; dec eax -> 0
    let c = regs(&[0xF8, 0x48], &[(EAX, 1)]);
    assert_eq!(czso(&c), (false, true, false, false));
    // neg eax
    let c = regs(&[0xF7, 0xD8], &[(EAX, 5)]);
    assert_eq!((c.regs[0], c.flag(CF)), (0xFFFF_FFFB, true));
    let c = regs(&[0xF7, 0xD8], &[(EAX, 0)]);
    assert!(!c.flag(CF) && c.flag(ZF));
    // neg INT_MIN overflows
    let c = regs(&[0xF7, 0xD8], &[(EAX, 0x8000_0000)]);
    assert_eq!((c.regs[0], c.flag(OF)), (0x8000_0000, true));
}

#[test]
fn logic_ops_clear_carry_and_overflow() {
    // stc; and eax, ebx
    let c = regs(&[0xF9, 0x21, 0xD8], &[(EAX, 0xF0F0), (EBX, 0x0FF0)]);
    assert_eq!((c.regs[0], czso(&c)), (0x00F0, (false, false, false, false)));
    // test eax, eax with negative value
    let c = regs(&[0x85, 0xC0], &[(EAX, 0x8000_0000)]);
    assert_eq!(czso(&c), (false, false, true, false));
    // stc; not eax leaves the flags alone
    let c = regs(&[0xF9, 0xF7, 0xD0], &[(EAX, 0)]);
    assert_eq!((c.regs[0], c.flag(CF)), (0xFFFF_FFFF, true));
    // xor eax, eax sets ZF and PF
    let c = regs(&[0x31, 0xC0], &[(EAX, 0x1234)]);
    assert!(c.flag(ZF) && c.flag(PF) && !c.flag(SF));
}

#[test]
fn byte_and_word_registers() {
    // mov ah, 0x12; mov al, 0x34; mov bx, ax; movzx ecx, ah; movsx edx, al
    let c = run(&[0xB4, 0x12, 0xB0, 0x34, 0x66, 0x89, 0xC3, 0x0F, 0xB6, 0xCC, 0x0F, 0xBE, 0xD0]);
    assert_eq!(c.regs[0] & 0xFFFF, 0x1234);
    assert_eq!(c.regs[3] & 0xFFFF, 0x1234);
    assert_eq!((c.regs[1], c.regs[2]), (0x12, 0x34));
    // mov al, 0x80; movsx edx, al; cbw; cwde; cdq
    let c = run(&[0xB0, 0x80, 0x0F, 0xBE, 0xD0, 0x66, 0x98, 0x98, 0x99]);
    assert_eq!((c.regs[0], c.regs[2]), (0xFFFF_FF80, 0xFFFF_FFFF));
    // xchg ah, al via 86 E0; mov word [DATA], ax; movzx esi, word [DATA]
    let mut code = vec![0x86, 0xE0, 0x66, 0xA3];
    code.extend_from_slice(&DATA.to_le_bytes());
    code.extend(at(&[0x0F, 0xB7, 0x35], DATA));
    let c = regs(&code, &[(EAX, 0xFFFF_1234)]);
    assert_eq!((c.regs[0], c.regs[6]), (0xFFFF_3412, 0x3412));
}

#[test]
fn shifts_and_rotates() {
    // shl eax, 1 (CF out, OF = msb ^ cf)
    let c = regs(&[0xD1, 0xE0], &[(EAX, 0xC000_0000)]);
    assert_eq!((c.regs[0], c.flag(CF), c.flag(OF)), (0x8000_0000, true, false));
    // shr eax, cl
    let c = regs(&[0xD3, 0xE8], &[(EAX, 0x0000_0181), (ECX, 4)]);
    assert_eq!((c.regs[0], c.flag(CF)), (0x18, false));
    let c = regs(&[0xD3, 0xE8], &[(EAX, 0x0000_0188), (ECX, 4)]);
    assert!(c.flag(CF));
    // sar eax, 31
    let c = regs(&[0xC1, 0xF8, 31], &[(EAX, 0x8000_0000)]);
    assert_eq!(c.regs[0], 0xFFFF_FFFF);
    // stc; shl eax, cl with cl = 0 changes nothing
    let c = regs(&[0xF9, 0xD3, 0xE0], &[(EAX, 1), (ECX, 0)]);
    assert_eq!((c.regs[0], c.flag(CF)), (1, true));
    // the count is masked to 5 bits: shl eax, 33 == shl eax, 1
    let c = regs(&[0xC1, 0xE0, 33], &[(EAX, 1)]);
    assert_eq!(c.regs[0], 2);
    // rol al, 1 / ror eax, 8
    let c = regs(&[0xD0, 0xC0], &[(EAX, 0x81)]);
    assert_eq!((c.regs[0], c.flag(CF)), (0x03, true));
    let c = regs(&[0xC1, 0xC8, 8], &[(EAX, 0x1234_5678)]);
    assert_eq!(c.regs[0], 0x7812_3456);
    // stc; rcr eax, 1 (64-bit shift idiom)
    let c = regs(&[0xF9, 0xD1, 0xD8], &[(EAX, 2)]);
    assert_eq!((c.regs[0], c.flag(CF)), (0x8000_0001, false));
    // clc; rcl al, 1
    let c = regs(&[0xF8, 0xD0, 0xD0], &[(EAX, 0x80)]);
    assert_eq!((c.regs[0] & 0xFF, c.flag(CF)), (0, true));
    // shld eax, edx, 4 / shrd eax, edx, cl
    let c = regs(&[0x0F, 0xA4, 0xD0, 4], &[(EAX, 0x1234_5678), (EDX, 0x9ABC_DEF0)]);
    assert_eq!((c.regs[0], c.flag(CF)), (0x2345_6789, true));
    let c = regs(&[0x0F, 0xAD, 0xD0], &[(EAX, 0x1234_5678), (EDX, 0x9ABC_DEF0), (ECX, 8)]);
    assert_eq!((c.regs[0], c.flag(CF)), (0xF012_3456, false));
}

#[test]
fn multiply_and_divide() {
    // mul ebx
    let c = regs(&[0xF7, 0xE3], &[(EAX, 0x8000_0000), (EBX, 4)]);
    assert_eq!((c.regs[0], c.regs[2], c.flag(CF), c.flag(OF)), (0, 2, true, true));
    // imul ebx (one operand, fits in eax)
    let c = regs(&[0xF7, 0xEB], &[(EAX, (-3i32) as u32), (EBX, 7)]);
    assert_eq!((c.regs[0], c.regs[2], c.flag(CF)), ((-21i32) as u32, 0xFFFF_FFFF, false));
    // imul eax, ecx, 0x64 / imul eax, ecx overflowing
    let c = regs(&[0x6B, 0xC1, 0x64], &[(ECX, 3)]);
    assert_eq!((c.regs[0], c.flag(OF)), (300, false));
    let c = regs(&[0x0F, 0xAF, 0xC1], &[(EAX, 0x1_0000), (ECX, 0x1_0000)]);
    assert_eq!((c.regs[0], c.flag(OF), c.flag(CF)), (0, true, true));
    // div ecx: edx:eax = 0x1_0000_0005 / 0x10
    let c = regs(&[0xF7, 0xF1], &[(EAX, 5), (EDX, 1), (ECX, 0x10)]);
    assert_eq!((c.regs[0], c.regs[2]), (0x1000_0000, 5));
    // cdq; idiv ecx: -7 / 2 = -3 remainder -1
    let c = regs(&[0x99, 0xF7, 0xF9], &[(EAX, (-7i32) as u32), (ECX, 2)]);
    assert_eq!((c.regs[0] as i32, c.regs[2] as i32), (-3, -1));
    // div bl uses ax
    let c = regs(&[0xF6, 0xF3], &[(EAX, 1000), (EBX, 7)]);
    assert_eq!(c.regs[0] & 0xFFFF, (6 << 8) | 142);
    // mul bl writes ax
    let c = regs(&[0xF6, 0xE3], &[(EAX, 0xFFFF_FF10), (EBX, 0x10)]);
    assert_eq!((c.regs[0], c.flag(CF)), (0xFFFF_0100, true));
    // division by zero and quotient overflow raise #DE
    assert_eq!(fails(&[0xF7, 0xF1], &[(ECX, 0)]), Exception::DivideError);
    assert_eq!(fails(&[0xF7, 0xF1], &[(EDX, 5), (ECX, 2)]), Exception::DivideError);
    assert_eq!(
        fails(&[0xF7, 0xF9], &[(EAX, 0x8000_0000), (EDX, 0xFFFF_FFFF), (ECX, 0xFFFF_FFFF)]),
        Exception::DivideError
    );
}

#[test]
fn stack_calls_and_frames() {
    let code = [
        0x6A, 42, // push 42
        0xE8, 2, 0, 0, 0, // call f
        0xEB, 10, // jmp end
        // f: push ebp; mov ebp, esp; mov eax, [ebp+8]; pop ebp; ret 4
        0x55, 0x89, 0xE5, 0x8B, 0x45, 0x08, 0x5D, 0xC2, 4, 0,
    ];
    let c = run(&code);
    assert_eq!((c.regs[0], c.esp()), (42, STACK_TOP));
    // enter 8, 0; mov eax, esp; leave
    let c = run(&[0xC8, 8, 0, 0, 0x89, 0xE0, 0xC9]);
    assert_eq!(c.regs[0], STACK_TOP - 12);
    assert_eq!(c.esp(), STACK_TOP);
    // push esp pushes the old value; pop [esp] addresses with the incremented esp
    let (c, m) = run_with(&[0x54, 0x8F, 0x04, 0x24], |_, _| {});
    assert_eq!(c.esp(), STACK_TOP);
    assert_eq!(m.read_u32(STACK_TOP).unwrap(), STACK_TOP);
    // pushad; xor eax, eax; xor esi, esi; popad
    let c = regs(&[0x60, 0x31, 0xC0, 0x31, 0xF6, 0x61], &[(EAX, 7), (ESI, 9)]);
    assert_eq!((c.regs[0], c.regs[6], c.esp()), (7, 9, STACK_TOP));
    // push word 0x1234 (66 68) moves esp by 2
    let c = run(&[0x66, 0x68, 0x34, 0x12]);
    assert_eq!(c.esp(), STACK_TOP - 2);
}

#[test]
fn indirect_calls_and_jump_tables() {
    // mov eax, 1; jmp [eax*4 + table]; t0: mov ebx, 10; jmp end; t1: mov ebx, 20
    let mut code = vec![0xB8, 1, 0, 0, 0];
    code.extend(at(&[0xFF, 0x24, 0x85], DATA));
    let t0 = CODE + code.len() as u32;
    code.extend_from_slice(&[0xBB, 10, 0, 0, 0, 0xEB, 5]);
    let t1 = CODE + code.len() as u32;
    code.extend_from_slice(&[0xBB, 20, 0, 0, 0]);
    let (c, _) = run_with(&code, |_, m| {
        m.write_u32(DATA, t0).unwrap();
        m.write_u32(DATA + 4, t1).unwrap();
    });
    assert_eq!(c.regs[3], 20);
    // call eax; jmp end; f: mov ecx, 5; ret
    let (c, _) = run_with(&[0xFF, 0xD0, 0xEB, 0x06, 0xB9, 0x05, 0, 0, 0, 0xC3], |cpu, _| cpu.regs[0] = CODE + 4);
    assert_eq!(c.regs[1], 5);
}

#[test]
fn string_instructions() {
    // rep movsd copies 3 dwords; mov ecx, 5; mov al, 0xEE; rep stosb
    let (c, m) = run_with(&[0xF3, 0xA5, 0xB9, 5, 0, 0, 0, 0xB0, 0xEE, 0xF3, 0xAA], |cpu, m| {
        m.write_bytes(DATA, &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]).unwrap();
        set_regs(cpu, &[(ESI, DATA), (EDI, DATA + 0x100), (ECX, 3)]);
    });
    let mut out = [0; 17];
    m.read_bytes(DATA + 0x100, &mut out).unwrap();
    assert_eq!(&out[..12], &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
    assert_eq!(&out[12..], &[0xEE; 5]);
    assert_eq!((c.regs[1], c.regs[7]), (0, DATA + 0x111));
    // strlen idiom: or ecx, -1; xor eax, eax; repne scasb; not ecx; dec ecx
    let (c, _) = run_with(&[0x83, 0xC9, 0xFF, 0x31, 0xC0, 0xF2, 0xAE, 0xF7, 0xD1, 0x49], |cpu, m| {
        m.write_bytes(DATA, b"hello\0").unwrap();
        set_regs(cpu, &[(EDI, DATA)]);
    });
    assert_eq!(c.regs[1], 5);
    // repe cmpsb stops after the first difference
    let (c, _) = run_with(&[0xF3, 0xA6], |cpu, m| {
        m.write_bytes(DATA, b"abcx").unwrap();
        m.write_bytes(DATA + 8, b"abcy").unwrap();
        set_regs(cpu, &[(ESI, DATA), (EDI, DATA + 8), (ECX, 4)]);
    });
    assert_eq!((c.regs[1], c.flag(ZF), c.flag(CF)), (0, false, true));
    // rep with ecx = 0 does nothing
    let c = regs(&[0xF3, 0xAB], &[(ECX, 0), (EDI, 0x7000_0000)]);
    assert_eq!(c.regs[7], 0x7000_0000);
    // std; lodsd; lodsd walks backwards
    let (c, _) = run_with(&[0xFD, 0xAD, 0xAD], |cpu, m| {
        m.write_u32(DATA, 1).unwrap();
        m.write_u32(DATA + 4, 2).unwrap();
        set_regs(cpu, &[(ESI, DATA + 4)]);
    });
    assert_eq!((c.regs[0], c.regs[6]), (1, DATA - 4));
}

#[test]
fn setcc_cmov_bit_ops_and_flag_transfer() {
    // cmp eax, ebx; setl cl; setb dl; cmovg esi, edi
    let c = regs(
        &[0x39, 0xD8, 0x0F, 0x9C, 0xC1, 0x0F, 0x92, 0xC2, 0x0F, 0x4F, 0xF7],
        &[(EAX, (-1i32) as u32), (EBX, 1), (ESI, 3), (EDI, 4)],
    );
    assert_eq!((c.regs[1] & 0xFF, c.regs[2] & 0xFF, c.regs[6]), (1, 0, 3));
    // bt eax, ebx; bts ecx, 31; bsf edx, esi; bswap edi
    let c = regs(
        &[0x0F, 0xA3, 0xD8, 0x0F, 0xBA, 0xE9, 31, 0x0F, 0xBC, 0xD6, 0x0F, 0xCF],
        &[(EAX, 8), (EBX, 3), (ECX, 3), (ESI, 0x50), (EDI, 0x1122_3344)],
    );
    assert_eq!((c.regs[1], c.regs[2], c.regs[7]), (0x8000_0003, 4, 0x4433_2211));
    assert!(!c.flag(CF), "bts saw bit 31 clear");
    // bt [ebx], eax with eax = 35 tests bit 3 of the next dword
    let (c, _) = run_with(&[0x0F, 0xA3, 0x03], |cpu, m| {
        m.write_u32(DATA + 4, 1 << 3).unwrap();
        set_regs(cpu, &[(EBX, DATA), (EAX, 35)]);
    });
    assert!(c.flag(CF));
    // stc; lahf; clc; sahf restores CF; pushfd; clc; popfd restores it too
    let c = run(&[0xF9, 0x9F, 0xF8, 0x9E, 0x9C, 0xF8, 0x9D]);
    assert!(c.flag(CF));
    // xadd ebx, eax and cmpxchg ecx, ebx
    let c = regs(&[0x0F, 0xC1, 0xC3], &[(EAX, 5), (EBX, 7)]);
    assert_eq!((c.regs[0], c.regs[3]), (7, 12));
    let c = regs(&[0x0F, 0xB1, 0xD9], &[(EAX, 5), (ECX, 5), (EBX, 9)]);
    assert_eq!((c.regs[1], c.flag(ZF)), (9, true));
    let c = regs(&[0x0F, 0xB1, 0xD9], &[(EAX, 5), (ECX, 6), (EBX, 9)]);
    assert_eq!((c.regs[0], c.regs[1], c.flag(ZF)), (6, 6, false));
}

#[test]
fn memory_operands_and_fs_segment() {
    // mov [ebx+ecx*4+8], eax; add dword [ebx+ecx*4+8], 1; mov edx, fs:[4]
    let code = [0x89, 0x44, 0x8B, 0x08, 0x83, 0x44, 0x8B, 0x08, 0x01, 0x64, 0x8B, 0x15, 4, 0, 0, 0];
    let (c, m) = run_with(&code, |cpu, m| {
        set_regs(cpu, &[(EAX, 41), (EBX, DATA), (ECX, 2)]);
        cpu.fs_base = DATA + 0x200;
        m.write_u32(DATA + 0x204, 0xABCD).unwrap();
    });
    assert_eq!(m.read_u32(DATA + 16).unwrap(), 42);
    assert_eq!(c.regs[2], 0xABCD);
    // lea does not touch memory
    let c = regs(&[0x8D, 0x84, 0x88, 0x00, 0x00, 0x00, 0x80], &[(EAX, 1), (ECX, 2)]);
    assert_eq!(c.regs[0], 0x8000_0009);
    // unmapped memory faults; the faulting instruction is reported
    assert_eq!(fails(&at(&[0x8B, 0x05], 0x5000_0000), &[]), Exception::Fault(Fault { addr: 0x5000_0000 }));
    assert_eq!(fails(&[0xCC], &[]), Exception::Breakpoint);
}

#[test]
fn fpu_arithmetic_and_conversions() {
    // fld qword [DATA]; fmul qword [DATA+8]; fsub qword [DATA+16]; fstp qword [DATA+24]
    let mut code = at(&[0xDD, 0x05], DATA);
    code.extend(at(&[0xDC, 0x0D], DATA + 8));
    code.extend(at(&[0xDC, 0x25], DATA + 16));
    code.extend(at(&[0xDD, 0x1D], DATA + 24));
    let (c, m) = run_with(&code, |_, m| {
        put_f64(m, DATA, 1.5);
        put_f64(m, DATA + 8, 4.0);
        put_f64(m, DATA + 16, 0.5);
    });
    assert_eq!(f64::from_bits(m.read_u64(DATA + 24).unwrap()), 5.5);
    assert_eq!(c.fpu.top, 0);

    // fldcw [DATA+12]; fild dword [DATA]; fidiv dword [DATA+4]; fistp dword [DATA+8]
    // computes 5 / 2 under each rounding mode (nearest-even, down, up, truncate).
    for (rc, expect) in [(0u16, 2), (1, 2), (2, 3), (3, 2)] {
        let mut code = at(&[0xD9, 0x2D], DATA + 12);
        code.extend(at(&[0xDB, 0x05], DATA));
        code.extend(at(&[0xDA, 0x35], DATA + 4));
        code.extend(at(&[0xDB, 0x1D], DATA + 8));
        let (_, m) = run_with(&code, |_, m| {
            m.write_u32(DATA, 5).unwrap();
            m.write_u32(DATA + 4, 2).unwrap();
            m.write_u16(DATA + 12, 0x027F | (rc << 10)).unwrap();
        });
        assert_eq!(m.read_u32(DATA + 8).unwrap(), expect, "rounding control {rc}");
    }

    // fld dword [DATA]; fistp qword [DATA+8]; out-of-range fistp dword stores 0x80000000
    let mut code = at(&[0xD9, 0x05], DATA);
    code.extend(at(&[0xDF, 0x3D], DATA + 8));
    code.extend(at(&[0xD9, 0x05], DATA + 4));
    code.extend(at(&[0xDB, 0x1D], DATA + 16));
    let (_, m) = run_with(&code, |_, m| {
        m.write_u32(DATA, (-1.0e10f32).to_bits()).unwrap();
        m.write_u32(DATA + 4, 1.0e10f32.to_bits()).unwrap();
    });
    assert_eq!(m.read_u64(DATA + 8).unwrap() as i64, -10_000_000_000);
    assert_eq!(m.read_u32(DATA + 16).unwrap(), 0x8000_0000);
}

#[test]
fn fpu_compare_and_branch_idiom() {
    // fld qword [DATA]; fcomp qword [DATA+8]; fnstsw ax; sahf; seta bl; test ah, 0x41; setz cl
    let mut code = at(&[0xDD, 0x05], DATA);
    code.extend(at(&[0xDC, 0x1D], DATA + 8));
    code.extend_from_slice(&[0xDF, 0xE0, 0x9E, 0x0F, 0x97, 0xC3, 0xF6, 0xC4, 0x41, 0x0F, 0x94, 0xC1]);
    for (a, b, above) in [(2.0, 1.0, true), (1.0, 2.0, false), (1.0, 1.0, false)] {
        let (c, _) = run_with(&code, |_, m| {
            put_f64(m, DATA, a);
            put_f64(m, DATA + 8, b);
        });
        assert_eq!(c.regs[3] & 0xFF == 1, above, "{a} > {b} via sahf");
        assert_eq!(c.regs[1] & 0xFF == 1, above, "{a} > {b} via test ah, 41h");
        assert_eq!(c.fpu.top, 0, "fcomp pops");
    }
}

#[test]
fn fpu_stack_operations_and_functions() {
    // fld1; fldpi; fxch st(1); fdivrp st(1), st: st1 = st0 / st1 = 1 / pi
    let c = run(&[0xD9, 0xE8, 0xD9, 0xEB, 0xD9, 0xC9, 0xDE, 0xF1]);
    assert_eq!(c.fpu.st(0), 1.0 / std::f64::consts::PI);
    // fldz; fld1; faddp st(1), st; fsqrt; fchs
    let c = run(&[0xD9, 0xEE, 0xD9, 0xE8, 0xDE, 0xC1, 0xD9, 0xFA, 0xD9, 0xE0]);
    assert_eq!((c.fpu.st(0), c.fpu.top), (-1.0, 7));
    // fld tword [DATA]
    let (c, _) = run_with(&at(&[0xDB, 0x2D], DATA), |_, m| m.write_bytes(DATA, &fpu::f64_to_f80(-2.75)).unwrap());
    assert_eq!(c.fpu.st(0), -2.75);
    // fldpi; fsin
    let c = run(&[0xD9, 0xEB, 0xD9, 0xFE]);
    assert!(c.fpu.st(0).abs() < 1e-15);
    // fld1; fld1; fpatan = atan2(1, 1)
    let c = run(&[0xD9, 0xE8, 0xD9, 0xE8, 0xD9, 0xF3]);
    assert_eq!(c.fpu.st(0), std::f64::consts::FRAC_PI_4);
    // fld1; fldz; fdivp st(1), st: st1 = st1 / st0 = 1 / 0; fnstsw [DATA] shows TOP = 7
    let (c, m) = run_with(&[0xD9, 0xE8, 0xD9, 0xEE, 0xDE, 0xF9, 0xDD, 0x3D, 0, 0, 2, 0], |_, _| {});
    assert_eq!(c.fpu.st(0), f64::INFINITY);
    assert_eq!((m.read_u16(DATA).unwrap() >> 11) & 7, 7);
}
