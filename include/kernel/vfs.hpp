#pragma once

#include <types/allocator.hpp>
#include <types/hash_map.hpp>
#include <types/list.hpp>
#include <types/stdint.h>
#include <types/types.h>
#include <types/vector.hpp>

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
    size_t size;
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
    size_t st_size;
    blksize_t st_blksize;
    blkcnt_t st_blocks;
};

class vfs {
public:
    struct dentry {
    public:
        using name_type = types::string<>;

    private:
        types::list<dentry> children;
        types::hash_map<name_type, dentry*, types::string_hasher<const name_type&>> idx_children;

    public:
        dentry* parent;
        inode* ind;
        // if the entry is not a file, this flag is ignored
        union {
            uint32_t v;
            struct {
                uint32_t present : 1;
                uint32_t dirty : 1;
            } in;
        } flags;
        name_type name;

        explicit dentry(dentry* parent, inode* ind, const name_type& name);
        explicit dentry(dentry* parent, inode* ind, name_type&& name);
        dentry(const dentry& val) = delete;
        dentry(dentry&& val);

        dentry& operator=(const dentry& val) = delete;
        dentry& operator=(dentry&& val) = delete;

        dentry* append(inode* ind, const name_type& name);
        dentry* append(inode* ind, name_type&& name);

        dentry* find(const name_type& name);

        dentry* replace(dentry* val);

        void invalidate(void);
    };

private:
    // TODO: use allocator designed for small objects
    using inode_list = types::list<inode>;
    using inode_index_cache_list = types::hash_map<ino_t, inode*, types::linux_hasher<ino_t>>;

private:
    inode_list _inodes;
    inode_index_cache_list _idx_inodes;
    types::hash_map<dentry*, dentry*, types::linux_hasher<dentry*>> _mount_recover_list;
    ino_t _last_inode_no;

private:
    ino_t _assign_inode_id(void);

protected:
    dentry _root;

protected:
    inode* cache_inode(inode_flags flags, uint32_t perm, size_t size, void* impl_data);
    inode* get_inode(ino_t ino);
    void register_root_node(inode* root);

    virtual int load_dentry(dentry* ent);

public:
    explicit vfs(void);
    vfs(const vfs&) = delete;
    vfs& operator=(const vfs&) = delete;
    vfs(vfs&&) = delete;
    vfs& operator=(vfs&&) = delete;

    constexpr dentry* root(void)
    {
        return &_root;
    }

    int mount(dentry* mnt, vfs* new_fs);

    virtual size_t inode_read(inode* file, char* buf, size_t buf_size, size_t offset, size_t n);
    virtual size_t inode_write(inode* file, const char* buf, size_t offset, size_t n);
    virtual int inode_mkfile(dentry* dir, const char* filename);
    virtual int inode_mknode(dentry* dir, const char* filename, union node_t sn);
    virtual int inode_rmfile(dentry* dir, const char* filename);
    virtual int inode_mkdir(dentry* dir, const char* dirname);
    virtual int inode_stat(dentry* dir, stat* stat);
};

extern fs::vfs::dentry* fs_root;

void register_special_block(uint16_t major,
    uint16_t minor,
    special_node_read read,
    special_node_write write,
    uint32_t data1,
    uint32_t data2);

vfs* register_fs(vfs* fs);

size_t vfs_read(inode* file, char* buf, size_t buf_size, size_t offset, size_t n);
size_t vfs_write(inode* file, const char* buf, size_t offset, size_t n);
int vfs_mkfile(fs::vfs::dentry* dir, const char* filename);
int vfs_mknode(fs::vfs::dentry* dir, const char* filename, node_t sn);
int vfs_rmfile(fs::vfs::dentry* dir, const char* filename);
int vfs_mkdir(fs::vfs::dentry* dir, const char* dirname);
int vfs_stat(const char* filename, stat* stat);
int vfs_stat(fs::vfs::dentry* ent, stat* stat);

// @return pointer to the dentry if found, nullptr if not
fs::vfs::dentry* vfs_open(const char* path);

} // namespace fs

extern "C" void init_vfs(void);
