use core::arch::asm;

#[repr(C)]
#[derive(Debug, Default)]
struct ContextSwitchFrame {
    r15: u64,
    r14: u64,
    r13: u64,
    r12: u64,
    rbx: u64,
    rbp: u64,
    eflags: u64,
    rip: u64,
}

/// Necessary hardware states of task for context switch
pub struct TaskContext {
    /// The kernel stack pointer
    pub rsp: u64,
    // Extended states, i.e., FP/SIMD states to do!
}

impl TaskContext {
    pub const fn new() -> Self {
        Self { rsp: 0 }
    }

    pub fn init(&mut self, entry: usize, kstack_top: usize) {
        unsafe {
            let frame_ptr = (kstack_top as *mut ContextSwitchFrame).sub(1);
            core::ptr::write(
                frame_ptr,
                ContextSwitchFrame {
                    rip: entry as u64,
                    eflags: 0x200,
                    ..Default::default()
                },
            );
            self.rsp = frame_ptr as u64;
        }
    }

    #[inline(always)]
    pub fn switch_to(&mut self, next_task: &mut Self) {
        unsafe { _switch_to(&mut self.rsp, &mut next_task.rsp) }
    }
}

#[naked]
unsafe extern "C" fn _switch_to(current_context_sp: &mut u64, next_context_sp: &mut u64) {
    asm!(
        "pushf",
        "push %rbp",
        "push %rbx",
        "push %r12",
        "push %r13",
        "push %r14",
        "push %r15",
        "mov %rsp, (%rdi)",
        "mov (%rsi), %rsp",
        "pop %r15",
        "pop %r14",
        "pop %r13",
        "pop %r12",
        "pop %rbx",
        "pop %rbp",
        "popf",
        "ret",
        options(att_syntax, noreturn),
    );
}
