# Eonix

*Eonix* 是一个使用 Rust 编写，适用于多处理器多架构，基于rust async语法的有/无栈异步任务管理的宏内核操作系统。本项目是原创的内核架构设计，**所有核心模块均由团队成员独立设计和实现**。

*Eonix* 项目的目标是创建一个高效、安全、可靠的操作系统。我们的目标是提供一个简单的内核设计，使得它易于理解和维护。同时，我们也希望 *Eonix* 能够提供足够复杂的特性，如虚拟内存管理、多进程支持、POSIX 兼容的文件系统模型、易于使用的设备驱动开发接口等，使其拥有较强的实用性。项目采用 Rust 的 crate 系统实现模块化架构。

## 自初赛以来的进展

- 完善对 ext4 文件系统和 page cache 的支持
- 重构当前的 task runtime，计划使用统一的有栈/无栈协程 runtime 取代目前的实现，默认使用无堆栈协程作为基本调度单元，在确实需要有栈的执行环境的情况下，也提供 `stackful()` 接口运行基于时间片的抢占式基础上运行的任务，具体可以看文档中的[任务管理](doc/task.md)
- 适配内核中所有的异步函数
- 完善对网络协议栈的支持
- 可以通过决赛的线上测试用例的85分

## 项目特性

- [x] 多处理器支持
- [x] 多架构支持（x86_64, riscv64, Loongarch64）
- [x] 内存管理
- [x] 基于有栈/无栈协程的任务管理
- [x] *POSIX* 兼容性，兼容 *Linux* 系统调用接口
- [x] 静态 ELF 加载器
- [x] TTY 及任务控制接口（正在进一步实现）
- [x] FAT32 文件系统的读取实现
- [x] EXT4 文件系统的读取写入实现
- [x] 全部 Rust 化
- [x] 动态加载器
- [x] 网卡驱动
- [x] *POSIX* 线程接口
- [ ] 用户、权限(WIP)

截止6月30日，可以通过初赛的 basic，busybox 的大部分测试用例，lua 的全部测试用例，还有较高的 iozone 得分。其余测试用例由于时间关系未完全实现。

![测试](doc/images/image.png)

## 文档

