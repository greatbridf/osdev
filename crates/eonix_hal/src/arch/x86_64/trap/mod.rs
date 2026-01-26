mod trap_context;

use core::arch::{asm, global_asm, naked_asm};

use eonix_hal_traits::context::RawTaskContext;
use eonix_hal_traits::trap::{
    IrqState as IrqStateTrait, RawTrapContext, TrapReturn,
};
pub use trap_context::TrapContext;

use super::context::TaskContext;
use super::cpu::CPU;

unsafe extern "C" {
    /// Default handler handles the trap on the current stack and returns
    /// to the context before interrut.
    fn _default_trap_handler(trap_context: &mut TrapContext);
}

/// State of the interrupt flag.
pub struct IrqState(u64);

macro_rules! cfi_all_same_value {
    () => {
        "
        .cfi_same_value %rax
        .cfi_same_value %rbx
        .cfi_same_value %rcx
        .cfi_same_value %rdx
        .cfi_same_value %rdi
        .cfi_same_value %rsi
        .cfi_same_value %r8
        .cfi_same_value %r9
        .cfi_same_value %r10
        .cfi_same_value %r11
        .cfi_same_value %r12
        .cfi_same_value %r13
        .cfi_same_value %r14
        .cfi_same_value %r15
        .cfi_same_value %rbp
        "
    };
}

#[unsafe(naked)]
pub unsafe extern "C" fn trap_stubs() {
    naked_asm!(
        "
        .altmacro
        .macro build_isr_no_err name
            .align 8
            .globl ISR\\name
            .type  ISR\\name @function
            ISR\\name:
                .cfi_startproc
                .cfi_signal_frame
                .cfi_def_cfa_offset 0x08
                .cfi_offset %rsp, 0x10
            ",
                cfi_all_same_value!(),
            "

                push %rbp # push placeholder for error code
                .cfi_def_cfa_offset 0x10

                call {entry}
                .cfi_endproc
        .endm

        .altmacro
        .macro build_isr_err name
            .align 8
            .globl ISR\\name
            .type  ISR\\name @function
            ISR\\name:
                .cfi_startproc
                .cfi_signal_frame
                .cfi_def_cfa_offset 0x10
                .cfi_offset %rsp, 0x10
            ",
                cfi_all_same_value!(),
            "

                call {entry}
                .cfi_endproc
        .endm

        build_isr_no_err 0
        build_isr_no_err 1
        build_isr_no_err 2
        build_isr_no_err 3
        build_isr_no_err 4
        build_isr_no_err 5
        build_isr_no_err 6
        build_isr_no_err 7
        build_isr_err    8
        build_isr_no_err 9
        build_isr_err    10
        build_isr_err    11
        build_isr_err    12
        build_isr_err    13
        build_isr_err    14
        build_isr_no_err 15
        build_isr_no_err 16
        build_isr_err    17
        build_isr_no_err 18
        build_isr_no_err 19
        build_isr_no_err 20
        build_isr_err    21
        build_isr_no_err 22
        build_isr_no_err 23
        build_isr_no_err 24
        build_isr_no_err 25
        build_isr_no_err 26
        build_isr_no_err 27
        build_isr_no_err 28
        build_isr_err    29
        build_isr_err    30
        build_isr_no_err 31

        .set i, 32
        .rept 0x80+1
            build_isr_no_err %i
            .set i, i+1
        .endr
        ",
        entry = sym raw_trap_entry,
        options(att_syntax),
    )
}

/// Offset of the capturer trap context in the percpu area.
const OFFSET_CAPTURER: usize = 8;

#[unsafe(naked)]
unsafe extern "C" fn raw_trap_entry() {
    naked_asm!(
        ".cfi_startproc",
        ".cfi_signal_frame",
        ".cfi_def_cfa %rsp, 0x18",
        ".cfi_offset %rsp, 0x10",
        cfi_all_same_value!(),
        "",
        "cmpq $0x08, 0x18(%rsp)",
        "je 2f",
        "swapgs",
        "",
        "2:",
        "subq ${trap_stubs}, (%rsp)",
        "shrq $3, (%rsp)",
        "",
        "cmpq $0, %gs:{offset_capturer}",
        "je {default_entry}",
        "",
        "cmpq $0x08, 0x18(%rsp)",
        "je {captured_kernel_entry}",
        "jmp {captured_user_entry}",
        ".cfi_endproc",
        trap_stubs = sym trap_stubs,
        default_entry = sym default_trap_entry,
        captured_kernel_entry = sym captured_trap_entry_kernel,
        captured_user_entry = sym captured_trap_entry_user,
        offset_capturer = const OFFSET_CAPTURER,
        options(att_syntax),
    )
}

