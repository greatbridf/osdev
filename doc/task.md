# 任务管理

在决赛阶段，我们对初赛时未完全完成的任务管理模块进行了完善。我们的运行时现在以无栈协程为打底，同时可以兼顾无栈协程以及有栈协程，做到了 "Pay for and only for what you need, and nothing else" 的零成本抽象思想。Eonix内核的任务管理系统采用了模块化的分层设计，将任务调度的运行时层（`Task`）、有栈与无栈的区分（是否使用 `stackful` 进行包装）以及POSIX规范中的线程与进程资源抽象分离，构建了一个灵活、高效的多任务管理框架。这种分层设计使各模块职责更加清晰，同时提供了较高的灵活性，能够支持不同类型的运行时与调度策略。

## 核心设计概述

任务管理是内核调度和进程间通信的基础，其核心组件分为两个层次：

### 运行时层
- **任务（Task）**：调度的基本单位，代表一个无栈打底（我们可以在其上实现有栈的任务，具体见下），可调度的执行单元
- **运行时（Runtime）**：管理所有任务，负责任务的调度和切换
- **就绪队列（ReadyQueue）**：管理就绪的任务，供调度器使用

### POSIX资源抽象层
- **线程（Thread）**：资源分配和POSIX线程语义的实现单位，可以与Task对应
- **进程（Process）**：资源隔离的基本单位，包含线程的集合和共享资源
- **进程组（ProcessGroup）**：组织和管理一组相关进程，通常用于信号广播
- **会话（Session）**：包含多个进程组，为作业控制和终端管理提供支持

## 任务模型

### 无栈

任务是Eonix内核调度的基本单位，由`Task`结构体表示，每个任务包含状态信息和对应的 Future 对象。每一个任务天生就是无栈的，
所以我们的运行时在实现的时候可以非常简单与纯粹，不需要考虑到复杂的同步、抢占等问题，每个 CPU 只需要一个栈，一个 Runtime
进行任务的调度。如果你不需要跑有栈的任务，这就是你全部需要的资源开销。因此这些任务非常轻量化，例如我们可以实现 call_rcu
来将 RCU 对象的释放推迟。这个函数可以直接将这个对象扔到一个 async 闭包中，等待 RCU 读者都释放后再执行。

无栈任务的创建也非常简单，只需要在 `Runtime` 对象上调用 `spawn` 方法，传入你要跑的 `Future` 即可。

```rust
async foo() {
    let join_handle = RUNTIME.spawn(async {
        println_info!("We are in the task!");
        sleep(3).await;

        42
    });

    assert_eq!(join_handle.await, 42);
}
```

无栈任务遵守协作式多任务的规范，所有人在需要等待时使用 `await` 让出控制权，并且保存 `Waker` 以便准备好时唤醒。如果你
需要做一些计算密集型的任务，你可以将这些任务拆分到专门的有栈的、按时间片抢占的任务中，或者是使用有栈的 Worker 来运行
你的任务。我们将在下方介绍。

### 有栈

有栈任务是在无栈任务的基础上，将对应的 `Future` 对象通过 `stackful` 这个函数进行包装后得到的 `Future`。它在
`Runtime` 看来，以及调度时与正常的无栈任务没有 **任何** 的区别。在内部，我们会为每一个有栈的任务分配一个栈，并将这个
`Future` 对象保存到栈上。我们使用 `TrapContext` 的 `captured_trap_return` 这个功能来实现有栈的任务，捕获时钟
中断来实现时间片的抢占。如果想将一个计算密集型的任务转变成有栈、可抢占的任务，或者是将一个现有的，很难 async 化的任务
适配到我们的内核中，我们只需要创建一个正常的 `Future`，然后将其包在 `stackful` 的里面......

```rust
RUNTIME.spawn(stackful(async { // 这里使用 stackful 函数将 Future 包装成有栈的任务
    for generation in 0..2147483647 {
        let result = some_computation_heavy_work(generation);
        println_info!("Generation {}: Result = {}", generation, result);
    }
}));
```

我们对于上面的抽象得到的好处非常明显：

如果我们想要一个轻量级的任务，我们可以使用无栈协程，保证不要在非 await 的地方进行阻塞调用，我们就可以获益于 Rust 的无
栈协程带来的高性能，以及方便的使用方法。由于每个任务在切换的过程其实只是 `poll` 函数的调用，而任务内存的分配我们通过
高性能的 Slab 和 Buddy 分配器，做到了极低的开销。我们没有为我们不需要的功能买单（有栈任务的栈资源，以及上下文切换的
开销），同时对于我们需要的部分，我们也将开销做到了最小。

