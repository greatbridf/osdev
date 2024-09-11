#include <bits/ioctl.h>
#include <errno.h>
#include <poll.h>
#include <sys/mman.h>
#include <unistd.h>

#include <types/path.hpp>

#include <kernel/log.hpp>
#include <kernel/mem/vm_area.hpp>
#include <kernel/process.hpp>
#include <kernel/syscall.hpp>
#include <kernel/vfs.hpp>

#define NOT_IMPLEMENTED not_implemented(__FILE__, __LINE__)

static inline void not_implemented(const char* pos, int line) {
    kmsgf(
        "[kernel] the function at %s:%d is not implemented, killing the "
        "pid%d...",
        pos, line, current_process->pid);
    current_thread->send_signal(SIGSYS);
}

ssize_t kernel::syscall::do_write(int fd, const char __user* buf, size_t n) {
    auto* file = current_process->files[fd];
    if (!file)
        return -EBADF;

    return file->write(buf, n);
}

ssize_t kernel::syscall::do_read(int fd, char __user* buf, size_t n) {
    auto* file = current_process->files[fd];
    if (!file)
        return -EBADF;

    return file->read(buf, n);
}

int kernel::syscall::do_close(int fd) {
    current_process->files.close(fd);
    return 0;
}

int kernel::syscall::do_dup(int old_fd) {
    return current_process->files.dup(old_fd);
}

int kernel::syscall::do_dup2(int old_fd, int new_fd) {
    return current_process->files.dup(old_fd, new_fd, 0);
}

int kernel::syscall::do_pipe(int __user* pipefd) {
    // TODO: use copy_from_user and copy_to_user
    return current_process->files.pipe(*(int(*)[2])pipefd);
}

ssize_t kernel::syscall::do_getdents(int fd, char __user* buf, size_t cnt) {
    auto* dir = current_process->files[fd];
    if (!dir)
        return -EBADF;

    return dir->getdents(buf, cnt);
}

ssize_t kernel::syscall::do_getdents64(int fd, char __user* buf, size_t cnt) {
    auto* dir = current_process->files[fd];
    if (!dir)
        return -EBADF;

    return dir->getdents64(buf, cnt);
}

int kernel::syscall::do_open(const char __user* path, int flags, mode_t mode) {
    mode &= ~current_process->umask;

    // TODO: use copy_from_user
    return current_process->files.open(current_process->cwd.get(), path, flags,
                                       mode);
}

int kernel::syscall::do_symlink(const char __user* target,
                                const char __user* linkpath) {
    // TODO: use copy_from_user
    auto [dent, status] = current_open(linkpath);
    if (!dent || status != -ENOENT)
        return status;

    return fs::symlink(dent.get(), target);
}

int kernel::syscall::do_readlink(const char __user* pathname, char __user* buf,
                                 size_t buf_size) {
    // TODO: use copy_from_user
    auto [dent, status] = current_open(pathname, false);

    if (!dent || status)
        return status;

    if (buf_size <= 0 || !S_ISLNK(dent->inode->mode))
        return -EINVAL;

    // TODO: use copy_to_user
    return fs::readlink(dent->inode, buf, buf_size);
}

int kernel::syscall::do_ioctl(int fd, unsigned long request, uintptr_t arg3) {
    // TODO: check fd type and get tty* from fd
    //
    //       we use a trick for now, check whether
    //       the file that fd points to is a pipe or
    //       not. and we suppose that stdin will be
    //       either a tty or a pipe.
    auto* file = current_process->files[fd];
    if (!file || !S_ISCHR(file->mode))
        return -ENOTTY;

    switch (request) {
        case TIOCGPGRP: {
            auto* pgid = (pid_t __user*)arg3;
            auto* ctrl_tty = current_process->control_tty;

            if (!ctrl_tty)
                return -ENOTTY;

            // TODO: copy_to_user
            *pgid = ctrl_tty->get_pgrp();
            break;
        }
        case TIOCSPGRP: {
            // TODO: copy_from_user
            auto pgid = *(const pid_t __user*)arg3;
            auto* ctrl_tty = current_process->control_tty;

            if (!ctrl_tty)
                return -ENOTTY;

            ctrl_tty->set_pgrp(pgid);
            break;
        }
        case TIOCGWINSZ: {
            auto* ws = (winsize __user*)arg3;
            // TODO: copy_to_user
            ws->ws_col = 80;
            ws->ws_row = 10;
            break;
        }
        case TCGETS: {
            auto* argp = (struct termios __user*)arg3;

            auto* ctrl_tty = current_process->control_tty;
            if (!ctrl_tty)
                return -EINVAL;

            // TODO: use copy_to_user
            memcpy(argp, &ctrl_tty->termio, sizeof(ctrl_tty->termio));

            break;
        }
        case TCSETS: {
            auto* argp = (const struct termios __user*)arg3;

            auto* ctrl_tty = current_process->control_tty;
            if (!ctrl_tty)
                return -EINVAL;

            // TODO: use copy_from_user
            memcpy(&ctrl_tty->termio, argp, sizeof(ctrl_tty->termio));

            break;
        }
        default:
            kmsgf("[error] the ioctl() function %x is not implemented",
                  request);
            return -EINVAL;
    }

    return 0;
}

