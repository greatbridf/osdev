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
using types::list;
using types::string;
using types::vector;

struct tmpfs_file_entry {
    size_t ino;
    char filename[128];
};

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
int fs::vfs::inode_readdir(inode*, dirent*, size_t)
{
    syscall(0x03);
    return GB_FAILED;
}
fs::inode* fs::vfs::inode_findinode(inode*, const char*)
{
    syscall(0x03);
    return nullptr;
}
int fs::vfs::inode_mkfile(inode*, const char*)
{
    syscall(0x03);
    return GB_FAILED;
}
int fs::vfs::inode_mknode(inode*, const char*, node_t)
{
    syscall(0x03);
    return GB_FAILED;
}
int fs::vfs::inode_rmfile(inode*, const char*)
{
    syscall(0x03);
    return GB_FAILED;
}
int fs::vfs::inode_mkdir(inode*, const char*)
{
    syscall(0x03);
    return GB_FAILED;
}
int fs::vfs::inode_stat(inode*, stat*, const char*)
{
    syscall(0x03);
    return GB_FAILED;
}

class tmpfs : public virtual fs::vfs {
private:
    using inode_list_type = list<fs::inode, kernel_allocator>;

private:
    size_t m_limit;
    // TODO: hashtable etc.
    inode_list_type m_inodes;
    fs::ino_t m_last_inode_no;

protected:
    inline vector<tmpfs_file_entry>* mk_fe_vector(void)
    {
        return allocator_traits<kernel_allocator<vector<tmpfs_file_entry>>>::allocate_and_construct();
    }

    inline vector<char>* mk_data_vector(void)
    {
        return allocator_traits<kernel_allocator<vector<char>>>::allocate_and_construct();
    }

    inline fs::inode mk_inode(fs::inode_flags flags, void* data)
    {
        fs::inode i {};
        i.flags.v = flags.v;
        i.impl = data;
        i.ino = m_last_inode_no++;
        i.perm = 0777;
        i.fs = this;
        return i;
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
    }

public:
    explicit tmpfs(size_t limit)
        : m_limit(limit)
        , m_last_inode_no(0)
    {
        auto in = mk_inode({ .v = INODE_DIR | INODE_MNT }, mk_fe_vector());

        mklink(&in, &in, ".");
        mklink(&in, &in, "..");

        m_inodes.push_back(in);
    }

    virtual int inode_mkfile(fs::inode* dir, const char* filename) override
    {
        auto file = mk_inode({ .v = INODE_FILE }, mk_data_vector());
        m_inodes.push_back(file);
        mklink(dir, &file, filename);
        return GB_OK;
    }

    virtual int inode_mknode(fs::inode* dir, const char* filename, fs::node_t sn) override
    {
        auto node = mk_inode({ .v = INODE_NODE }, (void*)sn.v);
        m_inodes.push_back(node);
        mklink(dir, &node, filename);
        return GB_OK;
    }

