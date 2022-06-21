#pragma once

#include <types/types.h>

#ifdef __cplusplus
extern "C" {
#endif

struct inode;
struct inode_flags;
struct inode_ops;
struct stat;
struct dirent;

typedef size_t (*inode_read)(struct inode* file, char* buf, size_t buf_size, size_t offset, size_t n);
typedef size_t (*inode_write)(struct inode* file, const char* buf, size_t offset, size_t n);
typedef int (*inode_readdir)(struct inode* dir, struct dirent* entry, size_t i);
typedef struct inode* (*inode_findinode)(struct inode* dir, const char* filename);
typedef int (*inode_mkfile)(struct inode* dir, const char* filename);
typedef int (*inode_rmfile)(struct inode* dir, const char* filename);
typedef int (*inode_mkdir)(struct inode* dir, const char* dirname);
typedef int (*inode_stat)(struct inode* dir, struct stat* stat, const char* dirname);

typedef uint32_t ino_t;
typedef uint32_t blksize_t;
typedef uint32_t blkcnt_t;

struct inode_flags {
    uint32_t file : 1;
    uint32_t directory : 1;
    uint32_t mount_point : 1;
};

struct inode_ops {
    inode_read read;
    inode_write write;
    inode_readdir readdir;
    inode_findinode findinode;
    inode_mkfile mkfile;
    inode_rmfile rmfile;
    inode_mkdir mkdir;
    inode_stat stat;
};

struct fs_info {
    const struct inode_ops* ops;
    void* impl;
};

struct inode {
    struct inode_flags flags;
    uint32_t perm;
    void* impl;
    ino_t ino;
    struct fs_info* fs;
};

struct stat {
    ino_t st_ino;
    blksize_t st_blksize;
    blkcnt_t st_blocks;
};

struct dirent {
    char name[128];
    uint32_t ino;
};

extern struct inode* fs_root;

void init_vfs(void);

size_t vfs_read(struct inode* file, char* buf, size_t buf_size, size_t offset, size_t n);
size_t vfs_write(struct inode* file, const char* buf, size_t offset, size_t n);
int vfs_readdir(struct inode* dir, struct dirent* entry, size_t i);
struct inode* vfs_findinode(struct inode* dir, const char* filename);
int vfs_mkfile(struct inode* dir, const char* filename);
int vfs_rmfile(struct inode* dir, const char* filename);
int vfs_mkdir(struct inode* dir, const char* dirname);

// @return pointer to the inode if found, nullptr if not
struct inode* vfs_open(const char* path);

// @return GB_OK if succeed, GB_FAILED if failed and set errno
int vfs_stat(struct stat* stat, const char* path);

#ifdef __cplusplus
}
#endif
