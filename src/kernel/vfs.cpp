#include <bit>
#include <cstddef>
#include <map>
#include <string>
#include <utility>
#include <vector>

#include <assert.h>
#include <bits/alltypes.h>
#include <errno.h>
#include <stdint.h>
#include <stdio.h>
#include <sys/mount.h>
#include <sys/types.h>

#include <types/allocator.hpp>
#include <types/path.hpp>

#include <kernel/log.hpp>
#include <kernel/process.hpp>
#include <kernel/tty.hpp>
#include <kernel/vfs.hpp>
#include <kernel/vfs/dentry.hpp>
#include <kernel/vfs/vfs.hpp>

using fs::vfs, fs::dentry;

dentry::dentry(dentry* _parent, inode* _ind, name_type _name)
    : parent(_parent) , ind(_ind) , flags { } , name(_name)
{
    // the dentry is filesystem root or _ind MUST be non null
    assert(_ind || !_parent);
    if (!ind || S_ISDIR(ind->mode)) {
        flags.dir = 1;
        children = new std::list<dentry>;
        idx_children = new types::hash_map<name_type, dentry*>;
    }
}

int dentry::load()
{
    if (!flags.dir || !S_ISDIR(ind->mode))
        return -ENOTDIR;

    size_t offset = 0;
    vfs* fs = ind->fs;

    while (true) {
        int ret = fs->readdir(ind, offset,
            [this](const char* name, size_t len, inode* ind, uint8_t) -> int {
                if (!len)
                    append(ind, name);
                else
                    append(ind, dentry::name_type(name, len));

                return 0;
            });

        if (ret == 0)
            break;

        offset += ret;
    }

    flags.present = 1;

    return 0;
}

dentry* dentry::append(inode* ind, name_type name)
{
    auto& ent = children->emplace_back(this, ind, name);
    idx_children->emplace(ent.name, &ent);
    return &ent;
}

dentry* dentry::find(const name_type& name)
{
    if (!flags.dir)
        return nullptr;

    if (name[0] == '.') {
        if (!name[1])
            return this;
        if (name[1] == '.' && !name[2])
            return parent ? parent : this;
    }

    if (!flags.present) {
        int ret = load();
        if (ret != 0) {
            errno = -ret;
            return nullptr;
        }
    }

    auto iter = idx_children->find(name);
    if (!iter) {
        errno = ENOENT;
        return nullptr;
    }

    return iter->second;
}

dentry* dentry::replace(dentry* val)
{
    // TODO: prevent the dirent to be swapped out of memory
    parent->idx_children->find(this->name)->second = val;
    return this;
}

void dentry::remove(const name_type& name)
{
    for (auto iter = children->begin(); iter != children->end(); ++iter) {
        if (iter->name != name)
            continue;
        children->erase(iter);
        break;
    }

    idx_children->remove(name);
}

void dentry::path(
    const dentry& root, types::path &out_dst) const
{
    const dentry* dents[32];
    int cnt = 0;

    const dentry* cur = this;
    while (cur != &root) {
        assert(cnt < 32);
        dents[cnt++] = cur;
        cur = cur->parent;
    }

    out_dst.append("/");
    for (int i = cnt - 1; i >= 0; --i)
        out_dst.append(dents[i]->name.c_str());
}

vfs::vfs()
    : _root { nullptr, nullptr, "" }
{
}

fs::inode* vfs::cache_inode(size_t size, ino_t ino,
    mode_t mode, uid_t uid, gid_t gid)
{
    auto [ iter, inserted ] =
        _inodes.try_emplace(ino, inode { ino, this, size, 0, mode, uid, gid });
    return &iter->second;
}

void vfs::free_inode(ino_t ino)
{
    int n = _inodes.erase(ino);
    assert(n == 1);
}

fs::inode* vfs::get_inode(ino_t ino)
{
    auto iter = _inodes.find(ino);
    // TODO: load inode from disk if not found
    if (iter)
        return &iter->second;
    else
        return nullptr;
}

void vfs::register_root_node(inode* root)
{
    if (!_root.ind)
        _root.ind = root;
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
    _mount_recover_list.emplace(new_ent, orig_ent);

    return 0;
}

// default behavior is to
// return -EINVAL to show that the operation
// is not supported by the fs

size_t vfs::read(inode*, char*, size_t, size_t, size_t)
{
    return -EINVAL;
}