如果我们确实需要抢占，或者是需要更复杂的调度策略，我们可以使用有栈的任务。但是我们的抽象设计仍然保证了我们不会比传统的
任务调度方案更劣，甚至会更好：
1. 有栈任务的上下文切换开销与传统线程相同。
2. 有栈任务与传统线程相同，需要栈空间。但是我们将 Future 对象也相应地放到了栈上，从而实现了更高效的内存使用。
3. 我们的调度策略仍然是基于协作式多任务的，不会因为使用有栈任务而引入额外的抢占开销。
4. 我们在实现过程当中将有栈任务的资源放到了 Future 对象中，从而保证了当任务结束时，资源一定会得到释放。减少了设计的复杂度。

更具体地了解我们的实现，可以参考 crates/eonix_runtime/src/scheduler.rs、 crates/eonix_runtime/src/task.rs
和src/kernel/task.rs。

### 任务的基本结构

每个任务由`Task`结构体表示，包含以下关键组件：
```rust
pub struct Task {
    /// 任务的唯一标识符
    pub id: TaskId,
    /// 任务所归属的 CPU
    pub(crate) cpu: AtomicU32,
    /// 状态
    pub(crate) state: TaskState,
    /// 任务的 Executor，一个类型擦除的 Future
    executor: AtomicUniqueRefCell<Executor>,
    /// 在全局任务链表中的 Link
    link_task_list: RBTreeAtomicLink,
    /// 在就绪队列中的 Link
    link_ready_queue: LinkedListAtomicLink,
}
```

## 信号管理

信号是Eonix内核进程间通信和异常处理的核心机制，用于实现进程控制、任务协作和异常处理。Eonix的信号系统符合POSIX标准，支持多种信号类型和处理机制。

### 信号的基本结构

信号由`Signal`结构体表示，包含信号类型，每个信号通过唯一的数字标识：

```rust
pub type Signal = u8;

// 常见的信号类型
pub const SIGHUP: Signal = 1;    // 终端挂起
pub const SIGINT: Signal = 2;    // 终端中断
pub const SIGQUIT: Signal = 3;   // 终端退出
pub const SIGKILL: Signal = 9;   // 强制终止
pub const SIGSEGV: Signal = 11;  // 段错误
pub const SIGSTOP: Signal = 19;  // 停止进程
```

Eonix支持常见的POSIX信号类型，包括：
- **终止信号**：如`SIGTERM`、`SIGKILL`，用于立即终止进程
- **停止信号**：如`SIGSTOP`、`SIGTSTP`，用于暂停进程
- **用户自定义信号**：如`SIGUSR1`、`SIGUSR2`，用于用户进程间通信
- **忽略信号**：如`SIGCHLD`，默认行为是被内核忽略
- **核心转储信号**：如`SIGSEGV`、`SIGFPE`，会生成核心转储文件并终止进程

### 信号管理与挂起

信号管理由`SignalList`完成，负责信号的挂起、屏蔽和优先级管理：

```rust
pub struct SignalList {
    pending: Spin<BinaryHeap<SignalInfo>>,
    mask: AtomicU64,
}
```

挂起信号存储在`BinaryHeap`中，按照优先级排序，以确保高优先级信号优先处理。通过`SignalList::mask`和`SignalList::unmask`动态管理信号掩码，决定哪些信号会被屏蔽。

### 信号处理与分发

信号的处理方式由`SignalAction`定义，支持以下三种模式：

1. **默认行为**：系统为每种信号定义的默认处理方式，如终止进程、暂停进程或忽略信号
2. **自定义Handler**：用户可以为特定信号设置自定义处理器，在用户态执行信号处理逻辑
3. **忽略信号**：指定某些信号被忽略，如`SIGCHLD`默认不会影响进程运行

信号分发可以针对单个线程、进程组或会话的前台进程组进行广播。通过`SignalList::raise`方法，信号被加入目标对象的挂起信号队列，并根据优先级等待处理。

### 信号与上下文切换

信号处理与上下文切换紧密结合。当线程被调度器切换到运行状态时，内核会检测其挂起信号列表，并调用相应的信号处理器。信号处理器通过修改`InterruptContext`返回地址，将执行跳转到用户态的信号处理函数中。

对于不可屏蔽信号（如`SIGKILL`、`SIGSTOP`），内核会立即终止或暂停目标进程，不会经过挂起队列或信号处理器。

