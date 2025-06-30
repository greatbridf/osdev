# 文件系统

Eonix 内核的文件系统模块旨在提供高效、稳定且灵活的文件管理能力。它通过 `Inode`、`Dentry` 和 `DentryCache` 等核心数据结构实现路径解析与缓存机制，并支持 `FAT32`、`EXT4` 磁盘文件系统以及 `TempFS` 内存文件系统。此外，Eonix 还引入了 Page Cache，显著提升文件系统的整体性能,这些组件与虚拟文件系统（VFS）层紧密协作.

---

## 1. 虚拟文件系统（VFS）层
虚拟文件系统（VFS）层是 Eonix 文件系统架构的核心抽象，它为上层应用程序及内核其他模块提供了统一的文件系统接口，从而屏蔽了底层具体文件系统（如 FAT32、EXT4、TempFS）的内在差异。VFS 位于文件系统栈的顶层，使得 Eonix 能够透明地处理各类文件系统，无论它们的存储介质或内部结构如何。

VFS 的核心功能包括：

* 统一接口：VFS 定义了一套标准的文件操作接口（例如 open、read、write、close、mkdir、unlink 等），内核的其余部分及用户空间程序均通过这些统一接口访问文件系统。

* 文件系统注册与挂载：VFS 允许不同的具体文件系统（例如 FAT32 驱动、EXT4 驱动）向其注册。它还管理着挂载点，将不同文件系统连接至一个单一、统一的目录树结构中。

* 请求分发：当文件操作请求抵达 VFS 层时，VFS 会根据请求路径所在的挂载点，将该请求透明地分发至对应的具体文件系统驱动进行处理。

* 命名空间管理：VFS 维护着文件系统的全局命名空间，确保路径解析的一致性与正确性。

### Vfs trait

Vfs trait 定义了所有文件系统必须具备的基本属性和查询接口，以确保 VFS 层能够以统一方式与它们进行交互。
```rust
pub trait Vfs: Send + Sync + AsAny {
    fn io_blksize(&self) -> usize;
    fn fs_devid(&self) -> DevId;
    fn is_read_only(&self) -> bool;
}
```

### MountCreator trait

为支持多种文件系统的动态加载与挂载，Eonix VFS 层引入了 MountCreator trait。任何期望被 Eonix 支持的文件系统类型，均须实现此 trait，以使 VFS 能够识别其文件系统类型并创建对应的挂载实例。
```rust
pub trait MountCreator: Send + Sync {
    fn check_signature(&self, first_block: &[u8]) -> KResult<bool>;
    fn create_mount(&self, source: &str, flags: u64, mp: &Arc<Dentry>) -> KResult<Mount>;
}
```

---

## 2. Inode：文件和目录的核心描述

Inode 是 Eonix 文件系统中表示文件或目录的核心结构，其职责在于存储并管理文件的元数据，同时提供对文件的基础操作接口。通过 InodeData 及 Inode 接口，Inode 实现了对底层存储细节的抽象。

Inode 包含的关键属性有：ino（文件的唯一标识符）、size 和 nlink（文件大小及硬链接数量）、uid 和 gid（文件拥有者用户 ID 及组 ID）、mode（文件权限及类型）、atime、ctime 和 mtime（访问、创建及修改时间），以及 rwsem（读写信号量，用于并发访问控制）。

Inode Trait 定义了所有文件和目录必须实现的通用操作接口。其中，许多方法提供了默认实现，这些默认实现通常返回权限错误（EPERM）或目录类型错误（ENOTDIR / EISDIR），表明这些操作可能不适用于所有类型的 Inode 或需要具体的底层文件系统实现。
```rust
pub trait Inode: Send + Sync + InodeInner + Any {
    fn is_dir(&self) -> bool;
    fn lookup(&self, dentry: &Arc<Dentry>) -> KResult<Option<Arc<dyn Inode>>>;
    fn creat(&self, at: &Arc<Dentry>, mode: Mode) -> KResult<()>;
    fn mkdir(&self, at: &Dentry, mode: Mode) -> KResult<()>;
    fn mknod(&self, at: &Dentry, mode: Mode, dev: DevId) -> KResult<()>;
    fn unlink(&self, at: &Arc<Dentry>) -> KResult<()>;
    fn symlink(&self, at: &Arc<Dentry>, target: &[u8]) -> KResult<()>;
    fn read(&self, buffer: &mut dyn Buffer, offset: usize) -> KResult<usize>;
    fn read_direct(&self, buffer: &mut dyn Buffer, offset: usize) -> KResult<usize>;
    fn write(&self, stream: &mut dyn Stream, offset: WriteOffset) -> KResult<usize>;
    fn write_direct(&self, stream: &mut dyn Stream, offset: WriteOffset) -> KResult<usize>;
    fn devid(&self) -> KResult<DevId>;
    fn readlink(&self, buffer: &mut dyn Buffer) -> KResult<usize>;
    fn truncate(&self, length: usize) -> KResult<()>;
    fn rename(&self, rename_data: RenameData) -> KResult<()>;
    fn do_readdir(
        &self,
        offset: usize,
        callback: &mut dyn FnMut(&[u8], Ino) -> KResult<ControlFlow<(), ()>>,
    ) -> KResult<usize>;
    fn chmod(&self, mode: Mode) -> KResult<()>;
    fn chown(&self, uid: u32, gid: u32) -> KResult<()>;
    fn page_cache(&self) -> Option<&PageCache>;
    fn statx(&self, stat: &mut StatX, mask: u32) -> KResult<()>;
    fn new_locked<F>(ino: Ino, vfs: Weak<dyn Vfs>, f: F) -> Arc<Self>
    where
        Self: Sized,
        F: FnOnce(*mut Self, &()),
    ;
}
```

