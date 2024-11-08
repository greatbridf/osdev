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

uintptr_t kernel::syscall::do_mmap_pgoff(uintptr_t addr, size_t len, int prot, int flags, int fd,
                                         off_t pgoffset) {
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

int kernel::syscall::do_socket(int domain, int type, int protocol) {
    return -EINVAL;
}
