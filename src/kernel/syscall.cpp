#include <asm/port_io.h>
#include <asm/sys.h>

#include <assert.h>
#include <errno.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <time.h>
#include <unistd.h>
#include <bits/alltypes.h>
#include <bits/ioctl.h>
#include <sys/types.h>
#include <sys/prctl.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <sys/utsname.h>

#include <kernel/user/thread_local.hpp>
#include <kernel/interrupt.h>
#include <kernel/log.hpp>
#include <kernel/mem.h>
#include <kernel/mm.hpp>
#include <kernel/process.hpp>
#include <kernel/signal.hpp>
#include <kernel/syscall.hpp>
#include <kernel/tty.hpp>
#include <kernel/utsname.hpp>
#include <kernel/vfs.hpp>
#include <kernel/hw/timer.h>

#include <types/allocator.hpp>
#include <types/elf.hpp>
#include <types/path.hpp>
#include <types/lock.hpp>
#include <types/status.h>
#include <types/string.hpp>
#include <types/types.h>

#define SYSCALL_NO ((data)->s_regs.eax)
#define SYSCALL_RETVAL ((data)->s_regs.eax)

#define SYSCALL_ARG1(type, name) type name = (type)((data)->s_regs.ebx)
#define SYSCALL_ARG2(type, name) type name = (type)((data)->s_regs.ecx)
#define SYSCALL_ARG3(type, name) type name = (type)((data)->s_regs.edx)
#define SYSCALL_ARG4(type, name) type name = (type)((data)->s_regs.esi)
#define SYSCALL_ARG5(type, name) type name = (type)((data)->s_regs.edi)
#define SYSCALL_ARG6(type, name) type name = (type)((data)->s_regs.ebp)

#define SYSCALL_HANDLERS_SIZE (404)
syscall_handler syscall_handlers[SYSCALL_HANDLERS_SIZE];

static void not_implemented()
{
    console->print("\n[kernel] this function is not implemented\n");
    current_thread->send_signal(SIGSYS);
}

