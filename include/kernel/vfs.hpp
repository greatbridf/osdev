#pragma once

#include <kernel/vfs/dentry.hpp>
#include <kernel/vfs/file.hpp>
#include <kernel/vfs/inode.hpp>
#include <kernel/vfs/vfs.hpp>

#include <stdint.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <bits/alltypes.h>

#include <types/path.hpp>

#define NODE_MAJOR(node) (((node) >> 8) & 0xFFU)
#define NODE_MINOR(node) ((node) & 0xFFU)

namespace fs {

constexpr dev_t make_device(uint32_t major, uint32_t minor)
{
    return ((major << 8) & 0xFF00U) | (minor & 0xFFU);
}

// buf, buf_size, offset, cnt
using blkdev_read = std::function<ssize_t(char*, std::size_t, std::size_t, std::size_t)>;

// buf, offset, cnt
using blkdev_write = std::function<ssize_t(const char*, std::size_t, std::size_t)>;

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
    ino_t d_ino; // inode number
    uint32_t d_off; // ignored
    uint16_t d_reclen; // length of this struct user_dirent
    char d_name[1]; // file name with a padding zero
    // uint8_t d_type; // file type, with offset of (d_reclen - 1)
};

struct user_dirent64 {
    ino64_t d_ino; // inode number
    uint64_t d_off; // implementation-defined field, ignored
    uint16_t d_reclen; // length of this struct user_dirent
    uint8_t d_type; // file type, with offset of (d_reclen - 1)
    char d_name[1]; // file name with a padding zero
};

inline dentry* fs_root;

int register_block_device(dev_t node, const blkdev_ops& ops);
int register_char_device(dev_t node, const chrdev_ops& ops);

// return value: pointer to created vfs object
// 1. dev_t: device number
using create_fs_func_t = std::function<vfs*(dev_t)>;

int register_fs(const char* name, create_fs_func_t);

// in tmpfs.cc
int register_tmpfs();

// returns a pointer to the vfs object
// vfs objects are managed by the kernel
int create_fs(const char* name, dev_t device, vfs*& out_vfs);

void partprobe();

ssize_t block_device_read(dev_t node, char* buf, size_t buf_size, size_t offset, size_t n);
ssize_t block_device_write(dev_t node, const char* buf, size_t offset, size_t n);

ssize_t char_device_read(dev_t node, char* buf, size_t buf_size, size_t n);
ssize_t char_device_write(dev_t node, const char* buf, size_t n);

size_t vfs_read(inode* file, char* buf, size_t buf_size, size_t offset, size_t n);
size_t vfs_write(inode* file, const char* buf, size_t offset, size_t n);
int vfs_mkfile(dentry* dir, const char* filename, mode_t mode);
int vfs_mknode(dentry* dir, const char* filename, mode_t mode, dev_t sn);
int vfs_rmfile(dentry* dir, const char* filename);
int vfs_mkdir(dentry* dir, const char* dirname, mode_t mode);
int vfs_stat(dentry* dent, statx* stat, unsigned int mask);
int vfs_truncate(inode* file, size_t size);

/**
 * @brief Opens a file or directory specified by the given path.
 *
 * @param root The root directory of the file system.
 * @param path The absolute path to the file or directory to be opened.
 * @return A pointer to the opened file or directory entry if found.
 *         Otherwise, nullptr is returned.
 */
dentry* vfs_open(dentry& root, const types::path& path, bool follow_symlinks = true, int recurs_no = 0);

} // namespace fs

extern "C" void init_vfs(void);
