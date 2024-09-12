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

using fs::dentry;

fs::regular_file::regular_file(file_flags flags, size_t cursor,
                               struct inode* ind)
    : file(ind->mode, flags), cursor(cursor), ind(ind) {}

ssize_t fs::regular_file::read(char* __user buf, size_t n) {
    if (!flags.read)
        return -EBADF;

    if (S_ISDIR(ind->mode))
        return -EISDIR;

    // TODO: copy to user function !IMPORTANT
    ssize_t n_wrote = fs::read(ind, buf, n, cursor, n);
    if (n_wrote >= 0)
        cursor += n_wrote;

    return n_wrote;
}

ssize_t fs::regular_file::do_write(const char* __user buf, size_t n) {
    if (S_ISDIR(mode))
        return -EISDIR;

    // TODO: check privilege of user ptr
    ssize_t n_wrote = fs::write(ind, buf, cursor, n);
    if (n_wrote >= 0)
        cursor += n_wrote;

    return n_wrote;
}

off_t fs::regular_file::seek(off_t n, int whence) {
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

int fs::regular_file::getdents(char* __user buf, size_t cnt) {
    if (!S_ISDIR(ind->mode))
        return -ENOTDIR;

    size_t orig_cnt = cnt;
    int nread = ind->fs->readdir(
        ind, cursor,
        [&buf, &cnt](const char* fn, struct inode* ind, uint8_t type) {
            size_t filename_len = strlen(fn);

            size_t reclen = sizeof(fs::user_dirent) + 1 + filename_len;
            if (cnt < reclen)
                return -EFAULT;

            auto* dirp = (fs::user_dirent*)buf;
            dirp->d_ino = ind->ino;
            dirp->d_reclen = reclen;
            // TODO: show offset
            // dirp->d_off = 0;
            // TODO: use copy_to_user
            memcpy(dirp->d_name, fn, filename_len);
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

int fs::regular_file::getdents64(char* __user buf, size_t cnt) {
    if (!S_ISDIR(ind->mode))
        return -ENOTDIR;

    size_t orig_cnt = cnt;
    int nread = ind->fs->readdir(
        ind, cursor,
        [&buf, &cnt](const char* fn, struct inode* ind, uint8_t type) {
            size_t filename_len = strlen(fn);

            size_t reclen = sizeof(fs::user_dirent64) + filename_len;
            if (cnt < reclen)
                return -EFAULT;

            auto* dirp = (fs::user_dirent64*)buf;
            dirp->d_ino = ind->ino;
            dirp->d_off = 114514;
            dirp->d_reclen = reclen;
            dirp->d_type = type;
            // TODO: use copy_to_user
            memcpy(dirp->d_name, fn, filename_len);
            buf[reclen - 1] = 0;

            buf += reclen;
            cnt -= reclen;
            return 0;
        });

    if (nread > 0)
        cursor += nread;

    return orig_cnt - cnt;
}

fs::fifo_file::fifo_file(file_flags flags, std::shared_ptr<fs::pipe> ppipe)
    : file(S_IFIFO, flags), ppipe(ppipe) {}

ssize_t fs::fifo_file::read(char* __user buf, size_t n) {
    if (!flags.read)
        return -EBADF;

    return ppipe->read(buf, n);
}

ssize_t fs::fifo_file::do_write(const char* __user buf, size_t n) {
    return ppipe->write(buf, n);
}

fs::fifo_file::~fifo_file() {
    assert(flags.read ^ flags.write);
    if (flags.read)
        ppipe->close_read();
    else
        ppipe->close_write();
}

static fs::blkdev_ops** blkdevs[256];
static fs::chrdev_ops** chrdevs[256];

size_t fs::read(struct fs::inode* file, char* buf, size_t buf_size,
                size_t offset, size_t n) {
    if (S_ISDIR(file->mode)) {
        errno = EISDIR;
        return -1U;
    }

    if (S_ISREG(file->mode))
        return file->fs->read(file, buf, buf_size, n, offset);

    if (S_ISBLK(file->mode) || S_ISCHR(file->mode)) {
        dev_t dev = file->fs->i_device(file);
        if (dev & 0x80000000) {
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
size_t fs::write(struct fs::inode* file, const char* buf, size_t offset,
                 size_t n) {
    if (S_ISDIR(file->mode)) {
        errno = EISDIR;
        return -1U;
    }

    if (S_ISREG(file->mode))
        return file->fs->write(file, buf, n, offset);

    if (S_ISBLK(file->mode) || S_ISCHR(file->mode)) {
        dev_t dev = file->fs->i_device(file);
        if (dev & 0x80000000) {
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

std::pair<fs::dentry_pointer, int> fs::open(const fs_context& context,
                                            dentry* _cwd,
                                            types::path_iterator path,
                                            bool follow, int recurs_no) {
    // too many recursive search layers will cause stack overflow
    // so we use 16 for now
    if (recurs_no >= 16)
        return {nullptr, -ELOOP};

    dentry_pointer cwd{path.is_absolute() ? d_get(context.root) : d_get(_cwd)};

    for (; path; ++path) {
        auto item = *path;
        if (item.empty() || item == ".")
            continue;

        if (!(cwd->flags & D_PRESENT))
            return {nullptr, -ENOENT};

        assert(cwd->inode);
        if (S_ISLNK(cwd->inode->mode)) {
            char linkpath[256];
            int ret = fs::readlink(cwd->inode, linkpath, sizeof(linkpath));
            if (ret < 0)
                return {nullptr, ret};
            linkpath[ret] = 0;

            std::tie(cwd, ret) =
                fs::open(context, cwd->parent, linkpath, true, recurs_no + 1);
            if (!cwd || ret)
                return {nullptr, ret};
        }

        if (1) {
            int status;
            std::tie(cwd, status) = d_find(cwd.get(), item);
            if (!cwd)
                return {nullptr, status};
        }

        while (cwd->flags & D_MOUNTPOINT) {
            auto iter = mounts.find(cwd.get());
            assert(iter);

            auto* fs = iter->second.fs;
            assert(fs);

            cwd = d_get(fs->root());
        }
    }

    if (!(cwd->flags & D_PRESENT))
        return {std::move(cwd), -ENOENT};

    if (follow && S_ISLNK(cwd->inode->mode)) {
        char linkpath[256];
        int ret = fs::readlink(cwd->inode, linkpath, sizeof(linkpath));
        if (ret < 0)
            return {nullptr, ret};
        linkpath[ret] = 0;

        return fs::open(context, cwd->parent, linkpath, true, recurs_no + 1);
    }

    return {std::move(cwd), 0};
}

int fs::statx(struct inode* inode, struct statx* stat, unsigned int mask) {
    assert(inode && inode->fs);
    return inode->fs->statx(inode, stat, mask);
}

int fs::truncate(struct inode* file, size_t size) {
    assert(file && file->fs);
    return file->fs->truncate(file, size);
}

int fs::readlink(struct inode* inode, char* buf, size_t size) {
    assert(inode && inode->fs);
    return inode->fs->readlink(inode, buf, size);
}

int fs::symlink(struct dentry* at, const char* target) {
    assert(at && at->parent && at->parent->fs);
    return at->parent->fs->symlink(at->parent->inode, at, target);
}

int fs::unlink(struct dentry* at) {
    assert(at && at->parent && at->parent->fs);
    return at->parent->fs->unlink(at->parent->inode, at);
}

int fs::mknod(struct dentry* at, mode_t mode, dev_t dev) {
    assert(at && at->parent && at->parent->fs);
    return at->parent->fs->mknod(at->parent->inode, at, mode, dev);
}

int fs::creat(struct dentry* at, mode_t mode) {
    assert(at && at->parent && at->parent->fs);
    return at->parent->fs->creat(at->parent->inode, at, mode);
}

int fs::mount(struct dentry* mnt, const char* source, const char* mount_point,
              const char* fstype, unsigned long flags, const void* data) {
    assert(mnt && mnt->fs);
    return mnt->fs->mount(mnt, source, mount_point, fstype, flags, data);
}

int fs::mkdir(struct dentry* at, mode_t mode) {
    assert(at && at->parent && at->parent->fs);
    return at->parent->fs->mkdir(at->parent->inode, at, mode);
}

int mount(dentry* mnt, const char* source, const char* mount_point,
          const char* fstype, unsigned long flags, const void* data) {
    assert(mnt && mnt->fs);
    return mnt->fs->mount(mnt, source, mount_point, fstype, flags, data);
}

int fs::register_block_device(dev_t node, const fs::blkdev_ops& ops) {
    int major = NODE_MAJOR(node);
    int minor = NODE_MINOR(node);

    if (!blkdevs[major])
        blkdevs[major] = new blkdev_ops* [256] {};

    if (blkdevs[major][minor])
        return -EEXIST;

    blkdevs[major][minor] = new blkdev_ops{ops};
    return 0;
}

int fs::register_char_device(dev_t node, const fs::chrdev_ops& ops) {
    int major = NODE_MAJOR(node);
    int minor = NODE_MINOR(node);

    if (!chrdevs[major])
        chrdevs[major] = new chrdev_ops* [256] {};

    if (chrdevs[major][minor])
        return -EEXIST;

    chrdevs[major][minor] = new chrdev_ops{ops};
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
static int mbr_part_probe(dev_t node) {
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
        fs::register_block_device(
            node + n,
            {[=](char* buf, size_t buf_size, size_t offset,
                 size_t n) -> ssize_t {
                 offset += part_offset;
                 return fs::block_device_read(node, buf, buf_size, offset, n);
             },
             [=](const char* buf, size_t offset, size_t n) -> ssize_t {
                 offset += part_offset;
                 return fs::block_device_write(node, buf, offset, n);
             }});

        ++n;
    }

    return 0;
}

void fs::partprobe() {
    for (int i = 0; i < 256; i += 16) {
        int ret = mbr_part_probe(make_device(8, i));

        if (ret != 0)
            continue;

        kmsgf("[info] found disk drive sd%c\n", 'a' + (i / 16));
    }
}

ssize_t fs::block_device_read(dev_t node, char* buf, size_t buf_size,
                              size_t offset, size_t n) {
    int major = NODE_MAJOR(node);
    int minor = NODE_MINOR(node);

    if (!blkdevs[major] || !blkdevs[major][minor])
        return -EINVAL;

    auto& read = blkdevs[major][minor]->read;
    if (!read)
        return -EINVAL;

    return read(buf, buf_size, offset, n);
}

ssize_t fs::block_device_write(dev_t node, const char* buf, size_t offset,
                               size_t n) {
    int major = NODE_MAJOR(node);
    int minor = NODE_MINOR(node);

    if (!blkdevs[major] || !blkdevs[major][minor])
        return -EINVAL;

    auto& write = blkdevs[major][minor]->write;
    if (!write)
        return -EINVAL;

    return write(buf, offset, n);
}

ssize_t fs::char_device_read(dev_t node, char* buf, size_t buf_size, size_t n) {
    int major = NODE_MAJOR(node);
    int minor = NODE_MINOR(node);

    if (!chrdevs[major] || !chrdevs[major][minor])
        return -EINVAL;

    auto& read = chrdevs[major][minor]->read;
    if (!read)
        return -EINVAL;

    return read(buf, buf_size, n);
}

ssize_t fs::char_device_write(dev_t node, const char* buf, size_t n) {
    int major = NODE_MAJOR(node);
    int minor = NODE_MINOR(node);

    if (!chrdevs[major] || !chrdevs[major][minor])
        return -EINVAL;

    auto& write = chrdevs[major][minor]->write;
    if (!write)
        return -EINVAL;

    return write(buf, n);
}

fs::pipe::pipe(void) : buf{PIPE_SIZE}, flags{READABLE | WRITABLE} {}

void fs::pipe::close_read(void) {
    kernel::async::lock_guard lck{mtx};
    flags &= (~READABLE);
    waitlist_w.notify_all();
}

void fs::pipe::close_write(void) {
    kernel::async::lock_guard lck{mtx};
    flags &= (~WRITABLE);
    waitlist_r.notify_all();
}

int fs::pipe::write(const char* buf, size_t n) {
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

int fs::pipe::read(char* buf, size_t n) {
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