size_t vfs::write(inode*, const char*, size_t, size_t)
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

int vfs::inode_statx(dentry*, statx*, unsigned int)
{
    return -EINVAL;
}

int vfs::inode_stat(dentry*, struct stat*)
{
    return -EINVAL;
}

int vfs::dev_id(inode*, dev_t&)
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

fs::regular_file::regular_file(dentry* parent,
    file_flags flags, size_t cursor, inode* ind)
    : file(ind->mode, parent, flags), cursor(cursor), ind(ind) { }

ssize_t fs::regular_file::read(char* __user buf, size_t n)
{
    if (!flags.read)
        return -EBADF;

    if (S_ISDIR(ind->mode))
        return -EISDIR;

    // TODO: copy to user function !IMPORTANT
    ssize_t n_wrote = fs::vfs_read(ind, buf, n, cursor, n);
    if (n_wrote >= 0)
        cursor += n_wrote;

    return n_wrote;
}

ssize_t fs::regular_file::do_write(const char* __user buf, size_t n)
{
    if (S_ISDIR(mode))
        return -EISDIR;

    // TODO: check privilege of user ptr
    ssize_t n_wrote = fs::vfs_write(ind, buf, cursor, n);
    if (n_wrote >= 0)
        cursor += n_wrote;

    return n_wrote;
}

off_t fs::regular_file::seek(off_t n, int whence)
{
    if (!S_ISREG(mode))
        return -ESPIPE;

    size_t pos;
    switch (whence) {
    case SEEK_SET:
        pos = n;
        break;
    case SEEK_CUR:
        pos = cursor + n;
        break;
    case SEEK_END:
        pos = ind->size + n;
        break;
    default:
        return -EINVAL;
    }

    if (pos > ind->size)
        return -EINVAL;

    cursor = pos;

    return cursor;
}

int fs::regular_file::getdents(char* __user buf, size_t cnt)
{
    if (!S_ISDIR(ind->mode))
        return -ENOTDIR;

    size_t orig_cnt = cnt;
    int nread = ind->fs->readdir(ind, cursor,
        [&buf, &cnt](const char* fn, size_t len, inode* ind, uint8_t type) {
            if (!len)
                len = strlen(fn);

            size_t reclen = sizeof(fs::user_dirent) + 1 + len;
            if (cnt < reclen)
                return -EFAULT;

            auto* dirp = (fs::user_dirent*)buf;
            dirp->d_ino = ind->ino;
            dirp->d_reclen = reclen;
            // TODO: show offset
            // dirp->d_off = 0;
            // TODO: use copy_to_user
            memcpy(dirp->d_name, fn, len);
            buf[reclen - 2] = 0;
            buf[reclen - 1] = type;

            buf += reclen;
            cnt -= reclen;
            return 0;
        });

    if (nread > 0)
        cursor += nread;

    return orig_cnt - cnt;
}

int fs::regular_file::getdents64(char* __user buf, size_t cnt)
{
    if (!S_ISDIR(ind->mode))
        return -ENOTDIR;

    size_t orig_cnt = cnt;
    int nread = ind->fs->readdir(ind, cursor,
        [&buf, &cnt](const char* fn, size_t len, inode* ind, uint8_t type) {
            if (!len)
                len = strlen(fn);

            size_t reclen = sizeof(fs::user_dirent64) + len;
            if (cnt < reclen)
                return -EFAULT;

            auto* dirp = (fs::user_dirent64*)buf;
            dirp->d_ino = ind->ino;
            dirp->d_off = 114514;
            dirp->d_reclen = reclen;
            dirp->d_type = type;
            // TODO: use copy_to_user
            memcpy(dirp->d_name, fn, len);
            buf[reclen - 1] = 0;

            buf += reclen;
            cnt -= reclen;
            return 0;
        });

    if (nread > 0)
        cursor += nread;

    return orig_cnt - cnt;
}

fs::fifo_file::fifo_file(dentry* parent, file_flags flags,
    std::shared_ptr<fs::pipe> ppipe)
    : file(S_IFIFO, parent, flags), ppipe(ppipe) { }

ssize_t fs::fifo_file::read(char* __user buf, size_t n)
{
    if (!flags.read)
        return -EBADF;

    return ppipe->read(buf, n);
}

