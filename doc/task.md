# 任务管理

Eonix内核的任务管理系统采用了模块化的分层设计，将任务调度的运行时层（`Task`）与POSIX规范中的线程与进程资源抽象分离，构建了一个灵活、高效的多任务管理框架。这种分层设计使各模块职责更加清晰，同时提供了较高的灵活性，能够支持不同类型的运行时与调度策略。

## 核心设计概述

任务管理是内核调度和进程间通信的基础，其核心组件分为两个层次：

### 运行时层
- **任务（Task）**：调度的基本单位，代表一个可调度的执行单元
- **执行器（Executor）**：负责具体任务的执行，管理任务的生命周期
- **调度器（Scheduler）**：管理所有任务，负责任务的调度和切换
- **执行上下文（ExecutionContext）**：保存任务执行状态，支持上下文切换
- **就绪队列（ReadyQueue）**：管理就绪的任务，供调度器使用

### POSIX资源抽象层
- **线程（Thread）**：资源分配和POSIX线程语义的实现单位，可以与Task对应
- **进程（Process）**：资源隔离的基本单位，包含线程的集合和共享资源
- **进程组（ProcessGroup）**：组织和管理一组相关进程，通常用于信号广播
- **会话（Session）**：包含多个进程组，为作业控制和终端管理提供支持

## 任务模型

任务是Eonix内核调度的基本单位，由`Task`结构体表示，每个任务包含独立的执行上下文、状态信息和执行器。任务支持协作式调度，通过 `park` 和 `unpark` 方法进行任务的暂停和恢复。

### 任务的基本结构

每个任务由`Task`结构体表示，包含以下关键组件：
```rust
pub struct Task {
    pub id: TaskId,                // 唯一标识符
    pub(crate) on_rq: AtomicBool, // 是否在就绪队列中
    pub(crate) unparked: AtomicBool, // 是否被唤醒
    pub(crate) cpu: AtomicU32,    // 亲和性CPU
    pub(crate) state: TaskState,  // 任务状态
    pub(crate) execution_context: ExecutionContext, // 执行上下文
    executor: AtomicUniqueRefCell<Option<Pin<Box<dyn Executor>>>>, // 执行器
    link_task_list: RBTreeAtomicLink, // 全局任务列表链接
}
```

### 任务的生命周期

任务创建通过 `Task::new` 方法实现，接受一个实现了 `Run` trait 的可运行对象和一个栈类型。任务创建后会获得一个 `TaskHandle`，可用于控制任务和获取任务结果。

任务的状态变化遵循以下流程：
1. 初始状态为 `RUNNING`，表示任务可以执行
2. 当任务调用 `park` 方法时，状态变为 `PARKING`
3. 如果任务没有被唤醒，则进入调度器
4. 调度器可能将任务状态变为 `PARKED`，表示任务已被挂起
5. 当任务被 `unpark` 唤醒时，状态返回 `RUNNING`，任务重新可被调度

任务执行通过 `run` 方法进行，当执行完成后返回 `ExecuteStatus::Finished` 状态。

## 执行器和调度器

### 执行器

执行器是负责具体任务执行的组件，由 `Executor` trait 定义：

```rust
pub trait Executor: Send {
    fn progress(&self) -> ExecuteStatus;
}
```

执行器通过 `progress` 方法推进任务的执行，并管理任务的输出和完成状态。`ExecutorBuilder` 提供了灵活的任务和栈配置方式，支持不同类型任务的创建。

### 调度器

调度器是任务管理的核心，负责任务的调度决策。调度器通过全局单例 `Scheduler` 实现，主要功能包括：

1. **任务切换**：通过 `schedule` 方法选择下一个要执行的任务，并进行上下文切换
2. **任务激活**：通过 `activate` 方法将任务添加到就绪队列中，使其可被调度
3. **任务跟踪**：通过全局任务列表 `TASKS` 和每CPU变量 `CURRENT_TASK` 跟踪所有任务和当前执行的任务

### 就绪队列

