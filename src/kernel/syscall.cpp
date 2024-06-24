#include <assert.h>
#include <bits/alltypes.h>
#include <bits/ioctl.h>
#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/prctl.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <sys/utsname.h>
#include <sys/wait.h>
#include <termios.h>
#include <time.h>
#include <unistd.h>

#include <types/allocator.hpp>
#include <types/elf.hpp>
#include <types/path.hpp>
#include <types/types.h>

#include <kernel/async/lock.hpp>
#include <kernel/hw/timer.hpp>
#include <kernel/interrupt.hpp>
#include <kernel/log.hpp>
#include <kernel/process.hpp>
#include <kernel/signal.hpp>
#include <kernel/syscall.hpp>
#include <kernel/task/readyqueue.hpp>
#include <kernel/task/thread.hpp>
#include <kernel/tty.hpp>
#include <kernel/user/thread_local.hpp>
#include <kernel/utsname.hpp>
#include <kernel/vfs.hpp>

#define SYSCALL_NO ((data)->s_regs.eax)
#define SYSCALL_RETVAL ((data)->s_regs.eax)

#define SYSCALL_HANDLERS_SIZE (404)
syscall_handler syscall_handlers[SYSCALL_HANDLERS_SIZE];

static void not_implemented(const char* pos, int line)
{
    kmsgf("[kernel] the function at %s:%d is not implemented, killing the pid%d...",
            pos, line, current_process->pid);
    current_thread->send_signal(SIGSYS);
}

#define NOT_IMPLEMENTED not_implemented(__FILE__, __LINE__)

extern "C" void _syscall_stub_fork_return(void);
long _syscall_fork(interrupt_stack* data)
{
    auto& newproc = procs->copy_from(*current_process);
    auto [ iter_newthd, inserted ] = newproc.thds.emplace(*current_thread, newproc.pid);
    assert(inserted);
    auto* newthd = &*iter_newthd;

    kernel::task::dispatcher::enqueue(newthd);

    uint32_t newthd_oldesp = (uintptr_t)newthd->kstack.esp;
    auto esp = &newthd->kstack.esp;

    // create fake interrupt stack
    push_stack(esp, data->ss);
    push_stack(esp, data->rsp);
    push_stack(esp, data->flags);
    push_stack(esp, data->cs);
    push_stack(esp, (uint64_t)data->v_rip);

    // TODO: LONG MODE
    // eax
    // push_stack(esp, 0);
    // push_stack(esp, data->s_regs.ecx);
    // edx
    // push_stack(esp, 0);
    // push_stack(esp, data->s_regs.ebx);
    // push_stack(esp, data->s_regs.esp);
    // push_stack(esp, data->s_regs.ebp);
    // push_stack(esp, data->s_regs.esi);
    // push_stack(esp, data->s_regs.edi);

    // ctx_switch stack
    // return address
    push_stack(esp, (uintptr_t)_syscall_stub_fork_return);
    // ebx
    push_stack(esp, 0);
    // edi
    push_stack(esp, 0);
    // esi
    push_stack(esp, 0);
    // ebp
    push_stack(esp, 0);
    // eflags
    push_stack(esp, 0);
    // original
    push_stack(esp, newthd_oldesp);

    return newproc.pid;
}

long _syscall_write(interrupt_stack* data)
{
    SYSCALL_ARG1(int, fd);
    SYSCALL_ARG2(const char* __user, buf);
    SYSCALL_ARG3(size_t, n);

    auto* file = current_process->files[fd];
    if (!file)
        return -EBADF;

    return file->write(buf, n);
}

long _syscall_read(interrupt_stack* data)
{
    SYSCALL_ARG1(int, fd);
    SYSCALL_ARG2(char* __user, buf);
    SYSCALL_ARG3(size_t, n);

    auto* file = current_process->files[fd];
    if (!file)
        return -EBADF;

    return file->read(buf, n);
}

// TODO: sleep seconds
long _syscall_sleep(interrupt_stack*)
{
    current_thread->set_attr(kernel::task::thread::USLEEP);

    schedule();
    return 0;
}