extern "C" void _syscall_stub_fork_return(void);
int _syscall_fork(interrupt_stack* data)
{
    auto& newproc = procs->copy_from(*current_process);
    auto [ iter_newthd, inserted ] = newproc.thds.emplace(*current_thread, newproc.pid);
    assert(inserted);
    auto* newthd = &*iter_newthd;

    readythds->push(newthd);

    uint32_t newthd_oldesp = (uint32_t)newthd->kstack.esp;
    auto esp = &newthd->kstack.esp;

    // create fake interrupt stack
    push_stack(esp, data->ss);
    push_stack(esp, data->esp);
    push_stack(esp, data->eflags);
    push_stack(esp, data->cs);
    push_stack(esp, (uint32_t)data->v_eip);

    // eax
    push_stack(esp, 0);
    push_stack(esp, data->s_regs.ecx);
    // edx
    push_stack(esp, 0);
    push_stack(esp, data->s_regs.ebx);
    push_stack(esp, data->s_regs.esp);
    push_stack(esp, data->s_regs.ebp);
    push_stack(esp, data->s_regs.esi);
    push_stack(esp, data->s_regs.edi);

    // ctx_switch stack
    // return address
    push_stack(esp, (uint32_t)_syscall_stub_fork_return);
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

int _syscall_write(interrupt_stack* data)
{
    SYSCALL_ARG1(int, fd);
    SYSCALL_ARG2(const char* __user, buf);
    SYSCALL_ARG3(size_t, n);

    auto* file = current_process->files[fd];
    if (!file)
        return -EBADF;

    return file->write(buf, n);
}

int _syscall_read(interrupt_stack* data)
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
int _syscall_sleep(interrupt_stack*)
{
    current_thread->sleep();

    schedule();
    return 0;
}

int _syscall_chdir(interrupt_stack* data)
{
    SYSCALL_ARG1(const char*, path);

    auto* dir = fs::vfs_open(*current_process->root,
        types::make_path(path, current_process->pwd));
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
int _syscall_execve(interrupt_stack* data)
{
    SYSCALL_ARG1(const char*, exec);
    SYSCALL_ARG2(char* const*, argv);
    SYSCALL_ARG3(char* const*, envp);

    types::elf::elf32_load_data d;
    d.argv = argv;
    d.envp = envp;
    d.system = false;

    d.exec_dent = fs::vfs_open(*current_process->root,
        types::make_path(exec, current_process->pwd));
    
    if (!d.exec_dent)
        return -ENOENT;

    current_process->files.onexec();

    int ret = types::elf::elf32_load(&d);
    if (ret != GB_OK)
        return -d.errcode;

    data->v_eip = d.eip;
    data->esp = (uint32_t)d.sp;

    current_thread->signals.on_exec();

    return 0;
}

// @param exit_code
int NORETURN _syscall_exit(interrupt_stack* data)
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

// @param pid: pid of the process to wait
// @param status: the exit code of the exited process
// @param options: options for waitpid
// @return pid of the exited process
int _syscall_waitpid(interrupt_stack* data)
{
    SYSCALL_ARG1(pid_t, pid_to_wait);
    SYSCALL_ARG2(int*, arg1);
    SYSCALL_ARG3(int, options);

    if (pid_to_wait != -1)
        return -EINVAL;

    auto& cv = current_process->cv_wait;
    auto& mtx = cv.mtx();
    types::lock_guard lck(mtx);

    auto& waitlist = current_process->waitlist;

    // TODO: check if it is waiting for stopped process
    (void)options;

    while (waitlist.empty()) {
        if (current_process->children.empty())
            return -ECHILD;

        cv.wait(mtx);

        // TODO: check WNOHANG
        // if (!cv.wait(mtx))
        //     return -EINTR;
    }

    auto iter = waitlist.begin();
    assert(iter != waitlist.end());

    auto& obj = *iter;
    pid_t pid = obj.pid;

    // TODO: copy_to_user check privilege
    *arg1 = obj.code;

    procs->remove(pid);
    waitlist.erase(iter);

    return pid;
}

int _syscall_wait4(interrupt_stack* data)
{
    SYSCALL_ARG4(void* __user, rusage);

    // TODO: getrusage
    if (rusage)
        return -EINVAL;

    return _syscall_waitpid(data);
}

int _syscall_getdents(interrupt_stack* data)
{
    SYSCALL_ARG1(int, fd);
    SYSCALL_ARG2(char* __user, buf);
    SYSCALL_ARG3(size_t, cnt);

    auto* dir = current_process->files[fd];
    if (!dir)
        return -EBADF;

    return dir->getdents(buf, cnt);
}

int _syscall_open(interrupt_stack* data)
{
    SYSCALL_ARG1(const char* __user, path);
    SYSCALL_ARG2(int, flags);
    SYSCALL_ARG3(mode_t, mode);

    mode &= ~current_process->umask;

    return current_process->files.open(*current_process,
        types::make_path(path, current_process->pwd), flags, mode);
}

int _syscall_getcwd(interrupt_stack* data)
{
    SYSCALL_ARG1(char*, buf);
    SYSCALL_ARG2(size_t, bufsize);

    // TODO: use copy_to_user
    auto path = current_process->pwd.full_path();
    strncpy(buf, path.c_str(), bufsize);
    buf[bufsize - 1] = 0;

    return (uint32_t)buf;
}

int _syscall_setsid(interrupt_stack*)
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

int _syscall_getsid(interrupt_stack* data)
{
    SYSCALL_ARG1(pid_t, pid);

    auto [ pproc, found ] = procs->try_find(pid);
    if (!found)
        return -ESRCH;
    if (pproc->sid != current_process->sid)
        return -EPERM;

    return pproc->sid;
}

int _syscall_close(interrupt_stack* data)
{
    SYSCALL_ARG1(int, fd);
    current_process->files.close(fd);
    return 0;
}

int _syscall_dup(interrupt_stack* data)
{
    SYSCALL_ARG1(int, old_fd);
    return current_process->files.dup(old_fd);
}

int _syscall_dup2(interrupt_stack* data)
{
    SYSCALL_ARG1(int, old_fd);
    SYSCALL_ARG2(int, new_fd);
    return current_process->files.dup2(old_fd, new_fd);
}

int _syscall_pipe(interrupt_stack* data)
{
    SYSCALL_ARG1(int* __user, pipefd);
    return current_process->files.pipe(pipefd);
}

int _syscall_setpgid(interrupt_stack* data)
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

int _syscall_ioctl(interrupt_stack* data)
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
        // TODO: copy_to_user
        *pgid = ctrl_tty->get_pgrp();
        break;
    }
    case TIOCSPGRP: {
        // TODO: copy_from_user
        SYSCALL_ARG3(const pid_t*, pgid);
        tty* ctrl_tty = current_process->control_tty;
        ctrl_tty->set_pgrp(*pgid);
        break;
    }
    case TIOCGWINSZ: {
        SYSCALL_ARG3(winsize*, ws);
        ws->ws_col = 80;
        ws->ws_row = 10;
        break;
    }
    default:
        not_implemented();
        return -EINVAL;
    }

    return 0;
}

