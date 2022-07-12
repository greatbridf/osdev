#pragma once

#include <types/hash_map.hpp>
#include <types/list.hpp>
#include <types/stdint.h>
#include <types/types.h>

#define INODE_FILE (1 << 0)
#define INODE_DIR (1 << 1)
#define INODE_MNT (1 << 2)
#define INODE_NODE (1 << 3)

namespace fs {
using ino_t = size_t;
using blksize_t = size_t;
using blkcnt_t = size_t;

class vfs;

union inode_flags {
    uint32_t v;
    struct {
        uint32_t file : 1;
        uint32_t directory : 1;
        uint32_t mount_point : 1;
        uint32_t special_node : 1;
    } in;
};

struct inode {
    inode_flags flags;
    uint32_t perm;
    void* impl;
    ino_t ino;
    vfs* fs;
};

struct dirent {
    char name[128];
    uint32_t ino;
};

union node_t {
    uint32_t v;
    struct {
        uint32_t major : 16;
        uint32_t minor : 16;
    } in;
};

struct special_node;

typedef size_t (*special_node_read)(special_node* sn, char* buf, size_t buf_size, size_t offset, size_t n);
typedef size_t (*special_node_write)(special_node* sn, const char* buf, size_t offset, size_t n);

struct special_node_ops {
    special_node_read read;
    special_node_write write;
};

struct special_node {
    special_node_ops ops;
    uint32_t data1;
    uint32_t data2;
};

struct stat {
    ino_t st_ino;
    node_t st_rdev;
    blksize_t st_blksize;
    blkcnt_t st_blocks;
};

class vfs {
private:
    // TODO: use allocator designed for small objects
    using inode_list = types::list<inode>;
    using inode_index_cache_list = types::hash_map<ino_t, inode*, types::linux_hasher<ino_t>>;

private:
    inode_list _inodes;
    inode* _root_inode;
    ino_t _last_inode_no;
    inode_index_cache_list _idx_inodes;

private:
    ino_t _assign_inode_id(void);

protected:
    inode* cache_inode(inode_flags flags, uint32_t perm, void* impl_data);
    inode* get_inode(ino_t ino);
    void register_root_node(inode* root);

public:
    explicit vfs(void);
    vfs(const vfs&) = delete;
    vfs& operator=(const vfs&) = delete;
    vfs(vfs&&) = delete;
    vfs& operator=(vfs&&) = delete;

    inode* root(void) const;

    virtual size_t inode_read(inode* file, char* buf, size_t buf_size, size_t offset, size_t n);
    virtual size_t inode_write(inode* file, const char* buf, size_t offset, size_t n);
    virtual int inode_readdir(inode* dir, dirent* entry, size_t i);
    virtual int inode_mkfile(inode* dir, const char* filename);
    virtual int inode_mknode(inode* dir, const char* filename, union node_t sn);
    virtual int inode_rmfile(inode* dir, const char* filename);
    virtual int inode_mkdir(inode* dir, const char* dirname);
    virtual int inode_stat(inode* dir, stat* stat, const char* dirname);
    // requires inode_readdir to work
    virtual inode* inode_findinode(inode* dir, const char* filename);
};

extern struct inode* fs_root;

void register_special_block(uint16_t major,
    uint16_t minor,
    special_node_read read,
    special_node_write write,
    uint32_t data1,
    uint32_t data2);

size_t vfs_read(inode* file, char* buf, size_t buf_size, size_t offset, size_t n);
size_t vfs_write(inode* file, const char* buf, size_t offset, size_t n);
int vfs_readdir(inode* dir, dirent* entry, size_t i);
inode* vfs_findinode(inode* dir, const char* filename);
int vfs_mkfile(inode* dir, const char* filename);
int vfs_mknode(inode* dir, const char* filename, node_t sn);
int vfs_rmfile(inode* dir, const char* filename);
int vfs_mkdir(inode* dir, const char* dirname);

// requires inode_findinode to work
// @return pointer to the inode if found, nullptr if not
inode* vfs_open(const char* path);

// @return GB_OK if succeed, GB_FAILED if failed and set errno
int vfs_stat(struct stat* stat, const char* path);

} // namespace fs

extern "C" void init_vfs(void);