long _syscall_chdir(interrupt_stack* data)
{
    SYSCALL_ARG1(const char*, path);

    auto* dir = fs::vfs_open(*current_process->root,
            current_process->pwd + path);
    if (!dir)
        return -ENOENT;

    if (!S_ISDIR(dir->ind->mode))
        return -ENOTDIR;

    current_process->pwd.clear();
    dir->path(*current_process->root, current_process->pwd);

    return 0;
}

// syscall_exec(const char* exec, const char** argv)
// @param exec: the path of program to execute
// @param argv: arguments end with nullptr
// @param envp: environment variables end with nullptr
long _syscall_execve(interrupt_stack* data)
{
    SYSCALL_ARG1(const char*, exec);
    SYSCALL_ARG2(char* const*, argv);
    SYSCALL_ARG3(char* const*, envp);

    types::elf::elf32_load_data d;
    d.argv = argv;
    d.envp = envp;
    d.system = false;

    d.exec_dent = fs::vfs_open(*current_process->root,
            current_process->pwd + exec);

    if (!d.exec_dent)
        return -ENOENT;

    current_process->files.onexec();

    int ret = types::elf::elf32_load(&d);
    if (ret != 0)
        return -d.errcode;

    data->v_rip = d.eip;
    data->rsp = (uintptr_t)d.sp;

    current_thread->signals.on_exec();

    return 0;
}

// @param exit_code
long NORETURN _syscall_exit(interrupt_stack* data)
{
    SYSCALL_ARG1(int, exit_code);

    // TODO: terminating a thread only
    if (current_process->thds.size() != 1)
        assert(false);

    // terminating a whole process:
    procs->kill(current_process->pid, (exit_code & 0xff) << 8);

    // switch to new process and continue
    schedule_noreturn();
}

long NORETURN _syscall_exit_group(interrupt_stack* data)
{
    // we implement exit_group as exit for now
    _syscall_exit(data);
}

// @param pid: pid of the process to wait
// @param status: the exit code of the exited process
// @param options: options for waitpid
// @return pid of the exited process
long _syscall_waitpid(interrupt_stack* data)
{
    SYSCALL_ARG1(pid_t, pid_to_wait);
    SYSCALL_ARG2(int*, arg1);
    SYSCALL_ARG3(int, options);

    if (pid_to_wait != -1)
        return -EINVAL;

    auto& cv = current_process->waitlist;
    kernel::async::lock_guard lck(current_process->mtx_waitprocs);

    auto& waitlist = current_process->waitprocs;

    // TODO: check if it is waiting for stopped process
    if (options & ~(WNOHANG | WUNTRACED)) {
        NOT_IMPLEMENTED;
        return -EINVAL;
    }

    while (waitlist.empty()) {
        if (current_process->children.empty())
            return -ECHILD;

        if (options & WNOHANG)
            return 0;

        bool interrupted = cv.wait(current_process->mtx_waitprocs);
        if (interrupted)
            return -EINTR;
    }

    for (auto iter = waitlist.begin(); iter != waitlist.end(); ++iter) {
        if (WIFSTOPPED(iter->code) && !(options & WUNTRACED))
            continue;

        pid_t pid = iter->pid;

        // TODO: copy_to_user check privilege
        *arg1 = iter->code;

        procs->remove(pid);
        waitlist.erase(iter);

        return pid;
    }

    // we should never reach here
    assert(false);
    return -EINVAL;
}

long _syscall_wait4(interrupt_stack* data)
{
    SYSCALL_ARG4(void* __user, rusage);

    // TODO: getrusage
    if (rusage)
        return -EINVAL;

    return _syscall_waitpid(data);
}

long _syscall_getdents(interrupt_stack* data)
{
    SYSCALL_ARG1(int, fd);
    SYSCALL_ARG2(char* __user, buf);
    SYSCALL_ARG3(size_t, cnt);

    auto* dir = current_process->files[fd];
    if (!dir)
        return -EBADF;

    return dir->getdents(buf, cnt);
}

long _syscall_open(interrupt_stack* data)
{
    SYSCALL_ARG1(const char* __user, path);
    SYSCALL_ARG2(int, flags);
    SYSCALL_ARG3(mode_t, mode);

    mode &= ~current_process->umask;

    return current_process->files.open(*current_process,
        current_process->pwd + path, flags, mode);
}

