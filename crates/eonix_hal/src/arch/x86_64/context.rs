use core::arch::naked_asm;
use eonix_hal_traits::context::RawTaskContext;

/// Necessary hardware states of task for doing context switches.
#[repr(C)]
#[derive(Debug, Default)]
pub struct TaskContext {
    r12: u64,
    r13: u64,
    r14: u64,
    r15: u64,
    rbx: u64,
    rbp: u64,
    rsp: u64,
    rip: u64,    // Should we save rip here?
    rflags: u64, // Should we save rflags here?
}

impl TaskContext {
    /// Create a new task context with the given entry point and stack pointer.
    /// The entry point is the function to be called when the task is scheduled.
    /// The stack pointer is the address of the top of the stack.
    /// The stack pointer should be aligned to 16 bytes.
    pub const fn new() -> Self {
        Self {
            r12: 0,
            r13: 0,
            r14: 0,
            r15: 0,
            rbx: 0,
            rbp: 0,
            rsp: 0,
            rip: 0,
            rflags: 0x200, // IF = 1 by default.
        }
    }

    #[unsafe(naked)]
    unsafe extern "C" fn do_call() -> ! {
        naked_asm!(
            "mov %r12, %rdi",
            "push %rbp", // NULL return address.
            "jmp *%rbx",
            options(att_syntax),
        );
    }
}

impl RawTaskContext for TaskContext {
    fn new() -> Self {
        Self::new()
    }

    fn set_program_counter(&mut self, pc: usize) {
        self.rip = pc as u64;
    }

    fn set_stack_pointer(&mut self, sp: usize) {
        self.rsp = sp as u64;
    }

    fn is_interrupt_enabled(&self) -> bool {
        (self.rflags & 0x200) != 0 // IF = 1
    }

    fn set_interrupt_enabled(&mut self, is_enabled: bool) {
        if is_enabled {
            self.rflags |= 0x200; // IF = 1
        } else {
            self.rflags &= !0x200; // IF = 0
        }
    }

    fn call(&mut self, func: unsafe extern "C" fn(usize) -> !, arg: usize) {
        self.set_program_counter(Self::do_call as _);
        self.rbx = func as _;
        self.r12 = arg as _;
        self.rbp = 0; // NULL previous stack frame
    }

    #[unsafe(naked)]
    unsafe extern "C" fn switch(from: &mut Self, to: &mut Self) {
        naked_asm!(
            "pop %rax",
            "pushf",
            "pop %rcx",
            "mov %r12, (%rdi)",
            "mov %r13, 8(%rdi)",
            "mov %r14, 16(%rdi)",
            "mov %r15, 24(%rdi)",
            "mov %rbx, 32(%rdi)",
            "mov %rbp, 40(%rdi)",
            "mov %rsp, 48(%rdi)",
            "mov %rax, 56(%rdi)",
            "mov %rcx, 64(%rdi)",
            "",
            "mov (%rsi), %r12",
            "mov 8(%rsi), %r13",
            "mov 16(%rsi), %r14",
            "mov 24(%rsi), %r15",
            "mov 32(%rsi), %rbx",
            "mov 40(%rsi), %rbp",
            "mov 48(%rsi), %rdi", // store next stack pointer
            "mov 56(%rsi), %rax",
            "mov 64(%rsi), %rcx",
            "push %rcx",
            "popf",
            "xchg %rdi, %rsp", // switch to new stack
            "jmp *%rax",
            options(att_syntax),
        );
    }
}
