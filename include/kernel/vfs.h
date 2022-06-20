#pragma once

#include <types/types.h>

#ifdef __cplusplus
extern "C" {
#endif

struct inode;
struct inode_flags;
struct inode_ops;
struct dirent;

typedef size_t (*inode_read)(struct inode* file, char* buf, size_t buf_size, size_t offset, size_t n);
typedef size_t (*inode_write)(struct inode* file, const char* buf, size_t offset, size_t n);
typedef int (*inode_readdir)(struct inode* dir, struct dirent* entry, size_t i);
typedef int (*inode_finddir)(struct inode* dir, struct dirent* entry, const char* filename);
typedef int (*inode_mkfile)(struct inode* dir, const char* filename);
typedef int (*inode_rmfile)(struct inode* dir, const char* filename);
typedef int (*inode_mkdir)(struct inode* dir, const char* dirname);

struct inode_flags {
    uint32_t file : 1;
    uint32_t directory : 1;
    uint32_t mount_point : 1;
};

struct inode_ops {
    inode_read read;
    inode_write write;
    inode_readdir readdir;
    inode_finddir finddir;
    inode_mkfile mkfile;
    inode_rmfile rmfile;
    inode_mkdir mkdir;
};

struct inode {
    struct inode_flags flags;
    uint32_t perm;
    uint32_t impl;
    uint32_t ino;
    const struct inode_ops* ops;
};

struct dirent {
    char name[128];
    uint32_t ino;
};

extern struct inode fs_root;

void init_vfs(void);

size_t vfs_read(struct inode* file, char* buf, size_t buf_size, size_t offset, size_t n);
size_t vfs_write(struct inode* file, const char* buf, size_t offset, size_t n);
int vfs_readdir(struct inode* dir, struct dirent* entry, size_t i);
int vfs_finddir(struct inode* dir, struct dirent* entry, const char* filename);
int vfs_mkfile(struct inode* dir, const char* filename);
int vfs_rmfile(struct inode* dir, const char* filename);
int vfs_mkdir(struct inode* dir, const char* dirname);

#ifdef __cplusplus
}
#endif