long _syscall_getcwd(interrupt_stack* data)
{
    SYSCALL_ARG1(char*, buf);
    SYSCALL_ARG2(size_t, bufsize);

    // TODO: use copy_to_user
    auto path = current_process->pwd.full_path();
    strncpy(buf, path.c_str(), bufsize);
    buf[bufsize - 1] = 0;

    return (uintptr_t)buf;
}

long _syscall_setsid(interrupt_stack*)
{
    if (current_process->pid == current_process->pgid)
        return -EPERM;

    current_process->sid = current_process->pid;
    current_process->pgid = current_process->pid;

    // TODO: get tty* from fd or block device id
    console->set_pgrp(current_process->pid);
    current_process->control_tty = console;

    return current_process->pid;
}

long _syscall_getsid(interrupt_stack* data)
{
    SYSCALL_ARG1(pid_t, pid);

    auto [ pproc, found ] = procs->try_find(pid);
    if (!found)
        return -ESRCH;
    if (pproc->sid != current_process->sid)
        return -EPERM;

    return pproc->sid;
}

long _syscall_close(interrupt_stack* data)
{
    SYSCALL_ARG1(int, fd);
    current_process->files.close(fd);
    return 0;
}

long _syscall_dup(interrupt_stack* data)
{
    SYSCALL_ARG1(int, old_fd);
    return current_process->files.dup(old_fd);
}

long _syscall_dup2(interrupt_stack* data)
{
    SYSCALL_ARG1(int, old_fd);
    SYSCALL_ARG2(int, new_fd);
    return current_process->files.dup2(old_fd, new_fd);
}

long _syscall_pipe(interrupt_stack* data)
{
    SYSCALL_ARG1(int* __user, pipefd);
    return current_process->files.pipe(pipefd);
}

long _syscall_setpgid(interrupt_stack* data)
{
    SYSCALL_ARG1(pid_t, pid);
    SYSCALL_ARG2(pid_t, pgid);

    if (pgid < 0)
        return -EINVAL;

    if (pid == 0)
        pid = current_process->pid;

    if (pgid == 0)
        pgid = pid;

    auto [ pproc, found ] = procs->try_find(pid);
    if (!found)
        return -ESRCH;

    // TODO: check whether pgid and the original
    //       pgid is in the same session

    pproc->pgid = pgid;

    return 0;
}

long _syscall_ioctl(interrupt_stack* data)
{
    SYSCALL_ARG1(int, fd);
    SYSCALL_ARG2(unsigned long, request);

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
        SYSCALL_ARG3(pid_t*, pgid);
        tty* ctrl_tty = current_process->control_tty;

        if (!ctrl_tty)
            return -ENOTTY;

        // TODO: copy_to_user
        *pgid = ctrl_tty->get_pgrp();
        break;
    }
    case TIOCSPGRP: {
        // TODO: copy_from_user
        SYSCALL_ARG3(const pid_t*, pgid);
        tty* ctrl_tty = current_process->control_tty;

        if (!ctrl_tty)
            return -ENOTTY;

        ctrl_tty->set_pgrp(*pgid);
        break;
    }
    case TIOCGWINSZ: {
        SYSCALL_ARG3(winsize*, ws);
        ws->ws_col = 80;
        ws->ws_row = 10;
        break;
    }
    case TCGETS: {
        SYSCALL_ARG3(struct termios* __user, argp);

        tty* ctrl_tty = current_process->control_tty;
        if (!ctrl_tty)
            return -EINVAL;

        // TODO: use copy_to_user
        memcpy(argp, &ctrl_tty->termio, sizeof(ctrl_tty->termio));

        break;
    }
    case TCSETS: {
        SYSCALL_ARG3(const struct termios* __user, argp);

        tty* ctrl_tty = current_process->control_tty;
        if (!ctrl_tty)
            return -EINVAL;

        // TODO: use copy_from_user
        memcpy(&ctrl_tty->termio, argp, sizeof(ctrl_tty->termio));

        break;
    }
    default:
        kmsgf("[error] the ioctl() function %x is not implemented", request);
        return -EINVAL;
    }

    return 0;
}

long _syscall_getpid(interrupt_stack*)
{
    return current_process->pid;
}

