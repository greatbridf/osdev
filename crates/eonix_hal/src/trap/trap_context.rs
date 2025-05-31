use arch::{TaskContext, TrapContext};
use core::{
    arch::{global_asm, naked_asm},
    mem::transmute,
};
use eonix_hal_traits::trap::IsRawTrapContext;

#[doc(notable_trait)]
pub trait TrapContextExt {
    /// Return to the context before the trap occurred.
    ///
    /// # Safety
    /// This function is unsafe because the caller MUST ensure that the
    /// context before the trap is valid, that is, that the stack pointer
    /// points to a valid stack frame and the program counter points to some
    /// valid instruction.
    unsafe fn trap_return(&mut self);
}

struct _CheckTrapContext(IsRawTrapContext<TrapContext>);

unsafe extern "C" {
    fn _default_trap_handler(trap_context: &mut TrapContext);
    pub static TRAP_STUBS_START: usize;
    fn _raw_trap_return();
}

#[eonix_percpu::define_percpu]
static TRAP_HANDLER: usize = 0;

#[eonix_percpu::define_percpu]
static CAPTURER_CONTEXT: TaskContext = TaskContext::new();

/// This value will never be used.
static mut DIRTY_TRAP_CONTEXT: TaskContext = TaskContext::new();

global_asm!(
    r"
    .set RAX, 0x00
    .set RBX, 0x08
    .set RCX, 0x10
    .set RDX, 0x18
    .set RDI, 0x20
    .set RSI, 0x28
    .set R8, 0x30
    .set R9, 0x38
    .set R10, 0x40
    .set R11, 0x48
    .set R12, 0x50
    .set R13, 0x58
    .set R14, 0x60
    .set R15, 0x68
    .set RBP, 0x70
    .set INT_NO, 0x78
    .set ERRCODE, 0x80
    .set RIP, 0x88
    .set CS, 0x90
    .set FLAGS, 0x98
    .set RSP, 0xa0
    .set SS, 0xa8

    .macro cfi_all_same_value
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
    .endm

    .globl TRAP_STUBS_START
    TRAP_STUBS_START:
        .quad _trap_stubs_start

    _trap_stubs_start:
        .altmacro
        .macro build_isr_no_err name
            .align 8
            .globl ISR\name
            .type  ISR\name @function
            ISR\name:
                .cfi_startproc
                .cfi_signal_frame
                .cfi_def_cfa_offset 0x08
                .cfi_offset %rsp, 0x10

                cfi_all_same_value

                push %rbp # push placeholder for error code
                .cfi_def_cfa_offset 0x10

                call _raw_trap_entry
                .cfi_endproc
        .endm

        .altmacro
        .macro build_isr_err name
            .align 8
            .globl ISR\name
            .type  ISR\name @function
            ISR\name:
                .cfi_startproc
                .cfi_signal_frame
                .cfi_def_cfa_offset 0x10
                .cfi_offset %rsp, 0x10

                cfi_all_same_value

                call _raw_trap_entry
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
    
    .globl _raw_trap_entry
    .type  _raw_trap_entry @function
    _raw_trap_entry:
        .cfi_startproc
        .cfi_signal_frame
        .cfi_def_cfa %rsp, 0x18
        .cfi_offset %rsp, 0x10

        cfi_all_same_value
        
        sub $0x78, %rsp
        .cfi_def_cfa_offset CS
        
        mov %rax, RAX(%rsp)
        .cfi_rel_offset %rax, RAX
        mov %rbx, RBX(%rsp)
        .cfi_rel_offset %rbx, RBX
        mov %rcx, RCX(%rsp)
        .cfi_rel_offset %rcx, RCX
        mov %rdx, RDX(%rsp)
        .cfi_rel_offset %rdx, RDX
        mov %rdi, RDI(%rsp)
        .cfi_rel_offset %rdi, RDI
        mov %rsi, RSI(%rsp)
        .cfi_rel_offset %rsi, RSI
        mov %r8, R8(%rsp)
        .cfi_rel_offset %r8, R8
        mov %r9, R9(%rsp)
        .cfi_rel_offset %r9, R9
        mov %r10, R10(%rsp)
        .cfi_rel_offset %r10, R10
        mov %r11, R11(%rsp)
        .cfi_rel_offset %r11, R11
        mov %r12, R12(%rsp)
        .cfi_rel_offset %r12, R12
        mov %r13, R13(%rsp)
        .cfi_rel_offset %r13, R13
        mov %r14, R14(%rsp)
        .cfi_rel_offset %r14, R14
        mov %r15, R15(%rsp)
        .cfi_rel_offset %r15, R15
        mov %rbp, RBP(%rsp)
        .cfi_rel_offset %rbp, RBP
        
        mov INT_NO(%rsp), %rcx
        sub $_trap_stubs_start, %rcx
        shr $3, %rcx
        mov %rcx, INT_NO(%rsp)
        
        cmpq $0x08, CS(%rsp)
        je 2f
        swapgs
        
        2:
        mov %gs:0, %rcx
        add ${handler}, %rcx
        mov (%rcx), %rcx
        
        jmp *%rcx
        .cfi_endproc
    
    _raw_trap_return:
        .cfi_startproc
        .cfi_def_cfa %rsp, CS
        .cfi_rel_offset %rax, RAX
        .cfi_rel_offset %rbx, RBX
        .cfi_rel_offset %rcx, RCX
        .cfi_rel_offset %rdx, RDX
        .cfi_rel_offset %rdi, RDI
        .cfi_rel_offset %rsi, RSI
        .cfi_rel_offset %r8, R8
        .cfi_rel_offset %r9, R9
        .cfi_rel_offset %r10, R10
        .cfi_rel_offset %r11, R11
        .cfi_rel_offset %r12, R12
        .cfi_rel_offset %r13, R13
        .cfi_rel_offset %r14, R14
        .cfi_rel_offset %r15, R15
        .cfi_rel_offset %rbp, RBP
        .cfi_rel_offset %rsp, RSP
        
        mov RAX(%rsp), %rax
        .cfi_restore %rax
        mov RBX(%rsp), %rbx
        .cfi_restore %rbx
        mov RCX(%rsp), %rcx
        .cfi_restore %rcx
        mov RDX(%rsp), %rdx
        .cfi_restore %rdx
        mov RDI(%rsp), %rdi
        .cfi_restore %rdi
        mov RSI(%rsp), %rsi
        .cfi_restore %rsi
        mov R8(%rsp), %r8
        .cfi_restore %r8
        mov R9(%rsp), %r9
        .cfi_restore %r9
        mov R10(%rsp), %r10
        .cfi_restore %r10
        mov R11(%rsp), %r11
        .cfi_restore %r11
        mov R12(%rsp), %r12
        .cfi_restore %r12
        mov R13(%rsp), %r13
        .cfi_restore %r13
        mov R14(%rsp), %r14
        .cfi_restore %r14
        mov R15(%rsp), %r15
        .cfi_restore %r15
        mov RBP(%rsp), %rbp
        .cfi_restore %rbp
        
        cmpq $0x08, CS(%rsp)
        je 2f
        swapgs
        
        2:
        lea RIP(%rsp), %rsp
        .cfi_def_cfa %rsp, 0x08
        .cfi_offset %rsp, 0x10
        
        iretq
        .cfi_endproc
    ",
    handler = sym _percpu_inner_TRAP_HANDLER,
    options(att_syntax),
);

