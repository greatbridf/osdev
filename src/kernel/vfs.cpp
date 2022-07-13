#include <kernel/errno.h>
#include <kernel/mem.h>
#include <kernel/stdio.h>
#include <kernel/syscall.hpp>
#include <kernel/tty.h>
#include <kernel/vfs.hpp>
#include <types/allocator.hpp>
#include <types/list.hpp>
#include <types/status.h>
#include <types/stdint.h>
#include <types/string.hpp>
#include <types/vector.hpp>

using types::allocator_traits;
using types::kernel_allocator;
using types::string;
using types::vector;

struct tmpfs_file_entry {
    size_t ino;
    char filename[128];
};

fs::vfs::dentry::dentry(dentry* _parent, inode* _ind, const name_type& _name)
    : parent(_parent)
    , ind(_ind)
    , flags { 0 }
    , name(_name)
{
}
fs::vfs::dentry::dentry(dentry* _parent, inode* _ind, name_type&& _name)
    : parent(_parent)
    , ind(_ind)
    , flags { 0 }
    , name(types::move(_name))
{
}
fs::vfs::dentry::dentry(dentry&& val)
    : children(types::move(val.children))
    , idx_children(types::move(val.idx_children))
    , parent(val.parent)
    , ind(val.ind)
    , flags { val.flags }
    , name(types::move(val.name))
{
    for (auto& item : children)
        item.parent = this;
}
fs::vfs::dentry* fs::vfs::dentry::append(inode* ind, const name_type& name)
{
    auto iter = children.emplace_back(this, ind, name);
    idx_children.insert(iter->name, iter.ptr());
    return iter.ptr();
}
fs::vfs::dentry* fs::vfs::dentry::append(inode* ind, name_type&& name)
{
    auto iter = children.emplace_back(this, ind, types::move(name));
    idx_children.insert(iter->name, iter.ptr());
    return iter.ptr();
}
fs::vfs::dentry* fs::vfs::dentry::find(const name_type& name)
{
    if (ind->flags.in.directory && !flags.in.present)
        ind->fs->load_dentry(this);

    auto iter = idx_children.find(name);
    if (!iter) {
        errno = ENOTFOUND;
        return nullptr;
    }

    return iter->value;
}
fs::vfs::dentry* fs::vfs::dentry::replace(dentry* val)
{
    // TODO: prevent the dirent to be swapped out of memory
    parent->idx_children.find(this->name)->value = val;
    return this;
}
void fs::vfs::dentry::invalidate(void)
{
    // TODO: write back
    flags.in.dirty = 0;
    children.clear();
    idx_children.clear();
    flags.in.present = 0;
}
fs::vfs::vfs(void)
    : _last_inode_no(0)
    , _root(nullptr, nullptr, "/")
{
}
fs::ino_t fs::vfs::_assign_inode_id(void)
{
    return ++_last_inode_no;
}
fs::inode* fs::vfs::cache_inode(inode_flags flags, uint32_t perm, size_t size, void* impl_data)
{
    auto iter = _inodes.emplace_back(inode { flags, perm, impl_data, _assign_inode_id(), this, size });
    _idx_inodes.insert(iter->ino, iter.ptr());
    return iter.ptr();
}
fs::inode* fs::vfs::get_inode(ino_t ino)
{
    auto iter = _idx_inodes.find(ino);
    // TODO: load inode from disk if not found
    if (!iter)
        return nullptr;
    else
        return iter->value;
}
void fs::vfs::register_root_node(inode* root)
{
    if (!_root.ind)
        _root.ind = root;
}
int fs::vfs::load_dentry(dentry*)
{
    syscall(0x03);
    return GB_FAILED;
}
int fs::vfs::mount(dentry* mnt, vfs* new_fs)
{
    if (!mnt->ind->flags.in.directory) {
        errno = ENOTDIR;
        return GB_FAILED;
    }

    auto* new_ent = new_fs->root();

    new_ent->parent = mnt->parent;
    new_ent->name = mnt->name;

    auto* orig_ent = mnt->replace(new_ent);
    _mount_recover_list.insert(new_ent, orig_ent);
    return GB_OK;
}
size_t fs::vfs::inode_read(inode*, char*, size_t, size_t, size_t)
{
    syscall(0x03);
    return 0xffffffff;
}
size_t fs::vfs::inode_write(inode*, const char*, size_t, size_t)
{
    syscall(0x03);
    return 0xffffffff;
}
int fs::vfs::inode_mkfile(dentry*, const char*)
{
    syscall(0x03);
    return GB_FAILED;
}
int fs::vfs::inode_mknode(dentry*, const char*, node_t)
{
    syscall(0x03);
    return GB_FAILED;
}
int fs::vfs::inode_rmfile(dentry*, const char*)
{
    syscall(0x03);
    return GB_FAILED;
}
int fs::vfs::inode_mkdir(dentry*, const char*)
{
    syscall(0x03);
    return GB_FAILED;
}
int fs::vfs::inode_stat(dentry*, stat*)
{
    syscall(0x03);
    return GB_FAILED;
}