ssize_t fs::fifo_file::do_write(const char* __user buf, size_t n)
{
    return ppipe->write(buf, n);
}

fs::fifo_file::~fifo_file()
{
    assert(flags.read ^ flags.write);
    if (flags.read)
        ppipe->close_read();
    else
        ppipe->close_write();
}

static fs::blkdev_ops** blkdevs[256];
static fs::chrdev_ops** chrdevs[256];

size_t fs::vfs_read(fs::inode* file, char* buf, size_t buf_size, size_t offset, size_t n)
{
    if (S_ISDIR(file->mode)) {
        errno = EISDIR;
        return -1U;
    }

    if (S_ISREG(file->mode))
        return file->fs->read(file, buf, buf_size, offset, n);

    if (S_ISBLK(file->mode) || S_ISCHR(file->mode)) {
        dev_t dev;
        if (file->fs->dev_id(file, dev) != 0) {
            errno = EINVAL;
            return -1U;
        }

        ssize_t ret;
        if (S_ISBLK(file->mode))
            ret = block_device_read(dev, buf, buf_size, offset, n);
        else
            ret = char_device_read(dev, buf, buf_size, n);

        if (ret < 0) {
            errno = -ret;
            return -1U;
        }

        return ret;
    }

    errno = EINVAL;
    return -1U;
}
size_t fs::vfs_write(fs::inode* file, const char* buf, size_t offset, size_t n)
{
    if (S_ISDIR(file->mode)) {
        errno = EISDIR;
        return -1U;
    }

    if (S_ISREG(file->mode))
        return file->fs->write(file, buf, offset, n);

    if (S_ISBLK(file->mode) || S_ISCHR(file->mode)) {
        dev_t dev;
        if (file->fs->dev_id(file, dev) != 0) {
            errno = EINVAL;
            return -1U;
        }

        ssize_t ret;
        if (S_ISBLK(file->mode))
            ret = block_device_write(dev, buf, offset, n);
        else
            ret = char_device_write(dev, buf, n);

        if (ret < 0) {
            errno = -ret;
            return -1U;
        }

        return ret;
    }

    errno = EINVAL;
    return -1U;
}
int fs::vfs_mkfile(dentry* dir, const char* filename, mode_t mode)
{
    return dir->ind->fs->inode_mkfile(dir, filename, mode);
}
int fs::vfs_mknode(dentry* dir, const char* filename, mode_t mode, dev_t dev)
{
    return dir->ind->fs->inode_mknode(dir, filename, mode, dev);
}
int fs::vfs_rmfile(dentry* dir, const char* filename)
{
    return dir->ind->fs->inode_rmfile(dir, filename);
}
int fs::vfs_mkdir(dentry* dir, const char* dirname, mode_t mode)
{
    return dir->ind->fs->inode_mkdir(dir, dirname, mode);
}

dentry* fs::vfs_open(dentry& root, const types::path& path, bool follow, int recurs_no)
{
    // too many recursive search layers will cause stack overflow
    // so we use 16 for now
    if (recurs_no >= 16)
        return nullptr;

    dentry* cur = &root;

    types::path curpath("/");
    for (const auto& item : path) {
        if (S_ISLNK(cur->ind->mode)) {
            char linkpath[256];
            int ret = cur->ind->fs->readlink(cur->ind, linkpath, sizeof(linkpath));

            // TODO: return error code
            if (ret < 0)
                return nullptr;

            curpath.remove_last();
            curpath.append(linkpath, ret);
            cur = fs::vfs_open(root, curpath, true, recurs_no+1);

            if (!cur)
                return nullptr;
        }

        if (item.empty())
            continue;
        cur = cur->find(item);
        if (!cur)
            return nullptr;

        curpath.append(item.c_str());
    }

    if (follow && S_ISLNK(cur->ind->mode)) {
        char linkpath[256];
        int ret = cur->ind->fs->readlink(cur->ind, linkpath, sizeof(linkpath));

        // TODO: return error code
        if (ret < 0)
            return nullptr;

        curpath.remove_last();
        curpath.append(linkpath, ret);
        cur = fs::vfs_open(root, curpath, true, recurs_no+1);

        if (!cur)
            return nullptr;
    }

    return cur;
}

int fs::vfs_stat(dentry* ent, statx* stat, unsigned int mask)
{
    return ent->ind->fs->inode_statx(ent, stat, mask);
}