ssize_t kernel::syscall::do_readv(int fd, const iovec* iov, int iovcnt) {
    auto* file = current_process->files[fd];

    if (!file)
        return -EBADF;

    // TODO: fix fake EOF
    ssize_t totn = 0;
    for (int i = 0; i < iovcnt; ++i) {
        ssize_t ret = file->read((char*)iov[i].iov_base, iov[i].iov_len);

        if (ret < 0)
            return ret;

        if (ret == 0)
            break;

        totn += ret;

        if ((size_t)ret != iov[i].iov_len)
            break;
    }

    return totn;
}

// TODO: this operation SHOULD be atomic
ssize_t kernel::syscall::do_writev(int fd, const iovec* iov, int iovcnt) {
    auto* file = current_process->files[fd];

    if (!file)
        return -EBADF;

    ssize_t totn = 0;
    for (int i = 0; i < iovcnt; ++i) {
        ssize_t ret = file->write((const char*)iov[i].iov_base, iov[i].iov_len);

        if (ret < 0)
            return ret;
        totn += ret;
    }

    return totn;
}

off_t kernel::syscall::do_lseek(int fd, off_t offset, int whence) {
    auto* file = current_process->files[fd];
    if (!file)
        return -EBADF;

    return file->seek(offset, whence);
}

uintptr_t kernel::syscall::do_mmap_pgoff(uintptr_t addr, size_t len, int prot,
                                         int flags, int fd, off_t pgoffset) {
    if (addr & 0xfff)
        return -EINVAL;
    if (len == 0)
        return -EINVAL;

    len = (len + 0xfff) & ~0xfff;

    // TODO: shared mappings
    if (flags & MAP_SHARED)
        return -ENOMEM;

    if (flags & MAP_ANONYMOUS) {
        if (fd != -1)
            return -EINVAL;
        if (pgoffset != 0)
            return -EINVAL;

        // TODO: shared mappings
        if (!(flags & MAP_PRIVATE))
            return -EINVAL;

        auto& mms = current_process->mms;

        // do unmapping, equal to munmap, MAP_FIXED set
        if (prot == PROT_NONE) {
            if (int ret = mms.unmap(addr, len, true); ret != 0)
                return ret;
        } else {
            // TODO: add NULL check in mm_list
            if (!addr || !mms.is_avail(addr, len)) {
                if (flags & MAP_FIXED)
                    return -ENOMEM;
                addr = mms.find_avail(addr, len);
            }

            // TODO: check current cs
            if (addr + len > 0x100000000ULL)
                return -ENOMEM;

            mem::mm_list::map_args args{};
            args.vaddr = addr;
            args.length = len;
            args.flags = mem::MM_ANONYMOUS;

            if (prot & PROT_WRITE)
                args.flags |= mem::MM_WRITE;

            if (prot & PROT_EXEC)
                args.flags |= mem::MM_EXECUTE;

            if (int ret = mms.mmap(args); ret != 0)
                return ret;
        }
    }

    return addr;
}

int kernel::syscall::do_munmap(uintptr_t addr, size_t len) {
    if (addr & 0xfff)
        return -EINVAL;

    return current_process->mms.unmap(addr, len, true);
}

ssize_t kernel::syscall::do_sendfile(int out_fd, int in_fd,
                                     off_t __user* offset, size_t count) {
    auto* out_file = current_process->files[out_fd];
    auto* in_file = current_process->files[in_fd];

    if (!out_file || !in_file)
        return -EBADF;

    // TODO: check whether in_fd supports mmapping
    if (!S_ISREG(in_file->mode) && !S_ISBLK(in_file->mode))
        return -EINVAL;

    if (offset) {
        NOT_IMPLEMENTED;
        return -EINVAL;
    }

    constexpr size_t bufsize = 4096;
    std::vector<char> buf(bufsize);
    size_t totn = 0;
    while (totn < count) {
        if (current_thread->signals.pending_signal() != 0)
            return (totn == 0) ? -EINTR : totn;

        size_t n = std::min(count - totn, bufsize);
        ssize_t ret = in_file->read(buf.data(), n);
        if (ret < 0)
            return ret;
        if (ret == 0)
            break;
        ret = out_file->write(buf.data(), ret);
        if (ret < 0)
            return ret;
        totn += ret;
    }

    return totn;
}

