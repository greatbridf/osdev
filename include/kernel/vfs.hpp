#pragma once

#include <map>
#include <list>
#include <vector>
#include <functional>

#include <sys/stat.h>
#include <sys/types.h>
#include <errno.h>
#include <bits/alltypes.h>

#include <assert.h>
#include <kernel/event/evtqueue.hpp>
#include <stdint.h>
#include <sys/types.h>
#include <types/allocator.hpp>
#include <types/buffer.hpp>
#include <types/cplusplus.hpp>
#include <types/hash_map.hpp>
#include <types/path.hpp>
#include <types/lock.hpp>
#include <types/types.h>

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
using blksize_t = size_t;
using blkcnt_t = size_t;

class vfs;

struct inode {
    ino_t ino;
    vfs* fs;
    size_t size;

    mode_t mode;
    uid_t uid;
    gid_t gid;
};

#define NODE_MAJOR(node) ((node) >> 16)
#define NODE_MINOR(node) ((node) & 0xffff)
constexpr dev_t NODE_INVALID = -1U;

constexpr dev_t make_device(uint32_t major, uint32_t minor)
{
    return (major << 16) | (minor & 0xffff);
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

class vfs {
public:
    struct dentry {
    public:
        using name_type = types::string<>;
        template <typename T>
        using allocator_type = types::kernel_allocator<T>;

    private:
        std::list<dentry, types::allocator_adapter<dentry, allocator_type>>* children = nullptr;
        types::hash_map<name_type, dentry*, types::linux_hasher, allocator_type>* idx_children = nullptr;

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

        explicit dentry(dentry* parent, inode* ind, name_type name);
        dentry(const dentry& val) = delete;
        constexpr dentry(dentry&& val)
            : children(std::exchange(val.children, nullptr))
            , idx_children(std::exchange(val.idx_children, nullptr))
            , parent(std::exchange(val.parent, nullptr))
            , ind(std::exchange(val.ind, nullptr))
            , flags { val.flags }
            , name(std::move(val.name))
        {
            if (children) {
                for (auto& item : *children)
                    item.parent = this;
            }
        }

        dentry& operator=(const dentry& val) = delete;
        dentry& operator=(dentry&& val) = delete;

        constexpr ~dentry()
        {
            if (children) {
                types::pdelete<allocator_type>(children);
                children = nullptr;
            }
            if (idx_children) {
                types::pdelete<allocator_type>(idx_children);
                idx_children = nullptr;
            }
        }

        dentry* append(inode* ind, const name_type& name, bool set_dirty);
        dentry* append(inode* ind, name_type&& name, bool set_dirty);

        dentry* find(const name_type& name);

        dentry* replace(dentry* val);

        // out_dst SHOULD be empty
        void path(const dentry& root, types::path& out_dst) const;

        void invalidate(void);
    };

public:
    using filldir_func = std::function<int(const char*, size_t, ino_t, uint8_t)>;

private:
    // TODO: use allocator designed for small objects
    using inode_list = std::map<ino_t, inode>;

private:
    inode_list _inodes;
    types::hash_map<dentry*, dentry*> _mount_recover_list;

protected:
    dentry _root;

protected:
    inode* cache_inode(size_t size, ino_t ino, mode_t mode, uid_t uid, gid_t gid);
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
    virtual int inode_mkfile(dentry* dir, const char* filename, mode_t mode);
    virtual int inode_mknode(dentry* dir, const char* filename, mode_t mode, dev_t sn);
    virtual int inode_rmfile(dentry* dir, const char* filename);
    virtual int inode_mkdir(dentry* dir, const char* dirname);
    virtual int inode_statx(dentry* dent, statx* buf, unsigned int mask);
    virtual int inode_stat(dentry* dent, struct stat* stat);
    virtual dev_t inode_devid(inode* file);

    // parameter 'length' in callback:
    // if 0, 'name' should be null terminated
    // else, 'name' size
    //
    // @return
    // return -1 if an error occurred
    // return 0 if no more entry available
    // otherwise, return bytes to be added to the offset
    virtual int inode_readdir(inode* dir, size_t offset, const filldir_func& callback) = 0;
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
    mode_t mode; // stores the file type in the same format as inode::mode
    vfs::dentry* parent {};
    struct file_flags {
        uint32_t read : 1;
        uint32_t write : 1;
        uint32_t close_on_exec : 1;
    } flags {};

    file(mode_t mode, vfs::dentry* parent, file_flags flags)
        : mode(mode) , parent(parent), flags(flags) { }

    virtual ~file() = default;

    virtual ssize_t read(char* __user buf, size_t n) = 0;
    virtual ssize_t write(const char* __user buf, size_t n) = 0;

    // regular files should override this method
    virtual int getdents(char* __user buf, size_t cnt)
    { return (void)buf, (void)cnt, -ENOTDIR; }
    virtual int getdents64(char* __user buf, size_t cnt)
    { return (void)buf, (void)cnt, -ENOTDIR; }
};

struct regular_file : public virtual file {
    virtual ~regular_file() = default;
    std::size_t cursor { };
    inode* ind { };

    regular_file(vfs::dentry* parent, file_flags flags, size_t cursor, inode* ind);

    virtual ssize_t read(char* __user buf, size_t n) override;
    virtual ssize_t write(const char* __user buf, size_t n) override;
    virtual int getdents(char* __user buf, size_t cnt) override;
    virtual int getdents64(char* __user buf, size_t cnt) override;
};

struct fifo_file : public virtual file {
    virtual ~fifo_file() override;
    std::shared_ptr<pipe> ppipe;

    fifo_file(vfs::dentry* parent, file_flags flags, std::shared_ptr<fs::pipe> ppipe);

    virtual ssize_t read(char* __user buf, size_t n) override;
    virtual ssize_t write(const char* __user buf, size_t n) override;
};

inline fs::vfs::dentry* fs_root;

int register_block_device(dev_t node, blkdev_ops ops);
int register_char_device(dev_t node, chrdev_ops ops);

void partprobe();

ssize_t block_device_read(dev_t node, char* buf, size_t buf_size, size_t offset, size_t n);
ssize_t block_device_write(dev_t node, const char* buf, size_t offset, size_t n);

ssize_t char_device_read(dev_t node, char* buf, size_t buf_size, size_t n);
ssize_t char_device_write(dev_t node, const char* buf, size_t n);

vfs* register_fs(vfs* fs);

size_t vfs_read(inode* file, char* buf, size_t buf_size, size_t offset, size_t n);
size_t vfs_write(inode* file, const char* buf, size_t offset, size_t n);
int vfs_mkfile(fs::vfs::dentry* dir, const char* filename, mode_t mode);
int vfs_mknode(fs::vfs::dentry* dir, const char* filename, mode_t mode, dev_t sn);
int vfs_rmfile(fs::vfs::dentry* dir, const char* filename);
int vfs_mkdir(fs::vfs::dentry* dir, const char* dirname);
int vfs_stat(fs::vfs::dentry* dent, statx* stat, unsigned int mask);

/**
 * @brief Opens a file or directory specified by the given path.
 * 
 * @param root The root directory of the file system.
 * @param path The absolute path to the file or directory to be opened.
 * @return A pointer to the opened file or directory entry if found.
 *         Otherwise, nullptr is returned.
 */
fs::vfs::dentry* vfs_open(fs::vfs::dentry& root, const types::path& path);

} // namespace fs

extern "C" void init_vfs(void);
