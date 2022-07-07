#include <kernel/errno.h>
#include <kernel/mem.h>
#include <kernel/stdio.h>
#include <kernel/tty.h>
#include <kernel/vfs.h>
#include <types/allocator.hpp>
#include <types/list.hpp>
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

class tmpfs {
private:
    using inode_list_type = list<struct inode, kernel_allocator>;

private:
    size_t m_limit;
    // TODO: hashtable etc.
    inode_list_type m_inodes;
    struct fs_info m_fs;
    size_t m_last_inode_no;

protected:
    inline vector<struct tmpfs_file_entry>* mk_fe_vector(void)
    {
        return allocator_traits<kernel_allocator<vector<struct tmpfs_file_entry>>>::allocate_and_construct();
    }

    inline vector<char>* mk_data_vector(void)
    {
        return allocator_traits<kernel_allocator<vector<char>>>::allocate_and_construct();
    }

    inline struct inode mk_inode(union inode_flags flags, void* data)
    {
        struct inode i { };
        i.flags.v = flags.v;
        i.fs = &m_fs;
        i.impl = data;
        i.ino = m_last_inode_no++;
        i.perm = 0777;
        return i;
    }

public:
    explicit tmpfs(size_t limit);
    void mklink(struct inode* dir, struct inode* inode, const char* filename);
    void mkfile(struct inode* dir, const char* filename);
    void mknode(struct inode* dir, const char* filename, union node_t sn);
    void mkdir(struct inode* dir, const char* dirname);
    size_t read(struct inode* file, char* buf, size_t buf_size, size_t offset, size_t n);
    size_t write(struct inode* file, const char* buf, size_t offset, size_t n);
    int readdir(struct inode* dir, struct dirent* entry, size_t i);
    struct inode* findinode(struct inode* dir, const char* filename);
    int stat(struct inode* dir, struct stat* stat, const char* filename);

    struct inode* root_inode(void)
    {
        return &*m_inodes.begin();
    }
};

size_t tmpfs_read(struct inode* file, char* buf, size_t buf_size, size_t offset, size_t n)
{
    auto* fs = static_cast<tmpfs*>(file->fs->impl);
    return fs->read(file, buf, buf_size, offset, n);
}
size_t tmpfs_write(struct inode* file, const char* buf, size_t offset, size_t n)
{
    auto* fs = static_cast<tmpfs*>(file->fs->impl);
    return fs->write(file, buf, offset, n);
}
int tmpfs_readdir(struct inode* dir, struct dirent* entry, size_t i)
{
    auto* fs = static_cast<tmpfs*>(dir->fs->impl);
    return fs->readdir(dir, entry, i);
}
struct inode* tmpfs_findinode(struct inode* dir, const char* filename)
{
    auto* fs = static_cast<tmpfs*>(dir->fs->impl);
    return fs->findinode(dir, filename);
}
int tmpfs_mkfile(struct inode* dir, const char* filename)
{
    auto* fs = static_cast<tmpfs*>(dir->fs->impl);
    fs->mkfile(dir, filename);
    return GB_OK;
}
int tmpfs_mknode(struct inode* dir, const char* filename, union node_t sn)
{
    auto* fs = static_cast<tmpfs*>(dir->fs->impl);
    fs->mknode(dir, filename, sn);
    return GB_OK;
}
// int tmpfs_rmfile(struct inode* dir, const char* filename)
// {
//     auto* fs = static_cast<tmpfs*>(dir->fs->impl);
//     fs->rmfile(dir, filename);
//     return GB_OK;
// }
int tmpfs_mkdir(struct inode* dir, const char* dirname)
{
    auto* fs = static_cast<tmpfs*>(dir->fs->impl);
    fs->mkdir(dir, dirname);
    return GB_OK;
}
int tmpfs_stat(struct inode* dir, struct stat* stat, const char* filename)
{
    auto* fs = static_cast<tmpfs*>(dir->fs->impl);
    return fs->stat(dir, stat, filename);
}

static const struct inode_ops tmpfs_inode_ops = {
    .read = tmpfs_read,
    .write = tmpfs_write,
    .readdir = tmpfs_readdir,
    .findinode = tmpfs_findinode,
    .mkfile = tmpfs_mkfile,
    .mknode = tmpfs_mknode,
    .rmfile = 0,
    .mkdir = tmpfs_mkdir,
    .stat = tmpfs_stat,
};

