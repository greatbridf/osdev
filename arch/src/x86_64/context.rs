use core::arch::naked_asm;

/// Necessary hardware states of task for context switch
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

    pub fn ip(&mut self, ip: usize) {
        self.rip = ip as u64;
    }

    pub fn sp(&mut self, sp: usize) {
        self.rsp = sp as u64;
    }

    pub fn call1(&mut self, func: unsafe extern "C" fn(usize) -> !, arg: [usize; 1]) {
        self.ip(Self::do_call as _);
        self.rbp = func as _;
        self.r12 = arg[0] as _;
    }

    pub fn call2(&mut self, func: unsafe extern "C" fn(usize, usize) -> !, arg: [usize; 2]) {
        self.ip(Self::do_call as _);
        self.rbp = func as _;

        (self.r12, self.r13) = (arg[0] as _, arg[1] as _);
    }

    pub fn call3(&mut self, func: unsafe extern "C" fn(usize, usize, usize) -> !, arg: [usize; 3]) {
        self.ip(Self::do_call as _);
        self.rbp = func as _;

        (self.r12, self.r13, self.r14) = (arg[0] as _, arg[1] as _, arg[2] as _);
    }

    pub fn call4(
        &mut self,
        func: unsafe extern "C" fn(usize, usize, usize, usize) -> !,
        arg: [usize; 4],
    ) {
        self.ip(Self::do_call as _);
        self.rbp = func as _;

        (self.r12, self.r13, self.r14, self.r15) =
            (arg[0] as _, arg[1] as _, arg[2] as _, arg[3] as _);
    }

    pub fn call5(
        &mut self,
        func: unsafe extern "C" fn(usize, usize, usize, usize, usize) -> !,
        arg: [usize; 5],
    ) {
        self.ip(Self::do_call as _);
        self.rbp = func as _;

        (self.r12, self.r13, self.r14, self.r15, self.rbx) = (
            arg[0] as _,
            arg[1] as _,
            arg[2] as _,
            arg[3] as _,
            arg[4] as _,
        );
    }

    pub fn interrupt(&mut self, is_enabled: bool) {
        if is_enabled {
            self.rflags |= 0x200; // IF = 1
        } else {
            self.rflags &= !0x200; // IF = 0
        }
    }

    #[naked]
    pub unsafe extern "C" fn switch(from: &mut Self, to: &mut Self) {
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

    #[naked]
    /// Maximum of 5 arguments supported.
    unsafe extern "C" fn do_call() -> ! {
        naked_asm!(
            "mov %r12, %rdi",
            "mov %r13, %rsi",
            "mov %r14, %rdx",
            "mov %r15, %rcx",
            "mov %rbx, %r8",
            "mov %rbp, %rax",
            "xor %rbp, %rbp",
            "jmp *%rax",
            options(att_syntax),
        );
    }
}
