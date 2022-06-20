#include <kernel/mem.h>
#include <kernel/stdio.h>
#include <kernel/tty.h>
#include <kernel/vfs.h>

struct tmpfs_file_entry {
    size_t ino;
    char filename[128];
};

struct tmpfs_inode {
    uint32_t occupied : 1;
    uint32_t file : 1;
    uint32_t directory : 1;
    uint32_t mount_point : 1;
    size_t data_size;
    size_t data_capacity;
    char* data;
};

struct tmpfs_data {
    size_t limit;
    size_t inode_count;
    size_t inode_capacity;
    struct tmpfs_inode* inodes;
};

static inline size_t* dir_item_count(struct tmpfs_inode* dir)
{
    return (size_t*)dir->data;
}

static inline struct tmpfs_file_entry* dir_item_p(struct tmpfs_inode* dir)
{
    return (struct tmpfs_file_entry*)(dir->data + sizeof(size_t));
}

static inline struct tmpfs_inode* from_inode(struct tmpfs_data* data, struct inode* inode)
{
    return &data->inodes[inode->ino];
}

static inline void double_data_size(char** p, size_t* current_capacity)
{
    char* tmp = (char*)k_malloc(*current_capacity * 2);
    memcpy(tmp, *p, *current_capacity);
    k_free(*p);
    *p = tmp;
    *current_capacity *= 2;
}

static inline void double_inode_list_size(struct tmpfs_inode** p, size_t* current_capacity)
{
    struct tmpfs_inode* tmp = (struct tmpfs_inode*)k_malloc(*current_capacity * 2 * sizeof(struct tmpfs_inode));
    memset(tmp, 0x00, *current_capacity * 2 * sizeof(struct tmpfs_inode));
    memcpy(tmp, *p, *current_capacity * sizeof(struct tmpfs_inode));
    k_free(*p);
    *p = tmp;
    *current_capacity *= 2;
}

static inline size_t _tmpfs_read(struct tmpfs_inode* inode, char* buf, size_t buf_size, size_t offset, size_t n)
{
    size_t fsize = inode->data_size;

    if (offset + n > fsize)
        n = fsize - offset;

    if (buf_size < n) {
        n = buf_size;
    }

    memcpy(buf, inode->data, n);

    return n;
}

size_t tmpfs_read(struct inode* file, char* buf, size_t buf_size, size_t offset, size_t n)
{
    if (file->flags.file != 1)
        return 0;

    struct tmpfs_data* data = (struct tmpfs_data*)file->impl;
    struct tmpfs_inode* inode = &data->inodes[file->ino];

    return _tmpfs_read(inode, buf, buf_size, offset, n);
}

static inline size_t _tmpfs_write(struct tmpfs_inode* inode, const char* buf, size_t offset, size_t n)
{
    size_t fsize = inode->data_size;

    while (offset + n > inode->data_capacity)
        double_data_size(&inode->data, &inode->data_capacity);

    memcpy(inode->data + offset, buf, n);

    if (offset + n > fsize)
        inode->data_size = offset + n;

    return n;
}

size_t tmpfs_write(struct inode* file, const char* buf, size_t offset, size_t n)
{
    if (file->flags.file != 1)
        return 0;

    struct tmpfs_data* data = (struct tmpfs_data*)file->impl;
    struct tmpfs_inode* inode = &data->inodes[file->ino];

    return _tmpfs_write(inode, buf, offset, n);
}

// a directory is a special file containing the tmpfs_inode
// ids and filenames of the files and directories. the first
// 4 bytes of its data[] is the file count it contains
int tmpfs_readdir(struct inode* dir, struct dirent* entry, size_t i)
{
    if (dir->flags.directory != 1)
        return GB_FAILED;

    struct tmpfs_data* data = (struct tmpfs_data*)dir->impl;
    struct tmpfs_inode* inode = &data->inodes[dir->ino];

    size_t n = *dir_item_count(inode);
    struct tmpfs_file_entry* files = dir_item_p(inode);

    if (i >= n)
        return GB_FAILED;

    entry->ino = files[i].ino;
    snprintf(entry->name, sizeof(entry->name), files[i].filename);

    return GB_OK;
}

static inline size_t _tmpfs_allocinode(struct tmpfs_data* data)
{
    // TODO: reclaim released inodes
    if (data->inode_count == data->inode_capacity) {
        double_inode_list_size(&data->inodes, &data->inode_capacity);
    }
    struct tmpfs_inode* inode = data->inodes + data->inode_count;

    inode->data = (char*)k_malloc(128);
    inode->data_capacity = 128;
    inode->data_size = 0;
    inode->occupied = 1;

    return data->inode_count++;
}