就绪队列是调度器的重要组成部分，使用 `ReadyQueue` 结构体管理每个CPU上就绪的任务：

```rust
pub struct ReadyQueue {
    queue: SpinIrq<VecDeque<Arc<Task>>>,
}
```

就绪队列支持任务的入队、出队和调度操作，确保任务能够高效地被选择执行。每个CPU核心维护一个独立的就绪队列，通过 `cpu_rq` 和 `local_rq` 函数访问。

## 上下文管理

执行上下文是任务调度和切换的关键，通过 `ExecutionContext` 结构体表示：

```rust
#[derive(Debug)]
pub struct ExecutionContext(UnsafeCell<TaskContext>);
```

上下文包含任务的寄存器状态，如程序计数器（IP）、栈指针（SP）和中断状态等。TaskContext 具体结构根据不同架构有不同的实现，但都提供了统一的接口。

### 上下文切换

上下文切换通过以下方式实现：

1. **`switch_to`方法**：在两个上下文间切换，保存当前状态并加载目标状态
   ```rust
   pub fn switch_to(&self, to: &Self) {
       let Self(from_ctx) = self;
       let Self(to_ctx) = to;
       unsafe {
           TaskContext::switch(&mut *from_ctx.get(), &mut *to_ctx.get());
       }
   }
   ```

2. **`switch_noreturn`方法**：实现不返回的上下文切换，用于任务退出场景
3. **`call1`方法**：支持带参数的函数调用，常用于任务初始化

每个CPU还维护一个局部调度器上下文 `LOCAL_SCHEDULER_CONTEXT`，用于在任务挂起时返回到调度器：

