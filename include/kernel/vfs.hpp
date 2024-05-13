#pragma once

#include <map>
#include <list>
#include <vector>
#include <functional>

#include <errno.h>
#include <fcntl.h>
#include <sys/stat.h>
#include <sys/types.h>
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
#include <types/string.hpp>

#define NODE_MAJOR(node) ((node) >> 16)
#define NODE_MINOR(node) ((node) & 0xffffU)

namespace fs {

class vfs;

struct inode {
    ino_t ino;
    vfs* fs;
    size_t size;

    nlink_t nlink;

    mode_t mode;
    uid_t uid;
    gid_t gid;
};

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

    private:
        std::list<dentry>* children = nullptr;
        types::hash_map<name_type, dentry*>* idx_children = nullptr;

public:
        dentry* parent;
        inode* ind;
        struct {
            uint32_t dir : 1; // whether the dentry is a directory.
            // if dir is 1, whether children contains valid data.
            // otherwise, ignored
            uint32_t present : 1;
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
                delete children;
                children = nullptr;
            }
            if (idx_children) {
                delete idx_children;
                idx_children = nullptr;
            }
        }

        dentry* append(inode* ind, name_type name);

        dentry* find(const name_type& name);

        dentry* replace(dentry* val);

        void remove(const name_type& name);

        // out_dst SHOULD be empty
        void path(const dentry& root, types::path& out_dst) const;
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
    void free_inode(ino_t ino);
    inode* get_inode(ino_t ino);
    void register_root_node(inode* root);

    int load_dentry(dentry* ent);

public:
    vfs();

    vfs(const vfs&) = delete;
    vfs& operator=(const vfs&) = delete;
    vfs(vfs&&) = delete;
    vfs& operator=(vfs&&) = delete;

    constexpr dentry* root(void)
    {
        return &_root;
    }

    int mount(dentry* mnt, vfs* new_fs);

    // directory operations

    virtual int inode_mkfile(dentry* dir, const char* filename, mode_t mode);
    virtual int inode_mknode(dentry* dir, const char* filename, mode_t mode, dev_t sn);
    virtual int inode_rmfile(dentry* dir, const char* filename);
    virtual int inode_mkdir(dentry* dir, const char* dirname, mode_t mode);

    // metadata operation

    virtual int inode_statx(dentry* dent, statx* buf, unsigned int mask);
    virtual int inode_stat(dentry* dent, struct stat* stat);

    // file operations

    virtual size_t read(inode* file, char* buf, size_t buf_size, size_t offset, size_t n);
    virtual size_t write(inode* file, const char* buf, size_t offset, size_t n);
    virtual int dev_id(inode* file, dev_t& out_dev);
    virtual int truncate(inode* file, size_t size);

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
    types::buffer buf;
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
        uint32_t append : 1;
    } flags {};

    file(mode_t mode, vfs::dentry* parent, file_flags flags)
        : mode(mode) , parent(parent), flags(flags) { }

    virtual ~file() = default;

    virtual ssize_t read(char* __user buf, size_t n) = 0;
    virtual ssize_t do_write(const char* __user buf, size_t n) = 0;

    virtual off_t seek(off_t n, int whence)
    { return (void)n, (void)whence, -ESPIPE; }

    ssize_t write(const char* __user buf, size_t n)
    {
        if (!flags.write)
            return -EBADF;

        if (flags.append) {
            seek(0, SEEK_END);
        }

        return do_write(buf, n);
    }

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
    virtual ssize_t do_write(const char* __user buf, size_t n) override;
    virtual off_t seek(off_t n, int whence) override;
    virtual int getdents(char* __user buf, size_t cnt) override;
    virtual int getdents64(char* __user buf, size_t cnt) override;
};

struct fifo_file : public virtual file {
    virtual ~fifo_file() override;
    std::shared_ptr<pipe> ppipe;

    fifo_file(vfs::dentry* parent, file_flags flags, std::shared_ptr<fs::pipe> ppipe);

    virtual ssize_t read(char* __user buf, size_t n) override;
    virtual ssize_t do_write(const char* __user buf, size_t n) override;
};

inline fs::vfs::dentry* fs_root;

int register_block_device(dev_t node, blkdev_ops ops);
int register_char_device(dev_t node, chrdev_ops ops);

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
int vfs_mkfile(fs::vfs::dentry* dir, const char* filename, mode_t mode);
int vfs_mknode(fs::vfs::dentry* dir, const char* filename, mode_t mode, dev_t sn);
int vfs_rmfile(fs::vfs::dentry* dir, const char* filename);
int vfs_mkdir(fs::vfs::dentry* dir, const char* dirname, mode_t mode);
int vfs_stat(fs::vfs::dentry* dent, statx* stat, unsigned int mask);
int vfs_truncate(inode* file, size_t size);

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
