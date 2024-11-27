use core::arch::{asm, global_asm};

use crate::interrupt;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct SP {
    low: u32,
    high: u32,
}

#[repr(C)]
pub struct TSS {
    _reserved1: u32,
    rsp: [SP; 3],
    _reserved2: u32,
    _reserved3: u32,
    ist: [SP; 7],
    _reserved4: u32,
    _reserved5: u32,
    _reserved6: u16,
    iomap_base: u16,
}

impl TSS {
    pub fn new() -> Self {
        Self {
            _reserved1: 0,
            rsp: [SP { low: 0, high: 0 }; 3],
            _reserved2: 0,
            _reserved3: 0,
            ist: [SP { low: 0, high: 0 }; 7],
            _reserved4: 0,
            _reserved5: 0,
            _reserved6: 0,
            iomap_base: 0,
        }
    }

    pub fn set_rsp0(&mut self, rsp: u64) {
        self.rsp[0].low = rsp as u32;
        self.rsp[0].high = (rsp >> 32) as u32;
    }
}

#[inline(always)]
pub fn halt() {
    unsafe {
        asm!("hlt", options(att_syntax, nostack));
    }
}

#[inline(always)]
pub fn pause() {
    unsafe {
        asm!("pause", options(att_syntax, nostack));
    }
}

#[inline(always)]
pub fn freeze() -> ! {
    loop {
        interrupt::disable();
        halt();
    }
}

#[inline(always)]
pub fn rdmsr(msr: u32) -> u64 {
    let edx: u32;
    let eax: u32;

    unsafe {
        asm!(
            "rdmsr",
            in("ecx") msr,
            out("eax") eax,
            out("edx") edx,
            options(att_syntax),
        );
    }

    (edx as u64) << 32 | eax as u64
}

#[inline(always)]
pub fn wrmsr(msr: u32, value: u64) {
    let eax = value as u32;
    let edx = (value >> 32) as u32;

    unsafe {
        asm!(
            "wrmsr",
            in("ecx") msr,
            in("eax") eax,
            in("edx") edx,
            options(att_syntax),
        );
    }
}

global_asm!(
    r"
    .macro movcfi reg, offset
        mov \reg, \offset(%rsp)
        .cfi_rel_offset \reg, \offset
    .endm

    .macro movrst reg, offset
        mov \offset(%rsp), \reg
        .cfi_restore \reg
    .endm

    .globl __context_switch_light
    .type __context_switch_light @function
    __context_switch_light:
    .cfi_startproc

        pushf
    .cfi_def_cfa_offset 0x10

        sub $0x38, %rsp  # extra 8 bytes to align to 16 bytes
    .cfi_def_cfa_offset 0x48

        movcfi %rbx, 0x08
        movcfi %rbp, 0x10
        movcfi %r12, 0x18
        movcfi %r13, 0x20
        movcfi %r14, 0x28
        movcfi %r15, 0x30

        push (%rdi)      # save sp of previous stack frame of current
                         # acts as saving bp
    .cfi_def_cfa_offset 0x50

        mov %rsp, (%rdi) # save sp of current stack
        mov (%rsi), %rsp # load sp of target stack

        pop (%rsi)       # load sp of previous stack frame of target
                         # acts as restoring previous bp
    .cfi_def_cfa_offset 0x48

        pop %rax         # align to 16 bytes
    .cfi_def_cfa_offset 0x40

        mov 0x28(%rsp), %r15
        mov 0x20(%rsp), %r14
        mov 0x18(%rsp), %r13
        mov 0x10(%rsp), %r12
        mov 0x08(%rsp), %rbp
        mov 0x00(%rsp), %rbx

        add $0x30, %rsp
    .cfi_def_cfa_offset 0x10

        popf
    .cfi_def_cfa_offset 0x08

        ret
    .cfi_endproc
    ",
    options(att_syntax),
);

extern "C" {
    fn __context_switch_light(current_task_sp: *mut usize, next_task_sp: *mut usize);
}

#[inline(always)]
pub fn context_switch_light(current_task_sp: *mut usize, next_task_sp: *mut usize) {
    unsafe { __context_switch_light(current_task_sp, next_task_sp) }
}
