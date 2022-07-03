#pragma once

#include "types/stdint.h"
#include <types/types.h>

#define INODE_FILE (1 << 0)
#define INODE_DIR (1 << 1)
#define INODE_MNT (1 << 2)
#define INODE_NODE (1 << 3)

#ifdef __cplusplus
extern "C" {
#endif

struct inode;
union inode_flags;
struct inode_ops;
struct stat;
struct dirent;
union special_node;
struct special_node_ops;

typedef size_t (*inode_read)(struct inode* file, char* buf, size_t buf_size, size_t offset, size_t n);
typedef size_t (*inode_write)(struct inode* file, const char* buf, size_t offset, size_t n);
typedef int (*inode_readdir)(struct inode* dir, struct dirent* entry, size_t i);
typedef struct inode* (*inode_findinode)(struct inode* dir, const char* filename);
typedef int (*inode_mkfile)(struct inode* dir, const char* filename);
typedef int (*inode_mknode)(struct inode* dir, const char* filename, union special_node sn);
typedef int (*inode_rmfile)(struct inode* dir, const char* filename);
typedef int (*inode_mkdir)(struct inode* dir, const char* dirname);
typedef int (*inode_stat)(struct inode* dir, struct stat* stat, const char* dirname);

typedef size_t (*special_node_read)(char* buf, size_t buf_size, size_t offset, size_t n);
typedef size_t (*special_node_write)(const char* buf, size_t offset, size_t n);

typedef uint32_t ino_t;
typedef uint32_t blksize_t;
typedef uint32_t blkcnt_t;

union inode_flags {
    uint32_t v;
    struct {
        uint32_t file : 1;
        uint32_t directory : 1;
        uint32_t mount_point : 1;
        uint32_t special_node : 1;
    } in;
};

struct inode_ops {
    inode_read read;
    inode_write write;
    inode_readdir readdir;
    inode_findinode findinode;
    inode_mkfile mkfile;
    inode_mknode mknode;
    inode_rmfile rmfile;
    inode_mkdir mkdir;
    inode_stat stat;
};

struct fs_info {
    const struct inode_ops* ops;
    void* impl;
};

struct inode {
    union inode_flags flags;
    uint32_t perm;
    void* impl;
    ino_t ino;
    struct fs_info* fs;
};

struct dirent {
    char name[128];
    uint32_t ino;
};

union special_node {
    uint32_t v;
    struct {
        uint32_t major : 16;
        uint32_t minor : 16;
    } in;
};

struct special_node_ops {
    special_node_read read;
    special_node_write write;
};

struct stat {
    ino_t st_ino;
    union special_node st_rdev;
    blksize_t st_blksize;
    blkcnt_t st_blocks;
};

extern struct inode* fs_root;

void init_vfs(void);

void register_special_block(uint16_t major, uint16_t minor, special_node_read read, special_node_write write);

size_t vfs_read(struct inode* file, char* buf, size_t buf_size, size_t offset, size_t n);
size_t vfs_write(struct inode* file, const char* buf, size_t offset, size_t n);
int vfs_readdir(struct inode* dir, struct dirent* entry, size_t i);
struct inode* vfs_findinode(struct inode* dir, const char* filename);
int vfs_mkfile(struct inode* dir, const char* filename);
int vfs_mknode(struct inode* dir, const char* filename, union special_node sn);
int vfs_rmfile(struct inode* dir, const char* filename);
int vfs_mkdir(struct inode* dir, const char* dirname);

// @return pointer to the inode if found, nullptr if not
struct inode* vfs_open(const char* path);

// @return GB_OK if succeed, GB_FAILED if failed and set errno
int vfs_stat(struct stat* stat, const char* path);

#ifdef __cplusplus
}
#endif