int fs::vfs_truncate(inode *file, size_t size)
{
    return file->fs->truncate(file, size);
}

int fs::register_block_device(dev_t node, const fs::blkdev_ops& ops)
{
    int major = NODE_MAJOR(node);
    int minor = NODE_MINOR(node);

    if (!blkdevs[major])
        blkdevs[major] = new blkdev_ops*[256] {};

    if (blkdevs[major][minor])
        return -EEXIST;

    blkdevs[major][minor] = new blkdev_ops { ops };
    return 0;
}

int fs::register_char_device(dev_t node, const fs::chrdev_ops& ops)
{
    int major = NODE_MAJOR(node);
    int minor = NODE_MINOR(node);

    if (!chrdevs[major])
        chrdevs[major] = new chrdev_ops*[256] {};

    if (chrdevs[major][minor])
        return -EEXIST;

    chrdevs[major][minor] = new chrdev_ops { ops };
    return 0;
}

static std::map<std::string, fs::create_fs_func_t> fs_list;

int fs::register_fs(const char* name, fs::create_fs_func_t func)
{
    fs_list.emplace(name, func);

    return 0;
}

int fs::create_fs(const char* source, const char* mount_point, const char* fstype,
        unsigned long flags, const void* data, vfs*& out_vfs)
{
    auto iter = fs_list.find(fstype);
    if (!iter)
        return -ENODEV;

    auto& [ _, func ] = *iter;

    if (!(flags & MS_NOATIME))
        flags |= MS_RELATIME;

    if (flags & MS_STRICTATIME)
        flags &= ~(MS_RELATIME | MS_NOATIME);

    vfs* created_vfs = func(source, flags, data);

    mount_data mnt_data {
        .source = source,
        .mount_point = mount_point,
        .fstype = fstype,
        .flags = flags,
    };

    mounts.emplace(created_vfs, mnt_data);

    out_vfs = created_vfs;

    return 0;
}

// MBR partition table, used by partprobe()

struct PACKED mbr_part_entry {
    uint8_t attr;
    uint8_t chs_start[3];
    uint8_t type;
    uint8_t chs_end[3];
    uint32_t lba_start;
    uint32_t cnt;
};

struct PACKED mbr {
    uint8_t code[440];
    uint32_t signature;
    uint16_t reserved;
    mbr_part_entry parts[4];
    uint16_t magic;
};

// TODO: devtmpfs
static int mbr_part_probe(dev_t node)
{
    mbr buf_mbr;

    int ret = fs::block_device_read(node, (char*)&buf_mbr, sizeof(mbr), 0, 512);
    if (ret < 0)
        return -EIO;

    int n = 1;
    for (const auto& part : buf_mbr.parts) {
        if (n >= 16)
            break;

        if (!part.type)
            continue;

        std::size_t part_offset = part.lba_start * 512;

        // TODO: add partition offset limit
        fs::register_block_device(node + n, {
            [=](char* buf, size_t buf_size, size_t offset, size_t n) -> ssize_t {
                offset += part_offset;
                return fs::block_device_read(node, buf, buf_size, offset, n);
            },
            [=](const char* buf, size_t offset, size_t n) -> ssize_t {
                offset += part_offset;
                return fs::block_device_write(node, buf, offset, n);
            }
        });

        ++n;
    }

    return 0;
}

void fs::partprobe()
{
    for (int i = 0; i < 256; i += 16) {
        int ret = mbr_part_probe(make_device(8, i));

        if (ret != 0)
            continue;

        kmsgf("[info] found disk drive sd%c\n", 'a' + (i / 16));
    }
}

ssize_t fs::block_device_read(dev_t node, char* buf, size_t buf_size, size_t offset, size_t n)
{
    int major = NODE_MAJOR(node);
    int minor = NODE_MINOR(node);

    if (!blkdevs[major] || !blkdevs[major][minor])
        return -EINVAL;

    auto& read = blkdevs[major][minor]->read;
    if (!read)
        return -EINVAL;

    return read(buf, buf_size, offset, n);
}

ssize_t fs::block_device_write(dev_t node, const char* buf, size_t offset, size_t n)
{
    int major = NODE_MAJOR(node);
    int minor = NODE_MINOR(node);

    if (!blkdevs[major] || !blkdevs[major][minor])
        return -EINVAL;

    auto& write = blkdevs[major][minor]->write;
    if (!write)
        return -EINVAL;

    return write(buf, offset, n);
}