## 会话、进程组与作业控制

Eonix内核中的会话和进程组机制是任务管理系统的重要组成部分，为进程的组织管理、信号广播和作业控制提供了基础。这些机制与POSIX标准兼容，支持完整的作业控制功能。

### 会话的基本结构

会话由`Session`结构体表示，通过`SessionJobControl`管理前台进程组和控制终端：

```rust
pub struct Session {
    pub sid: u32,  // 会话ID
    job_control: RwSemaphore<SessionJobControl>,
    groups: Spin<BTreeMap<u32, Arc<ProcessGroup>>>, // 进程组集合
}

struct SessionJobControl {
    leader: Option<Weak<Process>>,        // 会话领导进程
    foreground: Option<Arc<ProcessGroup>>, // 前台进程组
    control_terminal: Option<Arc<dyn Terminal>>, // 控制终端
}
```

会话包含以下核心组件：
- **SID（sid）**：由会话领导进程的PID决定，是会话的唯一标识
- **领导进程（leader）**：会话的创建者，负责初始化会话并管理其生命周期
- **前台进程组（foreground）**：当前与用户交互的进程组
- **控制终端（control_terminal）**：绑定到会话的终端设备
- **进程组集合（groups）**：存储会话中的所有进程组

### 进程组的基本结构

进程组由`ProcessGroup`结构体表示，是进程的集合，主要用于信号广播和作业管理：

```rust
pub struct ProcessGroup {
    pub pgid: u32,  // 进程组ID
    leader: RCUPointer<Process>,  // 领导进程
    session: Arc<Session>,  // 所属会话
    processes: Spin<BTreeMap<u32, Arc<Process>>>, // 成员进程
}
```

每个进程组包含以下部分：
- **进程组ID（pgid）**：唯一标识进程组，通常等于其领导进程的PID
- **领导进程（leader）**：进程组的创建者
- **所属会话（session）**：进程组所属的会话
- **成员进程（processes）**：使用`BTreeMap`存储进程组内的所有进程

### 作业控制

作业控制是Unix-like系统的关键特性，允许用户管理多个并发任务。Eonix通过会话和进程组机制实现了完整的作业控制，支持以下功能：

1. **前后台作业切换**：通过`Session::set_foreground_pgid`设置前台进程组
2. **信号广播**：通过`Session::raise_foreground`将信号发送给前台进程组
3. **终端绑定**：使用`Session::set_control_terminal`将终端绑定到会话

作业控制的工作流程如下图所示：

```
+---------------+      控制     +--------------+
| 控制终端(TTY) | <-----------> | 会话(Session) |
+---------------+               +--------------+
       |                              |
       | 输入/输出                     | 包含
       v                              v
+------------------+          +----------------+
| 前台进程组(PGID) | <-------- | 其他进程组(PGID) |
+------------------+   切换    +----------------+
       |                              |
       | 包含                          | 包含
       v                              v
+---------------+               +---------------+
|  进程(PID)    |               |  进程(PID)    |
+---------------+               +---------------+
```

## 线程与进程抽象

Eonix内核将线程与进程的抽象从基本调度单元（Task）中分离出来，放在 `src/kernel/task` 模块中实现。这种设计使得POSIX规范中的资源抽象与实际的调度执行单元解耦，带来了更高的灵活性和模块化程度。

### 线程抽象

线程由 `Thread` 结构体表示，是POSIX线程语义的实现单位：

```rust
pub struct Thread {
    pub tid: u32,  // 线程ID
    pub trap_ctx: Spin<TrapContext>,  // 陷阱上下文
    pub fpu_state: FpuState,  // 浮点状态
    pub fs_context: FsContext,  // 文件系统上下文
    pub files: FileArray,  // 文件描述符表
    pub signal_list: SignalList,  // 信号列表
    process: Arc<Process>,  // 所属进程
    // ...其他字段
}
```

线程包含以下关键组件：
- **线程ID（tid）**：唯一标识线程的数字
- **陷阱上下文（trap_ctx）**：处理中断和系统调用的上下文
- **浮点状态（fpu_state）**：保存浮点寄存器状态
- **文件系统上下文和文件描述符表**：线程特有的文件资源
- **信号列表（signal_list）**：线程接收的信号队列
- **所属进程（process）**：指向包含此线程的进程

线程可以通过 `ThreadBuilder` 创建，支持配置线程的各种属性。

### 进程抽象

进程由 `Process` 结构体表示，是资源隔离和管理的基本单位：