```rust
#[eonix_percpu::define_percpu]
static LOCAL_SCHEDULER_CONTEXT: ExecutionContext = ExecutionContext::new();
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

## 异步支持与阻塞操作

Eonix任务系统充分利用Rust语言的异步编程模型，提供了高效的异步执行和阻塞操作支持。

### 异步Future执行

Task实现了`block_on`方法，可以在当前任务上阻塞执行一个`Future`对象：

```rust
pub fn block_on<F>(future: F) -> F::Output
where
    F: Future,
{
    let waker = Waker::from(Task::current().clone());
    let mut context = Context::from_waker(&waker);
    let mut future = pin!(future);

    loop {
        if let Poll::Ready(output) = future.as_mut().poll(&mut context) {
            break output;
        }
        Task::park();
    }
}
```

### 任务等待与唤醒机制

Task实现了`Wake` trait，使其可以作为异步唤醒器使用：

```rust
impl Wake for Task {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.unpark();
    }
}
```

`JoinHandle`提供了等待任务完成并获取结果的方法，通过`join`方法阻塞当前任务直到目标任务完成并返回其结果。

当任务需要等待某些事件时，可以调用`Task::park`方法暂停自己，并在事件发生时通过`unpark`方法唤醒。这种机制结合异步编程模型，为Eonix提供了高效的I/O和事件处理能力。

## 关键特性与技术亮点

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

### 多核支持

Eonix内核原生支持多核处理器，通过对称多处理（SMP）架构实现：
- 每个CPU核心有独立的调度器上下文（`LOCAL_SCHEDULER_CONTEXT`）
- 每个CPU有独立的就绪队列（`ReadyQueue`）
- 全局任务列表（`TASKS`）协调各个CPU之间的任务分配

### POSIX兼容性

Eonix内核的任务管理模块支持POSIX标准中的核心概念：
- 完整的线程、进程、进程组和会话抽象
- 符合标准的信号处理机制
- 完善的作业控制功能
- 统一的系统调用接口

## 待优化事项

虽然Eonix内核已经实现了完善的任务管理系统，但以下几个方面仍有优化空间：

### 运行时与POSIX层接口优化

当前的分层设计虽然解耦了不同层次的职责，但有些接口可能存在冗余或不一致，需要进一步精简和统一。主要包括：
- Thread与Task之间的转换接口可以更加优化
- 信号处理机制在两层之间的交互可以进一步简化

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

### 无栈协程支持

我们希望在内核中加入无栈协程的支持，以提供更高效的异步编程模型。无栈协程可以减少上下文切换的开销，并允许更灵活的任务调度。

#### 无栈协程的优势

无栈协程（Stackless Coroutine）相比传统的有栈协程（如Eonix当前的Task模型）具有以下显著优势：

1. **内存效率**：无栈协程不需要为每个协程分配独立的栈空间，大幅降低内存占用，特别适合高并发场景
2. **切换开销低**：无需保存和恢复完整的栈和寄存器上下文，切换开销远低于有栈协程和线程
3. **更好的编译时优化**：编译器可以对无栈协程进行更彻底的优化，包括内联和状态机优化
4. **更易于调试**：无栈协程的状态更加明确，更容易追踪和调试

#### 实现方案

在Eonix内核中实现无栈协程支持，我们计划采用以下方案：

1. **基于Rust的生成器特性**：利用Rust语言对异步编程和生成器的原生支持，通过`async`/`await`语法构建无栈协程
   ```rust
   // 无栈协程示例
   async fn example_coroutine() -> Result<(), Error> {
       // 异步操作，不会阻塞内核
       let data = read_disk_async().await?;
       process_data(data).await?;
       Ok(())
   }
   ```

2. **Future执行器优化**：开发专门的内核Future执行器，高效管理和调度异步任务
   ```rust
   pub struct KernelExecutor {
       task_queue: LinkedList<Pin<Box<dyn Future<Output = ()>>>>, // 使用一个侵入式链表
       // 其他字段...
   }
   
   impl KernelExecutor {
       pub fn spawn<F>(&mut self, future: F) 
       where F: Future<Output = ()> + 'static {
           self.task_queue.push_back(Box::pin(future));
       }
       
       pub fn run(&mut self) {
           // 执行调度逻辑...
       }
   }
   ```

3. **与现有任务模型集成**：保持与现有Task模型的兼容性，允许两种模式共存
   - 通过适配层使无栈协程可以在当前的调度器上运行
   - 允许现有代码逐步迁移到无栈协程模型

#### 与现有任务管理系统的集成

Eonix的分层设计为集成无栈协程提供了良好基础。我们计划：

1. 在运行时层（`eonix_runtime`）增加对无栈协程的支持：
   - 实现`Future`特性的执行器
   - 提供与协程状态相关的唤醒与调度接口

2. 在POSIX抽象层保持现有的线程、进程抽象不变，但内部实现可利用无栈协程：
   - 系统调用可以返回`Future`而非直接阻塞
   - I/O操作可以异步化，提高并发性能

3. 提供统一的API适配层，使异步代码和同步代码可以无缝协作：
   ```rust
   // 在同步环境中执行异步代码
   fn sync_operation() {
       runtime::block_on(async_operation())
   }
   
   // 在异步环境中执行同步代码
   async fn run_blocking<F, R>(f: F) -> R
   where F: FnOnce() -> R + Send + 'static,
         R: Send + 'static {
       // 在单独的线程中执行阻塞操作
   }
   ```

#### 实现挑战与解决方案

由于Eonix内核中有大量现有的阻塞API，迁移到无栈协程模型主要面临需要将所有阻塞操作改造为异步模式，需要大量的接口重构工作的挑战。Eonix 经过我们的改造，其架构已经具备集成无栈协程的能力。我们的分层设计使运行时层和资源抽象层解耦，为引入无栈协程奠定了基础。但由于需要对所有阻塞相关接口进行修改，这是一项工作量巨大的任务，我们暂时没有完全实现。我们只要写一个简单的无栈协程调度器，将 Future 对象放入其中即可。这样即可实现比如真正的 IOWorker （而不是目前基于Task的状态）。

我们计划在未来的版本中逐步引入无栈协程支持，首先从非关键路径开始，如目前已经几乎完全完成的设备驱动，以及部分完成的文件系统操作，然后逐步扩展到核心子系统，最终实现全面的异步编程支持，以进一步提升系统的性能、并发能力和资源利用率。