ssize_t fs::char_device_read(dev_t node, char* buf, size_t buf_size, size_t n)
{
    int major = NODE_MAJOR(node);
    int minor = NODE_MINOR(node);

    if (!chrdevs[major] || !chrdevs[major][minor])
        return -EINVAL;

    auto& read = chrdevs[major][minor]->read;
    if (!read)
        return -EINVAL;

    return read(buf, buf_size, n);
}

ssize_t fs::char_device_write(dev_t node, const char* buf, size_t n)
{
    int major = NODE_MAJOR(node);
    int minor = NODE_MINOR(node);

    if (!chrdevs[major] || !chrdevs[major][minor])
        return -EINVAL;

    auto& write = chrdevs[major][minor]->write;
    if (!write)
        return -EINVAL;

    return write(buf, n);
}

ssize_t b_null_read(char*, size_t, size_t)
{
    return 0;
}

ssize_t b_null_write(const char*, size_t n)
{
    return n;
}

static ssize_t console_read(char* buf, size_t buf_size, size_t n)
{
    return kernel::tty::console->read(buf, buf_size, n);
}

static ssize_t console_write(const char* buf, size_t n)
{
    size_t orig_n = n;
    while (n--)
        kernel::tty::console->putchar(*(buf++));

    return orig_n;
}

fs::pipe::pipe(void)
    : buf { PIPE_SIZE }
    , flags { READABLE | WRITABLE }
{
}

void fs::pipe::close_read(void)
{
    kernel::async::lock_guard lck{mtx};
    flags &= (~READABLE);
    waitlist_w.notify_all();
}

void fs::pipe::close_write(void)
{
    kernel::async::lock_guard lck{mtx};
    flags &= (~WRITABLE);
    waitlist_r.notify_all();
}

int fs::pipe::write(const char* buf, size_t n)
{
    // TODO: check privilege
    // TODO: check EPIPE
    kernel::async::lock_guard lck{mtx};

    if (!is_readable()) {
        current_thread->send_signal(SIGPIPE);
        return -EPIPE;
    }

    if (n <= PIPE_SIZE) {
        while (this->buf.avail() < n) {
            bool interrupted = waitlist_w.wait(mtx);
            if (interrupted)
                return -EINTR;

            if (!is_readable()) {
                current_thread->send_signal(SIGPIPE);
                return -EPIPE;
            }
        }

        for (size_t i = 0; i < n; ++i)
            this->buf.put(*(buf++));

        waitlist_r.notify_all();

        return n;
    }

    size_t orig_n = n;
    while (true) {
        bool write = false;
        while (n && !this->buf.full()) {
            --n, this->buf.put(*(buf++));
            write = true;
        }

        if (write)
            waitlist_r.notify_all();

        if (n == 0)
            break;

        bool interrupted = waitlist_w.wait(mtx);
        if (interrupted)
            return -EINTR;

        if (!is_readable()) {
            current_thread->send_signal(SIGPIPE);
            return -EPIPE;
        }
    }

    return orig_n - n;
}

int fs::pipe::read(char* buf, size_t n)
{
    // TODO: check privilege
    kernel::async::lock_guard lck{mtx};
    size_t orig_n = n;

    if (n <= PIPE_SIZE || this->buf.empty()) {
        while (is_writeable() && this->buf.size() < n) {
            bool interrupted = waitlist_r.wait(mtx);
            if (interrupted)
                return -EINTR;

            if (n > PIPE_SIZE)
                break;
        }
    }

    while (!this->buf.empty() && n)
        --n, *(buf++) = this->buf.get();

    waitlist_w.notify_all();
    return orig_n - n;
}

SECTION(".text.kinit")
void init_vfs(void)
{
    using namespace fs;
    // null
    register_char_device(make_device(1, 0), { b_null_read, b_null_write });
    // console (supports serial console only for now)
    // TODO: add interface to bind console device to other devices
    register_char_device(make_device(2, 0), { console_read, console_write });

    // register tmpfs
    fs::register_tmpfs();

    vfs* rootfs;
    int ret = create_fs("none", "/", "tmpfs", MS_NOATIME, nullptr, rootfs);

    assert(ret == 0);
    fs_root = rootfs->root();
}