int _syscall_getpid(interrupt_stack*)
{
    return current_process->pid;
}

int _syscall_getppid(interrupt_stack*)
{
    return current_process->ppid;
}

int _syscall_set_thread_area(interrupt_stack* data)
{
    SYSCALL_ARG1(kernel::user::user_desc* __user, ptr);

    auto ret = current_thread->set_thread_area(ptr);
    if (ret != 0)
        return ret;

    current_thread->load_thread_area();
    return 0;
}

int _syscall_set_tid_address(interrupt_stack* data)
{
    SYSCALL_ARG1(int* __user, tidptr);
    current_thread->set_child_tid = tidptr;
    return current_thread->tid();
}

// TODO: this operation SHOULD be atomic
int _syscall_writev(interrupt_stack* data)
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

int _syscall_prctl(interrupt_stack* data)
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

int _syscall_clock_gettime64(interrupt_stack* data)
{
    SYSCALL_ARG1(clockid_t, clk_id);
    SYSCALL_ARG2(timespec* __user, tp);

    // TODO: check privilege of tp
    if (clk_id != CLOCK_REALTIME || !tp)
        return -EINVAL;

    tp->tv_sec = 10 + current_ticks();
    tp->tv_nsec = 0;

    return 0;
}

int _syscall_getuid(interrupt_stack*)
{
    return 0; // all user are root for now
}

int _syscall_geteuid(interrupt_stack*)
{
    return 0; // all user are root for now
}

int _syscall_brk(interrupt_stack* data)
{
    SYSCALL_ARG1(void*, addr);

    return (int)current_process->mms.set_brk(addr);
}

int _syscall_mmap_pgoff(interrupt_stack* data)
{
    SYSCALL_ARG1(void*, addr);
    SYSCALL_ARG2(size_t, len);
    SYSCALL_ARG3(int, prot);
    SYSCALL_ARG4(int, flags);
    SYSCALL_ARG5(int, fd);
    SYSCALL_ARG6(off_t, pgoffset);

    if ((ptr_t)addr % PAGE_SIZE != 0)
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
            if (ret != GB_OK)
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
            mms.add_empty_area(addr, len / PAGE_SIZE,
                PAGE_COW, prot & PROT_WRITE, false);
        }
    }

    return (int)addr;
}

int _syscall_munmap(interrupt_stack* data)
{
    SYSCALL_ARG1(void*, addr);
    SYSCALL_ARG2(size_t, len);

    if ((ptr_t)addr % PAGE_SIZE != 0)
        return -EINVAL;

    return current_process->mms.unmap(addr, len, false);
}

int _syscall_sendfile64(interrupt_stack* data)
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
        not_implemented();
        return -EINVAL;
    }

    constexpr size_t bufsize = 512;
    std::vector<char> buf(bufsize);
    size_t totn = 0;
    while (totn < count) {
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
        if (current_thread->signals.pending_signal() != 0)
            return (totn == 0) ? -EINTR : totn;
    }

    return totn;
}

