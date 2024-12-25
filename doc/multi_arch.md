# 多架构支持
ROS 目前仅支持x86-64架构，但为了后续多架构的支持，在 ROS 开发过程中尽可能的将架构相关的操作进行抽象，并充分利用 rust 包管理机制，将架构相关代码统一置于 arch crate 中，便于后续多架构扩展。ROS 通过 cfg-if 实现不同架构的条件编译。
``` rust
cfg_if::cfg_if! {
    if #[cfg(target_arch = "x86_64")] {
        mod x86_64;
        pub use self::x86_64::*;
    } else if #[cfg(target_arch = "riscv64")] {
        mod riscv64;
        pub use self::riscv64::*;
    } else if #[cfg(target_arch = "aarch64")]{
        mod aarch64;
        pub use self::aarch64::*;
    }
}
```
## 进程相关抽象
### 进程上下文抽象
进程上下文是内核调度的关键结构，但由于不同的体系结构在进程上下文的保存上存在差异，例如，x86-64 只能将上下文存储在内核栈上，而 riscv64 可以定义相关结构体存储上下文内容，所以我们将进程上下文及其相关操作，如上下文初始化，上下文切换等统一进行抽象，这样内核处理上下文时只需要 TaskContext 提供的统一接口。
``` rust
// x86-64
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
```
## 内存相关抽象
页表根目录的修改和获取，tlb的刷新等都是内核必须的内存操作，提供有关抽象很有必要。

## IO相关抽象
### 中断上下文抽象
中断上下文需要保存中断前CPU所有信息，为了多架构的统一，将中断上下文抽象为InterruptContext，这样内核再进行中断相关操作时无需考虑架构上的差异。
``` rust
// x86-64
pub struct InterruptContext {
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    pub rbp: u64,

    pub int_no: u64,
    pub error_code: u64,

    // Pushed by CPU
    pub rip: u64,
    pub cs: u64,
    pub eflags: u64,
    pub rsp: u64,
    pub ss: u64,
}
```
## 杂项
