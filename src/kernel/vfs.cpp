#include <cstddef>
#include <utility>

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

fs::regular_file::regular_file(file_flags flags, size_t cursor,
                               const struct rust_inode_handle* ind)
    : file(flags), cursor(cursor), ind(ind) {}

ssize_t fs::regular_file::read(char* __user buf, size_t n) {
    if (!flags.read)
        return -EBADF;

    // TODO: copy to user function !IMPORTANT
    ssize_t n_wrote = fs_read(ind, buf, n, cursor, n);
    if (n_wrote >= 0)
        cursor += n_wrote;

    return n_wrote;
}

ssize_t fs::regular_file::do_write(const char* __user buf, size_t n) {
    // TODO: check privilege of user ptr
    ssize_t n_wrote = fs_write(ind, buf, cursor, n);
    if (n_wrote >= 0)
        cursor += n_wrote;

    return n_wrote;
}

off_t fs::regular_file::seek(off_t n, int whence) {
    size_t ind_size = r_get_inode_size(ind);
    size_t pos;
    switch (whence) {
        case SEEK_SET:
            pos = n;
            break;
        case SEEK_CUR:
            pos = cursor + n;
            break;
        case SEEK_END:
            pos = ind_size + n;
            break;
        default:
            return -EINVAL;
    }

    if (pos > ind_size)
        return -EINVAL;

    cursor = pos;

    return cursor;
}

int fs::regular_file::getdents(char* __user buf, size_t cnt) {
    size_t orig_cnt = cnt;
    auto callback = readdir_callback_fn(
        [&buf, &cnt](const char* fn, size_t fnlen,
                     const struct rust_inode_handle*,
                     const struct inode_data* ind, uint8_t type) {
            size_t reclen = sizeof(fs::user_dirent) + 1 + fnlen;
            if (cnt < reclen)
                return -EFAULT;

            auto* dirp = (fs::user_dirent*)buf;
            dirp->d_ino = ind->ino;
            dirp->d_reclen = reclen;
            // TODO: show offset
            // dirp->d_off = 0;
            // TODO: use copy_to_user
            memcpy(dirp->d_name, fn, fnlen);
            buf[reclen - 2] = 0;
            buf[reclen - 1] = type;

            buf += reclen;
            cnt -= reclen;
            return 0;
        });

    int nread = fs_readdir(ind, cursor, &callback);

    if (nread > 0)
        cursor += nread;

    return orig_cnt - cnt;
}

int fs::regular_file::getdents64(char* __user buf, size_t cnt) {
    size_t orig_cnt = cnt;
    auto callback = readdir_callback_fn(
        [&buf, &cnt](const char* fn, size_t fnlen,
                     const struct rust_inode_handle*,
                     const struct inode_data* ind, uint8_t type) {
            size_t reclen = sizeof(fs::user_dirent64) + fnlen;
            if (cnt < reclen)
                return -EFAULT;

            auto* dirp = (fs::user_dirent64*)buf;
            dirp->d_ino = ind->ino;
            dirp->d_off = 114514;
            dirp->d_reclen = reclen;
            dirp->d_type = type;
            // TODO: use copy_to_user
            memcpy(dirp->d_name, fn, fnlen);
            buf[reclen - 1] = 0;

            buf += reclen;
            cnt -= reclen;
            return 0;
        });

    int nread = fs_readdir(ind, cursor, &callback);

    if (nread > 0)
        cursor += nread;

    return orig_cnt - cnt;
}

fs::fifo_file::fifo_file(file_flags flags, std::shared_ptr<fs::pipe> ppipe)
    : file(flags), ppipe(ppipe) {}

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

static fs::chrdev_ops** chrdevs[256];

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

        if (cwd->flags & D_SYMLINK) {
            char linkpath[256];
            int ret = fs_readlink(&cwd->inode, linkpath, sizeof(linkpath));
            if (ret < 0)
                return {nullptr, ret};
            linkpath[ret] = 0;

            std::tie(cwd, ret) =
                fs::open(context, cwd->parent, linkpath, true, recurs_no + 1);
            if (!cwd || ret)
                return {nullptr, ret};
        }

        if (item == ".." && cwd.get() == context.root.get())
            continue;

        if (1) {
            int status;
            std::tie(cwd, status) = d_find(cwd.get(), item);
            if (!cwd)
                return {nullptr, status};
        }

        while (cwd->flags & D_MOUNTPOINT)
            cwd = r_get_mountpoint(cwd.get());
    }

    if (!(cwd->flags & D_PRESENT))
        return {std::move(cwd), -ENOENT};

    if (follow && cwd->flags & D_SYMLINK) {
        char linkpath[256];
        int ret = fs_readlink(&cwd->inode, linkpath, sizeof(linkpath));
        if (ret < 0)
            return {nullptr, ret};
        linkpath[ret] = 0;

        return fs::open(context, cwd->parent, linkpath, true, recurs_no + 1);
    }

    return {std::move(cwd), 0};
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

extern "C" int call_callback(const fs::readdir_callback_fn* func,
                             const char* filename, size_t fnlen,
                             const struct fs::rust_inode_handle* inode,
                             const struct fs::inode_data* idata, uint8_t type) {
    return (*func)(filename, fnlen, inode, idata, type);
}
