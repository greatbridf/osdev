#pragma once

#include <assert.h>
#include <kernel/event/evtqueue.hpp>
#include <stdint.h>
#include <types/allocator.hpp>
#include <types/buffer.hpp>
#include <types/cplusplus.hpp>
#include <types/function.hpp>
#include <types/hash_map.hpp>
#include <types/list.hpp>
#include <types/lock.hpp>
#include <types/map.hpp>
#include <types/types.h>
#include <types/vector.hpp>

#define INODE_FILE (1 << 0)
#define INODE_DIR (1 << 1)
#define INODE_MNT (1 << 2)
#define INODE_NODE (1 << 3)

// dirent file types
#define DT_UNKNOWN 0
#define DT_FIFO 1
#define DT_CHR 2
#define DT_DIR 4
#define DT_BLK 6
#define DT_REG 8
#define DT_LNK 10
#define DT_SOCK 12
#define DT_WHT 14

#define DT_MAX (S_DT_MASK + 1) /* 16 */

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
    ino_t ino;
    vfs* fs;
    size_t size;
};

#define SN_INVALID (0xffffffff)
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

struct PACKED user_dirent {
    ino_t d_ino; // inode number
    uint32_t d_off; // ignored
    uint16_t d_reclen; // length of this struct user_dirent
    char d_name[1]; // file name with a padding zero
    // uint8_t d_type; // file type, with offset of (d_reclen - 1)
};

class vfs {
public:
    struct dentry {
    public:
        using name_type = types::string<>;
        template <typename T>
        using allocator_type = types::kernel_allocator<T>;

    private:
        types::list<dentry, allocator_type>* children = nullptr;
        types::hash_map<name_type, dentry*, types::string_hasher<const name_type&>, allocator_type>* idx_children = nullptr;

    public:
        dentry* parent;
        inode* ind;
        // if the entry is a file, this flag is ignored
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

        ~dentry();

        dentry* append(inode* ind, const name_type& name, bool set_dirty);
        dentry* append(inode* ind, name_type&& name, bool set_dirty);

        dentry* find(const name_type& name);

        dentry* replace(dentry* val);

        void invalidate(void);
    };

public:
    using filldir_func = std::function<int(const char*, size_t, ino_t, uint8_t)>;

private:
    // TODO: use allocator designed for small objects
    using inode_list = types::map<ino_t, inode>;

private:
    inode_list _inodes;
    types::hash_map<dentry*, dentry*, types::linux_hasher<dentry*>> _mount_recover_list;

protected:
    dentry _root;

protected:
    inode* cache_inode(inode_flags flags, uint32_t perm, size_t size, ino_t ino);
    inode* get_inode(ino_t ino);
    void register_root_node(inode* root);

    int load_dentry(dentry* ent);

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
    virtual uint32_t inode_getnode(inode* file);

    // parameter 'length' in callback:
    // if 0, 'name' should be null terminated
    // else, 'name' size
    //
    // @return
    // return -1 if an error occurred
    // return 0 if no more entry available
    // otherwise, return bytes to be added to the offset
    virtual int inode_readdir(inode* dir, size_t offset, filldir_func callback) = 0;
};

class pipe : public types::non_copyable {
private:
    static constexpr size_t PIPE_SIZE = 4096;
    static constexpr uint32_t READABLE = 1;
    static constexpr uint32_t WRITABLE = 2;

private:
    types::buffer<types::kernel_allocator> buf;
    kernel::cond_var m_cv;
    uint32_t flags;

public:
    pipe(void);

    void close_read(void);
    void close_write(void);

    int write(const char* buf, size_t n);
    int read(char* buf, size_t n);

    constexpr bool is_readable(void) const
    {
        return flags & READABLE;
    }

    constexpr bool is_writeable(void) const
    {
        return flags & WRITABLE;
    }

    constexpr bool is_free(void) const
    {
        return !(flags & (READABLE | WRITABLE));
    }
};

struct file {
    enum class types {
        ind,
        pipe,
        socket,
    } type;
    union {
        inode* ind;
        pipe* pp;
    } ptr;
    vfs::dentry* parent;
    size_t cursor;
    size_t ref;
    struct file_flags {
        uint32_t read : 1;
        uint32_t write : 1;
    } flags;
};

inline fs::vfs::dentry* fs_root;

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