int _syscall_statx(interrupt_stack* data)
{
    SYSCALL_ARG1(int, dirfd);
    SYSCALL_ARG2(const char* __user, path);
    SYSCALL_ARG3(int, flags);
    SYSCALL_ARG4(unsigned int, mask);
    SYSCALL_ARG5(statx* __user, statxbuf);

    // AT_STATX_SYNC_AS_STAT is the default value
    if (flags != AT_STATX_SYNC_AS_STAT && !(flags & AT_SYMLINK_NOFOLLOW)) {
        not_implemented();
        return -EINVAL;
    }

    if (dirfd != AT_FDCWD) {
        not_implemented();
        return -EINVAL;
    }

    auto* dent = fs::vfs_open(*current_process->root,
        types::make_path(path, current_process->pwd));

    if (!dent)
        return -ENOENT;

    // TODO: copy to user
    auto ret = fs::vfs_stat(dent, statxbuf, mask);

    return ret;
}

int _syscall_fcntl64(interrupt_stack* data)
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
        not_implemented();
        return -EINVAL;
    }
}

int _syscall_getdents64(interrupt_stack* data)
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
int _syscall_stat(interrupt_stack* data)
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
int _syscall_fstat(interrupt_stack* data)
{
    SYSCALL_ARG1(int, fd);
    SYSCALL_ARG2(struct stat* __user, buf);

    auto* file = current_process->files[fd];
    if (!file)
        return -EBADF;

    return fs::vfs_stat(file, buf);
}
*/

int _syscall_gettimeofday(interrupt_stack* data)
{
    SYSCALL_ARG1(timeval* __user, tv);
    SYSCALL_ARG2(void* __user, tz);
    // TODO: return time of the day, not time from this boot

    if (unlikely(tz))
        return -EINVAL;

    if (likely(tv)) {
        // TODO: use copy_to_user
        tv->tv_sec = current_ticks() / 100;
        tv->tv_usec = current_ticks() * 10 * 1000;
    }

    return 0;
}

int _syscall_umask(interrupt_stack* data)
{
    SYSCALL_ARG1(mode_t, mask);

    mode_t old = current_process->umask;
    current_process->umask = mask;

    return old;
}

int _syscall_kill(interrupt_stack* data)
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

int _syscall_rt_sigprocmask(interrupt_stack* data)
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

int _syscall_rt_sigaction(interrupt_stack* data)
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

int _syscall_newuname(interrupt_stack* data)
{
    SYSCALL_ARG1(new_utsname* __user, buf);

    if (!buf)
        return -EFAULT;

    // TODO: use copy_to_user
    memcpy(buf, kernel::sys_utsname, sizeof(new_utsname));

    return 0;
}

pid_t _syscall_getpgid(interrupt_stack* data)
{
    SYSCALL_ARG1(pid_t, pid);

    if (pid == 0)
        return current_process->pgid;

    auto [ pproc, found ] = procs->try_find(pid);
    if (!found)
        return -ESRCH;

    return pproc->pgid;
}

int _syscall_gettid(interrupt_stack* data)
{
    // TODO: real tid
    (void)data;
    return current_process->pid;
}

int _syscall_mkdir(interrupt_stack* data)
{
    SYSCALL_ARG1(const char* __user, pathname);
    SYSCALL_ARG2(mode_t, mode);

    mode &= (~current_process->umask & 0777);

    auto path = types::make_path(pathname, current_process->pwd);

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

    if (ret != GB_OK)
        return ret;

    return 0;
}

int _syscall_truncate(interrupt_stack* data)
{
    SYSCALL_ARG1(const char* __user, pathname);
    SYSCALL_ARG2(long, length);

    auto path = types::make_path(pathname, current_process->pwd);

    auto* dent = fs::vfs_open(*current_process->root, path);
    if (!dent)
        return -ENOENT;

    if (S_ISDIR(dent->ind->mode))
        return -EISDIR;

    auto ret = fs::vfs_truncate(dent->ind, length);

    if (ret != GB_OK)
        return ret;

    return 0;
}

int _syscall_unlink(interrupt_stack* data)
{
    SYSCALL_ARG1(const char* __user, pathname);

    auto path = types::make_path(pathname, current_process->pwd);
    auto* dent = fs::vfs_open(*current_process->root, path);

    if (!dent)
        return -ENOENT;

    if (S_ISDIR(dent->ind->mode))
        return -EISDIR;

    return fs::vfs_rmfile(dent->parent, dent->name.c_str());
}

