#include <cstddef>

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

fs::regular_file::regular_file(file_flags flags, size_t cursor, dentry_pointer dentry)
    : file(flags), cursor(cursor), dentry(std::move(dentry)) {}

ssize_t fs::regular_file::read(char* __user buf, size_t n) {
    if (!flags.read)
        return -EBADF;

    // TODO: copy to user function !IMPORTANT
    ssize_t n_wrote = fs_read(dentry.get(), buf, n, cursor, n);
    if (n_wrote >= 0)
        cursor += n_wrote;

    return n_wrote;
}

ssize_t fs::regular_file::do_write(const char* __user buf, size_t n) {
    // TODO: check privilege of user ptr
    ssize_t n_wrote = fs_write(dentry.get(), buf, cursor, n);
    if (n_wrote >= 0)
        cursor += n_wrote;

    return n_wrote;
}

off_t fs::regular_file::seek(off_t n, int whence) {
    size_t ind_size = r_dentry_get_size(dentry.get());
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
    auto callback = readdir_callback_fn([&buf, &cnt](const char* fn, size_t fnlen, ino_t ino) {
        size_t reclen = sizeof(fs::user_dirent) + 1 + fnlen;
        if (cnt < reclen)
            return -EFAULT;

        auto* dirp = (fs::user_dirent*)buf;
        dirp->d_ino = ino;
        dirp->d_reclen = reclen;
        // TODO: show offset
        // dirp->d_off = 0;
        // TODO: use copy_to_user
        memcpy(dirp->d_name, fn, fnlen);
        buf[reclen - 2] = 0;
        buf[reclen - 1] = 0;

        buf += reclen;
        cnt -= reclen;
        return 0;
    });

    int nread = fs_readdir(dentry.get(), cursor, &callback);

    if (nread > 0)
        cursor += nread;

    return orig_cnt - cnt;
}

int fs::regular_file::getdents64(char* __user buf, size_t cnt) {
    size_t orig_cnt = cnt;
    auto callback = readdir_callback_fn([&buf, &cnt](const char* fn, size_t fnlen, ino_t ino) {
        size_t reclen = sizeof(fs::user_dirent64) + fnlen;
        if (cnt < reclen)
            return -EFAULT;

        auto* dirp = (fs::user_dirent64*)buf;
        dirp->d_ino = ino;
        dirp->d_off = 114514;
        dirp->d_reclen = reclen;
        dirp->d_type = 0;
        // TODO: use copy_to_user
        memcpy(dirp->d_name, fn, fnlen);
        buf[reclen - 1] = 0;

        buf += reclen;
        cnt -= reclen;
        return 0;
    });

    int nread = fs_readdir(dentry.get(), cursor, &callback);

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

    while (is_writeable() && this->buf.size() == 0) {
        bool interrupted = waitlist_r.wait(mtx);
        if (interrupted)
            return -EINTR;
    }

    while (!this->buf.empty() && n)
        --n, *(buf++) = this->buf.get();

    waitlist_w.notify_all();
    return orig_n - n;
}

extern "C" int call_callback(const fs::readdir_callback_fn* func, const char* filename,
                             size_t fnlen, ino_t ino) {
    return (*func)(filename, fnlen, ino);
}

extern "C" struct dentry* dentry_open(struct dentry* context_root, struct dentry* cwd,
                                      const char* path, size_t path_length, bool follow);

std::pair<fs::dentry_pointer, int> fs::open(const fs::fs_context& context,
                                            const fs::dentry_pointer& cwd, types::string_view path,
                                            bool follow_symlinks) {
    auto result =
        dentry_open(context.root.get(), cwd.get(), path.data(), path.size(), follow_symlinks);
    auto result_int = reinterpret_cast<intptr_t>(result);

    if (result_int > -128)
        return {nullptr, result_int};

    if (fs::r_dentry_is_invalid(result))
        return {result, -ENOENT};

    return {result, 0};
}

extern "C" void r_dput(struct dentry* dentry);
extern "C" struct dentry* r_dget(struct dentry* dentry);

void fs::dentry_deleter::operator()(struct dentry* dentry) const {
    if (dentry)
        r_dput(dentry);
}

fs::dentry_pointer fs::d_get(const dentry_pointer& dp) {
    if (!dp)
        return nullptr;

    return dentry_pointer{r_dget(dp.get())};
}
