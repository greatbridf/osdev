# 文件系统

Eonix 内核的文件系统模块通过 `Inode`、`Dentry` 和 `DentryCache` 数据结构实现高效的文件管理、路径解析和缓存机制。它们共同构成了 Eonix 内核文件系统的核心数据结构，提供了高效的文件操作和路径解析能力。`Inode` 描述文件的元数据和操作接口，`Dentry` 连接路径和文件内容，而 `DentryCache` 则通过缓存机制大幅提升文件系统的性能。三者的协作确保了文件系统在复杂操作和高并发环境下的高效性和稳定性。

---

#### 1. Inode：文件和目录的核心描述

`Inode` 是 Eonix 文件系统中表示文件或目录的核心结构，提供了文件元数据的存储与访问。它通过 `InodeData` 和 `Inode` 接口实现，支持文件的基础操作。

**基本属性：**
- `ino`：文件的唯一标识符。
- `size` 和 `nlink`：表示文件大小和硬链接计数。
- `uid` 和 `gid`：文件的拥有者和组。
- `mode`：文件的权限和类型（如目录、常规文件等）。
- `atime`、`ctime` 和 `mtime`：文件的访问、创建和修改时间。
- `rwsem`：读写信号量，用于并发访问控制。

**功能接口：**
- 提供创建文件（`creat`）、创建目录（`mkdir`）、读写文件（`read` 和 `write`）等基本操作。
- 支持通过 `lookup` 方法查找子文件，`statx` 获取文件元数据，`truncate` 修改文件大小。
- 提供与设备相关的接口，如 `readlink` 解析符号链接，`mknod` 创建特殊文件。

**并发支持：**
- 使用原子操作和信号量确保多线程环境下的安全性。
- 通过 `Spin` 和 `RwSemaphore` 实现时间和读写操作的并发控制。

---

#### 2. Dentry：用于路径解析与文件缓存

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

#### 3. DentryCache：加速文件路径解析

`DentryCache` 是路径解析的高速缓存，通过维护 `Dentry` 的哈希表加速文件和目录的查找。

**缓存结构**方面，我们使用定长哈希表（`DCACHE`）存储不同哈希值对应的 `Dentry` 列表。提供静态根目录（`DROOT`），便于路径解析的起点管理。

**缓存一致性**方面，使用 RCU（Read-Copy-Update）机制维护多线程环境下的缓存一致性，确保高效的并发访问。

**核心操作：**
- `d_add`：将新的 `Dentry` 加入缓存。
- `d_find_fast` 和 `d_iter_for`：根据哈希值快速查找缓存中的 `Dentry`。
- `d_replace` 和 `d_remove`：支持缓存中节点的替换和移除操作。
- `d_try_revalidate`：通过父目录的 `lookup` 方法验证节点有效性。

---

### 4. Inode、Dentry 和 DentryCache 的协作

#### 路径解析

用户请求文件路径时，内核通过 `Dentry` 递归解析路径。若缓存命中，`DentryCache` 提供快速访问；若缓存未命中，调用父目录的 `lookup` 方法查找对应的 `Inode`。

#### 文件操作
文件操作首先通过 `Dentry` 获取 `Inode`，然后调用 `Inode` 提供的读写、创建等接口完成操作。在操作结束后，通过 `DentryCache` 缓存结果，加速后续访问。

#### 并发访问
- `Inode` 的读写信号量（`rwsem`）和 `Dentry` 的 RCU 机制共同保障文件系统的并发访问安全。