/// Default handler handles the trap on the current stack and returns
/// to the context before interrut.
#[naked]
unsafe extern "C" fn default_trap_handler() {
    unsafe {
        naked_asm!(
            ".cfi_startproc",
            "mov %rsp, %rbx",
            ".cfi_def_cfa_register %rbx",
            "",
            "and $~0xf, %rsp",
            "",
            "mov %rbx, %rdi",
            "call {handle_trap}",
            "",
            "mov %rbx, %rsp",
            ".cfi_def_cfa_register %rsp",
            "",
            "jmp {trap_return}",
            ".cfi_endproc",
            handle_trap = sym _default_trap_handler,
            trap_return = sym _raw_trap_return,
            options(att_syntax),
        );
    }
}

#[naked]
unsafe extern "C" fn captured_trap_handler() {
    unsafe {
        naked_asm!(
            "mov ${from_context}, %rdi",
            "mov %gs:0, %rsi",
            "add ${to_context}, %rsi",
            "",
            "mov %rdi, %rsp", // We need a temporary stack to use `switch()`.
            "",
            "jmp {switch}",
            from_context = sym DIRTY_TRAP_CONTEXT,
            to_context = sym _percpu_inner_CAPTURER_CONTEXT,
            switch = sym arch::TaskContext::switch,
            options(att_syntax),
        );
    }
}

#[naked]
unsafe extern "C" fn captured_trap_return(trap_context: usize) -> ! {
    unsafe {
        naked_asm!(
            "jmp {trap_return}",
            trap_return = sym _raw_trap_return,
            options(att_syntax),
        );
    }
}

impl TrapContextExt for TrapContext {
    unsafe fn trap_return(&mut self) {
        let irq_states = arch::disable_irqs_save();
        let old_handler = TRAP_HANDLER.swap(captured_trap_handler as *const () as usize);

        let mut to_ctx = arch::TaskContext::new();
        to_ctx.ip(captured_trap_return as _);
        to_ctx.sp(&raw mut *self as usize);
        to_ctx.interrupt(false);

        unsafe {
            arch::TaskContext::switch(CAPTURER_CONTEXT.as_mut(), &mut to_ctx);
        }

        TRAP_HANDLER.set(old_handler);
        irq_states.restore();
    }
}

pub fn init() {
    let addr = unsafe { transmute::<_, usize>(default_trap_handler as *const ()) };
    TRAP_HANDLER.set(addr as usize);
}
