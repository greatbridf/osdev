#include <errno.h>

#include <kernel/vfs.hpp>
#include <kernel/vfs/vfs.hpp>

using namespace fs;

vfs::vfs(dev_t device, size_t io_blksize)
    : m_root { nullptr, nullptr, "" }
    , m_device(device), m_io_blksize(io_blksize)
{
}

fs::inode* vfs::alloc_inode(ino_t ino)
{
    auto [iter, inserted] = m_inodes.try_emplace(ino);
    iter->second.ino = ino;
    iter->second.fs = this;

    assert(inserted);
    return &iter->second;
}

void vfs::free_inode(ino_t ino)
{
    int n = m_inodes.erase(ino);
    assert(n == 1);
}

fs::inode* vfs::get_inode(ino_t ino)
{
    auto iter = m_inodes.find(ino);
    // TODO: load inode from disk if not found
    if (iter)
        return &iter->second;
    else
        return nullptr;
}

void vfs::register_root_node(inode* root)
{
    if (!m_root.ind)
        m_root.ind = root;
}

int vfs::mount(dentry* mnt, const char* source, const char* mount_point,
        const char* fstype, unsigned long flags, const void *data)
{
    if (!mnt->flags.dir)
        return -ENOTDIR;

    vfs* new_fs;
    int ret = fs::create_fs(source, mount_point, fstype, flags, data, new_fs);

    if (ret != 0)
        return ret;

    auto* new_ent = new_fs->root();

    new_ent->parent = mnt->parent;
    new_ent->name = mnt->name;

    auto* orig_ent = mnt->replace(new_ent);
    m_mount_recover_list.emplace(new_ent, orig_ent);

    return 0;
}

// default behavior is to
// return -EINVAL to show that the operation
// is not supported by the fs

ssize_t vfs::read(inode*, char*, size_t, size_t, off_t)
{
    return -EINVAL;
}

ssize_t vfs::write(inode*, const char*, size_t, off_t)
{
    return -EINVAL;
}

int vfs::inode_mkfile(dentry*, const char*, mode_t)
{
    return -EINVAL;
}

int vfs::inode_mknode(dentry*, const char*, mode_t, dev_t)
{
    return -EINVAL;
}

int vfs::inode_rmfile(dentry*, const char*)
{
    return -EINVAL;
}

int vfs::inode_mkdir(dentry*, const char*, mode_t)
{
    return -EINVAL;
}

int vfs::symlink(dentry*, const char*, const char*)
{
    return -EINVAL;
}

int vfs::readlink(inode*, char*, size_t)
{
    return -EINVAL;
}

int vfs::truncate(inode*, size_t)
{
    return -EINVAL;
}

dev_t vfs::fs_device() const noexcept
{
    return m_device;
}

size_t vfs::io_blksize() const noexcept
{
    return m_io_blksize;
}