本项目开发时代码仓库在 [GitHub](https://github.com/greatbridf/osdev.git)。

- [启动加载](doc/boot.md)
- [内存管理](doc/memory.md)
- [任务管理](doc/task.md)
- [文件系统](doc/filesystem.md)
- [设备驱动](doc/device_driver.md)
- [多平台支持](doc/multi_arch.md)
- [网络栈](doc/net.md)

初赛的汇报PPT也在仓库中，由于仓库大小限制，汇报的视频上传到了[bilibili](https://www.bilibili.com/video/BV1PwgCzME1F/)中。

## 代码树结构

```
.
├── Cargo.lock                     # Rust 依赖锁定
├── Cargo.toml                     # Rust 项目配置
├── Makefile                       # 构建脚本
├── Makefile.real                  # 实际构建配置
├── Makefile.src                   # Makefile 模板
├── README.md                      # 项目说明
├── build/                         # 构建输出
├── build.rs                       # Rust 构建脚本
├── configure                      # 配置工具
├── crates                         # 核心 Rust 库
│   ├── atomic_unique_refcell      # 原子引用计数
│   ├── buddy_allocator            # Buddy 内存分配器
│   ├── eonix_hal                  # 硬件抽象层
│   │   ├── eonix_hal_traits       # HAL 通用接口
│   │   └── src                    # HAL 源码
│   │       └── arch               # 架构特定 HAL
│   │           ├── loongarch64    # LoongArch64 HAL
│   │           ├── x86_64         # x86_64 HAL
│   │           └── riscv64        # RISC-V 64 HAL
│   ├── eonix_log                  # 内核日志
│   ├── eonix_mm                   # 内存管理
│   ├── eonix_percpu               # Per-CPU 变量
│   ├── eonix_preempt              # 内核抢占
│   ├── eonix_runtime              # 内核运行时
│   ├── eonix_sync                 # 内核同步原语
│   ├── intrusive_list             # 侵入式链表
│   ├── pointers                   # 自定义指针
│   ├── posix_types                # POSIX 类型
│   └── slab_allocator             # Slab 内存分配器
├── doc                            # 项目文档
├── macros                         # 自定义宏
├── script                         # 辅助脚本
├── src                            # 内核源码
│   ├── driver                     # 设备驱动
│   │   ├── ahci                   # AHCI 驱动
│   │   ├── e1000e                 # E1000E 网卡驱动
│   │   ├── e1000e.rs              # E1000E 驱动入口
│   │   ├── goldfish_rtc.rs        # Goldfish RTC 驱动
│   │   ├── sbi_console.rs         # RISC-V SBI 控制台
│   │   ├── serial                 # 串口驱动
│   │   ├── serial.rs              # 串口驱动入口
│   │   ├── virtio                 # VirtIO 驱动
│   │   └── virtio.rs              # VirtIO 驱动入口
│   ├── driver.rs                  # 驱动模块定义
│   ├── fs                         # 文件系统
│   │   ├── ext4.rs                # EXT4 文件系统
│   │   ├── fat32                  # FAT32 文件系统
│   │   ├── fat32.rs               # FAT32 文件系统入口
│   │   ├── mod.rs                 # 文件系统模块根
│   │   ├── procfs.rs              # Procfs 伪文件系统
│   │   ├── shm.rs                 # 共享内存
│   │   └── tmpfs.rs               # 临时文件系统
│   ├── hash.rs                    # 哈希工具
│   ├── io.rs                      # I/O 工具
│   ├── kernel                     # 内核核心功能
│   │   ├── block                  # 块设备管理
│   │   ├── block.rs               # 块设备入口
│   │   ├── chardev.rs             # 字符设备抽象
│   │   ├── console.rs             # 内核控制台
│   │   ├── constants.rs           # 内核常量
│   │   ├── interrupt.rs           # 中断处理
│   │   ├── mem                    # 内存管理
│   │   │   ├── access.rs          # 内存访问辅助
│   │   │   ├── address.rs         # 地址抽象
│   │   │   ├── allocator.rs       # 内存分配器接口
│   │   │   ├── mm_area.rs         # 内存映射区域
│   │   │   ├── mm_list            # 内存管理列表
│   │   │   ├── mm_list.rs         # 内存管理列表入口
│   │   │   ├── page_alloc         # 页分配器
│   │   │   ├── page_alloc.rs      # 页分配器入口
│   │   │   ├── page_cache.rs      # Page Cache
│   │   │   └── paging.rs          # 分页机制
│   │   ├── mem.rs                 # 内存管理入口
│   │   ├── pcie                   # PCIe 总线管理
│   │   ├── pcie.rs                # PCIe 模块入口
│   │   ├── rtc                    # 实时时钟管理
│   │   ├── syscall                # 系统调用
│   │   ├── syscall.rs             # 系统调用入口
│   │   ├── task                   # 任务（进程/线程）管理
│   │   │   ├── clone.rs           # 进程克隆
│   │   │   ├── futex.rs           # Futex 实现
│   │   │   ├── kernel_stack.rs    # 内核栈管理
│   │   │   ├── loader             # 可执行文件加载器
│   │   │   │   ├── aux_vec.rs     # 辅助向量
│   │   │   │   ├── elf.rs         # ELF 加载器
│   │   │   │   └── mod.rs         # `loader` 模块根
│   │   │   ├── process.rs         # 进程管理
│   │   │   ├── process_group.rs   # 进程组管理
│   │   │   ├── process_list.rs    # 进程列表
│   │   │   ├── session.rs         # 会话管理
│   │   │   ├── signal             # 信号处理
│   │   │   ├── signal.rs          # 信号处理入口
│   │   │   └── thread.rs          # 线程管理
│   │   ├── task.rs                # 任务管理入口
│   │   ├── terminal.rs            # 终端设备抽象
│   │   ├── timer.rs               # 定时器管理
│   │   ├── user                   # 用户空间交互
│   │   │   └── dataflow.rs        # 用户数据流
│   │   ├── user.rs                # 用户交互入口
│   │   └── vfs                    # 虚拟文件系统 (VFS)
│   │       ├── dentry             # Dentry 模块
│   │       │   └── dcache.rs      # Dentry 缓存
│   │       ├── dentry.rs          # Dentry 结构及操作
│   │       ├── file.rs            # 打开文件描述符
│   │       ├── filearray.rs       # 进程文件数组
│   │       ├── inode.rs           # Inode 结构及操作
│   │       ├── mod.rs             # `vfs` 模块根
│   │       ├── mount.rs           # 文件系统挂载管理
│   │       └── vfs.rs             # VFS 核心接口
│   ├── kernel.rs                  # 内核模块顶层定义
│   ├── kernel_init.rs             # 内核初始化
│   ├── lib.rs                     # 核心库文件
│   ├── net                        # 网络协议栈
│   ├── net.rs                     # 网络模块入口
│   ├── path.rs                    # 文件路径处理
│   ├── prelude.rs                 # 常用公共导入
│   ├── rcu.rs                     # RCU 机制
│   ├── sync                       # 同步原语
│   └── sync.rs                    # 同步模块入口
├── user-programs                  # 用户空间测试程序
└── x86_64-unknown-none.json       # x86_64 裸机工具链配置
```

## 编译 & 运行

（以下内容适用于 github 上的版本，gitlab 中的 Makefile 是为了测评删减过的）

### 构建依赖

#### 编译

- [GCC (测试过)](https://gcc.gnu.org/) or

- [Rust(nightly-2024-12-18)](https://www.rust-lang.org/)
- [CMake](https://cmake.org/)

#### 生成硬盘镜像

- [(Optional) busybox (项目目录下有预编译过的busybox)](https://www.busybox.net/)
- [fdisk](https://www.gnu.org/software/fdisk/)
- [mtools](http://www.gnu.org/software/mtools/)

#### 调试 & 运行

- [GDB](https://www.gnu.org/software/gdb/)
- [QEMU](https://www.qemu.org/)

### 编译及运行命令

```bash
# 配置构建环境
./configure && make build

# 直接运行

make run

# 如果需要调试

# 1: 启动调试
make srun

# 2: 启动调试器
make debug

# 或者如果有 tmux

make tmux-debug
```

可能需要在运行 `./configure` 时在环境变量中指定正确版本的构建工具。

- `DEFAULT_ARCH`: 在调用 Makefile 时如果不进行额外指定，默认使用的架构。默认为 `x86_64`。
- `QEMU`: 用于调试运行的 QEMU。默认使用 `qemu-system-$(ARCH)`。
- `GDB`: 用于 `make debug` 的 GDB。我们将默认查找 `$(ARCH)-elf-gdb` 并检查支持的架构。
- `FDISK`: 用于创建磁盘镜像分区表的 fdisk 可执行文件，要求使用来自 util-linux 版本的 fdisk。默认使用 `fdisk`。
- `IMG`: 除启动磁盘以外，额外的磁盘镜像文件。默认不使用。

在运行 make 时可以指定的额外选项：

- `HOST`: 当前平台架构，用于决定 qemu 的默认加速模式，默认使用 `uname -s` 的输出。
- `ARCH`: 编译运行的目标架构，默认使用 configure 时指定的值。
- `MODE`: 编译运行的模式，可以使用 `debug` 或者是 `release`。
- `SMP`: 是否运行多处理器处理，默认使用 4 CPU。
- `QEMU`: 手动指定 qemu 路径。
- `GDB`: 手动指定 gdb 路径。
- `FDISK`: 手动指定 fdisk 路径。
- `QEMU_ACCEL`: 手动指定要使用的 qemu 加速方法。
- `DEBUG_TRAPS`: 是否要进行 trap 的调试，使 qemu 输出详细的 trap 日志。
- `FEATURES`: 手动指定要编译的特性，使用逗号分隔。具体见 `Cargo.toml` 中的 `features` 字段。

## 参考

参考代码：
* arceos[https://github.com/arceos-org/arceos]
* asterinas[https://github.com/asterinas/asterinas]
* linux[https://github.com/torvalds/linux]

直接使用内核组件：
* ext4-rs[https://github.com/yuoo655/ext4_rs]
* another_ext4[https://github.com/PKTH-Jx/another_ext4.git]
* virtio-drivers[https://github.com/rcore-os/virtio-drivers]
* xmas-elf[https://github.com/nrc/xmas-elf]
* intrusive-rs[https://github.com/Amanieu/intrusive-rs.git]
* smoltcp[https://github.com/smoltcp-rs/smoltcp]