#[unsafe(naked)]
unsafe extern "C" fn default_trap_entry() {
    naked_asm!(
        ".cfi_startproc",
        ".cfi_signal_frame",
        ".cfi_def_cfa %rsp, 0x18",
        ".cfi_offset %rsp, 0x10",
        cfi_all_same_value!(),
        "",
        "sub ${INT_NO}, %rsp",
        ".cfi_def_cfa_offset {CS}",
        "",
        "mov %rcx, {RCX}(%rsp)",
        ".cfi_rel_offset %rcx, {RCX}",
        "mov %rdx, {RDX}(%rsp)",
        ".cfi_rel_offset %rdx, {RDX}",
        "mov %rdi, {RDI}(%rsp)",
        ".cfi_rel_offset %rdi, {RDI}",
        "mov %rsi, {RSI}(%rsp)",
        ".cfi_rel_offset %rsi, {RSI}",
        "mov %r8, {R8}(%rsp)",
        ".cfi_rel_offset %r8, {R8}",
        "mov %r9, {R9}(%rsp)",
        ".cfi_rel_offset %r9, {R9}",
        "mov %r10, {R10}(%rsp)",
        ".cfi_rel_offset %r10, {R10}",
        "mov %r11, {R11}(%rsp)",
        ".cfi_rel_offset %r11, {R11}",
        "mov %rbx, {RBX}(%rsp)",
        ".cfi_rel_offset %rbx, {RBX}",
        "mov %r12, {R12}(%rsp)",
        ".cfi_rel_offset %r12, {R12}",
        "",
        "mov %rax, %r12",
        ".cfi_register %rax, %r12",
        "mov %rsp, %rbx",
        ".cfi_def_cfa_register %rbx",
        "",
        "and $-0x10, %rsp",
        "mov %rbx, %rdi",
        "",
        "call {default_entry}",
        "",
        "mov %rbx, %rsp",
        ".cfi_def_cfa_register %rsp",
        "mov %r12, %rax",
        ".cfi_restore %rax",
        "",
        "mov {RCX}(%rsp), %rcx",
        ".cfi_restore %rcx",
        "mov {RDX}(%rsp), %rdx",
        ".cfi_restore %rdx",
        "mov {RDI}(%rsp), %rdi",
        ".cfi_restore %rdi",
        "mov {RSI}(%rsp), %rsi",
        ".cfi_restore %rsi",
        "mov {R8}(%rsp), %r8",
        ".cfi_restore %r8",
        "mov {R9}(%rsp), %r9",
        ".cfi_restore %r9",
        "mov {R10}(%rsp), %r10",
        ".cfi_restore %r10",
        "mov {R11}(%rsp), %r11",
        ".cfi_restore %r11",
        "mov {RBX}(%rsp), %rbx",
        ".cfi_restore %rbx",
        "mov {R12}(%rsp), %r12",
        ".cfi_restore %r12",
        "",
        "cmpq $0x08, {CS}(%rsp)",
        "je 2f",
        "swapgs",
        "",
        "2:",
        "lea {RIP}(%rsp), %rsp",
        ".cfi_def_cfa %rsp, 0x08",
        ".cfi_offset %rsp, 0x10",
        "",
        "iretq",
        ".cfi_endproc",
        default_entry = sym _default_trap_handler,
        RBX = const TrapContext::OFFSET_RBX,
        RCX = const TrapContext::OFFSET_RCX,
        RDX = const TrapContext::OFFSET_RDX,
        RDI = const TrapContext::OFFSET_RDI,
        RSI = const TrapContext::OFFSET_RSI,
        R8 = const TrapContext::OFFSET_R8,
        R9 = const TrapContext::OFFSET_R9,
        R10 = const TrapContext::OFFSET_R10,
        R11 = const TrapContext::OFFSET_R11,
        R12 = const TrapContext::OFFSET_R12,
        INT_NO = const TrapContext::OFFSET_INT_NO,
        RIP = const TrapContext::OFFSET_RIP,
        CS = const TrapContext::OFFSET_CS,
        options(att_syntax),
    )
}