int kernel::syscall::do_statx(int dirfd, const char __user* path, int flags,
                              unsigned int mask, statx __user* statxbuf) {
    // AT_STATX_SYNC_AS_STAT is the default value
    if ((flags & AT_STATX_SYNC_TYPE) != AT_STATX_SYNC_AS_STAT) {
        NOT_IMPLEMENTED;
        return -EINVAL;
    }

    if (dirfd != AT_FDCWD) {
        NOT_IMPLEMENTED;
        return -EINVAL;
    }

    auto [dent, status] = current_open(path, !(flags & AT_SYMLINK_NOFOLLOW));
    if (!dent || status)
        return status;

    // TODO: copy to user
    return fs::statx(dent->inode, statxbuf, mask);
}

int kernel::syscall::do_fcntl(int fd, int cmd, unsigned long arg) {
    auto* file = current_process->files[fd];
    if (!file)
        return -EBADF;

    switch (cmd) {
        case F_SETFD:
            return current_process->files.set_flags(fd, arg);
        case F_DUPFD:
        case F_DUPFD_CLOEXEC: {
            return current_process->files.dupfd(fd, arg, FD_CLOEXEC);
        }
        default:
            NOT_IMPLEMENTED;
            return -EINVAL;
    }
}

int kernel::syscall::do_mkdir(const char __user* pathname, mode_t mode) {
    mode &= (~current_process->umask & 0777);

    // TODO: use copy_from_user
    auto [dent, status] = current_open(pathname);
    if (!dent || status != -ENOENT)
        return status;

    return fs::mkdir(dent.get(), mode);
}

int kernel::syscall::do_truncate(const char __user* pathname, long length) {
    auto [dent, status] = current_open(pathname);
    if (!dent || status)
        return status;

    if (S_ISDIR(dent->inode->mode))
        return -EISDIR;

    return fs::truncate(dent->inode, length);
}

int kernel::syscall::do_unlink(const char __user* pathname) {
    auto [dent, status] = current_open(pathname, false);

    if (!dent || status)
        return status;

    if (S_ISDIR(dent->inode->mode))
        return -EISDIR;

    return fs::unlink(dent.get());
}

int kernel::syscall::do_access(const char __user* pathname, int mode) {
    auto [dent, status] = current_open(pathname);
    if (!dent || status)
        return status;

    switch (mode) {
        case F_OK:
            return 0;
        case R_OK:
        case W_OK:
        case X_OK:
            // TODO: check privilege
            return 0;
        default:
            return -EINVAL;
    }
}

int kernel::syscall::do_mknod(const char __user* pathname, mode_t mode,
                              dev_t dev) {
    mode &= S_IFMT | (~current_process->umask & 0777);
    auto [dent, status] = current_open(pathname);
    if (!dent || status != -ENOENT)
        return status;

    return fs::mknod(dent.get(), mode, dev);
}

int kernel::syscall::do_poll(pollfd __user* fds, nfds_t nfds, int timeout) {
    if (nfds == 0)
        return 0;

    if (nfds > 1) {
        NOT_IMPLEMENTED;
        return -EINVAL;
    }

    // TODO: handle timeout
    // if (timeout != -1) {
    // }
    (void)timeout;

    // for now, we will poll from console only
    int ret = tty::console->poll();
    if (ret < 0)
        return ret;

    fds[0].revents = POLLIN;
    return ret;

    // TODO: check address validity
    // TODO: poll multiple fds and other type of files
    // for (nfds_t i = 0; i < nfds; ++i) {
    //     auto& pfd = fds[i];

    //     auto* file = current_process->files[pfd.fd];
    //     if (!file || !S_ISCHR(file->mode))
    //         return -EINVAL;

    //     // poll the fds
    // }
    //
    // return 0;
}

/* TODO: implement vfs_stat(stat*)
int do_stat(const char __user* pathname, stat __user* buf)
{
    auto* dent = fs::vfs_open(*current_process->root,
        types::make_path(pathname, current_process->pwd));

    if (!dent)
        return -ENOENT;

    return fs::vfs_stat(dent, buf);
}
*/

/* TODO: implement vfs_stat(stat*)
int do_fstat(int fd, stat __user* buf)
{
    auto* file = current_process->files[fd];
    if (!file)
        return -EBADF;

    return fs::vfs_stat(file, buf);
}
*/
