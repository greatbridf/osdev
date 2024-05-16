#pragma once

#include <kernel/vfs/inode.hpp>
#include <kernel/vfs/dentry.hpp>

#include <functional>
#include <map>

#include <stdint.h>
#include <sys/types.h>
#include <sys/stat.h>

#include <types/hash_map.hpp>

namespace fs {

class vfs {
public:

public:
    using filldir_func = std::function<int(const char*, size_t, inode*, uint8_t)>;

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

    virtual int symlink(dentry* dir, const char* linkname, const char* target);

    // metadata operation

    virtual int inode_statx(dentry* dent, statx* buf, unsigned int mask);
    virtual int inode_stat(dentry* dent, struct stat* stat);

    // file operations

    virtual size_t read(inode* file, char* buf, size_t buf_size, size_t offset, size_t n);
    virtual size_t write(inode* file, const char* buf, size_t offset, size_t n);
    virtual int dev_id(inode* file, dev_t& out_dev);
    virtual int readlink(inode* file, char* buf, size_t buf_size);
    virtual int truncate(inode* file, size_t size);

    // parameter 'length' in callback:
    // if 0, 'name' should be null terminated
    // else, 'name' size
    //
    // @return
    // return -1 if an error occurred
    // return 0 if no more entry available
    // otherwise, return bytes to be added to the offset
    virtual int readdir(inode* dir, size_t offset, const filldir_func& callback) = 0;
};

} // namespace fs