static inline int _tmpfs_mklink(struct tmpfs_inode* dir, size_t ino, const char* filename)
{
    struct tmpfs_file_entry ent = {
        .filename = 0,
        .ino = ino,
    };
    snprintf(ent.filename, sizeof(ent.filename), filename);

    int result = GB_OK;

    if (_tmpfs_write(dir, (const char*)&ent,
            sizeof(size_t) + sizeof(struct tmpfs_file_entry) * *dir_item_count(dir),
            sizeof(struct tmpfs_file_entry))
        != sizeof(struct tmpfs_file_entry)) {
        result = GB_FAILED;
    }
    *dir_item_count(dir) += 1;
    return result;
}

int tmpfs_mkfile(struct inode* dir, const char* filename)
{
    struct tmpfs_data* data = (struct tmpfs_data*)dir->impl;
    size_t ino = _tmpfs_allocinode(data);
    struct tmpfs_inode* inode = &data->inodes[ino];
    inode->file = 1;

    _tmpfs_mklink(from_inode(data, dir), ino, filename);

    return GB_OK;
}

int tmpfs_mkdir(struct inode* dir, const char* dirname)
{
    struct tmpfs_data* data = (struct tmpfs_data*)dir->impl;

    size_t ino = _tmpfs_allocinode(data);
    struct tmpfs_inode* inode = &data->inodes[ino];
    inode->directory = 1;
    *dir_item_count(inode) = 0;

    _tmpfs_mklink(from_inode(data, dir), ino, dirname);
    _tmpfs_mklink(inode, ino, ".");
    _tmpfs_mklink(inode, dir->ino, "..");

    return GB_OK;
}

int mkfs_tmpfs(struct tmpfs_data* data, size_t limit)
{
    data->limit = limit;
    data->inodes = (struct tmpfs_inode*)k_malloc(sizeof(struct tmpfs_inode) * 1);
    memset(data->inodes, 0x00, sizeof(struct tmpfs_inode));
    data->inode_capacity = 1;
    data->inode_count = 0;

    size_t root_ino = _tmpfs_allocinode(data);
    struct tmpfs_inode* root_inode = &data->inodes[root_ino];

    _tmpfs_mklink(root_inode, root_ino, ".");
    _tmpfs_mklink(root_inode, root_ino, "..");

    return GB_OK;
}

// typedef int (*inode_finddir)(struct inode* dir, struct dirent* entry, const char* filename);
// typedef int (*inode_rmfile)(struct inode* dir, const char* filename);

size_t vfs_read(struct inode* file, char* buf, size_t buf_size, size_t offset, size_t n)
{
    if (file->ops->read) {
        return file->ops->read(file, buf, buf_size, offset, n);
    } else {
        return 0;
    }
}
size_t vfs_write(struct inode* file, const char* buf, size_t offset, size_t n)
{
    if (file->ops->write) {
        return file->ops->write(file, buf, offset, n);
    } else {
        return 0;
    }
}
int vfs_readdir(struct inode* dir, struct dirent* entry, size_t i)
{
    if (dir->ops->readdir) {
        return dir->ops->readdir(dir, entry, i);
    } else {
        return 0;
    }
}
int vfs_finddir(struct inode* dir, struct dirent* entry, const char* filename)
{
    if (dir->ops->finddir) {
        return dir->ops->finddir(dir, entry, filename);
    } else {
        return 0;
    }
}
int vfs_mkfile(struct inode* dir, const char* filename)
{
    if (dir->ops->mkfile) {
        return dir->ops->mkfile(dir, filename);
    } else {
        return 0;
    }
}
int vfs_rmfile(struct inode* dir, const char* filename)
{
    if (dir->ops->rmfile) {
        return dir->ops->rmfile(dir, filename);
    } else {
        return 0;
    }
}
int vfs_mkdir(struct inode* dir, const char* dirname)
{
    if (dir->ops->mkdir) {
        return dir->ops->mkdir(dir, dirname);
    } else {
        return 0;
    }
}

static const struct inode_ops tmpfs_inode_ops = {
    .read = tmpfs_read,
    .write = tmpfs_write,
    .readdir = tmpfs_readdir,
    .finddir = 0,
    .mkfile = tmpfs_mkfile,
    .rmfile = 0,
    .mkdir = tmpfs_mkdir,
};
static struct tmpfs_data rootfs_data;
struct inode fs_root;

void init_vfs(void)
{
    mkfs_tmpfs(&rootfs_data, 4096 * 1024);

    fs_root.flags.directory = 1;
    fs_root.flags.mount_point = 1;
    fs_root.ino = 0;
    fs_root.impl = (uint32_t)&rootfs_data;
    fs_root.ops = &tmpfs_inode_ops;
    fs_root.perm = 0777;
}
