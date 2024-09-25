#pragma once

#include <bits/alltypes.h>
#include <stdint.h>
#include <sys/stat.h>
#include <sys/types.h>

#include <types/path.hpp>

#include <kernel/mem/paging.hpp>
#include <kernel/vfs/dentry.hpp>
#include <kernel/vfs/file.hpp>

#define NODE_MAJOR(node) (((node) >> 8) & 0xFFU)
#define NODE_MINOR(node) ((node) & 0xFFU)

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

int register_block_device(dev_t node, const blkdev_ops& ops);
int register_char_device(dev_t node, const chrdev_ops& ops);

void partprobe();

ssize_t block_device_read(dev_t node, char* buf, size_t buf_size, size_t offset,
                          size_t n);
ssize_t block_device_write(dev_t node, const char* buf, size_t offset,
                           size_t n);

ssize_t char_device_read(dev_t node, char* buf, size_t buf_size, size_t n);
ssize_t char_device_write(dev_t node, const char* buf, size_t n);

extern "C" int fs_creat(struct dentry* at, mode_t mode);
extern "C" int fs_mkdir(struct dentry* at, mode_t mode);
extern "C" int fs_mknod(struct dentry* at, mode_t mode, dev_t sn);
extern "C" int fs_unlink(struct dentry* at);
extern "C" int fs_symlink(struct dentry* at, const char* target);

extern "C" int fs_statx(const struct rust_inode_handle* inode,
                        struct statx* stat, unsigned int mask);
extern "C" int fs_readlink(const struct rust_inode_handle* inode, char* buf,
                           size_t buf_size);
extern "C" int fs_truncate(const struct rust_inode_handle* file, size_t size);
extern "C" size_t fs_read(const struct rust_inode_handle* file, char* buf,
                          size_t buf_size, size_t offset, size_t n);
extern "C" size_t fs_write(const struct rust_inode_handle* file,
                           const char* buf, size_t offset, size_t n);

using readdir_callback_fn =
    std::function<int(const char*, size_t, const struct rust_inode_handle*,
                      const struct inode_data*, uint8_t)>;

extern "C" ssize_t fs_readdir(const struct rust_inode_handle* file,
                              size_t offset,
                              const readdir_callback_fn* callback);

extern "C" int fs_mount(dentry* mnt, const char* source,
                        const char* mount_point, const char* fstype,
                        unsigned long flags, const void* data);

extern "C" struct dentry* r_get_mountpoint(struct dentry* mnt);
extern "C" mode_t r_dentry_save_inode(struct dentry* dent,
                                      const struct rust_inode_handle* inode);
extern "C" mode_t r_get_inode_mode(const struct rust_inode_handle* inode);
extern "C" size_t r_get_inode_size(const struct rust_inode_handle* inode);
extern "C" struct dentry* r_get_root_dentry();

#define current_open(...)                                             \
    fs::open(current_process->fs_context, current_process->cwd.get(), \
             __VA_ARGS__)
std::pair<dentry_pointer, int> open(const fs_context& context, dentry* cwd,
                                    types::path_iterator path,
                                    bool follow_symlinks = true,
                                    int recurs_no = 0);

} // namespace fs
