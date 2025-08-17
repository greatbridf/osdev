# 多架构支持
Eonix 目前支持 x86_64、riscv64 和 loongarch64 三种架构。为达成此目标，Eonix 在开发过程中采用了分层抽象的设计范式，对所有与架构相关的操作进行了彻底的抽象化处理，并通过 Rust 编程语言的包管理机制实现了高度的模块化管理。

## 硬件抽象层 (HAL)

所有架构相关的代码统一置于 `crates/eonix_hal` 硬件抽象层中，该层负责：
* 封装不同架构的底层硬件操作
* 提供统一的接口给内核调用
* 处理架构特定的内存管理、中断处理、上下文切换等功能

Eonix 通过 cfg-if 宏实现不同架构的条件编译，确保在编译时只包含目标架构的相关代码：
``` rust
cfg_if::cfg_if! {
    if #[cfg(target_arch = "x86_64")] {
        mod x86_64;
        pub use x86_64::*;
    } else if #[cfg(target_arch = "riscv64")] {
        pub mod riscv64;
        pub use riscv64::*;
    } else if #[cfg(target_arch = "loongarch64")] {
        pub mod loongarch64;
        pub use loongarch64::*;
    } else {
        compile_error!("Unsupported architecture");
    }
}
```

## 进程相关抽象

为了实现多架构支持，Eonix 在硬件抽象层 (HAL) 中对关键的系统组件进行了统一抽象。这些抽象隐藏了不同架构之间的差异，为内核提供了统一的编程接口，使得内核代码能够在多个架构上无缝运行，从而提升了系统的可维护性与可移植性。

### 进程上下文抽象 (RawTaskContext)
进程上下文是操作系统进行进程调度的核心结构，其职责在于保存和恢复进程的执行状态。鉴于不同体系结构在进程上下文保存方式上存在显著差异（例如，x86-64 架构通常将上下文存储于内核栈上，而 riscv64 架构则可定义特定结构体以存储上下文内容），Eonix 对进程上下文及其相关操作进行了统一抽象。

通过 `RawTaskContext` 这一 Rust trait，内核在处理上下文操作时，仅需调用其所提供的统一接口，而无需关注底层架构的具体实现细节。此种基于 trait 的设计利用了 Rust 的静态分发特性，确保了零运行时开销。

所有架构的 `TaskContext` 都提供以下统一接口：
```rust
pub trait RawTaskContext: Sized {
    fn new() -> Self;
    fn set_program_counter(&mut self, pc: usize);
    fn set_stack_pointer(&mut self, sp: usize);
    fn is_interrupt_enabled(&self) -> bool;
    fn set_interrupt_enabled(&mut self, is_enabled: bool);
    fn call(&mut self, func: unsafe extern "C" fn(usize) -> !, arg: usize);
    unsafe extern "C" fn switch(from: &mut Self, to: &mut Self);
    unsafe extern "C" fn switch_to_noreturn(to: &mut Self) -> ! {
        let mut from_ctx = Self::new();
        unsafe {
            Self::switch(&mut from_ctx, to);
        }
        unreachable!("We should never return from `switch_to_noreturn()`");
    }
}
```

### Trap 上下文抽象 (RawTrapContext)
Trap 上下文用于处理各种异常情况，包括系统调用、错误、中断和定时器事件。Eonix 通过 RawTrapContext trait 提供了统一的多架构抽象，支持完整的异常处理机制。
```rust
pub trait RawTrapContext: Copy {
    type FIrq: FnOnce(fn(irqno: usize));
    type FTimer: FnOnce(fn());

    fn new() -> Self;

    /// 获取 trap 类型和相关信息
    fn trap_type(&self) -> TrapType<Self::FIrq, Self::FTimer>;

    /// 获取程序计数器
    fn get_program_counter(&self) -> usize;
    
    /// 获取栈指针
    fn get_stack_pointer(&self) -> usize;

    /// 设置程序计数器
    fn set_program_counter(&mut self, pc: usize);
    
    /// 设置栈指针
    fn set_stack_pointer(&mut self, sp: usize);

    /// 检查中断是否启用
    fn is_interrupt_enabled(&self) -> bool;
    
    /// 设置中断启用状态
    fn set_interrupt_enabled(&mut self, enabled: bool);

    /// 检查是否为用户模式
    fn is_user_mode(&self) -> bool;
    
    /// 设置用户/内核模式
    fn set_user_mode(&mut self, user: bool);

    /// 设置用户态返回值 (用于系统调用)
    fn set_user_return_value(&mut self, retval: usize);

    /// 设置用户调用栈帧 (用于信号处理等)
    fn set_user_call_frame<E>(
        &mut self,
        pc: usize,
        sp: Option<usize>,
        ra: Option<usize>,
        args: &[usize],
        write_memory: impl Fn(VAddr, &[u8]) -> Result<(), E>,
    ) -> Result<(), E>;
}

pub trait TrapReturn {
    type TaskContext: RawTaskContext;

    /// 返回到 trap 发生前的上下文
    unsafe fn trap_return(&mut self);
}
```
Trap 类型分类包括系统调用、异常、外部中断、定时器中断。

Eonix 提供了编译时类型检查机制，确保只有实现了 RawTrapContext 的类型才能用作 trap 上下文：
```rust
/// 标记类型，用于编译时检查 RawTrapContext 实现
pub struct IsRawTrapContext<T>(PhantomData<T>)
where
    T: RawTrapContext;
```

通过这些架构抽象层，Eonix 实现了：

* 代码复用: 内核核心逻辑在所有架构上共享
* 易于维护: 架构特定代码集中管理，便于调试和优化
* 快速移植: 新增架构支持时只需实现 HAL 接口
* 类型安全: 利用 Rust 的类型系统确保接口的正确使用
* 性能优化: 编译时条件编译确保零运行时开销

这些设计使得 Eonix 能够在保持高性能的同时，实现真正的跨架构可移植性。

## 杂项