#[unsafe(naked)]
unsafe extern "C" fn captured_trap_entry_kernel() {
    naked_asm!(
        ".cfi_startproc",
        ".cfi_signal_frame",
        ".cfi_def_cfa %rsp, 0x18",
        ".cfi_offset %rsp, 0x10",
        cfi_all_same_value!(),
        "",
        "mov %rsi, %gs:0x10",
        ".cfi_undefined %rsi",
        "mov %rsp, %rsi",
        ".cfi_def_cfa_register %rsi",
        ".cfi_register %rsp, %rsi",

        "mov %gs:0x08, %rsp",
        // Save and load registers.
        "mov %rcx, {RCX}(%rsp)",
        ".cfi_rel_offset %rcx, {RCX}",
        "",
        "mov %gs:0x10, %rcx",
        ".cfi_register %rsi, %rcx",
        "",
        "mov %rdx, {RDX}(%rsp)",
        ".cfi_rel_offset %rdx, {RDX}",
        "mov %rdi, {RDI}(%rsp)",
        ".cfi_rel_offset %rdi, {RDI}",
        "mov %rcx, {RSI}(%rsp)",
        ".cfi_rel_offset %rsi, {RSI}",
        "mov %r8, {R8}(%rsp)",
        ".cfi_rel_offset %r8, {R8}",
        "mov %r9, {R9}(%rsp)",
        ".cfi_rel_offset %r9, {R9}",
        "mov %r10, {R10}(%rsp)",
        ".cfi_rel_offset %r10, {R10}",
        "mov %r11, {R11}(%rsp)",
        ".cfi_rel_offset %r11, {R11}",
        "",
        "xchg %rax, {RAX}(%rsp)",
        ".cfi_rel_offset %rax, {RAX}",
        "xchg %rbx, {RBX}(%rsp)",
        ".cfi_rel_offset %rbx, {RBX}",
        "xchg %r12, {R12}(%rsp)",
        ".cfi_rel_offset %r12, {R12}",
        "xchg %r13, {R13}(%rsp)",
        ".cfi_rel_offset %r13, {R13}",
        "xchg %r14, {R14}(%rsp)",
        ".cfi_rel_offset %r14, {R14}",
        "xchg %r15, {R15}(%rsp)",
        ".cfi_rel_offset %r15, {R15}",
        "xchg %rbp, {RBP}(%rsp)",
        ".cfi_rel_offset %rbp, {RBP}",
        "",
        "lea {INT_NO}(%rsp), %rdi",
        "mov $7, %rcx",
        "cld",
        "rep movsq",
        "",
        "mov %rax, %rsp",
        ".cfi_def_cfa %rsp, 0x10",
        ".cfi_undefined %rax",
        ".cfi_restore %rbx",
        ".cfi_undefined %rcx",
        ".cfi_undefined %rdx",
        ".cfi_undefined %rdi",
        ".cfi_undefined %rsi",
        ".cfi_undefined %r8",
        ".cfi_undefined %r9",
        ".cfi_undefined %r10",
        ".cfi_undefined %r11",
        ".cfi_restore %r12",
        ".cfi_restore %r13",
        ".cfi_restore %r14",
        ".cfi_restore %r15",
        ".cfi_restore %rbp",
        "",
        "popf",
        ".cfi_def_cfa_offset 0x08",
        "",
        "ret",
        ".cfi_endproc",
        RAX = const TrapContext::OFFSET_RAX,
        RBX = const TrapContext::OFFSET_RBX,
        RCX = const TrapContext::OFFSET_RCX,
        RDX = const TrapContext::OFFSET_RDX,
        RDI = const TrapContext::OFFSET_RDI,
        RSI = const TrapContext::OFFSET_RSI,
        R8 = const TrapContext::OFFSET_R8,
        R9 = const TrapContext::OFFSET_R9,
        R10 = const TrapContext::OFFSET_R10,
        R11 = const TrapContext::OFFSET_R11,
        R12 = const TrapContext::OFFSET_R12,
        R13 = const TrapContext::OFFSET_R13,
        R14 = const TrapContext::OFFSET_R14,
        R15 = const TrapContext::OFFSET_R15,
        RBP = const TrapContext::OFFSET_RBP,
        INT_NO = const TrapContext::OFFSET_INT_NO,
        options(att_syntax),
    )
}