class tmpfs : public virtual fs::vfs {
protected:
    inline vector<tmpfs_file_entry>* mk_fe_vector(void)
    {
        return allocator_traits<kernel_allocator<vector<tmpfs_file_entry>>>::allocate_and_construct();
    }

    inline vector<char>* mk_data_vector(void)
    {
        return allocator_traits<kernel_allocator<vector<char>>>::allocate_and_construct();
    }

    void mklink(fs::inode* dir, fs::inode* inode, const char* filename)
    {
        auto* fes = static_cast<vector<struct tmpfs_file_entry>*>(dir->impl);
        struct tmpfs_file_entry ent = {
            .ino = inode->ino,
            .filename = { 0 },
        };
        snprintf(ent.filename, sizeof(ent.filename), filename);
        fes->push_back(ent);
        dir->size += sizeof(tmpfs_file_entry);
    }

    virtual int load_dentry(dentry* ent) override
    {
        if (!ent->ind->flags.in.directory) {
            errno = ENOTDIR;
            return GB_FAILED;
        }

        auto& entries = *static_cast<vector<tmpfs_file_entry>*>(ent->ind->impl);
        for (const auto& entry : entries)
            ent->append(get_inode(entry.ino), entry.filename);

        ent->flags.in.present = 1;
        return GB_OK;
    }

public:
    explicit tmpfs(void)
    {
        auto& in = *cache_inode({ INODE_DIR | INODE_MNT }, 0777, 0, mk_fe_vector());

        mklink(&in, &in, ".");
        mklink(&in, &in, "..");

        register_root_node(&in);
    }

    virtual int inode_mkfile(dentry* dir, const char* filename) override
    {
        auto& file = *cache_inode({ .v = INODE_FILE }, 0777, 0, mk_data_vector());
        mklink(dir->ind, &file, filename);
        dir->invalidate();
        return GB_OK;
    }

    virtual int inode_mknode(dentry* dir, const char* filename, fs::node_t sn) override
    {
        auto& node = *cache_inode({ .v = INODE_NODE }, 0777, 0, (void*)sn.v);
        mklink(dir->ind, &node, filename);
        dir->invalidate();
        return GB_OK;
    }

    virtual int inode_mkdir(dentry* dir, const char* dirname) override
    {
        auto& new_dir = *cache_inode({ .v = INODE_DIR }, 0777, 0, mk_fe_vector());
        mklink(&new_dir, &new_dir, ".");

        mklink(dir->ind, &new_dir, dirname);
        mklink(&new_dir, dir->ind, "..");

        dir->invalidate();
        return GB_OK;
    }

    virtual size_t inode_read(fs::inode* file, char* buf, size_t buf_size, size_t offset, size_t n) override
    {
        if (file->flags.in.file != 1)
            return 0;

        auto* data = static_cast<vector<char>*>(file->impl);
        size_t fsize = data->size();

        if (offset + n > fsize)
            n = fsize - offset;

        if (buf_size < n) {
            n = buf_size;
        }

        memcpy(buf, data->data() + offset, n);

        return n;
    }

    virtual size_t inode_write(fs::inode* file, const char* buf, size_t offset, size_t n) override
    {
        if (file->flags.in.file != 1)
            return 0;

        auto* data = static_cast<vector<char>*>(file->impl);

        for (size_t i = data->size(); i < offset + n; ++i) {
            data->push_back(0);
        }
        memcpy(data->data() + offset, buf, n);

        return n;
    }

    virtual int inode_stat(dentry* dir, fs::stat* stat) override
    {
        auto* file_inode = dir->ind;

        stat->st_ino = file_inode->ino;
        stat->st_size = file_inode->size;
        if (file_inode->flags.in.file) {
            stat->st_rdev.v = 0;
            stat->st_blksize = 1;
            stat->st_blocks = file_inode->size;
        }
        if (file_inode->flags.in.directory) {
            stat->st_rdev.v = 0;
            stat->st_blksize = sizeof(tmpfs_file_entry);
            stat->st_blocks = file_inode->size;
        }
        if (file_inode->flags.in.special_node) {
            stat->st_rdev.v = (uint32_t)file_inode->impl;
            stat->st_blksize = 0;
            stat->st_blocks = 0;
        }

        return GB_OK;
    }
};

// 8 * 8 for now
static fs::special_node sns[8][8];