    virtual int inode_mkdir(fs::inode* dir, const char* dirname) override
    {
        auto new_dir = mk_inode({ .v = INODE_DIR }, mk_fe_vector());
        m_inodes.push_back(new_dir);
        mklink(&new_dir, &new_dir, ".");

        mklink(dir, &new_dir, dirname);
        mklink(&new_dir, dir, "..");
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

    virtual int inode_readdir(fs::inode* dir, fs::dirent* entry, size_t i) override
    {
        if (dir->flags.in.directory != 1) {
            errno = ENOTDIR;
            return GB_FAILED;
        }

        auto* fes = static_cast<vector<tmpfs_file_entry>*>(dir->impl);

        if (i >= fes->size()) {
            errno = ENOENT;
            return GB_FAILED;
        }

        entry->ino = fes->at(i).ino;
        snprintf(entry->name, sizeof(entry->name), fes->at(i).filename);

        return GB_OK;
    }

    virtual fs::inode* inode_findinode(fs::inode* dir, const char* filename) override
    {
        fs::dirent ent {};
        size_t i = 0;
        while (inode_readdir(dir, &ent, i) == GB_OK) {
            if (strcmp(ent.name, filename) == 0) {
                // TODO: if the inode is a mount point, the ino MIGHT BE THE SAME
                // optimize: use hash table to build an index
                for (auto iter = m_inodes.begin(); iter != m_inodes.end(); ++iter)
                    if (iter->ino == ent.ino)
                        return iter.ptr();
            }
            ++i;
        }
        return nullptr;
    }

    virtual int inode_stat(fs::inode* dir, fs::stat* stat, const char* filename) override
    {
        // for later use
        // auto* fes = static_cast<vector<struct tmpfs_file_entry>*>(dir->impl);

        auto* file_inode = vfs_findinode(dir, filename);

        if (!file_inode) {
            errno = ENOENT;
            return GB_FAILED;
        }

        stat->st_ino = file_inode->ino;
        if (file_inode->flags.in.file) {
            stat->st_rdev.v = 0;
            stat->st_blksize = 1;
            stat->st_blocks = static_cast<vector<char>*>(file_inode->impl)->size();
        }
        if (file_inode->flags.in.directory) {
            stat->st_rdev.v = 0;
            stat->st_blksize = sizeof(tmpfs_file_entry);
            stat->st_blocks = static_cast<vector<tmpfs_file_entry>*>(file_inode->impl)->size();
        }
        if (file_inode->flags.in.special_node) {
            stat->st_rdev.v = (uint32_t)file_inode->impl;
            stat->st_blksize = 0;
            stat->st_blocks = 0;
        }

        return GB_OK;
    }

    fs::inode* root_inode(void)
    {
        return m_inodes.begin().ptr();
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
int fs::vfs_readdir(fs::inode* dir, fs::dirent* entry, size_t i)
{
    return dir->fs->inode_readdir(dir, entry, i);
}
fs::inode* fs::vfs_findinode(fs::inode* dir, const char* filename)
{
    return dir->fs->inode_findinode(dir, filename);
}
int fs::vfs_mkfile(fs::inode* dir, const char* filename)
{
    return dir->fs->inode_mkfile(dir, filename);
}
int fs::vfs_mknode(fs::inode* dir, const char* filename, fs::node_t sn)
{
    return dir->fs->inode_mknode(dir, filename, sn);
}
int fs::vfs_rmfile(fs::inode* dir, const char* filename)
{
    return dir->fs->inode_rmfile(dir, filename);
}
int fs::vfs_mkdir(fs::inode* dir, const char* dirname)
{
    return dir->fs->inode_mkdir(dir, dirname);
}

fs::inode* fs::vfs_open(const char* path)
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
                string fname(path, n);
                cur = vfs_findinode(cur, fname.c_str());
                return cur;
            }
            if (path[n] == '/') {
                string fname(path, n);
                cur = vfs_findinode(cur, fname.c_str());
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

int fs::vfs_stat(struct stat* stat, const char* _path)
{
    if (_path[0] == '/' && _path[1] == 0x00)
        return fs_root->fs->inode_stat(fs_root, stat, ".");

    string path(_path);
    auto iter = path.back();
    while (*(iter - 1) != '/')
        --iter;
    string filename(&*iter);
    string parent_path = path.substr(0, &*iter - path.data());

    auto* dir_inode = vfs_open(parent_path.c_str());

    if (!dir_inode) {
        errno = ENOENT;
        return GB_FAILED;
    }

    return dir_inode->fs->inode_stat(dir_inode, stat, filename.c_str());
}

fs::inode* fs::fs_root;
static tmpfs* rootfs;

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
    // null
    fs::register_special_block(0, 0, b_null_read, b_null_write, 0, 0);

    rootfs = allocator_traits<kernel_allocator<tmpfs>>::allocate_and_construct(4096 * 1024);
    fs::fs_root = rootfs->root_inode();

    fs::vfs_mkdir(fs::fs_root, "dev");
    fs::vfs_mkdir(fs::fs_root, "root");
    fs::vfs_mkfile(fs::fs_root, "init");

    auto* init = fs::vfs_open("/init");
    const char* str = "#/bin/sh\nexec /bin/sh\n";
    fs::vfs_write(init, str, 0, strlen(str));

    auto* dev = fs::vfs_open("/dev");
    fs::vfs_mknode(dev, "null", { .in { .major = 0, .minor = 0 } });
    fs::vfs_mknode(dev, "console", { .in { .major = 1, .minor = 0 } });
    fs::vfs_mknode(dev, "hda", { .in { .major = 2, .minor = 0 } });

    fs::stat _stat {};

    fs::vfs_stat(&_stat, "/init");
    fs::vfs_stat(&_stat, "/");
    fs::vfs_stat(&_stat, "/dev");
    fs::vfs_stat(&_stat, "/dev/null");
    fs::vfs_stat(&_stat, "/dev/console");
    fs::vfs_stat(&_stat, "/dev/hda");
}