#[unsafe(naked)]
unsafe extern "C" fn captured_trap_entry_user() {
    naked_asm!(
        ".cfi_startproc",
        ".cfi_signal_frame",
        ".cfi_def_cfa %rsp, 0x18",
        ".cfi_offset %rsp, 0x10",
        cfi_all_same_value!(),
        "",
        "sub ${INT_NO}, %rsp",
        ".cfi_def_cfa_offset {CS}",
        "",
        // Save and load registers.
        "mov %rcx, {RCX}(%rsp)",
        ".cfi_rel_offset %rcx, {RCX}",
        "mov %rdx, {RDX}(%rsp)",
        ".cfi_rel_offset %rdx, {RDX}",
        "mov %rdi, {RDI}(%rsp)",
        ".cfi_rel_offset %rdi, {RDI}",
        "mov %rsi, {RSI}(%rsp)",
        ".cfi_rel_offset %rsi, {RSI}",
        "mov %r8,  {R8}(%rsp)",
        ".cfi_rel_offset %r8, {R8}",
        "mov %r9,  {R9}(%rsp)",
        ".cfi_rel_offset %r9, {R9}",
        "mov %r10, {R10}(%rsp)",
        ".cfi_rel_offset %r10, {R10}",
        "mov %r11, {R11}(%rsp)",
        ".cfi_rel_offset %r11, {R11}",
        "",
        "xchg %rax, {RAX}(%rsp)",
        ".cfi_rel_offset %rax, {RAX}",
        "xchg %rbx, {RBX}(%rsp)",
        ".cfi_rel_offset %rbx, {RBX}",
        "xchg %r12, {R12}(%rsp)",
        ".cfi_rel_offset %r12, {R12}",
        "xchg %r13, {R13}(%rsp)",
        ".cfi_rel_offset %r13, {R13}",
        "xchg %r14, {R14}(%rsp)",
        ".cfi_rel_offset %r14, {R14}",
        "xchg %r15, {R15}(%rsp)",
        ".cfi_rel_offset %r15, {R15}",
        "xchg %rbp, {RBP}(%rsp)",
        ".cfi_rel_offset %rbp, {RBP}",
        "",
        "mov %rax, %rsp",
        ".cfi_def_cfa %rsp, 0x10",
        ".cfi_undefined %rax",
        ".cfi_restore %rbx",
        ".cfi_undefined %rcx",
        ".cfi_undefined %rdx",
        ".cfi_undefined %rdi",
        ".cfi_undefined %rsi",
        ".cfi_undefined %r8",
        ".cfi_undefined %r9",
        ".cfi_undefined %r10",
        ".cfi_undefined %r11",
        ".cfi_restore %r12",
        ".cfi_restore %r13",
        ".cfi_restore %r14",
        ".cfi_restore %r15",
        ".cfi_restore %rbp",
        "",
        "popf",
        ".cfi_def_cfa_offset 0x08",
        "",
        "ret",
        ".cfi_endproc",
        RAX = const TrapContext::OFFSET_RAX,
        RBX = const TrapContext::OFFSET_RBX,
        RCX = const TrapContext::OFFSET_RCX,
        RDX = const TrapContext::OFFSET_RDX,
        RDI = const TrapContext::OFFSET_RDI,
        RSI = const TrapContext::OFFSET_RSI,
        R8 = const TrapContext::OFFSET_R8,
        R9 = const TrapContext::OFFSET_R9,
        R10 = const TrapContext::OFFSET_R10,
        R11 = const TrapContext::OFFSET_R11,
        R12 = const TrapContext::OFFSET_R12,
        R13 = const TrapContext::OFFSET_R13,
        R14 = const TrapContext::OFFSET_R14,
        R15 = const TrapContext::OFFSET_R15,
        RBP = const TrapContext::OFFSET_RBP,
        INT_NO = const TrapContext::OFFSET_INT_NO,
        CS = const TrapContext::OFFSET_CS,
        options(att_syntax),
    )
}