size_t fs::vfs_read(fs::inode* file, char* buf, size_t buf_size, size_t offset, size_t n)
{
    if (file->flags.in.special_node) {
        fs::node_t sn {
            .v = (uint32_t)file->impl
        };
        auto* ptr = &sns[sn.in.major][sn.in.minor];
        auto* ops = &ptr->ops;
        if (ops && ops->read)
            return ops->read(ptr, buf, buf_size, offset, n);
        else {
            errno = EINVAL;
            return 0xffffffff;
        }
    } else {
        return file->fs->inode_read(file, buf, buf_size, offset, n);
    }
}
size_t fs::vfs_write(fs::inode* file, const char* buf, size_t offset, size_t n)
{
    if (file->flags.in.special_node) {
        fs::node_t sn {
            .v = (uint32_t)file->impl
        };
        auto* ptr = &sns[sn.in.major][sn.in.minor];
        auto* ops = &ptr->ops;
        if (ops && ops->write)
            return ops->write(ptr, buf, offset, n);
        else {
            errno = EINVAL;
            return 0xffffffff;
        }
    } else {
        return file->fs->inode_write(file, buf, offset, n);
    }
}
int fs::vfs_mkfile(fs::vfs::dentry* dir, const char* filename)
{
    return dir->ind->fs->inode_mkfile(dir, filename);
}
int fs::vfs_mknode(fs::vfs::dentry* dir, const char* filename, fs::node_t sn)
{
    return dir->ind->fs->inode_mknode(dir, filename, sn);
}
int fs::vfs_rmfile(fs::vfs::dentry* dir, const char* filename)
{
    return dir->ind->fs->inode_rmfile(dir, filename);
}
int fs::vfs_mkdir(fs::vfs::dentry* dir, const char* dirname)
{
    return dir->ind->fs->inode_mkdir(dir, dirname);
}

fs::vfs::dentry* fs::vfs_open(const char* path)
{
    if (path[0] == '/' && path[1] == 0x00) {
        return fs::fs_root;
    }

    auto* cur = fs::fs_root;
    size_t n = 0;
    switch (*(path++)) {
    // absolute path
    case '/':
        while (true) {
            if (path[n] == 0x00) {
                cur = cur->find(string(path, n));
                return cur;
            }
            if (path[n] == '/') {
                cur = cur->find(string(path, n));
                if (path[n + 1] == 0x00) {
                    return cur;
                } else {
                    path += (n + 1);
                    n = 0;
                    continue;
                }
            }
            ++n;
        }
        break;
    // empty string
    case 0x00:
        return nullptr;
        break;
    // relative path
    default:
        return nullptr;
        break;
    }
    return nullptr;
}
int fs::vfs_stat(const char* filename, stat* stat)
{
    auto ent = vfs_open(filename);
    return vfs_stat(ent, stat);
}
int fs::vfs_stat(fs::vfs::dentry* ent, stat* stat)
{
    return ent->ind->fs->inode_stat(ent, stat);
}

fs::vfs::dentry* fs::fs_root;
static types::list<fs::vfs*>* fs_es;

void fs::register_special_block(
    uint16_t major,
    uint16_t minor,
    fs::special_node_read read,
    fs::special_node_write write,
    uint32_t data1,
    uint32_t data2)
{
    fs::special_node& sn = sns[major][minor];
    sn.ops.read = read;
    sn.ops.write = write;
    sn.data1 = data1;
    sn.data2 = data2;
}

fs::vfs* fs::register_fs(vfs* fs)
{
    fs_es->push_back(fs);
    return fs;
}

size_t b_null_read(fs::special_node*, char* buf, size_t buf_size, size_t, size_t n)
{
    if (n >= buf_size)
        n = buf_size;
    memset(buf, 0x00, n);
    return n;
}
size_t b_null_write(fs::special_node*, const char*, size_t, size_t n)
{
    return n;
}

void init_vfs(void)
{
    using namespace fs;
    // null
    register_special_block(0, 0, b_null_read, b_null_write, 0, 0);

    fs_es = types::kernel_allocator_new<types::list<vfs*>>();

    auto* rootfs = types::kernel_allocator_new<tmpfs>();
    fs_es->push_back(rootfs);
    fs_root = rootfs->root();

    vfs_mkdir(fs_root, "dev");
    vfs_mkdir(fs_root, "root");
    vfs_mkdir(fs_root, "mnt");
    vfs_mkfile(fs_root, "init");

    auto* init = vfs_open("/init");
    const char* str = "#/bin/sh\nexec /bin/sh\n";
    vfs_write(init->ind, str, 0, strlen(str));

    auto* dev = vfs_open("/dev");
    vfs_mknode(dev, "null", { .in { .major = 0, .minor = 0 } });
    vfs_mknode(dev, "console", { .in { .major = 1, .minor = 0 } });
    vfs_mknode(dev, "hda", { .in { .major = 2, .minor = 0 } });

    stat _stat {};

    vfs_stat("/init", &_stat);
    vfs_stat("/", &_stat);
    vfs_stat("/dev", &_stat);
    vfs_stat("/dev/null", &_stat);
    vfs_stat("/dev/console", &_stat);
    vfs_stat("/dev/hda", &_stat);
}