long _syscall_getppid(interrupt_stack*)
{
    return current_process->ppid;
}

long _syscall_set_thread_area(interrupt_stack* data)
{
    SYSCALL_ARG1(kernel::user::user_desc* __user, ptr);

    auto ret = current_thread->set_thread_area(ptr);
    if (ret != 0)
        return ret;

    current_thread->load_thread_area();
    return 0;
}

long _syscall_set_tid_address(interrupt_stack* data)
{
    SYSCALL_ARG1(int* __user, tidptr);
    current_thread->set_child_tid = tidptr;
    return current_thread->tid();
}

long _syscall_readv(interrupt_stack* data)
{
    SYSCALL_ARG1(int, fd);
    SYSCALL_ARG2(const iovec* __user, iov);
    SYSCALL_ARG3(int, iovcnt);

    auto* file = current_process->files[fd];

    if (!file)
        return -EBADF;

    // TODO: fix fake EOF
    ssize_t totn = 0;
    for (int i = 0; i < iovcnt; ++i) {
        ssize_t ret = file->read(
            (char*)iov[i].iov_base, iov[i].iov_len);

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
long _syscall_writev(interrupt_stack* data)
{
    SYSCALL_ARG1(int, fd);
    SYSCALL_ARG2(const iovec* __user, iov);
    SYSCALL_ARG3(int, iovcnt);

    auto* file = current_process->files[fd];

    if (!file)
        return -EBADF;

    ssize_t totn = 0;
    for (int i = 0; i < iovcnt; ++i) {
        ssize_t ret = file->write(
            (const char*)iov[i].iov_base, iov[i].iov_len);

        if (ret < 0)
            return ret;
        totn += ret;
    }

    return totn;
}

long _syscall_prctl(interrupt_stack* data)
{
    SYSCALL_ARG1(int, option);

    switch (option) {
    case PR_SET_NAME: {
        // TODO: copy_from_user or check privilege
        SYSCALL_ARG2(const char* __user, name);
        current_thread->name.assign(name, 15);
        break;
    }
    case PR_GET_NAME: {
        SYSCALL_ARG2(char* __user, name);
        // TODO: copy_to_user
        strncpy(name, current_thread->name.c_str(), 16);
        name[15] = 0;
        break;
    }
    default:
        return -EINVAL;
    }

    return 0;
}

long _syscall_clock_gettime64(interrupt_stack* data)
{
    SYSCALL_ARG1(clockid_t, clk_id);
    SYSCALL_ARG2(timespec* __user, tp);

    // TODO: check privilege of tp
    if ((clk_id != CLOCK_REALTIME && clk_id != CLOCK_MONOTONIC) || !tp) {
        NOT_IMPLEMENTED;
        return -EINVAL;
    }

    auto time = kernel::hw::timer::current_ticks();
    tp->tv_sec = time / 100;
    tp->tv_nsec = 10000000 * (time % 100);

    return 0;
}

long _syscall_getuid(interrupt_stack*)
{
    return 0; // all user are root for now
}

long _syscall_geteuid(interrupt_stack*)
{
    return 0; // all user are root for now
}

long _syscall_brk(interrupt_stack* data)
{
    SYSCALL_ARG1(void*, addr);

    return (uintptr_t)current_process->mms.set_brk(addr);
}

long _syscall_mmap_pgoff(interrupt_stack* data)
{
    SYSCALL_ARG1(void*, addr);
    SYSCALL_ARG2(size_t, len);
    SYSCALL_ARG3(int, prot);
    SYSCALL_ARG4(int, flags);
    SYSCALL_ARG5(int, fd);
    SYSCALL_ARG6(off_t, pgoffset);

    if ((uintptr_t)addr % 4096 != 0)
        return -EINVAL;
    if (len == 0)
        return -EINVAL;

    len = align_up<12>(len);

    // TODO: shared mappings
    if (flags & MAP_SHARED)
        return -ENOMEM;

    if (flags & MAP_ANONYMOUS) {
        if (fd != -1)
            return -EINVAL;
        if (pgoffset != 0)
            return -EINVAL;

        if (!(flags & MAP_PRIVATE))
            return -EINVAL;

        auto& mms = current_process->mms;

        // do unmapping, equal to munmap, MAP_FIXED set
        if (prot == PROT_NONE) {
            auto ret = mms.unmap(addr, len, false);
            if (ret != 0)
                return ret;
        }
        else {
            // TODO: add NULL check in mm_list
            if (!addr || !mms.is_avail(addr, len)) {
                if (flags & MAP_FIXED)
                    return -ENOMEM;
                addr = mms.find_avail(addr, len, false);
            }

            // TODO: append pages to the end of area if possible
            mms.add_empty_area(addr, len / 4096,
                PAGE_COW, prot & PROT_WRITE, false);
        }
    }

    return (uintptr_t)addr;
}

long _syscall_munmap(interrupt_stack* data)
{
    SYSCALL_ARG1(void*, addr);
    SYSCALL_ARG2(size_t, len);

    if ((uintptr_t)addr % 4096 != 0)
        return -EINVAL;

    return current_process->mms.unmap(addr, len, false);
}

long _syscall_sendfile64(interrupt_stack* data)
{
    SYSCALL_ARG1(int, out_fd);
    SYSCALL_ARG2(int, in_fd);
    SYSCALL_ARG3(off64_t*, offset);
    SYSCALL_ARG4(size_t, count);

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

        // TODO: this won't work, since when we are in the syscall handler,
        //       interrupts are blocked.
        //       one solution is to put the sendfile action into a kernel
        //       worker and pause the calling thread so that the worker
        //       thread could be interrupted normally.
    }

    return totn;
}

long _syscall_statx(interrupt_stack* data)
{
    SYSCALL_ARG1(int, dirfd);
    SYSCALL_ARG2(const char* __user, path);
    SYSCALL_ARG3(int, flags);
    SYSCALL_ARG4(unsigned int, mask);
    SYSCALL_ARG5(statx* __user, statxbuf);

    // AT_STATX_SYNC_AS_STAT is the default value
    if ((flags & AT_STATX_SYNC_TYPE) != AT_STATX_SYNC_AS_STAT) {
        NOT_IMPLEMENTED;
        return -EINVAL;
    }

    if (dirfd != AT_FDCWD) {
        NOT_IMPLEMENTED;
        return -EINVAL;
    }

    auto* dent = fs::vfs_open(*current_process->root,
            current_process->pwd + path,
            !(flags & AT_SYMLINK_NOFOLLOW));

    if (!dent)
        return -ENOENT;

    // TODO: copy to user
    auto ret = fs::vfs_stat(dent, statxbuf, mask);

    return ret;
}

long _syscall_fcntl64(interrupt_stack* data)
{
    SYSCALL_ARG1(int, fd);
    SYSCALL_ARG2(int, cmd);
    SYSCALL_ARG3(unsigned long, arg);

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

long _syscall_getdents64(interrupt_stack* data)
{
    SYSCALL_ARG1(int, fd);
    SYSCALL_ARG2(char* __user, buf);
    SYSCALL_ARG3(size_t, cnt);

    auto* dir = current_process->files[fd];
    if (!dir)
        return -EBADF;

    return dir->getdents64(buf, cnt);
}

/* TODO: implement vfs_stat(stat*)
long _syscall_stat(interrupt_stack* data)
{
    SYSCALL_ARG1(const char* __user, pathname);
    SYSCALL_ARG2(struct stat* __user, buf);

    auto* dent = fs::vfs_open(*current_process->root,
        types::make_path(pathname, current_process->pwd));

    if (!dent)
        return -ENOENT;

    return fs::vfs_stat(dent, buf);
}
*/

/* TODO: implement vfs_stat(stat*)
long _syscall_fstat(interrupt_stack* data)
{
    SYSCALL_ARG1(int, fd);
    SYSCALL_ARG2(struct stat* __user, buf);

    auto* file = current_process->files[fd];
    if (!file)
        return -EBADF;

    return fs::vfs_stat(file, buf);
}
*/

long _syscall_gettimeofday(interrupt_stack* data)
{
    SYSCALL_ARG1(timeval* __user, tv);
    SYSCALL_ARG2(void* __user, tz);
    // TODO: return time of the day, not time from this boot

    if (unlikely(tz))
        return -EINVAL;

    if (likely(tv)) {
        // TODO: use copy_to_user
        auto ticks = kernel::hw::timer::current_ticks();
        tv->tv_sec = ticks / 100;
        tv->tv_usec = ticks * 10 * 1000;
    }

    return 0;
}

long _syscall_umask(interrupt_stack* data)
{
    SYSCALL_ARG1(mode_t, mask);

    mode_t old = current_process->umask;
    current_process->umask = mask;

    return old;
}

long _syscall_kill(interrupt_stack* data)
{
    SYSCALL_ARG1(pid_t, pid);
    SYSCALL_ARG2(int, sig);

    auto [ pproc, found ] = procs->try_find(pid);
    if (!found)
        return -ESRCH;

    if (!kernel::signal_list::check_valid(sig))
        return -EINVAL;

    if (pproc->is_system())
        return 0;

    // TODO: check permission
    procs->send_signal(pid, sig);

    return 0;
}

long _syscall_rt_sigprocmask(interrupt_stack* data)
{
    using kernel::sigmask_type;

    SYSCALL_ARG1(int, how);
    SYSCALL_ARG2(const sigmask_type* __user, set);
    SYSCALL_ARG3(sigmask_type* __user, oldset);
    SYSCALL_ARG4(size_t, sigsetsize);

    if (sigsetsize != sizeof(sigmask_type))
        return -EINVAL;

    sigmask_type sigs = current_thread->signals.get_mask();

    // TODO: use copy_to_user
    if (oldset)
        memcpy(oldset, &sigs, sizeof(sigmask_type));

    if (!set)
        return 0;

    // TODO: use copy_from_user
    switch (how) {
    case SIG_BLOCK:
        current_thread->signals.mask(*set);
        break;
    case SIG_UNBLOCK:
        current_thread->signals.unmask(*set);
        break;
    case SIG_SETMASK:
        current_thread->signals.set_mask(*set);
        break;
    }

    return 0;
}

long _syscall_rt_sigaction(interrupt_stack* data)
{
    using kernel::sigaction;
    using kernel::sigmask_type;
    SYSCALL_ARG1(int, signum);
    SYSCALL_ARG2(const sigaction* __user, act);
    SYSCALL_ARG3(sigaction* __user, oldact);
    SYSCALL_ARG4(size_t, sigsetsize);

    if (sigsetsize != sizeof(sigmask_type))
        return -EINVAL;

    if (!kernel::signal_list::check_valid(signum)
        || signum == SIGKILL || signum == SIGSTOP)
        return -EINVAL;

    // TODO: use copy_to_user
    if (oldact)
        current_thread->signals.get_handler(signum, *oldact);

    if (!act)
        return 0;

    // TODO: use copy_from_user
    current_thread->signals.set_handler(signum, *act);

    return 0;
}

long _syscall_newuname(interrupt_stack* data)
{
    SYSCALL_ARG1(new_utsname* __user, buf);

    if (!buf)
        return -EFAULT;

    // TODO: use copy_to_user
    memcpy(buf, kernel::sys_utsname, sizeof(new_utsname));

    return 0;
}

long _syscall_getpgid(interrupt_stack* data)
{
    SYSCALL_ARG1(pid_t, pid);

    if (pid == 0)
        return current_process->pgid;

    auto [ pproc, found ] = procs->try_find(pid);
    if (!found)
        return -ESRCH;

    return pproc->pgid;
}

long _syscall_gettid(interrupt_stack* data)
{
    // TODO: real tid
    (void)data;
    return current_process->pid;
}

long _syscall_mkdir(interrupt_stack* data)
{
    SYSCALL_ARG1(const char* __user, pathname);
    SYSCALL_ARG2(mode_t, mode);

    mode &= (~current_process->umask & 0777);

    auto path = current_process->pwd + pathname;

    auto* dent = fs::vfs_open(*current_process->root, path);
    if (dent)
        return -EEXIST;

    // get parent path
    auto dirname = path.last_name();
    path.remove_last();

    dent = fs::vfs_open(*current_process->root, path);
    if (!dent)
        return -ENOENT;

    if (!S_ISDIR(dent->ind->mode))
        return -ENOTDIR;

    auto ret = fs::vfs_mkdir(dent, dirname.c_str(), mode);

    if (ret != 0)
        return ret;

    return 0;
}

long _syscall_truncate(interrupt_stack* data)
{
    SYSCALL_ARG1(const char* __user, pathname);
    SYSCALL_ARG2(long, length);

    auto path = current_process->pwd + pathname;

    auto* dent = fs::vfs_open(*current_process->root, path);
    if (!dent)
        return -ENOENT;

    if (S_ISDIR(dent->ind->mode))
        return -EISDIR;

    auto ret = fs::vfs_truncate(dent->ind, length);

    if (ret != 0)
        return ret;

    return 0;
}

long _syscall_unlink(interrupt_stack* data)
{
    SYSCALL_ARG1(const char* __user, pathname);

    auto path = current_process->pwd + pathname;
    auto* dent = fs::vfs_open(*current_process->root, path, false);

    if (!dent)
        return -ENOENT;

    if (S_ISDIR(dent->ind->mode))
        return -EISDIR;

    return fs::vfs_rmfile(dent->parent, dent->name.c_str());
}

long _syscall_access(interrupt_stack* data)
{
    SYSCALL_ARG1(const char* __user, pathname);
    SYSCALL_ARG2(int, mode);

    auto path = current_process->pwd + pathname;
    auto* dent = fs::vfs_open(*current_process->root, path);

    if (!dent)
        return -ENOENT;

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

long _syscall_mknod(interrupt_stack* data)
{
    SYSCALL_ARG1(const char* __user, pathname);
    SYSCALL_ARG2(mode_t, mode);
    SYSCALL_ARG3(dev_t, dev);

    auto path = current_process->pwd + pathname;
    auto* dent = fs::vfs_open(*current_process->root, path);

    if (dent)
        return -EEXIST;

    auto filename = path.last_name();
    path.remove_last();

    dent = fs::vfs_open(*current_process->root, path);
    if (!dent)
        return -ENOENT;

    return fs::vfs_mknode(dent, filename.c_str(), mode, dev);
}

long _syscall_poll(interrupt_stack* data)
{
    SYSCALL_ARG1(struct pollfd* __user, fds);
    SYSCALL_ARG2(nfds_t, nfds);
    SYSCALL_ARG3(int, timeout);

    if (nfds == 0)
        return 0;

    if (nfds > 1) {
        NOT_IMPLEMENTED;
        return -EINVAL;
    }

    // TODO: handle timeout
    // if (timeout != -1) {
    // }

    // for now, we will poll from console only
    int ret = console->poll();
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

long _syscall_llseek(interrupt_stack* data)
{
    SYSCALL_ARG1(unsigned int, fd);
    SYSCALL_ARG2(unsigned long, offset_high);
    SYSCALL_ARG3(unsigned long, offset_low);
    SYSCALL_ARG4(loff_t*, result);
    SYSCALL_ARG5(unsigned int, whence);

    auto* file = current_process->files[fd];
    if (!file)
        return -EBADF;

    if (!result)
        return -EFAULT;

    if (offset_high) {
        NOT_IMPLEMENTED;
        return -EINVAL;
    }

    // TODO: support long long result
    auto ret = file->seek(offset_low, whence);

    if (ret < 0)
        return ret;

    *result = ret;
    return 0;
}

extern "C" void syscall_entry(
    interrupt_stack* data,
    mmx_registers* mmxregs)
{
    int syscall_no = SYSCALL_NO;

    if (syscall_no >= SYSCALL_HANDLERS_SIZE
        || !syscall_handlers[syscall_no]) {
        kmsgf("[kernel] syscall %d(%x) is not implemented", syscall_no, syscall_no);
        NOT_IMPLEMENTED;

        if (current_thread->signals.pending_signal())
            current_thread->signals.handle(data, mmxregs);
        return;
    }

    int ret = syscall_handlers[syscall_no](data);

    SYSCALL_RETVAL = ret;

    if (current_thread->signals.pending_signal())
        current_thread->signals.handle(data, mmxregs);
}

#define REGISTER_SYSCALL_HANDLER(no, name) if (1) { extern long _syscall_ ## name (interrupt_stack*); syscall_handlers[(no)] = _syscall_ ## name; }

SECTION(".text.kinit")
void init_syscall(void)
{
    REGISTER_SYSCALL_HANDLER(0x01, exit);
    REGISTER_SYSCALL_HANDLER(0x02, fork);
    REGISTER_SYSCALL_HANDLER(0x03, read);
    REGISTER_SYSCALL_HANDLER(0x04, write);
    REGISTER_SYSCALL_HANDLER(0x05, open);
    REGISTER_SYSCALL_HANDLER(0x06, close);
    REGISTER_SYSCALL_HANDLER(0x07, waitpid);
    REGISTER_SYSCALL_HANDLER(0x0a, unlink);
    REGISTER_SYSCALL_HANDLER(0x0b, execve);
    REGISTER_SYSCALL_HANDLER(0x0c, chdir);
    REGISTER_SYSCALL_HANDLER(0x0e, mknod);
    REGISTER_SYSCALL_HANDLER(0x14, getpid);
    REGISTER_SYSCALL_HANDLER(0x15, mount);
    REGISTER_SYSCALL_HANDLER(0x21, access);
    REGISTER_SYSCALL_HANDLER(0x25, kill);
    REGISTER_SYSCALL_HANDLER(0x27, mkdir);
    REGISTER_SYSCALL_HANDLER(0x29, dup);
    REGISTER_SYSCALL_HANDLER(0x2a, pipe);
    REGISTER_SYSCALL_HANDLER(0x2d, brk);
    REGISTER_SYSCALL_HANDLER(0x36, ioctl);
    REGISTER_SYSCALL_HANDLER(0x39, setpgid);
    REGISTER_SYSCALL_HANDLER(0x3c, umask);
    REGISTER_SYSCALL_HANDLER(0x3f, dup2);
    REGISTER_SYSCALL_HANDLER(0x40, getppid);
    REGISTER_SYSCALL_HANDLER(0x42, setsid);
    REGISTER_SYSCALL_HANDLER(0x4e, gettimeofday);
    REGISTER_SYSCALL_HANDLER(0x53, symlink);
    REGISTER_SYSCALL_HANDLER(0x55, readlink);
    REGISTER_SYSCALL_HANDLER(0x5b, munmap);
    REGISTER_SYSCALL_HANDLER(0x5c, truncate);
    REGISTER_SYSCALL_HANDLER(0x72, wait4);
    REGISTER_SYSCALL_HANDLER(0x7a, newuname);
    REGISTER_SYSCALL_HANDLER(0x84, getpgid);
    REGISTER_SYSCALL_HANDLER(0x8c, llseek);
    REGISTER_SYSCALL_HANDLER(0x8d, getdents);
    REGISTER_SYSCALL_HANDLER(0x91, readv);
    REGISTER_SYSCALL_HANDLER(0x92, writev);
    REGISTER_SYSCALL_HANDLER(0x93, getsid);
    REGISTER_SYSCALL_HANDLER(0xa8, poll);
    REGISTER_SYSCALL_HANDLER(0xac, prctl);
    REGISTER_SYSCALL_HANDLER(0xae, rt_sigaction);
    REGISTER_SYSCALL_HANDLER(0xaf, rt_sigprocmask);
    REGISTER_SYSCALL_HANDLER(0xb7, getcwd);
    REGISTER_SYSCALL_HANDLER(0xc0, mmap_pgoff);
    REGISTER_SYSCALL_HANDLER(0xc7, getuid);
    REGISTER_SYSCALL_HANDLER(0xc9, geteuid);
    REGISTER_SYSCALL_HANDLER(0xdc, getdents64);
    REGISTER_SYSCALL_HANDLER(0xdd, fcntl64);
    REGISTER_SYSCALL_HANDLER(0xe0, gettid);
    REGISTER_SYSCALL_HANDLER(0xef, sendfile64);
    REGISTER_SYSCALL_HANDLER(0xf3, set_thread_area);
    REGISTER_SYSCALL_HANDLER(0xfc, exit_group);
    REGISTER_SYSCALL_HANDLER(0x10, _set_tid_address);
    REGISTER_SYSCALL_HANDLER(0x17, _statx);
    REGISTER_SYSCALL_HANDLER(0x19, _clock_gettime64);
}
