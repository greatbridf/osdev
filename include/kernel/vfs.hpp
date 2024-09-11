#pragma once

#include <memory>

#include <bits/alltypes.h>
#include <stdint.h>
#include <sys/stat.h>
#include <sys/types.h>

#include <types/path.hpp>

#include <kernel/vfs/dentry.hpp>
#include <kernel/vfs/file.hpp>
#include <kernel/vfs/inode.hpp>
#include <kernel/vfs/vfs.hpp>

#define NODE_MAJOR(node) (((node) >> 8) & 0xFFU)
#define NODE_MINOR(node) ((node)&0xFFU)

namespace fs {

constexpr dev_t make_device(uint32_t major, uint32_t minor) {
    return ((major << 8) & 0xFF00U) | (minor & 0xFFU);
}

// buf, buf_size, offset, cnt
using blkdev_read =
    std::function<ssize_t(char*, std::size_t, std::size_t, std::size_t)>;

// buf, offset, cnt
using blkdev_write =
    std::function<ssize_t(const char*, std::size_t, std::size_t)>;

struct blkdev_ops {
    blkdev_read read;
    blkdev_write write;
};

// buf, buf_size, cnt
using chrdev_read = std::function<ssize_t(char*, std::size_t, std::size_t)>;

// buf, cnt
using chrdev_write = std::function<ssize_t(const char*, std::size_t)>;

struct chrdev_ops {
    chrdev_read read;
    chrdev_write write;
};

struct PACKED user_dirent {
    ino_t d_ino;       // inode number
    uint32_t d_off;    // ignored
    uint16_t d_reclen; // length of this struct user_dirent
    char d_name[1];    // file name with a padding zero
    // uint8_t d_type; // file type, with offset of (d_reclen - 1)
};

struct PACKED user_dirent64 {
    ino64_t d_ino;     // inode number
    uint64_t d_off;    // implementation-defined field, ignored
    uint16_t d_reclen; // length of this struct user_dirent
    uint8_t d_type;    // file type, with offset of (d_reclen - 1)
    char d_name[1];    // file name with a padding zero
};

struct fs_context {
    dentry_pointer root;
};

struct mount_data {
    fs::vfs* fs;
    std::string source;
    std::string mount_point;
    std::string fstype;
    unsigned long flags;
};

inline std::map<struct dentry*, mount_data> mounts;

int register_block_device(dev_t node, const blkdev_ops& ops);
int register_char_device(dev_t node, const chrdev_ops& ops);

// return value: pointer to created vfs object
// 1. const char*: source, such as "/dev/sda" or "proc"
// 2. unsigned long: flags, such as MS_RDONLY | MS_RELATIME
// 3. const void*: data, for filesystem use, such as "uid=1000"
using create_fs_func_t =
    std::function<vfs*(const char*, unsigned long, const void*)>;

int register_fs(const char* name, create_fs_func_t);

// in tmpfs.cc
int register_tmpfs();

void partprobe();

ssize_t block_device_read(dev_t node, char* buf, size_t buf_size, size_t offset,
                          size_t n);
ssize_t block_device_write(dev_t node, const char* buf, size_t offset,
                           size_t n);

ssize_t char_device_read(dev_t node, char* buf, size_t buf_size, size_t n);
ssize_t char_device_write(dev_t node, const char* buf, size_t n);

int creat(struct dentry* at, mode_t mode);
int mkdir(struct dentry* at, mode_t mode);
int mknod(struct dentry* at, mode_t mode, dev_t sn);
int unlink(struct dentry* at);
int symlink(struct dentry* at, const char* target);

int statx(struct inode* inode, struct statx* stat, unsigned int mask);
int readlink(struct inode* inode, char* buf, size_t buf_size);
int truncate(struct inode* file, size_t size);
size_t read(struct inode* file, char* buf, size_t buf_size, size_t offset,
            size_t n);
size_t write(struct inode* file, const char* buf, size_t offset, size_t n);

int mount(dentry* mnt, const char* source, const char* mount_point,
          const char* fstype, unsigned long flags, const void* data);

#define current_open(...)                                             \
    fs::open(current_process->fs_context, current_process->cwd.get(), \
             __VA_ARGS__)
std::pair<dentry_pointer, int> open(const fs_context& context, dentry* cwd,
                                    types::path_iterator path,
                                    bool follow_symlinks = true,
                                    int recurs_no = 0);

} // namespace fs

extern "C" void init_vfs(void);