tmpfs::tmpfs(size_t limit)
    : m_limit(limit)
    , m_fs { .ops = &tmpfs_inode_ops, .impl = this }
    , m_last_inode_no(0)
{
    struct inode in = mk_inode({ .v = INODE_DIR | INODE_MNT }, mk_fe_vector());

    mklink(&in, &in, ".");
    mklink(&in, &in, "..");

    m_inodes.push_back(in);
}

void tmpfs::mklink(struct inode* dir, struct inode* inode, const char* filename)
{
    auto* fes = static_cast<vector<struct tmpfs_file_entry>*>(dir->impl);
    struct tmpfs_file_entry ent = {
        .ino = inode->ino,
        .filename = { 0 },
    };
    snprintf(ent.filename, sizeof(ent.filename), filename);
    fes->push_back(ent);
}

void tmpfs::mkfile(struct inode* dir, const char* filename)
{
    struct inode file = mk_inode({ .v = INODE_FILE }, mk_data_vector());
    m_inodes.push_back(file);
    mklink(dir, &file, filename);
}

void tmpfs::mknode(struct inode* dir, const char* filename, union node_t sn)
{
    struct inode node = mk_inode({ .v = INODE_NODE }, (void*)sn.v);
    m_inodes.push_back(node);
    mklink(dir, &node, filename);
}

void tmpfs::mkdir(struct inode* dir, const char* dirname)
{
    struct inode new_dir = mk_inode({ .v = INODE_DIR }, mk_fe_vector());
    m_inodes.push_back(new_dir);
    mklink(&new_dir, &new_dir, ".");

    mklink(dir, &new_dir, dirname);
    mklink(&new_dir, dir, "..");
}

size_t tmpfs::read(struct inode* file, char* buf, size_t buf_size, size_t offset, size_t n)
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

size_t tmpfs::write(struct inode* file, const char* buf, size_t offset, size_t n)
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

int tmpfs::readdir(struct inode* dir, struct dirent* entry, size_t i)
{
    if (dir->flags.in.directory != 1) {
        errno = ENOTDIR;
        return GB_FAILED;
    }

    auto* fes = static_cast<vector<struct tmpfs_file_entry>*>(dir->impl);

    if (i >= fes->size()) {
        errno = ENOENT;
        return GB_FAILED;
    }

    entry->ino = fes->at(i).ino;
    snprintf(entry->name, sizeof(entry->name), fes->at(i).filename);

    return GB_OK;
}

struct inode* tmpfs::findinode(struct inode* dir, const char* filename)
{
    struct dirent ent { };
    size_t i = 0;
    while (readdir(dir, &ent, i) == GB_OK) {
        if (strcmp(ent.name, filename) == 0) {
            // optimize: use hash table to build an index
            auto& inodes = static_cast<tmpfs*>(dir->fs->impl)->m_inodes;
            for (auto iter = inodes.begin(); iter != inodes.end(); ++iter)
                if (iter->ino == ent.ino)
                    return iter.ptr();
        }
        ++i;
    }
    return nullptr;
}

int tmpfs::stat(struct inode* dir, struct stat* stat, const char* filename)
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
        stat->st_blksize = sizeof(struct tmpfs_file_entry);
        stat->st_blocks = static_cast<vector<struct tmpfs_file_entry>*>(file_inode->impl)->size();
    }
    if (file_inode->flags.in.special_node) {
        stat->st_rdev.v = (uint32_t)file_inode->impl;
        stat->st_blksize = 0;
        stat->st_blocks = 0;
    }

    return GB_OK;
}

// 8 * 8 for now
static struct special_node_ops sn_ops[8][8];