#[unsafe(naked)]
unsafe extern "C" fn captured_trap_return(trap_context: &mut TrapContext) {
    naked_asm!(
        ".cfi_startproc",
        ".cfi_signal_frame",
        ".cfi_def_cfa %rsp, 0x08",
        "",
        "pushf",
        ".cfi_def_cfa_offset 0x10",
        "",
        "mov %rdi, %rax",
        ".cfi_def_cfa %rax, {CS}",
        ".cfi_rel_offset %rcx, {RCX}",
        ".cfi_rel_offset %rdx, {RDX}",
        ".cfi_rel_offset %rdi, {RDI}",
        ".cfi_rel_offset %rsi, {RSI}",
        ".cfi_rel_offset %r8, {R8}",
        ".cfi_rel_offset %r9, {R9}",
        ".cfi_rel_offset %r10, {R10}",
        ".cfi_rel_offset %r11, {R11}",
        ".cfi_rel_offset %rax, {RAX}",
        ".cfi_rel_offset %rbx, {RBX}",
        ".cfi_rel_offset %r12, {R12}",
        ".cfi_rel_offset %r13, {R13}",
        ".cfi_rel_offset %r14, {R14}",
        ".cfi_rel_offset %r15, {R15}",
        ".cfi_rel_offset %rbp, {RBP}",
        ".cfi_rel_offset %rflags, {FLAGS}",
        ".cfi_rel_offset %rsp, {RSP}",
        "",
        "mov {RCX}(%rax), %rcx",
        ".cfi_restore %rcx",
        "mov {RDX}(%rax), %rdx",
        ".cfi_restore %rdx",
        "mov {RDI}(%rax), %rdi",
        ".cfi_restore %rdi",
        "mov {RSI}(%rax), %rsi",
        ".cfi_restore %rsi",
        "mov  {R8}(%rax), %r8",
        ".cfi_restore %r8",
        "mov  {R9}(%rax), %r9",
        ".cfi_restore %r9",
        "mov {R10}(%rax), %r10",
        ".cfi_restore %r10",
        "mov {R11}(%rax), %r11",
        ".cfi_restore %r11",
        "",
        "xchg %rax, %rsp",
        "xchg %rax, {RAX}(%rsp)",
        ".cfi_restore %rax",
        "xchg %rbx, {RBX}(%rsp)",
        ".cfi_restore %rbx",
        "xchg %r12, {R12}(%rsp)",
        ".cfi_restore %r12",
        "xchg %r13, {R13}(%rsp)",
        ".cfi_restore %r13",
        "xchg %r14, {R14}(%rsp)",
        ".cfi_restore %r14",
        "xchg %r15, {R15}(%rsp)",
        ".cfi_restore %r15",
        "xchg %rbp, {RBP}(%rsp)",
        ".cfi_restore %rbp",
        "",
        "cmpq $0x08, {CS}(%rsp)",
        "je 2f",
        "swapgs",
        "",
        "2:",
        "lea {RIP}(%rsp), %rsp",
        ".cfi_def_cfa %rsp, 0x08",
        "iretq",
        ".cfi_endproc",
        RAX = const TrapContext::OFFSET_RAX,
        RBX = const TrapContext::OFFSET_RBX,
        RCX = const TrapContext::OFFSET_RCX,
        RDX = const TrapContext::OFFSET_RDX,
        RDI = const TrapContext::OFFSET_RDI,
        RSI = const TrapContext::OFFSET_RSI,
        R8 = const TrapContext::OFFSET_R8,
        R9 = const TrapContext::OFFSET_R9,
        R10 = const TrapContext::OFFSET_R10,
        R11 = const TrapContext::OFFSET_R11,
        R12 = const TrapContext::OFFSET_R12,
        R13 = const TrapContext::OFFSET_R13,
        R14 = const TrapContext::OFFSET_R14,
        R15 = const TrapContext::OFFSET_R15,
        RBP = const TrapContext::OFFSET_RBP,
        RIP = const TrapContext::OFFSET_RIP,
        CS = const TrapContext::OFFSET_CS,
        FLAGS = const TrapContext::OFFSET_FLAGS,
        RSP = const TrapContext::OFFSET_RSP,
        options(att_syntax),
    );
}

unsafe fn swap_percpu_capturer(new_capturer: usize) -> usize {
    let old_capturer: usize;
    asm!(
        "mov %gs:0x08, {old}",
        "mov {new}, %gs:0x08",
        new = in(reg) new_capturer,
        old = out(reg) old_capturer,
        options(att_syntax),
    );

    old_capturer
}

impl TrapReturn for TrapContext {
    type TaskContext = TaskContext;

    unsafe fn trap_return(&mut self) {
        let irq_states = disable_irqs_save();
        let old_handler = swap_percpu_capturer(self as *mut _ as usize);

        unsafe {
            CPU::local()
                .as_mut()
                .load_interrupt_stack(self as *mut _ as usize as u64);
        }

        captured_trap_return(self);

        swap_percpu_capturer(old_handler);
        irq_states.restore();
    }
}

impl IrqStateTrait for IrqState {
    fn restore(self) {
        let Self(state) = self;

        unsafe {
            asm!(
                "push {state}",
                "popf",
                state = in(reg) state,
                options(att_syntax, nomem)
            );
        }
    }
}

pub fn enable_irqs() {
    unsafe {
        asm!("sti", options(att_syntax, nomem, nostack));
    }
}

pub fn disable_irqs() {
    unsafe {
        asm!("cli", options(att_syntax, nomem, nostack));
    }
}

pub fn disable_irqs_save() -> IrqState {
    let state: u64;
    unsafe {
        asm!(
            "pushf",
            "pop {state}",
            "cli",
            state = out(reg) state,
            options(att_syntax, nomem)
        );
    }

    IrqState(state)
}
