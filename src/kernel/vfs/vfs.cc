#include <assert.h>
#include <errno.h>
#include <sys/mount.h>

#include <kernel/vfs.hpp>
#include <kernel/vfs/dentry.hpp>
#include <kernel/vfs/vfs.hpp>

using namespace fs;

static std::map<std::string, fs::create_fs_func_t> fs_list;

int fs::register_fs(const char* name, fs::create_fs_func_t func) {
    fs_list.emplace(name, func);

    return 0;
}

vfs::vfs(dev_t device, size_t io_blksize)
    : m_device(device), m_io_blksize(io_blksize) {
    dcache_init(&m_dcache, 8);
}

std::pair<vfs*, int> vfs::create(const char* source, const char* fstype,
                                 unsigned long flags, const void* data) {
    auto iter = fs_list.find(fstype);
    if (!iter)
        return {nullptr, -ENODEV};

    auto& [_, func] = *iter;

    if (!(flags & MS_NOATIME))
        flags |= MS_RELATIME;

    if (flags & MS_STRICTATIME)
        flags &= ~(MS_RELATIME | MS_NOATIME);

    return {func(source, flags, data), 0};
}

fs::inode* vfs::alloc_inode(ino_t ino) {
    auto [iter, inserted] = m_inodes.try_emplace(ino);
    iter->second.ino = ino;
    iter->second.fs = this;

    assert(inserted);
    return &iter->second;
}

void vfs::free_inode(ino_t ino) {
    int n = m_inodes.erase(ino);
    assert(n == 1);
}

fs::inode* vfs::get_inode(ino_t ino) {
    auto iter = m_inodes.find(ino);
    // TODO: load inode from disk if not found
    if (iter)
        return &iter->second;
    else
        return nullptr;
}

void vfs::register_root_node(struct inode* root_inode) {
    assert(!root());

    m_root = fs::dcache_alloc(&m_dcache);
    m_root->fs = this;

    m_root->inode = root_inode;
    m_root->flags = D_DIRECTORY | D_PRESENT;

    fs::dcache_init_root(&m_dcache, m_root);
}

int vfs::mount(dentry* mnt, const char* source, const char* mount_point,
               const char* fstype, unsigned long flags, const void* data) {
    if (!(mnt->flags & D_DIRECTORY))
        return -ENOTDIR;

    auto [new_fs, ret] = vfs::create(source, fstype, flags, data);
    if (ret != 0)
        return ret;

    mounts.emplace(d_get(mnt), mount_data{
                                   .fs = new_fs,
                                   .source = source,
                                   .mount_point = mount_point,
                                   .fstype = fstype,
                                   .flags = flags,
                               });
    mnt->flags |= D_MOUNTPOINT;

    auto* new_ent = new_fs->root();

    new_ent->parent = mnt->parent;
    new_ent->name = mnt->name;
    new_ent->hash = mnt->hash;

    return 0;
}

// default behavior is to
// return -EINVAL to show that the operation
// is not supported by the fs

ssize_t vfs::read(inode*, char*, size_t, size_t, off_t) {
    return -EINVAL;
}

ssize_t vfs::write(inode*, const char*, size_t, off_t) {
    return -EINVAL;
}

int vfs::creat(inode*, dentry*, mode_t) {
    return -EINVAL;
}

int vfs::mknod(inode*, dentry*, mode_t, dev_t) {
    return -EINVAL;
}

int vfs::unlink(inode*, dentry*) {
    return -EINVAL;
}

int vfs::mkdir(inode*, dentry*, mode_t) {
    return -EINVAL;
}

int vfs::symlink(inode*, dentry*, const char*) {
    return -EINVAL;
}

int vfs::readlink(inode*, char*, size_t) {
    return -EINVAL;
}

int vfs::truncate(inode*, size_t) {
    return -EINVAL;
}

struct dentry* vfs::root() const noexcept {
    return m_root;
}

dev_t vfs::fs_device() const noexcept {
    return m_device;
}

size_t vfs::io_blksize() const noexcept {
    return m_io_blksize;
}