int _syscall_access(interrupt_stack* data)
{
    SYSCALL_ARG1(const char* __user, pathname);
    SYSCALL_ARG2(int, mode);

    auto path = types::make_path(pathname, current_process->pwd);
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

extern "C" void syscall_entry(
    interrupt_stack* data,
    mmx_registers* mmxregs)
{
    int syscall_no = SYSCALL_NO;

    if (syscall_no >= SYSCALL_HANDLERS_SIZE
        || !syscall_handlers[syscall_no]) {
        char buf[64];
        snprintf(buf, 64,
            "[kernel] syscall %x not implemented\n", syscall_no);
        console->print(buf);
        not_implemented();
        return;
    }

    int ret = syscall_handlers[syscall_no](data);

    SYSCALL_RETVAL = ret;

    if (current_thread->signals.pending_signal())
        current_thread->signals.handle(data, mmxregs);
}

SECTION(".text.kinit")
void init_syscall(void)
{
    memset(syscall_handlers, 0x00, sizeof(syscall_handlers));

    syscall_handlers[0x01] = _syscall_exit;
    syscall_handlers[0x02] = _syscall_fork;
    syscall_handlers[0x03] = _syscall_read;
    syscall_handlers[0x04] = _syscall_write;
    syscall_handlers[0x05] = _syscall_open;
    syscall_handlers[0x06] = _syscall_close;
    syscall_handlers[0x07] = _syscall_waitpid;
    syscall_handlers[0x0a] = _syscall_unlink;
    syscall_handlers[0x0b] = _syscall_execve;
    syscall_handlers[0x0c] = _syscall_chdir;
    syscall_handlers[0x14] = _syscall_getpid;
    syscall_handlers[0x21] = _syscall_access;
    syscall_handlers[0x25] = _syscall_kill;
    syscall_handlers[0x27] = _syscall_mkdir;
    syscall_handlers[0x29] = _syscall_dup;
    syscall_handlers[0x2a] = _syscall_pipe;
    syscall_handlers[0x2d] = _syscall_brk;
    syscall_handlers[0x36] = _syscall_ioctl;
    syscall_handlers[0x39] = _syscall_setpgid;
    syscall_handlers[0x3c] = _syscall_umask;
    syscall_handlers[0x3f] = _syscall_dup2;
    syscall_handlers[0x40] = _syscall_getppid;
    syscall_handlers[0x42] = _syscall_setsid;
    syscall_handlers[0x4e] = _syscall_gettimeofday;
    syscall_handlers[0x5b] = _syscall_munmap;
    syscall_handlers[0x5c] = _syscall_truncate;
    syscall_handlers[0x72] = _syscall_wait4;
    syscall_handlers[0x7a] = _syscall_newuname;
    syscall_handlers[0x84] = _syscall_getpgid;
    syscall_handlers[0x8d] = _syscall_getdents;
    syscall_handlers[0x92] = _syscall_writev;
    syscall_handlers[0x93] = _syscall_getsid;
    syscall_handlers[0xac] = _syscall_prctl;
    syscall_handlers[0xae] = _syscall_rt_sigaction;
    syscall_handlers[0xaf] = _syscall_rt_sigprocmask;
    syscall_handlers[0xb7] = _syscall_getcwd;
    syscall_handlers[0xc0] = _syscall_mmap_pgoff;
    syscall_handlers[0xc7] = _syscall_getuid;
    syscall_handlers[0xc9] = _syscall_geteuid;
    syscall_handlers[0xdc] = _syscall_getdents64;
    syscall_handlers[0xdd] = _syscall_fcntl64;
    syscall_handlers[0xe0] = _syscall_gettid;
    syscall_handlers[0xef] = _syscall_sendfile64;
    syscall_handlers[0xf3] = _syscall_set_thread_area;
    syscall_handlers[0xfc] = _syscall_exit; // we implement exit_group as exit for now
    syscall_handlers[0x102] = _syscall_set_tid_address;
    syscall_handlers[0x17f] = _syscall_statx;
    syscall_handlers[0x193] = _syscall_clock_gettime64;
    // syscall_handlers[35] = _syscall_sleep;
}
