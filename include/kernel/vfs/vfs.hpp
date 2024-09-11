#pragma once

#include <functional>
#include <map>

#include <stdint.h>
#include <sys/stat.h>
#include <sys/types.h>

#include <kernel/vfs/dentry.hpp>
#include <kernel/vfs/inode.hpp>

namespace fs {

class vfs {
   public:
    using filldir_func = std::function<ssize_t(const char*, inode*, uint8_t)>;

   private:
    struct dcache m_dcache;
    struct dentry* m_root{};
    std::map<ino_t, inode> m_inodes;

   protected:
    dev_t m_device;
    size_t m_io_blksize;

   protected:
    vfs(dev_t device, size_t io_blksize);

    inode* alloc_inode(ino_t ino);

    void free_inode(ino_t ino);
    inode* get_inode(ino_t ino);
    void register_root_node(inode* root);

   public:
    static std::pair<vfs*, int> create(const char* source, const char* fstype,
                                       unsigned long flags, const void* data);

    vfs(const vfs&) = delete;
    vfs& operator=(const vfs&) = delete;
    vfs(vfs&&) = delete;
    vfs& operator=(vfs&&) = delete;

    struct dentry* root() const noexcept;
    dev_t fs_device() const noexcept;
    size_t io_blksize() const noexcept;

    int mount(dentry* mnt, const char* source, const char* mount_point,
              const char* fstype, unsigned long flags, const void* data);

    // directory operations
    virtual int creat(struct inode* dir, dentry* at, mode_t mode);
    virtual int mkdir(struct inode* dir, dentry* at, mode_t mode);
    virtual int mknod(struct inode* dir, dentry* at, mode_t mode, dev_t device);
    virtual int unlink(struct inode* dir, dentry* at);

    virtual int symlink(struct inode* dir, dentry* at, const char* target);

    // metadata operations
    int statx(inode* ind, struct statx* st, unsigned int mask);

    // file operations
    virtual ssize_t read(inode* file, char* buf, size_t buf_size, size_t count,
                         off_t offset);
    virtual ssize_t write(inode* file, const char* buf, size_t count,
                          off_t offset);

    virtual dev_t i_device(inode* ind);
    virtual int readlink(inode* file, char* buf, size_t buf_size);
    virtual int truncate(inode* file, size_t size);

    // directory operations

    // parameter 'length' in callback:
    // if 0, 'name' should be null terminated
    // else, 'name' size
    //
    // @return
    // return -1 if an error occurred
    // return 0 if no more entry available
    // otherwise, return bytes to be added to the offset
    virtual ssize_t readdir(inode* dir, size_t offset,
                            const filldir_func& callback) = 0;
};

} // namespace fs