其核心功能接口涵盖：
* 文件/目录类型判定：is_dir。
* 目录管理操作：lookup（查找子项）、creat（创建文件）、mkdir（创建目录）、mknod（创建特殊文件）、unlink（删除链接）、symlink（创建符号链接）、rename（重命名/移动），do_readdir（读取目录内容）。
* 文件内容操作：read、read_direct、write、write_direct（直接读写文件内容），truncate（截断文件）。
* 设备及链接操作：devid（获取设备ID），readlink（读取符号链接目标）。
* 元数据修改：chmod（修改权限），chown（修改所有者）。
* 缓存与统计：page_cache（获取关联的 Page Cache），statx（获取详细统计信息）。
* Inode 生命周期管理：new_locked（创建新的 Inode 实例）。

在并发控制方面，Eonix 采用原子操作及信号量机制，确保 Inode 在多线程环境下的安全访问。例如，使用 Spin 锁保护关键数据结构，并通过 RwSemaphore 对并发读写操作进行细粒度控制，以维护数据一致性。

---

## 3. Dentry：用于路径解析与文件缓存

`Dentry` 是文件路径与 `Inode` 的映射关系，主要用于路径解析和目录缓存。

**基本结构：**
- `name` 和 `parent`：当前路径组件名称及其父目录的 `Dentry`。
- `data`：指向文件或目录的 `Inode`。
- `hash`：通过路径计算的哈希值，用于高效查找。
- 链表指针（`prev` 和 `next`）：支持 `DentryCache` 中的快速访问。

**功能接口：**
- `find` 和 `open_recursive` 支持路径解析，递归查找目录结构。
- `save_data` 和 `get_inode` 关联或获取 `Inode`。
- 提供对文件的操作接口，包括 `mkdir`、`unlink`、`readlink` 和 `write`。
- 支持符号链接解析，通过 `resolve_directory` 实现递归解析。
- 提供 `is_directory` 和 `is_valid` 方法检查当前节点是否为目录及其是否有效。

---

## 4. DentryCache：加速文件路径解析

`DentryCache` 是路径解析的高速缓存，通过维护 `Dentry` 的哈希表加速文件和目录的查找。

**缓存结构**方面，我们使用定长哈希表（`DCACHE`）存储不同哈希值对应的 `Dentry` 列表。提供静态根目录（`DROOT`），便于路径解析的起点管理。

**缓存一致性**方面，使用 RCU（Read-Copy-Update）机制维护多线程环境下的缓存一致性，确保高效的并发访问。

**核心操作：**
- `d_add`：将新的 `Dentry` 加入缓存。
- `d_find_fast` 和 `d_iter_for`：根据哈希值快速查找缓存中的 `Dentry`。
- `d_replace` 和 `d_remove`：支持缓存中节点的替换和移除操作。
- `d_try_revalidate`：通过父目录的 `lookup` 方法验证节点有效性。

---

## 5. Inode、Dentry 和 DentryCache 的协作

### 路径解析

用户请求文件路径时，内核通过 `Dentry` 递归解析路径。若缓存命中，`DentryCache` 提供快速访问；若缓存未命中，调用父目录的 `lookup` 方法查找对应的 `Inode`。

### 文件操作
文件操作首先通过 `Dentry` 获取 `Inode`，然后调用 `Inode` 提供的读写、创建等接口完成操作。在操作结束后，通过 `DentryCache` 缓存结果，加速后续访问。

### 并发访问
- `Inode` 的读写信号量（`rwsem`）和 `Dentry` 的 RCU 机制共同保障文件系统的并发访问安全。

---

## 6. Page Cache：优化数据读写性能
Page Cache 是 Eonix 文件系统中用于缓存磁盘数据的重要组件，它通过在内存中缓冲文件数据页面，显著减少了对物理磁盘的访问次数，从而大幅提升了文件读写性能。

其工作原理是：应用程序读取文件时，系统首先检查 Page Cache。如果缓存命中，数据将直接从内存返回；如果缓存未命中，则从磁盘读取并加载至 Page Cache。写入操作亦然，数据首先写入 Page Cache，随后异步或在特定时机写入磁盘，以减少应用程序等待时间。

Page Cache 与 Inode 紧密关联。每个 Inode 都会维护与其文件数据相关的缓存页面列表，并在 read 或 write 操作被调用时，协调与 Page Cache 的交互，确保数据被正确地读入或写出缓存。

---

## Eonix 文件系统组件协作概览
Eonix 文件系统的核心组件——VFS 层、Inode、Dentry、DentryCache 及 Page Cache——紧密协作，共同提供了高效、可靠且高性能的文件管理及数据访问能力。

* 统一入口与分发：所有文件系统操作均通过 VFS 层的统一接口进入。VFS 会根据路径信息及挂载点，将请求透明地分发至对应的具体文件系统驱动。

* 路径解析：文件路径请求抵达时，VFS 引导 Dentry 进行递归解析。DentryCache 通过快速查找缓存中的 Dentry 显著加速解析。若缓存命中，直接获取 Inode；若未命中，具体文件系统将查找并创建新的 Inode 及 Dentry，并添加至缓存。

* 文件操作与数据缓存：获取 Inode 后，文件操作通过 Inode 接口完成。Page Cache 作为数据在内存与磁盘之间传输的缓冲区，优化了数据读写效率。操作完成后，DentryCache 也会缓存相关 Dentry。

* 并发访问与数据一致性：Inode 的 rwsem 确保元数据及内容并发访问安全；Dentry 的 RCU 机制保障 DentryCache 高并发读操作及一致性；Page Cache 配合锁机制维护内部数据一致性。

这些分层、协作设计使 Eonix 文件系统能够支持多种文件系统类型，提供快速路径解析，有效优化数据读写性能。