size_t vfs_read(struct inode* file, char* buf, size_t buf_size, size_t offset, size_t n)
{
    if (file->flags.in.special_node) {
        union node_t sn {
            .v = (uint32_t)file->impl
        };
        auto* ops = &sn_ops[sn.in.major][sn.in.minor];
        if (ops && ops->read)
            return ops->read(buf, buf_size, offset, n);
    } else {
        if (file->fs->ops->read)
            return file->fs->ops->read(file, buf, buf_size, offset, n);
    }
    return 0;
}
size_t vfs_write(struct inode* file, const char* buf, size_t offset, size_t n)
{
    if (file->flags.in.special_node) {
        union node_t sn {
            .v = (uint32_t)file->impl
        };
        auto* ops = &sn_ops[sn.in.major][sn.in.minor];
        if (ops && ops->write)
            return ops->write(buf, offset, n);
    } else {
        if (file->fs->ops->read)
            return file->fs->ops->write(file, buf, offset, n);
    }
    return 0;
}
int vfs_readdir(struct inode* dir, struct dirent* entry, size_t i)
{
    if (dir->fs->ops->readdir) {
        return dir->fs->ops->readdir(dir, entry, i);
    } else {
        return 0;
    }
}
struct inode* vfs_findinode(struct inode* dir, const char* filename)
{
    if (dir->fs->ops->findinode) {
        return dir->fs->ops->findinode(dir, filename);
    } else {
        return nullptr;
    }
}
int vfs_mkfile(struct inode* dir, const char* filename)
{
    if (dir->fs->ops->mkfile) {
        return dir->fs->ops->mkfile(dir, filename);
    } else {
        return 0;
    }
}
int vfs_mknode(struct inode* dir, const char* filename, union node_t sn)
{
    if (dir->fs->ops->mknode) {
        return dir->fs->ops->mknode(dir, filename, sn);
    } else {
        return 0;
    }
}
int vfs_rmfile(struct inode* dir, const char* filename)
{
    if (dir->fs->ops->rmfile) {
        return dir->fs->ops->rmfile(dir, filename);
    } else {
        return 0;
    }
}
int vfs_mkdir(struct inode* dir, const char* dirname)
{
    if (dir->fs->ops->mkdir) {
        return dir->fs->ops->mkdir(dir, dirname);
    } else {
        return 0;
    }
}

struct inode* vfs_open(const char* path)
{
    if (path[0] == '/' && path[1] == 0x00) {
        return fs_root;
    }

    struct inode* cur = fs_root;
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

int vfs_stat(struct stat* stat, const char* _path)
{
    if (_path[0] == '/' && _path[1] == 0x00) {
        if (fs_root->fs->ops->stat) {
            return fs_root->fs->ops->stat(fs_root, stat, ".");
        } else {
            errno = EINVAL;
            return GB_FAILED;
        }
    }

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

    if (dir_inode->fs->ops->stat) {
        return dir_inode->fs->ops->stat(dir_inode, stat, filename.c_str());
    } else {
        errno = EINVAL;
        return GB_FAILED;
    }
}

struct inode* fs_root;
static tmpfs* rootfs;

void register_special_block(uint16_t major, uint16_t minor, special_node_read read, special_node_write write)
{
    sn_ops[major][minor].read = read;
    sn_ops[major][minor].write = write;
}

size_t b_null_read(char* buf, size_t buf_size, size_t, size_t n)
{
    if (n >= buf_size)
        n = buf_size;
    memset(buf, 0x00, n);
    return n;
}
size_t b_null_write(const char*, size_t, size_t n)
{
    return n;
}

void init_vfs(void)
{
    // null
    register_special_block(0, 0, b_null_read, b_null_write);

    rootfs = allocator_traits<kernel_allocator<tmpfs>>::allocate_and_construct(4096 * 1024);
    fs_root = rootfs->root_inode();

    vfs_mkdir(fs_root, "dev");
    vfs_mkdir(fs_root, "root");
    vfs_mkfile(fs_root, "init");

    auto* init = vfs_open("/init");
    const char* str = "#/bin/sh\nexec /bin/sh\n";
    vfs_write(init, str, 0, strlen(str));

    auto* dev = vfs_open("/dev");
    vfs_mknode(dev, "null", { .in { .major = 0, .minor = 0 } });
    vfs_mknode(dev, "console", { .in { .major = 1, .minor = 0 } });
    vfs_mknode(dev, "hda", { .in { .major = 2, .minor = 0 } });

    struct stat _stat { };

    vfs_stat(&_stat, "/init");
    vfs_stat(&_stat, "/");
    vfs_stat(&_stat, "/dev");
    vfs_stat(&_stat, "/dev/null");
    vfs_stat(&_stat, "/dev/console");
    vfs_stat(&_stat, "/dev/hda");
}