```rust
pub struct Process {
    pub pid: u32,  // 进程ID
    pub wait_list: WaitList,  // 等待列表
    pub mm_list: MMList,  // 内存管理列表
    threads: Spin<BTreeMap<u32, Arc<Thread>>>,  // 线程集合
    children: Spin<BTreeMap<u32, Arc<Process>>>,  // 子进程
    parent: RCUPointer<Process>,  // 父进程
    pgroup: Arc<ProcessGroup>,  // 进程组
    session: Arc<Session>,  // 会话
    // ...其他字段
}
```

进程包含以下关键组件：
- **进程ID（pid）**：唯一标识进程
- **内存管理列表（mm_list）**：管理进程的地址空间
- **线程集合（threads）**：属于该进程的所有线程
- **父子关系（parent/children）**：指向父进程和子进程
- **等待列表（wait_list）**：用于父进程等待子进程状态变化
- **进程组和会话（pgroup/session）**：进程所属的组织结构

### 线程与任务的关系

在Eonix内核中，Thread（线程）和Task（任务）的关系是关联但分离的：

1. **Thread** 代表POSIX线程语义和资源，负责实现线程的API和状态管理
2. **Task** 代表实际的调度单元，负责任务的执行和调度

这种设计允许：
- 一个Thread可以使用Task来执行，也可以使用其他运行时实现
- 运行时层可以独立演化，而不影响POSIX语义的实现
- 更容易支持不同类型的线程模型，如内核线程、用户线程等

### 抢占式调度

Eonix内核实现了基于时间片的抢占式调度机制。在时钟中断处理中，系统会调用 `should_reschedule()` 函数检查当前任务的时间片是否已经用尽：

```rust
pub fn should_reschedule() -> bool {
    #[eonix_percpu::define_percpu]
    static PREV_SCHED_TICK: usize = 0;

    let prev_tick = PREV_SCHED_TICK.get();
    let current_tick = Ticks::now().0;

    if Ticks(current_tick - prev_tick).in_msecs() >= 10 {
        PREV_SCHED_TICK.set(current_tick);
        true
    } else {
        false
    }
}
```

如果时间片已用尽（默认为10毫秒）且抢占未被禁用（`preempt::count() == 0`），调度器会触发调度流程：

```rust
match trap_ctx.trap_type() {
    // ...
    TrapType::Timer { callback } => {
        callback(timer_interrupt);

        if eonix_preempt::count() == 0 && should_reschedule() {
            eonix_preempt::disable();
            Scheduler::schedule();
        }
    }
}
```

Eonix的调度机制既支持协作式调度（任务主动通过 `park` 让出CPU），也支持抢占式调度（时钟中断强制切换任务），提供了更好的实时性和公平性。

### 分层设计的任务管理

Eonix内核采用了创新的分层设计来管理任务：

1. **运行时层（eonix_runtime）**：专注于任务调度和执行的底层机制，提供高效的任务切换和异步支持
2. **POSIX资源抽象层（src/kernel/task）**：实现POSIX标准中的线程、进程、进程组和会话等概念

这种分层设计带来以下优势：

1. **解耦**：运行时调度单元与POSIX资源抽象分离，使每个模块职责清晰
2. **灵活性**：用户态进程可以使用不同的运行时实现，如基于Task的标准运行时或无栈协程的运行时
3. **可靠性**：模块间接口明确，降低了错误发生的可能性

### POSIX兼容性

Eonix内核的任务管理模块支持POSIX标准中的核心概念：
- 完整的线程、进程、进程组和会话抽象
- 符合标准的信号处理机制
- 完善的作业控制功能
- 统一的系统调用接口

## 待优化事项

虽然Eonix内核已经实现了完善的任务管理系统，但以下几个方面仍有优化空间：

### 多核负载均衡

改进跨CPU的任务迁移和负载均衡机制，特别是在CPU核心数量较多时，提高整体系统效率：
- 实现更智能的CPU亲和性策略
- 根据CPU负载自动迁移任务
- 考虑任务的缓存局部性优化调度决策

### 任务状态监控

增强任务状态的追踪和统计功能，放在procfs中，为系统调优提供更多数据支持：
```
/proc/
  ├── [pid]/          # 进程信息目录
  │     ├── status    # 进程状态信息
  │     ├── stat      # 进程统计信息
  │     └── task/     # 线程信息目录
  │           └── [tid]/  # 线程信息
  └── sched          # 调度器统计信息
```
